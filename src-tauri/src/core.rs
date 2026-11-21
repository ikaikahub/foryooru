use crate::activity;
use crate::booru::{Booru, Site, SiteQuery};
use crate::catalogue;
use crate::corpus::CorpusCache;
use crate::db::{self, Db};
use crate::error::Result;
use crate::images::Images;
use crate::import::{self, ImportStatus};
use crate::merge;
use crate::progress::Progress;
use crate::queue::{self, FetchQueue};
use crate::sync::{self, SyncStatus};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Notify;

pub struct Core {
    pub app: AppHandle,
    pub http: reqwest::Client,
    pub db: Arc<Db>,
    pub gelbooru: Booru,
    pub danbooru: Booru,
    pub images: Arc<Images>,
    pub corpus: Arc<CorpusCache>,
    pub import: Progress<ImportStatus>,
    pub popular_seeding: AtomicBool,
    pub queue: Arc<FetchQueue>,
    pub sync: Progress<SyncStatus>,
    pub sync_lock: tokio::sync::Mutex<()>,
    pub sync_queued: AtomicBool,
    // the site each search's first page came from, so its later pages come from the same one
    routes: Mutex<HashMap<String, Site>>,
}

impl Drop for Core {
    fn drop(&mut self) {
        self.queue.wake();
    }
}

impl Core {
    pub fn start(app: &AppHandle) -> Result<Arc<Core>> {
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        crate::rate_log::start(&data_dir);
        let db = Arc::new(merge::open(&data_dir)?);
        db::checkpoint_in_background(&db);
        activity::prune(&db.conn())?;
        let http = reqwest::Client::builder()
            .user_agent("foryooru/0.1")
            .build()?;
        let images = Arc::new(Images::new(
            app.clone(),
            app.path().app_cache_dir()?.join("images"),
            http.clone(),
        ));
        images.prune_in_background();
        let core = Arc::new(Core {
            app: app.clone(),
            gelbooru: Booru::new(Site::Gelbooru, app.clone(), db.clone(), http.clone()),
            danbooru: Booru::new(Site::Danbooru, app.clone(), db.clone(), http.clone()),
            http,
            db,
            images,
            corpus: Arc::new(CorpusCache::default()),
            import: Progress::new(import::EVENT),
            popular_seeding: AtomicBool::new(false),
            queue: Arc::new(FetchQueue::default()),
            sync: Progress::new(sync::EVENT),
            sync_lock: tokio::sync::Mutex::new(()),
            sync_queued: AtomicBool::new(false),
            routes: Mutex::new(HashMap::new()),
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

    pub fn booru(&self, site: Site) -> &Booru {
        match site {
            Site::Gelbooru => &self.gelbooru,
            Site::Danbooru => &self.danbooru,
        }
    }

    // Gelbooru's API refuses requests without a key. Danbooru answers anyone
    pub fn can_search(&self, site: Site) -> bool {
        match (site, &self.gelbooru) {
            (Site::Danbooru, _) => true,
            (Site::Gelbooru, Booru::Gelbooru(gelbooru)) => gelbooru.has_api_key(),
            (Site::Gelbooru, Booru::Danbooru(_)) => false,
        }
    }

    // the site a search goes to, and the search written in that site's terms. later pages stay on the first page's
    // site, as the sites' orders differ and mixing them would repeat and skip posts
    pub async fn route(&self, query: &SiteQuery<'_>, page: i64) -> Result<(Site, String)> {
        let key = format!("{:?} {:?} {:?}", query.tags, query.ratings, query.order);
        let remembered = self
            .routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
            .copied();
        let site = match remembered.filter(|site| page > 0 && self.can_search(*site)) {
            Some(site) => site,
            None => {
                let site = self.choose_site(query).await?;
                self.routes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(key, site);
                site
            }
        };
        Ok((site, self.booru(site).render(query).await?))
    }

    async fn choose_site(&self, query: &SiteQuery<'_>) -> Result<Site> {
        if !self.can_search(Site::Gelbooru) {
            return Ok(Site::Danbooru);
        }
        if !self.danbooru.takes_whole(query).await? {
            return Ok(Site::Gelbooru);
        }
        // both can answer it whole, so whichever waits less on its rate limit. Danbooru's tags are better on a tie
        Ok(if self.gelbooru.wait_estimate() < self.danbooru.wait_estimate() {
            Site::Gelbooru
        } else {
            Site::Danbooru
        })
    }

    // the sites to favourite a post on: every one the user is logged in to
    pub fn logged_in_sites(&self) -> Vec<Site> {
        Site::ALL
            .into_iter()
            .filter(|site| match self.booru(*site) {
                Booru::Gelbooru(gelbooru) => gelbooru.logged_in(),
                Booru::Danbooru(danbooru) => danbooru.logged_in(),
            })
            .collect()
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
        let core = command
            .message
            .webview_ref()
            .try_state::<Arc<Core>>()
            .ok_or_else(|| InvokeError::from("The app is still starting"))?;
        Ok(CurrentCore(core.inner().clone()))
    }
}
