use chrono::Utc;
use failure::Error;
use lambda_runtime::Context;
use log::{debug, info};
use serde_derive::{Deserialize, Serialize};

use aws::{AwsClientConfig, Filter};
use bosun::{Bosun, Datum, Tags};

use crate::config::{CredentialsConfig, FunctionConfig};
use crate::events::HandleResult;
use crate::metrics;
use duo::DuoClient;

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
    aws_client_config: &AwsClientConfig,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Scheduled Event.");

    let duo_client = DuoClient::new(&config.duo.api_host_name, &config.duo.integration_key, &config.duo.secret_key)?;

    let credentials = credentials(aws_client_config, &duo_client, &config.credentials, bosun)?;

    let handle_result = HandleResult::Cron { credentials };

    Ok(handle_result)
}

#[derive(Debug, Serialize)]
pub struct CredentialStats {
    pub kept: usize,
    pub disabled: usize,
    pub deleted: usize,
}

pub fn credentials<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    duo_client: &DuoClient,
    config: &CredentialsConfig,
    bosun: &T,
) -> Result<CredentialStats, Error> {

    Ok(CredentialStats{
        kept: 0,
        disabled: 0,
        deleted: 0
    })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;
    use spectral::prelude::*;

    use super::*;

    fn setup() {
        testing::setup();
    }

    #[test]
    fn parse_scheduled_event_from_json() {
        setup();

        let json = json!(
            {
                "account": "959123467016",
                "detail": {},
                "detail-type": "Scheduled Event",
                "id": "46cc8812-1000-45bc-50f8-a42d3335eeda",
                "region": "eu-central-1",
                "resources": [
                    "arn:aws:events:eu-central-1:959479900016:rule/scheduled_events_scaletower"
                ],
                "source": "aws.events",
                "time": "2020-08-31T16:51:48Z",
                "version": "0"
            }
        );

        let event: Result<ScheduledEvent, _> = serde_json::from_value(json);

        info!("event = {:?}", event);

        assert_that(&event).is_ok();
    }
}
