use aws::{
    auth::{create_provider_with_assuem_role, StsAssumeRoleSessionCredentialsProviderConfig},
    AwsClientConfig, Filter,
};
use aws_scaletower::*;
use chrono::prelude::*;
use prettytable::{format, Cell, Row, Table};
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

    let filters = vec![
        Filter::new("instance-state-name", vec!["running"]),
        Filter::new("tag:Name", vec!["centerdevice-ec2-document_server*"]),
    ];

    let end = Utc::now();
    let start = end - chrono::Duration::minutes(60);
    let forecasts =
        get_burst_balances(&aws_client_config, start, end, None, filters).expect("Failed to get burst balances");

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(Row::new(vec![
        Cell::new("Volume Id"),
        Cell::new("Instance Id"),
        Cell::new("Burst Balance [%]"),
        Cell::new("Estimate Time of 0%"),
        Cell::new("Estimate Time until 0% [min]"),
        Cell::new("Timestamp"),
    ]));

    for m in forecasts {
        match m {
            BurstBalance {
                timestamp: Some(ref timestamp),
                balance: Some(ref balance),
                forecast: Some(ref forecast),
                ..
            } => {
                let time_left = (*forecast - Utc::now()).num_minutes();
                let row = Row::new(vec![
                    Cell::new(&m.volume_id),
                    Cell::new(&m.instance_id),
                    Cell::new(&balance.to_string()).style_spec("r"),
                    Cell::new(&forecast.to_string()),
                    Cell::new(&time_left.to_string()).style_spec("r"),
                    Cell::new(&timestamp.to_string()),
                ]);
                table.add_row(row);
            }
            BurstBalance {
                timestamp: Some(ref timestamp),
                balance: Some(ref balance),
                forecast: None,
                ..
            } => {
                let row = Row::new(vec![
                    Cell::new(&m.volume_id),
                    Cell::new(&m.instance_id),
                    Cell::new(&balance.to_string()).style_spec("r"),
                    Cell::new("-").style_spec("c"),
                    Cell::new("-").style_spec("c"),
                    Cell::new(&timestamp.to_string()),
                ]);
                table.add_row(row);
            }
            BurstBalance {
                timestamp: None,
                balance: None,
                forecast: None,
                ..
            } => {
                let row = Row::new(vec![
                    Cell::new(&m.volume_id),
                    Cell::new(&m.instance_id),
                    Cell::new("-").style_spec("c"),
                    Cell::new("-").style_spec("c"),
                    Cell::new("-").style_spec("c"),
                    Cell::new("-").style_spec("c"),
                ]);
                table.add_row(row);
            }
            _ => {
                eprintln!("Failed to print {:?}.", &m);
            }
        };
    }

    table.printstd();
}
