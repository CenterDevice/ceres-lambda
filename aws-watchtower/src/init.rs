use crate::{
    config::{EncryptedFunctionConfig, EnvConfig, FunctionConfig},
    error::AwsWatchtowerError,
    metrics,
};
use aws::AwsClientConfig;
use bosun::{Bosun, BosunClient, Metadata};
use clams::config::Config;
use failure::Error;
use lambda_runtime::Context;
use log::debug;

pub fn log() {
    env_logger::init();
    debug!("Initialized logger.");
}

pub fn config(aws_client_config: &AwsClientConfig) -> Result<FunctionConfig, Error> {
    let env_config = EnvConfig::from_env()?;
    debug!("Loaded environment variables configuration = {:?}.", &env_config);

    let encrypted_config = EncryptedFunctionConfig::from_file(&env_config.config_file)
        // This map_err seems necessary since error_chain::Error is not Send + 'static
        .map_err(|e| AwsWatchtowerError::FailedConfig(e.to_string()))?;
    debug!(
        "Loaded encrypted configuration from file {:?}.",
        &env_config.config_file
    );
    let config = encrypted_config.decrypt(aws_client_config)?;
    debug!("Decrypted encrypted configuration.");

    Ok(config)
}

pub fn bosun(config: &FunctionConfig, ctx: &Context) -> Result<impl Bosun, Error> {
    let mut tags = config.bosun.tags.clone();
    tags.insert("host".to_string(), "lambda".to_string());
    tags.insert("function_name".to_string(), ctx.function_name.to_string());

    let mut bosun = BosunClient::with_tags(config.bosun.host.as_str(), config.bosun.timeout.unwrap_or(3), tags);
    bosun.set_basic_auth(config.bosun.user.clone(), Some(config.bosun.password.clone()));

    debug!("Initialized bosun.");
    Ok(bosun)
}

pub fn bosun_metrics<T: Bosun>(bosun: &T) -> Result<(), Error> {
    let metadata = Metadata::new(
        metrics::ASG_UP_DOWN,
        "rate",
        "Scaling",
        "ASG up and down scaling event [-1 = down scaling, +1 = up scaling]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        metrics::EBS_VOLUME_EVENT,
        "rate",
        "Change",
        "Creation or deletion of EBS volumes [-1 = deletion, +1 = creation]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        metrics::EBS_VOLUME_CREATION_RESULT,
        "gauge",
        "Result",
        "Creation result of EBS volumes [0 = success, 1 = failure]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        metrics::EC2_STATE_CHANGE,
        "gauge",
        "State",
        "Instance State Change Event [1 = pending, 2 = running, 3 = shutting-down, 4 = stopping, 5 = stopped, 6 = \
         terminated]",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        metrics::LAMBDA_INVOCATION_COUNT,
        "rate",
        "Invocations",
        "AWS Lambda function invocation counter",
    );
    bosun.emit_metadata(&metadata)?;

    let metadata = Metadata::new(
        metrics::LAMBDA_INVOCATION_RESULT,
        "gauge",
        "Result",
        "AWS Lambda function invocation result code [0 = success, >0 = failure]",
    );
    bosun.emit_metadata(&metadata)?;

    debug!("Initialized bosun metrics.");
    Ok(())
}
