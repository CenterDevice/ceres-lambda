use failure::Error;
use futures::future::Future;
use rusoto_core::{
    credential::{
        AutoRefreshingProvider, AwsCredentials, ChainProvider, CredentialsError, ProfileProvider, ProvideAwsCredentials,
    },
    HttpClient, Region,
};
use rusoto_sts::{StsAssumeRoleSessionCredentialsProvider, StsClient};
use std::{path::PathBuf, time::Duration};
use rusoto_core::credential::StaticProvider;

pub fn create_provider() -> Result<AutoRefreshingProvider<CeresAwsCredentialProvider>, Error> {
    let ceres_credential_provider = CeresAwsCredentialProvider::sts(None)?;
    let credentials_provider = AutoRefreshingProvider::new(ceres_credential_provider)?;

    Ok(credentials_provider)
}

pub fn create_provider_with_assuem_role(
    sts_config: StsAssumeRoleSessionCredentialsProviderConfig,
) -> Result<AutoRefreshingProvider<CeresAwsCredentialProvider>, Error> {
    let ceres_credential_provider = CeresAwsCredentialProvider::sts(sts_config)?;
    let credentials_provider = AutoRefreshingProvider::new(ceres_credential_provider)?;

    Ok(credentials_provider)
}

pub fn create_provider_with_static_provider(
    static_provider: StaticProvider,
) -> Result<AutoRefreshingProvider<CeresAwsCredentialProvider>, Error> {
    let ceres_credential_provider = CeresAwsCredentialProvider::static_provider(static_provider)?;
    let credentials_provider = AutoRefreshingProvider::new(ceres_credential_provider)?;

    Ok(credentials_provider)
}

pub struct StsAssumeRoleSessionCredentialsProviderConfig {
    credentials_path: PathBuf,
    profile_name: String,
    role_arn: String,
    region: Region,
}

impl StsAssumeRoleSessionCredentialsProviderConfig {
    pub fn new<S: Into<PathBuf>, T: Into<String>, U: Into<Region>>(
        credentials_path: S,
        profile_name: T,
        role_arn: T,
        region: U,
    ) -> StsAssumeRoleSessionCredentialsProviderConfig {
        StsAssumeRoleSessionCredentialsProviderConfig {
            credentials_path: credentials_path.into(),
            profile_name: profile_name.into(),
            role_arn: role_arn.into(),
            region: region.into(),
        }
    }
}

pub struct CeresAwsCredentialProvider {
    sts: Option<StsAssumeRoleSessionCredentialsProvider>,
    static_provider: Option<StaticProvider>,
    chain: ChainProvider,
}

impl CeresAwsCredentialProvider {
    pub fn sts<T: Into<Option<StsAssumeRoleSessionCredentialsProviderConfig>>>(sts_config: T) -> Result<Self, Error> {
        let sts_config = sts_config.into();
        let sts = sts_config.and_then(|x| sts_provider(x.credentials_path, x.profile_name, x.role_arn, x.region).ok());
        let chain = chain_provider()?;

        Ok(CeresAwsCredentialProvider { sts, static_provider: None, chain })
    }

    pub fn static_provider(static_provider: StaticProvider) -> Result<Self, Error> {
        let chain = chain_provider()?;

        Ok(CeresAwsCredentialProvider { sts: None, static_provider: Some(static_provider), chain })
    }
}

fn sts_provider<S: Into<PathBuf>, T: Into<String>, U: Into<Region>>(
    credentials_path: S,
    profile_name: T,
    role_arn: T,
    region: U,
) -> Result<StsAssumeRoleSessionCredentialsProvider, Error> {
    let credentials_path = credentials_path.into();
    let profile_name = profile_name.into();
    let role_arn = role_arn.into();
    let region = region.into();

    let base_provider = ProfileProvider::with_configuration(credentials_path, profile_name);
    let default_client = HttpClient::new()?;
    let sts = StsClient::new_with(default_client, base_provider, region);

    let provider =
        StsAssumeRoleSessionCredentialsProvider::new(sts, role_arn, "default".to_string(), None, None, None, None);

    Ok(provider)
}

pub fn chain_provider() -> Result<ChainProvider, Error> {
    let profile_provider = ProfileProvider::new()?;
    let mut credentials_provider = ChainProvider::with_profile_provider(profile_provider);
    credentials_provider.set_timeout(Duration::from_secs(7));

    Ok(credentials_provider)
}

impl ProvideAwsCredentials for CeresAwsCredentialProvider {
    type Future = Box<dyn Future<Item = AwsCredentials, Error = CredentialsError> + Send>;

    fn credentials(&self) -> Self::Future {
        if let Some(ref sts) = self.sts {
            let sts_f = sts.credentials();
            let chain_f = self.chain.credentials();
            let f = sts_f.or_else(|_| chain_f);
            Box::new(f)
        } else if let Some(ref static_provider) = self.static_provider {
            let static_provider_f = static_provider.credentials();
            let chain_f = self.chain.credentials();
            let f = static_provider_f.or_else(|_| chain_f);
            Box::new(f)
        } else {
            let f = self.chain.credentials();
            Box::new(f)
        }
    }
}
