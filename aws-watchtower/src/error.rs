use failure::Fail;

#[derive(Debug, Fail)]
pub enum WatchAutoscalingError {
    #[fail(display = "failed to read environment variable '{}'", _0)]
    FailedEnvVar(&'static str),
    #[fail(display = "autoScalingEvent for Successful Termination did not contain EC2InstanceId")]
    NoInstanceId,
    #[fail(display = "autoScalingEvent for Successful Termination did not contain AutoScalingGroupName")]
    NoAutoScalingGroupName,
    #[fail(display = "autoScalingEvent missing detail_type information")]
    NoDetailType,
    #[fail(display = "failed to parse AutoScalingEvent event")]
    FailedParseAsgEvent,
    #[fail(display = "failed to parse event")]
    FailedParseEvent,
    #[fail(display = "failed to load config file because {}", _0)]
    FailedConfig(String),
    #[fail(display = "did not find mapping from asg name '{}' to host prefix", _0)]
    NoHostMappingFound(String),
}
