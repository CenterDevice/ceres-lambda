use crate::AwsClientConfig;
use failure::{Error, err_msg};
use log::{debug, warn};
use rusoto_iam::{Iam, IamClient, ListUsersRequest, ListAccessKeysRequest, GetAccessKeyLastUsedRequest};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use std::convert::TryFrom;

#[derive(Debug)]
pub struct User {
    password_last_used: Option<DateTime<Utc>>,
    user_id: String,
    user_name: String,
    path: String,
}

impl From<rusoto_iam::User> for User {
    fn from(user: rusoto_iam::User) -> Self {
        let password_last_used = user.password_last_used
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
    let res = res?;

    if log::max_level() >= log::Level::Warn && res.is_truncated.is_some() && res.is_truncated.unwrap() {
        warn!("List users: Result is truncated.");
    }

    let res: Vec<User> = res.users.into_iter()
        .map(Into::into)
        .collect();

    Ok(res)
}

#[derive(Debug)]
pub struct AccessKeyMetadata {
    access_key_id: String,
    create_date: DateTime<Utc>,
    status: AccessKeyMetadataStatus,
    user_name: String,
}

impl TryFrom<rusoto_iam::AccessKeyMetadata> for AccessKeyMetadata {
    type Error = Error;

    fn try_from(value: rusoto_iam::AccessKeyMetadata) -> Result<Self, Self::Error> {
        let access_key_id = value.access_key_id.ok_or_else(|| err_msg("no access key provided"))?;
        let create_date = value.create_date
            .ok_or_else(|| err_msg("no create date provided"))
            .and_then(|x|
                DateTime::parse_from_rfc3339(&x).map_err(|_| err_msg("failed to parse create date"))
            )
            .map(|x| x.with_timezone(&Utc))?;
        let status = value.status
            .ok_or_else(|| err_msg("no status provided"))
            .and_then(|x| AccessKeyMetadataStatus::from_str(&x))?;
        let user_name = value.user_name.ok_or_else(|| err_msg("no user name provided"))?;

        Ok(AccessKeyMetadata {
            access_key_id,
            create_date,
            status,
            user_name
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
            _ => Err(err_msg("failed to parse access key status"))
        }
    }
}

pub fn list_access_keys_for_user(aws_client_config: &AwsClientConfig, user_name: String) -> Result<Vec<AccessKeyMetadata>, Error> {
    debug!("List access keys for user '{}'", &user_name);

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let iam = IamClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let request = ListAccessKeysRequest {
        marker: None,
        max_items: Some(100),
        user_name: Some(user_name.clone()),
    };
    let res = iam.list_access_keys(request).sync();
    debug!("Finished list access keys request for user '{}'; success={}.", &user_name, res.is_ok());
    let res = res?;

    if log::max_level() >= log::Level::Warn && res.is_truncated.is_some() && res.is_truncated.unwrap() {
        warn!("List users: Result is truncated.");
    }

    let res: Vec<Result<AccessKeyMetadata, Error>> = res.access_key_metadata.into_iter()
        .map(TryFrom::try_from)
        .collect();
    let res: Result<Vec<AccessKeyMetadata>, Error> = res.into_iter().collect();

    res
}

#[derive(Debug)]
pub struct AccessKeyLastUsed {
    last_used_date: DateTime<Utc>,
    region: String,
    service_name: String,
}

impl TryFrom<rusoto_iam::AccessKeyLastUsed> for AccessKeyLastUsed {
    type Error = Error;

    fn try_from(value: rusoto_iam::AccessKeyLastUsed) -> Result<Self, Self::Error> {
        let last_used_date = DateTime::parse_from_rfc3339(&value.last_used_date)
            .map_err(|_| err_msg("failed to parse create date"))
            .map(|x| x.with_timezone(&Utc))?;

        Ok(AccessKeyLastUsed {
            last_used_date,
            region: value.region,
            service_name: value.service_name,
        })
    }
}

pub fn list_access_last_used(aws_client_config: &AwsClientConfig, access_key_id: String) -> Result<AccessKeyLastUsed, Error> {
    debug!("Get access key last used for key '{}'", &access_key_id);

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let iam = IamClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let request = GetAccessKeyLastUsedRequest {
        access_key_id: access_key_id.clone(),
    };

    let res = iam.get_access_key_last_used(request).sync();
    debug!("Finished get access key last used for key '{}'; success={}.", &access_key_id, res.is_ok());
    let res = res?.access_key_last_used.ok_or_else(|| err_msg("no result received"))?;

    AccessKeyLastUsed::try_from(res)
}
