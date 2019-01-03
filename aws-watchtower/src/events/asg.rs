use crate::asg_mapping::Mapping;
use crate::bosun::{self, Bosun, Datum, Silence, Tags};
use crate::config::FunctionConfig;
use crate::events::HandleResult;
use crate::error::WatchAutoscalingError;
use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde;
use serde_derive::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoScalingEvent {
    pub version: String,
    pub id: String,
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub account: String,
    pub time: String,
    pub region: String,
    pub resources: Vec<String>,
    pub detail: AutoScalingEventDetail,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoScalingEventDetail {
    #[serde(rename = "RequestId")]
    pub request_id: String,
    #[serde(rename = "AutoScalingGroupName")]
    pub auto_scaling_group_name: String,
    #[serde(rename = "EC2InstanceId")]
    pub ec2_instance_id: String,
}

#[derive(Debug)]
pub enum AsgLifeCycleEvent<'a> {
    SuccessfulLaunch(LifeCycleDetails<'a>),
    UnsuccessfulLaunch(LifeCycleDetails<'a>),
    SuccessfulTermination(TerminationDetails<'a>),
    UnsuccessfulTermination(LifeCycleDetails<'a>),
}

#[derive(PartialEq, Eq, Debug)]
pub struct LifeCycleDetails<'a> {
    pub auto_scaling_group_name: &'a str,
}

#[derive(PartialEq, Eq, Debug)]
pub struct TerminationDetails<'a> {
    pub instance_id: &'a str,
    pub auto_scaling_group_name: &'a str,
}

impl<'a> AsgLifeCycleEvent<'a> {
    pub fn try_from(asg: &'a AutoScalingEvent) -> Result<AsgLifeCycleEvent<'a>, Error> {
        match asg.detail_type.as_str() {
            "EC2 Instance Launch Successful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::SuccessfulLaunch(details))
            }
            "EC2 Instance Launch Unsuccessful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::UnsuccessfulLaunch(details))
            }
            "EC2 Instance Terminate Successful" => AsgLifeCycleEvent::successful_termination_from(asg),
            "EC2 Instance Terminate Unsuccessful" => {
                let details = AsgLifeCycleEvent::lifecycle_details_from(asg)?;
                Ok(AsgLifeCycleEvent::UnsuccessfulTermination(details))
            }
            _ => Err(Error::from(WatchAutoscalingError::FailedParseAsgEvent)),
        }
    }

    fn lifecycle_details_from(asg: &'a AutoScalingEvent) -> Result<LifeCycleDetails<'a>, Error> {
        let details = LifeCycleDetails {
            auto_scaling_group_name: &asg.detail.auto_scaling_group_name,
        };

        Ok(details)
    }

    fn successful_termination_from(asg: &'a AutoScalingEvent) -> Result<AsgLifeCycleEvent<'a>, Error> {
        let details = TerminationDetails {
            instance_id: &asg.detail.ec2_instance_id,
            auto_scaling_group_name: &asg.detail.auto_scaling_group_name,
        };

        Ok(AsgLifeCycleEvent::SuccessfulTermination(details))
    }
}

pub fn handle<T: Bosun>(asg: AutoScalingEvent, _: &Context, config: &FunctionConfig, bosun: &T) -> Result<HandleResult, Error> {
    debug!("Received AutoScalingEvent {:?}.", asg);
    let event = AsgLifeCycleEvent::try_from(&asg)?;
    info!("Received AsgLifeCycleEvent {:?}.", event);

    let (asg_name, value) = match event {
        AsgLifeCycleEvent::SuccessfulLaunch(ref x) => (x.auto_scaling_group_name, 1),
        AsgLifeCycleEvent::UnsuccessfulLaunch(ref x) => (x.auto_scaling_group_name, 0),
        AsgLifeCycleEvent::SuccessfulTermination(ref x) => (x.auto_scaling_group_name, -1),
        AsgLifeCycleEvent::UnsuccessfulTermination(ref x) => (x.auto_scaling_group_name, 0),
    };

    let mapping = config.asg.mappings.map(asg_name);
    info!("Mapped ASG to '{:?}'.", mapping);

    let mut tags = Tags::new();
    tags.insert(
        "asg".to_string(),
        mapping
            .map(|x| x.tag_name.to_string())
            .unwrap_or_else(|| "unmapped".to_string()),
    );
    let value = value.to_string();
    let datum = Datum::now(bosun::METRIC_ASG_UP_DOWN, &value, &tags);
    bosun.emit_datum(&datum)?;

    if let AsgLifeCycleEvent::SuccessfulTermination(ref details) = event {
        set_bosun_silence(details, &config.asg.scaledown_silence_duration, mapping, bosun)?
    };

    Ok(HandleResult::Empty)
}

fn set_bosun_silence(
    details: &TerminationDetails,
    duration: &str,
    mapping: Option<&Mapping>,
    bosun: &Bosun,
) -> Result<(), Error> {
    let host_prefix = mapping.map(|x| &x.host_prefix).ok_or_else(|| {
        Error::from(WatchAutoscalingError::NoHostMappingFound(
            details.auto_scaling_group_name.to_string(),
        ))
    })?;

    let host = format!("{}{}*", &host_prefix, details.instance_id);
    info!("Setting silence of {} for host '{}'.", duration, host);

    let silence = Silence::host(&host, duration);
    bosun.set_silence(&silence)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::offset::Utc;
    use env_logger;
    use spectral::prelude::*;

    fn setup() {
        crate::testing::setup();
    }

    fn asg_success_full_termination_event() -> AutoScalingEvent {
        let asg = AutoScalingEvent {
            version: "0".to_string(),
            id: "12345678-1234-1234-1234-123456789012".to_string(),
            detail_type: "EC2 Instance Terminate Successful".to_string(),
            account: "123456789012".to_string(),
            time: Utc::now().to_string(),
            region: "us-west-2".to_string(),
            resources: vec!["auto-scaling-group-arn".to_string(), "instance-arn".to_string()],
            detail: AutoScalingEventDetail {
                request_id: "12345678-1234-1234-1234-123456789012".to_string(),
                ec2_instance_id: "i-1234567890abcdef0".to_string(),
                auto_scaling_group_name: "my-auto-scaling-group".to_string(),
            }
        };
        asg
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
            AsgLifeCycleEvent::SuccessfulTermination(ref details) => assert_that(&details)
                .named("termination details")
                .is_equal_to(&expected_details),
            _ => panic!("wrong event"),
        };
    }

}
