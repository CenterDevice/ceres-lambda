use crate::{asg_mapping::Mapping, config::FunctionConfig, error::AwsWatchtowerError, events::HandleResult, metrics};
use bosun::{Bosun, Datum, Silence, Tags};
use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::{Deserialize, Serialize};

// cf. https://docs.aws.amazon.com/AmazonCloudWatch/latest/events/EventTypes.html#ec2_event_type
// {
//    "id":"7bf73129-1428-4cd3-a780-95db273d1602",
//    "detail-type":"EC2 Instance State-change Notification",
//    "source":"aws.ec2",
//    "account":"123456789012",
//    "time":"2019-11-11T21:29:54Z",
//    "region":"us-east-1",
//    "resources":[
//       "arn:aws:ec2:us-east-1:123456789012:instance/i-abcd1111"
//    ],
//    "detail":{
//       "instance-id":"i-abcd1111",
//       "state":"pending"
//    }
// }
#[derive(Debug, Deserialize)]
pub struct Ec2StateChangeEvent {
    pub id: String,
    /* These fields do not exist, because serde used both of them to "route" the deserialization to this point.
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub source: String,
    */
    pub account:     String,
    pub time:        String,
    pub region:      String,
    pub resources:   Vec<String>,
    pub detail:      Ec2StateChangeDetail,
}

#[derive(Debug, Deserialize)]
pub struct Ec2StateChangeDetail {
    #[serde(rename = "instance-id")]
    pub instance_id:         String,
    pub state: Ec2StateChangeState,
}

#[derive(PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ec2StateChangeState {
    Pending,
    Running,
    ShuttingDown,
    Stopped,
    Stopping,
    Terminated,
}

/*
impl<'a> Ec2StateChangeEvent<'a> {
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
            _ => Err(Error::from(AwsWatchtowerError::FailedParseAsgEvent)),
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
            instance_id:             &asg.detail.ec2_instance_id,
            auto_scaling_group_name: &asg.detail.auto_scaling_group_name,
        };

        Ok(AsgLifeCycleEvent::SuccessfulTermination(details))
    }
}

pub fn handle<T: Bosun>(
    asg: AutoScalingEvent,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
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
    let datum = Datum::now(metrics::ASG_UP_DOWN, &value, &tags);
    bosun.emit_datum(&datum)?;

    if let AsgLifeCycleEvent::SuccessfulTermination(ref details) = event {
        set_bosun_silence(details, &config.asg.scaledown_silence_duration, mapping, bosun)?
    };

    let auto_scaling_info = AsgScalingInfo {
        ec2_instance_id:         asg.detail.ec2_instance_id,
        auto_scaling_group_name: asg.detail.auto_scaling_group_name,
        auto_scaling_event:      asg.detail_type,
    };

    Ok(HandleResult::AsgScalingInfo { auto_scaling_info })
}

fn set_bosun_silence(
    details: &TerminationDetails,
    duration: &str,
    mapping: Option<&Mapping>,
    bosun: &dyn Bosun,
) -> Result<(), Error> {
    let host_prefix = mapping.map(|x| &x.host_prefix).ok_or_else(|| {
        Error::from(AwsWatchtowerError::NoHostMappingFound(
            details.auto_scaling_group_name.to_string(),
        ))
    })?;

    let host = format!("{}{}*", &host_prefix, details.instance_id);
    info!("Setting silence of {} for host '{}'.", duration, host);

    let silence = Silence::host(&host, duration);
    bosun.set_silence(&silence)?;

    Ok(())
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use spectral::prelude::*;
    use testing;

    fn setup() { testing::setup(); }

     #[test]
     fn parse_pending_ec2_state_change_event_from_json() {
        setup();

        let json = json!(
            {
                "id":"7bf73129-1428-4cd3-a780-95db273d1602",
                "detail-type":"EC2 Instance State-change Notification",
                "source":"aws.ec2",
                "account":"123456789012",
                "time":"2019-11-11T21:29:54Z",
                "region":"us-east-1",
                "resources":[
                    "arn:aws:ec2:us-east-1:123456789012:instance/i-abcd1111"
                ],
                "detail":{
                    "instance-id":"i-abcd1111",
                    "state":"pending"
                }
            }
        );

        let event: Result<Ec2StateChangeEvent, _> = serde_json::from_value(json);

        info!("event = {:?}", event);

        assert_that(&event).is_ok();
    }

     #[test]
    fn parse_shutting_down_ec2_state_change_event_from_json() {
        setup();

        let json = json!(
            {
                "id":"7bf73129-1428-4cd3-a780-95db273d1602",
                "detail-type":"EC2 Instance State-change Notification",
                "source":"aws.ec2",
                "account":"123456789012",
                "time":"2019-11-11T21:29:54Z",
                "region":"us-east-1",
                "resources":[
                    "arn:aws:ec2:us-east-1:123456789012:instance/i-abcd1111"
                ],
                "detail":{
                    "instance-id":"i-abcd1111",
                    "state":"shutting-down"
                }
            }
        );

        let event: Result<Ec2StateChangeEvent, _> = serde_json::from_value(json);

        info!("event = {:?}", event);

        assert_that(&event).is_ok();
     }

/*
    fn asg_success_full_termination_event() -> AutoScalingEvent {
        AutoScalingEvent {
            version:     "0".to_string(),
            id:          "12345678-1234-1234-1234-123456789012".to_string(),
            detail_type: "EC2 Instance Terminate Successful".to_string(),
            account:     "123456789012".to_string(),
            time:        Utc::now().to_string(),
            region:      "us-west-2".to_string(),
            resources:   vec!["auto-scaling-group-arn".to_string(), "instance-arn".to_string()],
            detail:      AutoScalingEventDetail {
                request_id:              "12345678-1234-1234-1234-123456789012".to_string(),
                ec2_instance_id:         "i-1234567890abcdef0".to_string(),
                auto_scaling_group_name: "my-auto-scaling-group".to_string(),
            },
        }
    }

    #[test]
    fn parse_asg_lifecycle_event_from_asg_success_full_termination() {
        setup();

        let asg = asg_success_full_termination_event();
        let instance_id = "i-1234567890abcdef0".to_string();
        let auto_scaling_group_name = "my-auto-scaling-group".to_string();
        let expected_details = TerminationDetails {
            instance_id:             instance_id.as_str(),
            auto_scaling_group_name: auto_scaling_group_name.as_str(),
        };

        let asg_event = AsgLifeCycleEvent::try_from(&asg);

        asserting("failed to parse asg event").that(&asg_event).is_ok();
        match asg_event.unwrap() {
            AsgLifeCycleEvent::SuccessfulTermination(ref details) => {
                assert_that(&details)
                    .named("termination details")
                    .is_equal_to(&expected_details)
            }
            _ => panic!("wrong event"),
        };
    }

    */
}
