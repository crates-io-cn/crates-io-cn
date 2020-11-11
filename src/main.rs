extern crate pretty_env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper::{Method, StatusCode};

use hyper::header::{self, HeaderValue};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use futures::channel::mpsc;

mod helper;

use crate::helper::*;

lazy_static! {
    static ref ACTIVE_DOWNLOADS: Arc<RwLock<HashMap<String, Arc<Crate>>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:3000".parse().unwrap();

    let make_svc = make_service_fn(|_conn| async { Ok::<_, BoxError>(service_fn(handler)) });

    let server = Server::bind(&addr).serve(make_svc);

    info!("listen on {}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        error!("server error: {}", e);
    }
}

async fn handler(req: Request<Body>) -> Result<Response<Body>, BoxError> {
    let mut response = Response::new(Body::empty());

    if req.method() != Method::GET {
        *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        return Ok(response);
    }
    let parts: Vec<&str> = req.uri().path().trim_matches('/').split('/').collect();

    if parts.len() != 3 || parts[2].ne("download") {
        //warn!("unexpected request: {}", req.uri().path());
        *response.status_mut() = StatusCode::BAD_REQUEST;
        return Ok(response);
    }
    let (crate_name, crate_ver) = (parts[0].to_string(), parts[1].to_string());

    let (tx, rx) = mpsc::unbounded::<Result<hyper::body::Bytes, hyper::Error>>();
    *response.body_mut() = Body::wrap_stream(rx);

    let key = format!("{name}-{version}", name = crate_name, version = crate_ver);
    let krate = {
        let mut downloads = ACTIVE_DOWNLOADS.write().await;
        if !downloads.contains_key(&key) {
            match Crate::new(crate_name, crate_ver).await {
                Some(krate) => {
                    downloads.insert(key.clone(), krate);
                }
                None => {
                    *response.status_mut() = StatusCode::BAD_GATEWAY;
                    return Ok(response);
                }
            };
        }
        let krate = downloads.get(&key).unwrap().clone();
        response.headers_mut().append(
            header::CONTENT_TYPE,
            HeaderValue::from_str(krate.content_type.as_str()).unwrap(),
        );
        response.headers_mut().append(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(krate.size.to_string().as_str()).unwrap(),
        );
        krate
    };
    let mut notify = krate.notify.clone();
    tokio::spawn(async move {
        let mut ptr = 0;
        loop {
            let data = {
                let buffer = krate.buffer.read().await;
                let data = hyper::body::Bytes::copy_from_slice(&buffer[ptr..]);
                ptr += data.len();
                data
            };
            match tx.unbounded_send(Ok(data)) {
                Ok(_) => (),
                Err(e) => {
                    error!("{}", e);
                    break;
                }
            }
            info!("{}/{}", ptr, krate.size);
            if ptr == krate.size || notify.recv().await.is_none() {
                break;
            }
        }
    });
    Ok(response)
}
