//! Compact operator progress on stderr.
//!
//! The final result of a Build stays one parseable JSON value on stdout; every
//! transition, heartbeat and diagnostic here goes to stderr, so a redirected
//! run can read progress and still parse its result.

use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// How often a quiet in-flight action reports that it is still alive.
const HEARTBEAT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
pub struct Progress {
    tty: bool,
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

impl Progress {
    /// TTY detection uses this process's own stderr, so an interactive run gets
    /// compact lines and a redirected run gets timestamped ones.
    pub fn new() -> Self {
        Self {
            tty: std::io::stderr().is_terminal(),
        }
    }

    #[cfg(test)]
    pub fn quiet() -> Self {
        Self { tty: false }
    }

    /// One durable transition or fact.  It never carries engineering judgement.
    pub fn event(&self, message: &str) {
        if self.tty {
            eprintln!("[build] {message}");
        } else {
            eprintln!("[build {}] {message}", timestamp());
        }
    }

    /// Begin heartbeats for one accepted action.  Dropping the guard stops them.
    pub fn heartbeat(&self, action_dir: PathBuf, label: String) -> Heartbeat {
        Heartbeat::start(self.tty, action_dir, label, HEARTBEAT)
    }
}

pub struct Heartbeat {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Heartbeat {
    pub(crate) fn start(tty: bool, action_dir: PathBuf, label: String, interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let tick = Duration::from_millis(10).min(interval);
            let mut waited = Duration::ZERO;
            while !flag.load(Ordering::Relaxed) {
                std::thread::sleep(tick);
                waited += tick;
                if waited < interval {
                    continue;
                }
                waited = Duration::ZERO;
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let activity = newest_activity(&action_dir)
                    .map(|millis| format!("{}s ago", now_ms().saturating_sub(millis) / 1000))
                    .unwrap_or_else(|| "none observed yet".into());
                // A heartbeat says the controller is alive and what the provider
                // last wrote; it never says the work is progressing or accepted.
                let line = format!(
                    "{label} still in flight; last provider activity {activity} (activity is not completion)"
                );
                if tty {
                    eprintln!("[build] {line}");
                } else {
                    eprintln!("[build {}] {line}", timestamp());
                }
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn newest_activity(action_dir: &Path) -> Option<u128> {
    let mut newest = None;
    for name in ["transport.jsonl", "provider-stderr.log"] {
        let Ok(metadata) = std::fs::metadata(action_dir.join(name)) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let millis = modified
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        if newest.is_none_or(|seen| millis > seen) {
            newest = Some(millis);
        }
    }
    newest
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn timestamp() -> String {
    // A compact wall-clock stamp; the exact format is not part of any contract.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{now}")
}
