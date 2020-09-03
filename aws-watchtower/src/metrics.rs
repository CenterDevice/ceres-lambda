use bosun::{Bosun, Metadata};
use failure::Error;

pub static ASG_UP_DOWN: &str = "aws.ec2.asg.scaling.event";
pub static EC2_STATE_CHANGE: &str = "aws.ec2.ec2.state_change.event";
pub static EBS_VOLUME_EVENT: &str = "aws.ec2.ebs.volume.change.event";
pub static EBS_VOLUME_CREATION_RESULT: &str = "aws.ec2.ebs.volume.creation.result";

pub fn send_metadata<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadatas = bosun_metadata();
    lambda::bosun::send_metadata(bosun, &metadatas)
}

fn bosun_metadata() -> Vec<Metadata<'static>> {
    let mut metadatas = Vec::new();

    metadatas.push(Metadata::new(
        ASG_UP_DOWN,
        "rate",
        "Scaling",
        "ASG up and down scaling event [-1 = down scaling, +1 = up scaling]",
    ));

    metadatas.push(Metadata::new(
        EBS_VOLUME_EVENT,
        "rate",
        "Change",
        "Creation or deletion of EBS volumes [-1 = deletion, +1 = creation]",
    ));

    metadatas.push(Metadata::new(
        EBS_VOLUME_CREATION_RESULT,
        "gauge",
        "Result",
        "Creation result of EBS volumes [0 = success, 1 = failure]",
    ));

    metadatas.push(Metadata::new(
        EC2_STATE_CHANGE,
        "gauge",
        "State",
        "Instance State Change Event [1 = pending, 2 = running, 3 = shutting-down, 4 = stopping, 5 = stopped, 6 = \
         terminated]",
    ));

    metadatas
}
