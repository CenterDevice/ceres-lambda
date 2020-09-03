use chrono::{DateTime, Utc};
use failure::{err_msg, Error};

use aws::AwsClientConfig;
use aws::iam;
use aws::iam::AccessKeyLastUsed;
use duo::{Duo, DuoClient, DuoResponse};

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
    pub last_used: Option<DateTime<Utc>>,
}

impl From<iam::User> for Credential {
    fn from(user: iam::User) -> Self {
        Credential {
            id: user.user_id,
            user_name: user.user_name,
            credential: CredentialType::Password,
            last_used: user.password_last_used,
        }
    }
}

impl From<iam::AccessKeyLastUsed> for Credential {
    fn from(key: AccessKeyLastUsed) -> Self {
        Credential {
            id: key.access_key_id,
            user_name: key.user_name,
            credential: CredentialType::ApiKey,
            last_used: Some(key.last_used_date),
        }
    }
}

impl From<duo::User> for Credential {
    fn from(user: duo::User) -> Self {
        Credential {
            id: user.user_id.clone(),
            user_name: user.realname.clone().unwrap_or_else(|| "-".to_string()),
            credential: CredentialType::TwoFA,
            last_used: user.last_login.clone(),
        }
    }
}

#[derive(Debug)]
pub enum CredentialType {
    Password,
    ApiKey,
    TwoFA,
}

pub fn check_aws_credentials(
    aws_client_config: &AwsClientConfig,
) -> Result<Vec<CredentialCheck>, Error> {
    let users = iam::list_users(&aws_client_config)?;

    let mut credentials: Vec<CredentialCheck> = Vec::new();

    let user_credentials: Vec<Credential> = users.clone().into_iter()
        .map(Into::into)
        .collect();
    let _ = user_credentials.into_iter()
        .map(|x| CredentialCheck::Aws { credential: x })
        .map(|x| credentials.push(x)).collect::<Vec<_>>();

    let access_keys: Vec<_> = users.into_iter()
        .map(|x| x.user_name)
        .map(|x| {
            iam::list_access_keys_for_user(&aws_client_config, x)
        })
        .flatten()
        .flatten()
        .map(|x| iam::list_access_last_used(&aws_client_config, x.user_name.clone(), x.access_key_id))
        .filter(|x| x.is_ok())
        .flatten()
        .collect();
    let _ = access_keys.into_iter()
        .map(Into::into)
        .map(|x| CredentialCheck::Aws { credential: x })
        .map(|x| credentials.push(x)).collect::<Vec<_>>();

    Ok(credentials)
}

pub fn check_duo_credentials(
    duo_client: &DuoClient,
) -> Result<Vec<CredentialCheck>, Error> {
    let response = duo_client.get_users()?;
    match response {
        DuoResponse::Ok { response: users } => Ok(users
            .into_iter()
            .map(Into::into)
            .map(|x| CredentialCheck::Duo { credential: x })
            .collect()
        ),
        DuoResponse::Fail { code, message, message_detail } => {
            let msg = format!("failed to get Duo users (code: {}) because {}, {}", code, message, message_detail);
            Err(err_msg(msg))
        }
    }
}
