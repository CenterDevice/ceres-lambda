use chrono::Timelike;
use failure::Fail;
use log::{debug, info};
use reqwest::{StatusCode, Url};
use serde_derive::Serialize;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

/// Result of an attempt to send meta data or a metric datum
pub type BosunResult = Result<(), BosunError>;

/// Errors which may occur while sending either meta data or metric data.
#[derive(Debug, Fail)]
pub enum BosunError {
    /// Failed to create JSON.
    #[fail(display = "failed to parse JSON")]
    JsonParseError,
    /// Failed to send to Bosun
    #[fail(display = "failed to send to Bosun because '{}'", _0)]
    EmitError(String),
    /// Failed to read from Bosun
    #[fail(display = "failed to process Bosun response because '{}'", _0)]
    ReceiveError(String),
}

/// Encapsulates Bosun server connection.
#[derive(Debug)]
pub struct BosunClient {
    /// `<HOSTNAME|IP ADDR>:<PORT>`
    pub host: String,
    /// Timeout for http request connection
    pub timeout: u64,
}

pub trait Bosun {
    fn emit_metadata(&self, metadata: &Metadata) -> BosunResult;
    fn emit_datum(&self, datum: &Datum) -> BosunResult;
    fn set_silence(&self, silence: &Silence) -> BosunResult;
}

impl Bosun for BosunClient {
    fn emit_metadata(&self, metadata: &Metadata) -> BosunResult {
        let timeout = 5u64;
        let encoded = metadata.to_json()?;
        let res = BosunClient::send_to_bosun_api(
            &self.host,
            "/api/metadata/put",
            &encoded,
            timeout,
            StatusCode::NO_CONTENT,
        );
        info!(
            "Sent medata '{:?}' to '{:?}' with result: '{:?}'.",
            encoded, self.host, res
        );

        res
    }

    fn emit_datum(&self, datum: &Datum) -> BosunResult {
        let timeout = 5u64;
        let encoded = datum.to_json()?;
        let res = BosunClient::send_to_bosun_api(&self.host, "/api/put", &encoded, timeout, StatusCode::NO_CONTENT);
        info!(
            "Sent datum '{:?}' to '{:?}' with result: '{:?}'.",
            encoded, &self.host, res
        );

        res
    }

    fn set_silence(&self, silence: &Silence) -> BosunResult {
        let timeout = 5u64;
        let json = serde_json::to_string(silence)
            //TODO: Use context to carry original error on
            .map_err(|_| BosunError::JsonParseError)?;
        let res = BosunClient::send_to_bosun_api(&self.host, "/api/silence/set", &json, timeout, StatusCode::OK);
        info!(
            "Set silence '{:?}' at '{:?}' with result: '{:?}'.",
            json, &self.host, res
        );

        res
    }
}
impl BosunClient {
    /// Creates a new BosunClient.
    pub fn new(host: &str, timeout: u64) -> BosunClient {
        BosunClient {
            host: host.to_string(),
            timeout,
        }
    }

    fn send_to_bosun_api<T: Into<Option<u64>>>(
        host: &str,
        path: &str,
        json: &str,
        timeout: T,
        expected: StatusCode,
    ) -> BosunResult {
        let uri = if host.starts_with("http") {
            format!("{}{}", host, path)
        } else {
            format!("http://{}{}", host, path)
        };
        let url = Url::parse(&uri).unwrap();
        let timeout = timeout.into().unwrap_or(5); // Default timeout is set to 5 sec.

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()
            .map_err(|e| BosunError::EmitError(format!("failed to build http client because {}", e.to_string())))?;

        let body: Vec<u8> = json.as_bytes().into();

        let req = client
            .post(&uri)
            .header("Content-Type", "application/json; charset=utf-8")
            .body(body);

        // Only add basic auth, if username and password are set
        let req = if url.has_authority() {
            let username = url.username();
            let password = url.password();
            match (username, password) {
                (u, Some(_)) if u.len() > 0 => req.basic_auth(username, password),
                _ => req,
            }
        } else {
            req
        };

        let res = req.send();

        match res {
            Ok(ref response) if response.status() == expected => Ok(()),
            Ok(response) => Err(BosunError::ReceiveError(format!("{}", response.status()))),
            Err(err) => Err(BosunError::EmitError(format!("{}", err))),
        }
    }
}

#[derive(Debug, Serialize)]
/// Represents metric meta data.
pub struct Metadata<'a> {
    /// Metric name
    pub metric: &'a str,
    /// Metric rate type: [gauge, counter rate]
    pub rate: &'a str,
    /// Metric unit
    pub unit: &'a str,
    /// Metric description
    pub description: &'a str,
}

// TODO: Add check for rate type: [gauge, counter rate]
impl<'a> Metadata<'a> {
    /// Creates new metric meta data.
    pub fn new(metric: &'a str, rate: &'a str, unit: &'a str, description: &'a str) -> Metadata<'a> {
        Metadata {
            metric: metric,
            rate: rate,
            unit: unit,
            description: description,
        }
    }

    pub fn to_json(&self) -> Result<String, BosunError> {
        let mut metadata = [HashMap::new(), HashMap::new(), HashMap::new()];
        metadata[0].insert("metric", self.metric);
        metadata[0].insert("name", "unit");
        metadata[0].insert("value", self.unit);
        metadata[1].insert("metric", self.metric);
        metadata[1].insert("name", "rate");
        metadata[1].insert("value", self.rate);
        metadata[2].insert("metric", self.metric);
        metadata[2].insert("name", "desc");
        metadata[2].insert("value", self.description);

        let json = serde_json::to_string(&metadata)
            //TODO: Use context to carry original error on
            .map_err(|_| BosunError::JsonParseError)?;
        debug!("Metadata::to_json '{:?}', '{:?}'", &self, json);

        Ok(json)
    }
}

/// Metric tags equivalent to Rust's `HashMap<String, String>`
pub type Tags = HashMap<String, String>;

/// Represents a metric datum.
#[derive(Debug, Serialize)]
pub struct Datum<'a> {
    /// Metric name
    pub metric: &'a str,
    /// Unix timestamp in either _s_ or _ms_
    pub timestamp: i64,
    /// Value as string representation
    pub value: &'a str,
    /// Tags for this metric datum
    pub tags: &'a Tags,
}

impl<'a> Datum<'a> {
    /// Creates a new metric datum with a specified timestamp in ms.
    pub fn new(
        metric: &'a str,
        timestamp: i64,
        value: &'a str,
        // TODO: make me use refs
        tags: &'a Tags,
    ) -> Datum<'a> {
        Datum {
            metric: metric,
            timestamp: timestamp,
            value: value,
            tags: tags,
        }
    }
    /// Creates a new metric datum with timestamp _now_.
    pub fn now(
        metric: &'a str,
        value: &'a str,
        // TODO: make me use refs
        tags: &'a Tags,
    ) -> Datum<'a> {
        Datum {
            metric: metric,
            timestamp: now_in_ms(),
            value: value,
            tags: tags,
        }
    }

    pub fn to_json(&self) -> Result<String, BosunError> {
        let json = serde_json::to_string(&self)
            //TODO: Use context to carry original error on
            .map_err(|_| BosunError::JsonParseError)?;
        debug!("Datum::to_json '{:?}', '{:?}'.", &self, json);

        Ok(json)
    }
}

/// Returns Unix timestamp in ms.
pub fn now_in_ms() -> i64 {
    let now = chrono::Local::now();
    now.timestamp() * 1000 + (now.nanosecond() / 1_000_000) as i64
}

#[derive(Debug, Serialize)]
/* cf. https://github.com/bosun-monitor/bosun/blob/master/models/silence.go#L12. 28.11.2018
    Start, End time.Time
    Alert      string
    Tags       opentsdb.TagSet
    TagString  string
    Forget     bool
    User       string
    Message    string
*/
// {"duration":"24h","tags":"host=doc-server-i-lukas","forget":null,"message":"Server has been terminated by ASG."}
pub struct Silence {
    duration: String,
    tags: String,
    /// Bosun does not like bool, only Strings "true" or "false"
    forget: String,
    user: String,
    message: String,
    /// Bosun does not like bool, only Strings "true" or "false"
    confirm: String,
}

impl Silence {
    pub fn host(host: &str, duration: &str) -> Silence {
        Silence {
            // TODO: These parameters should be config parameters
            duration: duration.to_string(),
            tags: format!("host={}", host),
            forget: "true".to_string(),
            user: "kevin.lambda".to_string(),
            message: "Host has been terminated by ASG.".to_string(),
            confirm: "true".to_string(),
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[derive(PartialEq, Eq, Debug)]
    pub struct BosunMockClient {
        pub stats: Rc<RefCell<HashMap<&'static str, u32>>>
    }

    impl Default for BosunMockClient {
        fn default() -> BosunMockClient {
            BosunMockClient {
                stats: Rc::new(RefCell::new(HashMap::new())),
            }
        }
    }

    impl BosunMockClient {
        fn inc(&self, key: &'static str) {
            let mut stats = self.stats.borrow_mut();
            let count = stats.get(key).unwrap_or(&0) + 1;
            stats.insert(key, count);
        }
    }

    impl Bosun for BosunMockClient {
        fn emit_metadata(&self, _: &Metadata) -> BosunResult {
            self.inc("metadata");
            Ok(())
        }

        fn emit_datum(&self, _: &Datum) -> BosunResult {
            self.inc("datum");
            Ok(())
        }

        fn set_silence(&self, _: &Silence) -> BosunResult {
            self.inc("set_silence");
            Ok(())
        }
    }

    #[derive(PartialEq, Eq, Debug)]
    pub struct BosunCallStats {
        pub metadata_count: u32,
        pub datum_count: u32,
        pub set_silence_count: u32,
    }

    impl BosunCallStats {
        pub fn new(metadata_count: u32, datum_count: u32, set_silence_count: u32) -> BosunCallStats {
            BosunCallStats {
                metadata_count,
                datum_count,
                set_silence_count,
            }
        }
    }

    impl BosunMockClient {
        pub fn to_stats(self) -> BosunCallStats {
            let stats = self.stats.borrow_mut();

            BosunCallStats {
                metadata_count: *stats.get("metadata").unwrap_or(&0),
                datum_count: *stats.get("datum").unwrap_or(&0),
                set_silence_count: *stats.get("set_silence").unwrap_or(&0),
            }
        }
    }
}
