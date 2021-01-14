use systemd::daemon::{
    notify,
    STATE_READY, STATE_WATCHDOG
};

pub fn notify_ready() {
    match notify(false, [(STATE_READY, "1")].iter()) {
        Ok(result) => info!("notify result: {}", result),
        Err(e) => error!("fail to notify ready: {}", e)
    }
}

pub fn notify_watchdog() {
    match notify(false, [(STATE_WATCHDOG, "1")].iter()) {
        Ok(result) => info!("poke watchdog result: {}", result),
        Err(e) => error!("fail to poke watchdog: {}", e)
    }
}