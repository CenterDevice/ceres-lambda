use crate::bosun::{self, Bosun, Datum, Tags};
use crate::config::FunctionConfig;
use crate::error::WatchAutoscalingError;
use failure::{Error, Fail};
use lambda_runtime::Context;
use log::debug;
use serde_derive::Deserialize;
use serde_json::{self, Value};

pub mod asg;
pub mod ping;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Event {
    ASG(asg::AutoScalingEvent),
    Ping(ping::Ping),
}

pub fn handle<T: Bosun>(json: Value, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<(), Error> {
    let tags = Tags::new();
    let datum = Datum::now(bosun::METRIC_LAMBDA_INVOCATION_COUNT, "1", &tags);
    bosun.emit_datum(&datum)?;

    let event = parse_event(json)?;
    let res = handle_event(event, ctx, &config, bosun);

    match res {
        Ok(_) => {
            let datum = Datum::now(bosun::METRIC_LAMBDA_INVOCATION_RESULT, "0", &tags);
            bosun.emit_datum(&datum)?
        }
        Err(_) => {
            let datum = Datum::now(bosun::METRIC_LAMBDA_INVOCATION_RESULT, "1", &tags);
            bosun.emit_datum(&datum)?
        }
    }

    res
}

fn parse_event(json: Value) -> Result<Event, Error> {
    let event: Event = serde_json::from_value(json)
        .map_err(|e| e.context(WatchAutoscalingError::FailedParseEvent))?;
    debug!("Parsed event = {:?}.", event);

    Ok(event)
}

fn handle_event<T: Bosun>(event: Event, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<(), Error> {
    match event {
        Event::Ping(ping) => ping::handle(ping, &ctx, &config, bosun),
        Event::ASG(asg) => asg::handle(asg, &ctx, &config, bosun),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::asg::AutoScalingEvent;
    use crate::asg_mapping::{Mapping, Mappings};
    use crate::bosun::testing::{BosunMockClient, BosunCallStats};
    use crate::testing::setup;

    use chrono::offset::Utc;
    use env_logger;
    use serde_json::{json, Value};
    use spectral::prelude::*;
    use std::collections::HashMap;

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

        let res = handle(event, &ctx, &config, &bosun);
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

        let res = handle(event, &ctx, &config, &bosun);
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
