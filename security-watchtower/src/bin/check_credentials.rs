use std::env;

use chrono::prelude::*;
use prettytable::{format, Cell, Row, Table};
use rusoto_core::Region;

use aws::AwsClientConfig;
use duo::DuoClient;
use security_watchtower::check_credentials::{
    check_aws_credentials, check_duo_credentials, Credential, IdentifyInactive, InactiveCredential, InactiveSpec,
};

fn main() {
    env_logger::init();

    let api_host_name = env::var_os("DUO_API_HOST_NAME")
        .expect("Environment variable 'DUO_API_HOST_NAME' is not set.")
        .to_string_lossy()
        .to_string();
    let integration_key = env::var_os("DUO_INTEGRATION_KEY")
        .expect("Environment variable 'DUO_INTEGRATION_KEY' is not set.")
        .to_string_lossy()
        .to_string();
    let secret_key = env::var_os("DUO_SECRET_KEY")
        .expect("Environment variable 'DUO_SECRET_KEY' is not set.")
        .to_string_lossy()
        .to_string();
    let duo_client = DuoClient::new(api_host_name, integration_key, secret_key).expect("Failed to create Duo client");
    let aws_client_config = AwsClientConfig::with_region(Region::UsEast1).expect("Failed to create AWS client config");

    let mut credentials = check_duo_credentials(&duo_client).expect("Failed to get Duo credentials");
    let aws_credentials = check_aws_credentials(&aws_client_config).expect("failed to load credentials");
    credentials.extend(aws_credentials);

    print_credentials(&credentials);

    let inactive_spec = InactiveSpec {
        disable_threshold_days: 60,
        delete_threshold_days: 180,
    };

    let inactives = credentials.identify_inactive(&inactive_spec);
    print_inactives(&inactives);
}

fn print_credentials(credentials: &[Credential]) {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(Row::new(vec![
        Cell::new("Service"),
        Cell::new("User"),
        Cell::new("Id"),
        Cell::new("Linked Id"),
        Cell::new("Credential Type"),
        Cell::new("State"),
        Cell::new("Last Time Used"),
        Cell::new("Last Usage [days]"),
        Cell::new("> 2 Months"),
        Cell::new("> 6 Months"),
    ]));

    for c in credentials {
        let row = credential_to_row(c);
        table.add_row(row);
    }

    table.printstd();
}

fn credential_to_row(credential: &Credential) -> Row {
    let service = format!("{:?}", credential.service);
    let user_name = &credential.user_name;
    let id = &credential.id;
    let link_id = credential.linked_id.as_deref().unwrap_or_else(|| "-");
    let credential_type = format!("{:?}", credential.kind);
    let credential_state = format!("{:?}", credential.state);
    let last_time_used = credential
        .last_used
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
        Cell::new(&service),
        Cell::new(&user_name),
        Cell::new(&id),
        Cell::new(&link_id).style_spec("c"),
        Cell::new(&credential_type),
        Cell::new(&credential_state),
        Cell::new(&last_time_used).style_spec("c"),
        Cell::new(&last_usage).style_spec("r"),
        Cell::new(&last_usage_is_2_months).style_spec("c"),
        Cell::new(&last_usage_is_6_months).style_spec("c"),
    ])
}

fn print_inactives(credentials: &[InactiveCredential]) {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(Row::new(vec![
        Cell::new("Service"),
        Cell::new("User"),
        Cell::new("Id"),
        Cell::new("Credential Type"),
        Cell::new("State"),
        Cell::new("Action"),
    ]));

    for ic in credentials {
        let row = inactivity_to_row(ic);
        table.add_row(row);
    }

    table.printstd();
}

fn inactivity_to_row(ic: &InactiveCredential) -> Row {
    let service = format!("{:?}", ic.credential.service);
    let user_name = &ic.credential.user_name;
    let id = &ic.credential.id;
    let credential_type = format!("{:?}", ic.credential.kind);
    let credential_state = format!("{:?}", ic.credential.state);
    let action = format!("{:?}", ic.action);

    Row::new(vec![
        Cell::new(&service),
        Cell::new(&user_name),
        Cell::new(&id),
        Cell::new(&credential_type),
        Cell::new(&credential_state),
        Cell::new(&action),
    ])
}
