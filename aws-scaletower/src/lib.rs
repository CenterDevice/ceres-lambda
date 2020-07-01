use aws::ec2::ec2::{Filter, get_instances_ids};
use aws::ec2::ebs::get_volumes_info;
use aws::cloudwatch::{BurstBalanceMetricData, Metric, get_burst_balance};
use chrono::prelude::*;
use chrono::Duration;
use log::{debug, trace};
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

impl From<Vec<VolumeAttachment>> for  VolInstanceMap {
    fn from(xs: Vec<VolumeAttachment>) -> Self {
        let map: HashMap<String, String> = xs
            .into_iter()
            .map(|x| (x.volume_id, x.instance_id))
            .collect();

        VolInstanceMap(map)
    }
}

trait RunningOutOfBurstsForecast {
    fn forecast_running_out_of_burst(&self) -> Option<DateTime<Utc>>;
}

impl RunningOutOfBurstsForecast for BurstBalanceMetricData {
    fn forecast_running_out_of_burst(&self) -> Option<DateTime<Utc>> {
        use linreg::linear_regression_of;

        let tuples: Vec<(f64, f64)> = self.metrics.iter().map(|x| (x.timestamp.timestamp() as f64, x.value)).collect();
        let lin_reg = linear_regression_of(&tuples);
        let (slope, intercept): (f64, f64) = match lin_reg {
            Ok((slope, intercept)) => {
                trace!("Linear regression result for vol {} and {} metric data points: intercept={}, slope={}.", self.volume_id, self.metrics.len(), intercept, slope);
                (slope, intercept)
            }, 
            Err(err) => {
                trace!("Linear regression for vol {} and {} metric data points failed, because {}", self.volume_id, self.metrics.len(), err.to_string());
                return None
            }
        };

        if slope >= 0.0 {
            // If the slope 0, we have a constant value and thus, the forecast it "oo"
            // If the slope is positive (>0), we are gaining for the burst balance.
            // In both cases no sensible forecast until the volumes run out of burst can be
            // computed.
            trace!("No forecast computed for vol {}.", self.volume_id);
            return None
        }

        let forecast_unix = -1.0 * intercept / slope;
        let forecast = Utc.timestamp(forecast_unix as i64, 0);
        trace!("Forecast when vol {} runs out burst unix timestamp={}, datetime={}", self.volume_id, forecast_unix, forecast);

        Some(forecast)
    }
}

pub fn do_stuff<T: Into<Option<Duration>>>(start: DateTime<Utc>, end: DateTime<Utc>, period: T) {
    let filters = vec![
        Filter {
            name: Some("instance-state-name".to_string()),
            values: Some(vec!["running".to_string()]),
        },
        Filter {
            name: Some("tag:Name".to_string()),
            values: Some(vec!["centerdevice-ec2-document_server*".to_string()]),
        },
    ];
    let instance_ids = get_instances_ids(filters).expect("Failed to get instance ids.");
    debug!("{:#?}", &instance_ids);

    let filters = vec![
        Filter {
            name: Some("attachment.instance-id".to_string()),
            values: Some(instance_ids),
        },
    ];
    let volume_infos = get_volumes_info(filters).expect("Failed to get volumes infos.");
    debug!("{:#?}", &volume_infos);

    let vol_atts: Vec<_> = volume_infos
        .into_iter()
        .filter(|x| x.attachments.len() == 1) // Semantically possible, but we can only have 1 attachment, because we queried them via instance ids,
        .map(|x| VolumeAttachment { 
            instance_id: x.attachments.into_iter().next().unwrap().instance_id.unwrap(),
            volume_id: x.volume_id
        })
        .collect();
    debug!("{:#?}", &vol_atts);

    let vols_instances_map: VolInstanceMap = vol_atts.into();

    let vol_ids = vols_instances_map.0.keys().cloned().collect();
    let metric_data = get_burst_balance(vol_ids, start, end, period).expect("Failed to get burst balance.");
    debug!("{:#?}", &metric_data);

    for m in metric_data {
        if let Some(last) = m.get_last_metric() {
            let instance_id = vols_instances_map.0.get(&m.volume_id).map(|x| x.as_ref()).unwrap_or("<unknown>");
            println!("Burst balance for vol {} attached to instance {} at {} is {}", &m.volume_id, &instance_id, &last.timestamp, &last.value);
            let run_out = m.forecast_running_out_of_burst();
            let time_left = run_out.map(|x| x - Utc::now()).map(|x| x.num_minutes());
            println!("   -> running out of burst balance at {:?} witch is in {:?} min", run_out, time_left);
        } else {
            println!("No metric values found for vol {}", m.volume_id);
        }
    }
}
