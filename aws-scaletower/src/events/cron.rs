use failure::Error;
use lambda_runtime::Context;
use log::info;
use serde_derive::Deserialize;

use aws::AwsClientConfig;
use bosun::Bosun;

use crate::config::FunctionConfig;
use crate::events::HandleResult;

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

    unimplemented!()
}


#[cfg(test)]
mod tests {
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
}