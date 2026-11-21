// tag suggestions from the sites' own autocomplete, added to the ones from tags already stored: every tag the site
// knows with its count, and from Danbooru the alias that led to a tag. on Gelbooru both sites are asked, Gelbooru's
// answers first since they're that site's names and counts
use crate::booru::Site;
use crate::catalogue::TagKind;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// suggestions are only worth showing while the person is still typing
const TIMEOUT: Duration = Duration::from_secs(3);
const LIMIT: usize = 10;
const GELBOORU_URL: &str = "https://gelbooru.com/index.php";
const DANBOORU_URL: &str = "https://danbooru.donmai.us/autocomplete.json";

#[derive(Serialize)]
pub struct Suggestion {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<TagKind>,
    pub count: Option<i64>,
    // the name typed that the site treats as this tag, like bare_feet for barefoot
    pub alias: Option<String>,
}

#[derive(Deserialize)]
struct GelbooruEntry {
    value: String,
    post_count: Option<String>,
    category: Option<String>,
}

#[derive(Deserialize)]
struct DanbooruEntry {
    value: String,
    post_count: Option<i64>,
    category: Option<i64>,
    antecedent: Option<String>,
}

pub async fn suggest(http: &reqwest::Client, site: Site, typed: &str) -> Vec<Suggestion> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Vec::new();
    }
    match site {
        Site::Gelbooru => {
            let (gelbooru, danbooru) = tokio::join!(gelbooru(http, typed), danbooru(http, typed));
            merge(gelbooru, danbooru)
        }
        Site::Danbooru => danbooru(http, typed).await,
    }
}

// later lists only add tags the earlier ones lack, except that an alias is kept wherever it turns up
fn merge(first: Vec<Suggestion>, second: Vec<Suggestion>) -> Vec<Suggestion> {
    let mut merged = first;
    for suggestion in second {
        match merged.iter_mut().find(|existing| existing.name == suggestion.name) {
            Some(existing) => existing.alias = existing.alias.take().or(suggestion.alias),
            None => merged.push(suggestion),
        }
    }
    merged
}

async fn gelbooru(http: &reqwest::Client, typed: &str) -> Vec<Suggestion> {
    let request = http
        .get(GELBOORU_URL)
        .timeout(TIMEOUT)
        .query(&[("page", "autocomplete2"), ("type", "tag_query"), ("term", typed), ("limit", &LIMIT.to_string())]);
    let entries: Vec<GelbooruEntry> = match fetch(request).await {
        Some(entries) => entries,
        None => return Vec::new(),
    };
    entries
        .into_iter()
        .map(|entry| Suggestion {
            name: html_escape::decode_html_entities(&entry.value).into_owned(),
            kind: entry.category.as_deref().and_then(|category| match category {
                "tag" => Some(TagKind::General),
                "artist" => Some(TagKind::Artist),
                "copyright" => Some(TagKind::Copyright),
                "character" => Some(TagKind::Character),
                "metadata" => Some(TagKind::Metadata),
                "deprecated" => Some(TagKind::Deprecated),
                _ => None,
            }),
            count: entry.post_count.and_then(|count| count.parse().ok()),
            alias: None,
        })
        .collect()
}

async fn danbooru(http: &reqwest::Client, typed: &str) -> Vec<Suggestion> {
    let request = http.get(DANBOORU_URL).timeout(TIMEOUT).query(&[
        ("search[query]", typed),
        ("search[type]", "tag_query"),
        ("limit", &LIMIT.to_string()),
    ]);
    let entries: Vec<DanbooruEntry> = match fetch(request).await {
        Some(entries) => entries,
        None => return Vec::new(),
    };
    entries
        .into_iter()
        .map(|entry| Suggestion {
            name: entry.value,
            kind: entry.category.and_then(TagKind::from_code),
            count: entry.post_count,
            alias: entry.antecedent,
        })
        .collect()
}

// a site that's slow, offline or answering oddly just adds nothing
async fn fetch<T: serde::de::DeserializeOwned>(request: reqwest::RequestBuilder) -> Option<Vec<T>> {
    let response = request.send().await.ok()?.error_for_status().ok()?;
    response.json().await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(name: &str, alias: Option<&str>) -> Suggestion {
        Suggestion {
            name: name.to_owned(),
            kind: None,
            count: None,
            alias: alias.map(str::to_owned),
        }
    }

    // asks the real sites, so it only runs when asked for: cargo test --lib autocomplete -- --ignored
    #[tokio::test]
    #[ignore]
    async fn both_sites_answer_with_tags() {
        let http = reqwest::Client::builder().user_agent("foryooru/0.1").build().unwrap();
        let gelbooru = suggest(&http, Site::Gelbooru, "bare_fe").await;
        let danbooru = suggest(&http, Site::Danbooru, "bare_feet").await;
        assert!(gelbooru.iter().any(|tag| tag.kind.is_some() && tag.count.is_some()));
        assert!(danbooru.iter().any(|tag| tag.name == "barefoot" && tag.alias.as_deref() == Some("bare_feet")));
    }

    #[test]
    fn merging_keeps_the_first_sites_order_and_adds_the_rest() {
        let gelbooru = vec![suggestion("feet", None), suggestion("barefoot", None)];
        let danbooru = vec![suggestion("barefoot", Some("bare_feet")), suggestion("female", None)];
        let merged = merge(gelbooru, danbooru);
        let names: Vec<(&str, Option<&str>)> = merged.iter().map(|tag| (tag.name.as_str(), tag.alias.as_deref())).collect();
        assert_eq!(names, [("feet", None), ("barefoot", Some("bare_feet")), ("female", None)]);
    }
}
