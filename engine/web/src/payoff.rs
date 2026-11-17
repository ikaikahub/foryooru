use crate::error::Result;
use rusqlite::{Connection, Row, params};
use serde::Serialize;
use std::collections::HashMap;

const REACTION_WEIGHT: f64 = 3.0;
const ENGAGED_WEIGHT: f64 = 2.0;
const PRIOR_VIEWS: f64 = 10.0;
const MIN_FACTOR: f64 = 0.25;
const MAX_FACTOR: f64 = 4.0;
const MIN_LISTED_VIEWS: f64 = 10.0;
const LISTED_TAGS: usize = 5;

pub fn credit(conn: &Connection, tag: &str, posts: &[i64]) -> Result<()> {
    let mut insert =
        conn.prepare_cached("INSERT OR IGNORE INTO post_leads (post_id, tag) VALUES (?1, ?2)")?;
    for post in posts {
        insert.execute(params![post, tag])?;
    }
    Ok(())
}

#[derive(Default, Clone, Copy)]
struct Tally {
    good: f64,
    bad: f64,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.good += other.good;
        self.bad += other.bad;
    }

    fn judged(self) -> f64 {
        self.good + self.bad
    }
}

#[derive(Default)]
struct View {
    liked: bool,
    disliked: bool,
    engaged: bool,
    long_look: bool,
    quick_skip: bool,
}

impl View {
    fn tally(&self) -> Tally {
        let good = |weight: f64| Tally {
            good: weight,
            bad: 0.0,
        };
        let bad = |weight: f64| Tally {
            good: 0.0,
            bad: weight,
        };
        if self.liked {
            good(REACTION_WEIGHT)
        } else if self.disliked {
            bad(REACTION_WEIGHT)
        } else if self.engaged {
            good(ENGAGED_WEIGHT)
        } else if self.long_look {
            good(1.0)
        } else if self.quick_skip {
            bad(1.0)
        } else {
            Tally::default()
        }
    }
}

const VIEW_OUTCOME: &str = "a.long_look, a.quick_skip,
    EXISTS (SELECT 1 FROM likes l WHERE l.post_id = a.post_id),
    EXISTS (SELECT 1 FROM dislikes d WHERE d.post_id = a.post_id),
    EXISTS (SELECT 1 FROM signals s WHERE s.post_id = a.post_id)";

fn view_at(row: &Row, offset: usize) -> rusqlite::Result<View> {
    Ok(View {
        long_look: row.get(offset)?,
        quick_skip: row.get(offset + 1)?,
        liked: row.get(offset + 2)?,
        disliked: row.get(offset + 3)?,
        engaged: row.get(offset + 4)?,
    })
}

fn lead_tallies(conn: &Connection) -> Result<HashMap<String, Tally>> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT pl.tag, {VIEW_OUTCOME}
         FROM post_leads pl JOIN activity a ON a.post_id = pl.post_id
         WHERE a.kind = 'view' AND a.source = 'feed'"
    ))?;
    let mut tallies: HashMap<String, Tally> = HashMap::new();
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, view_at(row, 1)?.tally()))
    })?;
    for row in rows {
        let (tag, tally) = row?;
        tallies.entry(tag).or_default().add(tally);
    }
    Ok(tallies)
}

fn all_views_tally(conn: &Connection) -> Result<Tally> {
    let mut statement = conn.prepare_cached(&format!(
        "SELECT {VIEW_OUTCOME} FROM activity a WHERE a.kind = 'view' AND a.source = 'feed'"
    ))?;
    let mut overall = Tally::default();
    for tally in statement.query_map([], |row| Ok(view_at(row, 0)?.tally()))? {
        overall.add(tally?);
    }
    Ok(overall)
}

fn summed(tallies: &HashMap<String, Tally>) -> Tally {
    let mut overall = Tally::default();
    for tally in tallies.values() {
        overall.add(*tally);
    }
    overall
}

fn hit_rate(tally: Tally) -> Option<f64> {
    (tally.judged() > 0.0).then(|| tally.good / tally.judged())
}

pub use foryooru_core::payoff::Payoff;

pub fn read(conn: &Connection) -> Result<Payoff> {
    Ok(from_tallies(lead_tallies(conn)?))
}

fn from_tallies(tallies: HashMap<String, Tally>) -> Payoff {
    let overall = summed(&tallies);
    if overall.good <= 0.0 || overall.judged() <= 0.0 {
        return Payoff::default();
    }
    let rate = overall.good / overall.judged();
    Payoff(
        tallies
            .into_iter()
            .map(|(tag, tally)| {
                let smoothed = (tally.good + PRIOR_VIEWS * rate) / (tally.judged() + PRIOR_VIEWS);
                (tag, (smoothed / rate).clamp(MIN_FACTOR, MAX_FACTOR))
            })
            .collect(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeadTag {
    tag: String,
    views: f64,
    factor: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchStats {
    fetched: i64,
    judged: f64,
    fetched_rate: Option<f64>,
    overall_rate: Option<f64>,
    best: Vec<LeadTag>,
    worst: Vec<LeadTag>,
}

impl FetchStats {
    pub fn read(conn: &Connection) -> Result<FetchStats> {
        let tallies = lead_tallies(conn)?;
        let fetched_total = summed(&tallies);
        let payoff = from_tallies(tallies.clone());
        let (mut best, mut worst): (Vec<LeadTag>, Vec<LeadTag>) = tallies
            .into_iter()
            .filter(|(_, tally)| tally.judged() >= MIN_LISTED_VIEWS)
            .map(|(tag, tally)| LeadTag {
                factor: payoff.factor(&tag),
                views: tally.judged(),
                tag,
            })
            .filter(|lead| lead.factor != 1.0)
            .partition(|lead| lead.factor > 1.0);
        best.sort_by(|a, b| b.factor.total_cmp(&a.factor));
        best.truncate(LISTED_TAGS);
        worst.sort_by(|a, b| a.factor.total_cmp(&b.factor));
        worst.truncate(LISTED_TAGS);
        Ok(FetchStats {
            fetched: conn.query_row("SELECT COUNT(*) FROM post_leads", [], |row| row.get(0))?,
            judged: fetched_total.judged(),
            fetched_rate: hit_rate(fetched_total),
            overall_rate: hit_rate(all_views_tally(conn)?),
            best,
            worst,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tallies(entries: &[(&str, f64, f64)]) -> Payoff {
        from_tallies(
            entries
                .iter()
                .map(|(tag, good, bad)| {
                    (
                        tag.to_string(),
                        Tally {
                            good: *good,
                            bad: *bad,
                        },
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn tags_that_bring_good_posts_rise_and_duds_fall() {
        let payoff = tallies(&[
            ("hit", 30.0, 10.0),
            ("dud", 2.0, 40.0),
            ("even", 20.0, 20.0),
        ]);
        assert!(payoff.factor("hit") > 1.0);
        assert!(payoff.factor("dud") < 1.0);
        assert_eq!(payoff.factor("unseen"), 1.0);
    }

    #[test]
    fn little_evidence_stays_near_average() {
        let payoff = tallies(&[("lucky", 1.0, 0.0), ("plenty", 50.0, 50.0)]);
        assert!(payoff.factor("lucky") < 1.2);
    }

    #[test]
    fn reactions_outweigh_signals_and_signals_outweigh_views() {
        let liked = View {
            liked: true,
            quick_skip: true,
            ..View::default()
        };
        assert_eq!(liked.tally().good, REACTION_WEIGHT);
        let disliked = View {
            disliked: true,
            long_look: true,
            ..View::default()
        };
        assert_eq!(disliked.tally().bad, REACTION_WEIGHT);
        let shared = View {
            engaged: true,
            quick_skip: true,
            ..View::default()
        };
        assert_eq!(shared.tally().good, ENGAGED_WEIGHT);
        assert_eq!(View::default().tally().judged(), 0.0);
    }
}
