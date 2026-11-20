// replays a real account's likes against the dump: the newest likes are hidden, For you ranks from the older ones,
// and the report shows where the hidden likes land, split by whether their artist was liked before
use foryooru_core::corpus::Corpus;
use foryooru_core::kinds::TagKind;
use foryooru_core::recommend::{self, Reaction, UserData};
use foryooru_core::search::Search;
use foryooru_core::signals::Signals;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

// share of the likes, newest first, hidden from the ranking and looked for in it
const HELD_OUT: f64 = 0.2;
const DEPTHS: [usize; 4] = [60, 200, 1000, 2000];
const TOP: usize = 100;

#[derive(Deserialize)]
pub struct Reactions {
    // newest first
    likes: Vec<i64>,
    dislikes: Vec<i64>,
}

fn artists(corpus: &Corpus, id: i64) -> Vec<i64> {
    corpus.row_of(id).map_or_else(Vec::new, |row| {
        corpus
            .tags_of_row(row)
            .into_iter()
            .filter(|tag| corpus.tags.get(tag).is_some_and(|info| info.kind == Some(TagKind::Artist)))
            .collect()
    })
}

fn recall(positions: &HashMap<i64, usize>, posts: &[i64]) -> String {
    if posts.is_empty() {
        return "none".to_owned();
    }
    DEPTHS
        .iter()
        .map(|depth| {
            let found = posts
                .iter()
                .filter(|id| positions.get(id).is_some_and(|position| position < depth))
                .count();
            format!("top {depth}: {:.1}%", found as f64 * 100.0 / posts.len() as f64)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn run(corpus: &Corpus, reactions: &Reactions) -> String {
    let likes: Vec<i64> = reactions.likes.iter().copied().filter(|id| corpus.has(*id)).collect();
    let held_count = (likes.len() as f64 * HELD_OUT).round() as usize;
    let (held, earlier) = likes.split_at(held_count);
    let known_artists: HashSet<i64> = earlier.iter().flat_map(|id| artists(corpus, *id)).collect();
    let (new_artist, known_artist): (Vec<i64>, Vec<i64>) = held
        .iter()
        .partition(|id| !artists(corpus, **id).iter().any(|artist| known_artists.contains(artist)));

    let user = UserData {
        likes: earlier.iter().map(|id| Reaction { id: *id }).collect(),
        dislikes: reactions.dislikes.iter().map(|id| Reaction { id: *id }).collect(),
        blacklist: Vec::new(),
        signals: Signals::default(),
        payoff: Default::default(),
        session: Vec::new(),
    };
    let ranked: Vec<i64> = recommend::for_you(corpus, &user, &Search::parse(&[]), true, false)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let positions: HashMap<i64, usize> = ranked.iter().enumerate().map(|(position, id)| (*id, position)).collect();

    let top = &ranked[..TOP.min(ranked.len())];
    let top_artists: HashSet<i64> = top.iter().flat_map(|id| artists(corpus, *id)).collect();
    let by_known = top
        .iter()
        .filter(|id| artists(corpus, **id).iter().any(|artist| known_artists.contains(artist)))
        .count();

    let mut report = String::new();
    let _ = writeln!(
        report,
        "{} likes in the dump of {} ({} missing), {} dislikes; ranked from the older {}, looking for the newest {}",
        likes.len(),
        reactions.likes.len(),
        reactions.likes.len() - likes.len(),
        reactions.dislikes.len(),
        earlier.len(),
        held.len()
    );
    let _ = writeln!(report, "- all hidden likes ({}): {}", held.len(), recall(&positions, held));
    let _ = writeln!(
        report,
        "- by artists liked before ({}): {}",
        known_artist.len(),
        recall(&positions, &known_artist)
    );
    let _ = writeln!(
        report,
        "- by artists not liked before ({}): {}",
        new_artist.len(),
        recall(&positions, &new_artist)
    );
    let _ = writeln!(
        report,
        "- top {TOP}: {} different artists, {} posts by artists liked before",
        top_artists.len(),
        by_known
    );
    report
}
