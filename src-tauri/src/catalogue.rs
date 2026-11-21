use crate::booru::{RawPost, Site};
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

// each site's id for the post, when that site has it
#[derive(Serialize, Clone, Copy, Default)]
pub struct SiteIds {
    pub gelbooru: Option<i64>,
    pub danbooru: Option<i64>,
}

impl SiteIds {
    pub fn get(self, site: Site) -> Option<i64> {
        match site {
            Site::Gelbooru => self.gelbooru,
            Site::Danbooru => self.danbooru,
        }
    }
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
    pub sites: SiteIds,
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
    l.post_id IS NOT NULL, d.post_id IS NOT NULL,
    (SELECT site_id FROM site_posts WHERE post_id = p.id AND site = 'gelbooru'),
    (SELECT site_id FROM site_posts WHERE post_id = p.id AND site = 'danbooru')
    FROM shown_posts p LEFT JOIN likes l ON l.post_id = p.id LEFT JOIN dislikes d ON d.post_id = p.id";

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
        sites: SiteIds {
            gelbooru: row.get(11)?,
            danbooru: row.get(12)?,
        },
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
        "SELECT t.name, t.type, t.site_count FROM tags t
         WHERE t.id IN (SELECT tag_id FROM post_tags WHERE post_id = ?1) ORDER BY t.name",
    )?;
    let mut blocked_by = conn.prepare_cached(
        "SELECT t.name FROM tags t JOIN blacklist b ON b.name = t.name
         WHERE t.id IN (SELECT tag_id FROM post_tags WHERE post_id = ?1) ORDER BY t.name",
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

pub fn site_ids(conn: &Connection, id: i64) -> Result<SiteIds> {
    let mut ids = SiteIds::default();
    let mut statement = conn.prepare_cached("SELECT site, site_id FROM site_posts WHERE post_id = ?1")?;
    for row in statement.query_map([id], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))? {
        let (site, site_id) = row?;
        match Site::from_key(&site) {
            Some(Site::Gelbooru) => ids.gelbooru = Some(site_id),
            Some(Site::Danbooru) => ids.danbooru = Some(site_id),
            None => {}
        }
    }
    Ok(ids)
}

pub fn post_with_site_id(conn: &Connection, site: Site, site_id: i64) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT post_id FROM site_posts WHERE site = ?1 AND site_id = ?2",
            params![site.key(), site_id],
            |row| row.get(0),
        )
        .optional()?)
}

// site rows stored from partial listings, which need fetching again by id before their details are shown
pub fn partial_rows(conn: &Connection, id: i64) -> Result<Vec<(Site, i64)>> {
    let mut statement =
        conn.prepare_cached("SELECT site, site_id FROM site_posts WHERE post_id = ?1 AND partial = 1")?;
    let rows = statement
        .query_map([id], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(String, i64)>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(site, site_id)| Some((Site::from_key(&site)?, site_id)))
        .collect())
}

pub struct MediaUrls {
    pub site: Site,
    pub site_id: i64,
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

// one entry per site that has the post, Danbooru's first
pub fn media_urls(conn: &Connection, id: i64) -> Result<Vec<MediaUrls>> {
    let mut statement = conn.prepare_cached(
        "SELECT sp.site, sp.site_id, IFNULL(p.md5, ''), sp.preview_url, sp.sample_url, sp.file_url, p.width,
                sp.partial, sp.media_resolved
         FROM site_posts sp JOIN posts p ON p.id = sp.post_id WHERE sp.post_id = ?1 ORDER BY sp.site",
    )?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                MediaUrls {
                    site: Site::Danbooru,
                    site_id: row.get(1)?,
                    md5: row.get(2)?,
                    preview_url: row.get(3)?,
                    sample_url: row.get(4)?,
                    file_url: row.get(5)?,
                    width: row.get(6)?,
                    partial: row.get(7)?,
                    media_resolved: row.get(8)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(site, urls)| {
            Some(MediaUrls {
                site: Site::from_key(&site)?,
                ..urls
            })
        })
        .collect())
}

pub fn save_resolved_media(conn: &Connection, urls: &MediaUrls) -> Result<()> {
    conn.execute(
        "UPDATE site_posts SET sample_url = ?3, file_url = ?4, media_resolved = ?5 WHERE site = ?1 AND site_id = ?2",
        params![
            urls.site.key(),
            urls.site_id,
            urls.sample_url,
            urls.file_url,
            urls.media_resolved
        ],
    )?;
    Ok(())
}

pub struct Stored {
    pub ids: Vec<i64>,
    pub new_tags: bool,
    pub retagged: bool,
}

pub fn store_posts(conn: &mut Connection, site: Site, raws: &[RawPost], partial: bool) -> Result<Stored> {
    let started = timing::now();
    let tx = conn.transaction()?;
    let stored = store_posts_in(&tx, site, raws, partial)?;
    let committing = timing::now();
    tx.commit()?;
    let note = format!("{} posts", raws.len());
    timing::record("store.commit", committing.elapsed(), note.clone());
    timing::record("store", started.elapsed(), note);
    Ok(stored)
}

// sites send md5s in lowercase hex, and a post without one can't be matched with the other site's copy
pub fn normalized_md5(md5: &str) -> Option<String> {
    let md5 = md5.trim().to_lowercase();
    (!md5.is_empty()).then_some(md5)
}

const UPSERT_SITE_POST: &str = "INSERT INTO site_posts (site, site_id, post_id, rating, score, preview_url, preview_width,
        preview_height, sample_url, file_url, posted_at, partial, parent_id, has_children)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
    ON CONFLICT (site, site_id) DO UPDATE SET rating = excluded.rating, score = excluded.score,
        preview_url = excluded.preview_url, preview_width = excluded.preview_width,
        preview_height = excluded.preview_height, sample_url = excluded.sample_url, file_url = excluded.file_url,
        posted_at = COALESCE(excluded.posted_at, posted_at), partial = 0, media_resolved = 0,
        parent_id = CASE WHEN excluded.has_children IS NULL THEN parent_id ELSE excluded.parent_id END,
        has_children = COALESCE(excluded.has_children, has_children)";

fn store_posts_in(tx: &Transaction, site: Site, raws: &[RawPost], partial: bool) -> Result<Stored> {
    let mut stored = Stored {
        ids: Vec::with_capacity(raws.len()),
        new_tags: false,
        retagged: false,
    };
    let mut changed = Vec::new();
    let site_ids = serde_json::to_string(&raws.iter().map(|raw| raw.id).collect::<Vec<_>>())?;
    let rows_started = timing::now();
    {
        // the local ids of posts this site already sent
        let known: HashMap<i64, i64> = tx
            .prepare_cached(
                "SELECT site_id, post_id FROM site_posts WHERE site = ?1 AND site_id IN (SELECT value FROM json_each(?2))",
            )?
            .query_map(params![site.key(), &site_ids], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut by_md5 = tx.prepare_cached("SELECT id FROM posts WHERE md5 = ?1")?;
        let mut new_post = tx.prepare_cached(
            "INSERT INTO posts (md5, seq, width, height, posted_at)
             VALUES (?1, (SELECT IFNULL(MAX(seq), 0) + 1 FROM posts), ?2, ?3, ?4) RETURNING id",
        )?;
        let mut earlier = tx.prepare_cached("UPDATE posts SET posted_at = MIN(COALESCE(posted_at, ?2), ?2) WHERE id = ?1")?;
        let mut site_row_of = tx.prepare_cached("SELECT site_id FROM site_posts WHERE post_id = ?1 AND site = ?2")?;
        let mut upsert = tx.prepare_cached(UPSERT_SITE_POST)?;
        let mut entries = Vec::with_capacity(raws.len());
        for raw in raws {
            let md5 = normalized_md5(&raw.md5);
            let known_id = known.get(&raw.id).copied();
            let matched = match (known_id, &md5) {
                (Some(id), _) => Some(id),
                (None, Some(md5)) => by_md5.query_row([md5], |row| row.get(0)).optional()?,
                (None, None) => None,
            };
            let post_id = match matched {
                Some(id) => id,
                None => new_post.query_row(params![md5, raw.width, raw.height, raw.posted_at], |row| row.get(0))?,
            };
            stored.ids.push(post_id);
            if partial && known_id.is_some() {
                continue;
            }
            if known_id.is_none() && matched.is_some() {
                // a second id on this site for the same file, like a reupload, keeps the first one's row
                let other: Option<i64> = site_row_of.query_row(params![post_id, site.key()], |row| row.get(0)).optional()?;
                if other.is_some() {
                    continue;
                }
            }
            if let Some(posted_at) = raw.posted_at {
                earlier.execute(params![post_id, posted_at])?;
            }
            upsert.execute(params![
                site.key(),
                raw.id,
                post_id,
                raw.rating.name(),
                raw.score,
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
            entries.push((post_id, raw, matched.is_some()));
        }
        let post_ids = serde_json::to_string(&entries.iter().map(|(id, _, _)| *id).collect::<Vec<_>>())?;
        let mut linked: HashMap<i64, HashSet<String>> = HashMap::new();
        let mut statement = tx.prepare_cached(
            "SELECT pt.post_id, t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
             WHERE pt.site = ?1 AND pt.post_id IN (SELECT value FROM json_each(?2))",
        )?;
        for row in statement.query_map(params![site.key(), &post_ids], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (post_id, name): (i64, String) = row?;
            linked.entry(post_id).or_default().insert(name);
        }
        for (post_id, raw, post_was_known) in entries {
            let names: HashSet<String> = raw
                .tags
                .iter()
                .filter(|name| name.len() <= 255)
                .cloned()
                .collect();
            let unchanged = linked
                .get(&post_id)
                .map_or(names.is_empty(), |current| current == &names);
            if !unchanged {
                stored.retagged |= post_was_known;
                changed.push((post_id, names));
            }
        }
    }
    timing::record("store.rows", rows_started.elapsed(), format!("{} posts", raws.len()));
    let tags_started = timing::now();
    stored.new_tags = link_tags(tx, site, &changed)?;
    timing::record("store.tags", tags_started.elapsed(), format!("{} changed", changed.len()));
    Ok(stored)
}

// post_count follows the links of the given posts: taken off before they're replaced, added back after. a tag both
// sites gave a post counts once. counting by post keeps the tag side of post_tags unindexed, which made every batch
// rewrite most of that index
const ADJUST_POST_COUNTS: &str = "UPDATE tags SET post_count = post_count + ?2 * linked.posts
    FROM (SELECT tag_id, COUNT(DISTINCT post_id) AS posts FROM post_tags
          WHERE post_id IN (SELECT value FROM json_each(?1)) GROUP BY tag_id) AS linked
    WHERE tags.id = linked.tag_id";

// rewrites this site's tags of every changed post in a handful of statements rather than one per tag
fn link_tags(tx: &Transaction, site: Site, posts: &[(i64, HashSet<String>)]) -> Result<bool> {
    if posts.is_empty() {
        return Ok(false);
    }
    let ids = serde_json::to_string(&posts.iter().map(|(post_id, _)| *post_id).collect::<Vec<_>>())?;
    tx.execute(ADJUST_POST_COUNTS, params![&ids, -1])?;
    tx.execute(
        "DELETE FROM post_tags WHERE site = ?2 AND post_id IN (SELECT value FROM json_each(?1))",
        params![&ids, site.key()],
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
        "INSERT OR IGNORE INTO post_tags (post_id, site, tag_id)
         SELECT p.value ->> 0, ?2, t.id FROM json_each(?1) p JOIN tags t ON t.name = (p.value ->> 1)",
        params![serde_json::to_string(&pairs)?, site.key()],
    )?;

    tx.execute(ADJUST_POST_COUNTS, params![&ids, 1])?;
    Ok(new_tags)
}

pub struct StoredPage {
    pub ids: Vec<i64>,
    pub total: i64,
    pub fetched_at: i64,
}

fn load_page(conn: &Connection, site: Site, query: &str, page: i64) -> Result<Option<StoredPage>> {
    let row: Option<(String, i64, i64)> = conn
        .query_row(
            "SELECT post_ids, total, fetched_at FROM search_pages WHERE site = ?1 AND query = ?2 AND page = ?3",
            params![site.key(), query, page],
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

pub fn is_stored(conn: &Connection, site: Site, query: &str, page: i64, max_age: i64) -> Result<bool> {
    let fetched_at: Option<i64> = conn
        .query_row(
            "SELECT fetched_at FROM search_pages WHERE site = ?1 AND query = ?2 AND page = ?3",
            params![site.key(), normalize_query(query), page],
            |row| row.get(0),
        )
        .optional()?;
    Ok(fetched_at.is_some_and(|fetched_at| now_ms() - fetched_at < max_age))
}

// a short page stores the exact total, which beats the site's count on the pages before it
pub fn stored_total(conn: &Connection, site: Site, query: &str) -> Result<Option<i64>> {
    Ok(conn.query_row(
        "SELECT MIN(total) FROM search_pages WHERE site = ?1 AND query = ?2",
        params![site.key(), normalize_query(query)],
        |row| row.get(0),
    )?)
}

pub async fn search_page(
    core: &Arc<Core>,
    site: Site,
    query: &str,
    page: i64,
    max_age: i64,
) -> Result<StoredPage> {
    let query = normalize_query(query);
    let stored = load_page(&core.db.conn(), site, &query, page)?;
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
    let known_total = stored_total(&core.db.conn(), site, &query)?;
    let (raws, total) = match core.booru(site).search(&query, page, known_total).await {
        Ok(found) => found,
        Err(error) => return stored.ok_or(error),
    };
    let fetched_at = now_ms();
    let saved = {
        let mut conn = core.db.conn();
        let tx = conn.transaction()?;
        let saved = store_posts_in(&tx, site, &raws, false)?;
        tx.execute(
            "INSERT OR REPLACE INTO search_pages (site, query, page, total, post_ids, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![site.key(), query, page, total, serde_json::to_string(&saved.ids)?, fetched_at],
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
    site: Site,
    query: &str,
    page: i64,
    max_age: i64,
) -> Result<Option<StoredPage>> {
    if is_stored(&core.db.conn(), site, query, page, max_age)? {
        return Ok(None);
    }
    search_page(core, site, query, page, max_age).await.map(Some)
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

// Danbooru's tag types are curated, so every tag asks Danbooru first and Gelbooru only for the ones it doesn't know
pub async fn fill_tag_details_batch(core: &Core) -> Result<()> {
    let mut asked = fill_from(core, Site::Danbooru).await?;
    if asked < TAG_BATCH && core.can_search(Site::Gelbooru) {
        asked = fill_from(core, Site::Gelbooru).await?;
    }
    if asked == TAG_BATCH {
        fill_tag_details(core);
    } else {
        core.corpus.mark_stale();
    }
    Ok(())
}

async fn fill_from(core: &Core, site: Site) -> Result<usize> {
    let (unchecked, checked) = match site {
        Site::Danbooru => ("danbooru_checked = 0", "danbooru_checked"),
        Site::Gelbooru => ("gelbooru_checked = 0 AND count_site IS NULL", "gelbooru_checked"),
    };
    let names: Vec<String> = {
        let conn = core.db.conn();
        let mut statement = conn.prepare_cached(&format!(
            "SELECT name FROM tags WHERE {unchecked} ORDER BY id LIMIT ?1"
        ))?;
        statement
            .query_map([TAG_BATCH], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    if names.is_empty() {
        return Ok(0);
    }
    let details = core.booru(site).tag_details(&names).await?;
    let mut conn = core.db.conn();
    let tx = conn.transaction()?;
    {
        let mut found = tx.prepare_cached(&format!(
            "UPDATE tags SET type = ?2, site_count = ?3, count_site = ?4, {checked} = 1 WHERE name = ?1"
        ))?;
        let mut missing = tx.prepare_cached(&format!("UPDATE tags SET {checked} = 1 WHERE name = ?1"))?;
        for name in &names {
            match details.get(name) {
                Some(detail) => found.execute(params![name, detail.kind, detail.count, site.key()])?,
                None => missing.execute([name])?,
            };
        }
    }
    tx.commit()?;
    Ok(names.len())
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::search::Rating;

    pub(crate) fn database() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn
    }

    pub(crate) fn raw(id: i64, md5: &str, tags: &[&str]) -> RawPost {
        RawPost {
            id,
            md5: md5.to_owned(),
            rating: Rating::General,
            score: 1,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            width: 10,
            height: 10,
            preview_url: "preview".to_owned(),
            preview_width: 1,
            preview_height: 1,
            sample_url: "sample".to_owned(),
            file_url: "file.png".to_owned(),
            posted_at: None,
            lineage: None,
        }
    }

    fn post_count(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT post_count FROM tags WHERE name = ?1", [name], |row| row.get(0))
            .unwrap()
    }

    fn tag_names(conn: &Connection, id: i64) -> Vec<String> {
        post_details(conn, &[id]).unwrap()[0]
            .tag_groups
            .iter()
            .flat_map(|group| group.tags.iter().map(|tag| tag.name.clone()))
            .collect()
    }

    #[test]
    fn a_short_page_overrides_the_sites_count() {
        let conn = database();
        assert_eq!(stored_total(&conn, Site::Gelbooru, "cat").unwrap(), None);
        let store = |page: i64, total: i64| {
            conn.execute(
                "INSERT INTO search_pages (site, query, page, total, post_ids, fetched_at) VALUES ('gelbooru', 'cat', ?1, ?2, '[]', 0)",
                params![page, total],
            )
            .unwrap();
        };
        store(0, 500);
        assert_eq!(stored_total(&conn, Site::Gelbooru, "cat").unwrap(), Some(500));
        store(1, 152);
        assert_eq!(stored_total(&conn, Site::Gelbooru, "cat").unwrap(), Some(152));
        assert_eq!(stored_total(&conn, Site::Danbooru, "cat").unwrap(), None);
    }

    #[test]
    fn the_same_file_from_both_sites_is_one_post() {
        let mut conn = database();
        let md5 = "ABCDEF0123456789abcdef0123456789";
        let from_danbooru = store_posts(&mut conn, Site::Danbooru, &[raw(5, md5, &["cat", "sky"])], false).unwrap();
        let from_gelbooru = store_posts(&mut conn, Site::Gelbooru, &[raw(900, md5, &["cat", "cloud"])], false).unwrap();
        assert_eq!(from_danbooru.ids, from_gelbooru.ids);
        assert!(from_gelbooru.retagged);
        let id = from_danbooru.ids[0];
        let ids = site_ids(&conn, id).unwrap();
        assert_eq!((ids.danbooru, ids.gelbooru), (Some(5), Some(900)));
        assert_eq!(tag_names(&conn, id), ["cat", "cloud", "sky"]);
        assert_eq!(post_count(&conn, "cat"), 1);
    }

    #[test]
    fn one_sites_retagging_leaves_the_others_tags() {
        let mut conn = database();
        let md5 = "abcdef0123456789abcdef0123456789";
        store_posts(&mut conn, Site::Danbooru, &[raw(5, md5, &["cat", "sky"])], false).unwrap();
        let id = store_posts(&mut conn, Site::Gelbooru, &[raw(900, md5, &["cat"])], false).unwrap().ids[0];
        store_posts(&mut conn, Site::Gelbooru, &[raw(900, md5, &["dog"])], false).unwrap();
        assert_eq!(tag_names(&conn, id), ["cat", "dog", "sky"]);
        assert_eq!(post_count(&conn, "cat"), 1);
        assert_eq!(post_count(&conn, "dog"), 1);
    }

    #[test]
    fn posts_without_an_md5_never_merge() {
        let mut conn = database();
        let first = store_posts(&mut conn, Site::Danbooru, &[raw(1, "", &[])], false).unwrap();
        let second = store_posts(&mut conn, Site::Danbooru, &[raw(2, "", &[])], false).unwrap();
        let again = store_posts(&mut conn, Site::Danbooru, &[raw(1, "", &[])], false).unwrap();
        assert_ne!(first.ids, second.ids);
        assert_eq!(first.ids, again.ids);
    }
}
