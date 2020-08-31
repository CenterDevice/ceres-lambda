use failure::Fail;

#[derive(Debug, Fail)]
pub enum AwsWatchtowerError {
    #[fail(display = "failed to read environment variable '{}'", _0)]
    FailedEnvVar(&'static str),
    #[fail(display = "failed to parse event '{}'", _0)]
    FailedParseEvent(String),
    #[fail(display = "failed to load config file because {}", _0)]
    FailedConfig(String),
}
