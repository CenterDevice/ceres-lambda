use crate::bosun::Bosun;
use crate::config::FunctionConfig;
use failure::Error;
use lambda_runtime::Context;
use log::info;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Ping {
    ping: String,
}

pub fn handle<T: Bosun>(ping: Ping, _: &Context, _: &FunctionConfig, _: &T) -> Result<(), Error> {
    info!("Received {:?}.", ping);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Event;

    use serde_json::json;

    fn setup() {
        crate::testing::setup();
    }

    #[test]
    fn parse_event_ping() {
        setup();

        let event = json!(
            { "ping": "echo request" }
        );

        let parsed = serde_json::from_value(event);

        info!("parsed = {:?}", parsed);

        match parsed {
            Ok(Event::Ping(_)) => {}
            _ => assert!(false),
        }
    }
}
