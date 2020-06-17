use crate::{config::FunctionConfig, events::HandleResult};
use bosun::Bosun;
use failure::Error;
use lambda_runtime::Context;
use serde_derive::Deserialize;

pub mod ebs;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Ec2Event {
    Ebs(ebs::VolumeEvent),
}

pub fn handle<T: Bosun>(
    event: Ec2Event,
    ctx: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    match event {
        Ec2Event::Ebs(ebs_event) => ebs::handle(ebs_event, ctx, config, bosun),
    }
}
