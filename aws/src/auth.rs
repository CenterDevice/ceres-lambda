use failure::Error;
use futures::future::Future;
use rusoto_core::{HttpClient, Region};
use rusoto_core::credential::{AwsCredentials, AutoRefreshingProvider, CredentialsError, ChainProvider, ProfileProvider, ProvideAwsCredentials};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use std::time::Duration;


pub fn create_provider() -> Result<AutoRefreshingProvider<CeresAwsCredentialProvider>, Error> {
    let ceres_credential_provider = CeresAwsCredentialProvider::new(None)?;
    let credentials_provider = AutoRefreshingProvider::new(ceres_credential_provider)?;

    Ok(credentials_provider)
}

pub fn create_provider_with_assuem_role(sts_config: StsAssumeRoleSessionCredentialsProviderConfig) -> Result<AutoRefreshingProvider<CeresAwsCredentialProvider>, Error> {
    let ceres_credential_provider = CeresAwsCredentialProvider::new(sts_config)?;
    let credentials_provider = AutoRefreshingProvider::new(ceres_credential_provider)?;

    Ok(credentials_provider)
}


pub struct StsAssumeRoleSessionCredentialsProviderConfig {
    pub role_arn: String,
    pub region: Region,
}

impl StsAssumeRoleSessionCredentialsProviderConfig {
    pub fn new<T: Into<String>, S: Into<Region>>(role_arn: T, region: S) -> StsAssumeRoleSessionCredentialsProviderConfig {
        StsAssumeRoleSessionCredentialsProviderConfig {
            role_arn: role_arn.into(),
            region: region.into(),
        }
    }
}

pub struct CeresAwsCredentialProvider {
    sts: Option<StsAssumeRoleSessionCredentialsProvider>,
    chain: ChainProvider,
}

impl CeresAwsCredentialProvider {
    pub fn new<T: Into<Option<StsAssumeRoleSessionCredentialsProviderConfig>>>(sts_config: T) -> Result<Self, Error> {
        let sts_config = sts_config.into();
        let sts = sts_config
            .and_then(|x| sts_provider(x.role_arn, x.region).ok());
        let chain = chain_provider()?;

        Ok(CeresAwsCredentialProvider {
            sts,
            chain,
        })
    }
}

fn sts_provider<T: Into<String>, S: Into<Region>>(role_arn: T, region: S) -> Result<StsAssumeRoleSessionCredentialsProvider, Error> {
    let role_arn = role_arn.into();
    let region = region.into();

    let base_provider = ProfileProvider::with_configuration("/Users/lukas/.aws/credentials", "iam_centerdevice_my_person");
    let default_client = HttpClient::new()?;
    let sts = StsClient::new_with(default_client, base_provider, region);

    let provider = StsAssumeRoleSessionCredentialsProvider::new(
        sts,
        role_arn,
        "default".to_string(),
        None,
        None,
        None,
        None,
    );

    Ok(provider)
}

pub fn chain_provider() -> Result<ChainProvider, Error> {
    let profile_provider = ProfileProvider::new()?;
    let mut credentials_provider = ChainProvider::with_profile_provider(profile_provider);
    credentials_provider.set_timeout(Duration::from_secs(7));

    Ok(credentials_provider)
}

impl ProvideAwsCredentials for CeresAwsCredentialProvider {
    type Future = Box<dyn Future<Item=AwsCredentials, Error=CredentialsError> + Send>;

    fn credentials(&self) -> Self::Future {
        if let Some(ref sts) = self.sts {
            let sts_f = sts.credentials();
            let chain_f = self.chain.credentials();
            let f = sts_f.or_else(|_| chain_f);
            Box::new(f)
        } else {
            let f = self.chain.credentials();
            Box::new(f)
        }
    }
}

