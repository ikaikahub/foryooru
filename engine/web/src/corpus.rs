use crate::db::Db;
use crate::error::Result;
use crate::search::Rating;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub use foryooru_core::corpus::*;
use foryooru_core::kinds::{TagKind, media_of};

const FALLBACK_SITE_TOTAL: f64 = 14_000_000.0;

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
    load_tags(conn, corpus)?;
    let mut links: HashMap<i64, Vec<i64>> = HashMap::with_capacity(rows.len());
    let mut statement = conn.prepare(
        "SELECT pt.post_id, pt.tag_id FROM post_tags pt JOIN posts p ON p.id = pt.post_id WHERE p.seq > ?1",
    )?;
    for link in statement.query_map([after_seq], |row| {
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
        .collect();
    corpus.append(new);
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
                None => *current = Some(build(&conn)?),
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
        if self.stale.load(Ordering::SeqCst) && !self.rebuilding.swap(true, Ordering::SeqCst) {
            self.stale.store(false, Ordering::SeqCst);
            let (cache, db) = (self.clone(), db.clone());
            crate::platform::spawn(async move {
                match db.connect().and_then(|conn| build(&conn)) {
                    Ok(fresh) => {
                        *cache
                            .current
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(fresh)
                    }
                    Err(error) => eprintln!("Rebuilding the post index failed: {error}"),
                }
                cache.rebuilding.store(false, Ordering::SeqCst);
            });
        }
        Ok(current)
    }
}
