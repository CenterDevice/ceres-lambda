use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::Deserialize;

use aws::{AwsClientConfig, Filter};
use bosun::{Bosun, Datum, Tags};

use crate::config::{FunctionConfig, BurstBalanceConfig};
use crate::events::HandleResult;
use chrono::Utc;
use crate::burst_balance::{get_burst_balances, BurstBalance};
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
    pub source: String,
    pub time: String,
    pub id: String,
    pub resources: Vec<String>,
}

pub fn handle<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Scheduled Event.");

    let burst_balance = burst_balance(aws_client_config, &config.burst_balance, bosun)?;

    let handle_result = HandleResult::Cron {
        burst_balance,
    };

    Ok(handle_result)
}

pub fn burst_balance<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    config: &BurstBalanceConfig,
    bosun: &T,
) -> Result<usize, Error> {

    let filters = vec![
        Filter::new("instance-state-name", vec!["running"]),
        Filter::new("tag:Name", vec![config.instance_name_filter.as_str()]),
    ];

    let end = Utc::now();
    let start = end - chrono::Duration::minutes(config.look_back_min);
    let forecasts = get_burst_balances(&aws_client_config, start, end, None, filters)?;
    debug!("Received burst balances: {:?}", forecasts);

    let candidates: Vec<_> = forecasts.into_iter().filter(|x| x.is_exhausted(config)).collect();
    debug!("Burst balance: Identified candidates: '{:?}'", candidates);

    let candidates: Vec<_> = candidates.into_iter().map(|x| x.instance_id).collect();
    let len = candidates.len();
    info!("Burst balance: Identified {} candidates: '{:?}'", len, candidates);
    bosun_emit_candidates(bosun, len)?;

    if config.terminate {
        aws::ec2::ec2::terminate_instances(aws_client_config, candidates)?;
        bosun_emit_terminated(bosun, len)?;
    }

    Ok(len)
}

trait IsExhausted {
    fn is_exhausted(&self, config: &BurstBalanceConfig) -> bool;
}

impl IsExhausted for BurstBalance {
    fn is_exhausted(&self, config: &BurstBalanceConfig) -> bool {
        let balance = if let Some(balance) = self.balance {
            balance <= config.burst_balance_limit
        } else {
            false
        };

        let eta = if let Some(forecast) = self.forecast {
            let time_left = (forecast - Utc::now()).num_minutes();
            config.use_linear_regression && time_left <= config.eta_limit_min
        } else {
            false
        };

        balance || eta
    }
}

fn bosun_emit_candidates<T: Bosun>(bosun: &T, value: usize) -> Result<(), Error> {
    let tags = Tags::new();
    let value = value.to_string();
    let datum = Datum::now(metrics::BURST_BALANCE_TERMINATION_CANDIDATES, &value, &tags);
    bosun.emit_datum(&datum)?;

    Ok(())
}

fn bosun_emit_terminated<T: Bosun>(bosun: &T, value: usize) -> Result<(), Error> {
    let tags = Tags::new();
    let value = value.to_string();
    let datum = Datum::now(metrics::BURST_BALANCE_TERMINATION_TERMINATED, &value, &tags);
    bosun.emit_datum(&datum)?;

    Ok(())
}


#[cfg(test)]
mod tests {
    use serde_json::json;
    use spectral::prelude::*;

    use super::*;
    use chrono::Duration;

    fn setup() {
        testing::setup();
    }

    #[test]
    fn parse_scheduled_event_from_json() {
        setup();

        let json = json!(
            {
              "account": "123456789012",
              "region": "us-east-2",
              "detail": {},
              "detail-type": "Scheduled Event",
              "source": "aws.events",
              "time": "2019-03-01T01:23:45Z",
              "id": "cdc73f9d-aea9-11e3-9d5a-835b769c0d9c",
              "resources": [
                "arn:aws:events:us-east-1:123456789012:rule/my-schedule"
              ]
            }
        );

        let event: Result<ScheduledEvent, _> = serde_json::from_value(json);

        info!("event = {:?}", event);

        assert_that(&event).is_ok();
    }

    #[test]
    fn is_exhausted_balance_negative() {
        let config = BurstBalanceConfig {
            instance_name_filter: "not-relevant".to_string(),
            look_back_min: 0,
            use_linear_regression: false,
            burst_balance_limit: 10.0,
            eta_limit_min: 10,
            terminate: false
        };
        let burst_balance = BurstBalance {
            volume_id: "vol-123".to_string(),
            instance_id: "i-456".to_string(),
            timestamp: Some(Utc::now()),
            balance: Some(11.0),
            forecast: Some(Utc::now() + Duration::minutes(11)),
        };

        let res = burst_balance.is_exhausted(&config);

        assert_that(&res).is_false();
    }

    #[test]
    fn is_exhausted_balance_positive() {
        let config = BurstBalanceConfig {
            instance_name_filter: "not-relevant".to_string(),
            look_back_min: 0,
            use_linear_regression: false,
            burst_balance_limit: 10.0,
            eta_limit_min: 10,
            terminate: false
        };
        let burst_balance = BurstBalance {
            volume_id: "vol-123".to_string(),
            instance_id: "i-456".to_string(),
            timestamp: Some(Utc::now()),
            balance: Some(9.0),
            forecast: Some(Utc::now() + Duration::minutes(11)),
        };

        let res = burst_balance.is_exhausted(&config);

        assert_that(&res).is_true();
    }

    #[test]
    fn is_exhausted_forecast_negative() {
        let config = BurstBalanceConfig {
            instance_name_filter: "not-relevant".to_string(),
            look_back_min: 0,
            use_linear_regression: true,
            burst_balance_limit: 10.0,
            eta_limit_min: 10,
            terminate: false
        };
        let burst_balance = BurstBalance {
            volume_id: "vol-123".to_string(),
            instance_id: "i-456".to_string(),
            timestamp: Some(Utc::now()),
            balance: Some(11.0),
            forecast: Some(Utc::now() + Duration::minutes(12)),
        };

        let res = burst_balance.is_exhausted(&config);

        assert_that(&res).is_false();
    }

    #[test]
    fn is_exhausted_forecast_positive() {
        let config = BurstBalanceConfig {
            instance_name_filter: "not-relevant".to_string(),
            look_back_min: 0,
            use_linear_regression: true,
            burst_balance_limit: 10.0,
            eta_limit_min: 10,
            terminate: false
        };
        let burst_balance = BurstBalance {
            volume_id: "vol-123".to_string(),
            instance_id: "i-456".to_string(),
            timestamp: Some(Utc::now()),
            balance: Some(11.0),
            forecast: Some(Utc::now() + Duration::minutes(9)),
        };

        let res = burst_balance.is_exhausted(&config);

        assert_that(&res).is_true();
    }
}