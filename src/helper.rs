use bytes::{Bytes, BytesMut};
use serde::Deserialize;
use std::sync::Arc;
use tokio::stream::StreamExt;
use tokio::sync::{mpsc, watch, RwLock};

use crate::error::Error;
use crate::ACTIVE_DOWNLOADS;

#[derive(Clone, Debug, Deserialize, Hash, Eq, PartialEq)]
pub struct CrateReq {
    #[serde(rename = "crate")]
    name: String,
    version: String,
}

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
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
        let resp = CLIENT.get(&uri).send().await?;
        let content_length = resp.content_length().ok_or_else(|| Error::MissingField)? as usize;
        let (tx, rx) = watch::channel(0);
        let krate = Self {
            name,
            version,
            content_type: resp
                .headers()
                .get("content-type")
                .ok_or_else(|| Error::MissingField)?
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
                        buffer.extend_from_slice(&data[..]);
                        tx.broadcast(data.len()).unwrap();
                    }
                    Err(e) => {
                        error!("{}", e);
                        break;
                    }
                };
            }
            // TODO: Upload to upyun
        });
        guard.insert(krate_req.clone(), Arc::new(krate));
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
                if ptr == krate.content_length || notify.recv().await.is_none() {
                    break;
                }
            }
        });
    }
}
