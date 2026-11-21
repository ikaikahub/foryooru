use crate::db::Db;
use crate::error::Result;
use crate::search::Rating;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use foryooru_core::corpus::*;
use foryooru_core::kinds::{TagKind, media_of};

const FALLBACK_SITE_TOTAL: f64 = 14_000_000.0;
// tag details arrive after nearly every page, so their corrected weights are batched into one rebuild this often
const REBUILD_INTERVAL: Duration = Duration::from_secs(120);

#[derive(Clone)]
struct Loaded {
    corpus: Arc<Corpus>,
    max_seq: i64,
}

fn load_tags(conn: &Connection, corpus: &mut Corpus) -> Result<()> {
    let site_total = conn
        .query_row(
            "SELECT MAX(total) FROM search_pages WHERE query = ''",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )?
        .map_or(FALLBACK_SITE_TOTAL, |total| total as f64);
    let mut statement = conn.prepare("SELECT id, name, type, site_count FROM tags")?;
    let tags = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                TagInfo {
                    name: row.get(1)?,
                    kind: row.get::<_, Option<i64>>(2)?.and_then(TagKind::from_code),
                    count: row.get(3)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    corpus.set_tags(site_total, tags);
    Ok(())
}

fn append(conn: &Connection, corpus: &mut Corpus, after_seq: i64) -> Result<i64> {
    let mut posts = conn.prepare(
        "SELECT id, rating, score, md5, seq, posted_at, file_url FROM posts WHERE seq > ?1 ORDER BY seq",
    )?;
    let rows: Vec<(NewPost, i64)> = posts
        .query_map([after_seq], |row| {
            let rating: String = row.get(1)?;
            let file_url: String = row.get(6)?;
            Ok((
                NewPost {
                    id: row.get(0)?,
                    rating: Rating::parse(&rating).unwrap_or(Rating::Explicit),
                    score: row.get(2)?,
                    md5: row.get(3)?,
                    posted_at: row.get(5)?,
                    media: media_of(&file_url),
                    tags: Vec::new(),
                },
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let Some(last_seq) = rows.last().map(|(_, seq)| *seq) else {
        return Ok(after_seq);
    };
    let started = crate::timing::now();
    load_tags(conn, corpus)?;
    crate::timing::record("corpus.tags", started.elapsed(), format!("{} tags", corpus.tags.len()));
    let started = crate::timing::now();
    let mut links: HashMap<i64, Vec<i64>> = HashMap::with_capacity(rows.len());
    // a full build wants every link, and skipping the join halves the read
    let (sql, params): (&str, &[&dyn rusqlite::ToSql]) = if after_seq == 0 {
        ("SELECT post_id, tag_id FROM post_tags", &[])
    } else {
        (
            "SELECT pt.post_id, pt.tag_id FROM post_tags pt JOIN posts p ON p.id = pt.post_id WHERE p.seq > ?1",
            &[&after_seq],
        )
    };
    let mut statement = conn.prepare(sql)?;
    for link in statement.query_map(params, |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (post, tag) = link?;
        links.entry(post).or_default().push(tag);
    }
    let new = rows
        .into_iter()
        .map(|(mut post, _)| {
            post.tags = links.remove(&post.id).unwrap_or_default();
            post
        })
        .collect::<Vec<_>>();
    crate::timing::record("corpus.links", started.elapsed(), format!("{} posts", new.len()));
    let started = crate::timing::now();
    corpus.append(new);
    crate::timing::record("corpus.append", started.elapsed(), format!("{} posts stored", corpus.size()));
    Ok(last_seq)
}

fn build(conn: &Connection) -> Result<Loaded> {
    let mut corpus = Corpus::new();
    let max_seq = append(conn, &mut corpus, 0)?;
    Ok(Loaded {
        corpus: Arc::new(corpus),
        max_seq,
    })
}

#[derive(Default)]
pub struct CorpusCache {
    current: Mutex<Option<Loaded>>,
    stale: AtomicBool,
    rebuilding: Arc<AtomicBool>,
    last_build: Mutex<Option<Instant>>,
}

impl CorpusCache {
    pub fn mark_stale(&self) {
        self.stale.store(true, Ordering::SeqCst);
    }

    pub fn clear(&self) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub fn get(self: &Arc<Self>, db: &Arc<Db>) -> Result<Arc<Corpus>> {
        let conn = db.connect()?;
        let current = {
            let mut current = self
                .current
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let latest_seq: i64 =
                conn.query_row("SELECT IFNULL(MAX(seq), 0) FROM posts", [], |row| {
                    row.get(0)
                })?;
            match current.as_mut() {
                None => {
                    *current = Some(build(&conn)?);
                    *self
                        .last_build
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
                }
                Some(loaded) if latest_seq > loaded.max_seq => {
                    loaded.max_seq =
                        append(&conn, Arc::make_mut(&mut loaded.corpus), loaded.max_seq)?
                }
                Some(_) => {}
            }
            current
                .as_ref()
                .map_or_else(|| Arc::new(Corpus::new()), |loaded| loaded.corpus.clone())
        };
        let rebuild_due = {
            let mut last_build = self
                .last_build
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let due = self.stale.load(Ordering::SeqCst)
                && last_build.is_none_or(|built| built.elapsed() >= REBUILD_INTERVAL)
                && !self.rebuilding.swap(true, Ordering::SeqCst);
            if due {
                *last_build = Some(Instant::now());
            }
            due
        };
        if rebuild_due {
            self.stale.store(false, Ordering::SeqCst);
            let (cache, db) = (self.clone(), db.clone());
            // the rebuild is plain CPU and database work, which would stall an async worker thread
            tauri::async_runtime::spawn_blocking(move || {
                let started = crate::timing::now();
                match db.connect().and_then(|conn| build(&conn)) {
                    Ok(fresh) => {
                        *cache
                            .current
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(fresh)
                    }
                    Err(error) => eprintln!("Rebuilding the post index failed: {error}"),
                }
                crate::timing::record("corpus.rebuild", started.elapsed(), "");
                cache.rebuilding.store(false, Ordering::SeqCst);
            });
        }
        Ok(current)
    }
}
