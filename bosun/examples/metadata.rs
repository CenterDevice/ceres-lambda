use bosun::{Bosun, BosunClient, Metadata};

use std::env;

fn main() {
    env_logger::init();

    let bosun_url = env::var_os("BOSUN_URL")
        .expect("Environment variable 'BOSUN_URL' is not set.")
        .to_string_lossy()
        .to_string();
    let bosun_username = env::var_os("BOSUN_USERNAME")
        .expect("Environment variable 'BOSUN_USERNAME' is not set.")
        .to_string_lossy()
        .to_string();
    let bosun_password = env::var_os("BOSUN_PASSWORD")
        .expect("Environment variable 'BOSUN_PASSWORD' is not set.")
        .to_string_lossy()
        .to_string();

    let mut bosun = BosunClient::new(&bosun_url, 5);
    bosun.set_basic_auth(bosun_username, Some(bosun_password));

    let metadata = Metadata::new(
        "aws.ec2.asg.scaling.event",
        "rate",
        "Scaling",
        "ASG up and down scaling event [-1 = down scaling, +1 = up scaling]",
    );

    let res = bosun.emit_metadata(&metadata);
    println!("Res: {:#?}", res);
}
