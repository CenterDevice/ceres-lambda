use failure::Error;
use rusoto_core::credential::{ChainProvider, ProfileProvider};
use std::time::Duration;

// TODO: Would be nice to return impl ProvideAwsCredentials...
pub fn create_provider() -> Result<ChainProvider, Error> {
    let profile_provider = ProfileProvider::new()?;
    let mut credentials_provider = ChainProvider::with_profile_provider(profile_provider);
    // TODO: Add a KmsClient where this can be specified -- cf. clams-aws
    credentials_provider.set_timeout(Duration::from_secs(7));

    Ok(credentials_provider)
}
