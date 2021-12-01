use crate::config::CenterDeviceHealthConfig;
use failure::Error;
use log::{info, trace};
use reqwest::header;
use reqwest::{Client as ReqwestClient, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

const REQ_TIMEOUT: u64 = 5;

pub const ENDPOINTS: &[&str] = &["admin", "api", "app", "auth", "public", "sales", "upload"];

#[derive(Debug, Deserialize, Serialize)]
pub struct HealthCheck {
    pub service: String,
    pub result: HealthCheckResult,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum HealthCheckResult {
    Ok(HealthSamples),
    Failed(String),
}

pub type HealthSamples = HashMap<String, HealthSample>;

#[derive(Debug, Deserialize, Serialize)]
pub struct HealthSample {
    #[serde(rename = "timeStamp")]
    pub time_stamp: Option<i64>,
    #[serde(rename = "samplingTime")]
    pub stampling_time: Option<usize>,
    #[serde(rename = "value")]
    pub healthy: bool,
}

pub fn health_check(config: &CenterDeviceHealthConfig) -> Result<Vec<HealthCheck>, Error> {
    info!("Checking Health");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQ_TIMEOUT))
        .build()
        .map_err(|e| failure::err_msg(format!("failed to build http client because {}", e.to_string())))?;

    let mut healthchecks: Vec<HealthCheck> = Vec::new();
    for service in ENDPOINTS {
        let url = format!("https://{}.{}/healthcheck", service, &config.base_domain);
        let hc = query_health(&client, service, &url)?;
        healthchecks.push(hc);
    }
    trace!("{:#?}", healthchecks);

    Ok(healthchecks)
}

fn query_health(client: &ReqwestClient, service: &'static str, url: &str) -> Result<HealthCheck, Error> {
    trace!("Querying health for {}", url);

    let res = client.get(url).header(header::CONNECTION, "close").send();

    match res {
        Ok(mut response) if response.status() == StatusCode::OK => {
            let text = response
                .text()
                .map_err(|_| failure::err_msg("failed to read response body"))?;
            trace!("Answer: '{}'", text);
            let data: HealthCheck = serde_json::from_str::<HealthSamples>(&text)
                .map_err(|e| failure::err_msg(format!("failed to parse response: {}", e)))
                .map(|samples| HealthCheck {
                    service: service.to_string(),
                    result: HealthCheckResult::Ok(samples),
                })
                .or_else::<Error, _>(|e| {
                    Ok(HealthCheck {
                        service: service.to_string(),
                        result: HealthCheckResult::Failed(e.to_string()),
                    })
                })?;

            Ok(data)
        }
        Ok(response) => Ok(HealthCheck {
            service: service.to_string(),
            result: HealthCheckResult::Failed(format!("Unexpected status code (200): {}", response.status())),
        }),
        Err(err) if err.is_timeout() => Ok(HealthCheck {
            service: service.to_string(),
            result: HealthCheckResult::Failed(format!("Timeout ({} sec): {}", REQ_TIMEOUT, err)),
        }),
        Err(err) => Err(failure::err_msg(err)),
    }
}
