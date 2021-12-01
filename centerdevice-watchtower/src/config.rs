use std::collections::HashMap;

use clams::config::*;
use clams_derive::Config;
use failure::Error;
use serde_derive::{Deserialize, Serialize};

use aws::{kms, AwsClientConfig};
use lambda::config::{BosunConfig, EncryptedConfig};

#[derive(Config, PartialEq, Deserialize, Serialize, Debug)]
pub struct EncryptedFunctionConfig {
    pub bosun: BosunConfig,
    pub centerdevice_health: CenterDeviceHealthConfig,
}

impl EncryptedConfig<EncryptedFunctionConfig, FunctionConfig> for EncryptedFunctionConfig {
    fn decrypt(self, aws_client_config: &AwsClientConfig) -> Result<FunctionConfig, Error> {
        let bosun_auth_password = kms::decrypt_base64(aws_client_config, &self.bosun.password)?;

        let bosun = BosunConfig {
            password: bosun_auth_password,
            ..self.bosun
        };

        let config = FunctionConfig {
            bosun,
            centerdevice_health: self.centerdevice_health,
        };

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct FunctionConfig {
    pub bosun: BosunConfig,
    pub centerdevice_health: CenterDeviceHealthConfig,
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

        let centerdevice_health = CenterDeviceHealthConfig {
            base_domain: "centerdevice.de".to_string(),
        };

        FunctionConfig {
            bosun,
            centerdevice_health,
        }
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct CenterDeviceHealthConfig {
    pub base_domain: String,
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

[centerdevice_health]
base_domain = 'centerdevice.de'
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
