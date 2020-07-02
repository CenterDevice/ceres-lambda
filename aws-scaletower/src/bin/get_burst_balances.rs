use aws::{
    auth::{create_provider_with_assuem_role, StsAssumeRoleSessionCredentialsProviderConfig},
    AwsClientConfig,
};
use aws_scaletower::*;
use chrono::prelude::*;
use rusoto_core::Region;

fn main() {
    env_logger::init();

    let sts_config = StsAssumeRoleSessionCredentialsProviderConfig::new(
        "/Users/lukas/.aws/credentials",
        "iam_centerdevice_my_person",
        "arn:aws:iam::737288212407:role/OrganizationAccountAccessRole",
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
