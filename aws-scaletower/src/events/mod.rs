use crate::{config::FunctionConfig, error::AwsScaleTowerError};
use aws::{AwsClientConfig,};
use bosun::{Bosun, Datum, Tags};
use failure::{Error, Fail};
use lambda_runtime::Context;
use log::debug;
use serde_derive::{Deserialize, Serialize};
use serde_json::{self, Value};

pub mod cron;
pub mod ping;

#[derive(Debug, Deserialize)]
#[serde(tag = "source")]
#[allow(clippy::large_enum_variant)]
pub enum Event {
    #[serde(rename = "cron")]
    Cron(cron::ScheduledEvent),
    #[serde(rename = "ping")]
    Ping(ping::Ping),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum HandleResult {
    #[serde(rename = "empty")]
    Empty,
    #[serde(rename = "cron")]
    Cron,
    #[serde(rename = "ping")]
    Ping { echo_reply: String },
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
        .map_err(|e| e.context(AwsScaleTowerError::FailedParseEvent(json.to_string())))?;
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
        Event::Cron(_) => cron::handle(aws_client_config, ctx, config, bosun),
        Event::Ping(ping) => ping::handle(ping, ctx, config, bosun),
    }
}
