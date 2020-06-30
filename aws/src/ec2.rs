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

pub mod cloudwatch {
    use crate::{auth, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_core::{HttpClient, Region};
    use rusoto_cloudwatch::{CloudWatch, CloudWatchClient, Dimension, GetMetricDataInput, MetricDataQuery, MetricStat, Metric as RusotoMetric};
    use serde_derive::Serialize;
    use std::convert::{TryInto, TryFrom};

    #[derive(Debug, Serialize)]
    pub struct BurstBalanceMetricData {
        pub volume_id: String,
        pub metrics: Vec<Metric>,
    }

    impl TryFrom<rusoto_cloudwatch::MetricDataResult> for BurstBalanceMetricData {
        type Error = AwsError;
        fn try_from(x: rusoto_cloudwatch::MetricDataResult) -> Result<Self, Self::Error> {
            match x {
                rusoto_cloudwatch::MetricDataResult {
                    id: Some(id),
                    status_code: Some(status_code),
                    timestamps: Some(timestamps),
                    values: Some(values),
                    ..
                } if status_code == "Complete" => {
                    let metrics: Vec<_> = timestamps
                        .into_iter()
                        .zip(values.into_iter())
                        .map(|x| x.into())
                        .collect();
                    Ok(BurstBalanceMetricData {
                        volume_id: id.to_volume_id(),
                        metrics,
                    })
                }
                _ => {
                    Err(AwsError::GeneralError(
                        "volume information result is incomplete",
                    ))
                }
            }
        }
    } 

    #[derive(Debug, Serialize)]
    pub struct Metric {
        pub timestamp: String,
        pub value: f64,
    }

    impl From<(String, f64)> for Metric {
        fn from(x: (String, f64)) -> Self {
            let (timestamp, value) = x;
            Metric {
                timestamp,
                value,
            }
        }
    }

    trait ConvertVolumeIdToQueryId {
        fn to_volume_id(&self) -> String;
        fn to_query_id(&self) -> String;
    }

    impl ConvertVolumeIdToQueryId for String {
        fn to_volume_id(&self) -> String { self.replace("_", "-") }
        fn to_query_id(&self) -> String { self.replace("-", "_") }
    }

    pub fn get_burst_balance(volume_ids: Vec<String>, start_time: String, end_time: String) -> Result<Vec<BurstBalanceMetricData>, Error> {
        debug!("Retrieving cloudwatch burst balance for volume ids '{:?}'", &volume_ids);

        // TODO: Credentials provider should be a parameter and shared with KMS
        let credentials_provider = auth::create_provider()?;
        let http_client = HttpClient::new()?;

        // TODO: Region should be configurable; or ask the environment of this call
        let cloudwatch = CloudWatchClient::new_with(http_client, credentials_provider, Region::EuCentral1);

        let metric_data_queries: Vec<_> = volume_ids
            .into_iter()
            .map(|x|
                MetricDataQuery {
                    id: x.to_query_id(),
                    metric_stat: Some(MetricStat {
                        metric: RusotoMetric {
                            namespace: Some("AWS/EBS".to_string()),
                            metric_name: Some("BurstBalance".to_string()),
                            dimensions: Some(vec![
                                Dimension {
                                    name: "VolumeId".to_string(),
                                    value: x,
                                }
                            ]),
                        },
                        period: 300,
                        stat: "Minimum".to_string(),
                        unit: Some("Percent".to_string()),
                    }),
                    return_data: Some(true),
                    ..Default::default()
                }
            )
            .collect();

        let request = GetMetricDataInput {
            metric_data_queries,
            scan_by: Some("TimestampAscending".to_string()),
            start_time,
            end_time,
            ..Default::default()
        };
        debug!("CloudWatch burst balance request: '{:#?}'", request);

        let response = cloudwatch.get_metric_data(request).sync()?;
        debug!("CloudWatch burst balance request result: '{:?}'", response);

        let metric_data_results: Result<Vec<_>, _> = response
            .metric_data_results
            .ok_or_else(|| Error::from(AwsError::GeneralError("no cloudwatch information found")))?
            .into_iter()
            .map(TryFrom::try_from)
            .collect();
        let metric_data_results = metric_data_results?;

        Ok(metric_data_results)
    }
}

pub mod ebs {
    use crate::{auth, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_core::{HttpClient, Region};
    use rusoto_ec2::{DescribeVolumesRequest, Ec2, Ec2Client, Filter};
    use serde_derive::Serialize;
    use std::convert::{TryInto, TryFrom};

    #[derive(Debug, Serialize)]
    pub struct VolumeInfo {
        pub volume_id:   String,
        pub create_time: String,
        pub state:       String,
        pub kms_key_id:  Option<String>,
        pub encrypted:   bool,
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
                    let attachments: Vec<_> = attachments.map(|xs|
                            xs.into_iter().map(TryFrom::try_from).collect::<Result<Vec<_>,_>>()
                        ).unwrap_or_else(|| Ok(Vec::new()))?;
                    Ok(VolumeInfo {
                        volume_id,
                        create_time,
                        state,
                        kms_key_id,
                        encrypted,
                        attachments,
                    })
                }
                _ => {
                    Err(AwsError::GeneralError(
                        "volume information result is incomplete",
                    ))
                }
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
                } => {
                    Ok(Attachment {
                        volume_id,
                        state,
                        instance_id,
                    })
                }
                _ => {
                    Err(AwsError::GeneralError(
                        "volume attachment information result is incomplete",
                    ))
                }
            }
        }
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

        let volume_info = first_vol.try_into()?;
        debug!("Parsed volume information: '{:?}'", volume_info);

        Ok(volume_info)
    }

    pub fn get_volumes_info<T: Into<Option<Vec<Filter>>>>(filters: T) -> Result<Vec<VolumeInfo>, Error> {
        let filters = filters.into();
        debug!("Retrieving volume information with filter '{:?}'", &filters);

        // TODO: Credentials provider should be a parameter and shared with KMS
        let credentials_provider = auth::create_provider()?;
        let http_client = HttpClient::new()?;

        // TODO: Region should be configurable; or ask the environment of this call
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, Region::EuCentral1);

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
    use crate::{auth, AwsError};
    use failure::Error;
    use log::debug;
    use rusoto_core::{HttpClient, Region};
    use rusoto_ec2::{DescribeInstancesRequest, Ec2, Ec2Client};
    pub use rusoto_ec2::Filter;
    use serde_derive::{Deserialize, Serialize};

    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Clone, Copy)]
    #[serde(rename_all = "kebab-case")]
    pub enum Ec2State {
        Pending      = 1,
        Running      = 2,
        ShuttingDown = 3,
        Stopping     = 4,
        Stopped      = 5,
        Terminated   = 6,
    }

    impl Ec2State {
        pub fn is_coming_up(&self) -> bool {
            match self {
                Self::Pending | Self::Running => true,
                _ => false,
            }
        }

        pub fn is_going_down(&self) -> bool { !self.is_coming_up() }
    }

    #[derive(Debug, Serialize)]
    pub struct Ec2StateInfo {
        pub ec2_instance_id: String,
        pub state:           Ec2State,
    }

    pub fn get_instances_ids<T: Into<Option<Vec<Filter>>>>(filters: T) -> Result<Vec<String>, Error> {
        let filters = filters.into();
        debug!("Retrieving ec2 instance information for filters '{:?}'", &filters);

        // TODO: Credentials provider should be a parameter and shared with KMS
        let credentials_provider = auth::create_provider()?;
        let http_client = HttpClient::new()?;

        // TODO: Region should be configurable; or ask the environment of this call
        let ec2 = Ec2Client::new_with(http_client, credentials_provider, Region::EuCentral1);

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
            .map(|x| x.instance_id)  // Option
            .flatten()
            .collect();
        debug!("Successfully retrieved ec2 instance ids: '{:?}'", instance_ids);

        Ok(instance_ids)
    }
}
