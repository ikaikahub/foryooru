use crate::activity;
use crate::db::now_ms;
use crate::engine::implicit_score;
use crate::error::Result;
pub use foryooru_core::activity::Source;
pub use foryooru_core::signals::Signals;
use rusqlite::{Connection, params};
use serde::Deserialize;
use std::collections::HashSet;

const MAX_DWELL_MS: i64 = 60_000;
const QUICK_PCT: f64 = 0.20;
const QUICK_CAP_MS: i64 = 1500;
const MIN_JUDGED_MS: i64 = 400;
const LONG_PCT: f64 = 0.80;
const LONG_FLOOR_MS: i64 = 2000;
const DEFAULT_QUICK_MS: i64 = 600;
const DEFAULT_LONG_MS: i64 = 3000;
const SAMPLE: i64 = 300;
const MIN_SAMPLE: usize = 30;
const ACTIVE_DAYS: f64 = 14.0;

#[derive(Deserialize)]
pub struct Impression {
    post: i64,
    source: Source,
    #[serde(default)]
    ms: i64,
    #[serde(default)]
    opened: bool,
}

fn thresholds(conn: &Connection, source: Source) -> Result<(i64, i64)> {
    let mut statement = conn.prepare_cached(
        "SELECT dwell_ms FROM activity WHERE kind = 'view' AND source = ?1 AND dwell_ms >= ?2 ORDER BY at DESC LIMIT ?3",
    )?;
    let mut sample: Vec<i64> = statement
        .query_map(params![source.name(), MIN_JUDGED_MS, SAMPLE], |row| {
            row.get(0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    if sample.len() < MIN_SAMPLE {
        return Ok((DEFAULT_QUICK_MS, DEFAULT_LONG_MS));
    }
    sample.sort_unstable();
    let at = |pct: f64| sample[(pct * (sample.len() - 1) as f64) as usize];
    Ok((
        at(QUICK_PCT).min(QUICK_CAP_MS),
        at(LONG_PCT).max(LONG_FLOOR_MS),
    ))
}

// quick skips and long looks are still recorded so the activity page can show them, but nothing ranks by them
pub fn record(conn: &mut Connection, impressions: &[Impression]) -> Result<()> {
    let feed_thresholds = thresholds(conn, Source::Feed)?;
    let viewer_thresholds = thresholds(conn, Source::Viewer)?;
    let now = now_ms();
    let tx = conn.transaction()?;
    {
        let mut upsert = tx.prepare_cached(
            "INSERT INTO impressions (post_id, views, feed_views, dwell_ms, quick_skips, long_views, opens, first_seen, last_seen)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8 WHERE EXISTS (SELECT 1 FROM posts WHERE id = ?1)
             ON CONFLICT (post_id) DO UPDATE SET views = views + excluded.views, feed_views = feed_views + excluded.feed_views,
                dwell_ms = dwell_ms + excluded.dwell_ms, quick_skips = quick_skips + excluded.quick_skips,
                long_views = long_views + excluded.long_views, opens = opens + excluded.opens, last_seen = excluded.last_seen",
        )?;
        for impression in impressions {
            let feed = impression.source == Source::Feed;
            let judged = impression.source.has_dwell();
            let ms = if judged {
                impression.ms.clamp(0, MAX_DWELL_MS)
            } else {
                0
            };
            if judged && ms < MIN_JUDGED_MS && !impression.opened {
                continue;
            }
            let (quick_ms, long_ms) = if feed {
                feed_thresholds
            } else {
                viewer_thresholds
            };
            let quick = judged && ms <= quick_ms;
            let long = judged && !quick && ms >= long_ms;
            let stored = upsert.execute(params![
                impression.post,
                (impression.source != Source::Viewer) as i64,
                feed as i64,
                if feed { ms } else { 0 },
                (feed && quick) as i64,
                (feed && long) as i64,
                impression.opened as i64,
                now,
            ])?;
            if stored > 0 {
                activity::record_view(
                    &tx,
                    impression.post,
                    impression.source,
                    judged.then_some(ms),
                    quick,
                    long,
                )?;
                if impression.opened {
                    activity::record_signal(&tx, impression.post, activity::Signal::Opened, None)?;
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn read_signals(conn: &Connection, reacted: &HashSet<i64>) -> Result<Signals> {
    let now = now_ms();
    let mut statement = conn.prepare_cached(
        "SELECT post_id, views, opens, last_seen FROM impressions",
    )?;
    let mut signals = Signals::default();
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    for row in rows {
        let (id, views, opens, last_seen) = row?;
        let age_days = (now - last_seen) as f64 / 86_400_000.0;
        signals.views.insert(id, views);
        if age_days <= ACTIVE_DAYS {
            signals.active += 1;
        }
        if !reacted.contains(&id) {
            let score = implicit_score(opens, age_days);
            if score != 0.0 {
                signals.scores.insert(id, score);
            }
        }
    }
    Ok(signals)
}
