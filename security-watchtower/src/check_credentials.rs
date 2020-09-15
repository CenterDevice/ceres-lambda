use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use failure::{err_msg, Error};

use aws::iam;
use aws::iam::{AccessKeyLastUsed, AccessKeyMetadataStatus};
use aws::AwsClientConfig;
use duo::{Duo, DuoClient, DuoResponse, UserStatus};

#[derive(Debug, Clone, Copy)]
pub enum Service {
    Aws,
    Duo,
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub service: Service,
    pub id: String,
    pub user_name: String,
    pub credential: CredentialType,
    pub state: CredentialStatus,
    pub last_used: Option<DateTime<Utc>>,
    pub linked_id: Option<String>,
}

impl Credential {
}

impl From<iam::User> for Credential {
    fn from(user: iam::User) -> Self {
        Credential {
            service: Service::Aws,
            id: user.user_id,
            user_name: user.user_name,
            credential: CredentialType::Password,
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
            credential: CredentialType::ApiKey,
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
            credential: CredentialType::TwoFA,
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
pub enum CredentialType {
    Password,
    ApiKey,
    TwoFA,
}

#[derive(Debug, Clone, Copy)]
pub enum CredentialStatus {
    Enabled,
    Disabled,
    Unknown,
}

pub fn check_aws_credentials(aws_client_config: &AwsClientConfig) -> Result<Vec<Credential>, Error> {
    let users = iam::list_users(&aws_client_config)?;

    let mut credentials: Vec<Credential> = Vec::new();

    let user_credentials: Vec<Credential> = users.clone().into_iter().map(Into::into).collect();
    credentials.append(&mut user_credentials.clone());

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
        .map(|x| credentials.push(x))
        .collect::<Vec<_>>();

    Ok(credentials)
}

pub fn check_duo_credentials(duo_client: &DuoClient) -> Result<Vec<Credential>, Error> {
    let response = duo_client.get_users()?;
    match response {
        DuoResponse::Ok { response: users } => Ok(users
            .into_iter()
            .map(Into::into)
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

#[derive(Debug)]
pub struct InactiveSpec {
    pub notification_offset: i64,
    pub disable_threshold_days: i64,
    pub delete_threshold_days: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InactiveAction {
    Keep,
    NotifyDisable,
    Disable,
    NotifyDelete,
    Delete,
    Unknown,
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
            if since > spec.delete_threshold_days - spec.notification_offset {
                return InactiveAction::NotifyDelete;
            }
            if since > spec.disable_threshold_days {
                return InactiveAction::Disable;
            }
            if since > spec.disable_threshold_days - spec.notification_offset {
                return InactiveAction::NotifyDisable;
            }

            InactiveAction::Keep
        } else {
            InactiveAction::Keep
        }
    }
}

#[derive(Debug)]
pub struct InactiveCredential {
    pub credential: Credential,
    pub action: InactiveAction,
}

pub trait IdentifyInactive {
    fn identify_inactive(self, spec: &InactiveSpec) -> Vec<InactiveCredential>;
}

impl IdentifyInactive for Vec<Credential> {
    fn identify_inactive(self, spec: &InactiveSpec) -> Vec<InactiveCredential> {
        let mut matrix: HashMap<String, InactiveAction> = HashMap::new();

        for c in &self {
            let action = c.inactive_action(spec);
            matrix.insert(c.id.to_string(), action);
            if let Some(ref link_id) = c.linked_id {
                matrix.insert(link_id.to_string(), action);
            }
        }

        let inactive_ids: HashSet<&str> = matrix
            .iter()
            .filter(|(_, action)| !action.keep())
            .map(|(id, _)| id.as_str())
            .collect();

        self.into_iter()
            .filter(|c| inactive_ids.contains(c.id.as_str()))
            .map(|c| {
                let id = c.id.to_string();
                InactiveCredential {
                    credential: c,
                    action: matrix[&id], // Safe, because id comes from self
                }
            })
            .collect()
    }
}
