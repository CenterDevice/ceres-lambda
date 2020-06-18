use crate::{config::FunctionConfig, events::HandleResult};
use bosun::Bosun;
use failure::Error;
use lambda_runtime::Context;
use log::info;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Ping {
    ping: String,
}

pub fn handle<T: Bosun>(ping: Ping, _: &Context, _: &FunctionConfig, _: &T) -> Result<HandleResult, Error> {
    info!("Received {:?}.", ping);

    Ok(HandleResult::Ping {
        echo_reply: "Echo reply".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{super::Event, *};

    use serde_json::json;
    use spectral::prelude::*;

    fn setup() { testing::setup(); }

    #[test]
    #[allow(clippy::assertions_on_constants)]
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

    #[test]
    fn serialize_handle_result() {
        setup();

        let result = HandleResult::Ping {
            echo_reply: "Echo reply".to_string(),
        };

        let res = serde_json::to_string(&result);
        println!("Serialized = {:?}", res);

        assert_that!(&res).is_ok();
    }
}
