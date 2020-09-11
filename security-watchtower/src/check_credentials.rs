use chrono::{DateTime, Utc};
use failure::{err_msg, Error};

use aws::iam;
use aws::iam::{AccessKeyLastUsed, AccessKeyMetadataStatus};
use aws::AwsClientConfig;
use duo::{Duo, DuoClient, DuoResponse, UserStatus};

#[derive(Debug)]
pub enum CredentialCheck {
    Aws { credential: Credential },
    Duo { credential: Credential },
}

#[derive(Debug)]
pub struct Credential {
    pub id: String,
    pub user_name: String,
    pub credential: CredentialType,
    pub state: CredentialStatus,
    pub last_used: Option<DateTime<Utc>>,
    pub link_id: Option<String>,
}

impl From<iam::User> for Credential {
    fn from(user: iam::User) -> Self {
        Credential {
            id: user.user_id,
            user_name: user.user_name,
            credential: CredentialType::Password,
            state: CredentialStatus::Unknown,
            last_used: user.password_last_used,
            link_id: None,
        }
    }
}

impl From<iam::AccessKeyLastUsed> for Credential {
    fn from(key: AccessKeyLastUsed) -> Self {
        Credential {
            id: key.access_key_id,
            user_name: key.user_name,
            credential: CredentialType::ApiKey,
            state: match key.status {
                AccessKeyMetadataStatus::Active => CredentialStatus::Enabled,
                AccessKeyMetadataStatus::Inactive => CredentialStatus::Disabled,
            },
            last_used: Some(key.last_used_date),
            link_id: Some(key.user_id),
        }
    }
}

impl From<duo::User> for Credential {
    fn from(user: duo::User) -> Self {
        Credential {
            id: user.user_id.clone(),
            user_name: user.realname.clone().unwrap_or_else(|| "-".to_string()),
            credential: CredentialType::TwoFA,
            state: match user.status {
                UserStatus::Active | UserStatus::Bypass => CredentialStatus::Enabled,
                UserStatus::Disabled | UserStatus::LockedOut | UserStatus::PendingDeletion => {
                    CredentialStatus::Disabled
                }
            },
            last_used: user.last_login,
            link_id: None,
        }
    }
}

#[derive(Debug)]
pub enum CredentialType {
    Password,
    ApiKey,
    TwoFA,
}

#[derive(Debug)]
pub enum CredentialStatus {
    Enabled,
    Disabled,
    Unknown,
}

pub fn check_aws_credentials(aws_client_config: &AwsClientConfig) -> Result<Vec<CredentialCheck>, Error> {
    let users = iam::list_users(&aws_client_config)?;

    let mut credentials: Vec<CredentialCheck> = Vec::new();

    let user_credentials: Vec<Credential> = users.clone().into_iter().map(Into::into).collect();
    let _ = user_credentials
        .into_iter()
        .map(|x| CredentialCheck::Aws { credential: x })
        .map(|x| credentials.push(x))
        .collect::<Vec<_>>();

    let access_keys: Vec<_> = users
        .into_iter()
        .map(|x| iam::list_access_keys_for_user(&aws_client_config, x))
        .flatten()
        .flatten()
        .map(|x| iam::list_access_last_used(&aws_client_config, x))
        .filter(|x| x.is_ok())
        .flatten()
        .collect();
    let _ = access_keys
        .into_iter()
        .map(Into::into)
        .map(|x| CredentialCheck::Aws { credential: x })
        .map(|x| credentials.push(x))
        .collect::<Vec<_>>();

    Ok(credentials)
}

pub fn check_duo_credentials(duo_client: &DuoClient) -> Result<Vec<CredentialCheck>, Error> {
    let response = duo_client.get_users()?;
    match response {
        DuoResponse::Ok { response: users } => Ok(users
            .into_iter()
            .map(Into::into)
            .map(|x| CredentialCheck::Duo { credential: x })
            .collect()),
        DuoResponse::Fail {
            code,
            message,
            message_detail,
        } => {
            let msg = format!(
                "failed to get Duo users (code: {}) because {}, {}",
                code, message, message_detail
            );
            Err(err_msg(msg))
        }
    }
}
