use crate::booru::{self, Lineage, Logged, PAGE_SIZE, RawPost, Requester, Site, TagDetail};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::limiter::Limiter;
use crate::search::Rating;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::platform::AppHandle;
use tauri_plugin_store::StoreExt;

const API_URL: &str = "https://gelbooru.com/index.php";
pub const SITE_URL: &str = "https://gelbooru.com";
#[cfg(desktop)]
pub const LOGIN_URL: &str = "https://gelbooru.com/index.php?page=account&s=login&code=00";
pub const CDN: &str = "https://img4.gelbooru.com";
pub const MAX_OFFSET: i64 = foryooru_core::fetch_plan::GELBOORU_MAX_OFFSET;
const REQUESTS_PER_SECOND: f64 = 2.0;
const FAVOURITE_ADDED: &str = "3";
const ALREADY_FAVOURITED: &str = "1";
const SESSION_SETTING: &str = "gelbooruSession";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GelbooruSession {
    pub user_id: String,
    pub pass_hash: String,
}

impl GelbooruSession {
    #[cfg(desktop)]
    pub fn from_cookies<'a>(cookies: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<Self> {
        let (mut user_id, mut pass_hash) = (None, None);
        for (name, value) in cookies {
            match name {
                "user_id" => user_id = Some(value.to_owned()),
                "pass_hash" => pass_hash = Some(value.to_owned()),
                _ => {}
            }
        }
        Some(GelbooruSession {
            user_id: user_id.filter(|id| !id.is_empty())?,
            pass_hash: pass_hash.filter(|hash| !hash.is_empty())?,
        })
    }

    fn cookie(&self) -> String {
        format!("user_id={}; pass_hash={}", self.user_id, self.pass_hash)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GelbooruPost {
    id: i64,
    md5: String,
    rating: String,
    score: Option<i64>,
    tags: String,
    width: i64,
    height: i64,
    preview_url: String,
    preview_width: i64,
    preview_height: i64,
    sample_url: String,
    file_url: String,
    created_at: String,
    parent_id: i64,
    has_children: Option<Value>,
}

impl GelbooruPost {
    fn into_raw(self) -> Option<RawPost> {
        Some(RawPost {
            id: self.id,
            rating: Rating::parse(&self.rating)?,
            score: self.score.unwrap_or(0),
            tags: split_tags(&self.tags),
            width: self.width,
            height: self.height,
            preview_url: self.preview_url,
            preview_width: self.preview_width,
            preview_height: self.preview_height,
            sample_url: self.sample_url,
            file_url: self.file_url,
            posted_at: parse_date(&self.created_at),
            md5: self.md5,
            lineage: self
                .has_children
                .and_then(|value| {
                    value
                        .as_bool()
                        .or_else(|| value.as_str().map(|text| text == "true"))
                })
                .map(|has_children| Lineage {
                    parent: (self.parent_id > 0).then_some(self.parent_id),
                    has_children,
                }),
        })
    }
}

#[derive(Deserialize)]
struct RawFavourite {
    favorite: i64,
    added: i64,
}

#[derive(Deserialize)]
struct RawTag {
    name: String,
    count: i64,
    #[serde(rename = "type")]
    kind: i64,
}

pub struct Gelbooru {
    app: AppHandle,
    requester: Requester,
    latest: Mutex<Option<i64>>,
}

impl Gelbooru {
    pub fn new(app: AppHandle, db: Arc<Db>, http: reqwest::Client) -> Gelbooru {
        Gelbooru {
            app,
            requester: Requester::new(
                Site::Gelbooru,
                db,
                http,
                Limiter::new(REQUESTS_PER_SECOND, 5.0),
            ),
            latest: Mutex::new(None),
        }
    }

    // cached for the session so paging through a popular search keeps the same id window
    pub async fn latest_id(&self) -> Result<i64> {
        if let Some(id) = *self.latest.lock().unwrap() {
            return Ok(id);
        }
        let body = self
            .call(
                &[
                    ("s", "post".into()),
                    ("tags", String::new()),
                    ("pid", "0".into()),
                    ("limit", "1".into()),
                ],
                Logged {
                    endpoint: "latest",
                    query: "id",
                    page: None,
                },
            )
            .await?;
        let posts: Vec<GelbooruPost> = match body.get("post") {
            Some(posts) => serde_json::from_value(posts.clone())?,
            None => Vec::new(),
        };
        let id = posts
            .first()
            .map(|post| post.id)
            .ok_or(Error::PostNotFound)?;
        *self.latest.lock().unwrap() = Some(id);
        Ok(id)
    }

    pub async fn set_favourite(&self, post_id: i64, favourite: bool) -> Result<()> {
        let session: GelbooruSession = self
            .app
            .store(booru::SETTINGS_STORE)?
            .get(SESSION_SETTING)
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or(Error::NeedsGelbooruLogin)?;
        let id = post_id.to_string();
        let logged = Logged {
            endpoint: "favorites",
            query: &id,
            page: None,
        };
        if favourite {
            let answer = self
                .requester
                .send_with_cookie(
                    Method::GET,
                    &format!("{SITE_URL}/public/addfav.php"),
                    &[("id", id.clone())],
                    &session.cookie(),
                    logged,
                )
                .await?;
            match answer.trim() {
                FAVOURITE_ADDED | ALREADY_FAVOURITED => Ok(()),
                _ => Err(Error::GelbooruFavouriteRefused),
            }
        } else {
            let params = [
                ("page", "favorites".to_owned()),
                ("s", "delete".to_owned()),
                ("id", id.clone()),
            ];
            self.requester
                .send_with_cookie(Method::GET, API_URL, &params, &session.cookie(), logged)
                .await
                .map(drop)
        }
    }

    async fn call(&self, params: &[(&str, String)], logged: Logged<'_>) -> Result<Value> {
        let (api_key, user_id) = (
            booru::setting(&self.app, "apiKey")?,
            booru::setting(&self.app, "userId")?,
        );
        if api_key.is_empty() || user_id.is_empty() {
            return Err(Error::MissingApiKey);
        }
        let mut all: Vec<(&str, String)> = vec![
            ("page", "dapi".into()),
            ("q", "index".into()),
            ("json", "1".into()),
            ("api_key", api_key),
            ("user_id", user_id),
        ];
        all.extend_from_slice(params);
        self.requester.send(Method::GET, API_URL, &all, logged).await
    }

    pub async fn search(&self, query: &str, page: i64) -> Result<(Vec<RawPost>, i64)> {
        let body = self
            .call(
                &[
                    ("s", "post".into()),
                    ("tags", query.into()),
                    ("pid", page.to_string()),
                    ("limit", PAGE_SIZE.to_string()),
                ],
                Logged {
                    endpoint: "post",
                    query,
                    page: Some(page),
                },
            )
            .await?;
        let total = body
            .pointer("/@attributes/count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let posts: Vec<GelbooruPost> = match body.get("post") {
            Some(posts) => serde_json::from_value(posts.clone())?,
            None => Vec::new(),
        };
        Ok((
            posts
                .into_iter()
                .filter_map(GelbooruPost::into_raw)
                .collect(),
            total,
        ))
    }

    pub async fn favourites_of(&self, user_id: &str) -> Result<Vec<(i64, i64)>> {
        let mut favourites = Vec::new();
        for page in 0.. {
            let body = self
                .call(
                    &[
                        ("s", "favorite".into()),
                        ("id", user_id.into()),
                        ("pid", page.to_string()),
                        ("limit", PAGE_SIZE.to_string()),
                    ],
                    Logged {
                        endpoint: "favorite",
                        query: user_id,
                        page: Some(page),
                    },
                )
                .await?;
            let rows: Vec<RawFavourite> = match body.get("favorite") {
                Some(rows) => serde_json::from_value(rows.clone())?,
                None => Vec::new(),
            };
            let short = (rows.len() as i64) < PAGE_SIZE;
            favourites.extend(rows.into_iter().map(|row| (row.favorite, row.added * 1000)));
            if short {
                break;
            }
        }
        Ok(favourites)
    }

    pub async fn tag_details(&self, names: &[String]) -> Result<HashMap<String, TagDetail>> {
        let joined = names.join(" ");
        let body = self
            .call(
                &[
                    ("s", "tag".into()),
                    ("names", joined.clone()),
                    ("limit", names.len().to_string()),
                ],
                Logged {
                    endpoint: "tag",
                    query: &joined,
                    page: None,
                },
            )
            .await?;
        let tags: Vec<RawTag> = match body.get("tag") {
            Some(tags) => serde_json::from_value(tags.clone())?,
            None => Vec::new(),
        };
        Ok(tags
            .into_iter()
            .map(|tag| {
                (
                    unescape(&tag.name),
                    TagDetail {
                        kind: tag.kind,
                        count: tag.count,
                    },
                )
            })
            .collect())
    }
}

pub fn split_tags(tags: &str) -> Vec<String> {
    unescape(tags)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn unescape(text: &str) -> String {
    html_escape::decode_html_entities(text).into_owned()
}

fn parse_date(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_str(value, "%a %b %d %H:%M:%S %z %Y")
        .ok()
        .map(|date| date.timestamp_millis())
}
