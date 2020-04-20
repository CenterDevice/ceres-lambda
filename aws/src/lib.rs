use failure::Fail;

pub mod auth;
pub mod ec2;
pub mod kms;

#[derive(Debug, Fail)]
pub enum AwsError {
    #[fail(display = "failed because {}", _0)]
    GeneralError(&'static str),
}
