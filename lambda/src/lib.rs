use failure::Error;
use lambda_runtime::Context;
use serde_derive::Serialize;

pub mod bosun;
pub mod config;
pub mod error;
pub mod metrics;

#[derive(Debug, Serialize)]
pub struct LambdaResult<'a, T> {
    function_name:    &'a str,
    function_version: &'a str,
    aws_request_id:   &'a str,
    exit_code:        usize,
    error_msg:        Option<String>,
    details:          Option<&'a T>,
    git_commit_sha:   &'a str,
    git_commit_date:  &'a str,
    git_version:      &'a str,
    cargo_version:    &'a str,
    build_timestamp:  &'a str,
}

impl<'a, T> LambdaResult<'a, T>
    where
        T: serde::Serialize,
{
    pub fn from_ctx(ctx: &'a Context, error_msg: Option<String>, details: Option<&'a T>) -> LambdaResult<'a, T> {
        LambdaResult {
            function_name: &ctx.function_name,
            function_version: &ctx.function_version,
            aws_request_id: &ctx.aws_request_id,
            exit_code: if error_msg.is_none() { 0 } else { 1 },
            error_msg,
            details,
            git_commit_sha: env!("VERGEN_SHA_SHORT"),
            git_commit_date: env!("VERGEN_COMMIT_DATE"),
            git_version: env!("VERGEN_SEMVER_LIGHTWEIGHT"),
            cargo_version: env!("CARGO_PKG_VERSION"),
            build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),
        }
    }

    pub fn log_human(&self) {
        match self.error_msg {
            None => eprintln!("Successfully executed, request_id={}.", self.aws_request_id),
            Some(ref e) => eprintln!("Failed to execute, request_id={} because {}.", self.aws_request_id, e),
        }
    }

    pub fn log_json(&self) {
        static FAILED_LAMBDA_RESULT: &str =
            r#"{ "exit_code": 1, "error_msg": "Failed to serialize json for LambdaResult" }"#;
        let json = serde_json::to_string(self).unwrap_or_else(|_| FAILED_LAMBDA_RESULT.to_string());

        println!("lambda result = {}", json);
    }
}

pub fn log_invocation(invocation_counter: usize, ctx: &Context) {
    eprintln!(
        "I've been invoked ({}); function={}, lambda version={}, request_id={}, git commit sha={}, git commit \
         date={}, git version={}, cargo version={}, build timestamp={}.",
        invocation_counter,
        ctx.function_name,
        ctx.function_version,
        ctx.aws_request_id,
        env!("VERGEN_SHA_SHORT"),
        env!("VERGEN_COMMIT_DATE"),
        env!("VERGEN_SEMVER_LIGHTWEIGHT"),
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_BUILD_TIMESTAMP")
    );
}

pub fn log_result(res: &Result<impl serde::Serialize, Error>, ctx: &Context) {
    let lambda_result = match res {
        Ok(ref details) => LambdaResult::from_ctx(ctx, None, Some(details)),
        Err(ref e) => LambdaResult::from_ctx(ctx, Some(e.to_string()), None),
    };
    lambda_result.log_human();
    lambda_result.log_json();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_invocation() {
        let ctx = Context::default();
        let invocation_counter = 0;

        log_invocation(invocation_counter, &ctx);
    }
}
