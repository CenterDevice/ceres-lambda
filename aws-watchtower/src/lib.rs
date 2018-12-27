use crate::bosun::{BosunClient, Bosun, Metadata};
use crate::config::{EncryptedFunctionConfig, EnvConfig, FunctionConfig};
use crate::error::WatchAutoscalingError;
use crate::lambda::LambdaResult;
use clams::config::Config;
use failure::Error;
use lambda_runtime::{error::HandlerError, Context};
use log::{debug, info};
use serde_json::{self, Value};
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

mod asg_mapping;
mod bosun;
pub mod config;
pub mod error;
mod events;
mod kms;
mod lambda;

// Use a counter, in case we want to track how often the function gets called before getting cold
// again.
static INVOCATION_COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;

pub fn lambda_handler(json: Value, ctx: Context) -> Result<(), HandlerError> {
    run(json, &ctx)
        .map_err(|e| ctx.new_error(e.to_string().as_str()))
}

fn run(json: Value, ctx: &Context) -> Result<(), Error> {
    let invocation_counter = INVOCATION_COUNTER.fetch_add(1, Ordering::SeqCst);
    lambda::log_invocation(invocation_counter, ctx);

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        init_log()
    }

    // Run per each invocation
    let config = init_config()
        .map_err(|e| ctx.new_error(e.to_string().as_str()))?;
    let bosun = init_bosun(&config, ctx)
        .map_err(|e| ctx.new_error(e.to_string().as_str()))?;

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        init_bosun_metrics(&bosun)
            .map_err(|e| ctx.new_error(e.to_string().as_str()))?;
    }
    info!("Invocation initialization complete.");

    let res = events::handle(json, ctx, &config, &bosun);

    let lambda_result = match res {
        Ok(ref details) => {
            LambdaResult::from_ctx(ctx, None, Some(details))
        }
        Err(ref e) => {
            LambdaResult::from_ctx(ctx, Some(e.to_string()), None)
        }
    };
    lambda_result.log_human();
    lambda_result.log_json();

    res
}

fn init_log() {
    env_logger::init();
    debug!("Initialized logger.");
}

fn init_config() -> Result<FunctionConfig, Error> {
    let env_config = EnvConfig::from_env()?;
    debug!("Loaded environment variables configuration = {:?}.", &env_config);

    let encrypted_config = EncryptedFunctionConfig::from_file(&env_config.config_file)
        // This map_err seems necessary since error_chain::Error is not Send + 'static
        .map_err(|e| WatchAutoscalingError::FailedConfig(e.to_string()))?;
    debug!("Loaded encrypted configuration from file {:?}.", &env_config.config_file);
    let config = encrypted_config.decrypt()?;
    debug!("Decrypted encrypted configuration.");

    Ok(config)
}

fn init_bosun(config: &FunctionConfig, ctx: &Context) -> Result<impl Bosun, Error> {
    let mut tags = config.bosun.tags.clone();
    tags.insert("host".to_string(), "lambda".to_string());
    tags.insert("function_name".to_string(), ctx.function_name.to_string());

    let bosun = BosunClient::with_tags(
        config.bosun.uri().as_str(),
        config.bosun.timeout.unwrap_or(3),
        tags,
    );

    debug!("Initialized bosun.");
    Ok(bosun)
}

pub fn init_bosun_metrics<T: Bosun>(bosun: &T) -> Result<(), Error> {
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
        bosun::METRIC_LAMBDA_INVOCATION_COUNT,
        "gauge",
        "Result",
        "AWS Lambda function invocation result code [0 = success, >0 = failure]",
    );
    bosun.emit_metadata(&metadata)?;

    debug!("Initialized bosun metrics.");
    Ok(())
}

#[cfg(test)]
mod testing {
    use std::sync::{Once, ONCE_INIT};

    pub static INIT: Once = ONCE_INIT;

    /// Setup function that is only run once, even if called multiple times.
    pub fn setup() {
        INIT.call_once(|| {
            env_logger::init();
        });
    }
}

