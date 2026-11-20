use crate::booru::{Booru, PAGE_SIZE, RawPost, Site};
use crate::catalogue;
use crate::core::Core;
use crate::db::now_ms;
use crate::error::{Error, Result};
use crate::gelbooru::{CDN, Gelbooru};
use crate::search::Rating;
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

const PAGE_PAUSE: Duration = Duration::from_millis(500);
pub const EVENT: &str = "import";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatus {
    pub running: bool,
    pub source: String,
    pub total: Option<usize>,
    pub imported: usize,
    pub skipped: usize,
    pub error: Option<String>,
}

fn start(
    core: &Arc<Core>,
    source: String,
    total: Option<usize>,
    task: impl Future<Output = Result<()>> + Send + 'static,
) -> Result<()> {
    if core.import.get().running {
        return Err(Error::ImportRunning);
    }
    core.import.update(&core.app, |status| {
        *status = ImportStatus {
            running: true,
            source,
            total,
            ..ImportStatus::default()
        }
    });
    let core = core.clone();
    crate::platform::spawn(async move {
        let result = task.await;
        core.import.update(&core.app, |status| {
            status.running = false;
            status.error = result.err().map(|error| error.to_string());
        });
        crate::sync::sync_soon(&core);
    });
    Ok(())
}

pub fn favourites(core: &Arc<Core>, user: String) -> Result<()> {
    let task_core = core.clone();
    let source = format!("{} user {user}", core.site.name());
    start(core, source, None, async move {
        match &task_core.booru {
            Booru::Gelbooru(gelbooru) => import_gelbooru(&task_core, gelbooru, &user).await,
            Booru::Danbooru(_) => import_danbooru(&task_core, &user).await,
        }
    })
}

fn save_dated_likes(conn: &Connection, pending: &mut HashMap<i64, i64>) -> Result<usize> {
    let mut saved = Vec::new();
    let tx = conn.unchecked_transaction()?;
    {
        let mut statement = tx.prepare_cached(
            "INSERT INTO likes (post_id, sort_at, liked_at) SELECT ?1, ?2, ?2 WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?1)
             ON CONFLICT (post_id) DO UPDATE SET sort_at = excluded.sort_at, liked_at = excluded.liked_at",
        )?;
        for (id, added) in pending.iter() {
            if statement.execute(params![id, added])? > 0 {
                saved.push(*id);
            }
        }
    }
    tx.commit()?;
    for id in &saved {
        pending.remove(id);
    }
    Ok(saved.len())
}

async fn import_gelbooru(core: &Arc<Core>, gelbooru: &Gelbooru, user_id: &str) -> Result<()> {
    let mut pending: HashMap<i64, i64> =
        gelbooru.favourites_of(user_id).await?.into_iter().collect();
    let total = pending.len();
    let saved = save_dated_likes(&core.db.conn(), &mut pending)?;
    core.import.update(&core.app, |status| {
        status.total = Some(total);
        status.imported = saved;
    });
    let mut page = 0;
    while !pending.is_empty() && page * PAGE_SIZE < crate::gelbooru::MAX_OFFSET {
        crate::platform::sleep(PAGE_PAUSE).await;
        let (raws, _) = gelbooru.search(&format!("fav:{user_id}"), page).await?;
        let saved = {
            let mut conn = core.db.conn();
            let stored = catalogue::store_posts(&mut conn, &raws, false)?;
            catalogue::after_store(core, &stored);
            save_dated_likes(&conn, &mut pending)?
        };
        core.import
            .update(&core.app, |status| status.imported += saved);
        if (raws.len() as i64) < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    core.import
        .update(&core.app, |status| status.skipped = pending.len());
    Ok(())
}

async fn import_danbooru(core: &Arc<Core>, user: &str) -> Result<()> {
    let started = now_ms();
    let query = format!("ordfav:{user}");
    let mut total = None;
    let mut page = 0;
    loop {
        let (raws, found_total) = core.booru.search(&query, page, total).await?;
        total = Some(found_total);
        let ids: Vec<i64> = raws.iter().map(|raw| raw.id).collect();
        let saved = {
            let mut conn = core.db.conn();
            let stored = catalogue::store_posts(&mut conn, &raws, false)?;
            catalogue::after_store(core, &stored);
            save_undated_likes(&conn, &ids, started - page * PAGE_SIZE)?
        };
        core.import.update(&core.app, |status| {
            status.total = usize::try_from(found_total).ok();
            status.imported += saved;
        });
        page += 1;
        if page * PAGE_SIZE >= found_total.min(core.booru.max_offset()) {
            break;
        }
        crate::platform::sleep(PAGE_PAUSE).await;
    }
    core.import.update(&core.app, |status| {
        status.skipped = status.total.unwrap_or(0).saturating_sub(status.imported);
    });
    Ok(())
}

fn save_undated_likes(conn: &Connection, ids: &[i64], newest_sort_at: i64) -> Result<usize> {
    let mut saved = 0;
    let tx = conn.unchecked_transaction()?;
    {
        let mut statement = tx.prepare_cached(
            "INSERT INTO likes (post_id, sort_at, liked_at) SELECT ?1, ?2, NULL WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?1)
             ON CONFLICT (post_id) DO UPDATE SET sort_at = excluded.sort_at WHERE likes.liked_at IS NULL",
        )?;
        let mut exists = tx.prepare_cached("SELECT EXISTS (SELECT 1 FROM posts WHERE id = ?1)")?;
        for (index, id) in ids.iter().enumerate() {
            statement.execute(params![id, newest_sort_at - index as i64])?;
            saved += exists.query_row([id], |row| row.get::<_, bool>(0))? as usize;
        }
    }
    tx.commit()?;
    Ok(saved)
}

fn unwrap_proxy(url: Option<&str>) -> String {
    let Some(url) = url.filter(|url| !url.is_empty()) else {
        return String::new();
    };
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == "to")
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_else(|| url.to_owned())
}

fn md5_in(url: &str) -> Option<String> {
    let file = url.rsplit('/').next()?;
    let stem = file.split('.').next()?;
    let md5 = stem
        .trim_start_matches("sample_")
        .trim_start_matches("thumbnail_");
    (md5.len() == 32 && md5.chars().all(|c| c.is_ascii_hexdigit())).then(|| md5.to_lowercase())
}

fn from_booruview(entry: &Value) -> Option<RawPost> {
    let id = entry.get("id")?.as_i64()?;
    let thumbnail = unwrap_proxy(entry.get("thumbnail_url").and_then(Value::as_str));
    let md5 = ["thumbnail_url", "lowres_url", "image_url"]
        .iter()
        .find_map(|key| md5_in(&unwrap_proxy(entry.get(*key).and_then(Value::as_str))))?;
    let directory = format!("{}/{}", &md5[..2], &md5[2..4]);
    let width = entry.get("width").and_then(Value::as_i64).unwrap_or(0);
    let height = entry.get("height").and_then(Value::as_i64).unwrap_or(0);
    let scale = (250.0 / width.max(height).max(1) as f64).min(1.0);
    let tags: Vec<&str> = entry
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .filter(|tag| !tag.starts_with("rating:"))
                .collect()
        })
        .unwrap_or_default();
    Some(RawPost {
        id,
        rating: Rating::parse(entry.get("rating").and_then(Value::as_str)?)?,
        score: entry.get("score").and_then(Value::as_i64).unwrap_or(0),
        tags: tags.into_iter().map(str::to_owned).collect(),
        width,
        height,
        preview_url: format!("{CDN}/thumbnails/{directory}/thumbnail_{md5}.jpg"),
        preview_width: Some((width as f64 * scale).round() as i64)
            .filter(|width| *width > 0)
            .unwrap_or(250),
        preview_height: Some((height as f64 * scale).round() as i64)
            .filter(|height| *height > 0)
            .unwrap_or(250),
        lineage: None,
        sample_url: if thumbnail.contains("sample_") {
            format!("{CDN}/samples/{directory}/sample_{md5}.jpg")
        } else {
            String::new()
        },
        file_url: String::new(),
        md5,
        posted_at: None,
    })
}

fn parse_file(contents: &str) -> Result<Vec<Value>> {
    let data: Value = serde_json::from_str(contents)
        .map_err(|_| Error::InvalidImportFile("That isn't a valid JSON file".into()))?;
    let Value::Array(items) = data else {
        return Err(Error::InvalidImportFile("Expected a list of posts".into()));
    };
    let mut seen = HashSet::new();
    let entries: Vec<Value> = items
        .into_iter()
        .filter(|item| {
            item.get("id")
                .and_then(Value::as_i64)
                .is_some_and(|id| id > 0 && seen.insert(id))
        })
        .collect();
    if entries.is_empty() {
        return Err(Error::InvalidImportFile(
            "No posts with a Gelbooru \"id\" found in that file".into(),
        ));
    }
    Ok(entries)
}

pub fn from_file(core: &Arc<Core>, name: String, contents: &str) -> Result<()> {
    if core.site != Site::Gelbooru {
        return Err(Error::NeedsGelbooru);
    }
    let entries = parse_file(contents)?;
    let task_core = core.clone();
    start(core, name, Some(entries.len()), async move {
        import_file(&task_core, entries).await
    })
}

async fn import_file(core: &Arc<Core>, entries: Vec<Value>) -> Result<()> {
    let started = now_ms();
    for (chunk_index, chunk) in entries.chunks(PAGE_SIZE as usize).enumerate() {
        let ids: Vec<i64> = chunk
            .iter()
            .filter_map(|entry| entry.get("id")?.as_i64())
            .collect();
        let mut unresolved = Vec::new();
        {
            let raws: Vec<RawPost> = chunk
                .iter()
                .filter_map(|entry| {
                    let raw = from_booruview(entry);
                    if raw.is_none() {
                        unresolved.extend(entry.get("id").and_then(Value::as_i64));
                    }
                    raw
                })
                .collect();
            let mut conn = core.db.conn();
            let stored = catalogue::store_posts(&mut conn, &raws, true)?;
            catalogue::after_store(core, &stored);
        }
        for id in unresolved {
            crate::platform::sleep(PAGE_PAUSE).await;
            if let Ok((raws, _)) = core.booru.search(&format!("id:{id}"), 0, None).await {
                let stored = catalogue::store_posts(&mut core.db.conn(), &raws, false)?;
                catalogue::after_store(core, &stored);
            }
        }
        let saved = save_undated_likes(
            &core.db.conn(),
            &ids,
            started - (chunk_index as i64) * PAGE_SIZE,
        )?;
        core.import.update(&core.app, |status| {
            status.imported += saved;
            status.skipped += ids.len() - saved;
        });
    }
    Ok(())
}
