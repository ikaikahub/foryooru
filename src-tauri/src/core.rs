use crate::activity;
use crate::booru::{self, Booru, Site};
use crate::catalogue;
use crate::corpus::CorpusCache;
use crate::db::{self, Db};
use crate::error::Result;
use crate::images::Images;
use crate::import::{self, ImportStatus};
use crate::progress::Progress;
use crate::queue::{self, FetchQueue};
use crate::sync::{self, SyncStatus};
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_store::StoreExt;
use tokio::sync::Notify;

const LEGACY_DATABASE: &str = "foryooru.db";

pub struct Core {
    pub app: AppHandle,
    pub site: Site,
    pub http: reqwest::Client,
    pub db: Arc<Db>,
    pub booru: Booru,
    pub images: Arc<Images>,
    pub corpus: Arc<CorpusCache>,
    pub import: Progress<ImportStatus>,
    pub popular_seeding: AtomicBool,
    pub queue: Arc<FetchQueue>,
    pub sync: Progress<SyncStatus>,
    pub sync_lock: tokio::sync::Mutex<()>,
    pub sync_queued: AtomicBool,
}

impl Drop for Core {
    fn drop(&mut self) {
        self.queue.wake();
    }
}

impl Core {
    fn open(
        app: &AppHandle,
        site: Site,
        http: &reqwest::Client,
        images: &Arc<Images>,
    ) -> Result<Arc<Core>> {
        let db = Arc::new(Db::open(
            &app.path().app_data_dir()?.join(site.database_file()),
        )?);
        db::checkpoint_in_background(&db);
        activity::prune(&db.conn())?;
        let core = Arc::new(Core {
            app: app.clone(),
            site,
            http: http.clone(),
            booru: Booru::new(site, app.clone(), db.clone(), http.clone()),
            db,
            images: images.clone(),
            corpus: Arc::new(CorpusCache::default()),
            import: Progress::new(import::EVENT),
            popular_seeding: AtomicBool::new(false),
            queue: Arc::new(FetchQueue::default()),
            sync: Progress::new(sync::EVENT),
            sync_lock: tokio::sync::Mutex::new(()),
            sync_queued: AtomicBool::new(false),
        });
        queue::start(&core);
        sync::sync_now(&core);
        catalogue::fill_tag_details(&core);
        let warm = core.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = warm.corpus.get(&warm.db) {
                eprintln!("Building the post index failed: {error}");
            }
        });
        Ok(core)
    }
}

#[derive(Default)]
pub struct Startup {
    ready: AtomicBool,
    finished: Notify,
}

impl Startup {
    pub fn finish(&self) {
        self.ready.store(true, Ordering::SeqCst);
        self.finished.notify_waiters();
    }

    pub async fn wait(&self) {
        let finished = self.finished.notified();
        tokio::pin!(finished);
        finished.as_mut().enable();
        if !self.ready.load(Ordering::SeqCst) {
            finished.await;
        }
    }
}

pub struct Sites {
    current: RwLock<Arc<Core>>,
    http: reqwest::Client,
    images: Arc<Images>,
}

impl Sites {
    pub fn start(app: &AppHandle) -> Result<Sites> {
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        crate::rate_log::start(&data_dir);
        adopt_legacy_database(&data_dir)?;
        let http = reqwest::Client::builder()
            .user_agent("foryooru/0.1")
            .build()?;
        let images = Arc::new(Images::new(
            app.clone(),
            app.path().app_cache_dir()?.join("images"),
            http.clone(),
        ));
        images.prune_in_background();
        let site = saved_site(app)?;
        let core = Core::open(app, site, &http, &images)?;
        Ok(Sites {
            current: RwLock::new(core),
            http,
            images,
        })
    }

    pub fn current(&self) -> Arc<Core> {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn switch(&self, app: &AppHandle, site: Site) -> Result<()> {
        if self.current().site != site {
            let core = Core::open(app, site, &self.http, &self.images)?;
            *self
                .current
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = core;
        }
        let store = app.store(booru::SETTINGS_STORE)?;
        store.set("site", serde_json::to_value(site)?);
        store.save()?;
        Ok(())
    }
}

fn saved_site(app: &AppHandle) -> Result<Site> {
    let saved = app.store(booru::SETTINGS_STORE)?.get("site");
    Ok(saved
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or(Site::Danbooru))
}

fn adopt_legacy_database(data_dir: &Path) -> Result<()> {
    let (legacy, target) = (
        data_dir.join(LEGACY_DATABASE),
        data_dir.join(Site::Gelbooru.database_file()),
    );
    if !legacy.exists() || target.exists() {
        return Ok(());
    }
    for (from, to) in db::database_files(&legacy)
        .into_iter()
        .zip(db::database_files(&target))
    {
        if from.exists() {
            std::fs::rename(from, to)?;
        }
    }
    Ok(())
}

pub struct CurrentCore(Arc<Core>);

impl CurrentCore {
    pub fn inner(&self) -> &Arc<Core> {
        &self.0
    }
}

impl Deref for CurrentCore {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.0
    }
}

impl<'de, R: Runtime> CommandArg<'de, R> for CurrentCore {
    fn from_command(command: CommandItem<'de, R>) -> std::result::Result<Self, InvokeError> {
        let sites = command
            .message
            .webview_ref()
            .try_state::<Sites>()
            .ok_or_else(|| InvokeError::from("The app is still starting"))?;
        Ok(CurrentCore(sites.current()))
    }
}
