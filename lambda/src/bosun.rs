use crate::config::BosunConfig;
use crate::metrics;
use bosun::{Bosun, BosunClient, Metadata};
use failure::Error;
use lambda_runtime::Context;
use log::debug;

pub fn init(config: &BosunConfig, ctx: &Context) -> Result<impl Bosun, Error> {
    let mut tags = config.tags.clone();
    tags.insert("host".to_string(), "lambda".to_string());
    tags.insert("function_name".to_string(), ctx.function_name.to_string());

    let mut bosun = BosunClient::with_tags(config.host.as_str(), config.timeout.unwrap_or(3), tags);
    bosun.set_basic_auth(config.user.clone(), Some(config.password.clone()));

    debug!("Initialized bosun.");
    Ok(bosun)
}

pub fn send_metadata<T: Bosun>(bosun: &T, metadatas: &[Metadata]) -> Result<(), Error> {
    for metadata in metadatas {
        bosun.emit_metadata(&metadata)?;
    }

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

    Ok(())
}
