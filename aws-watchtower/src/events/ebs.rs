use crate::aws::{self, AwsError};
use crate::bosun::{self, Bosun, Datum, Tags};
use crate::config::FunctionConfig;
use failure::Error;
use lambda_runtime::Context;
use log::info;
use serde_derive::Deserialize;
use serde_json;
use std::fmt;

// https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ebs-cloud-watch-events.html
// {
//    "version": "0",
//    "id": "01234567-0123-0123-0123-012345678901",
//    "detail-type": "EBS Volume Notification",
//    "source": "aws.ec2",
//    "account": "012345678901",
//    "time": "yyyy-mm-ddThh:mm:ssZ",
//    "region": "us-east-1",
//    "resources": [
//       "arn:aws:ec2:us-east-1:012345678901:volume/vol-01234567"
//    ],
//    "detail": {
//       "result": "available",
//       "cause": "",
//       "event": "createVolume",
//       "request-id": "01234567-0123-0123-0123-0123456789ab"
//    }
// }
#[derive(Debug, Deserialize)]
pub struct VolumeEvent {
    pub version: String,
    pub id: String,
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub account: String,
    pub time: String,
    pub region: String,
    pub resources: Vec<String>,
    pub detail: VolumeEventDetail,
}

#[derive(Debug, Deserialize)]
pub struct VolumeEventDetail {
    event: VolumeEventType,
    result: VolumeResult,
    cause: String,
    #[serde(rename = "request-id")]
    request_id: String,
}

#[derive(PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeEventType {
    AttachVolume,
    CopySnapshot,
    CreateSnapshot,
    CreateVolume,
    DeleteVolume,
    ReattachVolume,
    ShareSnapshot,
}

impl fmt::Display for VolumeEventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output = match self {
            VolumeEventType::AttachVolume => "attach_volume",
            VolumeEventType::CopySnapshot => "copy_snapshot",
            VolumeEventType::CreateSnapshot => "create_snapshot",
            VolumeEventType::CreateVolume => "create_volume",
            VolumeEventType::DeleteVolume => "delete_volume",
            VolumeEventType::ReattachVolume => "reattach_volume",
            VolumeEventType::ShareSnapshot => "share_snapshot",
        };
        write!(f, "{}", output)
    }
}

#[derive(PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeResult {
    Available,
    Deleted,
    Failed,
    Succeeded,
}

impl fmt::Display for VolumeResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output = match self {
            VolumeResult::Available => "available",
            VolumeResult::Deleted => "deleted",
            VolumeResult::Failed => "failed",
            VolumeResult::Succeeded => "succeeded",
        };
        write!(f, "{}", output)
    }
}

pub fn handle<T: Bosun>(event: VolumeEvent, _: &Context, _config: &FunctionConfig, bosun: &T) -> Result<(), Error> {
    info!("Received VolumeEvent {:?}.", event);

    let change_value = match &event.detail.event {
        VolumeEventType::CreateVolume if event.detail.result == VolumeResult::Available => 1,
        VolumeEventType::DeleteVolume if event.detail.result == VolumeResult::Deleted => -1,
        _ => 0,
    };

    let mut tags = Tags::new();
    tags.insert("event".to_string(), event.detail.event.to_string());
    tags.insert("result".to_string(), event.detail.result.to_string());
    let value = change_value.to_string();
    let datum = Datum::now(bosun::METRIC_EBS_VOLUME_EVENT, &value, &tags);
    bosun.emit_datum(&datum)?;

    if event.detail.event == VolumeEventType::CreateVolume {
        let value = if event.detail.result == VolumeResult::Available {
            0
        } else {
            1
        };

        let volume_arn = event.resources.first()
            .ok_or_else(|| Error::from(AwsError::GeneralError("no volume ids found in event")))?;
        let volume_info = aws::ec2::ebs::get_volume_info_by_arn(volume_arn.to_string())?;
        info!("Details for '{}' created volume: '{:?}'",
            if value == 0 {"successfully"} else {"unsucessfully"},
            volume_info
        );

        let mut tags = Tags::new();
        tags.insert("encrypted".to_string(), volume_info.encrypted.to_string());
        let value = value.to_string();
        let datum = Datum::now(bosun::METRIC_EBS_VOLUME_CREATION_RESULT, &value, &tags);
        bosun.emit_datum(&datum)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use spectral::prelude::*;

    #[test]
    fn test_deserialize_create_volume_event_result_is_available() {
        let json = r#"{
   "version": "0",
   "id": "01234567-0123-0123-0123-012345678901",
   "detail-type": "EBS Volume Notification",
   "source": "aws.ec2",
   "account": "012345678901",
   "time": "yyyy-mm-ddThh:mm:ssZ",
   "region": "us-east-1",
   "resources": [
      "arn:aws:ec2:us-east-1:012345678901:volume/vol-01234567"
   ],
   "detail": {
      "result": "available",
      "cause": "",
      "event": "createVolume",
      "request-id": "01234567-0123-0123-0123-0123456789ab"
   }
}"#;

        let event: Result<VolumeEvent, _> = serde_json::from_str(json);

        assert_that(&event).is_ok();
    }

    #[test]
    fn test_deserialize_create_volume_event_result_is_failed() {
        let json = r#"{
   "version": "0",
   "id": "01234567-0123-0123-0123-012345678901",
   "detail-type": "EBS Volume Notification",
   "source": "aws.ec2",
   "account": "012345678901",
   "time": "yyyy-mm-ddThh:mm:ssZ",
   "region": "us-east-1",
   "resources": [
      "arn:aws:ec2:us-east-1:012345678901:volume/vol-01234567"
   ],
   "detail": {
      "result": "failed",
      "cause": "arn:aws:kms:sa-east-1:0123456789ab:key/01234567-0123-0123-0123-0123456789ab is pending import.",
      "event": "createVolume",
      "request-id": "01234567-0123-0123-0123-0123456789ab"
   }
}"#;

        let event: Result<VolumeEvent, _> = serde_json::from_str(json);

        assert_that(&event).is_ok();
    }
}