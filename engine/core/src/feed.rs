use crate::corpus::{Adjust, Corpus, Mask, Taste};
use crate::engine::{Vector, blend, fetch_gap, impression_share, weighted_sum};
use crate::interests;
use crate::kinds::TagKind;
use crate::model::{self, Model};
use crate::payoff::Payoff;
use crate::recommend::{self, IMPRESSION_CAP, MAX_PER_ARTIST, SIMILAR_LEAN, UserData};
use crate::search::Search;
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const BATCH: usize = 8;
const STREAK_LENGTH: usize = 5;
const RECENT_WINDOW: usize = 60;
const MIN_THROWBACK_LIKES: usize = 10;
const SIMILAR_DRIFT_MAX: f64 = 0.7;
const SIMILAR_DRIFT_HALF: f64 = 2.0;
const LEAD_CHOICES: usize = 10;
pub const MAX_LEAD_POSTS: i64 = 300_000;
const MAX_GENERAL_LEAD_POSTS: i64 = 20_000;

pub fn lead_limit(kind: Option<TagKind>) -> i64 {
    if kind.is_some_and(TagKind::is_specific) {
        MAX_LEAD_POSTS
    } else {
        MAX_GENERAL_LEAD_POSTS
    }
}
const NEIGHBOUR_POSTS: usize = 200;
const MIN_SESSION_PULL: f64 = 0.3;
const DISCOVER_POOL: usize = 8;

#[derive(Clone, Copy)]
struct Mix {
    similar: usize,
    now: usize,
    throwback: usize,
    explore: usize,
}

const MIX: Mix = Mix {
    similar: 0,
    now: 6,
    throwback: 1,
    explore: 1,
};
const SIMILAR_MIX: Mix = Mix {
    similar: 6,
    now: 1,
    throwback: 0,
    explore: 1,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Similar,
    Now,
    Throwback,
    Explore,
    Random,
}

pub struct Batch {
    pub ids: Vec<i64>,
    pub leads: Leads,
    pub exhausted: bool,
}

#[derive(Default, Clone)]
pub struct Leads {
    pub taste: Option<String>,
    pub neighbour: Option<String>,
    pub session: Option<String>,
}

fn is_lead(corpus: &Corpus, tag: i64, artists: bool) -> bool {
    corpus.tags.get(&tag).is_some_and(|info| {
        let kind_fits = match info.kind {
            Some(TagKind::General | TagKind::Character | TagKind::Copyright) => true,
            Some(TagKind::Artist) => artists,
            _ => false,
        };
        kind_fits
            && info
                .count
                .is_some_and(|count| count <= lead_limit(info.kind))
    })
}

fn pick_lead(
    corpus: &Corpus,
    scored: Vec<(i64, f64)>,
    payoff: &Payoff,
    rng: &mut impl Rng,
) -> Option<String> {
    let mut named: Vec<(&str, f64)> = scored
        .into_iter()
        .filter_map(|(tag, score)| {
            let name = corpus.tag_name(tag)?;
            Some((name, score * payoff.factor(name)))
        })
        .collect();
    named.sort_by(|a, b| b.1.total_cmp(&a.1));
    named.truncate(LEAD_CHOICES);
    let (name, _) = named.choose_weighted(rng, |(_, score)| *score).ok()?;
    Some((*name).to_owned())
}

fn taste_scores(corpus: &Corpus, profile: &Vector) -> Vec<(i64, f64)> {
    profile
        .iter()
        .filter(|(tag, weight)| **weight > 0.0 && is_lead(corpus, **tag, false))
        .map(|(tag, weight)| {
            let site = corpus.tags.get(tag).and_then(|info| info.count);
            (*tag, weight * fetch_gap(corpus.stored_posts(*tag), site))
        })
        .filter(|(_, score)| *score > 0.0)
        .collect()
}

fn taste_lead(
    corpus: &Corpus,
    profile: &Vector,
    payoff: &Payoff,
    rng: &mut impl Rng,
) -> Option<String> {
    pick_lead(corpus, taste_scores(corpus, profile), payoff, rng)
}

fn session_lead(corpus: &Corpus, user: &UserData, rng: &mut impl Rng) -> Option<String> {
    let session = recommend::session(corpus, &user.session);
    if session.pull < MIN_SESSION_PULL {
        return None;
    }
    taste_lead(corpus, &session.avoid(&session.towards), &user.payoff, rng)
}

fn neighbour_scores(
    corpus: &Corpus,
    user: &UserData,
    profile: &Vector,
    candidates: &Mask,
    adjust: &Adjust,
) -> Vec<(i64, f64)> {
    let liked_tags = recommend::liked_tags(corpus, user);
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for id in corpus.top(
        Taste::plain(profile),
        candidates,
        NEIGHBOUR_POSTS,
        MAX_PER_ARTIST,
        adjust,
    ) {
        for (tag, weight) in corpus.vector_of(id).unwrap_or_default() {
            if !liked_tags.contains(&tag) && is_lead(corpus, tag, false) {
                *scores.entry(tag).or_default() += weight;
            }
        }
    }
    scores.into_iter().collect()
}

fn neighbour_lead(
    corpus: &Corpus,
    user: &UserData,
    profile: &Vector,
    candidates: &Mask,
    adjust: &Adjust,
    rng: &mut impl Rng,
) -> Option<String> {
    pick_lead(
        corpus,
        neighbour_scores(corpus, user, profile, candidates, adjust),
        &user.payoff,
        rng,
    )
}

#[derive(Default)]
struct Picker {
    taken: HashSet<i64>,
    items: Vec<(i64, Kind)>,
}

impl Picker {
    fn take(&mut self, kind: Kind, ids: &[i64], mut wanted: usize) -> usize {
        for id in ids {
            if wanted == 0 {
                break;
            }
            if self.taken.insert(*id) {
                self.items.push((*id, kind));
                wanted -= 1;
            }
        }
        wanted
    }
}

fn drifted(corpus: &Corpus, profile: &Vector, session_likes: &[i64]) -> Vector {
    let liked: Vec<Vector> = session_likes
        .iter()
        .filter_map(|id| corpus.vector_of(*id))
        .collect();
    if liked.is_empty() {
        return profile.clone();
    }
    let drift = SIMILAR_DRIFT_MAX * liked.len() as f64 / (liked.len() as f64 + SIMILAR_DRIFT_HALF);
    blend(
        profile,
        &weighted_sum(liked.iter().map(|vector| (vector, 1.0))),
        drift,
    )
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeedOptions {
    pub hide_liked: bool,
    pub similar_per_artist: Option<usize>,
    pub discover: bool,
}

fn unseen_candidates(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    seen: &HashSet<i64>,
    hide_liked: bool,
) -> Mask {
    let liked = user.likes.iter().filter(|_| hide_liked);
    let reacted = liked.chain(&user.dislikes).map(|reaction| reaction.id);
    let seen_mask = corpus.mask_of(seen.iter().copied().chain(reacted));
    recommend::candidates(corpus, search, &user.blacklist)
        .into_iter()
        .zip(seen_mask)
        .map(|(allowed, seen)| allowed && !seen)
        .collect()
}

#[derive(Clone, Copy)]
enum DropOrder {
    GeneralFirst,
    RarestFirst,
}

fn kept_longest(kind: Option<TagKind>) -> u8 {
    match kind {
        Some(TagKind::Artist) => 3,
        Some(TagKind::Character) => 2,
        Some(TagKind::Copyright) => 1,
        _ => 0,
    }
}

fn dropping_order(corpus: &Corpus, tags: &[String], order: DropOrder) -> Vec<String> {
    let info = |name: &String| corpus.tag_id(name).and_then(|tag| corpus.tags.get(&tag));
    let mut tags = tags.to_vec();
    match order {
        DropOrder::GeneralFirst => {
            tags.sort_by_key(|name| kept_longest(info(name).and_then(|tag| tag.kind)))
        }
        DropOrder::RarestFirst => {
            tags.sort_by_key(|name| info(name).and_then(|tag| tag.count).unwrap_or(0))
        }
    }
    tags
}

fn taste_profile(corpus: &Corpus, user: &UserData) -> Vector {
    let profile = blend(
        &recommend::likes_profile(corpus, user),
        &user.signals.profile(|id| corpus.vector_of(id)),
        impression_share(user.signals.active, IMPRESSION_CAP),
    );
    recommend::with_session(corpus, user, profile)
}

pub fn leads_for(corpus: &Corpus, user: &UserData, search: &Search, hide_liked: bool) -> Leads {
    let mut rng = rand::rng();
    let profile = taste_profile(corpus, user);
    let candidates = unseen_candidates(corpus, user, search, &HashSet::new(), hide_liked);
    Leads {
        taste: taste_lead(corpus, &profile, &user.payoff, &mut rng),
        neighbour: neighbour_lead(
            corpus,
            user,
            &profile,
            &candidates,
            &recommend::adjustments(corpus, user, search, &candidates, None),
            &mut rng,
        ),
        session: session_lead(corpus, user, &mut rng),
    }
}

pub fn broaden(
    corpus: &Corpus,
    user: &UserData,
    chips: &[String],
    seen: &HashSet<i64>,
    hide_liked: bool,
) -> Option<Vec<String>> {
    let search = Search::parse(chips);
    if !search.has_tag() {
        return None;
    }
    let order = if rand::rng().random_bool(0.5) {
        DropOrder::GeneralFirst
    } else {
        DropOrder::RarestFirst
    };
    let mut wider = chips.to_vec();
    for tag in dropping_order(corpus, &search.include, order) {
        wider.retain(|chip| *chip != tag);
        let candidates = unseen_candidates(corpus, user, &Search::parse(&wider), seen, hide_liked);
        if candidates.iter().filter(|allowed| **allowed).count() >= BATCH {
            break;
        }
    }
    Some(wider)
}

pub struct Scrolled<'a> {
    pub seen: &'a HashSet<i64>,
    pub since: i64,
    pub session_likes: &'a [i64],
}

pub fn next_batch(
    corpus: &Corpus,
    user: &UserData,
    search: &Search,
    scrolled: Scrolled,
    options: FeedOptions,
    model: &Model,
) -> Batch {
    let Scrolled {
        seen,
        since,
        session_likes,
    } = scrolled;
    let mut rng = rand::rng();
    let candidates = unseen_candidates(corpus, user, search, seen, options.hide_liked);
    let candidate_ids = corpus.ids_where(&candidates);
    if candidate_ids.is_empty() {
        return Batch {
            ids: Vec::new(),
            leads: Leads::default(),
            exhausted: true,
        };
    }

    let chip_profile = recommend::similar_profile(corpus, search);
    let seed_profile = chip_profile
        .values()
        .any(|weight| *weight > 0.0)
        .then(|| drifted(corpus, &chip_profile, session_likes));
    let implicit = user.signals.profile(|id| corpus.vector_of(id));
    let mut found = recommend::interests(corpus, user);
    if !chip_profile.is_empty() && seed_profile.is_none() {
        for interest in &mut found {
            interest.profile = blend(&interest.profile, &chip_profile, SIMILAR_LEAN);
        }
    }
    let liked = model::Liked::new(corpus, user, &found);
    let context = model::Context::new(corpus, &liked, &user.session);
    let liked_tags = options
        .discover
        .then(|| recommend::liked_tags(corpus, user));
    let adjust = recommend::adjustments(corpus, user, search, &candidates, Some(&model.tags));
    let (mix, main) = if seed_profile.is_some() {
        (SIMILAR_MIX, Kind::Similar)
    } else {
        (MIX, Kind::Now)
    };
    let mut picker = Picker::default();

    let streaks: Vec<Option<usize>> = (0..BATCH)
        .map(|offset| {
            let streak = (seen.len() + offset) / STREAK_LENGTH;
            interests::pick(&found, (since as u64).wrapping_add(streak as u64))
        })
        .collect();
    let mut lists: HashMap<usize, Vec<i64>> = HashMap::new();
    for interest in streaks.iter().flatten() {
        lists.entry(*interest).or_insert_with(|| match &liked_tags {
            Some(liked_tags) => recommend::discovering(
                corpus,
                liked_tags,
                corpus.top(
                    found[*interest].taste(),
                    &candidates,
                    BATCH * DISCOVER_POOL,
                    MAX_PER_ARTIST,
                    &adjust,
                ),
            ),
            None => model.rerank(
                &context,
                corpus.top(
                    found[*interest].taste(),
                    &candidates,
                    BATCH * 2,
                    MAX_PER_ARTIST,
                    &adjust,
                ),
            ),
        });
    }
    let mut queued = HashSet::new();
    let now: Vec<i64> = streaks
        .iter()
        .flatten()
        .filter_map(|interest| {
            lists[interest]
                .iter()
                .find(|id| queued.insert(**id))
                .copied()
        })
        .collect();
    let mut shown_soon: HashSet<i64> = now.iter().copied().collect();
    let mut similar = Vec::new();
    let mut similar_short = 0;
    if let Some(seed) = &seed_profile {
        similar = corpus.top(
            Taste::plain(seed),
            &candidates,
            mix.similar + BATCH,
            options.similar_per_artist.unwrap_or(usize::MAX),
            &adjust,
        );
        similar_short = picker.take(Kind::Similar, &similar, mix.similar);
        shown_soon.extend(&similar);
    }

    let liked = user.liked();
    let old: Vec<Vector> = liked
        .iter()
        .skip(RECENT_WINDOW)
        .filter_map(|like| corpus.vector_of(like.id))
        .collect();
    let mut missing = mix.throwback;
    if missing > 0 && old.len() >= MIN_THROWBACK_LIKES {
        let old_profile = weighted_sum(old.iter().map(|vector| (vector, 1.0)));
        let throwback: Vec<i64> = corpus
            .top(
                Taste::plain(&old_profile),
                &candidates,
                BATCH * 3,
                MAX_PER_ARTIST,
                &adjust,
            )
            .into_iter()
            .filter(|id| !shown_soon.contains(id))
            .collect();
        missing = picker.take(Kind::Throwback, &throwback, mix.throwback);
    }

    let well_liked = recommend::explore_sample(corpus, user, BATCH * 2, |row| {
        candidates[row]
            && corpus.score_level_of_row(row) >= 0.6
            && !shown_soon.contains(&corpus.ids[row])
    });
    let explore = if implicit.is_empty() {
        well_liked
    } else {
        let lingering = corpus.top(
            Taste::plain(&implicit),
            &corpus.mask_of(well_liked.iter().copied()),
            mix.explore,
            MAX_PER_ARTIST,
            &Adjust::default(),
        );
        lingering
            .iter()
            .zip(&well_liked)
            .flat_map(|(a, b)| [*a, *b])
            .chain(well_liked.iter().copied())
            .collect()
    };
    missing += picker.take(Kind::Explore, &explore, mix.explore);

    missing += similar_short;
    if main == Kind::Similar {
        missing = picker.take(Kind::Similar, &similar, missing);
    }
    picker.take(Kind::Now, &now, mix.now + missing);
    if picker.items.len() < BATCH {
        let wanted = BATCH - picker.items.len();
        let rest: Vec<i64> = candidate_ids
            .choose_multiple(&mut rng, BATCH * 2)
            .copied()
            .collect();
        picker.take(Kind::Random, &rest, wanted);
    }

    let (mut ordered, others): (Vec<_>, Vec<_>) = picker
        .items
        .into_iter()
        .partition(|(_, kind)| *kind == main);
    for item in others {
        let at = rng.random_range(ordered.len().min(1)..=ordered.len());
        ordered.insert(at, item);
    }
    let ids: Vec<i64> = ordered.into_iter().map(|(id, _)| id).collect();

    let no_taste = Vector::new();
    let shaping = seed_profile
        .as_ref()
        .or_else(|| streaks[0].map(|interest| &found[interest].profile))
        .unwrap_or(&no_taste);
    let leads = Leads {
        taste: taste_lead(corpus, shaping, &user.payoff, &mut rng),
        neighbour: neighbour_lead(corpus, user, shaping, &candidates, &adjust, &mut rng),
        session: session_lead(corpus, user, &mut rng),
    };
    Batch {
        exhausted: ids.len() < BATCH,
        ids,
        leads,
    }
}
