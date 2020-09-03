use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use failure::Fail;
use log::{debug, info, trace};
use reqwest::{Method, RequestBuilder, StatusCode};
use ring::hmac;
use std::io::Read;

/// Result of an attempt to send meta data or a metric datum
pub type DuoResult<T> = Result<T, DuoError>;

/// Errors which may occur while sending either meta data or metric data.
#[derive(Debug, Fail)]
pub enum DuoError {
    /// Failed to create JSON.
    #[fail(display = "failed to parse JSON")]
    JsonParseError,
    /// Failed to create Client
    #[fail(display = "failed create client because {}", _0)]
    ClientError(String),
    /// Failed to send to Duo
    #[fail(display = "failed to send to Duo because {}", _0)]
    SendError(String),
    /// Failed to read from Duo
    #[fail(display = "failed to process Duo response because {}", _0)]
    ReceiveError(String),
}

/// Encapsulates Duo server connection.
#[derive(Debug)]
pub struct DuoClient {
    api_host_name: String,
    integration_key: String,
    secret_key: String,
    client: Arc<reqwest::Client>,
}

impl DuoClient {
    /// Creates a new DuoClient.
    pub fn new<S: Into<String>, T: Into<String>, U: Into<String>>(api_host_name: S, integration_key: T, secret_key: U) -> DuoResult<DuoClient> {
        DuoClient::with_timeout(api_host_name, integration_key, secret_key, 5)
    }

    /// Creates a new DuoClient with specific timeout
    pub fn with_timeout<S: Into<String>, T: Into<String>, U: Into<String>>(api_host_name: S, integration_key: T, secret_key: U, timeout_sec: u64) -> DuoResult<DuoClient> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_sec))
            .build()
            .map_err(|e| DuoError::ClientError(format!("failed to build http client because {}", e.to_string())))?;

        Ok(DuoClient {
            api_host_name: api_host_name.into(),
            integration_key: integration_key.into(),
            secret_key: secret_key.into(),
            client: Arc::new(client),
        })
    }

    fn send_to_duo_api(&self, path: &str, expected: StatusCode) -> DuoResult<()> {
        let uri = format!("https://{}{}", self.api_host_name, path);

        let req = self.client
            .get(&uri)
            //.header("Content-Type", "application/x-www-form-urlencoded");
        ;
        let req = self.sign_req(req, Method::GET, path);
        debug!("Request: '{:?}'", req);

        let res = req.send();
        match res {
            Ok(ref response) if response.status() == expected => Ok(()),
            Ok(response) => Err(DuoError::ReceiveError(format!("{}", response.status()))),
            Err(err) => Err(DuoError::SendError(format!("{}", err))),
        }
    }

    fn sign_req(&self, req: RequestBuilder, method: Method, path: &str) -> RequestBuilder {
        let now = Local::now().to_rfc2822();
        let method = method.as_str();
        let api_host_name = self.api_host_name.to_lowercase();
        let params = "";
        let canon = [now.as_str(), method, api_host_name.as_str(), path, params];

        let basic_auth = basic_auth_for_canon(&self.integration_key, &self.secret_key, &canon);

        req
            .header("Date", &now)
            .header("Authorization", &basic_auth)
    }
}

fn basic_auth_for_canon(integration_key: &str, secret_key: &str, canon: &[&str]) -> String {
    let canon = canon.join("\n");
    trace!("Canon: '{}'", canon);

    let s_key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret_key.as_bytes());
    let mut s_ctx = hmac::Context::with_key(&s_key);
    s_ctx.update(canon.as_bytes());
    let sig = s_ctx.sign();
    let auth = format!("{}:{}",
                       integration_key,
                       hex::encode(sig.as_ref())
    );
    trace!("Auth: '{}'", auth);

    let basic_auth = format!("Basic {}", base64::encode(&auth));
    trace!("Basic Auth: '{}'", basic_auth);

    basic_auth
}

pub trait Duo {
    fn get_users(&self) -> DuoResult<()>;
}

impl Duo for DuoClient {
    fn get_users(&self) -> DuoResult<()> {
        let res = self.send_to_duo_api("/admin/v1/users", StatusCode::OK);

        eprintln!("Res: {:?}", res);
        res
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use spectral::prelude::*;

    use super::*;


    #[test]
    fn basic_auth_for_canon_test() {
        testing::setup();

        let expected = "Basic RElXSjhYNkFFWU9SNU9NQzZUUTE6ZWE4MmExMzcyMGI5ZDE5MDIxNWNjODkxNzljMmNiMTcxZDg2MDdiMw==";

        let integration_key = "DIWJ8X6AEYOR5OMC6TQ1";
        let secret_key = "Zh5eGmUq9zpfQnyUIu5OL9iWoMMv5ZNmk3zLJ4Ep";

        let now = "Tue, 21 Aug 2012 17:29:18 -0000";
        let method = Method::POST.as_str();
        let host = "api-XXXXXXXX.duosecurity.com".to_lowercase();
        let path = "/admin/v1/users";
        let params = "";
        let canon = [now, method, host.as_str(), path, params];

        let basic_auth = basic_auth_for_canon(integration_key, secret_key, &canon);

        assert_that(&basic_auth.as_str()).is_equal_to(&expected);

    }

    #[test]
    fn get_users() {
        testing::setup();

        let api_host_name = env::var_os("DUO_API_HOST_NAME")
            .expect("Environment variable 'DUO_API_HOST_NAME' is not set.")
            .to_string_lossy().to_string();
        let integration_key = env::var_os("DUO_INTEGRATION_KEY")
            .expect("Environment variable 'DUO_INTEGRATION_KEY' is not set.")
            .to_string_lossy().to_string();
        let secret_key = env::var_os("DUO_SECRET_KEY")
            .expect("Environment variable 'DUO_SECRET_KEY' is not set.")
            .to_string_lossy().to_string();

        let client = DuoClient::new(api_host_name, integration_key, secret_key)
            .expect("Failed to create Duo Client");

        let response = client.get_users();

        assert_that(&response).is_ok();
        let response = response.expect("Request failed");
    }
}