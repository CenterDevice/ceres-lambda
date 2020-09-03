use failure::Error;
use lambda_runtime::Context;
use serde_derive::Serialize;

pub mod bosun;
pub mod config;
pub mod error;
pub mod metrics;

pub struct FunctionVersion {
    pub git_commit_sha: &'static str,
    pub git_commit_date: &'static str,
    pub git_version: &'static str,
    pub cargo_version: &'static str,
    pub build_timestamp: &'static str,
}

#[derive(Debug, Serialize)]
pub struct LambdaResult<'a, T> {
    function_name: &'a str,
    function_version: &'a str,
    aws_request_id: &'a str,
    exit_code: usize,
    error_msg: Option<String>,
    details: Option<&'a T>,
    git_commit_sha: &'a str,
    git_commit_date: &'a str,
    git_version: &'a str,
    cargo_version: &'a str,
    build_timestamp: &'a str,
}

impl<'a, T> LambdaResult<'a, T>
where
    T: serde::Serialize,
{
    pub fn from_ctx(
        ctx: &'a Context,
        function_version: &FunctionVersion,
        error_msg: Option<String>,
        details: Option<&'a T>,
    ) -> LambdaResult<'a, T> {
        LambdaResult {
            function_name: &ctx.function_name,
            function_version: &ctx.function_version,
            aws_request_id: &ctx.aws_request_id,
            exit_code: if error_msg.is_none() { 0 } else { 1 },
            error_msg,
            details,
            git_commit_sha: function_version.git_commit_sha,
            git_commit_date: function_version.git_commit_date,
            git_version: function_version.git_version,
            cargo_version: function_version.cargo_version,
            build_timestamp: function_version.build_timestamp,
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

pub fn log_invocation(invocation_counter: usize, ctx: &Context, function_version: &FunctionVersion) {
    eprintln!(
        "I've been invoked ({}); function={}, lambda version={}, request_id={}, git commit sha={}, git commit \
         date={}, git version={}, cargo version={}, build timestamp={}.",
        invocation_counter,
        ctx.function_name,
        ctx.function_version,
        ctx.aws_request_id,
        function_version.git_commit_sha,
        function_version.git_commit_date,
        function_version.git_version,
        function_version.cargo_version,
        function_version.build_timestamp,
    );
}

pub fn log_result(res: &Result<impl serde::Serialize, Error>, ctx: &Context, function_version: &FunctionVersion) {
    let lambda_result = match res {
        Ok(ref details) => LambdaResult::from_ctx(ctx, function_version, None, Some(details)),
        Err(ref e) => LambdaResult::from_ctx(ctx, function_version, Some(e.to_string()), None),
    };
    lambda_result.log_human();
    lambda_result.log_json();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_invocation() {
        let invocation_counter = 0;
        let ctx = Context::default();
        let function_version = FunctionVersion {
            git_commit_sha: "git_commit_sha",
            git_commit_date: "git_commit_date",
            git_version: "git_version",
            cargo_version: "cargo_version",
            build_timestamp: "build_timestamp",
        };

        log_invocation(invocation_counter, &ctx, &function_version);
    }
}
