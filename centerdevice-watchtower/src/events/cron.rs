use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::Deserialize;

use aws::AwsClientConfig;
use bosun::{Bosun, Datum, Tags};

use crate::centerdevice::health::{self, HealthCheck, HealthCheckResult, HealthSamples};
use crate::config::{FunctionConfig, CenterDeviceHealthConfig};
use crate::events::HandleResult;
use crate::metrics;

// cf. https://docs.aws.amazon.com/lambda/latest/dg/services-cloudwatchevents.html
// {
//   "account": "123456789012",
//   "region": "us-east-2",
//   "detail": {},
//   "detail-type": "Scheduled Event",
//   "source": "aws.events",
//   "time": "2019-03-01T01:23:45Z",
//   "id": "cdc73f9d-aea9-11e3-9d5a-835b769c0d9c",
//   "resources": [
//     "arn:aws:events:us-east-1:123456789012:rule/my-schedule"
//   ]
// }
#[derive(Debug, Deserialize)]
pub struct ScheduledEvent {
    pub account: String,
    pub region: String,
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub time: String,
    pub id: String,
    pub resources: Vec<String>,
}

pub fn handle<T: Bosun>(
    _: &AwsClientConfig,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Scheduled Event.");

    let centerdevice_healthchecks = centerdevice_health(&config.centerdevice_health, bosun)?;

    let handle_result = HandleResult::Cron { centerdevice_healthchecks };

    Ok(handle_result)
}

pub fn centerdevice_health<T: Bosun>(
    config: &CenterDeviceHealthConfig,
    bosun: &T,
) -> Result<usize, Error> {

    let healthchecks = health::health_check(config)?;
    debug!("Received health checks: {:?}", healthchecks);

    bosun_send_healthchecks(bosun, &healthchecks)?;

    Ok(healthchecks.len())
}

fn bosun_send_healthchecks<T: Bosun>(bosun: &T, health_checks: &Vec<HealthCheck>) -> Result<(), Error> {
    for hc in health_checks {
        match &hc.result {
            HealthCheckResult::Ok(ref samples) => bosun_emit_health_samples(bosun, &hc.service, samples)?,
            HealthCheckResult::Failed(_) => bosun_emit_check_failure(bosun, &hc.service)?,
        }
    }

    Ok(())
}

fn bosun_emit_health_samples<T: Bosun>(bosun: &T, service: &str, samples: &HealthSamples) -> Result<(), Error> {
    for (resource, sample) in samples {
        let mut tags = Tags::new();
        tags.insert("service".to_string(), service.to_string());
        tags.insert("resource".to_string(), resource.clone());

        let value = if sample.healthy {
            "1"
        } else {
            "0"
        };

        let datum = if let Some(timestamp) = sample.time_stamp {
            Datum::new(metrics::CENTERDEVICE_HEALTH, timestamp, &value, &tags)
        } else {
            Datum::now(metrics::CENTERDEVICE_HEALTH, &value, &tags)
        };

        bosun.emit_datum(&datum)?;
    }

    Ok(())
}

fn bosun_emit_check_failure<T: Bosun>(bosun: &T, service: &str) -> Result<(), Error> {
    let mut tags = Tags::new();
    tags.insert("service".to_string(), service.to_string());
    tags.insert("resource".to_string(), "global".to_string());
    let value = "-1".to_string();
    let datum = Datum::now(metrics::CENTERDEVICE_HEALTH, &value, &tags);
    bosun.emit_datum(&datum)?;

    Ok(())
}
