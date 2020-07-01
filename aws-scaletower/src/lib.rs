use aws::ec2::ec2::{Filter, get_instances_ids};
use aws::ec2::ebs::get_volumes_info;
use aws::cloudwatch::{BurstBalanceMetricData, Metric, get_burst_balance};
use log::debug;
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

pub fn do_stuff() {
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
    let volume_infos = get_volumes_info(filters).expect("Faild to get volumes infos.");
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
    let metric_data = get_burst_balance(vol_ids, "2020-07-01T06:00:00Z".to_string(), "2020-07-01T06:30:00Z".to_string()).expect("Failed to get burst balance.");
    debug!("{:#?}", &metric_data);

    for m in metric_data {
        if let Some(last) = m.get_last_metric() {
            let instance_id = vols_instances_map.0.get(&m.volume_id).map(|x| x.as_ref()).unwrap_or("<unknown>");
            println!("Burst balance for vol {} attached to instance {} at {} is {}", &m.volume_id, &instance_id, &last.timestamp, &last.value);
        } else {
            println!("No metric values found for vol {}", m.volume_id);
        }
    }
}
