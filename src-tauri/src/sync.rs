// keeps likes, dislikes, favourite tags, the blacklist and saved searches in step with the account on the sync worker.
// sync_snapshot holds what the account looked like after the last sync, so comparing it with the live tables
// shows what changed here without touching every place that edits them
use crate::booru::{Lineage, RawPost};
use crate::catalogue;
use crate::core::Core;
use crate::db::now_ms;
use crate::error::{Error, Result};
use crate::search::Rating;
use argon2::{Algorithm, Argon2, Params, Version};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

// the deployed worker. `pnpm dev` in worker/ serves a local copy at http://127.0.0.1:8787
const SERVER: &str = "https://foryooru-sync.foryooru-sync.workers.dev";
const STORE: &str = "account.json";
const ACCOUNT_SETTING: &str = "account";
pub const EVENT: &str = "sync";
const TIMEOUT: Duration = Duration::from_secs(30);
// lets a burst of likes or tag edits finish so they go up in one push
const DELAY: Duration = Duration::from_secs(5);
// matches PUSH_LIMIT in the worker
const PUSH_BATCH: usize = 200;
const MAX_PULL_PAGES: usize = 1000;
const MIN_PASSWORD: usize = 8;
const USER_META: &str = "sync_user";
const CURSOR_META: &str = "sync_cursor";
const SYNCED_AT_META: &str = "sync_at";

const CURRENT_ITEMS: &str = "WITH current (list, key, sort_at, changed_at) AS (
    SELECT 'like', CAST(post_id AS TEXT), sort_at, COALESCE(liked_at, sort_at) FROM likes
    UNION ALL SELECT 'dislike', CAST(post_id AS TEXT), disliked_at, disliked_at FROM dislikes
    UNION ALL SELECT 'favouriteTag', name, created_at, created_at FROM favourite_tags
    UNION ALL SELECT 'blacklist', name, created_at, created_at FROM blacklist
    UNION ALL SELECT 'savedSearch', query, created_at, created_at FROM saved_searches
    UNION ALL SELECT 'followedArtist', name, created_at, created_at FROM followed_artists
)";

#[derive(Serialize, Deserialize, Clone)]
struct Account {
    username: String,
    token: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub last_synced: Option<i64>,
    pub pulled: usize,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    list: String,
    key: String,
    present: bool,
    sort_at: i64,
    changed_at: i64,
    post: Option<SyncedPost>,
}

// enough of a post for another device to show it without asking the booru
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncedPost {
    id: i64,
    md5: String,
    rating: String,
    score: i64,
    tags: Vec<String>,
    width: i64,
    height: i64,
    preview_url: String,
    preview_width: i64,
    preview_height: i64,
    sample_url: String,
    file_url: String,
    posted_at: Option<i64>,
    parent_id: Option<i64>,
    has_children: Option<bool>,
}

impl SyncedPost {
    fn into_raw(self) -> Option<RawPost> {
        Some(RawPost {
            id: self.id,
            md5: self.md5,
            rating: Rating::parse(&self.rating)?,
            score: self.score,
            tags: self.tags,
            width: self.width,
            height: self.height,
            preview_url: self.preview_url,
            preview_width: self.preview_width,
            preview_height: self.preview_height,
            sample_url: self.sample_url,
            file_url: self.file_url,
            posted_at: self.posted_at,
            lineage: self.has_children.map(|has_children| Lineage {
                parent: self.parent_id,
                has_children,
            }),
        })
    }
}

#[derive(Deserialize)]
struct Session {
    token: String,
}

#[derive(Deserialize)]
struct PullPage {
    items: Vec<Item>,
    cursor: i64,
    more: bool,
}

fn account(app: &AppHandle) -> Option<Account> {
    app.store(STORE)
        .ok()?
        .get(ACCOUNT_SETTING)
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn username(app: &AppHandle) -> Option<String> {
    account(app).map(|account| account.username)
}

fn save_account(app: &AppHandle, account: Option<&Account>) -> Result<()> {
    let store = app.store(STORE)?;
    match account {
        Some(account) => store.set(ACCOUNT_SETTING, serde_json::to_value(account)?),
        None => {
            store.delete(ACCOUNT_SETTING);
        }
    }
    store.save()?;
    Ok(())
}

// argon2 is too slow for the worker's 10ms CPU budget, so the app stretches the password and the worker only
// stores a SHA-256 of the result. the username is the salt because the app needs one before it can ask anything
fn derive_key(username: &str, password: &str) -> Result<String> {
    let invalid = |error: argon2::Error| Error::Account(error.to_string());
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(invalid)?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), format!("foryooru:{username}").as_bytes(), &mut key)
        .map_err(invalid)?;
    Ok(key.iter().map(|byte| format!("{byte:02x}")).collect())
}

async fn call<T: DeserializeOwned>(core: &Core, path: &str, token: Option<&str>, body: Value) -> Result<T> {
    let mut request = core
        .http
        .post(format!("{SERVER}{path}"))
        .timeout(TIMEOUT)
        .json(&body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    let answer: Value = response.json().await?;
    if status >= 400 {
        let message = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The sync server refused the request")
            .to_owned();
        return Err(Error::SyncRejected { status, message });
    }
    Ok(serde_json::from_value(answer)?)
}

pub async fn log_in(core: &Arc<Core>, username: &str, password: &str, new_account: bool) -> Result<String> {
    let username = username.trim().to_lowercase();
    if new_account && password.chars().count() < MIN_PASSWORD {
        return Err(Error::Account(format!("Passwords need at least {MIN_PASSWORD} characters")));
    }
    let key = {
        let (username, password) = (username.clone(), password.to_owned());
        tauri::async_runtime::spawn_blocking(move || derive_key(&username, &password)).await??
    };
    let path = if new_account { "/signup" } else { "/login" };
    let session: Session = call(core, path, None, json!({ "username": username, "key": key })).await?;
    save_account(&core.app, Some(&Account { username: username.clone(), token: session.token }))?;
    sync_now(core);
    Ok(username)
}

pub async fn log_out(core: &Core) -> Result<()> {
    if let Some(account) = account(&core.app) {
        // the token is forgotten here either way, telling the worker just stops a copied token from working
        if let Err(error) = call::<Value>(core, "/logout", Some(&account.token), json!({})).await {
            crate::platform::log(&format!("Couldn't end the sync session: {error}"));
        }
    }
    save_account(&core.app, None)?;
    core.sync.update(&core.app, |status| *status = SyncStatus::default());
    Ok(())
}

pub fn status(core: &Core) -> Result<SyncStatus> {
    let mut status = core.sync.get();
    if status.last_synced.is_none() {
        status.last_synced = meta(&core.db.conn(), SYNCED_AT_META)?.and_then(|value| value.parse().ok());
    }
    Ok(status)
}

pub fn sync_now(core: &Arc<Core>) {
    if account(&core.app).is_none() {
        return;
    }
    let core = core.clone();
    crate::platform::spawn(async move { run(&core).await });
}

pub fn sync_soon(core: &Arc<Core>) {
    if account(&core.app).is_none() || core.sync_queued.swap(true, Ordering::SeqCst) {
        return;
    }
    let core = core.clone();
    crate::platform::spawn(async move {
        crate::platform::sleep(DELAY).await;
        core.sync_queued.store(false, Ordering::SeqCst);
        run(&core).await;
    });
}

async fn run(core: &Arc<Core>) {
    let _running = core.sync_lock.lock().await;
    let Some(account) = account(&core.app) else {
        return;
    };
    core.sync.update(&core.app, |status| {
        status.running = true;
        status.error = None;
    });
    let result = sync(core, &account).await;
    // the worker no longer knows this session, so stay logged out until the user logs in again
    if let Err(Error::SyncRejected { status: 401, .. }) = &result
        && let Err(error) = save_account(&core.app, None)
    {
        crate::platform::log(&format!("Couldn't forget the sync session: {error}"));
    }
    core.sync.update(&core.app, |status| {
        status.running = false;
        match &result {
            Ok(pulled) => {
                status.last_synced = Some(now_ms());
                status.pulled = *pulled;
            }
            Err(error) => status.error = Some(error.to_string()),
        }
    });
}

async fn sync(core: &Arc<Core>, account: &Account) -> Result<usize> {
    let site = serde_json::to_value(core.site)?;
    let token = Some(account.token.as_str());
    let (changes, mut cursor) = local_changes(&core.db.conn(), &account.username)?;
    for batch in changes.chunks(PUSH_BATCH) {
        call::<Value>(core, "/push", token, json!({ "site": site, "items": batch })).await?;
        mark_pushed(&core.db.conn(), batch)?;
    }
    let mut pulled = 0;
    for _ in 0..MAX_PULL_PAGES {
        let page: PullPage = call(core, "/pull", token, json!({ "site": site, "since": cursor })).await?;
        if page.more && page.cursor <= cursor {
            return Err(Error::Account("The sync server sent the same page twice".to_owned()));
        }
        pulled += apply(core, page.items)?;
        cursor = page.cursor;
        let conn = core.db.conn();
        set_meta(&conn, CURSOR_META, &cursor.to_string())?;
        if !page.more {
            set_meta(&conn, SYNCED_AT_META, &now_ms().to_string())?;
            return Ok(pulled);
        }
    }
    Err(Error::Account("The account has more changes than one sync can pull".to_owned()))
}

fn meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| row.get(0))
        .optional()?)
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

fn local_changes(conn: &Connection, username: &str) -> Result<(Vec<Item>, i64)> {
    // a different account starts from an empty snapshot, so everything here merges into it
    if meta(conn, USER_META)?.as_deref() != Some(username) {
        conn.execute("DELETE FROM sync_snapshot", [])?;
        conn.execute("DELETE FROM meta WHERE key = ?1", [CURSOR_META])?;
        set_meta(conn, USER_META, username)?;
    }
    let cursor = meta(conn, CURSOR_META)?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let mut changes = Vec::new();
    {
        let mut added = conn.prepare(&format!(
            "{CURRENT_ITEMS} SELECT list, key, sort_at, changed_at FROM current c
             WHERE NOT EXISTS (SELECT 1 FROM sync_snapshot s WHERE s.list = c.list AND s.key = c.key)"
        ))?;
        for row in added.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get(2)?, row.get(3)?))
        })? {
            let (list, key, sort_at, changed_at) = row?;
            let post = match list.as_str() {
                "like" | "dislike" => synced_post(conn, key.parse().unwrap_or_default())?,
                _ => None,
            };
            changes.push(Item { list, key, present: true, sort_at, changed_at, post });
        }
    }
    let removed_at = now_ms();
    let mut removed = conn.prepare(&format!(
        "{CURRENT_ITEMS} SELECT list, key FROM sync_snapshot s
         WHERE NOT EXISTS (SELECT 1 FROM current c WHERE c.list = s.list AND c.key = s.key)"
    ))?;
    for row in removed.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
        let (list, key) = row?;
        changes.push(Item {
            list,
            key,
            present: false,
            sort_at: removed_at,
            changed_at: removed_at,
            post: None,
        });
    }
    Ok((changes, cursor))
}

fn synced_post(conn: &Connection, id: i64) -> Result<Option<SyncedPost>> {
    Ok(conn
        .query_row(
            "SELECT id, md5, rating, score, width, height, preview_url, preview_width, preview_height, sample_url,
                    file_url, posted_at, parent_id, has_children,
                    (SELECT group_concat(t.name, ' ') FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = p.id)
             FROM posts p WHERE id = ?1",
            [id],
            |row| {
                Ok(SyncedPost {
                    id: row.get(0)?,
                    md5: row.get(1)?,
                    rating: row.get(2)?,
                    score: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    preview_url: row.get(6)?,
                    preview_width: row.get(7)?,
                    preview_height: row.get(8)?,
                    sample_url: row.get(9)?,
                    file_url: row.get(10)?,
                    posted_at: row.get(11)?,
                    parent_id: row.get(12)?,
                    has_children: row.get(13)?,
                    tags: row
                        .get::<_, Option<String>>(14)?
                        .map(|names| names.split(' ').map(str::to_owned).collect())
                        .unwrap_or_default(),
                })
            },
        )
        .optional()?)
}

fn mark_pushed(conn: &Connection, items: &[Item]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for item in items {
        remember(&tx, item, item.present)?;
    }
    tx.commit()?;
    Ok(())
}

fn remember(tx: &Transaction, item: &Item, present: bool) -> Result<()> {
    if present {
        tx.execute(
            "INSERT OR IGNORE INTO sync_snapshot (list, key) VALUES (?1, ?2)",
            params![item.list, item.key],
        )?;
    } else {
        tx.execute(
            "DELETE FROM sync_snapshot WHERE list = ?1 AND key = ?2",
            params![item.list, item.key],
        )?;
    }
    Ok(())
}

// the table and key column behind each synced list
fn table_of(list: &str) -> Option<(&'static str, &'static str)> {
    match list {
        "like" => Some(("likes", "post_id")),
        "dislike" => Some(("dislikes", "post_id")),
        "favouriteTag" => Some(("favourite_tags", "name")),
        "blacklist" => Some(("blacklist", "name")),
        "savedSearch" => Some(("saved_searches", "query")),
        "followedArtist" => Some(("followed_artists", "name")),
        _ => None,
    }
}

fn apply(core: &Arc<Core>, mut items: Vec<Item>) -> Result<usize> {
    // posts liked on another device are stored first, likes can only point at posts that exist here
    let new_posts: Vec<RawPost> = {
        let conn = core.db.conn();
        let mut known = conn.prepare_cached("SELECT EXISTS (SELECT 1 FROM posts WHERE id = ?1)")?;
        let mut new_posts = Vec::new();
        for item in items.iter_mut().filter(|item| item.present) {
            if let Some(post) = item.post.take()
                && !known.query_row([post.id], |row| row.get::<_, bool>(0))?
                && let Some(raw) = post.into_raw()
            {
                new_posts.push(raw);
            }
        }
        new_posts
    };
    if !new_posts.is_empty() {
        let mut conn = core.db.conn();
        let stored = catalogue::store_posts(&mut conn, &new_posts, false)?;
        catalogue::after_store(core, &stored);
    }
    apply_items(&core.db.conn(), &items)
}

fn apply_items(conn: &Connection, items: &[Item]) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = 0;
    for item in items {
        let Some((table, column)) = table_of(&item.list) else {
            continue;
        };
        let key = match column {
            "post_id" => match item.key.parse::<i64>() {
                Ok(id) => rusqlite::types::Value::Integer(id),
                Err(_) => continue,
            },
            _ => rusqlite::types::Value::Text(item.key.clone()),
        };
        if !item.present {
            changed += tx.execute(&format!("DELETE FROM {table} WHERE {column} = ?1"), [&key])?;
            remember(&tx, item, false)?;
            continue;
        }
        let inserted = match item.list.as_str() {
            "like" => tx.execute(
                "INSERT INTO likes (post_id, sort_at, liked_at) SELECT ?1, ?2, ?3
                 WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?1) ON CONFLICT (post_id) DO NOTHING",
                params![key, item.sort_at, item.changed_at],
            )?,
            "dislike" => tx.execute(
                "INSERT INTO dislikes (post_id, disliked_at) SELECT ?1, ?2
                 WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?1) ON CONFLICT (post_id) DO NOTHING",
                params![key, item.sort_at],
            )?,
            _ => tx.execute(
                &format!("INSERT OR IGNORE INTO {table} ({column}, created_at) VALUES (?1, ?2)"),
                params![key, item.sort_at],
            )?,
        };
        // a post is liked or disliked, never both. the snapshot keeps the other one, so its removal gets pushed
        if inserted > 0 {
            match item.list.as_str() {
                "like" => tx.execute("DELETE FROM dislikes WHERE post_id = ?1", [&key])?,
                "dislike" => tx.execute("DELETE FROM likes WHERE post_id = ?1", [&key])?,
                _ => 0,
            };
        }
        changed += inserted;
        let exists: bool = tx.query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE {column} = ?1)"),
            [&key],
            |row| row.get(0),
        )?;
        // a like whose post couldn't be stored stays out of the snapshot, so it isn't pushed back as a removal
        if exists {
            remember(&tx, item, true)?;
        }
    }
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{self, TagList};

    fn device() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn
    }

    fn store_post(conn: &mut Connection, id: i64) {
        let raw = RawPost {
            id,
            md5: "0".repeat(32),
            rating: Rating::General,
            score: 1,
            tags: vec!["blue_sky".to_owned(), "cat".to_owned()],
            width: 10,
            height: 10,
            preview_url: "preview".to_owned(),
            preview_width: 1,
            preview_height: 1,
            sample_url: "sample".to_owned(),
            file_url: "file.png".to_owned(),
            posted_at: None,
            lineage: None,
        };
        catalogue::store_posts(conn, &[raw], false).unwrap();
    }

    // pushes this device's changes and returns them as the server would hand them to the other device
    fn push(conn: &Connection) -> Vec<Item> {
        let (changes, _) = local_changes(conn, "jay").unwrap();
        mark_pushed(conn, &changes).unwrap();
        changes
    }

    // the device side of apply(), without the post index that needs a running app
    fn receive(conn: &mut Connection, mut items: Vec<Item>) {
        local_changes(conn, "jay").unwrap();
        let posts: Vec<RawPost> = items
            .iter_mut()
            .filter_map(|item| item.post.take()?.into_raw())
            .collect();
        catalogue::store_posts(conn, &posts, false).unwrap();
        apply_items(conn, &items).unwrap();
    }

    fn liked(conn: &Connection, id: i64) -> bool {
        library::post_state(conn, id).unwrap().liked
    }

    #[test]
    fn a_like_travels_with_its_post_and_comes_back_as_an_unlike() {
        let (mut phone, mut desktop) = (device(), device());
        store_post(&mut phone, 7);
        library::toggle_like(&mut phone, 7).unwrap();

        let changes = push(&phone);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].post.as_ref().unwrap().tags, ["blue_sky", "cat"]);
        assert!(push(&phone).is_empty());

        receive(&mut desktop, changes);
        assert!(liked(&desktop, 7));
        assert!(push(&desktop).is_empty(), "applied items must not be pushed back");

        library::toggle_like(&mut desktop, 7).unwrap();
        let unlike = push(&desktop);
        assert!(!unlike[0].present);
        receive(&mut phone, unlike);
        assert!(!liked(&phone, 7));
        assert!(push(&phone).is_empty());
    }

    #[test]
    fn a_dislike_from_elsewhere_replaces_the_local_like() {
        let (mut phone, mut desktop) = (device(), device());
        store_post(&mut phone, 7);
        library::toggle_like(&mut phone, 7).unwrap();
        receive(&mut desktop, push(&phone));

        library::toggle_dislike(&mut desktop, 7).unwrap();
        receive(&mut phone, push(&desktop));
        let state = library::post_state(&phone, 7).unwrap();
        assert!(state.disliked && !state.liked);
        assert!(push(&phone).is_empty());
    }

    #[test]
    fn tags_and_saved_searches_sync() {
        let (phone, mut desktop) = (device(), device());
        library::toggle_tag(&phone, TagList::Blacklist, "gore").unwrap();
        library::toggle_saved_search(&phone, "cat rating:general").unwrap();
        receive(&mut desktop, push(&phone));
        assert_eq!(library::tag_names(&desktop, TagList::Blacklist).unwrap(), ["gore"]);
        assert_eq!(library::saved_searches(&desktop).unwrap()[0].query, "cat rating:general");
    }

    #[test]
    fn a_like_without_its_post_is_skipped_rather_than_pushed_back_as_removed() {
        let mut desktop = device();
        let orphan = Item {
            list: "like".to_owned(),
            key: "99".to_owned(),
            present: true,
            sort_at: 1,
            changed_at: 1,
            post: None,
        };
        receive(&mut desktop, vec![orphan]);
        assert!(!liked(&desktop, 99));
        assert!(push(&desktop).is_empty());
    }

    #[test]
    fn the_derived_key_matches_what_the_worker_accepts() {
        let key = derive_key("jay", "hunter22").unwrap();
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        assert_eq!(key, derive_key("jay", "hunter22").unwrap());
        assert_ne!(key, derive_key("kay", "hunter22").unwrap());
    }

    #[test]
    fn another_account_starts_by_merging_everything() {
        let mut phone = device();
        store_post(&mut phone, 7);
        library::toggle_like(&mut phone, 7).unwrap();
        push(&phone);
        let (changes, cursor) = local_changes(&phone, "someone_else").unwrap();
        assert_eq!((changes.len(), cursor), (1, 0));
    }
}
