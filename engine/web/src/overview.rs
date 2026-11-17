use crate::corpus::Corpus;
use crate::model::ModelStatus;
use crate::payoff::FetchStats;
use crate::recommend::{self, SEEN_DROP, UserData};
use crate::search::Search;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    posts: usize,
    candidates: usize,
    unseen: usize,
    model: ModelStatus,
    fetching: FetchStats,
}

pub fn overview(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    hide_liked: bool,
    model: ModelStatus,
    fetching: FetchStats,
) -> Overview {
    let mask = recommend::for_you_candidates(corpus, user, search, hide_liked);
    let seen = user.signals.penalties(SEEN_DROP);
    Overview {
        posts: corpus.size(),
        candidates: mask.iter().filter(|allowed| **allowed).count(),
        unseen: corpus
            .ids
            .iter()
            .zip(&mask)
            .filter(|(id, allowed)| **allowed && !seen.contains_key(id))
            .count(),
        model,
        fetching,
    }
}
