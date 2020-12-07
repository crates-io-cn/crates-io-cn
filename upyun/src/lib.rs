#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use base64::encode as b64enc;
use chrono::{DateTime, Utc};

use bytes::Bytes;
use reqwest::{header, Method, RequestBuilder, StatusCode};
use serde_json::Value;

pub mod error;
mod provider;
use error::{Error, Result, UpyunError};
pub use provider::Provider;

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}

#[derive(Debug, Clone)]
pub struct Upyun {
    operator: Operator,
    provider: Provider,
}

#[derive(Debug, Clone)]
pub struct Operator {
    name: &'static str,
    passwd: &'static str,
    authorization: String,
}

impl Operator {
    pub fn new(name: &'static str, passwd: &'static str) -> Self {
        let authorization = format!("Basic {}", b64enc(format!("{}:{}", name, passwd)));
        Self {
            name,
            passwd,
            authorization,
        }
    }

    pub fn request<P>(
        &self,
        method: Method,
        provider: Provider,
        path: P,
        date: Option<DateTime<Utc>>,
    ) -> RequestBuilder
    where
        P: AsRef<str>,
    {
        let url = format!("{}{}", provider.as_ref(), path.as_ref());
        debug!("{}", url);
        let req = CLIENT
            .request(method, &url)
            .header(header::USER_AGENT, "upyun-client (crates-io.cn)")
            .header(header::AUTHORIZATION, &self.authorization);
        if let Some(date) = date {
            req.header(header::DATE, format_gmt(date))
        } else {
            req.header(header::DATE, format_gmt(Utc::now()))
        }
    }
}

impl Upyun {
    pub fn new(operator: Operator) -> Self {
        Self {
            operator,
            provider: Provider::Auto,
        }
    }

    pub fn set_provider(&mut self, provider: Provider) {
        self.provider = provider;
    }

    pub async fn put_file<B, K>(&self, bucket: B, key: K, content: Bytes) -> Result<()>
    where
        B: AsRef<str>,
        K: AsRef<str>,
    {
        let path = format!("/{}/{}", bucket.as_ref(), key.as_ref());
        let resp = self
            .operator
            .request(Method::PUT, self.provider, path, None)
            .body(content)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(()),
            _ => {
                let err: Value = resp.json().await.unwrap();
                Err(Error::Upyun(UpyunError::from(
                    err["code"].as_u64().unwrap_or(0),
                )))
            }
        }
    }
}

fn format_gmt(date: DateTime<Utc>) -> String {
    format!("{}", date.format("%a, %d %b %Y %H:%M:%S GMT"))
}
