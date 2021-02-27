// Modified from https://github.com/mozilla/sccache/blob/master/src/simples3/s3.rs
use core::fmt;

use bytes::Bytes;
use hmac::{Hmac, NewMac, Mac};
use sha1::Sha1;
use reqwest::Response;

use super::credentials::*;

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
/// Whether or not to use SSL.
pub enum Ssl {
    /// Use SSL.
    Yes,
    /// Do not use SSL.
    No,
}

fn base_url(endpoint: &str, ssl: Ssl) -> String {
    format!(
        "{}://{}/",
        match ssl {
            Ssl::Yes => "https",
            Ssl::No => "http",
        },
        endpoint
    )
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut hmac = Hmac::<Sha1>::new_varkey(key).expect("HMAC can take key of any size");
    hmac.update(data);
    hmac.finalize().into_bytes().as_slice().to_vec()
}

fn signature(string_to_sign: &str, signing_key: &str) -> String {
    let s = hmac(signing_key.as_bytes(), string_to_sign.as_bytes());
    base64::encode(&s)
}

/// An Obs bucket.
pub struct Bucket {
    name: String,
    base_url: String,
    host: String,
    client: reqwest::Client,
}

impl fmt::Display for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bucket(name={}, base_url={})", self.name, self.base_url)
    }
}

impl Bucket {
    pub fn new(name: &str, endpoint: &str, ssl: Ssl) -> Bucket {
        let base_url = base_url(&endpoint, ssl);
        Bucket {
            name: name.to_owned(),
            base_url,
            host: format!("{}.{}", &name, &endpoint),
            client: reqwest::Client::new(),
        }
    }

    pub async fn put(&self, key: &str, content: Bytes, creds: &ObsCredentials) -> Result<Response> {
        use chrono::Utc;
        use reqwest::header;

        let url = format!("{}{}", self.base_url, key);
        debug!("PUT {}", url);
        let request = self.client.put(&url);

        let content_type = "application/octet-stream";
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let mut canonical_headers = String::new();
        let request = if let Some(ref token) = creds.security_token() {
            canonical_headers
                .push_str(format!("{}:{}\n", "x-obs-security-token", token).as_ref());
            request.header("x-obs-security-token", token)
        } else {
            request
        };

        let auth = self.auth(
            "PUT",
            &date,
            key,
            "",
            &canonical_headers,
            content_type,
            creds,
        );
        let request = request.header(header::DATE, date)
            .header(header::HOST, &self.host)
            .header(header::CONTENT_TYPE, content_type)
           .header(header::CONTENT_LENGTH, content.len())
           .header(header::AUTHORIZATION, auth)
           .body(content);

        let result = request.send().await?;
        let headers = result.headers();
        let request_id = headers.get("x-obs-request-id").map_or_else(|| "null", |h| h.to_str().unwrap_or("invalid"));
        let obs_id = headers.get("x-obs-id-2").map_or_else(|| "null", |h| h.to_str().unwrap_or("invalid"));
        trace!("obs result: [{}], x-obs-request-id: {:?}, x-obs-id-2: {:?}",
               result.status(),
               request_id,
               obs_id
        );
        Ok(result)
    }

    /// https://support.huaweicloud.com/api-obs/obs_04_0010.html
    ///
    /// StringToSign definition:
    /// ```text
    /// StringToSign = HTTP-Verb + "\n"
    /// + Content-MD5 + "\n"
    /// + Content-Type + "\n"
    /// + Date + "\n"
    /// + CanonicalizedHeaders + CanonicalizedResource
    /// ```
    ///
    /// CanonicalizedHeaders definition:
    /// 1. filter all header starts with `x-obs-`, convert to lowercase
    /// 2. sort by dictionary order
    /// 3. append with `key:value\n`, concat duplicate key-value with `,` (example:`key:value1,value2\n`)
    ///
    /// Signature definition:
    /// ```text
    /// Signature = Base64( HMAC-SHA1( SecretAccessKeyID, UTF-8-Encoding-Of( StringToSign ) ) )
    /// ```
    ///
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn auth(
        &self,
        verb: &str,
        date: &str,
        path: &str,
        md5: &str,
        headers: &str,
        content_type: &str,
        creds: &ObsCredentials,
    ) -> String {
        let string = format!(
            "{verb}\n{md5}\n{ty}\n{date}\n{headers}{resource}",
            verb = verb,
            md5 = md5,
            ty = content_type,
            date = date,
            headers = headers,
            resource = format!("/{}/{}", self.name, path)
        );
        let signature = signature(&string, creds.secret());
        format!("OBS {}:{}", creds.access(), signature)
    }
}