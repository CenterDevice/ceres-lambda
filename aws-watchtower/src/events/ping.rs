use crate::bosun::Bosun;
use crate::config::FunctionConfig;
use crate::events::HandleResult;
use failure::Error;
use lambda_runtime::Context;
use log::info;
use serde;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Ping {
    ping: String,
}

pub fn handle<T: Bosun>(ping: Ping, _: &Context, _: &FunctionConfig, _: &T) -> Result<HandleResult, Error> {
    info!("Received {:?}.", ping);

    Ok(HandleResult::Ping("Echo reply".to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::Event;
    use super::*;

    use serde_json::json;

    fn setup() {
        crate::testing::setup();
    }

    #[test]
    fn parse_event_ping() {
        setup();

        let event = json!(
            { "source": "ping", "ping": "echo request" }
        );

        let parsed = serde_json::from_value(event);

        info!("parsed = {:?}", parsed);

        match parsed {
            Ok(Event::Ping(_)) => {}
            _ => assert!(false),
        }
    }
}
