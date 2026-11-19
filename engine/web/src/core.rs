use crate::activity;
use crate::booru::{Booru, Site};
use crate::catalogue;
use crate::corpus::CorpusCache;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::import::{self, ImportStatus};
use crate::platform::AppHandle;
use crate::progress::Progress;
use crate::queue::{self, FetchQueue};
use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfgBuilder, OpfsSAHPoolUtil};
use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct Core {
    pub app: AppHandle,
    pub site: Site,
    pub db: Arc<Db>,
    pub booru: Booru,
    pub corpus: Arc<CorpusCache>,
    pub import: Progress<ImportStatus>,
    pub learning: Mutex<()>,
    pub queue: Arc<FetchQueue>,
    pub popular_seeding: AtomicBool,
}

impl Drop for Core {
    fn drop(&mut self) {
        self.queue.wake();
    }
}

impl Core {
    fn open(site: Site, http: &reqwest::Client) -> Result<Arc<Core>> {
        let app = AppHandle;
        let db = Arc::new(Db::open(Path::new(site.database_file()))?);
        activity::prune(&db.conn())?;
        let core = Arc::new(Core {
            app,
            site,
            booru: Booru::new(site, app, db.clone(), http.clone()),
            db,
            corpus: Arc::new(CorpusCache::default()),
            import: Progress::new(import::EVENT),
            learning: Mutex::new(()),
            queue: Arc::new(FetchQueue::default()),
            popular_seeding: AtomicBool::new(false),
        });
        queue::start(&core);
        catalogue::fill_tag_details(&core);
        Ok(core)
    }
}

// the worker is single-threaded, so the open site lives in a thread-local instead of Tauri's managed state
struct Sites {
    current: Arc<Core>,
    http: reqwest::Client,
    files: OpfsSAHPoolUtil,
}

thread_local! {
    static SITES: RefCell<Option<Sites>> = const { RefCell::new(None) };
}

// SQLite keeps its files in the origin's private file system; this registers that as the default storage
pub async fn start(site: Site) -> Result<()> {
    let options = OpfsSAHPoolCfgBuilder::new().directory("foryooru").build();
    let files = sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(&options, true)
        .await
        .map_err(|error| Error::Storage(format!("{error:?}")))?;
    let http = reqwest::Client::new();
    let current = Core::open(site, &http)?;
    SITES.with(|sites| *sites.borrow_mut() = Some(Sites { current, http, files }));
    Ok(())
}

pub fn current() -> Result<Arc<Core>> {
    SITES.with(|sites| {
        sites
            .borrow()
            .as_ref()
            .map(|sites| sites.current.clone())
            .ok_or(Error::NotStarted)
    })
}

// a shared link previews a post of the other site, whose client shares this connection
pub fn http() -> Result<reqwest::Client> {
    SITES.with(|sites| {
        sites
            .borrow()
            .as_ref()
            .map(|sites| sites.http.clone())
            .ok_or(Error::NotStarted)
    })
}

pub fn switch(site: Site) -> Result<()> {
    if current()?.site == site {
        return Ok(());
    }
    let http = SITES.with(|sites| sites.borrow().as_ref().map(|sites| sites.http.clone()));
    let core = Core::open(site, &http.ok_or(Error::NotStarted)?)?;
    SITES.with(|sites| {
        if let Some(sites) = sites.borrow_mut().as_mut() {
            sites.current = core;
        }
    });
    Ok(())
}

// the file pool exports and imports whole databases, which backups use
pub fn with_files<T>(work: impl FnOnce(&OpfsSAHPoolUtil) -> T) -> Result<T> {
    SITES.with(|sites| {
        sites
            .borrow()
            .as_ref()
            .map(|sites| work(&sites.files))
            .ok_or(Error::NotStarted)
    })
}
