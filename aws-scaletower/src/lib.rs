use crate::config::{EncryptedFunctionConfig, FunctionConfig};
use aws::AwsClientConfig;
use failure::Error;
use lambda::{self, config::EncryptedConfig, FunctionVersion};
use lambda_runtime::{error::HandlerError, Context};
use log::{debug, info};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod burst_balance;
pub mod config;
pub mod error;
pub mod events;
pub mod metrics;

static FUNCTION_VERSION: lambda::FunctionVersion = FunctionVersion {
    git_commit_sha: env!("VERGEN_SHA_SHORT"),
    git_commit_date: env!("VERGEN_COMMIT_DATE"),
    git_version: env!("VERGEN_SEMVER_LIGHTWEIGHT"),
    cargo_version: env!("CARGO_PKG_VERSION"),
    build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),
};

// Use a counter, in case we want to track how often the function gets called before getting cold
// again.
static INVOCATION_COUNTER: AtomicUsize = AtomicUsize::new(0);

lazy_static::lazy_static! {
    static ref AWS_CLIENT_CONFIG: AwsClientConfig = AwsClientConfig::new()
        .expect("Failed to AWS client config.");
    static ref CONFIG: FunctionConfig = EncryptedFunctionConfig::load_from_env(&AWS_CLIENT_CONFIG)
        .expect("Failed to initialize configuration.");
}

pub fn lambda_handler(json: Value, ctx: Context) -> Result<(), HandlerError> {
    run(json, &ctx).map_err(|e| ctx.new_error(e.to_string().as_str()))
}

fn run(json: Value, ctx: &Context) -> Result<(), Error> {
    let invocation_counter = INVOCATION_COUNTER.fetch_add(1, Ordering::SeqCst);
    lambda::log_invocation(invocation_counter, ctx, &FUNCTION_VERSION);

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        env_logger::init();
        debug!("Initialized logger.");
        lazy_static::initialize(&CONFIG);
    }

    // Run per each invocation
    let bosun = lambda::bosun::init(&CONFIG.bosun, ctx).map_err(|e| ctx.new_error(e.to_string().as_str()))?;

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        metrics::send_metadata(&bosun).map_err(|e| ctx.new_error(e.to_string().as_str()))?;
        debug!("Initialized bosun metrics.");
    }
    info!("Initialization complete.");

    let res = events::handle(&AWS_CLIENT_CONFIG, json, ctx, &CONFIG, &bosun);
    info!("Finished event handling.");

    lambda::log_result(&res, ctx, &FUNCTION_VERSION);

    Ok(())
}
