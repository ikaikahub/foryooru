use crate::activity;
use crate::catalogue::{self, Post, Tag};
use crate::db::{now_ms, page_count};
use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub const LISTING_PAGE_SIZE: i64 = 100;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostState {
    pub id: i64,
    pub liked: bool,
    pub disliked: bool,
}

pub fn post_state(conn: &Connection, id: i64) -> Result<PostState> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM likes WHERE post_id = ?1), EXISTS (SELECT 1 FROM dislikes WHERE post_id = ?1)",
        [id],
        |row| Ok(PostState { id, liked: row.get(0)?, disliked: row.get(1)? }),
    )?)
}

fn ensure_post(conn: &Connection, id: i64) -> Result<()> {
    let exists: Option<i64> = conn
        .query_row("SELECT 1 FROM posts WHERE id = ?1", [id], |row| row.get(0))
        .optional()?;
    exists.map(|_| ()).ok_or(Error::PostNotFound)
}

pub fn toggle_like(conn: &Connection, id: i64) -> Result<PostState> {
    ensure_post(conn, id)?;
    if conn.execute("DELETE FROM likes WHERE post_id = ?1", [id])? == 0 {
        let now = now_ms();
        conn.execute(
            "INSERT INTO likes (post_id, sort_at, liked_at) VALUES (?1, ?2, ?2)",
            [id, now],
        )?;
        conn.execute("DELETE FROM dislikes WHERE post_id = ?1", [id])?;
        activity::record_reaction(conn, activity::Reaction::Like, id)?;
    } else {
        activity::record_reaction(conn, activity::Reaction::Unlike, id)?;
    }
    post_state(conn, id)
}

pub fn toggle_dislike(conn: &Connection, id: i64) -> Result<PostState> {
    ensure_post(conn, id)?;
    if conn.execute("DELETE FROM dislikes WHERE post_id = ?1", [id])? == 0 {
        conn.execute(
            "INSERT INTO dislikes (post_id, disliked_at) VALUES (?1, ?2)",
            [id, now_ms()],
        )?;
        conn.execute("DELETE FROM likes WHERE post_id = ?1", [id])?;
        activity::record_reaction(conn, activity::Reaction::Dislike, id)?;
    } else {
        activity::record_reaction(conn, activity::Reaction::Undislike, id)?;
    }
    post_state(conn, id)
}

pub use foryooru_core::recommend::Reaction;

fn reactions(conn: &Connection, sql: &str, before: i64) -> Result<Vec<Reaction>> {
    let mut statement = conn.prepare_cached(sql)?;
    Ok(statement
        .query_map([before], |row| Ok(Reaction { id: row.get(0)? }))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn likes_before(conn: &Connection, before: i64) -> Result<Vec<Reaction>> {
    reactions(
        conn,
        "SELECT post_id FROM likes WHERE liked_at IS NULL OR liked_at < ?1 ORDER BY sort_at DESC",
        before,
    )
}

pub fn dislikes_before(conn: &Connection, before: i64) -> Result<Vec<Reaction>> {
    reactions(
        conn,
        "SELECT post_id FROM dislikes WHERE disliked_at < ?1 ORDER BY disliked_at DESC",
        before,
    )
}

pub fn likes(conn: &Connection) -> Result<Vec<Reaction>> {
    likes_before(conn, i64::MAX)
}

pub fn dislikes(conn: &Connection) -> Result<Vec<Reaction>> {
    dislikes_before(conn, i64::MAX)
}

pub fn likes_since(conn: &Connection, since: i64) -> Result<Vec<i64>> {
    let mut statement = conn.prepare_cached("SELECT post_id FROM likes WHERE sort_at >= ?1")?;
    Ok(statement
        .query_map([since], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

#[derive(Serialize)]
pub struct HiddenTag {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub posts: Vec<Post>,
    pub pages: i64,
    pub hidden: Vec<HiddenTag>,
}

const BLACKLISTED: &str = "SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
    JOIN blacklist b ON b.name = t.name WHERE pt.post_id = ";

pub fn without_blacklisted(conn: &Connection, ids: Vec<i64>) -> Result<(Vec<i64>, Vec<HiddenTag>)> {
    let mut statement = conn.prepare_cached(&format!("{BLACKLISTED} ?1 LIMIT 1"))?;
    let mut kept = Vec::with_capacity(ids.len());
    let mut hidden: HashMap<String, i64> = HashMap::new();
    for id in ids {
        match statement
            .query_row([id], |row| row.get::<_, String>(0))
            .optional()?
        {
            Some(tag) => *hidden.entry(tag).or_default() += 1,
            None => kept.push(id),
        }
    }
    let mut hidden: Vec<HiddenTag> = hidden
        .into_iter()
        .map(|(name, count)| HiddenTag { name, count })
        .collect();
    hidden.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    Ok((kept, hidden))
}

pub enum Matching {
    InOrder(HashSet<i64>),
    SortedBy(HashMap<i64, f64>),
}

impl Matching {
    fn contains(&self, id: i64) -> bool {
        match self {
            Matching::InOrder(ids) => ids.contains(&id),
            Matching::SortedBy(keys) => keys.contains_key(&id),
        }
    }
}

fn matching_ids(conn: &Connection, sql: &str, matching: &Matching) -> Result<Vec<i64>> {
    let mut statement = conn.prepare_cached(sql)?;
    let mut ids: Vec<i64> = statement
        .query_map([], |row| row.get(0))?
        .filter(|id| id.as_ref().map_or(true, |id| matching.contains(*id)))
        .collect::<rusqlite::Result<_>>()?;
    if let Matching::SortedBy(keys) = matching {
        ids.sort_by(|a, b| keys[b].total_cmp(&keys[a]));
    }
    Ok(ids)
}

fn listing_page(
    conn: &Connection,
    ids: &[i64],
    page: i64,
    hidden: Vec<HiddenTag>,
) -> Result<Listing> {
    let shown: Vec<i64> = ids
        .iter()
        .skip((page * LISTING_PAGE_SIZE) as usize)
        .take(LISTING_PAGE_SIZE as usize)
        .copied()
        .collect();
    Ok(Listing {
        posts: catalogue::posts(conn, &shown)?,
        pages: page_count(ids.len() as i64, LISTING_PAGE_SIZE),
        hidden,
    })
}

pub fn liked_listing(conn: &Connection, matching: &Matching, page: i64) -> Result<Listing> {
    let ids = matching_ids(
        conn,
        "SELECT post_id FROM likes ORDER BY sort_at DESC",
        matching,
    )?;
    let (kept, hidden) = without_blacklisted(conn, ids)?;
    listing_page(conn, &kept, page, hidden)
}

pub fn disliked_listing(conn: &Connection, matching: &Matching, page: i64) -> Result<Listing> {
    let ids = matching_ids(
        conn,
        "SELECT post_id FROM dislikes ORDER BY disliked_at DESC",
        matching,
    )?;
    listing_page(conn, &ids, page, Vec::new())
}

pub fn clear_likes(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM likes", [])?;
    Ok(())
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagList {
    FavouriteTags,
    Blacklist,
}

impl TagList {
    fn table(self) -> &'static str {
        match self {
            TagList::FavouriteTags => "favourite_tags",
            TagList::Blacklist => "blacklist",
        }
    }
}

pub fn clean_tag_name(name: &str) -> Option<String> {
    let name = name
        .trim()
        .trim_start_matches('-')
        .to_lowercase()
        .replace(' ', "_");
    (!name.is_empty() && name.len() <= 255).then_some(name)
}

pub fn tag_names(conn: &Connection, list: TagList) -> Result<Vec<String>> {
    let mut statement =
        conn.prepare_cached(&format!("SELECT name FROM {} ORDER BY name", list.table()))?;
    Ok(statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn tags(conn: &Connection, list: TagList) -> Result<Vec<Tag>> {
    catalogue::tags_named(conn, &tag_names(conn, list)?)
}

pub fn toggle_tag(conn: &Connection, list: TagList, name: &str) -> Result<()> {
    let Some(name) = clean_tag_name(name) else {
        return Ok(());
    };
    let table = list.table();
    if conn.execute(&format!("DELETE FROM {table} WHERE name = ?1"), [&name])? == 0 {
        conn.execute(
            &format!("INSERT INTO {table} (name, created_at) VALUES (?1, ?2)"),
            params![name, now_ms()],
        )?;
    }
    Ok(())
}

pub fn add_tags(conn: &Connection, list: TagList, names: &str) -> Result<()> {
    let table = list.table();
    for name in names.split_whitespace().filter_map(clean_tag_name) {
        conn.execute(
            &format!("INSERT OR IGNORE INTO {table} (name, created_at) VALUES (?1, ?2)"),
            params![name, now_ms()],
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct SavedSearch {
    pub query: String,
    pub tags: Vec<Tag>,
}

pub fn saved_searches(conn: &Connection) -> Result<Vec<SavedSearch>> {
    let queries: Vec<String> = {
        let mut statement =
            conn.prepare_cached("SELECT query FROM saved_searches ORDER BY created_at DESC")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    queries
        .into_iter()
        .map(|query| {
            let words: Vec<String> = query
                .split_whitespace()
                .map(|word| word.trim_start_matches('-').to_owned())
                .collect();
            let mut tags = catalogue::tags_named(conn, &words)?;
            for (tag, word) in tags.iter_mut().zip(query.split_whitespace()) {
                tag.name = word.to_owned();
            }
            Ok(SavedSearch { query, tags })
        })
        .collect()
}

pub fn toggle_saved_search(conn: &Connection, query: &str) -> Result<()> {
    let mut words: Vec<&str> = Vec::new();
    for word in query.split_whitespace() {
        if !words.contains(&word) {
            words.push(word);
        }
    }
    let query = words.join(" ");
    if query.is_empty() {
        return Ok(());
    }
    if conn.execute("DELETE FROM saved_searches WHERE query = ?1", [&query])? == 0 {
        conn.execute(
            "INSERT INTO saved_searches (query, created_at) VALUES (?1, ?2)",
            params![query, now_ms()],
        )?;
    }
    Ok(())
}
