#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use actix_web::middleware::Logger;
use actix_web::{get, web, App, HttpResponse, HttpServer};
use std::env;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc::unbounded_channel, RwLock};

mod error;
mod helper;
#[allow(dead_code)]
mod index;
#[cfg(all(feature = "systemd-integration", target_os = "linux"))]
mod systemd;
#[cfg(feature = "obs")]
mod simple_obs;

use crate::index::{Config, GitIndex};
use helper::{Crate, CrateReq};
use std::ops::Add;
use tokio::time::{Duration, Instant};

lazy_static! {
    static ref GIT_INDEX_DIR: &'static str =
        Box::leak(env::var("GIT_INDEX_DIR").unwrap().into_boxed_str());
    static ref DL_FORMAT: &'static str =
        Box::leak(env::var("DL_FORMAT").unwrap().into_boxed_str());
    static ref ACTIVE_DOWNLOADS: Arc<RwLock<HashMap<CrateReq, Arc<Crate>>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

///
/// With this as config in crates.io-index
/// ```json
/// {
///     "dl": "https://bucket-cdn/{crate}/{version}",
///     "api": "https://crates.io"
/// }
/// ```
/// Upyun will redirect 404 (non-exist) crate to given address configured
/// replace `$_URI` with the path part `/{crate}/{version}`
#[get("/sync/{crate}/{version}")]
async fn sync(krate_req: web::Path<CrateReq>) -> HttpResponse {
    let krate_req = krate_req.into_inner();
    format!("{:?}", krate_req);
    match Crate::create(krate_req).await {
        Err(e) => {
            error!("{}", e);
            HttpResponse::NotFound().finish()
        }
        Ok(krate) => {
            let (tx, rx) = unbounded_channel::<Result<bytes::Bytes, ()>>();
            let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
            krate.tee(tx);
            HttpResponse::Ok()
                .content_type("application/x-tar")
                .streaming(rx)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    log4rs::init_file("config/log4rs.yml", Default::default()).unwrap();
    dotenv::dotenv().ok();
    let (tx, rx) = async_channel::unbounded();
    for i in 0..10 {
        let worker_rx = rx.clone();
        tokio::spawn(async move {
            while let Ok(krate) = worker_rx.recv().await {
                debug!("[worker#{}]start to sync {:?}", i, krate);
                match Crate::create(krate).await {
                    Ok(_) => (),
                    Err(e) => error!("{}", e),
                };
            }
        });
    }
    tokio::spawn(async move {
        let gi = GitIndex::new(
            &GIT_INDEX_DIR,
            &Config {
                dl: "https://static.crates-io.cn/{crate}/{version}".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        loop {
            let ddl = Instant::now().add(Duration::from_secs(300));
            info!("next update will on {:?}, exec git update now", ddl);
            #[cfg(all(feature = "systemd", target_os = "linux"))]
            systemd::notify_watchdog();
            let updated = gi.update();
            let crates = match updated {
                Ok(crates) => crates,
                Err(e) => {
                    error!("git update error: {}", e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            for krate in crates {
                if let Err(e) = tx.send(krate).await {
                    error!("{}", e);
                }
            }
            tokio::time::sleep_until(ddl).await;
        }
    });
    let server = HttpServer::new(|| App::new().wrap(Logger::default()).service(sync))
        .bind("127.0.0.1:8080")?
        .run();
    #[cfg(all(feature = "systemd", target_os = "linux"))]
    systemd::notify_ready();
    server.await
}
