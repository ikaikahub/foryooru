use std::collections::HashMap;

pub type Vector = HashMap<i64, f64>;

// explore picks start at half of For you and shrink as likes grow, down to a quarter at this many likes
const EXPLORE_STRENGTH: f64 = 20.0;
// popular sort: score / (age in days + HOT_AGE_OFFSET_DAYS) ^ HOT_GRAVITY, so newer posts rank higher
const HOT_GRAVITY: f64 = 1.5;
// keeps brand new posts from dividing by almost zero
const HOT_AGE_OFFSET_DAYS: f64 = 2.0;
// posts without a date are treated as this old
const UNDATED_AGE_DAYS: f64 = 365.0;
const DAY_MS: f64 = 86_400_000.0;
// refill searches go to tags next to your taste at least this often, however many likes you have
const MIN_DISCOVERY: f64 = 1.0 / 3.0;
// how much opening a post in the viewer adds to its implicit score; opens count at most twice.
// view time was left out: replaying real history ranked liked posts higher without it
const OPEN: f64 = 0.3;
const MAX_OPENS: i64 = 2;
// a post's implicit score halves after this many days
const IMPRESSION_HALF_LIFE_DAYS: f64 = 3.0;
// impressions reach half of IMPRESSION_CAP once this many posts have them
const IMPRESSION_SATURATION: f64 = 300.0;

pub fn normalise(vector: Vector) -> Vector {
    let norm = vector
        .values()
        .map(|weight| weight * weight)
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 {
        return Vector::new();
    }
    vector
        .into_iter()
        .filter(|(_, weight)| *weight != 0.0)
        .map(|(tag, weight)| (tag, weight / norm))
        .collect()
}

pub fn dot(a: &Vector, b: &Vector) -> f64 {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    small
        .iter()
        .map(|(tag, weight)| weight * large.get(tag).copied().unwrap_or(0.0))
        .sum()
}

pub fn weighted_sum<'a>(parts: impl IntoIterator<Item = (&'a Vector, f64)>) -> Vector {
    let mut total = Vector::new();
    for (vector, factor) in parts {
        for (tag, weight) in vector {
            *total.entry(*tag).or_default() += factor * weight;
        }
    }
    normalise(total)
}

pub fn taste(liked: &[(Vector, f64)], disliked: &[(Vector, f64)], dislike_strength: f64) -> Vector {
    let liked = weighted_sum(liked.iter().map(|(vector, weight)| (vector, *weight)));
    if disliked.is_empty() || liked.is_empty() {
        return liked;
    }
    let disliked = weighted_sum(disliked.iter().map(|(vector, weight)| (vector, *weight)));
    weighted_sum([(&liked, 1.0), (&disliked, -dislike_strength)])
}

pub fn blend(profile: &Vector, other: &Vector, share: f64) -> Vector {
    if other.is_empty() || share <= 0.0 {
        return profile.clone();
    }
    weighted_sum([(profile, 1.0 - share), (other, share)])
}

pub fn hotness(score: i64, posted_at: Option<i64>, now: i64) -> f64 {
    let age_days = posted_at.map_or(UNDATED_AGE_DAYS, |at| ((now - at) as f64 / DAY_MS).max(0.0));
    score as f64 / (age_days + HOT_AGE_OFFSET_DAYS).powf(HOT_GRAVITY)
}

pub fn missing_share(stored: usize, site_count: Option<i64>) -> Option<f64> {
    let site = site_count.filter(|count| *count > 0)? as f64;
    Some((1.0 - stored as f64 / site).clamp(0.0, 1.0))
}

pub fn fetch_gap(stored: usize, site_count: Option<i64>) -> f64 {
    missing_share(stored, site_count).unwrap_or_else(|| 1.0 / (1.0 + stored as f64).sqrt())
}

fn base_explore(likes: usize) -> f64 {
    0.5 * EXPLORE_STRENGTH / (likes as f64 + EXPLORE_STRENGTH)
}

pub fn explore_fraction(likes: usize) -> f64 {
    base_explore(likes).clamp(0.0, 1.0)
}

pub fn discovery_share(likes: usize) -> f64 {
    MIN_DISCOVERY + (1.0 - MIN_DISCOVERY) * 2.0 * base_explore(likes)
}

pub fn interleave(ranked: Vec<i64>, picks: Vec<i64>, fraction: f64) -> Vec<(i64, bool)> {
    let mut out = Vec::with_capacity(ranked.len() + picks.len());
    let mut placed = std::collections::HashSet::new();
    let mut ranked = ranked.into_iter();
    let mut picks = picks.into_iter();
    let mut credit = 0.0;
    loop {
        credit += fraction;
        let explore = credit >= 1.0;
        if explore {
            credit -= 1.0;
        }
        let next = if explore {
            picks
                .by_ref()
                .find(|id| !placed.contains(id))
                .map(|id| (id, true))
        } else {
            None
        }
        .or_else(|| {
            ranked
                .by_ref()
                .find(|id| !placed.contains(id))
                .map(|id| (id, false))
        });
        let Some((id, explored)) = next else {
            return out;
        };
        placed.insert(id);
        out.push((id, explored));
    }
}

pub fn implicit_score(opens: i64, age_days: f64) -> f64 {
    OPEN * opens.clamp(0, MAX_OPENS) as f64 * 0.5_f64.powf(age_days.max(0.0) / IMPRESSION_HALF_LIFE_DAYS)
}

pub fn impression_share(active: usize, cap: f64) -> f64 {
    if active == 0 {
        0.0
    } else {
        cap * active as f64 / (active as f64 + IMPRESSION_SATURATION)
    }
}

pub fn diversify_prefix(
    ranked: &[usize],
    artists_of: impl Fn(usize) -> Vec<i64>,
    cap: usize,
    page_size: usize,
    limit: usize,
) -> (Vec<usize>, bool) {
    if cap == 0 {
        return (ranked.iter().copied().take(limit).collect(), true);
    }
    let mut out = Vec::new();
    let mut waiting: Vec<usize> = ranked.to_vec();
    let mut complete = true;
    while !waiting.is_empty() && out.len() < limit {
        let mut page = Vec::new();
        let mut counts: HashMap<i64, usize> = HashMap::new();
        let mut skipped = Vec::new();
        let mut taken = 0;
        while taken < waiting.len() && page.len() < page_size {
            let row = waiting[taken];
            taken += 1;
            let artists = artists_of(row);
            if artists
                .iter()
                .any(|artist| counts.get(artist).copied().unwrap_or(0) >= cap)
            {
                skipped.push(row);
                continue;
            }
            for artist in artists {
                *counts.entry(artist).or_default() += 1;
            }
            page.push(row);
        }
        if page.len() < page_size && out.len() + page.len() < limit {
            complete = false;
        }
        if page.is_empty() {
            let rest = skipped.split_off(page_size.min(skipped.len()));
            page = std::mem::replace(&mut skipped, rest);
        }
        out.extend(page);
        skipped.extend_from_slice(&waiting[taken..]);
        waiting = skipped;
    }
    out.truncate(limit);
    (out, complete)
}
