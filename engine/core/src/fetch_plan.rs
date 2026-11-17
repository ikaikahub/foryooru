use crate::engine;
use crate::feed::{self, Leads, MAX_LEAD_POSTS};
use crate::kinds::TagKind;
use crate::search::Order;
use rand::Rng;

pub const GELBOORU_MAX_OFFSET: i64 = 20_000;
const ARTIST_PAGES: usize = 3;
const OTHER_PAGES: usize = 1;
const TAGS_PER_POST: usize = 3;
const REFILL_SEARCHES: usize = 2;
const WELL_FETCHED: f64 = 0.5;

pub struct PostTag {
    pub name: String,
    pub kind: Option<TagKind>,
    pub count: Option<i64>,
    pub stored: usize,
}

impl PostTag {
    pub fn under_fetched(&self) -> bool {
        engine::missing_share(self.stored, self.count).is_none_or(|missing| missing > WELL_FETCHED)
    }
}

pub struct TagFetch {
    pub order: Option<Order>,
    pub pages: usize,
}

pub fn tag_fetch(tag: &PostTag) -> TagFetch {
    if tag.kind == Some(TagKind::Artist) {
        TagFetch {
            order: None,
            pages: ARTIST_PAGES,
        }
    } else {
        TagFetch {
            order: Some(Order::Score),
            pages: OTHER_PAGES,
        }
    }
}

pub fn targets(tags: &[PostTag]) -> Vec<&PostTag> {
    let mut scored: Vec<(&PostTag, f64)> = tags
        .iter()
        .filter(|tag| {
            tag.kind
                .is_none_or(|kind| kind.is_specific() || kind == TagKind::General)
                && tag.under_fetched()
        })
        .filter_map(|tag| {
            let count = tag.count.unwrap_or(tag.stored as i64).max(1);
            if count > feed::lead_limit(tag.kind) {
                return None;
            }
            let distinctive = (1.0 + MAX_LEAD_POSTS as f64 / count as f64).ln();
            let score = distinctive * engine::fetch_gap(tag.stored, tag.count);
            (score > 0.0).then_some((tag, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
        .into_iter()
        .take(TAGS_PER_POST)
        .map(|(tag, _)| tag)
        .collect()
}

pub fn followed_artists(tags: &[PostTag]) -> impl Iterator<Item = &PostTag> {
    tags.iter()
        .filter(|tag| tag.kind == Some(TagKind::Artist) && tag.under_fetched())
}

pub fn lead_tags(leads: Leads, likes: usize, rng: &mut impl Rng) -> Vec<String> {
    let mut discovery: Vec<String> = leads.neighbour.into_iter().collect();
    let mut taste: Vec<String> = leads.taste.into_iter().collect();
    let share = engine::discovery_share(likes);
    let mut tags = Vec::new();
    for _ in 0..REFILL_SEARCHES {
        let (preferred, other) = if rng.random_bool(share) {
            (&mut discovery, &mut taste)
        } else {
            (&mut taste, &mut discovery)
        };
        tags.extend(preferred.pop().or_else(|| other.pop()));
    }
    tags.extend(leads.session);
    tags
}
