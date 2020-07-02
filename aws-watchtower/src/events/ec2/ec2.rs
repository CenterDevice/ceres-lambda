use crate::{asg_mapping::Mapping, config::FunctionConfig, events::HandleResult, metrics};
use aws::AwsClientConfig;
use aws::ec2::ec2::{Ec2State, Ec2StateInfo};
use bosun::{Bosun, Datum, Silence, Tags};
use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::Deserialize;
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};

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
    pub id:        String,
    // These fields do not exist, because serde used both of them to "route" the deserialization to this point.
    // #[serde(rename = "detail-type")]
    // pub detail_type: String,
    // pub source: String,
    pub account:   String,
    pub time:      String,
    pub region:    String,
    pub resources: Vec<String>,
    pub detail:    Ec2StateChangeDetail,
}

#[derive(Debug, Deserialize)]
pub struct Ec2StateChangeDetail {
    #[serde(rename = "instance-id")]
    pub instance_id: String,
    pub state:       Ec2State,
}

lazy_static::lazy_static! {
    /// This HashSet stores Ids of instanced that have been already silenced
    /// -- yes, it's volatile, it's not multi-invocation safe.
    ///
    /// Problem: Ec2 State Change Events do not arrive in a strict manner from ShuttingDown, Stopping,
    /// Stopped to Terminated. The order may vary. If we only react on ShuttingDown, we miss an earlier
    /// indicator of the shut down in process and thus, miss the opportunity to silence early. This may
    /// still lead to Unknown alarms in Bosun
    ///
    /// Goal: We have to react on any shut down indication State while preventing to set an silence
    /// for each, because it creates unnecessary burden on Bosun and increases latency of this
    /// function. Therefore, we need to preserve a state for each already processed / silenced instance
    /// Id.
    ///
    /// Solution: We exploit the fact that these shut down State Change events come in quick
    /// succession. This makes it highly probable that the succeeding events will hit the already
    /// same, already running function instance. So we store the state in this static.
    /// In case the function has been shut down and needs to be restarted cold or we hit another
    /// instance of the function not much harm is done at we just fall back to the multiple silence
    /// situation described above.
    // Make this thread safe.
    static ref SILENCED_INSTANCES: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

}

pub fn handle<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    state_change: Ec2StateChangeEvent,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Ec2StateChangeEvent {:?}.", state_change);

    // Get ASG for this instance if any
    let asg = aws::ec2::asg::get_asg_by_instance_id(aws_client_config, state_change.detail.instance_id.clone())?;
    info!(
        "Mapped instance id to ASG '{:?}'.",
        asg.as_ref()
            .map(|x| x.auto_scaling_group_name.as_str())
            .unwrap_or("unmapped")
    );

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

    let instance_going_down = state_change.detail.state.is_going_down();
    let instance_already_silened = {
        let silenced_instances = SILENCED_INSTANCES
            .read()
            .expect("Could not retrieve Mutex lock (r) for SILENCED_INSTANCES");
        silenced_instances.contains(&state_change.detail.instance_id)
    };

    // If we haven't found an ASG this instance belongs to, the
    // the instance cannot have been terminated because of an
    // auto-scaling lifecycle event. Therefore we're not going to set a silence to
    // prevent silencing a infrastructure problem.
    match mapping {
        Some(ref mapping) if instance_going_down && !instance_already_silened => {
            set_bosun_silence(
                &state_change.detail.instance_id,
                &config.ec2.scaledown_silence_duration,
                mapping,
                bosun,
            )?;
            // Save state: this instance has been silenced
            let mut silenced_instances = SILENCED_INSTANCES
                .write()
                .expect("Could not retrieve Mutex lock (w) for SILENCED_INSTANCES");
            silenced_instances.insert(state_change.detail.instance_id.clone());
        }
        Some(_) if instance_going_down && instance_already_silened => {
            debug!(
                "Instance id ({}) has already been silenced, no silence necessary",
                &state_change.detail.instance_id
            );
        }
        Some(_) => {
            debug!(
                "Non-shutting-down state change for instance id ({}), no silence necessary",
                &state_change.detail.instance_id
            );
        }
        None => {
            info!(
                "No ASG found for instance id ({}), refusing to set a silence",
                &state_change.detail.instance_id
            );
        }
    }

    let ec2_state_info = Ec2StateInfo {
        ec2_instance_id: state_change.detail.instance_id,
        state:           state_change.detail.state,
    };

    Ok(HandleResult::Ec2StateInfo { ec2_state_info })
}

fn set_bosun_silence(instance_id: &str, duration: &str, mapping: &Mapping, bosun: &dyn Bosun) -> Result<(), Error> {
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
}
