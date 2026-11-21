// a plain log of the moments requests were held back or refused, to tune the rate limits from. one tab-separated
// line per event in rate-limits.log in the app's data folder: time, source, event, milliseconds, details
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const FILE_NAME: &str = "rate-limits.log";
// shorter waits happen all the time and would bury the ones that matter
const MIN_LOGGED_WAIT: Duration = Duration::from_millis(250);
// past this size the log moves to rate-limits.log.old and starts over
const MAX_BYTES: u64 = 5 * 1024 * 1024;

static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();

pub fn start(data_dir: &Path) {
    let _ = LOG.set(Mutex::new(data_dir.join(FILE_NAME)));
}

pub fn waited(source: &str, what: &str, wait: Duration, details: &str) {
    if wait >= MIN_LOGGED_WAIT {
        write(source, what, wait.as_millis(), details);
    }
}

pub fn event(source: &str, what: &str, details: &str) {
    write(source, what, 0, details);
}

fn write(source: &str, what: &str, ms: u128, details: &str) {
    let Some(log) = LOG.get() else {
        return;
    };
    let path = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if std::fs::metadata(&*path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = std::fs::rename(&*path, path.with_extension("log.old"));
    }
    let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let details = details.replace(['\t', '\n'], " ");
    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&*path)
        .and_then(|mut file| writeln!(file, "{time}\t{source}\t{what}\t{ms}\t{details}"));
    if let Err(error) = written {
        crate::platform::log(&format!("Couldn't write to the rate limit log: {error}"));
    }
}
