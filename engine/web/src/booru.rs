use crate::activity;
use crate::danbooru::Danbooru;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::gelbooru::Gelbooru;
use crate::limiter::Limiter;
use crate::search::Rating;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use web_time::{Duration, Instant};
use crate::platform::AppHandle;

pub const SETTINGS_STORE: &str = "settings.json";
pub const PAGE_SIZE: i64 = 100;
const TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(5), Duration::from_secs(20)];
const RETRY_STATUSES: [u16; 4] = [429, 502, 503, 504];

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    #[serde(alias = "kimochii")]
    Gelbooru,
    Danbooru,
}

impl Site {
    pub const ALL: [Site; 2] = [Site::Gelbooru, Site::Danbooru];

    pub fn name(self) -> &'static str {
        match self {
            Site::Gelbooru => "Gelbooru",
            Site::Danbooru => "Danbooru",
        }
    }

    pub fn database_file(self) -> &'static str {
        match self {
            Site::Gelbooru => "foryooru-gelbooru.db",
            Site::Danbooru => "foryooru-danbooru.db",
        }
    }
}

pub struct RawPost {
    pub id: i64,
    pub md5: String,
    pub rating: Rating,
    pub score: i64,
    pub tags: Vec<String>,
    pub width: i64,
    pub height: i64,
    pub preview_url: String,
    pub preview_width: i64,
    pub preview_height: i64,
    pub sample_url: String,
    pub file_url: String,
    pub posted_at: Option<i64>,
    pub lineage: Option<Lineage>,
}

#[derive(Clone, Copy)]
pub struct Lineage {
    pub parent: Option<i64>,
    pub has_children: bool,
}

pub use foryooru_core::search::{Order, SiteQuery};

pub struct TagDetail {
    pub kind: i64,
    pub count: i64,
}

pub fn setting(app: &AppHandle, key: &str) -> Result<String> {
    Ok(app
        .setting(key)
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default())
}

pub struct Requester {
    site: Site,
    db: Arc<Db>,
    http: reqwest::Client,
    limiter: Limiter,
}

pub struct Logged<'a> {
    pub endpoint: &'a str,
    pub query: &'a str,
    pub page: Option<i64>,
}

impl Requester {
    pub fn new(site: Site, db: Arc<Db>, http: reqwest::Client, limiter: Limiter) -> Requester {
        Requester {
            site,
            db,
            http,
            limiter,
        }
    }

    pub async fn send(
        &self,
        method: Method,
        url: &str,
        params: &[(&str, String)],
        logged: Logged<'_>,
    ) -> Result<Value> {
        self.logged(logged, async {
            let text = self.request(method, url, params, None).await?;
            Ok(if text.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text)?
            })
        })
        .await
    }

    pub async fn send_with_cookie(
        &self,
        method: Method,
        url: &str,
        params: &[(&str, String)],
        cookie: &str,
        logged: Logged<'_>,
    ) -> Result<String> {
        self.logged(logged, self.request(method, url, params, Some(cookie)))
            .await
    }

    async fn logged<T>(
        &self,
        logged: Logged<'_>,
        work: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let started = Instant::now();
        let outcome = work.await;
        let ms = started.elapsed().as_millis() as i64;
        if let Err(error) = activity::record_query(
            &self.db.conn(),
            logged.endpoint,
            logged.query,
            logged.page,
            outcome.as_ref().err(),
            ms,
        ) {
            crate::platform::log(&format!("Couldn't record a {} query: {error}", self.site.name()));
        }
        outcome
    }

    async fn request(
        &self,
        method: Method,
        url: &str,
        params: &[(&str, String)],
        cookie: Option<&str>,
    ) -> Result<String> {
        let mut delays = RETRY_DELAYS.iter();
        loop {
            self.limiter.acquire(None).await;
            let mut request = self
                .http
                .request(method.clone(), url)
                .timeout(TIMEOUT)
                .query(params);
            // browsers drop a Cookie header from fetch, so the relay takes the login cookie in its own header
            if let Some(cookie) = cookie {
                request = request.header("X-Gelbooru-Cookie", cookie);
            }
            let result = request.send().await;
            let worth_retrying = match &result {
                Ok(response) => RETRY_STATUSES.contains(&response.status().as_u16()),
                Err(error) => error.is_request() || error.is_timeout(),
            };
            if worth_retrying && let Some(delay) = delays.next() {
                crate::platform::sleep(*delay).await;
                continue;
            }
            let response = result?;
            let status = response.status().as_u16();
            let text = response.text().await?;
            if !(200..300).contains(&status) {
                let message = serde_json::from_str::<Value>(&text).ok().and_then(|body| {
                    body.get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
                return Err(match message {
                    Some(message) => Error::Rejected {
                        site: self.site.name(),
                        status,
                        message,
                    },
                    None => Error::Status(status),
                });
            }
            return Ok(text);
        }
    }
}

pub enum Booru {
    Gelbooru(Gelbooru),
    Danbooru(Danbooru),
}

impl Booru {
    pub fn new(site: Site, app: AppHandle, db: Arc<Db>, http: reqwest::Client) -> Booru {
        match site {
            Site::Gelbooru => Booru::Gelbooru(Gelbooru::new(app, db, http)),
            Site::Danbooru => Booru::Danbooru(Danbooru::new(app, db, http)),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        page: i64,
        known_total: Option<i64>,
    ) -> Result<(Vec<RawPost>, i64)> {
        match self {
            Booru::Gelbooru(gelbooru) => gelbooru.search(query, page).await,
            Booru::Danbooru(danbooru) => danbooru.search(query, page, known_total).await,
        }
    }

    pub async fn set_favourite(&self, post_id: i64, favourite: bool) -> Result<()> {
        match self {
            Booru::Gelbooru(gelbooru) => gelbooru.set_favourite(post_id, favourite).await,
            Booru::Danbooru(danbooru) => danbooru.set_favourite(post_id, favourite).await,
        }
    }

    // fetches one post without storing it, for shared links to the other site
    pub async fn preview(&self, id: i64) -> Result<RawPost> {
        match self {
            Booru::Gelbooru(gelbooru) => gelbooru.preview(id).await,
            Booru::Danbooru(danbooru) => danbooru.preview(id).await,
        }
    }

    pub async fn tag_details(&self, names: &[String]) -> Result<HashMap<String, TagDetail>> {
        match self {
            Booru::Gelbooru(gelbooru) => gelbooru.tag_details(names).await,
            Booru::Danbooru(danbooru) => danbooru.tag_details(names).await,
        }
    }

    pub async fn render(&self, query: &SiteQuery<'_>) -> Result<String> {
        let order_fits = self.order_fits(query.tags.len()).await?;
        let mut words = query.tags.clone();
        words.extend(
            Rating::ALL
                .iter()
                .filter(|rating| !query.ratings.contains(**rating))
                .map(|rating| match self {
                    Booru::Gelbooru(_) => format!("-rating:{}", rating.name()),
                    Booru::Danbooru(_) => format!("-rating:{}", &rating.name()[..1]),
                }),
        );
        if let Booru::Gelbooru(_) = self {
            words.extend(crate::search::blacklist_excludes(query.blacklist));
        }
        let order = match (self, query.order.filter(|_| order_fits)) {
            (_, None) => None,
            (Booru::Gelbooru(_), Some(Order::Score | Order::Popular)) => Some("sort:score"),
            (Booru::Gelbooru(_), Some(Order::Newest)) => Some("sort:id"),
            (Booru::Gelbooru(_), Some(Order::Random)) => Some("sort:random"),
            (Booru::Danbooru(_), Some(Order::Score)) => Some("order:score"),
            (Booru::Danbooru(_), Some(Order::Popular)) => Some("order:rank"),
            (Booru::Danbooru(_), Some(Order::Newest)) => Some("order:id_desc"),
            (Booru::Danbooru(_), Some(Order::Random)) => Some("order:random"),
        };
        words.extend(order.map(str::to_owned));
        words.dedup();
        Ok(words.join(" "))
    }

    pub fn ranks_popular(&self) -> bool {
        matches!(self, Booru::Danbooru(_))
    }

    pub async fn order_fits(&self, tag_count: usize) -> Result<bool> {
        Ok(match self {
            Booru::Gelbooru(_) => true,
            Booru::Danbooru(danbooru) => tag_count < danbooru.tag_limit().await?,
        })
    }

    pub fn max_offset(&self) -> i64 {
        match self {
            Booru::Gelbooru(_) => crate::gelbooru::MAX_OFFSET,
            Booru::Danbooru(_) => crate::danbooru::MAX_OFFSET,
        }
    }
}
