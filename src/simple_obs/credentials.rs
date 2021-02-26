// Modified from https://github.com/mozilla/sccache/blob/master/src/simples3/credential.rs

use chrono::{DateTime, Utc, offset, Duration};
use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ObsError>;

#[derive(Debug, Error)]
pub enum ObsError {
    #[error("request error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("chrono error: {0}")]
    Chrono(#[from] chrono::ParseError),
}

#[derive(Clone, Debug)]
pub struct ObsCredentials {
    access: String,
    secret: String,
    security_token: Option<String>,
    expires_at: DateTime<Utc>,
}

impl ObsCredentials {
    pub fn access(&self) -> &str {
        &self.access
    }
    pub fn secret(&self) -> &str {
        &self.secret
    }
    pub fn security_token(&self) -> &Option<String> {
        &self.security_token
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    /// Determine whether or not the credentials are expired.
    fn credentials_are_expired(&self) -> bool {
        // This is a rough hack to hopefully avoid someone requesting creds then sitting on them
        // before issuing the request:
        self.expires_at() < offset::Utc::now() + Duration::seconds(20)
    }
}

/// A trait for types that produce `ObsCredentials`.
#[async_trait]
pub trait ProvideObsCredentials: Send + Sync {
    /// Produce a new `ObsCredentials`.
    async fn credentials(&self) -> Result<ObsCredentials>;
}

/// Provides OBS credentials from a resource's IAM role.
pub struct IamProvider {
    client: reqwest::Client,
}

const OBS_IAM_CREDENTIALS_URL: &str = "http://169.254.169.254/openstack/latest/securitykey";

impl IamProvider {
    pub fn new() -> Self {
        IamProvider {
            client: reqwest::Client::new()
        }
    }
}

#[derive(Deserialize, Debug)]
struct OpenStackResponseDe {
    access: String,
    secret: String,
    #[serde(rename = "securitytoken")]
    security_token: String,
    expires_at: String,
}

#[derive(Deserialize, Debug)]
struct OpenStackResponseDeWrapper {
    credential: OpenStackResponseDe,
}

#[async_trait]
impl ProvideObsCredentials for IamProvider {
    async fn credentials(&self) -> Result<ObsCredentials> {
        use reqwest::header;
        use std::str::FromStr;

        let OpenStackResponseDeWrapper {
            credential: OpenStackResponseDe {
                access, secret, security_token, expires_at
            }
        } = self.client
            .get(OBS_IAM_CREDENTIALS_URL)
            .header(header::CONNECTION, "close")
            .send().await?
            .json().await?;

        Ok(ObsCredentials {
            access,
            secret,
            security_token: Some(security_token),
            expires_at: chrono::DateTime::from_str(expires_at.as_str())?,
        })
    }
}

use std::cell::RefCell;
use tokio::sync::Mutex;

/// Wrapper for ProvideAwsCredentials that caches the credentials returned by the
/// wrapped provider.  Each time the credentials are accessed, they are checked to see if
/// they have expired, in which case they are retrieved from the wrapped provider again.
pub struct AutoRefreshingProvider<P: ProvideObsCredentials> {
    credentials_provider: P,
    cached_credentials: Mutex<RefCell<Option<ObsCredentials>>>,
}

impl<P: ProvideObsCredentials> AutoRefreshingProvider<P> {
    pub fn new(provider: P) -> AutoRefreshingProvider<P> {
        AutoRefreshingProvider {
            cached_credentials: Default::default(),
            credentials_provider: provider,
        }
    }
}

#[async_trait]
impl<P: ProvideObsCredentials> ProvideObsCredentials for AutoRefreshingProvider<P> {
    async fn credentials(&self) -> Result<ObsCredentials> {
        let guard = self.cached_credentials.lock().await;
        let is_invalid = match guard.borrow().as_ref() {
            Some(credentials) => credentials.credentials_are_expired(),
            None => true,
        };
        return if is_invalid {
            match self.credentials_provider.credentials().await {
                Ok(credentials) => {
                    guard.replace(Some(credentials.clone()));
                    Ok(credentials)
                },
                Err(e) => Err(e)
            }
        } else {
            Ok(guard.borrow().as_ref().unwrap().clone())
        }
    }
}
