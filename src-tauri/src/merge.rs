// joins the per-site databases from before into one, once. posts become one row per file by md5, and everything
// that pointed at a site's post id points at that row instead. the old files are only read, and removed once the
// merged file is in place
use crate::booru::Site;
use crate::catalogue::normalized_md5;
use crate::db::{self, Db, SITE_MIGRATIONS};
use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};

pub const DATABASE: &str = "foryooru.db";
const PARTIAL: &str = "foryooru.db.partial";

pub fn open(data_dir: &Path) -> Result<Db> {
    let merged = data_dir.join(DATABASE);
    let old_files: Vec<(Site, PathBuf)> = Site::ALL
        .into_iter()
        .map(|site| (site, data_dir.join(site.database_file())))
        .filter(|(_, path)| path.exists())
        .collect();
    if merged.exists() && is_merged(&merged)? {
        // the app may have stopped between putting the merged file in place and removing the old ones
        for (_, path) in &old_files {
            db::remove_database(path)?;
        }
        return Db::open(&merged);
    }
    // a foryooru.db that isn't merged is from before the per-site files, which never read it again
    db::remove_database(&merged)?;
    if old_files.is_empty() {
        return Db::open(&merged);
    }
    let partial = data_dir.join(PARTIAL);
    let report = merge(&old_files, &partial).map_err(|error| {
        let _ = db::remove_database(&partial);
        Error::Merge(format!(
            "{error}. Nothing was changed, the old databases are still in {}",
            data_dir.display()
        ))
    })?;
    crate::platform::log(&format!("Merged the site databases: {report}"));
    std::fs::rename(&partial, &merged)?;
    for (_, path) in &old_files {
        db::remove_database(path)?;
    }
    Db::open(&merged)
}

fn is_merged(path: &Path) -> Result<bool> {
    let conn = Connection::open(path)?;
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'site_posts')",
        [],
        |row| row.get(0),
    )?)
}

fn schema(site: Site) -> &'static str {
    match site {
        Site::Gelbooru => "gel",
        Site::Danbooru => "dan",
    }
}

#[derive(Default, Debug)]
pub struct Report {
    pub old_posts: usize,
    pub posts: usize,
    pub on_both: usize,
    pub likes: usize,
    pub dislikes: usize,
    pub conflicts: usize,
    pub without_md5: usize,
}

impl std::fmt::Display for Report {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "{} posts from the old files became {}, {} on both sites, {} without an md5. {} likes, {} dislikes, {} posts liked on one site and disliked on the other",
            self.old_posts, self.posts, self.on_both, self.without_md5, self.likes, self.dislikes, self.conflicts
        )
    }
}

pub fn merge(old_files: &[(Site, PathBuf)], target: &Path) -> Result<Report> {
    for (_, path) in old_files {
        let mut old = Connection::open(path)?;
        db::migrate_with(&mut old, SITE_MIGRATIONS)?;
        old.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    db::remove_database(target)?;
    let mut conn = Connection::open(target)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    db::migrate(&mut conn)?;
    // Danbooru first, so its rows win where both files have something
    let mut sites: Vec<Site> = old_files.iter().map(|(site, _)| *site).collect();
    sites.sort_by_key(|site| *site != Site::Danbooru);
    for (site, path) in old_files {
        conn.execute("ATTACH DATABASE ?1 AS ?2", params![path.to_string_lossy(), schema(*site)])?;
    }
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TEMP TABLE merged (site TEXT NOT NULL, old_id INTEGER NOT NULL, post_id INTEGER NOT NULL,
             PRIMARY KEY (site, old_id));",
    )?;
    let mut report = Report::default();
    for site in &sites {
        report.old_posts += merge_posts(&tx, *site)?;
        merge_site(&tx, *site)?;
    }
    merge_logs(&tx, &sites)?;
    merge_saved_searches(&tx, &sites)?;
    report.conflicts = settle_conflicts(&tx)?;
    // post_tags has no index by tag, so every count comes from one pass over it
    tx.execute(
        "UPDATE tags SET post_count = counted.posts
         FROM (SELECT tag_id, COUNT(DISTINCT post_id) AS posts FROM post_tags GROUP BY tag_id) AS counted
         WHERE tags.id = counted.tag_id",
        [],
    )?;
    check(&tx, &sites)?;
    let count = |sql: &str| -> Result<usize> { Ok(tx.query_row(sql, [], |row| row.get::<_, i64>(0))? as usize) };
    report.posts = count("SELECT COUNT(*) FROM posts")?;
    report.on_both = count("SELECT COUNT(*) FROM (SELECT post_id FROM site_posts GROUP BY post_id HAVING COUNT(*) > 1)")?;
    report.likes = count("SELECT COUNT(*) FROM likes")?;
    report.dislikes = count("SELECT COUNT(*) FROM dislikes")?;
    report.without_md5 = count("SELECT COUNT(*) FROM posts WHERE md5 IS NULL")?;
    tx.commit()?;
    for site in &sites {
        conn.execute("DETACH DATABASE ?1", [schema(*site)])?;
    }
    conn.close().map_err(|(_, error)| error)?;
    Ok(report)
}

// gives every old post its local id, matching files the other site already brought by md5
fn merge_posts(tx: &Transaction, site: Site) -> Result<usize> {
    let schema = schema(site);
    let mut old_posts = tx.prepare(&format!(
        "SELECT id, md5, width, height, posted_at FROM {schema}.posts ORDER BY seq"
    ))?;
    let mut by_md5 = tx.prepare("SELECT id FROM posts WHERE md5 = ?1")?;
    let mut new_post = tx.prepare(
        "INSERT INTO posts (md5, seq, width, height, posted_at)
         VALUES (?1, (SELECT IFNULL(MAX(seq), 0) + 1 FROM posts), ?2, ?3, ?4) RETURNING id",
    )?;
    let mut earlier = tx.prepare("UPDATE posts SET posted_at = MIN(COALESCE(posted_at, ?2), ?2) WHERE id = ?1")?;
    let mut map = tx.prepare("INSERT INTO merged (site, old_id, post_id) VALUES (?1, ?2, ?3)")?;
    let rows = old_posts
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (old_id, md5, width, height, posted_at) in &rows {
        let md5 = normalized_md5(md5);
        let existing: Option<i64> = match &md5 {
            Some(md5) => by_md5.query_row([md5], |row| row.get(0)).optional()?,
            None => None,
        };
        let post_id = match existing {
            Some(id) => {
                if let Some(posted_at) = posted_at {
                    earlier.execute(params![id, posted_at])?;
                }
                id
            }
            None => new_post.query_row(params![md5, width, height, posted_at], |row| row.get(0))?,
        };
        map.execute(params![site.key(), old_id, post_id])?;
    }
    Ok(rows.len())
}

// everything else from one site's file that belongs to its posts or is shared by name
fn merge_site(tx: &Transaction, site: Site) -> Result<()> {
    let schema = schema(site);
    let key = site.key();
    let mapped = format!("JOIN merged m ON m.site = '{key}' AND m.old_id = o.post_id");
    let checked = match site {
        Site::Danbooru => "danbooru_checked",
        Site::Gelbooru => "gelbooru_checked",
    };
    // Gelbooru's details only fill tags Danbooru didn't describe
    let unclaimed = match site {
        Site::Danbooru => "",
        Site::Gelbooru => "AND tags.count_site IS NULL",
    };
    tx.execute_batch(&format!(
        "INSERT INTO site_posts (site, site_id, post_id, rating, score, preview_url, preview_width, preview_height,
             sample_url, file_url, posted_at, partial, media_resolved, parent_id, has_children)
         SELECT '{key}', o.id, m.post_id, o.rating, o.score, o.preview_url, o.preview_width, o.preview_height,
             o.sample_url, o.file_url, o.posted_at, o.partial, o.media_resolved, o.parent_id, o.has_children
         FROM {schema}.posts o JOIN merged m ON m.site = '{key}' AND m.old_id = o.id
         WHERE true ORDER BY o.seq
         ON CONFLICT DO NOTHING;

         INSERT INTO tags (name) SELECT name FROM {schema}.tags WHERE true ON CONFLICT (name) DO NOTHING;
         UPDATE tags SET type = o.type, site_count = o.site_count, count_site = '{key}'
             FROM {schema}.tags o
             WHERE o.name = tags.name AND o.details_fetched = 1 AND o.type IS NOT NULL {unclaimed};
         UPDATE tags SET {checked} = 1 FROM {schema}.tags o WHERE o.name = tags.name AND o.details_fetched = 1;

         INSERT OR IGNORE INTO post_tags (post_id, site, tag_id)
         SELECT m.post_id, '{key}', t.id FROM {schema}.post_tags o {mapped}
             JOIN {schema}.tags ot ON ot.id = o.tag_id JOIN tags t ON t.name = ot.name;

         INSERT INTO likes (post_id, sort_at, liked_at)
         SELECT m.post_id, o.sort_at, o.liked_at FROM {schema}.likes o {mapped} WHERE true
         ON CONFLICT (post_id) DO UPDATE SET sort_at = MAX(sort_at, excluded.sort_at),
             liked_at = CASE WHEN liked_at IS NULL OR excluded.liked_at IS NULL THEN COALESCE(liked_at, excluded.liked_at)
                             ELSE MIN(liked_at, excluded.liked_at) END;

         INSERT INTO dislikes (post_id, disliked_at)
         SELECT m.post_id, o.disliked_at FROM {schema}.dislikes o {mapped} WHERE true
         ON CONFLICT (post_id) DO UPDATE SET disliked_at = MIN(disliked_at, excluded.disliked_at);

         INSERT INTO impressions (post_id, views, feed_views, dwell_ms, quick_skips, long_views, opens, first_seen, last_seen)
         SELECT m.post_id, o.views, o.feed_views, o.dwell_ms, o.quick_skips, o.long_views, o.opens, o.first_seen, o.last_seen
         FROM {schema}.impressions o {mapped} WHERE true
         ON CONFLICT (post_id) DO UPDATE SET views = views + excluded.views, feed_views = feed_views + excluded.feed_views,
             dwell_ms = dwell_ms + excluded.dwell_ms, quick_skips = quick_skips + excluded.quick_skips,
             long_views = long_views + excluded.long_views, opens = opens + excluded.opens,
             first_seen = MIN(first_seen, excluded.first_seen), last_seen = MAX(last_seen, excluded.last_seen);

         INSERT OR IGNORE INTO post_leads (post_id, tag) SELECT m.post_id, o.tag FROM {schema}.post_leads o {mapped};

         INSERT OR IGNORE INTO site_files (post_id, site, present, checked_at)
         SELECT m.post_id, o.site, o.present, o.checked_at FROM {schema}.site_files o {mapped}
         WHERE NOT EXISTS (SELECT 1 FROM site_posts sp WHERE sp.post_id = m.post_id AND sp.site = o.site);

         INSERT INTO favourite_tags (name, created_at) SELECT name, created_at FROM {schema}.favourite_tags WHERE true
         ON CONFLICT (name) DO UPDATE SET created_at = MIN(created_at, excluded.created_at);
         INSERT INTO blacklist (name, created_at) SELECT name, created_at FROM {schema}.blacklist WHERE true
         ON CONFLICT (name) DO UPDATE SET created_at = MIN(created_at, excluded.created_at);
         INSERT INTO followed_artists (name, created_at) SELECT name, created_at FROM {schema}.followed_artists WHERE true
         ON CONFLICT (name) DO UPDATE SET created_at = MIN(created_at, excluded.created_at);

         INSERT OR REPLACE INTO meta (key, value)
         SELECT 'popular_seeded:{key}', value FROM {schema}.meta WHERE key = 'popular_seeded';
         INSERT OR REPLACE INTO meta (key, value)
         SELECT 'site_total:{key}', MAX(total) FROM {schema}.search_pages WHERE query = '' HAVING MAX(total) IS NOT NULL;"
    ))?;
    Ok(())
}

// activity, signals and searches from both files, in time order
fn merge_logs(tx: &Transaction, sites: &[Site]) -> Result<()> {
    let union = |select: &dyn Fn(Site) -> String| {
        sites
            .iter()
            .map(|site| select(*site))
            .collect::<Vec<_>>()
            .join(" UNION ALL ")
    };
    let activity = union(&|site| {
        format!(
            "SELECT o.at, o.kind, m.post_id, o.source, o.dwell_ms, o.quick_skip, o.long_look,
                    CASE WHEN o.kind = 'query' THEN '{key}' END, o.endpoint, o.query, o.page, o.status, o.error, o.ms
             FROM {schema}.activity o LEFT JOIN merged m ON m.site = '{key}' AND m.old_id = o.post_id",
            key = site.key(),
            schema = schema(site)
        )
    });
    let signals = union(&|site| {
        format!(
            "SELECT o.at, m.post_id, o.kind, o.tag FROM {schema}.signals o
             JOIN merged m ON m.site = '{key}' AND m.old_id = o.post_id",
            key = site.key(),
            schema = schema(site)
        )
    });
    let searches = union(&|site| format!("SELECT at, query FROM {}.searches", schema(site)));
    tx.execute_batch(&format!(
        "INSERT INTO activity (at, kind, post_id, source, dwell_ms, quick_skip, long_look, site, endpoint, query, page,
             status, error, ms)
         SELECT * FROM ({activity}) ORDER BY 1;
         INSERT INTO signals (at, post_id, kind, tag) SELECT * FROM ({signals}) ORDER BY 1;
         INSERT INTO searches (at, query) SELECT * FROM ({searches}) ORDER BY 1;"
    ))?;
    Ok(())
}

// saved searches are shared by text, and a similar:<id> in one points at its own site's post
fn merge_saved_searches(tx: &Transaction, sites: &[Site]) -> Result<()> {
    let mut local_id = tx.prepare("SELECT post_id FROM merged WHERE site = ?1 AND old_id = ?2")?;
    let mut save = tx.prepare(
        "INSERT INTO saved_searches (query, created_at) VALUES (?1, ?2)
         ON CONFLICT (query) DO UPDATE SET created_at = MIN(created_at, excluded.created_at)",
    )?;
    for site in sites {
        let rows: Vec<(String, i64)> = tx
            .prepare(&format!("SELECT query, created_at FROM {}.saved_searches", schema(*site)))?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (query, created_at) in rows {
            let mut words = Vec::new();
            for word in query.split_whitespace() {
                let old_id = word.strip_prefix("similar:").and_then(|id| id.parse::<i64>().ok());
                let mapped: Option<i64> = match old_id {
                    Some(old_id) => local_id.query_row(params![site.key(), old_id], |row| row.get(0)).optional()?,
                    None => None,
                };
                words.push(mapped.map_or_else(|| word.to_owned(), |id| format!("similar:{id}")));
            }
            save.execute(params![words.join(" "), created_at])?;
        }
    }
    Ok(())
}

// a post liked in one file and disliked in the other keeps whichever happened last
fn settle_conflicts(tx: &Transaction) -> Result<usize> {
    let conflicts = tx.execute(
        "DELETE FROM dislikes WHERE post_id IN (
             SELECT l.post_id FROM likes l JOIN dislikes d ON d.post_id = l.post_id
             WHERE COALESCE(l.liked_at, l.sort_at) >= d.disliked_at)",
        [],
    )?;
    let disliked_later = tx.execute("DELETE FROM likes WHERE post_id IN (SELECT post_id FROM dislikes)", [])?;
    Ok(conflicts + disliked_later)
}

fn check(tx: &Transaction, sites: &[Site]) -> Result<()> {
    let fail = |what: String| Err(Error::Merge(what));
    for site in sites {
        let schema = schema(*site);
        let key = site.key();
        for table in ["likes", "dislikes", "impressions"] {
            let unmapped: i64 = tx.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.{table} o
                     LEFT JOIN merged m ON m.site = '{key}' AND m.old_id = o.post_id WHERE m.post_id IS NULL"
                ),
                [],
                |row| row.get(0),
            )?;
            if unmapped > 0 {
                return fail(format!("{unmapped} {key} {table} rows lost their post"));
            }
        }
        let missing: i64 = tx.query_row(
            &format!(
                "SELECT COUNT(*) FROM {schema}.posts o JOIN merged m ON m.site = '{key}' AND m.old_id = o.id
                 WHERE NOT EXISTS (SELECT 1 FROM site_posts sp WHERE sp.post_id = m.post_id AND sp.site = '{key}')"
            ),
            [],
            |row| row.get(0),
        )?;
        if missing > 0 {
            return fail(format!("{missing} {key} posts have no site row"));
        }
    }
    let broken: Option<String> = tx
        .query_row("SELECT \"table\" FROM pragma_foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if let Some(table) = broken {
        return fail(format!("{table} points at rows that don't exist"));
    }
    let integrity: String = tx.query_row("PRAGMA main.integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return fail(format!("the merged database is damaged: {integrity}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booru::RawPost;
    use crate::search::Rating;

    const SHARED: &str = "0123456789abcdef0123456789abcdef";

    // a per-site file as the app wrote it before the merge
    fn old_file(dir: &Path, site: Site, setup: &str) -> (Site, PathBuf) {
        let path = dir.join(site.database_file());
        let mut conn = Connection::open(&path).unwrap();
        db::migrate_with(&mut conn, SITE_MIGRATIONS).unwrap();
        conn.execute_batch(setup).unwrap();
        (site, path)
    }

    fn old_post(id: i64, md5: &str, seq: i64) -> String {
        format!(
            "INSERT INTO posts (id, seq, md5, rating, score, width, height, preview_url, preview_width, preview_height,
                 sample_url, file_url, posted_at) VALUES ({id}, {seq}, '{md5}', 'general', 1, 10, 10, 'p', 1, 1, 's', 'f.png', {id});"
        )
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("foryooru-merge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn two_files(dir: &Path) -> Vec<(Site, PathBuf)> {
        let danbooru = format!(
            "{}{}
             INSERT INTO tags (id, name, type, site_count, details_fetched) VALUES (1, 'cat', 0, 900, 1), (2, 'some_artist', 1, 5, 1);
             INSERT INTO post_tags VALUES (5, 1), (5, 2), (6, 1);
             INSERT INTO likes (post_id, sort_at, liked_at) VALUES (5, 100, 100);
             INSERT INTO dislikes (post_id, disliked_at) VALUES (6, 300);
             INSERT INTO impressions (post_id, views, first_seen, last_seen) VALUES (5, 2, 10, 20);
             INSERT INTO activity (at, kind, post_id) VALUES (50, 'view', 5);
             INSERT INTO blacklist VALUES ('gore', 7);
             INSERT INTO saved_searches VALUES ('similar:5 cat', 1);",
            old_post(5, &SHARED.to_uppercase(), 1),
            old_post(6, "", 2),
        );
        let gelbooru = format!(
            "{}{}
             INSERT INTO tags (id, name, type, site_count, details_fetched) VALUES (1, 'cat', 0, 4000, 1), (2, 'some_artist', 0, 9, 1),
                 (3, 'cloud', 0, 80, 1);
             INSERT INTO post_tags VALUES (900, 1), (900, 3), (901, 3);
             INSERT INTO likes (post_id, sort_at, liked_at) VALUES (900, 200, 150), (901, 400, 400);
             INSERT INTO impressions (post_id, views, first_seen, last_seen) VALUES (900, 3, 5, 30);
             INSERT INTO activity (at, kind, post_id) VALUES (40, 'like', 900);
             INSERT INTO blacklist VALUES ('gore', 3);",
            old_post(900, SHARED, 1),
            old_post(901, "fedcba9876543210fedcba9876543210", 2),
        );
        vec![old_file(dir, Site::Danbooru, &danbooru), old_file(dir, Site::Gelbooru, &gelbooru)]
    }

    fn one(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn the_same_file_on_both_sites_becomes_one_post_with_everything_pointing_at_it() {
        let dir = scratch("both");
        let files = two_files(&dir);
        let report = merge(&files, &dir.join(PARTIAL)).unwrap();
        assert_eq!((report.old_posts, report.posts, report.on_both, report.without_md5), (4, 3, 1, 1));
        let conn = Connection::open(dir.join(PARTIAL)).unwrap();
        let shared = one(&conn, &format!("SELECT id FROM posts WHERE md5 = '{SHARED}'"));
        assert_eq!(one(&conn, &format!("SELECT COUNT(*) FROM site_posts WHERE post_id = {shared}")), 2);
        // liked on both: the later sort and the earlier like time
        let (sort_at, liked_at): (i64, i64) = conn
            .query_row(&format!("SELECT sort_at, liked_at FROM likes WHERE post_id = {shared}"), [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!((sort_at, liked_at), (200, 100));
        assert_eq!(one(&conn, "SELECT COUNT(*) FROM likes"), 2);
        assert_eq!(one(&conn, &format!("SELECT views FROM impressions WHERE post_id = {shared}")), 5);
        assert_eq!(one(&conn, "SELECT COUNT(*) FROM activity"), 2);
        assert_eq!(one(&conn, "SELECT created_at FROM blacklist WHERE name = 'gore'"), 3);
        // Danbooru's type wins, and a tag on both sites' copies counts once
        assert_eq!(one(&conn, "SELECT type FROM tags WHERE name = 'some_artist'"), 1);
        assert_eq!(one(&conn, "SELECT post_count FROM tags WHERE name = 'cat'"), 2);
        let query: String = conn.query_row("SELECT query FROM saved_searches", [], |row| row.get(0)).unwrap();
        assert_eq!(query, format!("similar:{shared} cat"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_later_of_a_like_and_a_dislike_wins() {
        let dir = scratch("conflict");
        let files = vec![
            old_file(&dir, Site::Danbooru, &format!("{} INSERT INTO dislikes VALUES (1, 500);", old_post(1, SHARED, 1))),
            old_file(&dir, Site::Gelbooru, &format!("{} INSERT INTO likes VALUES (2, 100, 100);", old_post(2, SHARED, 1))),
        ];
        let report = merge(&files, &dir.join(PARTIAL)).unwrap();
        assert_eq!((report.likes, report.dislikes, report.conflicts), (0, 1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_merges_once_and_removes_the_old_files() {
        let dir = scratch("open");
        let files = two_files(&dir);
        let db = open(&dir).unwrap();
        assert_eq!(one(&db.conn(), "SELECT COUNT(*) FROM posts"), 3);
        drop(db);
        assert!(files.iter().all(|(_, path)| !path.exists()));
        assert!(!dir.join(PARTIAL).exists());
        // a second start just opens it
        let db = open(&dir).unwrap();
        assert_eq!(one(&db.conn(), "SELECT COUNT(*) FROM likes"), 2);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_merge_stopped_halfway_starts_over() {
        let dir = scratch("halfway");
        two_files(&dir);
        std::fs::write(dir.join(PARTIAL), b"half a database").unwrap();
        std::fs::write(dir.join(format!("{PARTIAL}-journal")), b"a journal from the stopped merge").unwrap();
        let db = open(&dir).unwrap();
        assert_eq!(one(&db.conn(), "SELECT COUNT(*) FROM posts"), 3);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_merge_leaves_the_old_files_alone() {
        let dir = scratch("failed");
        let (_, path) = old_file(&dir, Site::Danbooru, &old_post(1, SHARED, 1));
        let damaged = Connection::open(&path).unwrap();
        damaged.execute_batch("DROP TABLE likes;").unwrap();
        drop(damaged);
        assert!(open(&dir).is_err());
        assert!(path.exists());
        assert!(!dir.join(DATABASE).exists());
        assert!(!dir.join(PARTIAL).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // merges copies of this machine's real databases and prints the report. the originals are only read:
    // cargo test --lib merge::tests::real_databases -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_databases() {
        let data_dir = PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.j.foryooru");
        let dir = scratch("real");
        let mut copies = Vec::new();
        for site in Site::ALL {
            let source = data_dir.join(site.database_file());
            if !source.exists() {
                continue;
            }
            for (from, to) in db::database_files(&source).into_iter().zip(db::database_files(&dir.join(site.database_file()))) {
                if from.exists() && !from.to_string_lossy().ends_with("-shm") {
                    std::fs::copy(from, to).unwrap();
                }
            }
            copies.push((site, dir.join(site.database_file())));
        }
        let started = std::time::Instant::now();
        let report = merge(&copies, &dir.join(PARTIAL)).unwrap();
        println!("{report} in {:?}", started.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merged_posts_store_like_any_other() {
        let dir = scratch("store");
        let files = two_files(&dir);
        let db = open(&dir).unwrap();
        let raw = RawPost {
            rating: Rating::General,
            ..crate::catalogue::tests::raw(5, SHARED, &["cat", "dog"])
        };
        let stored = crate::catalogue::store_posts(&mut db.conn(), Site::Danbooru, &[raw], false).unwrap();
        let shared = one(&db.conn(), &format!("SELECT id FROM posts WHERE md5 = '{SHARED}'"));
        assert_eq!(stored.ids, [shared]);
        drop(files);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
