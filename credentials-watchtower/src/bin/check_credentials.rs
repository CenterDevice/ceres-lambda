use chrono::prelude::*;
use prettytable::{Cell, format, Row, Table};
use rusoto_core::Region;

use aws::AwsClientConfig;
use credentials_watchtower::check_credentials::{Credential, check_aws_credentials, CredentialCheck, check_duo_credentials};
use duo::DuoClient;
use std::env;

fn main() {
    env_logger::init();

    let api_host_name = env::var_os("DUO_API_HOST_NAME")
        .expect("Environment variable 'DUO_API_HOST_NAME' is not set.")
        .to_string_lossy().to_string();
    let integration_key = env::var_os("DUO_INTEGRATION_KEY")
        .expect("Environment variable 'DUO_INTEGRATION_KEY' is not set.")
        .to_string_lossy().to_string();
    let secret_key = env::var_os("DUO_SECRET_KEY")
        .expect("Environment variable 'DUO_SECRET_KEY' is not set.")
        .to_string_lossy().to_string();
    let duo_client = DuoClient::new(api_host_name, integration_key, secret_key).expect("Failed to create Duo client");
    let mut credentials = check_duo_credentials(&duo_client).expect("Failed to get Duo credentials");

    let aws_client_config = AwsClientConfig::with_region(Region::UsEast1).expect("Failed to create AWS client config");
    let aws_redentials = check_aws_credentials(&aws_client_config)
        .expect("failed to load credentials");
    credentials.extend(aws_redentials);

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
            CredentialCheck::Aws { credential } => credential_to_row("AWS", credential),
            CredentialCheck::Duo { credential } => credential_to_row("Duo", credential),
        };
        table.add_row(row);
    }

    table.printstd();
}

fn credential_to_row(service: &str, credential: &Credential) -> Row {
    let user = format!("{} ({})", credential.user_name, credential.id);
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