use failure::Error;

use aws::AwsClientConfig;
use aws::iam;
use chrono::{DateTime, Utc};
use aws::iam::AccessKeyLastUsed;

#[derive(Debug)]
pub enum CredentialCheck {
    Aws { credential: AwsCredential },
}

#[derive(Debug)]
pub struct AwsCredential {
    pub id: String,
    pub user_name: String,
    pub credential: CredentialType,
    pub last_used: Option<DateTime<Utc>>,
}

impl From<iam::User> for AwsCredential {
    fn from(user: iam::User) -> Self {
        AwsCredential {
            id: user.user_id,
            user_name: user.user_name,
            credential: CredentialType::Password,
            last_used: user.password_last_used,
        }
    }
}

impl From<iam::AccessKeyLastUsed> for AwsCredential {
    fn from(key: AccessKeyLastUsed) -> Self {
        AwsCredential {
            id: key.access_key_id,
            user_name: key.user_name,
            credential: CredentialType::ApiKey,
            last_used: Some(key.last_used_date),
        }
    }
}

#[derive(Debug)]
pub enum CredentialType {
    Password,
    ApiKey,
}

pub fn check_aws_credentials(
    aws_client_config: &AwsClientConfig,
) -> Result<Vec<CredentialCheck>, Error> {

    let users = iam::list_users(&aws_client_config)?;

    let mut credentials: Vec<CredentialCheck> = Vec::new();

    let user_credentials: Vec<AwsCredential> = users.clone().into_iter()
        .map(Into::into)
        .collect();
    let _ = user_credentials.into_iter()
        .map(|x| CredentialCheck::Aws { credential: x})
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
        .map(|x| CredentialCheck::Aws {credential: x})
        .map(|x| credentials.push(x)).collect::<Vec<_>>();

    Ok(credentials)
}
