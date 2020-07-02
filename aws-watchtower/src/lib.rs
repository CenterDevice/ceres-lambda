use crate::{config::FunctionConfig, error::AwsWatchtowerError, lambda::LambdaResult};
use aws::AwsClientConfig;
use failure::Error;
use lambda_runtime::{error::HandlerError, Context};
use log::info;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

mod asg_mapping;
pub mod config;
pub mod error;
mod events;
mod init;
mod lambda;
mod metrics;

// Use a counter, in case we want to track how often the function gets called before getting cold
// again.
static INVOCATION_COUNTER: AtomicUsize = AtomicUsize::new(0);

lazy_static::lazy_static! {
    static ref AWS_CLIENT_CONFIG: AwsClientConfig = AwsClientConfig::new()
        .expect("Failed to AWS client config.");
    static ref CONFIG: FunctionConfig = init::config(&AWS_CLIENT_CONFIG)
        .expect("Failed to initialize configuration.");
}

pub fn lambda_handler(json: Value, ctx: Context) -> Result<(), HandlerError> {
    run(json, &ctx).map_err(|e| ctx.new_error(e.to_string().as_str()))
}

fn run(json: Value, ctx: &Context) -> Result<(), Error> {
    let invocation_counter = INVOCATION_COUNTER.fetch_add(1, Ordering::SeqCst);
    lambda::log_invocation(invocation_counter, ctx);

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        init::log();
        lazy_static::initialize(&CONFIG);
    }

    // Run per each invocation
    let bosun = init::bosun(&CONFIG, ctx).map_err(|e| ctx.new_error(e.to_string().as_str()))?;

    // Only run once per instance of lambda function
    if invocation_counter == 0 {
        init::bosun_metrics(&bosun).map_err(|e| ctx.new_error(e.to_string().as_str()))?;
    }
    info!("Initialization complete.");

    let res = events::handle(&AWS_CLIENT_CONFIG, json, ctx, &CONFIG, &bosun);
    info!("Finished event handling.");

    log_result(&res, ctx);

    Ok(())
}

fn log_result(res: &Result<impl serde::Serialize, Error>, ctx: &Context) {
    let lambda_result = match res {
        Ok(ref details) => LambdaResult::from_ctx(ctx, None, Some(details)),
        Err(ref e) => LambdaResult::from_ctx(ctx, Some(e.to_string()), None),
    };
    lambda_result.log_human();
    lambda_result.log_json();
}
