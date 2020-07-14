use crate::{config::FunctionConfig, error::AwsWatchtowerError};
use aws::{
    ec2::{asg::AsgScalingInfo, ebs::VolumeInfo, ec2::Ec2StateInfo},
    AwsClientConfig,
};
use bosun::{Bosun, Datum, Tags};
use failure::{Error, Fail};
use lambda;
use lambda_runtime::Context;
use log::debug;
use serde_derive::{Deserialize, Serialize};
use serde_json::{self, Value};

pub mod asg;
pub mod ec2;
pub mod ping;

#[derive(Debug, Deserialize)]
#[serde(tag = "source")]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    #[serde(rename = "aws.autoscaling")]
    Asg(asg::AutoScalingEvent),
    #[serde(rename = "aws.ec2")]
    Ec2(ec2::Ec2Event),
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
    #[serde(rename = "ec2.asg.auto_scaling_info")]
    AsgScalingInfo { auto_scaling_info: AsgScalingInfo },
    #[serde(rename = "ec2.ec2.state_info")]
    Ec2StateInfo { ec2_state_info: Ec2StateInfo },
    #[serde(rename = "ec2.ebs.volume_info")]
    VolumeInfo { volume_info: VolumeInfo },
}

pub fn handle<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    json: Value,
    ctx: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    let tags = Tags::new();
    let datum = Datum::now(lambda::metrics::LAMBDA_INVOCATION_COUNT, "1", &tags);
    bosun.emit_datum(&datum)?;

    let res = parse_event(json).and_then(|event| handle_event(aws_client_config, event, ctx, &config, bosun));

    match res {
        Ok(_) => {
            let datum = Datum::now(lambda::metrics::LAMBDA_INVOCATION_RESULT, "0", &tags);
            bosun.emit_datum(&datum)?
        }
        Err(_) => {
            let datum = Datum::now(lambda::metrics::LAMBDA_INVOCATION_RESULT, "1", &tags);
            bosun.emit_datum(&datum)?
        }
    }

    res
}

fn parse_event(json: Value) -> Result<Event, Error> {
    let event = serde_json::from_value(json.clone())
        .map_err(|e| e.context(AwsWatchtowerError::FailedParseEvent(json.to_string())))?;
    debug!("Parsed event = {:?}.", event);

    Ok(event)
}

fn handle_event<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    event: Event,
    ctx: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    match event {
        Event::Asg(asg) => asg::handle(asg, ctx, config, bosun),
        Event::Ec2(ec2) => ec2::handle(aws_client_config, ec2, ctx, config, bosun),
        Event::Ping(ping) => ping::handle(ping, ctx, config, bosun),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::asg_mapping::{Mapping, Mappings};
    use bosun::testing::{BosunCallStats, BosunMockClient};

    use serde_json::json;
    use spectral::prelude::*;
    use testing::setup;

    #[test]
    fn test_parsing_error() {
        setup();

        let event = json!(
            { "this": "object", "does": "not parse" }
        );
        let res = parse_event(event);

        asserting("Parsing failed").that(&res).is_err();
        let err = res.unwrap_err().to_string();
        asserting("Error contains original event").that(&err).contains("object");
    }

    #[test]
    fn test_handle_ping() {
        setup();

        let aws_client_config = AwsClientConfig::new().expect("Failed to create AWS client config.");
        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let config = FunctionConfig::default();
        let event = json!(
            { "source": "ping", "ping": "echo request" }
        );
        let expected = BosunCallStats::new(0, 2, 0);

        let res = handle(&aws_client_config, event, &ctx, &config, &bosun);
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

        let aws_client_config = AwsClientConfig::new().expect("Failed to create AWS client config.");
        let bosun: BosunMockClient = Default::default();
        let ctx = Context::default();
        let mut config = FunctionConfig::default();
        config.asg.mappings = Mappings {
            items: vec![Mapping {
                search:      "my".to_string(),
                tag_name:    "my".to_string(),
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

        let res = handle(&aws_client_config, event, &ctx, &config, &bosun);
        assert_that!(&res).is_ok();

        let bosun_stats = bosun.to_stats();
        asserting("bosun calls")
            .that(&bosun_stats)
            .named("actual calls")
            .is_equal_to(&expected);
    }
}
