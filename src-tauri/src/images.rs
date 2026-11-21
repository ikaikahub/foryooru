use crate::booru::{SETTINGS_STORE, Site};
use crate::error::{Error, Result};
use crate::limiter::Limiter;
use md5::{Digest, Md5};
use reqwest::header::{CONTENT_LENGTH, RANGE, REFERER};
use reqwest::{Method, StatusCode};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::Semaphore;

const TIMEOUT: Duration = Duration::from_secs(15);
const ORIGINAL_TIMEOUT: Duration = Duration::from_secs(120);
const COOLDOWN: Duration = Duration::from_secs(10);
const MAX_WAIT: Duration = Duration::from_secs(8);
const REFUSAL_404S: usize = 3;
const REFUSAL_WINDOW: Duration = Duration::from_secs(5);
const PRUNE_EVERY: u32 = 500;
const CACHE_LIMIT_KEY: &str = "cacheLimitMb";
pub const DEFAULT_CACHE_LIMIT_MB: u64 = if cfg!(mobile) { 512 } else { 2048 };

// each CDN has its own rate limit and pause, so trouble on one doesn't slow the other down
struct Cdn {
    limiter: Limiter,
    paused_until: Mutex<Option<Instant>>,
    recent_404s: Mutex<VecDeque<Instant>>,
}

impl Cdn {
    fn new(name: &'static str) -> Cdn {
        Cdn {
            limiter: Limiter::new(name, 8.0, 20.0),
            paused_until: Mutex::new(None),
            recent_404s: Mutex::new(VecDeque::with_capacity(REFUSAL_404S)),
        }
    }

    fn pause_left(&self) -> Option<Duration> {
        self.paused_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .and_then(|until| until.checked_duration_since(Instant::now()))
    }
}

pub struct Images {
    app: AppHandle,
    dir: PathBuf,
    http: reqwest::Client,
    gelbooru: Cdn,
    danbooru: Cdn,
    slots: Semaphore,
    waiting: Semaphore,
    stored_since_prune: AtomicU32,
    pruning: Arc<AtomicBool>,
}

pub fn content_type(url: &str) -> &'static str {
    let path = url.split('?').next().unwrap_or_default().to_lowercase();
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "image/jpeg",
    }
}

impl Images {
    pub fn new(app: AppHandle, dir: PathBuf, http: reqwest::Client) -> Images {
        Images {
            app,
            dir,
            http,
            gelbooru: Cdn::new("gelbooru images"),
            danbooru: Cdn::new("danbooru images"),
            slots: Semaphore::new(12),
            waiting: Semaphore::new(48),
            stored_since_prune: AtomicU32::new(0),
            pruning: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn cdn(&self, url: &str) -> &Cdn {
        match Site::of_url(url) {
            Site::Gelbooru => &self.gelbooru,
            Site::Danbooru => &self.danbooru,
        }
    }

    // how long a download from this url's CDN would wait right now
    pub fn wait_estimate(&self, url: &str) -> Duration {
        let cdn = self.cdn(url);
        cdn.pause_left()
            .unwrap_or_default()
            .max(cdn.limiter.wait_estimate())
    }

    fn path_for(&self, key: &str) -> PathBuf {
        let key = format!("{:x}", Md5::digest(key.as_bytes()));
        self.dir.join(&key[..2]).join(&key[2..4]).join(key)
    }

    fn read_cached(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.path_for(key);
        let bytes = fs::read(&path).ok()?;
        if let Ok(file) = File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    fn write_cached(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(&temporary, &path)?;
        if self.stored_since_prune.fetch_add(1, Ordering::Relaxed) + 1 >= PRUNE_EVERY {
            self.stored_since_prune.store(0, Ordering::Relaxed);
            self.prune_in_background();
        }
        Ok(())
    }

    pub fn prune_in_background(&self) {
        if self.pruning.swap(true, Ordering::SeqCst) {
            return;
        }
        let (dir, limit, pruning) = (self.dir.clone(), self.limit_bytes(), self.pruning.clone());
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = prune(&dir, limit) {
                eprintln!("Pruning the image cache failed: {error}");
            }
            pruning.store(false, Ordering::SeqCst);
        });
    }

    pub fn limit_mb(&self) -> u64 {
        self.app
            .store(SETTINGS_STORE)
            .ok()
            .and_then(|store| store.get(CACHE_LIMIT_KEY))
            .and_then(|value| value.as_u64())
            .unwrap_or(DEFAULT_CACHE_LIMIT_MB)
    }

    fn limit_bytes(&self) -> u64 {
        self.limit_mb() * 1024 * 1024
    }

    pub fn set_limit_mb(&self, megabytes: u64) -> Result<()> {
        let store = self.app.store(SETTINGS_STORE)?;
        store.set(CACHE_LIMIT_KEY, megabytes);
        store.save()?;
        self.prune_in_background();
        Ok(())
    }

    fn refused(&self, why: &str, url: &str) {
        crate::rate_log::event("images", &format!("{why}, pausing {}s", COOLDOWN.as_secs()), url);
        let mut paused = self
            .cdn(url)
            .paused_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let until = Instant::now() + COOLDOWN;
        *paused = Some(paused.map_or(until, |current| current.max(until)));
    }

    fn note_404(&self, url: &str) {
        let mut recent = self
            .cdn(url)
            .recent_404s
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        if recent.len() == REFUSAL_404S {
            recent.pop_front();
        }
        recent.push_back(now);
        let burst = recent.len() == REFUSAL_404S
            && recent
                .front()
                .is_some_and(|first| now - *first < REFUSAL_WINDOW);
        drop(recent);
        if burst {
            self.refused(&format!("{REFUSAL_404S} not found in a row"), url);
        }
    }

    async fn request(
        &self,
        method: Method,
        url: &str,
        range: Option<&str>,
        max_wait: Option<Duration>,
        timeout: Duration,
    ) -> Result<reqwest::Response> {
        let cdn = self.cdn(url);
        if let Some(pause) = cdn.pause_left() {
            if max_wait.is_some_and(|max| pause > max) {
                crate::rate_log::waited("images", "gave up during a pause", pause, url);
                return Err(Error::CdnBusy);
            }
            crate::rate_log::waited("images", "waited out a pause", pause, url);
            tokio::time::sleep(pause).await;
        }
        if !cdn.limiter.acquire(max_wait, url).await {
            return Err(Error::CdnBusy);
        }
        let mut request = self
            .http
            .request(method, url)
            .header(REFERER, Site::referer_for(url))
            .timeout(timeout);
        if let Some(range) = range {
            request = request.header(RANGE, range);
        }
        Ok(request.send().await?)
    }

    pub fn cached(&self, key: &str) -> Option<Vec<u8>> {
        self.read_cached(key)
    }

    // key names the cache entry, so the same file from either CDN fills one entry. a guessed url's 404 is expected
    // and doesn't count towards pausing the CDN
    pub async fn get(&self, key: &str, url: &str, guessed: bool) -> Result<Vec<u8>> {
        if let Some(bytes) = self.read_cached(key) {
            return Ok(bytes);
        }
        let _waiting = self.waiting.try_acquire().map_err(|_| {
            crate::rate_log::event("images", "queue full, skipped", url);
            Error::CdnBusy
        })?;
        let deadline = Instant::now() + MAX_WAIT;
        let slot_wait = Instant::now();
        let _slot = tokio::time::timeout(MAX_WAIT, self.slots.acquire())
            .await
            .map_err(|_| {
                crate::rate_log::waited("images", "no free download slot, skipped", MAX_WAIT, url);
                Error::CdnBusy
            })?
            .map_err(|_| Error::CdnBusy)?;
        crate::rate_log::waited("images", "waited for a download slot", slot_wait.elapsed(), url);
        if let Some(bytes) = self.read_cached(key) {
            return Ok(bytes);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        self.download(key, url, Some(left), TIMEOUT, guessed).await
    }

    pub async fn get_original(&self, key: &str, url: &str, guessed: bool) -> Result<Vec<u8>> {
        if let Some(bytes) = self.read_cached(key) {
            return Ok(bytes);
        }
        self.download(key, url, None, ORIGINAL_TIMEOUT, guessed).await
    }

    async fn download(
        &self,
        key: &str,
        url: &str,
        max_wait: Option<Duration>,
        timeout: Duration,
        guessed: bool,
    ) -> Result<Vec<u8>> {
        let response = self
            .request(Method::GET, url, None, max_wait, timeout)
            .await?;
        match response.status() {
            StatusCode::NOT_FOUND => {
                if !guessed {
                    self.note_404(url);
                }
                return Err(Error::NotOnCdn);
            }
            status if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
                self.refused(&format!("got {}", status.as_u16()), url);
                return Err(Error::CdnBusy);
            }
            status if !status.is_success() => return Err(Error::Status(status.as_u16())),
            _ => {}
        }
        let bytes = response.bytes().await?.to_vec();
        self.write_cached(key, &bytes)?;
        Ok(bytes)
    }

    pub async fn exists(&self, url: &str) -> Option<bool> {
        let response = self
            .request(Method::HEAD, url, None, Some(MAX_WAIT), TIMEOUT)
            .await
            .ok()?;
        match response.status() {
            StatusCode::OK => Some(true),
            StatusCode::NOT_FOUND => Some(false),
            status => {
                self.refused(&format!("got {} checking a file", status.as_u16()), url);
                None
            }
        }
    }

    pub async fn content_length(&self, url: &str) -> Result<Option<u64>> {
        let response = self
            .request(Method::HEAD, url, None, Some(MAX_WAIT), TIMEOUT)
            .await?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        Ok(response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok()))
    }

    pub async fn original(&self, url: &str, range: Option<&str>) -> Result<reqwest::Response> {
        let response = self
            .request(Method::GET, url, range, None, ORIGINAL_TIMEOUT)
            .await?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        Ok(response)
    }
}

pub fn prune(dir: &Path, limit: u64) -> Result<()> {
    let mut files = Vec::new();
    let mut total = 0;
    let mut folders = vec![dir.to_owned()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                folders.push(entry.path());
            } else {
                total += meta.len();
                files.push((
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                    entry.path(),
                ));
            }
        }
    }
    if total <= limit {
        return Ok(());
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in files {
        if total <= limit * 9 / 10 {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
    Ok(())
}
