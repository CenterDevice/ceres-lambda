use crate::config::FunctionConfig;
use crate::error::WatchAutoscalingError;
use crate::metrics;
use aws::ec2::ebs::VolumeInfo;
use bosun::{Bosun, Datum, Tags};
use failure::{Error, Fail};
use lambda_runtime::Context;
use log::debug;
use serde;
use serde_derive::{Deserialize, Serialize};
use serde_json::{self, Value};

pub mod asg;
pub mod ebs;
pub mod ping;

#[derive(Debug, Deserialize)]
#[serde(tag = "source")]
pub enum Event {
    #[serde(rename = "aws.autoscaling")]
    Asg(asg::AutoScalingEvent),
    #[serde(rename = "aws.ec2")]
    Ebs(ebs::VolumeEvent),
    #[serde(rename = "ping")]
    Ping(ping::Ping),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum HandleResult {
    #[serde(rename = "empty")]
    Empty,
    #[serde(rename = "ping")]
    Ping { echo_reply: String },
    #[serde(rename = "ec2.ebs.volume_info")]
    VolumeInfo { volume_info: VolumeInfo },
}

pub fn handle<T: Bosun>(json: Value, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<HandleResult, Error> {
    let tags = Tags::new();
    let datum = Datum::now(metrics::LAMBDA_INVOCATION_COUNT, "1", &tags);
    bosun.emit_datum(&datum)?;

    let event = parse_event(json)?;
    let res = handle_event(event, ctx, &config, bosun);

    match res {
        Ok(_) => {
            let datum = Datum::now(metrics::LAMBDA_INVOCATION_RESULT, "0", &tags);
            bosun.emit_datum(&datum)?
        }
        Err(_) => {
            let datum = Datum::now(metrics::LAMBDA_INVOCATION_RESULT, "1", &tags);
            bosun.emit_datum(&datum)?
        }
    }

    res
}

fn parse_event(json: Value) -> Result<Event, Error> {
    let event: Event = serde_json::from_value(json).map_err(|e| e.context(WatchAutoscalingError::FailedParseEvent))?;
    debug!("Parsed event = {:?}.", event);

    Ok(event)
}

fn handle_event<T: Bosun>(event: Event, ctx: &Context, config: &FunctionConfig, bosun: &T) -> Result<HandleResult, Error> {
    match event {
        Event::Asg(asg) => asg::handle(asg, ctx, config, bosun),
        Event::Ebs(ebs) => ebs::handle(ebs, ctx, config, bosun),
        Event::Ping(ping) => ping::handle(ping, ctx, config, bosun),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::asg_mapping::{Mapping, Mappings};
    use bosun::testing::{BosunCallStats, BosunMockClient};

    use env_logger;
    use serde_json::json;
    use spectral::prelude::*;
    use testing::setup;

    #[test]
    fn test_handle_ping() {
        setup();

        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let config = FunctionConfig::default();
        let event = json!(
            { "source": "ping", "ping": "echo request" }
        );
        let expected = BosunCallStats::new(0, 2, 0);

        let res = handle(event, &ctx, &config, &bosun);
        assert_that!(&res).is_ok();

        let bosun_stats = bosun.to_stats();
        asserting("bosun calls")
            .that(&bosun_stats)
            .named("actual calls")
            .is_equal_to(&expected);
    }

    #[test]
    fn test_handle_asg_successful_termination() {
        setup();

        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let mut config = FunctionConfig::default();
        config.asg.mappings = Mappings {
            items: vec![Mapping {
                search: "my".to_string(),
                tag_name: "my".to_string(),
                host_prefix: "my-server-".to_string(),
            }],
        };
        let asg_event = r#"{
  "version": "0",
  "id": "12345678-1234-1234-1234-123456789012",
  "detail-type": "EC2 Instance Terminate Successful",
  "source": "aws.autoscaling",
  "account": "123456789012",
  "time": "yyyy-mm-ddThh:mm:ssZ",
  "region": "us-west-2",
  "resources": [
    "auto-scaling-group-arn",
    "instance-arn"
  ],
  "detail": {
      "StatusCode": "InProgress",
      "Description": "Terminating EC2 instance: i-12345678",
      "AutoScalingGroupName": "my-auto-scaling-group",
      "ActivityId": "87654321-4321-4321-4321-210987654321",
      "Details": {
          "Availability Zone": "us-west-2b",
          "Subnet ID": "subnet-12345678"
      },
      "RequestId": "12345678-1234-1234-1234-123456789012",
      "StatusMessage": "",
      "EndTime": "yyyy-mm-ddThh:mm:ssZ",
      "EC2InstanceId": "i-1234567890abcdef0",
      "StartTime": "yyyy-mm-ddThh:mm:ssZ",
      "Cause": "description-text"
  }
}"#;
        let event = serde_json::from_str(&asg_event).unwrap();
        let expected = BosunCallStats::new(0, 3, 1);

        let res = handle(event, &ctx, &config, &bosun);
        assert_that!(&res).is_ok();

        let bosun_stats = bosun.to_stats();
        asserting("bosun calls")
            .that(&bosun_stats)
            .named("actual calls")
            .is_equal_to(&expected);
    }
}
