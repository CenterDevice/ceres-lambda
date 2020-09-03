use bosun::{Bosun, Metadata};
use failure::Error;

pub static BURST_BALANCE_TERMINATION_CANDIDATES: &str = "aws.burst_balance.termination.candidates";
pub static BURST_BALANCE_TERMINATION_TERMINATED: &str = "aws.burst_balance.termination.terminated";
pub static SCHEDULED_EVENT: &str = "aws.events.scheduled_event";

pub fn send_metadata<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadatas = bosun_metadata();
    lambda::bosun::send_metadata(bosun, &metadatas)
}

fn bosun_metadata() -> Vec<Metadata<'static>> {
    let mut metadatas = Vec::new();

    metadatas.push(Metadata::new(
        BURST_BALANCE_TERMINATION_CANDIDATES,
        "gauge",
        "Instances",
        "Number of candidate instances to terminate because of burst balance exhaustion",
    ));

    metadatas.push(Metadata::new(
        BURST_BALANCE_TERMINATION_TERMINATED,
        "gauge",
        "Instances",
        "Number of instances terminated because of burst balance exhaustion",
    ));

    metadatas.push(Metadata::new(SCHEDULED_EVENT, "gauge", "Event", "AWS schedule event"));

    metadatas
}
