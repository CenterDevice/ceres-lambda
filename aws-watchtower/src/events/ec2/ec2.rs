use crate::{asg_mapping::Mapping, config::FunctionConfig, events::HandleResult, metrics};
use aws::ec2::ec2::{Ec2StateInfo, Ec2State};
use bosun::{Bosun, Datum, Silence, Tags};
use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::Deserialize;

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
    pub state: Ec2State,
}

pub fn handle<T: Bosun>(
    state_change: Ec2StateChangeEvent,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Ec2StateChangeEvent {:?}.", state_change);

    // Get ASG for this instance if any
    let asg = aws::ec2::asg::get_asg_by_instance_id(state_change.detail.instance_id.clone())?;
    info!("Mapped instance id to ASG '{:?}'.", asg.as_ref().map(|x| x.auto_scaling_group_name.as_str()).unwrap_or("unmapped"));

    let mapping = asg
        .as_ref()
        .map(|x| x.auto_scaling_group_name.as_str())
        .map(|auto_scaling_group_name| config.asg.mappings.map(auto_scaling_group_name))
        .flatten();
    info!("Mapped ASG to '{:?}'.", mapping);

    let mut tags = Tags::new();
    tags.insert(
        "asg".to_string(),
        mapping
            .map(|x| x.tag_name.to_string())
            .unwrap_or_else(|| "unmapped".to_string()),
    );
    let value = (state_change.detail.state as u32).to_string();
    let datum = Datum::now(metrics::EC2_STATE_CHANGE, &value, &tags);
    bosun.emit_datum(&datum)?;

    // We haven't found an ASG, so this instance does not belong to an ASG.
    // In this case, the instance cannot have been terminated because of an 
    // auto-scaling lifecycle event. There we not going to set a silence to 
    // prevent silencing a infrastructure problem.
    match mapping {
        Some(ref mapping) if state_change.detail.state == Ec2State::ShuttingDown => {
            set_bosun_silence(&state_change.detail.instance_id, &config.asg.scaledown_silence_duration, mapping, bosun)?;
        }
        Some(_) => {
            debug!("Non shut-down state change for instance id ({}), no silence necessary", &state_change.detail.instance_id);
        }
        None => {
            info!("No ASG found for instance id ({}), refusing to set a silence", &state_change.detail.instance_id);
        }
    }

    let ec2_state_info = Ec2StateInfo {
        ec2_instance_id: state_change.detail.instance_id,
        state: state_change.detail.state,
    };

    Ok(HandleResult::Ec2StateInfo{ec2_state_info})
}

fn set_bosun_silence(
    instance_id: &str,
    duration: &str,
    mapping: &Mapping,
    bosun: &dyn Bosun,
) -> Result<(), Error> {
    let host = format!("{}{}*", &mapping.host_prefix, instance_id);
    info!("Setting silence of {} for host '{}'.", duration, host);

    let silence = Silence::host(&host, duration);
    bosun.set_silence(&silence)?;

    Ok(())
}

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
