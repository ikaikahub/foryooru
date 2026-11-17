use crate::error::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const MIGRATIONS: &[&str] = &["
    CREATE TABLE tags (
        id              INTEGER PRIMARY KEY,
        name            TEXT NOT NULL UNIQUE,
        type            INTEGER,
        gelbooru_count  INTEGER,
        post_count      INTEGER NOT NULL DEFAULT 0,
        details_fetched INTEGER NOT NULL DEFAULT 0 CHECK (details_fetched IN (0, 1))
    );
    CREATE INDEX tags_without_details ON tags (id) WHERE details_fetched = 0;

    CREATE TABLE posts (
        id             INTEGER PRIMARY KEY,
        seq            INTEGER NOT NULL UNIQUE,
        md5            TEXT NOT NULL,
        rating         TEXT NOT NULL,
        score          INTEGER NOT NULL,
        tag_string     TEXT NOT NULL,
        width          INTEGER NOT NULL,
        height         INTEGER NOT NULL,
        preview_url    TEXT NOT NULL,
        preview_width  INTEGER NOT NULL,
        preview_height INTEGER NOT NULL,
        sample_url     TEXT NOT NULL,
        file_url       TEXT NOT NULL,
        posted_at      INTEGER,
        partial        INTEGER NOT NULL DEFAULT 0 CHECK (partial IN (0, 1)),
        media_resolved INTEGER NOT NULL DEFAULT 0 CHECK (media_resolved IN (0, 1))
    );

    CREATE TABLE post_tags (
        post_id INTEGER NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        tag_id  INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
        PRIMARY KEY (post_id, tag_id)
    ) WITHOUT ROWID;
    CREATE INDEX post_tags_by_tag ON post_tags (tag_id);

    CREATE TABLE search_pages (
        query      TEXT NOT NULL,
        page       INTEGER NOT NULL,
        total      INTEGER NOT NULL,
        post_ids   TEXT NOT NULL CHECK (json_valid(post_ids)),
        fetched_at INTEGER NOT NULL,
        PRIMARY KEY (query, page)
    );

    CREATE TABLE likes (
        post_id  INTEGER PRIMARY KEY REFERENCES posts (id) ON DELETE CASCADE,
        sort_at  INTEGER NOT NULL,
        liked_at INTEGER
    );
    CREATE INDEX likes_by_sort ON likes (sort_at);

    CREATE TABLE dislikes (
        post_id     INTEGER PRIMARY KEY REFERENCES posts (id) ON DELETE CASCADE,
        disliked_at INTEGER NOT NULL
    );

    CREATE TABLE favourite_tags (
        name       TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE blacklist (
        name       TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE saved_searches (
        query      TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE impressions (
        post_id     INTEGER PRIMARY KEY REFERENCES posts (id) ON DELETE CASCADE,
        views       INTEGER NOT NULL DEFAULT 0,
        feed_views  INTEGER NOT NULL DEFAULT 0,
        dwell_ms    INTEGER NOT NULL DEFAULT 0,
        quick_skips INTEGER NOT NULL DEFAULT 0,
        long_views  INTEGER NOT NULL DEFAULT 0,
        opens       INTEGER NOT NULL DEFAULT 0,
        first_seen  INTEGER NOT NULL,
        last_seen   INTEGER NOT NULL
    );
    CREATE INDEX impressions_by_last_seen ON impressions (last_seen);
", "
    CREATE TABLE activity (
        id         INTEGER PRIMARY KEY,
        at         INTEGER NOT NULL,
        kind       TEXT NOT NULL CHECK (kind IN ('view', 'like', 'unlike', 'dislike', 'undislike', 'query')),
        post_id    INTEGER,
        source     TEXT CHECK (source IN ('grid', 'viewer', 'feed')),
        dwell_ms   INTEGER,
        quick_skip INTEGER NOT NULL DEFAULT 0 CHECK (quick_skip IN (0, 1)),
        long_look  INTEGER NOT NULL DEFAULT 0 CHECK (long_look IN (0, 1)),
        endpoint   TEXT,
        query      TEXT,
        page       INTEGER,
        status     INTEGER,
        error      TEXT,
        ms         INTEGER
    );
    CREATE INDEX activity_by_time ON activity (at);
    CREATE INDEX activity_by_kind ON activity (kind, at);
", "
    ALTER TABLE posts DROP COLUMN tag_string;
", "
    ALTER TABLE tags RENAME COLUMN gelbooru_count TO site_count;
", "
    DELETE FROM impressions WHERE post_id IN (
        SELECT id FROM posts WHERE lower(file_url) LIKE '%.gif' OR lower(file_url) LIKE '%.mp4' OR lower(file_url) LIKE '%.webm'
    );
", "
    DROP TABLE IF EXISTS jev_training;
", "
    DROP TABLE IF EXISTS jev_training;
    CREATE TABLE model_state (
        id    INTEGER PRIMARY KEY CHECK (id = 1),
        state TEXT NOT NULL
    );
", "
    CREATE TABLE post_leads (
        post_id INTEGER PRIMARY KEY REFERENCES posts (id) ON DELETE CASCADE,
        tag     TEXT NOT NULL
    );
    CREATE INDEX activity_by_post ON activity (post_id);
", "
    CREATE TABLE signals (
        id      INTEGER PRIMARY KEY,
        at      INTEGER NOT NULL,
        post_id INTEGER NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        kind    TEXT NOT NULL CHECK (kind IN ('scrollBack', 'videoLooped', 'info', 'shared', 'tag', 'similar')),
        tag     TEXT
    );
    CREATE INDEX signals_by_post ON signals (post_id);
    CREATE INDEX signals_by_time ON signals (at);
", "
    ALTER TABLE posts ADD COLUMN parent_id INTEGER;
    ALTER TABLE posts ADD COLUMN has_children INTEGER CHECK (has_children IN (0, 1));
    CREATE INDEX posts_by_parent ON posts (parent_id);
", "
    CREATE TABLE signals_next (
        id      INTEGER PRIMARY KEY,
        at      INTEGER NOT NULL,
        post_id INTEGER NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        kind    TEXT NOT NULL CHECK (kind IN ('scrollBack', 'videoLooped', 'info', 'shared', 'tag', 'similar', 'opened')),
        tag     TEXT
    );
    INSERT INTO signals_next SELECT id, at, post_id, kind, tag FROM signals;
    DROP TABLE signals;
    ALTER TABLE signals_next RENAME TO signals;
    CREATE INDEX signals_by_post ON signals (post_id);
    CREATE INDEX signals_by_time ON signals (at);
    CREATE TABLE searches (
        id    INTEGER PRIMARY KEY,
        at    INTEGER NOT NULL,
        query TEXT NOT NULL
    );
    CREATE INDEX searches_by_time ON searches (at);
"];

pub const TABLES: &[&str] = &[
    "model_state",
    "searches",
    "signals",
    "activity",
    "impressions",
    "saved_searches",
    "blacklist",
    "favourite_tags",
    "dislikes",
    "likes",
    "search_pages",
    "post_tags",
    "post_leads",
    "posts",
    "tags",
];

pub struct Db {
    path: PathBuf,
    main: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let mut conn = connect(path)?;
        migrate(&mut conn)?;
        Ok(Db {
            path: path.to_owned(),
            main: Mutex::new(conn),
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.main
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn restore(&self, from: &Path) -> Result<()> {
        let mut conn = self.conn();
        conn.restore(
            rusqlite::MAIN_DB,
            from,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        migrate(&mut conn)
    }

    pub fn connect(&self) -> Result<Connection> {
        connect(&self.path)
    }

}

fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = DELETE;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 10000;",
    )?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn page_count(rows: i64, page_size: i64) -> i64 {
    ((rows + page_size - 1) / page_size).max(1)
}

pub use foryooru_core::now_ms;

#[cfg(test)]
mod tests {
    use super::*;

    fn has_table(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fresh_databases_get_the_model_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert!(has_table(&conn, "model_state"));
        assert!(has_table(&conn, "post_leads"));
        assert!(!has_table(&conn, "jev_training"));
    }

    #[test]
    fn databases_that_ran_the_old_jev_migration_catch_up() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "CREATE TABLE jev_training (id INTEGER PRIMARY KEY); PRAGMA user_version = 6;",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        assert!(has_table(&conn, "model_state"));
        assert!(!has_table(&conn, "jev_training"));
    }
}
