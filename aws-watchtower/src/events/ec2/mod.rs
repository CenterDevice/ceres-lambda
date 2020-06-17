use crate::{config::FunctionConfig, events::HandleResult};
use bosun::Bosun;
use failure::Error;
use lambda_runtime::Context;
use serde_derive::Deserialize;

pub mod ebs;
#[allow(clippy::module_inception)]
pub mod ec2;

pub use ebs::VolumeEvent;
pub use ec2::Ec2StateChangeEvent;

#[derive(Debug, Deserialize)]
#[serde(tag = "detail-type")]
pub enum Ec2Event {
    #[serde(rename = "EC2 Instance State-change Notification")]
    Ec2StateChangeEvent(Ec2StateChangeEvent),
    #[serde(rename = "EBS Volume Notification")]
    VolumeEvent(VolumeEvent),
}

pub fn handle<T: Bosun>(
    event: Ec2Event,
    ctx: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    match event {
        Ec2Event::Ec2StateChangeEvent(event) => ec2::handle(event, ctx, config, bosun),
        Ec2Event::VolumeEvent(event) => ebs::handle(event, ctx, config, bosun),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use testing::setup;

    #[test]
    /// The purpose of this test is to show if an event received at the `Event` level can be parsed
    /// down to an `ec2::ebs::EbsVolumeEvent`.
    fn test_parse_ebs_volume_event() {
        setup();

        let json = r#"{
            "version": "0",
            "id": "01234567-0123-0123-0123-012345678901",
            "detail-type": "EBS Volume Notification",
            "source": "aws.ec2",
            "account": "012345678901",
            "time": "yyyy-mm-ddThh:mm:ssZ",
            "region": "us-east-1",
            "resources": [
               "arn:aws:ec2:us-east-1:012345678901:volume/vol-01234567"
            ],
            "detail": {
               "result": "available",
               "cause": "",
               "event": "createVolume",
               "request-id": "01234567-0123-0123-0123-0123456789ab"
            }
         }"#;
        let event: Ec2Event = serde_json::from_str(&json).unwrap();

        match event {
            Ec2Event::VolumeEvent(_) => {}
            _ => panic!("Parsed wrong event"),
        }
    }

    #[test]
    /// The purpose of this test is to show if an event received at the `Event` level can be parsed
    /// down to an `ec2::ec2::Ec2StateChangeEvent`.
    fn test_parse_ec2_state_change_event() {
        setup();

        let json = r#"{
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
         }"#;
        let event: Ec2Event = serde_json::from_str(&json).unwrap();

        match event {
            Ec2Event::Ec2StateChangeEvent(_) => {}
            _ => panic!("Parsed wrong event"),
        }
    }
}
