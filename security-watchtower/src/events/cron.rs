use chrono::Utc;
use failure::{Error, Fail};
use lambda_runtime::Context;
use log::{debug, error, info, trace};
use rusoto_sts::{AssumeRoleError, AssumeRoleRequest, Sts, StsClient};
use serde_derive::{Deserialize, Serialize};

use aws::AwsClientConfig;
use bosun::{Bosun, Datum, Tags};
use duo::DuoClient;

use crate::check_credentials::{
    check_aws_credentials, check_duo_credentials, ApplyInactiveAction, Credential, IdentifyInactive, InactiveAction,
    InactiveSpec,
};
use crate::config::{CredentialsConfig, FunctionConfig};
use crate::events::HandleResult;
use crate::metrics;
use aws::auth::create_provider_with_static_provider;
use lambda::error::LambdaError;
use rusoto_core::credential::StaticProvider;
use rusoto_core::Region;

// cf. https://docs.aws.amazon.com/lambda/latest/dg/services-cloudwatchevents.html
// {
//   "account": "123456789012",
//   "region": "us-east-2",
//   "detail": {},
//   "detail-type": "Scheduled Event",
//   "source": "aws.events",
//   "time": "2019-03-01T01:23:45Z",
//   "id": "cdc73f9d-aea9-11e3-9d5a-835b769c0d9c",
//   "resources": [
//     "arn:aws:events:us-east-1:123456789012:rule/my-schedule"
//   ]
// }
#[derive(Debug, Deserialize)]
pub struct ScheduledEvent {
    pub account: String,
    pub region: String,
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub time: String,
    pub id: String,
    pub resources: Vec<String>,
}

pub fn handle<T: Bosun>(
    _: &AwsClientConfig,
    _: &Context,
    config: &FunctionConfig,
    bosun: &T,
) -> Result<HandleResult, Error> {
    info!("Received Scheduled Event.");

    let duo_client = DuoClient::new(
        &config.duo.api_host_name,
        &config.duo.integration_key,
        &config.duo.secret_key,
    )?;

    let iam_role_arn =
        std::env::var("CD_IAM_ROLE_ARN").map_err(|e| e.context(LambdaError::FailedEnvVar("CD_IAM_ROLE_ARN")))?;
    let iam_aws_client_config = assume_iam_role(iam_role_arn)?;

    let credentials = get_credentials(&iam_aws_client_config, &duo_client, &config.credentials, bosun)?;

    let handle_result = HandleResult::Cron { credentials };

    Ok(handle_result)
}

fn assume_iam_role(iam_role_arn: String) -> Result<AwsClientConfig, Error> {
    let sts = StsClient::new(Region::UsEast1);
    let res = sts
        .assume_role(AssumeRoleRequest {
            role_arn: iam_role_arn,
            role_session_name: "Lambda2Iam".to_string(),
            ..Default::default()
        })
        .sync();
    if let Err(AssumeRoleError::Unknown(ref buf)) = res {
            let str = String::from_utf8_lossy(&buf.body);
            error!("Error: {}", str);
    }
    let credentials = res?.credentials.unwrap(); // Safe unwrap, because the call was successfull
    let static_provider = StaticProvider::new(
        credentials.access_key_id,
        credentials.secret_access_key,
        Some(credentials.session_token),
        None,
    );

    let credential_provider = create_provider_with_static_provider(static_provider)?;
    let iam_aws_client_config =
        AwsClientConfig::with_credentials_provider_and_region(credential_provider, Region::UsEast1)?;

    Ok(iam_aws_client_config)
}

#[derive(Debug, Serialize)]
pub struct CredentialStats {
    pub total: usize,
    pub kept: usize,
    pub disabled: usize,
    pub deleted: usize,
    pub failed: usize,
}

pub fn get_credentials<T: Bosun>(
    aws_client_config: &AwsClientConfig,
    duo_client: &DuoClient,
    config: &CredentialsConfig,
    bosun: &T,
) -> Result<CredentialStats, Error> {
    debug!("Config: {:?}", config);
    let mut credentials = check_duo_credentials(&duo_client).expect("Failed to get Duo credentials");
    debug!("Retrieved DUO credentials: {}", credentials.len());

    let aws_credentials = check_aws_credentials(&aws_client_config).expect("failed to load credentials");
    debug!("Retrieved AWS credentials: {}", aws_credentials.len());
    credentials.extend(aws_credentials);

    bosun_emit_credential_last_used(bosun, &credentials)?;

    debug!("Checking for inactive credentials");
    let inactive_spec = InactiveSpec {
        disable_threshold_days: config.disable_threshold_days,
        delete_threshold_days: config.delete_threshold_days,
    };
    let inactives = credentials.identify_inactive(&inactive_spec);
    if log::max_level() >= log::Level::Info {
        for ic in &inactives {
            info!(
                "Credential {}:{} for user '{}' with id {} is inactive. Appropriate action would be to {} it.",
                ic.credential.service, ic.credential.kind, ic.credential.user_name, ic.credential.id, ic.action,
            );
        }
    }

    debug!("Applying actions for inactive credentials");
    let mut stats = CredentialStats {
        total: credentials.len(),
        kept: credentials.len() - inactives.len(),
        disabled: 0,
        deleted: 0,
        failed: 0,
    };
    if !inactives.is_empty() {
        for ic in &inactives {
            let whitelist_key = ic.credential.whitelist_key();
            trace!("Whitelist key: {}", whitelist_key);
            if config.whitelist.contains(&whitelist_key) {
                debug!(
                    "Ignoring '{}:{}:{}/{}' because this credential is whitelisted.",
                    ic.credential.service, ic.credential.kind, ic.credential.user_name, ic.credential.id
                );
                continue;
            }
            if config.actions_enabled {
                debug!(
                    "Applying {} to '{}:{}:{}/{}'",
                    ic.action, ic.credential.service, ic.credential.kind, ic.credential.user_name, ic.credential.id
                );
                let res = ic.apply(aws_client_config, duo_client);
                info!(
                    "Applied {} to '{}:{}:{}/{}': success = {}",
                    ic.action,
                    ic.credential.service,
                    ic.credential.kind,
                    ic.credential.user_name,
                    ic.credential.id,
                    res.is_ok()
                );

                if res.is_err() {
                    stats.failed += 1;
                } else {
                    match ic.action {
                        InactiveAction::Disable => stats.disabled += 1,
                        InactiveAction::Delete => stats.deleted += 1,
                        _ => {}
                    }
                }
            } else {
                ic.dry_run(aws_client_config, duo_client)?;
            }
        }
    } else {
        info!("No inactive credentials found, nothing to do.")
    }

    Ok(stats)
}

fn bosun_emit_credential_last_used<T: Bosun>(bosun: &T, credentials: &[Credential]) -> Result<(), Error> {
    for credential in credentials {
        let mut tags = Tags::new();
        tags.insert("service".to_string(), credential.service.to_string());
        tags.insert("kind".to_string(), credential.kind.to_string());
        tags.insert("user_name".to_string(), credential.user_name.replace(" ", "_"));

        let value = if let Some(last_used) = credential.last_used {
            (Utc::now() - last_used).num_days()
        } else {
            -1
        }
        .to_string();

        let datum = Datum::now(metrics::CREDENTIAL_LAST_USAGE, &value, &tags);
        bosun.emit_datum(&datum)?;
    }

    Ok(())
}

trait WhiteListKey {
    fn whitelist_key(&self) -> String;
}

impl WhiteListKey for Credential {
    fn whitelist_key(&self) -> String {
        format!("{}:{}:{}", self.service, self.kind, self.id)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;
    use spectral::prelude::*;

    use super::*;

    fn setup() {
        testing::setup();
    }

    #[test]
    fn parse_scheduled_event_from_json() {
        setup();

        let json = json!(
            {
                "account": "959123467016",
                "detail": {},
                "detail-type": "Scheduled Event",
                "id": "46cc8812-1000-45bc-50f8-a42d3335eeda",
                "region": "eu-central-1",
                "resources": [
                    "arn:aws:events:eu-central-1:959479900016:rule/scheduled_events_security-watchtower"
                ],
                "source": "aws.events",
                "time": "2020-08-31T16:51:48Z",
                "version": "0"
            }
        );

        let event: Result<ScheduledEvent, _> = serde_json::from_value(json);

        info!("event = {:?}", event);

        assert_that(&event).is_ok();
    }
}
