use crate::AwsClientConfig;
use chrono::{DateTime, Utc};
use failure::{err_msg, Error};
use log::{debug, warn};
use rusoto_iam::{GetAccessKeyLastUsedRequest, Iam, IamClient, ListAccessKeysRequest, ListUsersRequest};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct User {
    pub password_last_used: Option<DateTime<Utc>>,
    pub user_id: String,
    pub user_name: String,
    pub path: String,
}

impl From<rusoto_iam::User> for User {
    fn from(user: rusoto_iam::User) -> Self {
        let password_last_used = user
            .password_last_used
            .and_then(|x| DateTime::parse_from_rfc3339(&x).ok())
            .map(|x| x.with_timezone(&Utc));

        User {
            password_last_used,
            user_id: user.user_id,
            user_name: user.user_name,
            path: user.path,
        }
    }
}

pub fn list_users(aws_client_config: &AwsClientConfig) -> Result<Vec<User>, Error> {
    debug!("List users");

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let iam = IamClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let request = ListUsersRequest {
        marker: None,
        max_items: Some(100),
        path_prefix: None,
    };
    let res = iam.list_users(request).sync();
    debug!("Finished list user request; success={}.", res.is_ok());
    let res = res.expect("failed to list users");

    if log::max_level() >= log::Level::Warn && res.is_truncated.is_some() && res.is_truncated.unwrap() {
        warn!("List users: Result is truncated.");
    }

    let res: Vec<User> = res.users.into_iter().map(Into::into).collect();

    Ok(res)
}

#[derive(Debug, Clone)]
pub struct AccessKeyMetadata {
    pub access_key_id: String,
    pub create_date: DateTime<Utc>,
    pub status: AccessKeyMetadataStatus,
    pub user_name: String,
    pub user_id: String,
}

impl AccessKeyMetadata {
    fn try_from(user_id: String, value: rusoto_iam::AccessKeyMetadata) -> Result<Self, Error> {
        let access_key_id = value.access_key_id.ok_or_else(|| err_msg("no access key provided"))?;
        let create_date = value
            .create_date
            .ok_or_else(|| err_msg("no create date provided"))
            .and_then(|x| DateTime::parse_from_rfc3339(&x).map_err(|_| err_msg("failed to parse create date")))
            .map(|x| x.with_timezone(&Utc))?;
        let status = value
            .status
            .ok_or_else(|| err_msg("no status provided"))
            .and_then(|x| AccessKeyMetadataStatus::from_str(&x))?;
        let user_name = value.user_name.ok_or_else(|| err_msg("no user name provided"))?;

        Ok(AccessKeyMetadata {
            access_key_id,
            create_date,
            status,
            user_name,
            user_id,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AccessKeyMetadataStatus {
    Active,
    Inactive,
}

impl FromStr for AccessKeyMetadataStatus {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(AccessKeyMetadataStatus::Active),
            "inactive" => Ok(AccessKeyMetadataStatus::Inactive),
            _ => Err(err_msg("failed to parse access key status")),
        }
    }
}

pub fn list_access_keys_for_user(
    aws_client_config: &AwsClientConfig,
    user: User,
) -> Result<Vec<AccessKeyMetadata>, Error> {
    debug!("List access keys for user '{}'", &user.user_name);

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let iam = IamClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let request = ListAccessKeysRequest {
        marker: None,
        max_items: Some(100),
        user_name: Some(user.user_name.clone()),
    };
    let res = iam.list_access_keys(request).sync();
    debug!(
        "Finished list access keys request for user '{}'; success={}.",
        &user.user_name,
        res.is_ok()
    );
    let res = res?;

    if log::max_level() >= log::Level::Warn && res.is_truncated.is_some() && res.is_truncated.unwrap() {
        warn!("List users: Result is truncated.");
    }

    let res: Vec<Result<AccessKeyMetadata, Error>> =
        res.access_key_metadata.into_iter().map(|x| AccessKeyMetadata::try_from(user.user_id.clone(), x)).collect();
    let res: Result<Vec<AccessKeyMetadata>, Error> = res.into_iter().collect();

    res
}

#[derive(Debug, Clone)]
pub struct AccessKeyLastUsed {
    pub user_name: String,
    pub user_id: String,
    pub access_key_id: String,
    pub status: AccessKeyMetadataStatus,
    pub last_used_date: DateTime<Utc>,
    pub region: String,
    pub service_name: String,
}

impl AccessKeyLastUsed {
    fn try_from(access_key: AccessKeyMetadata, value: rusoto_iam::AccessKeyLastUsed) -> Result<Self, Error> {
        let last_used_date = DateTime::parse_from_rfc3339(&value.last_used_date)
            .map_err(|_| err_msg("failed to parse create date"))
            .map(|x| x.with_timezone(&Utc))?;

        Ok(AccessKeyLastUsed {
            user_name: access_key.user_name,
            user_id: access_key.user_id,
            access_key_id: access_key.access_key_id,
            status: access_key.status,
            last_used_date,
            region: value.region,
            service_name: value.service_name,
        })
    }
}

pub fn list_access_last_used(
    aws_client_config: &AwsClientConfig,
    access_key: AccessKeyMetadata
) -> Result<AccessKeyLastUsed, Error> {
    debug!("Get access key last used for key '{}'", &access_key.access_key_id);

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let iam = IamClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let request = GetAccessKeyLastUsedRequest {
        access_key_id: access_key.access_key_id.clone(),
    };

    let res = iam.get_access_key_last_used(request).sync();
    debug!(
        "Finished get access key last used for key '{}'; success={}.",
        &access_key.access_key_id,
        res.is_ok()
    );
    let res = res?.access_key_last_used.ok_or_else(|| err_msg("no result received"))?;

    AccessKeyLastUsed::try_from(access_key, res)
}
