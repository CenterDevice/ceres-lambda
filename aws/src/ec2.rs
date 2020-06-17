pub mod asg {
    use crate::{auth, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_autoscaling::{Autoscaling, AutoscalingClient, DescribeAutoScalingInstancesType};
    use rusoto_core::{HttpClient, Region};
    use serde_derive::Serialize;

    #[derive(Debug, Serialize)]
    pub struct AsgScalingInfo {
        pub ec2_instance_id:         String,
        pub auto_scaling_group_name: String,
        pub auto_scaling_event:      String,
    }

    #[derive(Debug, Serialize)]
    pub struct AsgInfo {
        pub ec2_instance_id:         String,
        pub auto_scaling_group_name: String,
    }

    pub fn get_asg_by_instance_id(instance_id: String) -> Result<Option<AsgInfo>, Error> {
        debug!("Retrieving autoscaling information for instance id '{}'", &instance_id);

        // TODO: Credentials provider should be a parameter and shared with KMS
        let credentials_provider = auth::create_provider()?;
        let http_client = HttpClient::new()?;

        // TODO: Region should be configurable; or ask the environment of this call
        let as_client = AutoscalingClient::new_with(http_client, credentials_provider, Region::EuCentral1);

        let request = DescribeAutoScalingInstancesType {
            instance_ids: Some(vec![instance_id.clone()]),
            ..Default::default()
        };

        let response = as_client.describe_auto_scaling_instances(request).sync()?;
        debug!("Autoscaling Instances information request result: '{:?}'", response);
        let first_asg = response
            .auto_scaling_instances
            .ok_or_else(|| Error::from(AwsError::GeneralError("no autoscaling information found")))?
            .into_iter()
            .next();
        debug!("Successfully retrieved autoscaling information.");

        let asg_info = first_asg.map(|details| {
            AsgInfo {
                ec2_instance_id:         instance_id,
                auto_scaling_group_name: details.auto_scaling_group_name,
            }
        });
        debug!("Parsed autoscaling information: '{:?}'", asg_info);

        Ok(asg_info)
    }
}

pub mod ebs {
    use crate::{auth, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_core::{HttpClient, Region};
    use rusoto_ec2::{DescribeVolumesRequest, Ec2, Ec2Client};
    use serde_derive::Serialize;

    #[derive(Debug, Serialize)]
    pub struct VolumeInfo {
        pub volume_id:   String,
        pub create_time: String,
        pub state:       String,
        pub kms_key_id:  Option<String>,
        pub encrypted:   bool,
    }

    pub fn get_volume_info(volume_id: String) -> Result<VolumeInfo, Error> {
        debug!("Retrieving volume information for volume id '{}'", &volume_id);

        // TODO: Credentials provider should be a parameter and shared with KMS
        let credentials_provider = auth::create_provider()?;
        let http_client = HttpClient::new()?;

        // TODO: Region should be configurable; or ask the environment of this call
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, Region::EuCentral1);

        let request = DescribeVolumesRequest {
            volume_ids: Some(vec![volume_id]),
            ..Default::default()
        };

        let response = ec2.describe_volumes(request).sync()?;
        debug!("Volume information request result: '{:?}'", response);
        let first_vol = response
            .volumes
            .ok_or_else(|| Error::from(AwsError::GeneralError("no volume information found")))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::from(AwsError::GeneralError("volume information is empty")))?;
        debug!("Successfully retrieved volume information.");

        let volume_info = match first_vol {
            rusoto_ec2::Volume {
                volume_id: Some(volume_id),
                create_time: Some(create_time),
                state: Some(state),
                kms_key_id,
                encrypted: Some(encrypted),
                ..
            } => {
                Ok(VolumeInfo {
                    volume_id,
                    create_time,
                    state,
                    kms_key_id,
                    encrypted,
                })
            }
            _ => {
                Err(Error::from(AwsError::GeneralError(
                    "volume information result is incomplete",
                )))
            }
        };
        debug!("Parsed volume information: '{:?}'", volume_info);

        volume_info
    }

    pub fn get_volume_info_by_arn(arn: String) -> Result<VolumeInfo, Error> {
        let vol_id = id_from_arn(&arn)?;
        get_volume_info(vol_id.to_string())
    }

    fn id_from_arn(arn: &str) -> Result<&str, Error> {
        debug!("Getting id from arn '{}'", arn);
        let slash = arn
            .rfind('/')
            .ok_or_else(|| Error::from(AwsError::GeneralError("Could not parse arn for id")))?;
        let (_, id) = arn.split_at(slash + 1); // Safe, because slash has been found.

        Ok(id)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use spectral::prelude::*;
        use testing;

        #[test]
        fn test_id_from_arn() {
            testing::setup();

            let arn = "arn:aws:ec2:us-east-1:012345678901:volume/vol-01234567";
            let expected = "vol-01234567";

            let res = id_from_arn(&arn);

            assert_that(&res).is_ok().is_equal_to(expected);
        }

        #[test]
        fn test_id_from_arn_fail() {
            testing::setup();

            let arn = "arn:aws:ec2:us-east-1:012345678901:volume_vol-01234567";

            let res = id_from_arn(&arn);

            assert_that(&res).is_err();
        }
    }
}

#[allow(clippy::module_inception)]
pub mod ec2 {
    use serde_derive::{Deserialize, Serialize};

    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Clone, Copy)]
    #[serde(rename_all = "kebab-case")]
    pub enum Ec2State {
        Pending      = 1,
        Running      = 2,
        ShuttingDown = 3,
        Stopped      = 4,
        Stopping     = 5,
        Terminated   = 6,
    }

    #[derive(Debug, Serialize)]
    pub struct Ec2StateInfo {
        pub ec2_instance_id: String,
        pub state:           Ec2State,
    }
}
