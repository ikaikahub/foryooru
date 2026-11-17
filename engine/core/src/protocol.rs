use crate::feed::FeedOptions;
use crate::model::Model;
use crate::recommend::UserData;
use crate::search::Rating;
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "foryooru-pool-1";

#[derive(Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub posts: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedPost {
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
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedTag {
    pub name: String,
    pub kind: Option<i64>,
    pub count: i64,
}

#[derive(Serialize, Deserialize)]
pub struct SearchRequest {
    pub chips: Vec<String>,
    pub page: i64,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub posts: Vec<ServedPost>,
    pub total: i64,
}

#[derive(Serialize, Deserialize)]
pub struct TagsRequest {
    pub names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForYouRequest {
    pub user: UserData,
    pub model: Model,
    pub chips: Vec<String>,
    pub hide_liked: bool,
    pub discover: bool,
    pub shown: Vec<i64>,
    pub limit: usize,
}

#[derive(Serialize, Deserialize)]
pub struct RankedPost {
    pub id: i64,
    pub explore: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ForYouResponse {
    pub ranked: Vec<RankedPost>,
    pub posts: Vec<ServedPost>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRequest {
    pub user: UserData,
    pub model: Model,
    pub chips: Vec<String>,
    pub seen: Vec<i64>,
    pub since: i64,
    pub session_likes: Vec<i64>,
    pub options: FeedOptions,
}

#[derive(Serialize, Deserialize)]
pub struct FeedResponse {
    pub ids: Vec<i64>,
    pub exhausted: bool,
    pub posts: Vec<ServedPost>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarRequest {
    pub user: UserData,
    pub chips: Vec<String>,
    pub per_artist: Option<usize>,
    pub limit: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SimilarResponse {
    pub ids: Vec<i64>,
    pub posts: Vec<ServedPost>,
}
