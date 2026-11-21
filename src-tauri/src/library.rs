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

pub fn toggle_like(conn: &mut Connection, id: i64) -> Result<PostState> {
    let tx = conn.transaction()?;
    ensure_post(&tx, id)?;
    if tx.execute("DELETE FROM likes WHERE post_id = ?1", [id])? == 0 {
        let now = now_ms();
        tx.execute(
            "INSERT INTO likes (post_id, sort_at, liked_at) VALUES (?1, ?2, ?2)",
            [id, now],
        )?;
        tx.execute("DELETE FROM dislikes WHERE post_id = ?1", [id])?;
        activity::record_reaction(&tx, activity::Reaction::Like, id)?;
    } else {
        activity::record_reaction(&tx, activity::Reaction::Unlike, id)?;
    }
    let state = post_state(&tx, id)?;
    tx.commit()?;
    Ok(state)
}

pub fn toggle_dislike(conn: &mut Connection, id: i64) -> Result<PostState> {
    let tx = conn.transaction()?;
    ensure_post(&tx, id)?;
    if tx.execute("DELETE FROM dislikes WHERE post_id = ?1", [id])? == 0 {
        tx.execute(
            "INSERT INTO dislikes (post_id, disliked_at) VALUES (?1, ?2)",
            [id, now_ms()],
        )?;
        tx.execute("DELETE FROM likes WHERE post_id = ?1", [id])?;
        activity::record_reaction(&tx, activity::Reaction::Dislike, id)?;
    } else {
        activity::record_reaction(&tx, activity::Reaction::Undislike, id)?;
    }
    let state = post_state(&tx, id)?;
    tx.commit()?;
    Ok(state)
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

pub fn without_blacklisted(conn: &Connection, ids: Vec<i64>) -> Result<(Vec<i64>, Vec<HiddenTag>)> {
    let mut statement = conn.prepare_cached(
        "SELECT pt.post_id, MIN(t.name) FROM post_tags pt
         JOIN tags t ON t.id = pt.tag_id
         JOIN blacklist b ON b.name = t.name
         WHERE pt.post_id IN (SELECT value FROM json_each(?1))
         GROUP BY pt.post_id",
    )?;
    let mut blocked: HashSet<i64> = HashSet::new();
    let mut hidden: HashMap<String, i64> = HashMap::new();
    let rows = statement.query_map([serde_json::to_string(&ids)?], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, tag) = row?;
        blocked.insert(id);
        *hidden.entry(tag).or_default() += 1;
    }
    let mut hidden: Vec<HiddenTag> = hidden
        .into_iter()
        .map(|(name, count)| HiddenTag { name, count })
        .collect();
    hidden.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    Ok((ids.into_iter().filter(|id| !blocked.contains(id)).collect(), hidden))
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
    FollowedArtists,
}

impl TagList {
    fn table(self) -> &'static str {
        match self {
            TagList::FavouriteTags => "favourite_tags",
            TagList::Blacklist => "blacklist",
            TagList::FollowedArtists => "followed_artists",
        }
    }
}

// newest posts shown in each followed artist's row
const ROW_POSTS: usize = 10;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FollowedOrder {
    LatestPost,
    RecentlyFollowed,
    Name,
}

#[derive(Serialize)]
pub struct ArtistRow {
    pub artist: Tag,
    pub posts: Vec<Post>,
}

// post ids from newest to oldest. finding them goes through the corpus, post_tags has no index by tag
fn newest_first(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>> {
    Ok(conn
        .prepare_cached(
            "SELECT p.id FROM posts p WHERE p.id IN (SELECT value FROM json_each(?1))
             ORDER BY COALESCE(p.posted_at, 0) DESC, p.id DESC",
        )?
        .query_map([serde_json::to_string(ids)?], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

// tagged holds (followed artist, post id) pairs, from Corpus::posts_tagged
pub fn followed_rows(
    conn: &Connection,
    matching: &Matching,
    order: FollowedOrder,
    tagged: &[(String, i64)],
) -> Result<Vec<ArtistRow>> {
    let mut followed: Vec<(String, i64)> = conn
        .prepare_cached("SELECT name, created_at FROM followed_artists")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let rows = newest_by_artist(conn, matching, tagged)?;
    match order {
        FollowedOrder::LatestPost => followed.sort_by_key(|(name, _)| rows.get(name).map_or(usize::MAX, |(first, _)| *first)),
        FollowedOrder::RecentlyFollowed => followed.sort_by_key(|(_, followed_at)| std::cmp::Reverse(*followed_at)),
        FollowedOrder::Name => followed.sort_by(|a, b| a.0.cmp(&b.0)),
    }
    rows_in_order(conn, followed.into_iter().map(|(name, _)| name).collect(), rows)
}

// rows for the named artists in the order given, for the artists a search found; tagged as for followed_rows
pub fn artist_rows(conn: &Connection, matching: &Matching, names: Vec<String>, tagged: &[(String, i64)]) -> Result<Vec<ArtistRow>> {
    let rows = newest_by_artist(conn, matching, tagged)?;
    rows_in_order(conn, names, rows)
}

// each artist's newest matching posts that aren't blacklisted, with where their first one falls among everyone's
// posts newest first, which is how recently they posted
fn newest_by_artist(conn: &Connection, matching: &Matching, tagged: &[(String, i64)]) -> Result<HashMap<String, (usize, Vec<i64>)>> {
    let ids: Vec<i64> = tagged.iter().map(|(_, id)| *id).filter(|id| matching.contains(*id)).collect();
    let position: HashMap<i64, usize> = newest_first(conn, &ids)?
        .into_iter()
        .enumerate()
        .map(|(position, id)| (id, position))
        .collect();
    let mut newest_first: Vec<(String, i64)> = tagged
        .iter()
        .filter(|(_, id)| position.contains_key(id))
        .cloned()
        .collect();
    newest_first.sort_by_key(|(_, id)| position[id]);
    let (allowed, _) = without_blacklisted(conn, ids)?;
    let allowed: HashSet<i64> = allowed.into_iter().collect();

    let mut rows: HashMap<String, (usize, Vec<i64>)> = HashMap::new();
    for (position, (name, id)) in newest_first.into_iter().enumerate() {
        if !allowed.contains(&id) {
            continue;
        }
        let (_, posts) = rows.entry(name).or_insert_with(|| (position, Vec::new()));
        if posts.len() < ROW_POSTS {
            posts.push(id);
        }
    }
    Ok(rows)
}

fn rows_in_order(conn: &Connection, names: Vec<String>, mut rows: HashMap<String, (usize, Vec<i64>)>) -> Result<Vec<ArtistRow>> {
    let mut tags: HashMap<String, Tag> = catalogue::tags_named(conn, &names)?
        .into_iter()
        .map(|tag| (tag.name.clone(), tag))
        .collect();
    names
        .into_iter()
        .map(|name| {
            let ids = rows.remove(&name).map(|(_, ids)| ids).unwrap_or_default();
            let mut posts = catalogue::posts(conn, &ids)?;
            posts.sort_by_key(|post| ids.iter().position(|id| *id == post.id));
            let artist = tags.remove(&name).unwrap_or(Tag {
                name,
                kind: Some(catalogue::TagKind::Artist),
                count: None,
            });
            Ok(ArtistRow { artist, posts })
        })
        .collect()
}

pub fn followed_listing(conn: &Connection, matching: &Matching, page: i64, tagged: &[(String, i64)]) -> Result<Listing> {
    let ids: HashSet<i64> = tagged.iter().map(|(_, id)| *id).filter(|id| matching.contains(*id)).collect();
    let mut ids = newest_first(conn, &ids.into_iter().collect::<Vec<_>>())?;
    if let Matching::SortedBy(keys) = matching {
        ids.sort_by(|a, b| keys[b].total_cmp(&keys[a]));
    }
    let (kept, hidden) = without_blacklisted(conn, ids)?;
    listing_page(conn, &kept, page, hidden)
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
    let tx = conn.unchecked_transaction()?;
    if tx.execute(&format!("DELETE FROM {table} WHERE name = ?1"), [&name])? == 0 {
        tx.execute(
            &format!("INSERT INTO {table} (name, created_at) VALUES (?1, ?2)"),
            params![name, now_ms()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn add_tags(conn: &Connection, list: TagList, names: &str) -> Result<()> {
    let table = list.table();
    let now = now_ms();
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert =
            tx.prepare_cached(&format!("INSERT OR IGNORE INTO {table} (name, created_at) VALUES (?1, ?2)"))?;
        for name in names.split_whitespace().filter_map(clean_tag_name) {
            insert.execute(params![name, now])?;
        }
    }
    tx.commit()?;
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
    let tx = conn.unchecked_transaction()?;
    if tx.execute("DELETE FROM saved_searches WHERE query = ?1", [&query])? == 0 {
        tx.execute(
            "INSERT INTO saved_searches (query, created_at) VALUES (?1, ?2)",
            params![query, now_ms()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booru::RawPost;
    use crate::search::Rating;

    fn store(conn: &mut Connection, id: i64, posted_at: i64, artist: &str) {
        let raw = RawPost {
            id,
            md5: format!("{id:032x}"),
            rating: Rating::General,
            score: 1,
            tags: vec![artist.to_owned(), "sky".to_owned()],
            width: 10,
            height: 10,
            preview_url: String::new(),
            preview_width: 1,
            preview_height: 1,
            sample_url: String::new(),
            file_url: "file.png".to_owned(),
            posted_at: Some(posted_at),
            lineage: None,
        };
        catalogue::store_posts(conn, &[raw], false).unwrap();
    }

    fn follow(conn: &Connection, name: &str, at: i64) {
        conn.execute("INSERT INTO followed_artists (name, created_at) VALUES (?1, ?2)", params![name, at])
            .unwrap();
    }

    #[test]
    fn followed_rows_hold_each_artists_newest_posts_in_the_chosen_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        store(&mut conn, 1, 100, "early_bird");
        store(&mut conn, 2, 300, "early_bird");
        store(&mut conn, 3, 500, "night_owl");
        store(&mut conn, 4, 900, "not_followed");
        follow(&conn, "night_owl", 10);
        follow(&conn, "early_bird", 20);
        follow(&conn, "quiet_one", 30);
        let everything = Matching::InOrder((1..=4).collect());
        let tagged = [("early_bird".to_owned(), 1), ("early_bird".to_owned(), 2), ("night_owl".to_owned(), 3)];

        let rows = |order| -> Vec<(String, Vec<i64>)> {
            followed_rows(&conn, &everything, order, &tagged)
                .unwrap()
                .into_iter()
                .map(|row| (row.artist.name, row.posts.iter().map(|post| post.id).collect()))
                .collect()
        };
        let by_latest = rows(FollowedOrder::LatestPost);
        assert_eq!(
            by_latest,
            [
                ("night_owl".to_owned(), vec![3]),
                ("early_bird".to_owned(), vec![2, 1]),
                ("quiet_one".to_owned(), vec![]),
            ]
        );
        let names = |rows: Vec<(String, Vec<i64>)>| rows.into_iter().map(|(name, _)| name).collect::<Vec<_>>();
        assert_eq!(names(rows(FollowedOrder::RecentlyFollowed)), ["quiet_one", "early_bird", "night_owl"]);
        assert_eq!(names(rows(FollowedOrder::Name)), ["early_bird", "night_owl", "quiet_one"]);

        let listing = followed_listing(&conn, &everything, 0, &tagged).unwrap();
        assert_eq!(listing.posts.iter().map(|post| post.id).collect::<Vec<_>>(), [3, 2, 1]);

        // a search's artists keep the order they were found in, followed or not
        let found = [("not_followed".to_owned(), 4), ("early_bird".to_owned(), 1), ("early_bird".to_owned(), 2)];
        let searched = artist_rows(&conn, &everything, vec!["not_followed".to_owned(), "early_bird".to_owned()], &found).unwrap();
        let searched: Vec<(String, Vec<i64>)> =
            searched.into_iter().map(|row| (row.artist.name, row.posts.iter().map(|post| post.id).collect())).collect();
        assert_eq!(searched, [("not_followed".to_owned(), vec![4]), ("early_bird".to_owned(), vec![2, 1])]);
    }
}
