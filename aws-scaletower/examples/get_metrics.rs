use aws::AwsClientConfig;
use aws::auth::{StsAssumeRoleSessionCredentialsProviderConfig, create_provider_with_assuem_role};
use aws_scaletower::*;
use chrono::prelude::*;
use env_logger;
use rusoto_core::Region;

fn main() {
    env_logger::init();

    let sts_config = StsAssumeRoleSessionCredentialsProviderConfig::new(
        "/Users/lukas/.aws/credentials",
        "iam_centerdevice_my_person",
        "arn:aws:iam::737288212407:role/OrganizationAccountAccessRole",
        Region::EuCentral1
    );
    let credential_provider = create_provider_with_assuem_role(sts_config).expect("Failed to create credential provider");
    let aws_client_config = AwsClientConfig::with_credentials_provider_and_region(credential_provider, Region::EuCentral1).expect("Failed to create AWS client config");

    let end = Utc::now();
    let start = end - chrono::Duration::minutes(30);

    do_stuff(&aws_client_config, start, end, None);
}
