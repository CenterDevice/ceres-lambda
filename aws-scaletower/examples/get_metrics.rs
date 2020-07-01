use aws_scaletower::*;
use env_logger;
use chrono::prelude::*;

fn main() {
    env_logger::init();

    let end = Utc::now();
    let start = end - chrono::Duration::minutes(30);

    do_stuff(start, end);
}
