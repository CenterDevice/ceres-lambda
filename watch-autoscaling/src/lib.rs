use crate::bosun::{BosunClient, Bosun, Datum, Metadata, Tags};
use crate::config::{EncryptedFunctionConfig, EnvConfig, FunctionConfig};
use crate::asg_mapping::Mapping;
use aws_lambda_events::event::autoscaling::AutoScalingEvent;
use clams::config::Config;
use failure::{Error, Fail};
use lambda_runtime::{error::HandlerError, Context};
use log::{debug, info};
use serde_derive::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

mod asg_mapping;
pub mod config;
mod bosun;
mod kms;

static FAILED_LAMBDA_RESULT: &str= r#"{ "exit_code": 1, "error_msg": "Failed to serialize json for LambdaResult" }"#;

static METRIC_ASG_UP_DOWN: &str = "aws.ec2.asg.scaling.event";
static METRIC_LAMBDA_INVOCATION_COUNT: &str = "aws.lambda.function.invocation.count";
static METRIC_LAMBDA_INVOCATION_RESULT: &str = "aws.lambda.function.invocation.result";

// Use a counter, in case we want to track how often the function gets called before getting cold
// again.
static INVOCATION_COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug, Fail)]
enum WatchAutoscalingError {
    #[fail(display = "failed to read environment variable '{}'", _0)]
    FailedEnvVar(&'static str),
    #[fail(display = "autoScalingEvent for Successful Termination did not contain EC2InstanceId")]
    NoInstanceId,
    #[fail(display = "autoScalingEvent for Successful Termination did not contain AutoScalingGroupName")]
    NoAutoScalingGroupName,
    #[fail(display = "autoScalingEvent missing detail_type information")]
    NoDetailType,
    #[fail(display = "failed to parse AutoScalingEvent event")]
    FailedParseAsgEvent,
    #[fail(display = "failed to parse event")]
    FailedParseEvent,
    #[fail(display = "failed to load config file because {}", _0)]
    FailedConfig(String),
    #[fail(display = "did not find mapping from asg name '{}' to host prefix", _0)]
    NoHostMappingFound(String)
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Event {
    ASG(AutoScalingEvent),
    Ping(Ping),
}

#[derive(Debug, Deserialize)]
struct Ping {
    ping: String,
}

pub fn lambda_handler(input: Value, ctx: Context) -> Result<(), HandlerError> {
    let res = handler(input, &ctx);
    match res {
        Ok(_) => eprintln!("Successfully executed, request_id={}.", ctx.aws_request_id),
        Err(ref e) => eprintln!("Failed to execute, request_id={} because {}.", ctx.aws_request_id, e),
    };
    let lambda_result = match res {
        Ok(ref details) => {
            LambdaResult::from_ctx(&ctx, None, Some(details))
        }
        Err(ref e) => {
            LambdaResult::from_ctx(&ctx, Some(e.to_string()), None)
        }
    };
    log_lambda_result_safe(&lambda_result);

    res
        .map_err(|e| ctx.new_error(e.to_string().as_str()))
}

fn log_lambda_result_safe<T: serde::Serialize>(lambda_result: &LambdaResult<T>) {
    let json = serde_json::to_string(lambda_result)
        .unwrap_or_else(|_| FAILED_LAMBDA_RESULT.to_string());

    println!("lambda result = {}", json);
}

#[derive(Debug, Serialize)]
pub struct LambdaResult<'a, T> {
    function_name: &'a str,
    function_version: &'a str,
    aws_request_id: &'a str,
    exit_code: usize,
    error_msg: Option<String>,
    details: Option<&'a T>,
    git_commit_sha: &'a str,
    git_commit_date: &'a str,
    version: &'a str,
    build_timestamp: &'a str,
}

impl<'a, T> LambdaResult<'a, T> {
    pub fn from_ctx(ctx: &'a Context, error_msg: Option<String>, details: Option<&'a T>) -> LambdaResult<'a, T> {
        LambdaResult {
            function_name: &ctx.function_name,
            function_version: &ctx.function_version,
            aws_request_id: &ctx.aws_request_id,
            exit_code: if error_msg.is_none() {0} else {1},
            error_msg,
            details,
            git_commit_sha: env!("VERGEN_SHA_SHORT"),
            git_commit_date:  env!("VERGEN_COMMIT_DATE"),
            version: env!("CARGO_PKG_VERSION"),
            build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),
        }
    }
}

fn handler(input: Value, ctx: &Context) -> Result<(), Error> {
    let invocation_counter = INVOCATION_COUNTER.fetch_add(1, Ordering::SeqCst);
    i_have_been_invoked(invocation_counter, ctx);

    let env_config = EnvConfig::from_env()?;
    let encrypted_config = EncryptedFunctionConfig::from_file(&env_config.config_file)
        // This map_err seems necessary since error_chain::Error is not Send + 'static
        .map_err(|e| WatchAutoscalingError::FailedConfig(e.to_string()))?;
    let config = encrypted_config.decrypt()?;
    if invocation_counter == 0 {
        let res = init(&config);
        if let Err(ref e) = res {
            eprintln!("Failed to initialize function because {}.", e);
        };
        res?
    }
    debug!("Loaded environment variables configuration = {:?}.", &env_config);
    debug!("Loaded encrypted configuration from file {:?}.", &env_config.config_file);
    debug!("Decrypted encrypted configuration.");
    info!("Invocation initialization complete.");

    let bosun = BosunClient::with_tags(config.bosun.uri().as_str(), 3, config.bosun.tags.clone());
    debug!("Created Bosun client.");

    do_handler(input, ctx, &config, &bosun)
}

fn i_have_been_invoked(invocation_counter: usize, ctx: &Context) {
    eprintln!(
        "I've been invoked ({}); function={}, lambda version={}, request_id={}, git commit sha={}, git commit date={}, version={}, build timestamp={}.",
        invocation_counter, ctx.function_name, ctx.function_version, ctx.aws_request_id, env!("VERGEN_SHA_SHORT"), env!("VERGEN_COMMIT_DATE"), env!("CARGO_PKG_VERSION"), env!("VERGEN_BUILD_TIMESTAMP")
    );
}

/// The purpose of this function is to run once per instance of this function
fn init(config: &FunctionConfig) -> Result<(), Error> {
    env_logger::init();
    debug!("Initialized logger.");

    let bosun = BosunClient::new(config.bosun.uri().as_str(), 3);

    let metadata = Metadata::new(
        METRIC_ASG_UP_DOWN,
        "rate",
        "Scaling",
        "ASG up and down scaling event [-1 = down scaling, +1 = up scaling]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        METRIC_LAMBDA_INVOCATION_COUNT,
        "rate",
        "Invocations",
        "AWS Lambda function invocation counter",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        METRIC_LAMBDA_INVOCATION_COUNT,
        "gauge",
        "Result",
        "AWS Lambda function invocation result code [0 = success, >0 = failure]",
    );
    bosun.emit_metadata(&metadata)?;

    info!("Initialization complete.");
    Ok(())
}

fn do_handler<T: Bosun>(input: Value, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<(), Error> {
    let tags = bosun_tags(ctx);
    let datum = Datum::now(METRIC_LAMBDA_INVOCATION_COUNT, "1", &tags);
    bosun.emit_datum(&datum)?;

    let event: Event = serde_json::from_value(input)
        .map_err(|e| e.context(WatchAutoscalingError::FailedParseEvent))?;
    debug!("Parsed event = {:?}.", event);

    let res = match event {
        Event::Ping(ping) => handle_ping(ping, &ctx, &config, bosun),
        Event::ASG(asg) => handle_asg(asg, &ctx, &config, bosun),
    };

    match res {
        Ok(_) => {
            let datum = Datum::now(METRIC_LAMBDA_INVOCATION_RESULT, "0", &tags);
            bosun.emit_datum(&datum)?
        }
        Err(_) => {
            let datum = Datum::now(METRIC_LAMBDA_INVOCATION_RESULT, "1", &tags);
            bosun.emit_datum(&datum)?
        }
    }

    res
}

fn handle_ping<T: Bosun>(ping: Ping, _: &Context, _: &FunctionConfig, _: &T) -> Result<(), Error> {
    info!("Received {:?}.", ping);

    Ok(())
}

#[derive(Debug)]
enum AsgLifeCycleEvent<'a> {
    SuccessfulLaunch(LifeCycleDetails<'a>),
    UnsuccessfulLaunch(LifeCycleDetails<'a>),
    SuccessfulTermination(TerminationDetails<'a>),
    UnsuccessfulTermination(LifeCycleDetails<'a>),
}

#[derive(PartialEq, Eq, Debug)]
struct LifeCycleDetails<'a> {
    auto_scaling_group_name: &'a str,
}

#[derive(PartialEq, Eq, Debug)]
struct TerminationDetails<'a> {
    instance_id: &'a str,
    auto_scaling_group_name: &'a str,
}

impl<'a> AsgLifeCycleEvent<'a> {
    pub fn try_from(asg: &'a AutoScalingEvent) -> Result<AsgLifeCycleEvent<'a>, Error> {
        let detail_type = asg.detail_type.as_ref()
            .ok_or_else(|| Error::from(WatchAutoscalingError::NoDetailType))?;

        match detail_type.as_str() {
            "EC2 Instance Launch Successful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::SuccessfulLaunch(details))
            }
            "EC2 Instance Launch Unsuccessful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::UnsuccessfulLaunch(details))
            }
            "EC2 Instance Terminate Successful" =>
                AsgLifeCycleEvent::successful_termination_from(asg),
            "EC2 Instance Terminate Unsuccessful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::UnsuccessfulTermination(details))
            }
            _ => Err(Error::from(WatchAutoscalingError::FailedParseAsgEvent))
        }
    }

    fn lifecycle_details_from(asg: &'a AutoScalingEvent) -> Result<LifeCycleDetails<'a>, Error> {
        let auto_scaling_group_name = asg.detail
            .get("AutoScalingGroupName")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::from(WatchAutoscalingError::NoAutoScalingGroupName))?;

        let details = LifeCycleDetails {
            auto_scaling_group_name,
        };
        Ok(details)
    }

    fn successful_termination_from(asg: &'a AutoScalingEvent) -> Result<AsgLifeCycleEvent<'a>, Error> {
        let instance_id = asg.detail
            .get("EC2InstanceId")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::from(WatchAutoscalingError::NoInstanceId))?;
        let auto_scaling_group_name = asg.detail
            .get("AutoScalingGroupName")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::from(WatchAutoscalingError::NoAutoScalingGroupName))?;

        let details = TerminationDetails {
            instance_id,
            auto_scaling_group_name,
        };
        Ok(AsgLifeCycleEvent::SuccessfulTermination(details))
    }
}

fn handle_asg<T: Bosun>(asg: AutoScalingEvent, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<(), Error> {
    debug!("Received AutoScalingEvent {:?}.", asg);
    let event = AsgLifeCycleEvent::try_from(&asg)?;
    info!("Received AsgLifeCycleEvent {:?}.", event);

    let (asg_name, value) = match event {
        AsgLifeCycleEvent::SuccessfulLaunch(ref x) => (x.auto_scaling_group_name, 1),
        AsgLifeCycleEvent::UnsuccessfulLaunch(ref x) => (x.auto_scaling_group_name, 0),
        AsgLifeCycleEvent::SuccessfulTermination(ref x) => (x.auto_scaling_group_name, -1),
        AsgLifeCycleEvent::UnsuccessfulTermination(ref x) => (x.auto_scaling_group_name, 0),
    };

    let mapping = config.asg_mappings.map(asg_name);
    info!("Mapped ASG to '{:?}'.", mapping);

    let mut tags = bosun_tags(ctx);
    tags.insert("asg".to_string(), mapping
        .map(|x| x.tag_name.to_string())
        .unwrap_or_else(|| "unmapped".to_string())
    );
    let value = value.to_string();
    let datum = Datum::now(METRIC_ASG_UP_DOWN, &value, &tags);
    bosun.emit_datum(&datum)?;

    if let AsgLifeCycleEvent::SuccessfulTermination(ref details) = event {
        set_bosun_silence(details, mapping, bosun)?
    };

    Ok(())
}

fn set_bosun_silence(details: &TerminationDetails, mapping: Option<&Mapping>, bosun: &Bosun) -> Result<(), Error> {
    let host_prefix = mapping
        .map(|x| &x.host_prefix)
        .ok_or_else(|| Error::from(WatchAutoscalingError::NoHostMappingFound(details.auto_scaling_group_name.to_string())))?;

    let host = format!("{}{}*", &host_prefix, details.instance_id);
    info!("Setting silence for host '{}'.", host);

    // TODO: Make 24h a Duration and configurable
    let silence = bosun::Silence::host(&host, "24h");
    bosun.set_silence(&silence)?;

    Ok(())
}

fn bosun_tags(ctx: &Context) -> Tags {
    let mut tags: Tags = Tags::new();
    tags.insert("host".to_string(), "lambda".to_string());
    tags.insert("function_name".to_string(), ctx.function_name.to_string());

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::asg_mapping::Mappings;
    use crate::bosun::testing::{BosunMockClient, BosunCallStats};

    use chrono::offset::Utc;
    use env_logger;
    use serde_json::{json, Value};
    use spectral::prelude::*;
    use std::collections::HashMap;
    use std::sync::{Once, ONCE_INIT};

    static INIT: Once = ONCE_INIT;

    /// Setup function that is only run once, even if called multiple times.
    fn setup() {
        INIT.call_once(|| {
            env_logger::init();
        });
    }

    #[test]
    fn test_i_have_been_invoked() {
        let ctx = Context::default();
        let invocation_counter = 0;
        i_have_been_invoked(invocation_counter, &ctx);
    }

    #[test]
    fn parse_asg_lifecycle_event_from_asg_success_full_termination() {
        setup();

        let asg = asg_success_full_termination_event();
        let instance_id = "i-1234567890abcdef0".to_string();
        let auto_scaling_group_name = "my-auto-scaling-group".to_string();
        let expected_details = TerminationDetails {
            instance_id: instance_id.as_str(),
            auto_scaling_group_name: auto_scaling_group_name.as_str(),
        };

        let asg_event = AsgLifeCycleEvent::try_from(&asg);

        asserting("failed to parse asg event").that(&asg_event).is_ok();
        match asg_event.unwrap() {
            AsgLifeCycleEvent::SuccessfulTermination(ref details) => assert_that(&details).named("termination details").is_equal_to(&expected_details),
            _ => panic!("wrong event"),
        };
    }

    #[test]
    fn parse_event_ping() {
        setup();

        let event = json!(
            { "ping": "echo request" }
        );

        let parsed = serde_json::from_value(event);

        info!("parsed = {:?}", parsed);

        match parsed {
            Ok(Event::Ping(_)) => {}
            _ => assert!(false),
        }
    }

    #[test]
    fn test_handle_ping() {
        setup();

        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let config = FunctionConfig::default();
        let event = json!(
            { "ping": "echo request" }
        );
        let expected = BosunCallStats::new(0, 2, 0);

        let res = do_handler(event, &ctx, &config, &bosun);
        assert_that!(&res).is_ok();

        let bosun_stats = bosun.to_stats();
        asserting("bosun calls").that(&bosun_stats).named("actual calls").is_equal_to(&expected);
    }

    #[test]
    fn test_handle_asg_successful_termination() {
        setup();

        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let mut config = FunctionConfig::default();
        config.asg_mappings = Mappings {
            items: vec![
                Mapping { search: "my".to_string(), tag_name: "my".to_string(), host_prefix: "my-server-".to_string() },
            ],
        };
        let asg_event = asg_success_full_termination_event();
        let event = serde_json::to_value(asg_event).unwrap();
        let expected = BosunCallStats::new(0, 3, 1);

        let res = do_handler(event, &ctx, &config, &bosun);
        assert_that!(&res).is_ok();

        let bosun_stats = bosun.to_stats();
        asserting("bosun calls").that(&bosun_stats).named("actual calls").is_equal_to(&expected);
    }

    fn asg_success_full_termination_event() -> AutoScalingEvent {
        let mut detail = HashMap::new();
        detail.insert("EC2InstanceId".to_string(), Value::String("i-1234567890abcdef0".to_string()));
        detail.insert("AutoScalingGroupName".to_string(), Value::String("my-auto-scaling-group".to_string()));
        let asg = AutoScalingEvent {
            version: Some("0".to_string()),
            id: Some("12345678-1234-1234-1234-123456789012".to_string()),
            detail_type: Some("EC2 Instance Terminate Successful".to_string()),
            source: Some("aws.autoscaling".to_string()),
            account_id: Some("123456789012".to_string()),
            time: Utc::now(),
            region: Some("us-west-2".to_string()),
            resources: vec!["auto-scaling-group-arn".to_string(), "instance-arn".to_string()],
            detail,
        };
        asg
    }
}
