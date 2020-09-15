use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use failure::{err_msg, Error};
use log::info;

use aws::iam;
use aws::iam::{AccessKeyLastUsed, AccessKeyMetadataStatus};
use aws::AwsClientConfig;
use duo::{Duo, DuoClient, DuoResponse, UserStatus};

#[derive(Debug, Clone, Copy)]
pub enum Service {
    Aws,
    Duo,
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Service::Aws => f.write_str("aws"),
            Service::Duo => f.write_str("duo"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub service: Service,
    pub id: String,
    pub user_name: String,
    pub kind: CredentialKind,
    pub state: CredentialStatus,
    pub last_used: Option<DateTime<Utc>>,
    pub linked_id: Option<String>,
}

impl Credential {
    pub fn is_aws(&self) -> bool {
        match self.service {
            Service::Aws => true,
            _ => false,
        }
    }

    pub fn is_duo(&self) -> bool {
        match self.service {
            Service::Duo => true,
            _ => false,
        }
    }

    pub fn is_api_key(&self) -> bool {
        match self.kind {
            CredentialKind::ApiKey => true,
            _ => false,
        }
    }

    pub fn is_password(&self) -> bool {
        match self.kind {
            CredentialKind::Password => true,
            _ => false,
        }
    }

    pub fn is_two_fa(&self) -> bool {
        match self.kind {
            CredentialKind::TwoFA => true,
            _ => false,
        }
    }
}

impl From<iam::User> for Credential {
    fn from(user: iam::User) -> Self {
        Credential {
            service: Service::Aws,
            id: user.user_id,
            user_name: user.user_name,
            kind: CredentialKind::Password,
            state: CredentialStatus::Unknown,
            last_used: user.password_last_used,
            linked_id: None,
        }
    }
}

impl From<iam::AccessKeyLastUsed> for Credential {
    fn from(key: AccessKeyLastUsed) -> Self {
        Credential {
            service: Service::Aws,
            id: key.access_key_id,
            user_name: key.user_name,
            kind: CredentialKind::ApiKey,
            state: match key.status {
                AccessKeyMetadataStatus::Active => CredentialStatus::Enabled,
                AccessKeyMetadataStatus::Inactive => CredentialStatus::Disabled,
            },
            last_used: Some(key.last_used_date),
            linked_id: Some(key.user_id),
        }
    }
}

impl From<duo::User> for Credential {
    fn from(user: duo::User) -> Self {
        Credential {
            service: Service::Duo,
            id: user.user_id.clone(),
            user_name: user.realname.clone().unwrap_or_else(|| "-".to_string()),
            kind: CredentialKind::TwoFA,
            state: match user.status {
                UserStatus::Active | UserStatus::Bypass => CredentialStatus::Enabled,
                UserStatus::Disabled | UserStatus::LockedOut | UserStatus::PendingDeletion => {
                    CredentialStatus::Disabled
                }
            },
            last_used: user.last_login,
            linked_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CredentialKind {
    Password,
    ApiKey,
    TwoFA,
}

impl fmt::Display for CredentialKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialKind::ApiKey => f.write_str("api_key"),
            CredentialKind::Password => f.write_str("password"),
            CredentialKind::TwoFA => f.write_str("tfa"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CredentialStatus {
    Enabled,
    Disabled,
    Unknown,
}

pub fn check_aws_credentials(aws_client_config: &AwsClientConfig) -> Result<Vec<Credential>, Error> {
    let users = iam::list_users(&aws_client_config)?;

    let mut credentials: Vec<Credential> = users.clone().into_iter().map(Into::into).collect();

    let access_keys: Vec<_> = users
        .iter()
        .map(|x| iam::list_access_keys_for_user(&aws_client_config, x.clone()))
        .flatten()
        .flatten()
        .map(|x| iam::list_access_last_used(&aws_client_config, x))
        .filter(|x| x.is_ok())
        .flatten()
        .collect();
    let _ = access_keys
        .into_iter()
        .map(Into::into)
        .map(|x| credentials.push(x))
        .collect::<Vec<_>>();

    Ok(credentials)
}

pub fn check_duo_credentials(duo_client: &DuoClient) -> Result<Vec<Credential>, Error> {
    let response = duo_client.get_users()?;
    match response {
        DuoResponse::Ok { response: users } => Ok(users.into_iter().map(Into::into).collect()),
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

#[derive(Debug)]
pub struct InactiveSpec {
    pub disable_threshold_days: i64,
    pub delete_threshold_days: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InactiveAction {
    Keep = 1,
    Disable = 2,
    Delete = 3,
}

impl fmt::Display for InactiveAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InactiveAction::Keep => f.write_str("keep"),
            InactiveAction::Disable => f.write_str("disable"),
            InactiveAction::Delete => f.write_str("delete"),
        }
    }
}

impl InactiveAction {
    pub fn keep(&self) -> bool {
        *self == InactiveAction::Keep
    }
}

pub trait Inactive {
    fn inactive_action(&self, spec: &InactiveSpec) -> InactiveAction;
}

impl Inactive for Credential {
    fn inactive_action(&self, spec: &InactiveSpec) -> InactiveAction {
        if let Some(ref last_used) = self.last_used {
            let since = (Utc::now() - *last_used).num_days();

            if since > spec.delete_threshold_days {
                return InactiveAction::Delete;
            }
            if since > spec.disable_threshold_days {
                return InactiveAction::Disable;
            }

            InactiveAction::Keep
        } else {
            InactiveAction::Keep
        }
    }
}

#[derive(Debug)]
pub struct InactiveCredential<'a> {
    pub credential: &'a Credential,
    pub action: InactiveAction,
}

pub trait IdentifyInactive {
    fn identify_inactive(&self, spec: &InactiveSpec) -> Vec<InactiveCredential>;
}

impl IdentifyInactive for Vec<Credential> {
    fn identify_inactive(&self, spec: &InactiveSpec) -> Vec<InactiveCredential> {
        let mut matrix: HashMap<&str, InactiveAction> = HashMap::new();
        let mut result = Vec::new();

        // Check inactivity for Duo TFA
        for credential in self.iter().filter(|x| x.is_duo()) {
            let action = credential.inactive_action(spec);
            if !action.keep() {
                result.push(InactiveCredential { credential, action })
            }
        }

        // Check inactivity for AWS API keys
        for credential in self.iter().filter(|x| x.is_aws() && x.is_api_key()) {
            let action = credential.inactive_action(spec);
            if !action.keep() {
                result.push(InactiveCredential { credential, action })
            }
            matrix.insert(credential.id.as_str(), action);
        }

        // Check inactivity for AWS Password. In case a password is inactive, compare action with results from API keys
        // If API key inactivity has positive deviation, use key action. Positive deviation is defined
        // as "key has been used after password" ^= "account is more active than it seems by the password only.
        // In this way, we won't disable or even delete an AWS IAM Account just because somebody only uses
        // the API, e.g., Terraform, AWS CLI etc.
        for credential in self.iter().filter(|x| x.is_aws() && x.is_password()) {
            let action = credential.inactive_action(spec);
            let linked_action = matrix.get(credential.id.as_str());

            use InactiveAction::*;
            let action = match (action, linked_action) {
                (Keep, _) | (_, Some(Keep)) => break,
                (Disable, None) | (Disable, Some(Disable)) | (Disable, Some(Delete)) => Disable,
                (Delete, Some(Disable)) => Disable,
                (Delete, None) | (Delete, Some(Delete)) => Delete,
            };

            result.push(InactiveCredential { credential, action })
        }

        result
    }
}

pub trait ApplyInactiveAction {
    fn apply(&self, aws: &AwsClientConfig, duo: &DuoClient) -> Result<(), Error>;
    fn dry_run(&self, aws: &AwsClientConfig, duo: &DuoClient) -> Result<(), Error>;
}

impl<'a> ApplyInactiveAction for InactiveCredential<'a> {
    fn apply(&self, aws: &AwsClientConfig, duo: &DuoClient) -> Result<(), Error> {
        use CredentialKind::*;
        use InactiveAction::*;
        use Service::*;

        let id = self.credential.id.clone();
        let user_name = self.credential.user_name.clone();
        match (self.credential.service, self.credential.kind, self.action) {
            (Aws, ApiKey, Disable) => iam::disable_access_key(aws, id, user_name),
            (Aws, ApiKey, Delete) => iam::delete_access_key(aws, id, user_name),
            (Aws, Password, Disable) => iam::disable_user(aws, user_name),
            (Aws, Password, Delete) => iam::delete_user(aws, user_name),
            (Duo, TwoFA, Disable) => duo.disable_user(id)?.as_result(),
            (Duo, TwoFA, Delete) => duo.delete_user(id)?.as_result(),
            _ => Ok(()),
        }
    }

    fn dry_run(&self, _: &AwsClientConfig, _: &DuoClient) -> Result<(), Error> {
        use CredentialKind::*;
        use InactiveAction::*;
        use Service::*;

        let id = self.credential.id.clone();
        let user_name = self.credential.user_name.clone();
        match (self.credential.service, self.credential.kind, self.action) {
            (Aws, ApiKey, Disable) => info!("Would have disabled AWS access key {} for {}", id, user_name),
            (Aws, ApiKey, Delete) => info!("Would have deleted AWS access key {} for {}", id, user_name),
            (Aws, Password, Disable) => info!("Would have disabled AWS user {}", user_name),
            (Aws, Password, Delete) => info!("Would have deleted AWS user {}", user_name),
            (Duo, TwoFA, Disable) => info!("Would have disabled DUO user {}", user_name),
            (Duo, TwoFA, Delete) => info!("Would have deleted DUO user {}", user_name),
            _ => {}
        }

        Ok(())
    }
}

trait ToUnitResult {
    fn as_result(self) -> Result<(), Error>;
}

impl<T> ToUnitResult for DuoResponse<T> {
    fn as_result(self) -> Result<(), Error> {
        match self {
            DuoResponse::Ok { .. } => Ok(()),
            DuoResponse::Fail {
                code,
                message,
                message_detail,
            } => {
                let msg = format!(
                    "Duo call failed (code: {}) because {}, {}",
                    code, message, message_detail
                );
                Err(err_msg(msg))
            }
        }
    }
}
