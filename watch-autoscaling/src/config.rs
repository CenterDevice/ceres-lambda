use crate::WatchAutoscalingError;
use crate::asg_mapping::Mappings;
use crate::kms;

use clams::config::*;
use clams_derive::Config;
use failure::{Error, Fail};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Config, PartialEq, Deserialize, Serialize, Debug)]
pub struct EncryptedFunctionConfig {
    pub bosun: Bosun,
    pub asg_mappings: Mappings,
}

impl EncryptedFunctionConfig {
    pub fn decrypt(self) -> Result<FunctionConfig, Error> {
        let bosun_auth_password = kms::decrypt_base64(&self.bosun.password)?;

        let bosun = Bosun {
            password: bosun_auth_password,
            .. self.bosun
        };

        let config = FunctionConfig {
            bosun,
            asg_mappings: self.asg_mappings,
        };

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct Bosun {
    pub host: String,
    pub user: String,
    pub password: String,
    pub tags: HashMap<String, String>,
}

impl Bosun {
    pub fn uri(&self) -> String {
        format!(
            "https://{}:{}@{}",
            &self.user, &self.password, &self.host
        )
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct FunctionConfig {
    pub bosun: Bosun,
    pub asg_mappings: Mappings,
}

impl Default for FunctionConfig {
    fn default() -> Self {
        let bosun = Bosun {
            host: "localhost:8070".to_string(),
            user: "bosun".to_string(),
            password: "bosun".to_string(),
            tags: HashMap::new(),
        };

        let asg_mappings = Mappings {
            items: Vec::new(),
        };

        FunctionConfig {
            bosun,
            asg_mappings,
        }
    }
}

#[derive(Debug)]
pub struct EnvConfig {
    pub config_file: String,
}

impl EnvConfig {
    pub fn from_env() -> Result<Self, Error> {
        let config_file = std::env::var("CD_CONFIG_FILE")
            .map_err(|e| e.context(WatchAutoscalingError::FailedEnvVar("CD_CONFIG_FILE")))?;

        let env_config = EnvConfig {
            config_file,
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asg_mapping::Mapping;

    use toml;
    use spectral::prelude::*;

    #[test]
    fn deserialize_function_config() {
        let toml =
r#"[bosun]
host = 'localhost:8070'
user = 'bosun'
password = 'bosun'

[bosun.tags]
tag1 = 'value1'
tag2 = 'value2'

[[asg_mappings.mapping]]
search = 'webserver'
tag_name = 'webserver'
host_prefix = 'webserver-'

[[asg_mappings.mapping]]
search = 'import'
tag_name = 'import'
host_prefix = 'import-'
"#;
        let mut expected = FunctionConfig::default();
        expected.bosun.tags.insert("tag1".to_string(), "value1".to_string());
        expected.bosun.tags.insert("tag2".to_string(), "value2".to_string());
        let asg_mappings = Mappings {
            items: vec![
                Mapping { search: "webserver".to_string(), tag_name: "webserver".to_string(), host_prefix: "webserver-".to_string() },
                Mapping { search: "import".to_string(), tag_name: "import".to_string(), host_prefix: "import-".to_string() },
            ],
        };
        expected.asg_mappings = asg_mappings;

        let config: Result<FunctionConfig, _> = toml::from_str(&toml);

        asserting("function config loads successfully").that(&config).is_ok().is_equal_to(&expected);
    }
}
