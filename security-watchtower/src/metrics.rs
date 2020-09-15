use bosun::{Bosun, Metadata};
use failure::Error;

pub static CREDENTIAL_LAST_USAGE: &str = "security.credentials.last_usage";
pub static SCHEDULED_EVENT: &str = "aws.events.scheduled_event";

pub fn send_metadata<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadatas = bosun_metadata();
    lambda::bosun::send_metadata(bosun, &metadatas)
}

fn bosun_metadata() -> Vec<Metadata<'static>> {
    let mut metadatas = Vec::new();

    metadatas.push(Metadata::new(
        CREDENTIAL_LAST_USAGE,
        "gauge",
        "Days",
        "Number of days a credential has been used for the last time; -1 is used, if unknown",
    ));

    metadatas.push(Metadata::new(SCHEDULED_EVENT, "gauge", "Event", "AWS schedule event"));

    metadatas
}
