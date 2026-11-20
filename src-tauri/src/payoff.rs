use crate::error::Result;
use rusqlite::{Connection, Row, params};
use std::collections::HashMap;

const REACTION_WEIGHT: f64 = 3.0;
const ENGAGED_WEIGHT: f64 = 2.0;
const PRIOR_VIEWS: f64 = 10.0;
const MIN_FACTOR: f64 = 0.25;
const MAX_FACTOR: f64 = 4.0;

pub fn credit(conn: &Connection, tag: &str, posts: &[i64]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert =
            tx.prepare_cached("INSERT OR IGNORE INTO post_leads (post_id, tag) VALUES (?1, ?2)")?;
        for post in posts {
            insert.execute(params![post, tag])?;
        }
    }
    tx.commit()?;
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
        } else {
            Tally::default()
        }
    }
}

const VIEW_OUTCOME: &str = "EXISTS (SELECT 1 FROM likes l WHERE l.post_id = a.post_id),
    EXISTS (SELECT 1 FROM dislikes d WHERE d.post_id = a.post_id),
    EXISTS (SELECT 1 FROM signals s WHERE s.post_id = a.post_id)";

fn view_at(row: &Row, offset: usize) -> rusqlite::Result<View> {
    Ok(View {
        liked: row.get(offset)?,
        disliked: row.get(offset + 1)?,
        engaged: row.get(offset + 2)?,
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

fn summed(tallies: &HashMap<String, Tally>) -> Tally {
    let mut overall = Tally::default();
    for tally in tallies.values() {
        overall.add(*tally);
    }
    overall
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
    fn reactions_outweigh_signals_and_plain_views_count_for_nothing() {
        let liked = View {
            liked: true,
            engaged: true,
            ..View::default()
        };
        assert_eq!(liked.tally().good, REACTION_WEIGHT);
        let disliked = View {
            disliked: true,
            ..View::default()
        };
        assert_eq!(disliked.tally().bad, REACTION_WEIGHT);
        let shared = View {
            engaged: true,
            ..View::default()
        };
        assert_eq!(shared.tally().good, ENGAGED_WEIGHT);
        assert_eq!(View::default().tally().judged(), 0.0);
    }
}
