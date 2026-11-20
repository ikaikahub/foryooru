use crate::activity::{Moment, Reacted, Subject};
use crate::corpus::{Adjust, Corpus, Mask, Taste};
use crate::engine::{
    self, Vector, blend, explore_fraction, impression_share, interleave, weighted_sum,
};
use crate::interests::{self, Interest};
use crate::kinds::TagKind;
use crate::payoff::Payoff;
use crate::search::Search;
use crate::signals::Signals;
use crate::tag_vectors;
use rand::seq::SliceRandom;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

// how many posts one ranking returns
pub const RANKING_LIMIT: usize = 2000;
// posts per artist allowed on one page, so one artist can't fill it
pub const MAX_PER_ARTIST: usize = 3;
// the most of the ranking that impressions (looks and skips, not likes) can steer
pub const IMPRESSION_CAP: f64 = 0.30;
// how strongly "similar:-id" chips push away, against 1.0 for the posts searched towards
const SIMILAR_AWAY: f64 = 0.5;
// share of each interest replaced by the similar: chips when both are present
pub const SIMILAR_LEAN: f64 = 0.5;
// the most of the ranking that this visit's likes and long looks can take over;
// a new interest (say a character never liked before) gets at most this share of For you
const SESSION_PULL_MAX: f64 = 0.5;
// the most that this visit's skips and dislikes can push the ranking away from them
const SESSION_PUSH_MAX: f64 = 0.9;
// session strength at which the pull reaches half its max; a like counts 1.0, a long look 0.5
const SESSION_PULL_HALF: f64 = 3.0;
// how close a session post must be to an existing interest to boost it instead of starting a new one
const SESSION_MATCH: f64 = 0.2;
// session strength at which the push reaches half its max, low so a single skip already counts
const SESSION_PUSH_HALF: f64 = 0.25;
// below this many likes every like is its own interest, above it likes are clustered into groups
const PER_LIKE_BELOW: usize = 30;
// how much each kind of session moment counts, against 1.0 for a like
const LONG_LOOK: f64 = 0.5;
const ENGAGED: f64 = 0.75;
const SEARCHED: f64 = 0.75;
const VIEWER_LONG_LOOK: f64 = 0.35;
const VIEWER_QUICK_SKIP: f64 = 0.25;
const QUICK_SKIP: f64 = 0.5;
// session moments that have faded below this are ignored, along with everything older
const MIN_RECENCY: f64 = 0.01;
// clustered interests kept per likes and dislikes combination, so switching searches doesn't recluster
const GROUP_CACHE_SIZE: usize = 4;
// a like's weight halves after this many newer likes
const LIKE_HALF_LIFE: f64 = 300.0;
// session moments other than likes and searches halve after this many viewed posts
const SESSION_HALF_LIFE: f64 = 4.0;
// session likes and searches halve after this many viewed posts
const LASTING_HALF_LIFE: f64 = 16.0;
// extra pull of the newest like when splitting For you between interests; it counts 1 + MOMENTUM times
const MOMENTUM: f64 = 1.5;
// the momentum bonus halves after this many newer likes
const MOMENTUM_HALF_LIFE: f64 = 10.0;
// the newest likes join existing groups or start their own instead of being reclustered, so new tastes show up
const NEWCOMERS: usize = 8;
// how close a newcomer must be to a group to join it rather than start its own
const JOIN_CLOSENESS: f64 = 0.35;
// how strongly dislikes subtract from a taste, against 1.0 for likes
const DISLIKE_STRENGTH: f64 = 0.5;
// share of a post's score lost each time it has been shown without a reaction
pub const SEEN_DROP: f64 = 0.3;
// with a sort chip, the lowest-sorted post keeps this much of its score and the highest keeps all of it
const SORT_FLOOR: f64 = 0.3;
// explore picks are drawn from this many times the posts wanted, preferring unfamiliar series
const EXPLORE_POOL: usize = 12;

#[derive(Serialize, Deserialize)]
pub struct Reaction {
    pub id: i64,
}

#[derive(Serialize, Deserialize)]
pub struct UserData {
    pub likes: Vec<Reaction>,
    pub dislikes: Vec<Reaction>,
    pub blacklist: Vec<String>,
    pub signals: Signals,
    pub payoff: Payoff,
    pub session: Vec<Moment>,
}

impl UserData {
    pub fn liked(&self) -> Vec<&Reaction> {
        self.likes.iter().collect()
    }
}

fn momentum(newer: usize) -> f64 {
    1.0 + MOMENTUM * 0.5_f64.powf(newer as f64 / MOMENTUM_HALF_LIFE)
}

fn weights(count: usize) -> impl Iterator<Item = f64> {
    (0..count).map(|newer| 0.5_f64.powf(newer as f64 / LIKE_HALF_LIFE))
}

fn weighted(corpus: &Corpus, reactions: &[&Reaction]) -> Vec<(i64, Vector, f64)> {
    let present: Vec<&Reaction> = reactions
        .iter()
        .copied()
        .filter(|reaction| corpus.has(reaction.id))
        .collect();
    present
        .iter()
        .zip(weights(present.len()))
        .filter_map(|(reaction, weight)| {
            corpus
                .vector_of(reaction.id)
                .map(|vector| (reaction.id, vector, weight))
        })
        .collect()
}

fn vectors(corpus: &Corpus, reactions: &[&Reaction]) -> Vec<(Vector, f64)> {
    weighted(corpus, reactions)
        .into_iter()
        .map(|(_, vector, weight)| (vector, weight))
        .collect()
}

pub fn likes_profile(corpus: &Corpus, user: &UserData) -> Vector {
    let disliked: Vec<&Reaction> = user.dislikes.iter().collect();
    engine::taste(
        &vectors(corpus, &user.liked()),
        &vectors(corpus, &disliked),
        DISLIKE_STRENGTH,
    )
}

pub struct Session {
    pub parts: Vec<(Vector, f64)>,
    pub towards: Vector,
    pub pull: f64,
    pub away: Vector,
    pub push: f64,
}

impl Session {
    pub fn avoid(&self, profile: &Vector) -> Vector {
        if self.away.is_empty() {
            return profile.clone();
        }
        weighted_sum([(profile, 1.0), (&self.away, -self.push)])
    }
}

fn moment_weight(reacted: Reacted) -> f64 {
    match reacted {
        Reacted::Liked => 1.0,
        Reacted::Disliked => -DISLIKE_STRENGTH,
        Reacted::LongLook => LONG_LOOK,
        Reacted::QuickSkip => -QUICK_SKIP,
        Reacted::Engaged => ENGAGED,
        Reacted::Searched => SEARCHED,
        Reacted::ViewerLongLook => VIEWER_LONG_LOOK,
        Reacted::ViewerQuickSkip => -VIEWER_QUICK_SKIP,
        Reacted::Viewed => 0.0,
    }
}

fn session_share(parts: &[(Vector, f64)], max: f64, half: f64) -> f64 {
    let strength: f64 = parts.iter().map(|(_, weight)| weight).sum();
    max * strength / (strength + half)
}

fn searched_vector(corpus: &Corpus, query: &str) -> Option<Vector> {
    let chips: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
    let tags: Vector = Search::parse(&chips)
        .include
        .iter()
        .filter_map(|name| corpus.tag_id(name))
        .map(|tag| (tag, 1.0))
        .collect();
    (!tags.is_empty()).then(|| engine::normalise(tags))
}

fn moment_vector(corpus: &Corpus, moment: &Moment) -> Option<Vector> {
    match &moment.subject {
        Subject::Post(post) => corpus.vector_of(*post),
        Subject::Search(query) => searched_vector(corpus, query),
    }
}

pub fn session(corpus: &Corpus, moments: &[Moment]) -> Session {
    let mut towards = Vec::new();
    let mut away = Vec::new();
    let mut views_since = 0;
    for moment in moments {
        if 0.5_f64.powf(views_since as f64 / LASTING_HALF_LIFE) < MIN_RECENCY {
            break;
        }
        let half_life = match moment.reacted {
            Reacted::Liked | Reacted::Searched => LASTING_HALF_LIFE,
            _ => SESSION_HALF_LIFE,
        };
        let left = 0.5_f64.powf(views_since as f64 / half_life);
        let weight = moment_weight(moment.reacted) * left;
        if let Some(vector) = moment_vector(corpus, moment) {
            if weight > 0.0 {
                towards.push((vector, weight));
            } else if weight < 0.0 {
                away.push((vector, -weight));
            }
        }
        if moment.reacted.is_view() {
            views_since += 1;
        }
    }
    let combined = |parts: &[(Vector, f64)]| {
        weighted_sum(parts.iter().map(|(vector, weight)| (vector, *weight)))
    };
    Session {
        pull: session_share(&towards, SESSION_PULL_MAX, SESSION_PULL_HALF),
        push: session_share(&away, SESSION_PUSH_MAX, SESSION_PUSH_HALF),
        towards: combined(&towards),
        away: combined(&away),
        parts: towards,
    }
}

pub fn with_session(corpus: &Corpus, user: &UserData, profile: Vector) -> Vector {
    let session = session(corpus, &user.session);
    session.avoid(&blend(&profile, &session.towards, session.pull))
}

fn steer(interests: &mut Vec<Interest>, session: &Session) {
    let total: f64 = session.parts.iter().map(|(_, weight)| weight).sum();
    if total <= 0.0 {
        return;
    }
    let centres: Vec<Vector> = interests
        .iter()
        .map(|interest| interest.profile.clone())
        .collect();
    let mut boosts = vec![0.0; interests.len()];
    let mut unmatched = Vec::new();
    for (vector, weight) in &session.parts {
        match interests::nearest(vector, &centres) {
            (index, closeness) if closeness >= SESSION_MATCH => boosts[index] += weight,
            _ => unmatched.push((vector, *weight)),
        }
    }
    for (interest, boost) in interests.iter_mut().zip(boosts) {
        interest.share = interest.share * (1.0 - session.pull) + session.pull * boost / total;
    }
    let leftover: f64 = unmatched.iter().map(|(_, weight)| weight).sum();
    if leftover > 0.0 {
        interests.push(Interest {
            profile: weighted_sum(unmatched),
            share: session.pull * leftover / total,
            likes: 0,
            score_fit: None,
        });
    }
}

fn add_interest(interests: &mut Vec<Interest>, profile: Vector, share: f64) {
    if profile.is_empty() || share <= 0.0 {
        return;
    }
    let share = if interests.is_empty() { 1.0 } else { share };
    for interest in interests.iter_mut() {
        interest.share *= 1.0 - share;
    }
    interests.push(Interest {
        profile,
        share,
        likes: 0,
        score_fit: None,
    });
}

fn groups_key(corpus: &Corpus, liked: &[&Reaction], user: &UserData) -> u64 {
    let mut hasher = DefaultHasher::new();
    corpus.size().hash(&mut hasher);
    liked.iter().for_each(|like| like.id.hash(&mut hasher));
    user.dislikes
        .iter()
        .for_each(|dislike| dislike.id.hash(&mut hasher));
    hasher.finish()
}

fn grouped(corpus: &Corpus, user: &UserData) -> Vec<Interest> {
    static CACHE: Mutex<Option<HashMap<u64, Vec<Interest>>>> = Mutex::new(None);
    let liked_reactions = user.liked();
    let key = groups_key(corpus, &liked_reactions, user);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cache = cache.get_or_insert_default();
    if let Some(groups) = cache.get(&key) {
        return groups.clone();
    }
    let groups = cluster_likes(corpus, user, &liked_reactions);
    if cache.len() >= GROUP_CACHE_SIZE {
        cache.clear();
    }
    cache.insert(key, groups.clone());
    groups
}

fn cluster_likes(corpus: &Corpus, user: &UserData, liked_reactions: &[&Reaction]) -> Vec<Interest> {
    let liked = weighted(corpus, liked_reactions);
    let points: Vec<(Vector, f64)> = liked
        .iter()
        .map(|(_, vector, weight)| (vector.clone(), *weight))
        .collect();
    let disliked: Vec<&Reaction> = user.dislikes.iter().collect();
    let disliked = vectors(corpus, &disliked);
    let pulls: Vec<f64> = points
        .iter()
        .enumerate()
        .map(|(newer, (_, weight))| weight * momentum(newer))
        .collect();
    let total: f64 = pulls.iter().sum();
    let groups = if points.len() < PER_LIKE_BELOW {
        (0..points.len()).map(|member| vec![member]).collect()
    } else {
        interests::cluster_with_newcomers(&points, NEWCOMERS, JOIN_CLOSENESS)
    };
    groups
        .into_iter()
        .map(|members| {
            let group: Vec<(Vector, f64)> = members
                .iter()
                .map(|member| points[*member].clone())
                .collect();
            let scored: Vec<(i64, f64)> = members
                .iter()
                .map(|member| (liked[*member].0, liked[*member].2))
                .collect();
            Interest {
                share: members.iter().map(|member| pulls[*member]).sum::<f64>() / total,
                likes: members.len(),
                profile: engine::taste(&group, &disliked, DISLIKE_STRENGTH),
                score_fit: corpus.score_fit(&scored),
            }
        })
        .filter(|interest| !interest.profile.is_empty())
        .collect()
}

pub fn interests(corpus: &Corpus, user: &UserData) -> Vec<Interest> {
    let mut found = grouped(corpus, user);
    add_interest(
        &mut found,
        user.signals.profile(|id| corpus.vector_of(id)),
        impression_share(user.signals.active, IMPRESSION_CAP),
    );
    let session = session(corpus, &user.session);
    steer(&mut found, &session);
    for interest in &mut found {
        interest.profile = session.avoid(&interest.profile);
    }
    found
}

pub fn liked_tags(corpus: &Corpus, user: &UserData) -> HashSet<i64> {
    user.likes
        .iter()
        .filter_map(|like| corpus.vector_of(like.id))
        .flat_map(|vector| vector.into_keys())
        .filter(|key| tag_vectors::dimension_of(*key).is_none())
        .collect()
}

fn novelty(corpus: &Corpus, liked_tags: &HashSet<i64>, id: i64) -> f64 {
    let Some(vector) = corpus.vector_of(id) else {
        return 0.0;
    };
    let (unfamiliar, total) = vector
        .iter()
        .filter(|(key, weight)| tag_vectors::dimension_of(**key).is_none() && **weight > 0.0)
        .fold((0.0, 0.0), |(unfamiliar, total), (key, weight)| {
            let new = if liked_tags.contains(key) {
                0.0
            } else {
                *weight
            };
            (unfamiliar + new, total + weight)
        });
    if total > 0.0 { unfamiliar / total } else { 0.0 }
}

pub fn discovering(corpus: &Corpus, liked_tags: &HashSet<i64>, ranked: Vec<i64>) -> Vec<i64> {
    let count = ranked.len().max(1) as f64;
    let mut keyed: Vec<(f64, i64)> = ranked
        .into_iter()
        .enumerate()
        .map(|(position, id)| {
            let closeness = 1.0 - position as f64 / count;
            (closeness * novelty(corpus, liked_tags, id), id)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    keyed.into_iter().map(|(_, id)| id).collect()
}

pub fn similar_profile(corpus: &Corpus, search: &Search) -> Vector {
    let towards: Vec<Vector> = search
        .similar_towards
        .iter()
        .filter_map(|id| corpus.vector_of(*id))
        .collect();
    let away: Vec<Vector> = search
        .similar_away
        .iter()
        .filter_map(|id| corpus.vector_of(*id))
        .collect();
    weighted_sum(
        towards
            .iter()
            .map(|vector| (vector, 1.0))
            .chain(away.iter().map(|vector| (vector, -SIMILAR_AWAY))),
    )
}

pub fn candidates(corpus: &Corpus, search: &Search, blacklist: &[String]) -> Mask {
    let blacklisted = corpus.any_tag_mask(blacklist);
    let seeds: Vec<i64> = search
        .similar_towards
        .iter()
        .chain(&search.similar_away)
        .copied()
        .collect();
    let seeds = corpus.same_files_mask(&seeds);
    corpus
        .search_mask(search)
        .into_iter()
        .zip(blacklisted)
        .zip(seeds)
        .map(|((allowed, blacklisted), seed)| allowed && !blacklisted && !seed)
        .collect()
}

fn region(corpus: &Corpus, row: usize) -> Option<i64> {
    let tags = corpus.tags_of_row(row);
    let largest = |kind: TagKind| {
        tags.iter()
            .filter_map(|tag| corpus.tags.get(tag).map(|info| (*tag, info)))
            .filter(|(_, info)| info.kind == Some(kind))
            .max_by_key(|(_, info)| info.count.unwrap_or(0))
            .map(|(tag, _)| tag)
    };
    largest(TagKind::Copyright)
        .or_else(|| largest(TagKind::Character))
        .or_else(|| largest(TagKind::Artist))
}

fn known_regions(corpus: &Corpus, user: &UserData) -> HashSet<i64> {
    user.likes
        .iter()
        .map(|like| like.id)
        .chain(user.signals.views.keys().copied())
        .filter_map(|id| corpus.row_of(id))
        .filter_map(|row| region(corpus, row))
        .collect()
}

pub fn explore_sample(
    corpus: &Corpus,
    user: &UserData,
    wanted: usize,
    accept: impl Fn(usize) -> bool,
) -> Vec<i64> {
    let known = known_regions(corpus, user);
    let mut probed = HashSet::new();
    let (mut fresh, mut rest): (Vec<i64>, Vec<i64>) = (Vec::new(), Vec::new());
    for id in corpus.sample(wanted * EXPLORE_POOL, accept) {
        let found = corpus.row_of(id).and_then(|row| region(corpus, row));
        match found {
            Some(region) if !known.contains(&region) && probed.insert(region) => fresh.push(id),
            _ => rest.push(id),
        }
    }
    fresh.shuffle(&mut rand::rng());
    fresh.extend(rest);
    fresh.truncate(wanted);
    fresh
}

fn explore_picks(
    corpus: &Corpus,
    user: &UserData,
    mask: &Mask,
    ranked: &[i64],
    share: f64,
) -> Vec<i64> {
    let near_top: HashSet<i64> = ranked
        .iter()
        .take(100.max(ranked.len() / 10))
        .copied()
        .collect();
    let wanted = (ranked.len() as f64 * share) as usize + 1;
    explore_sample(corpus, user, wanted, |row| {
        mask[row] && corpus.score_level_of_row(row) >= 0.5 && !near_top.contains(&corpus.ids[row])
    })
}

pub fn sort_lean(corpus: &Corpus, search: &Search, mask: &Mask) -> HashMap<i64, f64> {
    let Some(order) = search.sort else {
        return HashMap::new();
    };
    corpus
        .sort_percentiles(order, mask, rand::random())
        .into_iter()
        .enumerate()
        .filter(|(row, _)| mask[*row])
        .map(|(row, percentile)| {
            (
                corpus.ids[row],
                SORT_FLOOR + (1.0 - SORT_FLOOR) * percentile,
            )
        })
        .collect()
}

pub fn adjustments(corpus: &Corpus, user: &UserData, search: &Search, mask: &Mask) -> Adjust {
    let mut posts = sort_lean(corpus, search, mask);
    for (id, penalty) in user.signals.penalties(SEEN_DROP) {
        *posts.entry(id).or_insert(1.0) *= penalty;
    }
    Adjust { posts }
}

pub fn for_you_candidates(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    hide_liked: bool,
) -> Mask {
    let mask = candidates(corpus, search, &user.blacklist);
    let liked = user.likes.iter().filter(|_| hide_liked);
    let hidden = corpus.mask_of(liked.chain(&user.dislikes).map(|reaction| reaction.id));
    mask.into_iter()
        .zip(hidden)
        .map(|(allowed, hidden)| allowed && !hidden)
        .collect()
}

pub fn for_you(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    hide_liked: bool,
    discover: bool,
) -> Vec<(i64, bool)> {
    let mask = for_you_candidates(corpus, user, search, hide_liked);
    let mut found = interests(corpus, user);
    let lean = similar_profile(corpus, search);
    if !lean.is_empty() {
        for interest in &mut found {
            interest.profile = blend(&interest.profile, &lean, SIMILAR_LEAN);
        }
        if found.is_empty() {
            add_interest(&mut found, lean, 1.0);
        }
    }
    let adjust = adjustments(corpus, user, search, &mask);
    if found.is_empty() {
        return corpus
            .rank_popular(&mask, RANKING_LIMIT, MAX_PER_ARTIST, &adjust)
            .into_iter()
            .map(|id| (id, false))
            .collect();
    }
    let liked = discover.then(|| liked_tags(corpus, user));
    let lists = found
        .par_iter()
        .map(|interest| {
            let ranked = corpus.rank(
                interest.taste(),
                &mask,
                RANKING_LIMIT,
                MAX_PER_ARTIST,
                &adjust,
            );
            match &liked {
                Some(liked) => discovering(corpus, liked, ranked),
                None => ranked,
            }
        })
        .collect();
    let shares: Vec<f64> = found.iter().map(|interest| interest.share).collect();
    let ranked = interests::weave(lists, &shares);
    let share = explore_fraction(user.likes.len());
    let picks = if ranked.is_empty() {
        Vec::new()
    } else {
        explore_picks(corpus, user, &mask, &ranked, share)
    };
    interleave(ranked, picks, share)
}

pub fn similar(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    per_artist: Option<usize>,
    limit: usize,
) -> Option<Vec<i64>> {
    let profile = similar_profile(corpus, search);
    if !profile.values().any(|weight| *weight > 0.0) {
        return None;
    }
    let mask = candidates(corpus, search, &user.blacklist);
    let cap = per_artist.unwrap_or(limit);
    Some(corpus.rank(
        Taste::plain(&profile),
        &mask,
        limit,
        cap,
        &Adjust {
            posts: sort_lean(corpus, search, &mask),
        },
    ))
}
