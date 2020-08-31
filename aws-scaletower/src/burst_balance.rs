use aws::{
    cloudwatch::{self, BurstBalanceMetricData, Metric},
    ec2::{ebs::get_volumes_info, ec2::get_instances_ids, ec2::Filter as AwsFilter},
    AwsClientConfig, Filters,
};
use chrono::{prelude::*, Duration};
use failure::Error;
use log::trace;
use std::collections::HashMap;

#[derive(Debug)]
struct VolumeAttachment {
    pub instance_id: String,
    pub volume_id: String,
}

trait LastMetric {
    fn get_last_metric(&self) -> Option<&Metric>;
}

impl LastMetric for BurstBalanceMetricData {
    fn get_last_metric(&self) -> Option<&Metric> {
        self.metrics.last()
    }
}

struct VolInstanceMap(HashMap<String, String>);

impl From<Vec<VolumeAttachment>> for VolInstanceMap {
    fn from(xs: Vec<VolumeAttachment>) -> Self {
        let map: HashMap<String, String> = xs.into_iter().map(|x| (x.volume_id, x.instance_id)).collect();

        VolInstanceMap(map)
    }
}

trait RunningOutOfBurstsForecast {
    fn forecast_running_out_of_burst(&self) -> Option<DateTime<Utc>>;
}

impl RunningOutOfBurstsForecast for BurstBalanceMetricData {
    fn forecast_running_out_of_burst(&self) -> Option<DateTime<Utc>> {
        use linreg::linear_regression_of;

        let tuples: Vec<(f64, f64)> = self
            .metrics
            .iter()
            .map(|x| (x.timestamp.timestamp() as f64, x.value))
            .collect();
        let lin_reg = linear_regression_of(&tuples);
        let (slope, intercept): (f64, f64) = match lin_reg {
            Ok((slope, _)) if slope >= 0.0 => {
                // If the slope 0, we have a constant value and thus, the forecast it "oo"
                // If the slope is positive (>0), we are gaining for the burst balance.
                // In both cases no sensible forecast until the volumes run out of burst can be
                // computed.
                trace!("No forecast computed for vol {}.", self.volume_id);
                return None;
            }
            Ok((slope, intercept)) => {
                trace!(
                    "Linear regression result for vol {} and {} metric data points: intercept={}, slope={}.",
                    self.volume_id,
                    self.metrics.len(),
                    intercept,
                    slope
                );
                (slope, intercept)
            }
            Err(err) => {
                trace!(
                    "Linear regression for vol {} and {} metric data points failed, because {}",
                    self.volume_id,
                    self.metrics.len(),
                    err.to_string()
                );
                return None;
            }
        };

        let forecast_unix = -1.0 * intercept / slope;
        let forecast = Utc.timestamp(forecast_unix as i64, 0);
        trace!(
            "Forecast when vol {} runs out burst unix timestamp={}, datetime={}",
            self.volume_id,
            forecast_unix,
            forecast
        );

        Some(forecast)
    }
}

#[derive(Debug)]
pub struct BurstBalance {
    pub volume_id: String,
    pub instance_id: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub balance: Option<f64>,
    pub forecast: Option<DateTime<Utc>>,
}

pub fn get_burst_balances<S: Into<Option<Duration>>, T: Into<Option<Filters>>>(
    aws_client_config: &AwsClientConfig,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    period: S,
    filters: T,
) -> Result<Vec<BurstBalance>, Error> {
    let filters: Option<Filters> = filters.into();
    let filters = filters.map(|x| x.into_iter().map(|f| f.into()).collect());
    let instance_ids = get_instances_ids(aws_client_config, filters)?;
    trace!("{:#?}", &instance_ids);

    let filters = vec![AwsFilter {
        name: Some("attachment.instance-id".to_string()),
        values: Some(instance_ids),
    }];
    let volume_infos = get_volumes_info(aws_client_config, filters)?;
    trace!("{:#?}", &volume_infos);

    let vol_atts: Vec<_> = volume_infos
        .into_iter()
        .filter(|x| x.attachments.len() == 1) // Semantically possible, but we can only have 1 attachment, because we queried them via instance ids,
        .map(|x| VolumeAttachment {
            instance_id: x.attachments.into_iter().next().unwrap().instance_id.unwrap(), // Safe, due to filter
            volume_id: x.volume_id,
        })
        .collect();
    trace!("{:#?}", &vol_atts);

    let vols_instances_map: VolInstanceMap = vol_atts.into();

    let vol_ids = vols_instances_map.0.keys().cloned().collect();
    let metric_data = cloudwatch::get_burst_balances(aws_client_config, vol_ids, start, end, period)?;
    trace!("{:#?}", &metric_data);

    let burst_balances = metric_data
        .into_iter()
        .map(|metric| {
            let forecast = metric.forecast_running_out_of_burst();
            let (timestamp, balance) = metric
                .get_last_metric()
                .map(|m| (Some(m.timestamp), Some(m.value)))
                .unwrap_or_else(|| (None, None));
            BurstBalance {
                volume_id: metric.volume_id.clone(),
                instance_id: vols_instances_map
                    .0
                    .get(&metric.volume_id)
                    .map(|x| x.as_ref())
                    .unwrap_or("<unknown>")
                    .to_string(),
                timestamp,
                balance,
                forecast,
            }
        })
        .collect();

    Ok(burst_balances)
}
