use chrono::prelude::*;
use prettytable::{Cell, format, Row, Table};
use rusoto_core::Region;

use aws::AwsClientConfig;
use credentials_watchtower::check_credentials::{AwsCredential, check_aws_credentials, CredentialCheck};

fn main() {
    env_logger::init();
    let aws_client_config = AwsClientConfig::with_region(Region::UsEast1).expect("Failed to create AWS client config");

    let credentials = check_aws_credentials(&aws_client_config)
        .expect("failed to load credentials");

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(Row::new(vec![
        Cell::new("Service"),
        Cell::new("User"),
        Cell::new("Credential Type"),
        Cell::new("Last Time Used"),
        Cell::new("Last Usage [days]"),
        Cell::new("> 2 Months"),
        Cell::new("> 6 Months"),
    ]));

    for c in &credentials {
        let row = match c {
            CredentialCheck::Aws { credential } => aws_credential_to_row(credential),
        };
        table.add_row(row);
    }

    table.printstd();
}

fn aws_credential_to_row(credential: &AwsCredential) -> Row {
    let service = "AWS";
    let user = format!("{} ({})", credential.user_name, credential.user_name);
    let credential_type = format!("{:?}", credential.credential);
    let last_time_used = credential.last_used
        .map(|x| x.to_rfc3339())
        .unwrap_or_else(|| "-".to_string());
    let (last_usage, last_usage_is_2_months, last_usage_is_6_months) = if let Some(last_used) = credential.last_used {
        let since = Utc::now() - last_used;
        (
            format!("{}", since.num_days()),
            format!("{}", since.num_weeks() > 8),
            format!("{}", since.num_weeks() > 24),
        )
    } else {
        ("-".to_string(), "-".to_string(), "-".to_string())
    };

    Row::new(vec![
        Cell::new(service),
        Cell::new(&user),
        Cell::new(&credential_type),
        Cell::new(&last_time_used),
        Cell::new(&last_usage).style_spec("r"),
        Cell::new(&last_usage_is_2_months).style_spec("c"),
        Cell::new(&last_usage_is_6_months).style_spec("c"),
    ])
}