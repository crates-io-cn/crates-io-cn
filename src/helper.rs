use hyper::header;
use hyper::Client;
use hyper_tls::HttpsConnector;
use http_body::Body as _;

use std::sync::Arc;
use bytes::BytesMut;
use tokio::sync::{watch, RwLock};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::ACTIVE_DOWNLOADS;

pub type BoxError = Box<dyn std::error::Error + 'static + Send + Sync>;

pub struct Crate {
    pub name: String,
    pub ver: String,
    pub size: usize,
    pub content_type: String,
    pub buffer: Arc<RwLock<BytesMut>>,
    pub notify: watch::Receiver<usize>,
}

impl Crate {
    pub async fn new(name: String, ver: String) -> Option<Arc<Self>> {
        let uri = format!(
            "https://static.crates.io/crates/{name}/{name}-{version}.crate",
            name = name,
            version = ver
        )
        .parse()
        .ok()?;

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);

        let mut resp = client.get(uri).await.ok()?;
        // Only 200 code is acceptable in this situation, 3xx is not acceptable and should never happen
        if resp.status() != 200 {
            warn!("upstream error: {}", resp.status());
            return None;
        }

        let headers = resp.headers().clone();
        let content_type: String = headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .ok()?
            .to_string();
        let content_length: usize = headers
            .get(header::CONTENT_LENGTH)
            .unwrap()
            .to_str()
            .ok()?
            .parse()
            .ok()?;

        let (tx, rx) = watch::channel(0);

        let krate = Arc::new(Self {
            name,
            ver,
            size: content_length,
            content_type,
            buffer: Arc::new(RwLock::new(BytesMut::new())),
            notify: rx,
        });

        let krate_w = krate.clone();
        tokio::spawn(async move {
            while let Some(chunk) = resp.body_mut().data().await {
                match chunk {
                    Ok(data) => {
                        let mut buffer = krate_w.buffer.write().await;
                        buffer.extend_from_slice(&data[..]);
                        tx.broadcast(data.len()).unwrap();
                    }
                    Err(e) => {
                        error!("{}", e);
                        break;
                    }
                };
            }
            fs::create_dir_all(&format!("/mirror_nfs/crates/{name}", name = krate_w.name)).await.unwrap();
            let mut file = File::create(&format!("/mirror_nfs/crates/{name}/{version}", name = krate_w.name, version = krate_w.ver)).await.unwrap();
            file.write_all(krate_w.buffer.write().await.as_ref()).await.unwrap();
            ACTIVE_DOWNLOADS.write().await.remove(&format!("{name}-{version}", name = krate_w.name, version = krate_w.ver));
        });

        Some(krate)
    }
}
