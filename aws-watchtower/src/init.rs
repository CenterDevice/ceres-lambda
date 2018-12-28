use crate::bosun::{self, Bosun, BosunClient, Metadata};
use crate::config::{EncryptedFunctionConfig, EnvConfig, FunctionConfig};
use crate::error::WatchAutoscalingError;
use clams::config::Config;
use failure::Error;
use lambda_runtime::Context;
use lazy_static;
use log::debug;
use serde_json;

pub fn log() {
    env_logger::init();
    debug!("Initialized logger.");
}

pub fn config() -> Result<FunctionConfig, Error> {
    let env_config = EnvConfig::from_env()?;
    debug!("Loaded environment variables configuration = {:?}.", &env_config);

    let encrypted_config = EncryptedFunctionConfig::from_file(&env_config.config_file)
        // This map_err seems necessary since error_chain::Error is not Send + 'static
        .map_err(|e| WatchAutoscalingError::FailedConfig(e.to_string()))?;
    debug!(
        "Loaded encrypted configuration from file {:?}.",
        &env_config.config_file
    );
    let config = encrypted_config.decrypt()?;
    debug!("Decrypted encrypted configuration.");

    Ok(config)
}

pub fn bosun(config: &FunctionConfig, ctx: &Context) -> Result<impl Bosun, Error> {
    let mut tags = config.bosun.tags.clone();
    tags.insert("host".to_string(), "lambda".to_string());
    tags.insert("function_name".to_string(), ctx.function_name.to_string());

    let bosun = BosunClient::with_tags(config.bosun.uri().as_str(), config.bosun.timeout.unwrap_or(3), tags);

    debug!("Initialized bosun.");
    Ok(bosun)
}

pub fn bosun_metrics<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadata = Metadata::new(
        bosun::METRIC_ASG_UP_DOWN,
        "rate",
        "Scaling",
        "ASG up and down scaling event [-1 = down scaling, +1 = up scaling]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        bosun::METRIC_LAMBDA_INVOCATION_COUNT,
        "rate",
        "Invocations",
        "AWS Lambda function invocation counter",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        bosun::METRIC_LAMBDA_INVOCATION_RESULT,
        "gauge",
        "Result",
        "AWS Lambda function invocation result code [0 = success, >0 = failure]",
    );
    bosun.emit_metadata(&metadata)?;

    debug!("Initialized bosun metrics.");
    Ok(())
}
