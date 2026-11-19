use crate::booru::{self, Lineage, Logged, PAGE_SIZE, RawPost, Requester, Site, TagDetail};
use crate::catalogue;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::limiter::Limiter;
use crate::search::Rating;
use reqwest::Method;
use serde::Deserialize;
use rusqlite::OptionalExtension;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use crate::platform::AppHandle;
use tokio::sync::Mutex;

const API_URL: &str = "https://danbooru.donmai.us";
const MAX_PAGES: i64 = 1000;
pub const MAX_OFFSET: i64 = MAX_PAGES * PAGE_SIZE;
const REQUESTS_PER_SECOND: f64 = 1.0;
const FREE_TAG_LIMIT: usize = 2;
const DEPRECATED_KIND: i64 = 6;
const THUMBNAIL_VARIANT: &str = "180x180";
const STILL_VARIANT: &str = "720x720";
const ALREADY_FAVOURITED: u16 = 422;
const NOT_FAVOURITED: u16 = 404;
// Danbooru answers a sorted search on a huge tag (like 1girl) with a 500 once its query times out
const QUERY_TIMED_OUT: u16 = 500;
// these don't count towards the account's tag limit
const FREE_METATAGS: [&str; 4] = ["rating:", "status:", "is:", "age:"];

#[derive(Deserialize, Default)]
#[serde(default)]
struct DanbooruPost {
    id: i64,
    md5: Option<String>,
    rating: Option<String>,
    score: i64,
    tag_string: String,
    image_width: i64,
    image_height: i64,
    file_url: Option<String>,
    large_file_url: Option<String>,
    preview_file_url: Option<String>,
    created_at: Option<String>,
    media_asset: MediaAsset,
    parent_id: Option<i64>,
    has_children: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MediaAsset {
    variants: Vec<Variant>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Variant {
    #[serde(rename = "type")]
    kind: String,
    url: String,
    width: i64,
    height: i64,
}

impl DanbooruPost {
    fn into_raw(self) -> Option<RawPost> {
        let file_url = self.file_url?;
        let rating = Rating::ALL
            .into_iter()
            .find(|rating| self.rating.as_deref() == Some(&rating.name()[..1]))?;
        let thumbnail = self
            .media_asset
            .variants
            .iter()
            .find(|variant| variant.kind == THUMBNAIL_VARIANT);
        let sample_url = self
            .large_file_url
            .filter(|url| catalogue::is_still(url))
            .or_else(|| {
                self.media_asset
                    .variants
                    .iter()
                    .find(|variant| variant.kind == STILL_VARIANT)
                    .map(|variant| variant.url.clone())
            })
            .unwrap_or_default();
        Some(RawPost {
            id: self.id,
            md5: self.md5.unwrap_or_default(),
            rating,
            score: self.score,
            tags: self
                .tag_string
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            width: self.image_width,
            height: self.image_height,
            preview_url: self.preview_file_url.unwrap_or_default(),
            preview_width: thumbnail.map_or(self.image_width, |variant| variant.width),
            preview_height: thumbnail.map_or(self.image_height, |variant| variant.height),
            sample_url,
            file_url,
            posted_at: self
                .created_at
                .and_then(|date| chrono::DateTime::parse_from_rfc3339(&date).ok())
                .map(|date| date.timestamp_millis()),
            lineage: self.has_children.map(|has_children| Lineage {
                parent: self.parent_id,
                has_children,
            }),
        })
    }
}

#[derive(Deserialize)]
struct DanbooruTag {
    name: String,
    post_count: i64,
    category: i64,
    is_deprecated: bool,
}

#[derive(PartialEq)]
struct Credentials {
    login: String,
    api_key: String,
}

pub struct Danbooru {
    app: AppHandle,
    db: Arc<Db>,
    requester: Requester,
    tag_limit: Mutex<Option<(Credentials, usize)>>,
}

impl Danbooru {
    pub fn new(app: AppHandle, db: Arc<Db>, http: reqwest::Client) -> Danbooru {
        Danbooru {
            app,
            db: db.clone(),
            requester: Requester::new(
                Site::Danbooru,
                db,
                http,
                Limiter::new(REQUESTS_PER_SECOND, 10.0),
            ),
            tag_limit: Mutex::new(None),
        }
    }

    fn credentials(&self) -> Result<Option<Credentials>> {
        let credentials = Credentials {
            login: booru::setting(&self.app, "danbooruLogin")?,
            api_key: booru::setting(&self.app, "danbooruApiKey")?,
        };
        Ok(
            (!credentials.login.is_empty() && !credentials.api_key.is_empty())
                .then_some(credentials),
        )
    }

    pub async fn tag_limit(&self) -> Result<usize> {
        let Some(credentials) = self.credentials()? else {
            return Ok(FREE_TAG_LIMIT);
        };
        let mut known = self.tag_limit.lock().await;
        if let Some((owner, limit)) = known.as_ref()
            && *owner == credentials
        {
            return Ok(*limit);
        }
        let profile = self
            .call(
                "profile.json",
                &[],
                Logged {
                    endpoint: "profile",
                    query: "",
                    page: None,
                },
            )
            .await?;
        let limit = profile
            .get("tag_query_limit")
            .and_then(Value::as_u64)
            .map_or(FREE_TAG_LIMIT, |limit| limit as usize);
        *known = Some((credentials, limit));
        Ok(limit)
    }

    async fn call(
        &self,
        path: &str,
        params: &[(&str, String)],
        logged: Logged<'_>,
    ) -> Result<Value> {
        self.send(Method::GET, path, params, logged).await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        logged: Logged<'_>,
    ) -> Result<Value> {
        let mut all: Vec<(&str, String)> = params.to_vec();
        if let Some(credentials) = self.credentials()? {
            all.push(("login", credentials.login));
            all.push(("api_key", credentials.api_key));
        }
        self.requester
            .send(method, &format!("{API_URL}/{path}"), &all, logged)
            .await
    }

    // the Danbooru side of a shared link preview, see Gelbooru::preview
    pub async fn preview(&self, id: i64) -> Result<RawPost> {
        let query = id.to_string();
        let body = self
            .call(
                &format!("posts/{id}.json"),
                &[],
                Logged {
                    endpoint: "preview",
                    query: &query,
                    page: None,
                },
            )
            .await?;
        let post: DanbooruPost = serde_json::from_value(body)?;
        post.into_raw().ok_or(Error::PostNotFound)
    }

    pub async fn set_favourite(&self, post_id: i64, favourite: bool) -> Result<()> {
        if self.credentials()?.is_none() {
            return Err(Error::NeedsDanbooruLogin);
        }
        let query = post_id.to_string();
        let logged = Logged {
            endpoint: "favorites",
            query: &query,
            page: None,
        };
        let (outcome, already) = if favourite {
            let params = [("post_id", query.clone())];
            let outcome = self
                .send(Method::POST, "favorites.json", &params, logged)
                .await;
            (outcome, ALREADY_FAVOURITED)
        } else {
            let path = format!("favorites/{post_id}.json");
            let outcome = self.send(Method::DELETE, &path, &[], logged).await;
            (outcome, NOT_FAVOURITED)
        };
        match outcome {
            Ok(_) => Ok(()),
            Err(Error::Rejected { status, .. } | Error::Status(status)) if status == already => {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        page: i64,
        known_total: Option<i64>,
    ) -> Result<(Vec<RawPost>, i64)> {
        match self.search_once(query, page, known_total).await {
            Err(Error::Status(QUERY_TIMED_OUT) | Error::Rejected { status: QUERY_TIMED_OUT, .. })
                if query.split_whitespace().any(|word| word.starts_with("order:")) =>
            {
                // unsorted is cheap for Danbooru, so newest first still brings posts back
                let unsorted: Vec<&str> = query
                    .split_whitespace()
                    .filter(|word| !word.starts_with("order:"))
                    .collect();
                self.search_once(&unsorted.join(" "), page, known_total).await
            }
            other => other,
        }
    }

    // Seeding searches one tag at a time, and that tag's post count is already stored,
    // so the separate counts request is only needed for real multi-tag searches
    fn total_without_counting(&self, query: &str) -> Result<Option<i64>> {
        if query.split_whitespace().any(|word| word == "order:random") {
            return Ok(Some(0));
        }
        let mut plain = query.split_whitespace().filter(|word| {
            let word = word.trim_start_matches('-');
            !word.starts_with("order:") && !FREE_METATAGS.iter().any(|free| word.starts_with(free))
        });
        let (Some(tag), None) = (plain.next(), plain.next()) else {
            return Ok(None);
        };
        if tag.starts_with('-') || tag.contains(':') {
            return Ok(None);
        }
        Ok(self
            .db
            .conn()
            .query_row("SELECT site_count FROM tags WHERE name = ?1", [tag], |row| row.get::<_, Option<i64>>(0))
            .optional()?
            .flatten())
    }

    async fn search_once(
        &self,
        query: &str,
        page: i64,
        known_total: Option<i64>,
    ) -> Result<(Vec<RawPost>, i64)> {
        let body = self
            .call(
                "posts.json",
                &[
                    ("tags", query.into()),
                    ("page", (page + 1).to_string()),
                    ("limit", PAGE_SIZE.to_string()),
                ],
                Logged {
                    endpoint: "posts",
                    query,
                    page: Some(page),
                },
            )
            .await?;
        let posts: Vec<DanbooruPost> = serde_json::from_value(body)?;
        let returned = posts.len() as i64;
        let counted = match known_total {
            Some(total) if page > 0 => Some(total),
            _ => match self.total_without_counting(query)? {
                Some(total) => Some(total),
                None => self.count(query).await.ok().flatten(),
            },
        };
        let total = counted.unwrap_or(0).max(estimate_total(page, returned));
        Ok((
            posts
                .into_iter()
                .filter_map(DanbooruPost::into_raw)
                .collect(),
            total,
        ))
    }

    async fn count(&self, query: &str) -> Result<Option<i64>> {
        let body = self
            .call(
                "counts/posts.json",
                &[("tags", query.into())],
                Logged {
                    endpoint: "counts",
                    query,
                    page: None,
                },
            )
            .await?;
        Ok(body.pointer("/counts/posts").and_then(Value::as_i64))
    }

    pub async fn tag_details(&self, names: &[String]) -> Result<HashMap<String, TagDetail>> {
        let joined = names.join(",");
        let body = self
            .call(
                "tags.json",
                &[
                    ("search[name_comma]", joined.clone()),
                    ("limit", names.len().to_string()),
                ],
                Logged {
                    endpoint: "tags",
                    query: &joined,
                    page: None,
                },
            )
            .await?;
        let tags: Vec<DanbooruTag> = serde_json::from_value(body)?;
        Ok(tags
            .into_iter()
            .map(|tag| {
                (
                    tag.name,
                    TagDetail {
                        kind: if tag.is_deprecated {
                            DEPRECATED_KIND
                        } else {
                            tag.category
                        },
                        count: tag.post_count,
                    },
                )
            })
            .collect())
    }
}

fn estimate_total(page: i64, returned: i64) -> i64 {
    let seen = page * PAGE_SIZE + returned;
    if returned < PAGE_SIZE {
        seen
    } else {
        seen + PAGE_SIZE
    }
}
