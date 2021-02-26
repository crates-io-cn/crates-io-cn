use bytes::{Bytes, BytesMut};
use serde::Deserialize;
use std::env;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};
use tokio_stream::StreamExt;

use crate::error::Error;
use crate::ACTIVE_DOWNLOADS;
use reqwest::StatusCode;
#[cfg(feature = "upyun-oss")]
use upyun::{Operator, Upyun};
#[cfg(feature = "obs")]
use crate::simple_obs::{Bucket, IamProvider, AutoRefreshingProvider, ProvideObsCredentials, Ssl};

#[derive(Clone, Debug, Deserialize, Hash, Eq, PartialEq)]
pub struct CrateReq {
    #[serde(alias = "crate")]
    name: String,
    #[serde(alias = "vers")]
    version: String,
}

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}
#[cfg(feature = "upyun-oss")]
lazy_static! {
    static ref UPYUN_NAME: &'static str =
        Box::leak(env::var("UPYUN_NAME").unwrap().into_boxed_str());
    static ref UPYUN_TOKEN: &'static str =
        Box::leak(env::var("UPYUN_TOKEN").unwrap().into_boxed_str());
    static ref UPYUN_BUCKET: &'static str =
        Box::leak(env::var("UPYUN_BUCKET").unwrap().into_boxed_str());
    static ref UPYUN: Upyun = Upyun::new(Operator::new(&UPYUN_NAME, &UPYUN_TOKEN));

}
#[cfg(feature = "obs")]
lazy_static! {
    static ref OBS_BUCKET_NAME: &'static str =
        Box::leak(env::var("OBS_BUCKET_NAME").unwrap().into_boxed_str());
    static ref OBS_ENDPOINT: &'static str =
        Box::leak(env::var("OBS_ENDPOINT").unwrap().into_boxed_str());
    static ref OBS_CREDENTIALS: AutoRefreshingProvider<IamProvider> =
        AutoRefreshingProvider::new(IamProvider::new());
    static ref OBS_BUCKET: Bucket = Bucket::new(&OBS_BUCKET_NAME, &OBS_ENDPOINT, Ssl::Yes);
}

#[derive(Clone, Debug)]
pub struct Crate {
    name: String,
    version: String,
    content_type: String,
    content_length: usize,
    pub buffer: Arc<RwLock<BytesMut>>,
    pub notify: watch::Receiver<usize>,
    ptr: usize,
}

impl Crate {
    pub async fn create(krate_req: CrateReq) -> Result<Arc<Self>, Error> {
        if let Some(krate) = ACTIVE_DOWNLOADS.read().await.get(&krate_req) {
            return Ok(krate.clone());
        }
        let mut guard = ACTIVE_DOWNLOADS.write().await;
        let CrateReq { name, version } = krate_req.clone();
        let uri = format!(
            "https://static.crates.io/crates/{name}/{name}-{version}.crate",
            name = name,
            version = version
        );
        let key = format!("{}/{}", name, version);
        let krate_req_key = krate_req.clone();
        let resp = CLIENT.get(&uri).send().await?;
        if resp.status() != StatusCode::OK {
            return Err(Error::FetchFail);
        }
        let content_length = resp.content_length().ok_or(Error::MissingField)? as usize;
        let (tx, rx) = watch::channel(0);
        let krate = Self {
            name,
            version,
            content_type: resp
                .headers()
                .get("content-type")
                .ok_or(Error::MissingField)?
                .to_str()?
                .to_string(),
            content_length,
            buffer: Arc::new(RwLock::new(BytesMut::with_capacity(
                content_length as usize,
            ))),
            notify: rx,
            ptr: 0,
        };
        let write_buffer = krate.buffer.clone();
        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(data) => {
                        let mut buffer = write_buffer.write().await;
                        trace!("recv {}", data.len());
                        buffer.extend_from_slice(&data[..]);
                        tx.send(data.len()).unwrap();
                    }
                    Err(e) => {
                        error!("{}", e);
                        break;
                    }
                };
            }
            let buffer = write_buffer.read().await.clone().freeze();
            debug!("{:?} download complete", krate_req_key);
            let mut counter: i32 = 10;
            while counter > 0 {
                #[cfg(feature = "obs")]
                let result = match OBS_CREDENTIALS.credentials().await {
                    Ok(credentials) => OBS_BUCKET.put(&key, buffer.clone(), &credentials).await.err(),
                    Err(e) => Some(e)
                };
                #[cfg(feature = "upyun-oss")]
                let result = UPYUN.put_file(*UPYUN_BUCKET, &key, buffer.clone()).await.err();
                if let Some(e) = result {
                    error!("retry attempt {}:{}", 10 - counter, e);
                    counter -= 1;
                    continue;
                }
                ACTIVE_DOWNLOADS.write().await.remove(&krate_req_key);
                debug!("remove {:?} from active download", krate_req_key);
                break;
            }
        });
        guard.insert(krate_req.clone(), Arc::new(krate));
        debug!("insert {:?} into active download", krate_req);
        Ok(guard.get(&krate_req).unwrap().clone())
    }

    pub fn tee(&self, tx: mpsc::UnboundedSender<Result<Bytes, ()>>) {
        let mut notify = self.notify.clone();
        let krate = self.clone();
        tokio::spawn(async move {
            let mut ptr = 0;
            loop {
                let data = {
                    let buffer = krate.buffer.read().await;
                    let data = Bytes::copy_from_slice(&buffer[ptr..]);
                    ptr += data.len();
                    data
                };
                match tx.send(Ok(data)) {
                    Ok(_) => (),
                    Err(e) => {
                        error!("{}", e);
                        break;
                    }
                }
                info!("{}/{}", ptr, krate.content_length);
                if ptr == krate.content_length || notify.changed().await.is_ok() {
                    break;
                }
            }
        });
    }
}
