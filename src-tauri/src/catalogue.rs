use crate::booru::RawPost;
use crate::core::Core;
use crate::db::now_ms;
use crate::error::Result;
use crate::queue::{Job, Priority};
use crate::search::normalize_query;
use crate::timing;
pub use foryooru_core::kinds::{Media, TagKind, extension, is_still, media_of};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub const SEARCH_PAGE_MAX_AGE: i64 = 5 * 60 * 1000;
const TAG_BATCH: usize = 100;

#[derive(Serialize, Clone)]
pub struct Tag {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<TagKind>,
    pub count: Option<i64>,
}

impl Tag {
    fn from_row(row: &rusqlite::Row, offset: usize) -> rusqlite::Result<Tag> {
        Ok(Tag {
            name: row.get(offset)?,
            kind: row
                .get::<_, Option<i64>>(offset + 1)?
                .and_then(TagKind::from_code),
            count: row.get(offset + 2)?,
        })
    }
}

#[derive(Serialize)]
pub struct TagGroup {
    pub label: &'static str,
    pub tags: Vec<Tag>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: i64,
    pub rating: String,
    pub score: i64,
    pub width: i64,
    pub height: i64,
    pub preview_width: i64,
    pub preview_height: i64,
    pub posted_at: Option<i64>,
    pub file_url: String,
    pub media: Media,
    pub file_ext: String,
    pub liked: bool,
    pub disliked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostDetail {
    #[serde(flatten)]
    pub post: Post,
    pub tag_groups: Vec<TagGroup>,
    pub blocked_by: Vec<String>,
}

const POST_COLUMNS: &str = "p.id, p.rating, p.score, p.width, p.height, p.preview_width, p.preview_height, p.posted_at, p.file_url,
    l.post_id IS NOT NULL, d.post_id IS NOT NULL
    FROM posts p LEFT JOIN likes l ON l.post_id = p.id LEFT JOIN dislikes d ON d.post_id = p.id";

fn post_from_row(row: &rusqlite::Row) -> rusqlite::Result<Post> {
    let file_url: String = row.get(8)?;
    Ok(Post {
        id: row.get(0)?,
        rating: row.get(1)?,
        score: row.get(2)?,
        width: row.get(3)?,
        height: row.get(4)?,
        preview_width: row.get(5)?,
        preview_height: row.get(6)?,
        posted_at: row.get(7)?,
        media: media_of(&file_url),
        file_ext: extension(&file_url).to_uppercase(),
        file_url,
        liked: row.get(9)?,
        disliked: row.get(10)?,
    })
}

pub fn posts(conn: &Connection, ids: &[i64]) -> Result<Vec<Post>> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {POST_COLUMNS} WHERE p.id IN (SELECT value FROM json_each(?1))"
    ))?;
    let mut by_id: HashMap<i64, Post> = statement
        .query_map([serde_json::to_string(ids)?], post_from_row)?
        .map(|post| post.map(|post| (post.id, post)))
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

pub fn post_details(conn: &Connection, ids: &[i64]) -> Result<Vec<PostDetail>> {
    let mut tags_of = conn.prepare_cached(
        "SELECT t.name, t.type, t.site_count FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
         WHERE pt.post_id = ?1 ORDER BY t.name",
    )?;
    let mut blocked_by = conn.prepare_cached(
        "SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id JOIN blacklist b ON b.name = t.name
         WHERE pt.post_id = ?1 ORDER BY t.name",
    )?;
    posts(conn, ids)?
        .into_iter()
        .map(|post| {
            let tags: Vec<Tag> = tags_of
                .query_map([post.id], |row| Tag::from_row(row, 0))?
                .collect::<rusqlite::Result<_>>()?;
            let blocked_by = blocked_by
                .query_map([post.id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(PostDetail {
                post,
                tag_groups: group_tags(tags),
                blocked_by,
            })
        })
        .collect()
}

fn group_tags(tags: Vec<Tag>) -> Vec<TagGroup> {
    let kinds = TagKind::DISPLAY_ORDER
        .iter()
        .copied()
        .map(Some)
        .chain([None]);
    kinds
        .map(|kind| TagGroup {
            label: TagKind::label(kind),
            tags: tags
                .iter()
                .filter(|tag| tag.kind == kind)
                .cloned()
                .collect(),
        })
        .filter(|group| !group.tags.is_empty())
        .collect()
}

// posts stored from partial listings need fetching again by id before their details are shown
pub fn is_partial(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn
        .query_row("SELECT partial FROM posts WHERE id = ?1", [id], |row| row.get(0))
        .optional()?
        .unwrap_or(false))
}

pub struct MediaUrls {
    pub md5: String,
    pub preview_url: String,
    pub sample_url: String,
    pub file_url: String,
    pub width: i64,
    pub partial: bool,
    pub media_resolved: bool,
}

impl MediaUrls {
    pub fn display_url(&self) -> &str {
        [&self.sample_url, &self.file_url]
            .into_iter()
            .find(|url| is_still(url))
            .unwrap_or(&self.preview_url)
    }
}

pub fn media_urls(conn: &Connection, id: i64) -> Result<Option<MediaUrls>> {
    Ok(conn
        .query_row(
            "SELECT md5, preview_url, sample_url, file_url, width, partial, media_resolved FROM posts WHERE id = ?1",
            [id],
            |row| {
                Ok(MediaUrls {
                    md5: row.get(0)?,
                    preview_url: row.get(1)?,
                    sample_url: row.get(2)?,
                    file_url: row.get(3)?,
                    width: row.get(4)?,
                    partial: row.get(5)?,
                    media_resolved: row.get(6)?,
                })
            },
        )
        .optional()?)
}

pub fn save_resolved_media(
    conn: &Connection,
    id: i64,
    sample_url: &str,
    file_url: &str,
    resolved: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE posts SET sample_url = ?2, file_url = ?3, media_resolved = ?4 WHERE id = ?1",
        params![id, sample_url, file_url, resolved],
    )?;
    Ok(())
}

pub struct Stored {
    pub ids: Vec<i64>,
    pub new_tags: bool,
    pub retagged: bool,
}

pub fn store_posts(conn: &mut Connection, raws: &[RawPost], partial: bool) -> Result<Stored> {
    let started = timing::now();
    let tx = conn.transaction()?;
    let stored = store_posts_in(&tx, raws, partial)?;
    let committing = timing::now();
    tx.commit()?;
    let note = format!("{} posts", raws.len());
    timing::record("store.commit", committing.elapsed(), note.clone());
    timing::record("store", started.elapsed(), note);
    Ok(stored)
}

fn store_posts_in(tx: &Transaction, raws: &[RawPost], partial: bool) -> Result<Stored> {
    let mut stored = Stored {
        ids: Vec::with_capacity(raws.len()),
        new_tags: false,
        retagged: false,
    };
    let mut changed = Vec::new();
    let ids = serde_json::to_string(&raws.iter().map(|raw| raw.id).collect::<Vec<_>>())?;
    let rows_started = timing::now();
    {
        let known: HashSet<i64> = tx
            .prepare_cached("SELECT id FROM posts WHERE id IN (SELECT value FROM json_each(?1))")?
            .query_map([&ids], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut linked: HashMap<i64, HashSet<String>> = HashMap::new();
        if !known.is_empty() {
            let mut statement = tx.prepare_cached(
                "SELECT pt.post_id, t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
                 WHERE pt.post_id IN (SELECT value FROM json_each(?1))",
            )?;
            for row in statement.query_map([&ids], |row| Ok((row.get(0)?, row.get(1)?)))? {
                let (post_id, name): (i64, String) = row?;
                linked.entry(post_id).or_default().insert(name);
            }
        }
        let mut upsert = tx.prepare_cached(
            "INSERT INTO posts (id, seq, md5, rating, score, width, height, preview_url, preview_width,
                                preview_height, sample_url, file_url, posted_at, partial, parent_id, has_children)
             VALUES (?1, (SELECT IFNULL(MAX(seq), 0) + 1 FROM posts), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT (id) DO UPDATE SET md5 = excluded.md5, rating = excluded.rating, score = excluded.score,
                width = excluded.width, height = excluded.height,
                preview_url = excluded.preview_url, preview_width = excluded.preview_width,
                preview_height = excluded.preview_height, sample_url = excluded.sample_url, file_url = excluded.file_url,
                posted_at = COALESCE(excluded.posted_at, posted_at), partial = 0, media_resolved = 0,
                parent_id = CASE WHEN excluded.has_children IS NULL THEN parent_id ELSE excluded.parent_id END,
                has_children = COALESCE(excluded.has_children, has_children)",
        )?;
        for raw in raws {
            let is_known = known.contains(&raw.id);
            stored.ids.push(raw.id);
            if partial && is_known {
                continue;
            }
            upsert.execute(params![
                raw.id,
                raw.md5,
                raw.rating.name(),
                raw.score,
                raw.width,
                raw.height,
                raw.preview_url,
                raw.preview_width,
                raw.preview_height,
                raw.sample_url,
                raw.file_url,
                raw.posted_at,
                partial,
                raw.lineage.and_then(|lineage| lineage.parent),
                raw.lineage.map(|lineage| lineage.has_children),
            ])?;
            let names: HashSet<String> = raw
                .tags
                .iter()
                .filter(|name| name.len() <= 255)
                .cloned()
                .collect();
            let unchanged = is_known
                && linked
                    .get(&raw.id)
                    .map_or(names.is_empty(), |current| current == &names);
            if !unchanged {
                stored.retagged |= is_known;
                changed.push((raw.id, names));
            }
        }
    }
    timing::record("store.rows", rows_started.elapsed(), format!("{} posts", raws.len()));
    let tags_started = timing::now();
    stored.new_tags = link_tags(tx, &changed)?;
    timing::record("store.tags", tags_started.elapsed(), format!("{} changed", changed.len()));
    Ok(stored)
}

// post_count follows the links of the given posts: taken off before they're replaced, added back after.
// counting by post keeps the tag side of post_tags unindexed, which made every batch rewrite most of that index
const ADJUST_POST_COUNTS: &str = "UPDATE tags SET post_count = post_count + ?2 * linked.posts
    FROM (SELECT tag_id, COUNT(*) AS posts FROM post_tags
          WHERE post_id IN (SELECT value FROM json_each(?1)) GROUP BY tag_id) AS linked
    WHERE tags.id = linked.tag_id";

// rewrites the tags of every changed post in a handful of statements rather than one per tag
fn link_tags(tx: &Transaction, posts: &[(i64, HashSet<String>)]) -> Result<bool> {
    if posts.is_empty() {
        return Ok(false);
    }
    let ids = serde_json::to_string(&posts.iter().map(|(post_id, _)| *post_id).collect::<Vec<_>>())?;
    tx.execute(ADJUST_POST_COUNTS, params![&ids, -1])?;
    tx.execute(
        "DELETE FROM post_tags WHERE post_id IN (SELECT value FROM json_each(?1))",
        [&ids],
    )?;

    let mut names: Vec<&str> = posts
        .iter()
        .flat_map(|(_, names)| names.iter().map(String::as_str))
        .collect();
    names.sort_unstable();
    names.dedup();
    let names = serde_json::to_string(&names)?;
    let new_tags = tx.execute(
        "INSERT OR IGNORE INTO tags (name) SELECT value FROM json_each(?1)",
        [&names],
    )? > 0;

    let pairs: Vec<(i64, &str)> = posts
        .iter()
        .flat_map(|(post_id, names)| names.iter().map(move |name| (*post_id, name.as_str())))
        .collect();
    tx.execute(
        "INSERT OR IGNORE INTO post_tags (post_id, tag_id)
         SELECT p.value ->> 0, t.id FROM json_each(?1) p JOIN tags t ON t.name = (p.value ->> 1)",
        [serde_json::to_string(&pairs)?],
    )?;

    tx.execute(ADJUST_POST_COUNTS, params![&ids, 1])?;
    Ok(new_tags)
}

pub struct StoredPage {
    pub ids: Vec<i64>,
    pub total: i64,
    pub fetched_at: i64,
}

fn load_page(conn: &Connection, query: &str, page: i64) -> Result<Option<StoredPage>> {
    let row: Option<(String, i64, i64)> = conn
        .query_row(
            "SELECT post_ids, total, fetched_at FROM search_pages WHERE query = ?1 AND page = ?2",
            params![query, page],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    row.map(|(ids, total, fetched_at)| {
        Ok(StoredPage {
            ids: serde_json::from_str(&ids)?,
            total,
            fetched_at,
        })
    })
    .transpose()
}

pub fn is_stored(conn: &Connection, query: &str, page: i64, max_age: i64) -> Result<bool> {
    let fetched_at: Option<i64> = conn
        .query_row(
            "SELECT fetched_at FROM search_pages WHERE query = ?1 AND page = ?2",
            params![normalize_query(query), page],
            |row| row.get(0),
        )
        .optional()?;
    Ok(fetched_at.is_some_and(|fetched_at| now_ms() - fetched_at < max_age))
}

pub fn stored_total(conn: &Connection, query: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT total FROM search_pages WHERE query = ?1 AND page = 0",
            [normalize_query(query)],
            |row| row.get(0),
        )
        .optional()?)
}

pub async fn search_page(
    core: &Arc<Core>,
    query: &str,
    page: i64,
    max_age: i64,
) -> Result<StoredPage> {
    let query = normalize_query(query);
    let stored = load_page(&core.db.conn(), &query, page)?;
    if let Some(stored) = stored
        .as_ref()
        .filter(|stored| now_ms() - stored.fetched_at < max_age)
    {
        return Ok(StoredPage {
            ids: stored.ids.clone(),
            total: stored.total,
            fetched_at: stored.fetched_at,
        });
    }
    let known_total = stored_total(&core.db.conn(), &query)?;
    let (raws, total) = match core.booru.search(&query, page, known_total).await {
        Ok(found) => found,
        Err(error) => return stored.ok_or(error),
    };
    let fetched_at = now_ms();
    let saved = {
        let mut conn = core.db.conn();
        let tx = conn.transaction()?;
        let saved = store_posts_in(&tx, &raws, false)?;
        tx.execute(
            "INSERT OR REPLACE INTO search_pages (query, page, total, post_ids, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![query, page, total, serde_json::to_string(&saved.ids)?, fetched_at],
        )?;
        let committing = timing::now();
        tx.commit()?;
        timing::record("store.commit", committing.elapsed(), format!("{} posts", raws.len()));
        saved
    };
    after_store(core, &saved);
    Ok(StoredPage {
        ids: saved.ids,
        total,
        fetched_at,
    })
}

pub async fn fetch_page(
    core: &Arc<Core>,
    query: &str,
    page: i64,
    max_age: i64,
) -> Result<Option<StoredPage>> {
    if is_stored(&core.db.conn(), query, page, max_age)? {
        return Ok(None);
    }
    search_page(core, query, page, max_age).await.map(Some)
}

pub fn after_store(core: &Arc<Core>, stored: &Stored) {
    if stored.retagged {
        core.corpus.mark_stale();
    }
    if stored.new_tags {
        fill_tag_details(core);
    }
}

pub fn fill_tag_details(core: &Core) {
    core.queue.push(Job::TagDetails, Priority::TagDetails);
}

pub async fn fill_tag_details_batch(core: &Core) -> Result<()> {
    let names: Vec<String> = {
        let conn = core.db.conn();
        let mut statement = conn.prepare_cached(
            "SELECT name FROM tags WHERE details_fetched = 0 ORDER BY id LIMIT ?1",
        )?;
        statement
            .query_map([TAG_BATCH], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    if !names.is_empty() {
        let details = core.booru.tag_details(&names).await?;
        {
            let mut conn = core.db.conn();
            let tx = conn.transaction()?;
            {
                let mut update = tx.prepare_cached(
                    "UPDATE tags SET type = ?2, site_count = ?3, details_fetched = 1 WHERE name = ?1",
                )?;
                for name in &names {
                    let (kind, count) = details.get(name).map_or((None, None), |detail| {
                        (Some(detail.kind), Some(detail.count))
                    });
                    update.execute(params![name, kind, count])?;
                }
            }
            tx.commit()?;
        }
    }
    if names.len() == TAG_BATCH {
        fill_tag_details(core);
    } else {
        core.corpus.mark_stale();
    }
    Ok(())
}

pub fn suggest(conn: &Connection, typed: &str) -> Result<Vec<Tag>> {
    let words: Vec<String> = typed
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|word| !word.is_empty())
        .map(|word| word.replace('\\', "\\\\").replace('%', "\\%"))
        .collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let trailing_separator = typed.ends_with(|c: char| c.is_whitespace() || c == '_' || c == '-');
    let exact = words.join("_") + if trailing_separator { "_" } else { "" };
    let mut statement = conn.prepare_cached(
        "SELECT name, type, site_count FROM tags
         WHERE name LIKE ?1 || '%' ESCAPE '\\' AND post_count > 0
         ORDER BY name LIKE ?1 ESCAPE '\\' DESC, site_count IS NULL, site_count DESC, post_count DESC, name
         LIMIT 10",
    )?;
    Ok(statement
        .query_map([exact], |row| Tag::from_row(row, 0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn tags_named(conn: &Connection, names: &[String]) -> Result<Vec<Tag>> {
    let mut statement = conn.prepare_cached(
        "SELECT n.value, t.type, t.site_count FROM json_each(?1) n LEFT JOIN tags t ON t.name = n.value",
    )?;
    Ok(statement
        .query_map([serde_json::to_string(names)?], |row| Tag::from_row(row, 0))?
        .collect::<rusqlite::Result<_>>()?)
}
