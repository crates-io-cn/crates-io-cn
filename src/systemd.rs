use systemd::daemon::{
    notify,
    STATE_READY, STATE_WATCHDOG
};

pub fn notify_ready() {
    if notify(false, [(STATE_READY, "1")].iter()).is_err() {
        warn!("fail to notify systemd (ready)")
    }
}

pub fn notify_watchdog() {
    if notify(false, [(STATE_WATCHDOG, "1")].iter()).is_err() {
        warn!("fail to poke watchdog");
    }
}