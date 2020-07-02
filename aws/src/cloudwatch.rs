use crate::{AwsClientConfig, AwsError};
use chrono::{prelude::*, Duration};
use failure::Error;
use log::debug;
use rusoto_cloudwatch::{
    CloudWatch,
    CloudWatchClient,
    Dimension,
    GetMetricDataInput,
    Metric as RusotoMetric,
    MetricDataQuery,
    MetricStat,
};
use serde_derive::Serialize;
use std::convert::{TryFrom, TryInto};

#[derive(Debug, Serialize)]
pub struct BurstBalanceMetricData {
    pub volume_id: String,
    pub metrics:   Vec<Metric>,
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
                let metrics: Result<Vec<_>, _> = timestamps
                    .into_iter()
                    .zip(values.into_iter())
                    .map(|x| x.try_into())
                    .collect();
                let metrics =
                    metrics.map_err(|_| AwsError::GeneralError("Failed to parse timestamp from metric data"))?;
                Ok(BurstBalanceMetricData {
                    volume_id: id.to_volume_id(),
                    metrics,
                })
            }
            _ => Err(AwsError::GeneralError("volume information result is incomplete")),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub timestamp: DateTime<Utc>,
    pub value:     f64,
}

impl TryFrom<(String, f64)> for Metric {
    type Error = chrono::format::ParseError;

    fn try_from(x: (String, f64)) -> Result<Self, Self::Error> {
        let (timestamp, value) = x;
        let timestamp = timestamp.parse::<DateTime<Utc>>()?;
        Ok(Metric { timestamp, value })
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

pub fn get_burst_balances<T: Into<Option<Duration>>>(
    aws_client_config: &AwsClientConfig,
    volume_ids: Vec<String>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    period: T,
) -> Result<Vec<BurstBalanceMetricData>, Error> {
    let period = period.into();
    let period = period.map(|x| x.num_seconds()).unwrap_or(300);
    debug!("Retrieving cloudwatch burst balance for volume ids '{:?}'", &volume_ids);

    let credentials_provider = aws_client_config.credentials_provider.clone();
    let http_client = aws_client_config.http_client.clone();
    let cloudwatch = CloudWatchClient::new_with(http_client, credentials_provider, aws_client_config.region.clone());

    let metric_data_queries: Vec<_> = volume_ids
        .into_iter()
        .map(|x| {
            MetricDataQuery {
                id: x.to_query_id(),
                metric_stat: Some(MetricStat {
                    metric: RusotoMetric {
                        namespace:   Some("AWS/EBS".to_string()),
                        metric_name: Some("BurstBalance".to_string()),
                        dimensions:  Some(vec![Dimension {
                            name:  "VolumeId".to_string(),
                            value: x,
                        }]),
                    },
                    period,
                    stat: "Minimum".to_string(),
                    unit: Some("Percent".to_string()),
                }),
                return_data: Some(true),
                ..Default::default()
            }
        })
        .collect();

    let start_time = start.to_rfc3339();
    let end_time = end.to_rfc3339();
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
