pub mod asg {
    use crate::{AwsClientConfig, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_autoscaling::{Autoscaling, AutoscalingClient, DescribeAutoScalingInstancesType};
    use serde_derive::Serialize;

    #[derive(Debug, Serialize)]
    pub struct AsgScalingInfo {
        pub ec2_instance_id: String,
        pub auto_scaling_group_name: String,
        pub auto_scaling_event: String,
    }

    #[derive(Debug, Serialize)]
    pub struct AsgInfo {
        pub ec2_instance_id: String,
        pub auto_scaling_group_name: String,
    }

    pub fn get_asg_by_instance_id(
        aws_client_config: &AwsClientConfig,
        instance_id: String,
    ) -> Result<Option<AsgInfo>, Error> {
        debug!("Retrieving autoscaling information for instance id '{}'", &instance_id);

        let credentials_provider = aws_client_config.credentials_provider.clone();
        let http_client = aws_client_config.http_client.clone();
        let as_client =
            AutoscalingClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

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

        let asg_info = first_asg.map(|details| AsgInfo {
            ec2_instance_id: instance_id,
            auto_scaling_group_name: details.auto_scaling_group_name,
        });
        debug!("Parsed autoscaling information: '{:?}'", asg_info);

        Ok(asg_info)
    }
}

pub mod ebs {
    use crate::{AwsClientConfig, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_ec2::{DescribeVolumesRequest, Ec2, Ec2Client, Filter};
    use serde_derive::Serialize;
    use std::convert::{TryFrom, TryInto};

    #[derive(Debug, Serialize)]
    pub struct VolumeInfo {
        pub volume_id: String,
        pub create_time: String,
        pub state: String,
        pub kms_key_id: Option<String>,
        pub encrypted: bool,
        pub attachments: Vec<Attachment>,
    }

    impl TryFrom<rusoto_ec2::Volume> for VolumeInfo {
        type Error = AwsError;

        fn try_from(vol: rusoto_ec2::Volume) -> Result<Self, Self::Error> {
            match vol {
                rusoto_ec2::Volume {
                    volume_id: Some(volume_id),
                    create_time: Some(create_time),
                    state: Some(state),
                    kms_key_id,
                    encrypted: Some(encrypted),
                    attachments,
                    ..
                } => {
                    let attachments: Vec<_> = attachments
                        .map(|xs| xs.into_iter().map(TryFrom::try_from).collect::<Result<Vec<_>, _>>())
                        .unwrap_or_else(|| Ok(Vec::new()))?;
                    Ok(VolumeInfo {
                        volume_id,
                        create_time,
                        state,
                        kms_key_id,
                        encrypted,
                        attachments,
                    })
                }
                _ => Err(AwsError::GeneralError("volume information result is incomplete")),
            }
        }
    }

    #[derive(Debug, Serialize)]
    pub struct Attachment {
        pub volume_id: String,
        pub state: String,
        pub instance_id: Option<String>,
    }

    impl TryFrom<rusoto_ec2::VolumeAttachment> for Attachment {
        type Error = AwsError;

        fn try_from(attachment: rusoto_ec2::VolumeAttachment) -> Result<Self, Self::Error> {
            match attachment {
                rusoto_ec2::VolumeAttachment {
                    volume_id: Some(volume_id),
                    state: Some(state),
                    instance_id,
                    ..
                } => Ok(Attachment {
                    volume_id,
                    state,
                    instance_id,
                }),
                _ => Err(AwsError::GeneralError(
                    "volume attachment information result is incomplete",
                )),
            }
        }
    }

    pub fn get_volume_info(aws_client_config: &AwsClientConfig, volume_id: String) -> Result<VolumeInfo, Error> {
        debug!("Retrieving volume information for volume id '{}'", &volume_id);

        let credentials_provider = aws_client_config.credentials_provider.clone();
        let http_client = aws_client_config.http_client.clone();
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, aws_client_config.region.clone());

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

        let volume_info = first_vol.try_into()?;
        debug!("Parsed volume information: '{:?}'", volume_info);

        Ok(volume_info)
    }

    pub fn get_volumes_info<T: Into<Option<Vec<Filter>>>>(
        aws_client_config: &AwsClientConfig,
        filters: T,
    ) -> Result<Vec<VolumeInfo>, Error> {
        let filters = filters.into();
        debug!("Retrieving volume information with filter '{:?}'", &filters);

        let credentials_provider = aws_client_config.credentials_provider.clone();
        let http_client = aws_client_config.http_client.clone();
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, aws_client_config.region.clone());

        let request = DescribeVolumesRequest {
            filters,
            ..Default::default()
        };

        let response = ec2.describe_volumes(request).sync()?;
        debug!("Volume information request result: '{:?}'", response);
        let volume_infos: Result<Vec<_>, _> = response
            .volumes
            .ok_or_else(|| Error::from(AwsError::GeneralError("no volume information found")))?
            .into_iter()
            .map(TryFrom::try_from)
            .collect();
        let volume_infos = volume_infos?;
        debug!("Successfully retrieved volume information: '{:?}'", volume_infos);

        Ok(volume_infos)
    }

    pub fn get_volume_info_by_arn(aws_client_config: &AwsClientConfig, arn: String) -> Result<VolumeInfo, Error> {
        let vol_id = id_from_arn(&arn)?;
        get_volume_info(aws_client_config, vol_id.to_string())
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
    use crate::{AwsClientConfig, AwsError};
    use failure::Error;
    use log::debug;
    pub use rusoto_ec2::Filter;
    use rusoto_ec2::{DescribeInstancesRequest, Ec2, Ec2Client};
    use serde_derive::{Deserialize, Serialize};

    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Clone, Copy)]
    #[serde(rename_all = "kebab-case")]
    pub enum Ec2State {
        Pending = 1,
        Running = 2,
        ShuttingDown = 3,
        Stopping = 4,
        Stopped = 5,
        Terminated = 6,
    }

    impl Ec2State {
        pub fn is_coming_up(&self) -> bool {
            match self {
                Self::Pending | Self::Running => true,
                _ => false,
            }
        }

        pub fn is_going_down(&self) -> bool {
            !self.is_coming_up()
        }
    }

    #[derive(Debug, Serialize)]
    pub struct Ec2StateInfo {
        pub ec2_instance_id: String,
        pub state: Ec2State,
    }

    pub fn get_instances_ids<T: Into<Option<Vec<Filter>>>>(
        aws_client_config: &AwsClientConfig,
        filters: T,
    ) -> Result<Vec<String>, Error> {
        let filters = filters.into();
        debug!("Retrieving ec2 instance information for filters '{:?}'", &filters);

        let credentials_provider = aws_client_config.credentials_provider.clone();
        let http_client = aws_client_config.http_client.clone();
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, aws_client_config.region.clone());

        let request = DescribeInstancesRequest {
            filters,
            ..Default::default()
        };

        let response = ec2.describe_instances(request).sync()?;
        debug!("Ec2 instance information request result: '{:?}'", response);
        let instance_ids: Vec<String> = response
            .reservations
            .ok_or_else(|| Error::from(AwsError::GeneralError("no ec2 instance information found")))?
            .into_iter()
            .map(|x| x.instances) // https://docs.rs/rusoto_ec2/0.36.0/rusoto_ec2/struct.Reservation.html
            .flatten() // Option
            .flatten() // Vecs of Instance, https://docs.rs/rusoto_ec2/0.36.0/rusoto_ec2/struct.Instance.html
            .map(|x| x.instance_id) // Option
            .flatten()
            .collect();
        debug!("Successfully retrieved ec2 instance ids: '{:?}'", instance_ids);

        Ok(instance_ids)
    }
}
