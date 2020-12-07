#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use actix_web::middleware::Logger;
use actix_web::{get, web, App, HttpResponse, HttpServer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc::unbounded_channel, RwLock};

mod error;
mod helper;
#[allow(dead_code)]
mod index;
use helper::{Crate, CrateReq};
use crate::index::{GitIndex, Config};
use tokio::time::{Duration, Instant};
use std::ops::Add;

lazy_static! {
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
async fn sync(web::Path(krate_req): web::Path<CrateReq>) -> HttpResponse {
    format!("{:?}", krate_req);
    match Crate::create(krate_req).await {
        Err(e) => {
            error!("{}", e);
            HttpResponse::NotFound().finish()
        }
        Ok(krate) => {
            let (tx, rx) = unbounded_channel::<Result<bytes::Bytes, ()>>();
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
    tokio::spawn(async move {
        let gi = GitIndex::new("/var/www/git/crates.io-index", &Config {
            dl: "https://static.crates-io.cn/{crate}/{version}".to_string(),
            .. Default::default()
        }).unwrap();
        loop {
            let ddl = Instant::now().add(Duration::from_secs(300));
            info!("next update will on {:?}, exec git update now", ddl);
            let crates = gi.update().unwrap();
            for krate in crates {
                debug!("start to sync {:?}", krate);
                match Crate::create(krate).await {
                    Ok(_) => (),
                    Err(e) => error!("{}", e)
                };
            }
            tokio::time::delay_until(ddl).await;
        }
    });
    HttpServer::new(|| App::new().wrap(Logger::default()).service(sync))
        .bind("127.0.0.1:8080")?
        .run()
        .await
}
