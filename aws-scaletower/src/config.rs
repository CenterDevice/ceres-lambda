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
    pub burst_balance: BurstBalanceConfig,
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
            burst_balance: self.burst_balance,

        };

        Ok(config)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct FunctionConfig {
    pub bosun: BosunConfig,
    pub burst_balance: BurstBalanceConfig,
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

        let burst_balance = BurstBalanceConfig {
            instance_name_filter: "centerdevice-ec2-document_server*".to_string(),
            look_back_min: 60,
            use_linear_regression: true,
            burst_balance_limit: 10,
            eta_limit_min: 10,
            terminate: false,
        };

        FunctionConfig { bosun, burst_balance }
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct BurstBalanceConfig {
    pub instance_name_filter: String,
    pub look_back_min: i64,
    pub use_linear_regression: bool,
    pub burst_balance_limit: usize,
    pub eta_limit_min: usize,
    pub terminate: bool,
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

[burst_balance]
instance_name_filter = "centerdevice-ec2-document_server*"
look_back_min = 60
use_linear_regression = true
burst_balance_limit = 10
eta_limit_min = 10
terminate = false
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
