use failure::{Error, Fail};
use rusoto_core::{credential::AutoRefreshingProvider, HttpClient, Region};
use std::sync::Arc;

pub mod auth;
pub mod cloudwatch;
pub mod ec2;
pub mod kms;

#[derive(Debug, Fail)]
pub enum AwsError {
    #[fail(display = "failed because {}", _0)]
    GeneralError(&'static str),
}

pub type CredentialsProvider = AutoRefreshingProvider<auth::CeresAwsCredentialProvider>;

pub struct AwsClientConfig {
    credentials_provider: Arc<CredentialsProvider>,
    http_client:          Arc<HttpClient>,
    region:               Region,
}

impl AwsClientConfig {
    pub fn new() -> Result<AwsClientConfig, Error> {
        let credential_provider = auth::create_provider()?;
        let region = Region::EuCentral1;

        AwsClientConfig::with_credentials_provider_and_region(credential_provider, region)
    }

    pub fn with_credentials_provider_and_region(
        credentials_provider: CredentialsProvider,
        region: Region,
    ) -> Result<AwsClientConfig, Error> {
        let http_client = HttpClient::new()?;
        Ok(AwsClientConfig {
            credentials_provider: Arc::new(credentials_provider),
            http_client: Arc::new(http_client),
            region,
        })
    }
}

pub type Filters = Vec<Filter>;

#[derive(Debug)]
pub struct Filter {
    pub name: String,
    pub values: Vec<String>,
}

impl Filter {
    pub fn new<S: Into<String>, T: Into<String>, U: Into<Vec<T>>>(name: S, values: U) -> Filter {
        let values = values.into().into_iter().map(|x| x.into()).collect();
        Filter {
            name: name.into(),
            values,
        }
    }
}

impl From<Filter> for rusoto_ec2::Filter {
    fn from(f: Filter) -> Self {
        rusoto_ec2::Filter {
            name: Some(f.name),
            values: Some(f.values),
        }
    }
}

