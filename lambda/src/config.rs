use std::collections::HashMap;

use clams::config::Config;
use failure::{Error, Fail};
use log::debug;
use serde_derive::{Deserialize, Serialize};

use crate::error::LambdaError;
use aws::AwsClientConfig;

#[derive(Debug)]
pub struct EnvConfig {
    pub config_file: String,
}

impl EnvConfig {
    pub fn from_env() -> Result<Self, Error> {
        let config_file =
            std::env::var("CD_CONFIG_FILE").map_err(|e| e.context(LambdaError::FailedEnvVar("CD_CONFIG_FILE")))?;

        let env_config = EnvConfig { config_file };

        Ok(env_config)
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        EnvConfig {
            config_file: "".to_string(),
        }
    }
}

pub trait EncryptedConfig<T: EncryptedConfig<T, S>, S>: Config<ConfigStruct = T> {
    fn decrypt(self, aws_client_config: &AwsClientConfig) -> Result<S, Error>;

    fn load_from_env(aws_client_config: &AwsClientConfig) -> Result<S, Error> {
        let env_config = EnvConfig::from_env()?;
        debug!("Loaded environment variables configuration = {:?}.", &env_config);

        let encrypted_config: T = Self::from_file(&env_config.config_file)
            // This map_err seems necessary since error_chain::Error is not Send + 'static
            .map_err(|e| LambdaError::FailedConfig(e.to_string()))?;
        debug!(
            "Loaded encrypted configuration from file {:?}.",
            &env_config.config_file
        );
        let config: S = encrypted_config.decrypt(aws_client_config)?;
        debug!("Decrypted encrypted configuration.");

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct BosunConfig {
    pub host: String,
    pub user: String,
    pub password: String,
    pub timeout: Option<u64>,
    pub tags: HashMap<String, String>,
}
