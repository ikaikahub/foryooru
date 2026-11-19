use crate::db::{now_ms, page_count};
use crate::error::{Error, Result};
pub use foryooru_core::activity::*;
use rusqlite::types::Type;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};

const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const PAGE_SIZE: i64 = 50;
const MAX_QUERY_LENGTH: usize = 500;

#[derive(Clone, Copy)]
pub enum Reaction {
    Like,
    Unlike,
    Dislike,
    Undislike,
}

impl Reaction {
    fn kind(self) -> &'static str {
        match self {
            Reaction::Like => "like",
            Reaction::Unlike => "unlike",
            Reaction::Dislike => "dislike",
            Reaction::Undislike => "undislike",
        }
    }
}

pub fn record_reaction(conn: &Connection, reaction: Reaction, post: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO activity (at, kind, post_id) VALUES (?1, ?2, ?3)",
        params![now_ms(), reaction.kind(), post],
    )?;
    Ok(())
}

pub fn record_view(
    conn: &Connection,
    post: i64,
    source: Source,
    dwell_ms: Option<i64>,
    quick_skip: bool,
    long_look: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO activity (at, kind, post_id, source, dwell_ms, quick_skip, long_look) VALUES (?1, 'view', ?2, ?3, ?4, ?5, ?6)",
        params![now_ms(), post, source.name(), dwell_ms, quick_skip, long_look],
    )?;
    Ok(())
}

pub fn record_search(conn: &Connection, query: &str) -> Result<()> {
    let query = query.trim();
    if query.is_empty() || query.len() > MAX_QUERY_LENGTH {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO searches (at, query) VALUES (?1, ?2)",
        params![now_ms(), query],
    )?;
    Ok(())
}

pub fn record_query(
    conn: &Connection,
    endpoint: &str,
    query: &str,
    page: Option<i64>,
    failure: Option<&Error>,
    ms: i64,
) -> Result<()> {
    let (status, error) = match failure {
        None => (Some(200), None),
        Some(Error::Status(code)) => (Some(*code), None),
        Some(Error::Rejected {
            status, message, ..
        }) => (Some(*status), Some(message.clone())),
        Some(error) => (None, Some(error.to_string())),
    };
    conn.execute(
        "INSERT INTO activity (at, kind, endpoint, query, page, status, error, ms) VALUES (?1, 'query', ?2, ?3, ?4, ?5, ?6, ?7)",
        params![now_ms(), endpoint, query, page, status, error, ms],
    )?;
    Ok(())
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Signal {
    ScrollBack,
    VideoLooped,
    Info,
    Shared,
    Tag,
    Similar,
    Opened,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Signal::ScrollBack => "scrollBack",
            Signal::VideoLooped => "videoLooped",
            Signal::Info => "info",
            Signal::Shared => "shared",
            Signal::Tag => "tag",
            Signal::Similar => "similar",
            Signal::Opened => "opened",
        }
    }
}

pub fn record_signal(
    conn: &Connection,
    post: i64,
    signal: Signal,
    tag: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO signals (at, post_id, kind, tag) SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?2)",
        params![now_ms(), post, signal.name(), tag],
    )?;
    Ok(())
}

#[derive(Deserialize)]
pub struct SignalEntry {
    pub post: i64,
    pub signal: Signal,
    #[serde(default)]
    pub tag: Option<String>,
}

pub fn record_signals(conn: &Connection, signals: &[SignalEntry]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for entry in signals {
        record_signal(&tx, entry.post, entry.signal, entry.tag.as_deref())?;
    }
    tx.commit()?;
    Ok(())
}

const LOGGED: &str = "SELECT at, kind, post_id, source, quick_skip, long_look FROM (
         SELECT at, id, kind, post_id, source, quick_skip, long_look FROM activity
         WHERE kind IN ('view', 'like', 'unlike', 'dislike', 'undislike')
         UNION ALL
         SELECT at, id, 'engaged', post_id, NULL, 0, 0 FROM signals
     )";

fn logged_from_row(row: &Row) -> rusqlite::Result<Logged> {
    let kind: String = row.get(1)?;
    let happened = match kind.as_str() {
        "view" => Happened::View,
        "like" => Happened::Like,
        "unlike" => Happened::Unlike,
        "dislike" => Happened::Dislike,
        "undislike" => Happened::Undislike,
        "engaged" => Happened::Engaged,
        other => return Err(unknown(1, other)),
    };
    Ok(Logged {
        at: row.get(0)?,
        happened,
        post: row.get(2)?,
        source: match row.get::<_, Option<String>>(3)? {
            Some(name) => Some(Source::parse(&name).ok_or_else(|| unknown(3, &name))?),
            None => None,
        },
        quick_skip: row.get(4)?,
        long_look: row.get(5)?,
    })
}

fn newest_logged(conn: &Connection, at_or_before: i64, limit: usize) -> Result<Vec<Logged>> {
    let mut statement = conn.prepare_cached(&format!(
        "{LOGGED} WHERE at <= ?1 ORDER BY at DESC, id DESC LIMIT ?2"
    ))?;
    Ok(statement
        .query_map(params![at_or_before, limit as i64], logged_from_row)?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn logged_after(conn: &Connection, after: i64) -> Result<Vec<Logged>> {
    let mut earlier = newest_logged(conn, after, SESSION_EVENTS)?;
    earlier.reverse();
    let mut statement = conn.prepare_cached(&format!("{LOGGED} WHERE at > ?1 ORDER BY at, id"))?;
    let later = statement
        .query_map([after], logged_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    earlier.extend(later);
    Ok(earlier)
}

fn recent_searches(conn: &Connection, now: i64) -> Result<Vec<Searched>> {
    let mut statement = conn.prepare_cached(
        "SELECT at, query FROM searches WHERE at <= ?1 ORDER BY at DESC, id DESC LIMIT ?2",
    )?;
    Ok(statement
        .query_map(params![now, SESSION_EVENTS as i64], |row| {
            Ok(Searched {
                at: row.get(0)?,
                query: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn session(conn: &Connection) -> Result<Vec<Moment>> {
    let now = now_ms();
    Ok(moments(
        newest_logged(conn, now, SESSION_EVENTS)?.iter(),
        &recent_searches(conn, now)?,
        now,
    ))
}

pub fn prune(conn: &Connection) -> Result<()> {
    let oldest = now_ms() - RETENTION_MS;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM activity WHERE at < ?1", [oldest])?;
    tx.execute("DELETE FROM signals WHERE at < ?1", [oldest])?;
    tx.execute("DELETE FROM searches WHERE at < ?1", [oldest])?;
    tx.commit()?;
    Ok(())
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    View {
        post: i64,
        source: Source,
        dwell_ms: Option<i64>,
        quick_skip: bool,
        long_look: bool,
    },
    Like {
        post: i64,
    },
    Unlike {
        post: i64,
    },
    Dislike {
        post: i64,
    },
    Undislike {
        post: i64,
    },
    Query {
        endpoint: String,
        query: String,
        page: Option<i64>,
        status: Option<u16>,
        error: Option<String>,
        ms: i64,
    },
}

#[derive(Serialize)]
pub struct Entry {
    id: i64,
    at: i64,
    #[serde(flatten)]
    event: Event,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Filter {
    All,
    Views,
    Likes,
    Dislikes,
    Queries,
}

impl Filter {
    fn kinds(self) -> &'static [&'static str] {
        match self {
            Filter::All => &["view", "like", "unlike", "dislike", "undislike", "query"],
            Filter::Views => &["view"],
            Filter::Likes => &["like", "unlike"],
            Filter::Dislikes => &["dislike", "undislike"],
            Filter::Queries => &["query"],
        }
    }
}

#[derive(Serialize)]
pub struct ActivityPage {
    entries: Vec<Entry>,
    total: i64,
    pages: i64,
}

fn unknown(column: usize, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        Type::Text,
        format!("unknown activity value {value:?}").into(),
    )
}

fn entry_from_row(row: &Row) -> rusqlite::Result<Entry> {
    let kind: String = row.get(2)?;
    let post = || row.get::<_, i64>(3);
    let event = match kind.as_str() {
        "view" => Event::View {
            post: post()?,
            source: {
                let source: String = row.get(4)?;
                Source::parse(&source).ok_or_else(|| unknown(4, &source))?
            },
            dwell_ms: row.get(5)?,
            quick_skip: row.get(6)?,
            long_look: row.get(7)?,
        },
        "like" => Event::Like { post: post()? },
        "unlike" => Event::Unlike { post: post()? },
        "dislike" => Event::Dislike { post: post()? },
        "undislike" => Event::Undislike { post: post()? },
        "query" => Event::Query {
            endpoint: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
            query: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
            page: row.get(10)?,
            status: row.get(11)?,
            error: row.get(12)?,
            ms: row.get(13)?,
        },
        other => return Err(unknown(2, other)),
    };
    Ok(Entry {
        id: row.get(0)?,
        at: row.get(1)?,
        event,
    })
}

pub fn page(conn: &Connection, filter: Filter, page: i64) -> Result<ActivityPage> {
    let kinds = serde_json::to_string(filter.kinds())?;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM activity WHERE kind IN (SELECT value FROM json_each(?1))",
        [&kinds],
        |row| row.get(0),
    )?;
    let mut statement = conn.prepare_cached(
        "SELECT id, at, kind, post_id, source, dwell_ms, quick_skip, long_look, endpoint, query, page, status, error, ms
         FROM activity WHERE kind IN (SELECT value FROM json_each(?1)) ORDER BY at DESC, id DESC LIMIT ?2 OFFSET ?3",
    )?;
    let entries = statement
        .query_map(params![kinds, PAGE_SIZE, page * PAGE_SIZE], entry_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ActivityPage {
        entries,
        total,
        pages: page_count(total, PAGE_SIZE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(conn: &Connection, at: i64, kind: &str, post: i64) {
        conn.execute(
            "INSERT INTO activity (at, kind, post_id) VALUES (?1, ?2, ?3)",
            params![at, kind, post],
        )
        .unwrap();
    }

    fn reacted_posts(conn: &Connection) -> Vec<i64> {
        session(conn)
            .unwrap()
            .into_iter()
            .filter(|moment| matches!(moment.reacted, Reacted::Liked | Reacted::Disliked))
            .filter_map(|moment| moment.post())
            .collect()
    }

    fn tables() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE posts (id INTEGER PRIMARY KEY);
             INSERT INTO posts (id) VALUES (1), (2), (3);
             CREATE TABLE activity (id INTEGER PRIMARY KEY, at INTEGER NOT NULL, kind TEXT NOT NULL, post_id INTEGER,
             source TEXT, quick_skip INTEGER NOT NULL DEFAULT 0, long_look INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE signals (id INTEGER PRIMARY KEY, at INTEGER NOT NULL, post_id INTEGER NOT NULL, kind TEXT NOT NULL, tag TEXT);
             CREATE TABLE searches (id INTEGER PRIMARY KEY, at INTEGER NOT NULL, query TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn taking_a_like_back_removes_it() {
        let conn = tables();
        let now = now_ms();
        log(&conn, now - 5000, "like", 1);
        log(&conn, now - 4000, "unlike", 1);
        log(&conn, now - 3000, "like", 2);
        log(&conn, now - 2000, "dislike", 3);
        log(&conn, now - 1000, "like", 3);
        assert_eq!(reacted_posts(&conn), [3, 2]);
    }

    #[test]
    fn signals_show_up_as_engagement() {
        let conn = tables();
        record_signal(&conn, 2, Signal::Shared, None).unwrap();
        record_signal(&conn, 99, Signal::Tag, Some("cat_ears")).unwrap();
        let engaged: Vec<i64> = session(&conn)
            .unwrap()
            .into_iter()
            .filter(|moment| matches!(moment.reacted, Reacted::Engaged))
            .filter_map(|moment| moment.post())
            .collect();
        assert_eq!(engaged, [2]);
    }

    #[test]
    fn searches_join_the_session_in_time_order() {
        let conn = tables();
        let now = now_ms();
        log(&conn, now - 3000, "like", 1);
        conn.execute(
            "INSERT INTO searches (at, query) VALUES (?1, 'cat_ears')",
            [now - 2000],
        )
        .unwrap();
        log(&conn, now - 1000, "like", 2);
        let order: Vec<String> = session(&conn)
            .unwrap()
            .into_iter()
            .map(|moment| match moment.subject {
                Subject::Post(post) => post.to_string(),
                Subject::Search(query) => query,
            })
            .collect();
        assert_eq!(order, ["2", "cat_ears", "1"]);
    }
}
