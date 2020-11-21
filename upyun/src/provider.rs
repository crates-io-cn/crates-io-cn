use std::convert::From;

use reqwest::Url;

#[derive(Debug, Clone, Copy)]
pub enum Provider {
    Auto,
    ChinaNet,
    Unicom,
    ChinaMobile,
}

impl From<Provider> for &'static str {
    fn from(provider: Provider) -> &'static str {
        match provider {
            Provider::Auto => AUTO,
            Provider::ChinaNet => CHINA_NET,
            Provider::Unicom => UNICOM,
            Provider::ChinaMobile => CHINA_MOBILE,
        }
    }
}

impl From<Provider> for Url {
    fn from(provider: Provider) -> Url {
        Url::parse(provider.as_ref()).unwrap()
    }
}

impl AsRef<str> for Provider {
    fn as_ref(&self) -> &'static str {
        self.clone().into()
    }
}

static AUTO: &str = "https://v0.api.upyun.com";
static CHINA_NET: &str = "https://v1.api.upyun.com";
static UNICOM: &str = "https://v2.api.upyun.com";
static CHINA_MOBILE: &str = "https://v3.api.upyun.com";
