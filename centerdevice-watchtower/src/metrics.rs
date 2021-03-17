use failure::Error;

use bosun::{Bosun, Metadata};

pub static CENTERDEVICE_HEALTH: &str = "cd.health.service.status";
pub static SCHEDULED_EVENT: &str = "aws.events.scheduled_event";

pub fn send_metadata<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadatas = bosun_metadata();
    lambda::bosun::send_metadata(bosun, &metadatas)
}

fn bosun_metadata() -> Vec<Metadata<'static>> {
    let mut metadatas = Vec::new();

    metadatas.push(Metadata::new(
        CENTERDEVICE_HEALTH,
        "gauge",
        "Health",
        "CenterDevice app health per service and resource [1=up, 0=down, -1=failed to retrieve health]. Mind that results may come from different backend servers for each call and thus, time stamps may very.",
    ));

    metadatas.push(Metadata::new(SCHEDULED_EVENT, "gauge", "Event", "AWS schedule event"));

    metadatas
}
