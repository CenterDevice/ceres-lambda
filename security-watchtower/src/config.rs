use std::collections::{HashMap, HashSet};

use clams::config::*;
use clams_derive::Config;
use failure::Error;
use serde_derive::{Deserialize, Serialize};

use aws::{kms, AwsClientConfig};
use duo::DuoClientConfig;
use lambda::config::{BosunConfig, EncryptedConfig};

#[derive(Config, PartialEq, Deserialize, Serialize, Debug)]
pub struct EncryptedFunctionConfig {
    pub bosun: BosunConfig,
    pub duo: DuoClientConfig,
    pub credentials: CredentialsConfig,
}

impl EncryptedConfig<EncryptedFunctionConfig, FunctionConfig> for EncryptedFunctionConfig {
    fn decrypt(self, aws_client_config: &AwsClientConfig) -> Result<FunctionConfig, Error> {
        let bosun_auth_password = kms::decrypt_base64(aws_client_config, &self.bosun.password)?;
        let duo_secret_key = kms::decrypt_base64(aws_client_config, &self.duo.secret_key)?;

        let bosun = BosunConfig {
            password: bosun_auth_password,
            ..self.bosun
        };

        let duo = DuoClientConfig {
            secret_key: duo_secret_key,
            ..self.duo
        };

        let config = FunctionConfig {
            bosun,
            duo,
            credentials: self.credentials,
        };

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct FunctionConfig {
    pub bosun: BosunConfig,
    pub duo: DuoClientConfig,
    pub credentials: CredentialsConfig,
}

impl FunctionConfig {}

impl Default for FunctionConfig {
    fn default() -> Self {
        let bosun = BosunConfig {
            host: "localhost:8070".to_string(),
            user: "bosun".to_string(),
            password: "bosun".to_string(),
            timeout: Some(5),
            tags: HashMap::new(),
        };

        let duo = DuoClientConfig {
            api_host_name: "apixxxxx.duo.com".to_string(),
            integration_key: "123456789ABCDEF".to_string(),
            secret_key: "WouldYouWant2Know?".to_string(),
        };

        let credentials = CredentialsConfig {
            disable_threshold_days: 60,
            delete_threshold_days: 180,
            actions_enabled: false,
            whitelist: HashSet::new(),
        };

        FunctionConfig {
            bosun,
            duo,
            credentials,
        }
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct CredentialsConfig {
    pub disable_threshold_days: i64,
    pub delete_threshold_days: i64,
    pub actions_enabled: bool,
    pub whitelist: HashSet<String>,
}

#[cfg(test)]
mod tests {
    use spectral::prelude::*;

    use super::*;

    #[test]
    fn deserialize_function_config() {
        let toml = r#"[bosun]
host = 'localhost:8070'
user = 'bosun'
password = 'bosun'
timeout = 5

[bosun.tags]
tag1 = 'value1'
tag2 = 'value2'

[duo]
api_host_name = "apixxxxx.duo.com"
integration_key = "123456789ABCDEF"
secret_key = "WouldYouWant2Know?"

[credentials]
disable_threshold_days = 60
delete_threshold_days = 180
actions_enabled = false
whitelist = []
"#;
        let mut expected = FunctionConfig::default();
        expected.bosun.tags.insert("tag1".to_string(), "value1".to_string());
        expected.bosun.tags.insert("tag2".to_string(), "value2".to_string());
        let config: Result<FunctionConfig, _> = toml::from_str(&toml);

        asserting("function config loads successfully")
            .that(&config)
            .is_ok()
            .is_equal_to(&expected);
    }
}
