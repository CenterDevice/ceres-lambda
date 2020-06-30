use aws::ec2::ec2::{Filter, get_instances_ids};
use aws::ec2::ebs::get_volumes_info;
use aws::ec2::cloudwatch::get_burst_balance;

#[derive(Debug)]
struct VolumeAttachment {
    pub instance_id: String,
    pub volume_id: String,
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
    println!("{:#?}", &instance_ids);

    let filters = vec![
        Filter {
            name: Some("attachment.instance-id".to_string()),
            values: Some(instance_ids),
        },
    ];
    let volume_infos = get_volumes_info(filters).expect("Faild to get volumes infos.");
    println!("{:#?}", &volume_infos);

    let vol_atts: Vec<_> = volume_infos
        .into_iter()
        .filter(|x| x.attachments.len() == 1) // Semantically possible, but we can only have 1 attachment, because we queried them via instance ids,
        .map(|x| VolumeAttachment { 
            instance_id: x.attachments.into_iter().next().unwrap().instance_id.unwrap(),
            volume_id: x.volume_id
        })
        .collect();
    println!("{:#?}", &vol_atts);

    let vol_ids = vol_atts.iter().map(|x| x.volume_id.clone()).collect();
    let metric_data = get_burst_balance(vol_ids, "2020-06-30T11:00:00Z".to_string(), "2020-06-30T13:00:00Z".to_string()).expect("Failed to get burst balance.");
    println!("{:#?}", &metric_data);
}
