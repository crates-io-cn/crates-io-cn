use sd_notify::{notify, NotifyState};
use std::{env, process, time::Duration};

lazy_static! {
    static ref NOTIFY_ENABLED: bool = env::var_os("NOTIFY_SOCKET").is_some();
    static ref WATCHODG_TIMEOUT: Option<Duration> = watchdog_timeout();
}

pub fn notify_ready() {
    if *NOTIFY_ENABLED && notify(false, &[NotifyState::Ready]).is_err() {
        warn!("fail to notify systemd (ready)")
    }
}

pub fn notify_watchdog() {
    if *NOTIFY_ENABLED && notify(false, &[NotifyState::Watchdog]).is_err() {
        warn!("fail to poke watchdog");
    }
}

/// Return the watchdog timeout if it's enabled by systemd.
fn watchdog_timeout() -> Option<Duration> {
    if !*NOTIFY_ENABLED {
        return None;
    }
    let pid: u32 = env::var("WATCHDOG_PID").ok()?.parse().ok()?;
    if pid != process::id() {
        debug!(
            "WATCHDOG_PID was set to {}, not ours {}",
            pid,
            process::id()
        );
        return None;
    }
    let usec: u64 = env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    Some(Duration::from_micros(usec))
}