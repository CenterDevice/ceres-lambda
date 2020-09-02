use failure::Error;

use aws::AwsClientConfig;
use aws::iam;
use chrono::{DateTime, Utc};
use aws::iam::User;

#[derive(Debug)]
pub enum CredentialCheck {
    Aws { credential: AwsCredential },
}

#[derive(Debug)]
pub struct AwsCredential {
    pub user_id: String,
    pub user_name: String,
    pub credential: CredentialType,
    pub last_used: Option<DateTime<Utc>>,
}

impl From<iam::User> for AwsCredential {
    fn from(user: iam::User) -> Self {
        AwsCredential {
            user_id: user.user_id,
            user_name: user.user_name,
            credential: CredentialType::Password,
            last_used: user.password_last_used,
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

    user_credentials.into_iter()
        .map(|x| CredentialCheck::Aws { credential: x})
        .map(|x| credentials.push(x)).collect::<Vec<_>>();

    Ok(credentials)
}