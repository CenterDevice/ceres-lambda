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
        };

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct FunctionConfig {
    pub bosun: BosunConfig,
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

        FunctionConfig { bosun }
    }
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

[asg]
scaledown_silence_duration = "24h"

[[asg.mappings.mapping]]
search = 'webserver'
tag_name = 'webserver'
host_prefix = 'webserver-'

[[asg.mappings.mapping]]
search = 'import'
tag_name = 'import'
host_prefix = 'import-'

[ec2]
scaledown_silence_duration = "15m"
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
