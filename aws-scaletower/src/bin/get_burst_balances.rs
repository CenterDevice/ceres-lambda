use aws::{
    auth::{create_provider_with_assuem_role, StsAssumeRoleSessionCredentialsProviderConfig},
    AwsClientConfig,
};
use aws_scaletower::*;
use chrono::prelude::*;
use rusoto_core::Region;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    /// AWS CLI credentials file
    #[structopt(
        short = "-c",
        long = "--credentials-file",
        name = "credentials-file",
        parse(from_os_str)
    )]
    credentials_file: Option<PathBuf>,

    /// AWS CLI credentials profile
    #[structopt(short = "-p", long = "--profile", name = "profile")]
    profile: String,

    /// Role ARN
    #[structopt(short = "-r", long = "--role-arn", name = "role-arn")]
    role_arn: String,
}

fn default_credentials_file() -> PathBuf {
    let mut path = dirs::home_dir().expect("Failed to determine home directory");
    path.push(".aws");
    path.push("credentials");
    path
}

fn main() {
    env_logger::init();
    let args = Opt::from_args();
    let credentials_file = args.credentials_file.unwrap_or_else(|| default_credentials_file());

    let sts_config = StsAssumeRoleSessionCredentialsProviderConfig::new(
        credentials_file,
        args.profile,
        args.role_arn,
        Region::EuCentral1,
    );
    let credential_provider =
        create_provider_with_assuem_role(sts_config).expect("Failed to create credential provider");
    let aws_client_config =
        AwsClientConfig::with_credentials_provider_and_region(credential_provider, Region::EuCentral1)
            .expect("Failed to create AWS client config");

    let end = Utc::now();
    let start = end - chrono::Duration::minutes(60);
    let forecasts = get_burst_balances(&aws_client_config, start, end, None).expect("Failed to get burst balances");

    for m in forecasts {
        match m {
            BurstBalance {
                timestamp: Some(ref timestamp),
                balance: Some(ref balance),
                ..
            } => {
                println!(
                    "Burst balance for vol {} attached to instance {} at {} is {}",
                    &m.volume_id, &m.instance_id, timestamp, balance
                )
            }
            _ => {
                println!(
                    "Burst balance for vol {} attached to instance {} not available",
                    &m.volume_id, &m.instance_id
                )
            }
        };
        if let BurstBalance {
            forecast: Some(ref forecast),
            ..
        } = m
        {
            let time_left = (*forecast - Utc::now()).num_minutes();
            println!(
                "   -> running out of burst balance at {:?} witch is in {:?} min",
                forecast, &time_left
            );
        }
    }
}
