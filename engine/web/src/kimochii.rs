use crate::booru::{self, Logged, RawPost, Requester, Site};
use crate::catalogue;
use crate::core::Core;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::limiter::Limiter;
use crate::model::{self, Model};
use crate::recommend::{self, UserData};
use foryooru_core::protocol::{
    FeedRequest, FeedResponse, ForYouRequest, ForYouResponse, ServedPost, SimilarRequest,
    SimilarResponse, Version,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use crate::platform::AppHandle;

pub const URL_SETTING: &str = "kimochiiUrl";
const REQUESTS_PER_SECOND: f64 = 50.0;
const RANKING_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Kimochii {
    app: AppHandle,
    requester: Requester,
}

pub fn active(core: &Core) -> Option<&Kimochii> {
    let configured = booru::setting(&core.app, URL_SETTING).is_ok_and(|url| !url.trim().is_empty());
    (core.site == Site::Gelbooru && configured).then_some(&core.pool)
}

pub fn user_and_model(core: &Core) -> Result<(UserData, Model)> {
    let conn = core.db.conn();
    Ok((recommend::load_user(&conn)?, model::load(&conn)?))
}

pub fn store(core: &Arc<Core>, posts: Vec<ServedPost>) -> Result<()> {
    let raws: Vec<RawPost> = posts.into_iter().map(raw).collect();
    let stored = catalogue::store_posts(&mut core.db.conn(), &raws, false)?;
    catalogue::after_store(core, &stored);
    Ok(())
}

fn raw(post: ServedPost) -> RawPost {
    RawPost {
        id: post.id,
        md5: post.md5,
        rating: post.rating,
        score: post.score,
        tags: post.tags,
        width: post.width,
        height: post.height,
        preview_url: post.preview_url,
        preview_width: post.preview_width,
        preview_height: post.preview_height,
        sample_url: post.sample_url,
        file_url: post.file_url,
        posted_at: post.posted_at,
        lineage: None,
    }
}

impl Kimochii {
    pub fn new(app: AppHandle, db: Arc<Db>, http: reqwest::Client) -> Kimochii {
        Kimochii {
            app,
            requester: Requester::new(
                Site::Gelbooru,
                db,
                http,
                Limiter::new(REQUESTS_PER_SECOND, REQUESTS_PER_SECOND),
            ),
        }
    }

    fn url(&self, path: &str) -> Result<String> {
        let server = booru::setting(&self.app, URL_SETTING)?;
        let server = server.trim().trim_end_matches('/');
        if server.is_empty() {
            return Err(Error::MissingServer);
        }
        Ok(format!("{server}/{path}"))
    }

    async fn call<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        query: &str,
        page: Option<i64>,
    ) -> Result<T> {
        self.requester
            .post_json(
                &self.url(path)?,
                body,
                RANKING_TIMEOUT,
                Logged {
                    endpoint: path,
                    query,
                    page,
                },
            )
            .await
    }

    pub async fn version(&self) -> Result<Version> {
        let url = self.url("version")?;
        self.requester.get_json(&url).await
    }

    pub async fn for_you(&self, request: &ForYouRequest) -> Result<ForYouResponse> {
        self.call("for-you", request, &request.chips.join(" "), None)
            .await
    }

    pub async fn feed(&self, request: &FeedRequest) -> Result<FeedResponse> {
        self.call("feed", request, &request.chips.join(" "), None)
            .await
    }

    pub async fn similar(&self, request: &SimilarRequest) -> Result<SimilarResponse> {
        self.call("similar", request, &request.chips.join(" "), None)
            .await
    }
}
