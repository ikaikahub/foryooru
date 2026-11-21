mod file;

use crate::engine::{self, Vector, diversify_prefix};
use crate::kinds::{Media, TagKind};
use crate::now_ms;
use crate::search::{Order, Rating, Ratings, Search};
use crate::tag_vectors;
pub use file::{Plain, read_one, read_text, read_values, write_text, write_values};
use rand::Rng;
use rand::seq::IteratorRandom;
use rayon::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

// the per-artist cap applies within each page of this many posts
pub const PAGE_SIZE: usize = 100;

pub type Mask = Vec<bool>;

// post vectors mix exact tag matches with learned tag meanings, this much learned
const LEARNED_SHARE: f64 = 0.5;
// above this many stored posts, ranking only scores posts sharing a strong taste tag
const FULL_SCAN_LIMIT: usize = 200_000;
// the most taste tags used to find those posts
const TASTE_TAGS: usize = 32;
// tags on more posts than this are skipped when finding them, unless they're among the defining tags
const MAX_TAG_POSTS: usize = 150_000;
// the strongest this many taste tags are always used, even if common
const DEFINING_TAGS: usize = 6;
// tags on more posts than this are never used to find posts
const MAX_DEFINING_POSTS: usize = 1_500_000;
// the best this many rough matches get fully scored
const CANDIDATES: usize = 20_000;
// random tries per wanted post when sampling a large corpus
const SAMPLE_TRIES: usize = 200;
// tags without a bundled meaning get one learned from their posts once they have this many
const MIN_FOLD_POSTS: usize = 3;
// posts far from the score range an interest usually likes lose up to this share of their score
const SCORE_FIT_STRENGTH: f64 = 0.5;
// every artist counts as having this many extra non-matching posts when ranking how much of their work matches a
// search, so one matching post out of one doesn't beat forty out of fifty
const SIGNATURE_HEAD_START: f64 = 5.0;
// that score range is never narrower than this, in percentiles
const MIN_SCORE_SPREAD: f64 = 0.15;

#[derive(Clone, Copy)]
pub struct ScoreFit {
    centre: f64,
    spread: f64,
}

#[derive(Clone, Copy)]
pub struct Taste<'a> {
    pub profile: &'a Vector,
    pub fit: Option<ScoreFit>,
}

impl<'a> Taste<'a> {
    pub fn plain(profile: &'a Vector) -> Taste<'a> {
        Taste { profile, fit: None }
    }
}

impl ScoreFit {
    fn factor(self, percentile: f64) -> f64 {
        let distance = (percentile - self.centre) / self.spread;
        1.0 - SCORE_FIT_STRENGTH + SCORE_FIT_STRENGTH * (-0.5 * distance * distance).exp()
    }
}

pub struct NewPost {
    pub id: i64,
    pub rating: Rating,
    pub score: i64,
    pub md5: String,
    pub posted_at: Option<i64>,
    pub media: Media,
    pub tags: Vec<i64>,
}

#[derive(Clone)]
struct Folding {
    sum: Vec<f64>,
    posts: usize,
}

fn md5_key(md5: &str) -> u128 {
    u128::from_str_radix(md5, 16).unwrap_or_else(|_| {
        let mut hasher = DefaultHasher::new();
        md5.hash(&mut hasher);
        hasher.finish() as u128
    })
}

fn unit(values: Vec<f64>) -> Option<Vec<f64>> {
    let norm = values.iter().map(|value| value * value).sum::<f64>().sqrt();
    (norm > 0.0).then(|| values.into_iter().map(|value| value / norm).collect())
}

#[derive(Clone)]
pub struct TagInfo {
    pub name: String,
    pub kind: Option<TagKind>,
    pub count: Option<i64>,
}

#[derive(Clone, Default)]
pub struct Corpus {
    site_total: f64,
    pub tags: HashMap<i64, TagInfo>,
    tag_ids: HashMap<String, i64>,
    pub ids: Vec<i64>,
    ratings: Vec<Rating>,
    scores: Vec<i64>,
    score_percentiles: Vec<f64>,
    posted_at: Vec<Option<i64>>,
    md5s: Vec<u128>,
    media: Vec<Media>,
    indptr: Vec<usize>,
    cols: Vec<u32>,
    row_scale: Vec<f32>,
    learned: Vec<i8>,
    learned_scale: Vec<f32>,
    tag_of_col: Vec<i64>,
    col_of_tag: HashMap<i64, u32>,
    artist_col: Vec<bool>,
    idf_of_col: Vec<f64>,
    posts_of_col: Vec<usize>,
    learned_row_of_col: Vec<Option<usize>>,
    folding: HashMap<u32, Folding>,
    rows: HashMap<i64, usize>,
    postings: std::sync::OnceLock<Vec<Vec<u32>>>,
}

type Scored = Vec<(usize, f64)>;

#[derive(Default)]
pub struct Adjust {
    pub posts: HashMap<i64, f64>,
}

thread_local! {
    static PARTIAL: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl Corpus {
    pub fn new() -> Corpus {
        Corpus {
            indptr: vec![0],
            ..Corpus::default()
        }
    }

    pub fn set_tags(&mut self, site_total: f64, tags: impl IntoIterator<Item = (i64, TagInfo)>) {
        self.site_total = site_total;
        for (id, info) in tags {
            self.tag_ids.insert(info.name.clone(), id);
            self.tags.insert(id, info);
        }
    }

    fn idf(&self, tag: i64) -> f64 {
        match self.tags.get(&tag) {
            None
            | Some(TagInfo {
                kind: Some(TagKind::Metadata | TagKind::Deprecated),
                ..
            }) => 0.0,
            Some(info) => (self.site_total / info.count.unwrap_or(1).max(1) as f64)
                .ln()
                .max(0.0),
        }
    }

    fn column(&mut self, tag: i64) -> u32 {
        if let Some(col) = self.col_of_tag.get(&tag) {
            return *col;
        }
        let col = self.tag_of_col.len() as u32;
        self.col_of_tag.insert(tag, col);
        self.tag_of_col.push(tag);
        self.artist_col.push(
            self.tags
                .get(&tag)
                .is_some_and(|info| info.kind == Some(TagKind::Artist)),
        );
        self.idf_of_col.push(self.idf(tag));
        self.posts_of_col.push(0);
        self.learned_row_of_col.push(
            self.tags
                .get(&tag)
                .and_then(|info| tag_vectors::bundled().row(&info.name)),
        );
        col
    }

    fn learned(&self, cols: &[u32], folded: Option<&HashMap<u32, Vec<f64>>>) -> Option<Vec<f64>> {
        let vectors = tag_vectors::bundled();
        let mut total = vec![0.0; vectors.dimensions];
        for col in cols {
            let idf = self.idf_of_col[*col as usize];
            if idf <= 0.0 {
                continue;
            }
            if let Some(row) = self.learned_row_of_col[*col as usize] {
                for (sum, value) in total.iter_mut().zip(vectors.values(row)) {
                    *sum += idf * *value as f64;
                }
            } else if let Some(folded) = folded.and_then(|folded| folded.get(col)) {
                for (sum, value) in total.iter_mut().zip(folded) {
                    *sum += idf * value;
                }
            }
        }
        unit(total)
    }

    fn fold(&mut self, cols: &[u32]) {
        let Some(known) = self.learned(cols, None) else {
            return;
        };
        for col in cols {
            if self.learned_row_of_col[*col as usize].is_some()
                || self.idf_of_col[*col as usize] <= 0.0
            {
                continue;
            }
            let folding = self.folding.entry(*col).or_insert_with(|| Folding {
                sum: vec![0.0; known.len()],
                posts: 0,
            });
            for (sum, value) in folding.sum.iter_mut().zip(&known) {
                *sum += value;
            }
            folding.posts += 1;
        }
    }

    pub fn append(&mut self, new: Vec<NewPost>) {
        if new.is_empty() {
            return;
        }
        let tagged: Vec<Vec<u32>> = new
            .iter()
            .map(|post| post.tags.iter().map(|tag| self.column(*tag)).collect())
            .collect();
        for cols in &tagged {
            self.fold(cols);
        }
        let folded: HashMap<u32, Vec<f64>> = self
            .folding
            .iter()
            .filter(|(_, folding)| folding.posts >= MIN_FOLD_POSTS)
            .filter_map(|(col, folding)| unit(folding.sum.clone()).map(|vector| (*col, vector)))
            .collect();
        for (post, cols) in new.into_iter().zip(tagged) {
            let norm = cols
                .iter()
                .map(|col| self.idf_of_col[*col as usize].powi(2))
                .sum::<f64>()
                .sqrt();
            let learned = self.learned(&cols, Some(&folded));
            let (tag_share, learned_share) = match learned {
                Some(_) => {
                    let mix = (1.0 - LEARNED_SHARE).hypot(LEARNED_SHARE);
                    ((1.0 - LEARNED_SHARE) / mix, LEARNED_SHARE / mix)
                }
                None => (1.0, 0.0),
            };
            for col in &cols {
                self.posts_of_col[*col as usize] += 1;
            }
            self.cols.extend(cols);
            self.row_scale.push(if norm > 0.0 {
                (tag_share / norm) as f32
            } else {
                0.0
            });
            self.push_learned(learned.as_deref(), learned_share);
            self.rows.insert(post.id, self.ids.len());
            self.ids.push(post.id);
            self.ratings.push(post.rating);
            self.scores.push(post.score);
            self.posted_at.push(post.posted_at);
            self.media.push(post.media);
            self.md5s.push(md5_key(&post.md5));
            self.indptr.push(self.cols.len());
        }
        self.score_percentiles = self.percentile_by_rating(&vec![true; self.size()]);
        self.postings = std::sync::OnceLock::new();
    }

    fn postings(&self) -> Option<&[Vec<u32>]> {
        if self.size() <= FULL_SCAN_LIMIT {
            return None;
        }
        Some(self.postings.get_or_init(|| {
            let mut postings: Vec<Vec<u32>> = self
                .posts_of_col
                .iter()
                .map(|count| Vec::with_capacity(*count))
                .collect();
            for row in 0..self.size() {
                for entry in self.entries(row) {
                    postings[self.cols[entry] as usize].push(row as u32);
                }
            }
            postings
        }))
    }

    pub fn prepare(&self) {
        self.postings();
    }

    pub fn score_fit(&self, liked: &[(i64, f64)]) -> Option<ScoreFit> {
        let known: Vec<(f64, f64)> = liked
            .iter()
            .filter_map(|(id, weight)| {
                self.rows
                    .get(id)
                    .map(|row| (self.score_percentiles[*row], *weight))
            })
            .collect();
        let total: f64 = known.iter().map(|(_, weight)| weight).sum();
        if total <= 0.0 {
            return None;
        }
        let centre = known
            .iter()
            .map(|(percentile, weight)| percentile * weight)
            .sum::<f64>()
            / total;
        let variance = known
            .iter()
            .map(|(percentile, weight)| weight * (percentile - centre).powi(2))
            .sum::<f64>()
            / total;
        Some(ScoreFit {
            centre,
            spread: variance.sqrt().max(MIN_SCORE_SPREAD),
        })
    }

    pub fn size(&self) -> usize {
        self.ids.len()
    }

    pub fn has(&self, id: i64) -> bool {
        self.rows.contains_key(&id)
    }

    fn entries(&self, row: usize) -> std::ops::Range<usize> {
        self.indptr[row]..self.indptr[row + 1]
    }

    fn push_learned(&mut self, learned: Option<&[f64]>, share: f64) {
        let dimensions = tag_vectors::bundled().dimensions;
        let largest = learned
            .into_iter()
            .flatten()
            .fold(0.0_f64, |largest, value| largest.max(value.abs()));
        if largest <= 0.0 {
            self.learned.extend(std::iter::repeat_n(0, dimensions));
            self.learned_scale.push(0.0);
            return;
        }
        for value in learned.into_iter().flatten() {
            self.learned.push((value / largest * 127.0).round() as i8);
        }
        self.learned_scale.push((share * largest / 127.0) as f32);
    }

    fn learned_of_row(&self, row: usize) -> &[i8] {
        let dimensions = tag_vectors::bundled().dimensions;
        &self.learned[row * dimensions..(row + 1) * dimensions]
    }

    pub fn vector_of(&self, id: i64) -> Option<Vector> {
        let row = *self.rows.get(&id)?;
        let scale = self.row_scale[row] as f64;
        let mut vector: Vector = self
            .entries(row)
            .map(|entry| self.cols[entry] as usize)
            .map(|col| (self.tag_of_col[col], self.idf_of_col[col] * scale))
            .filter(|(_, weight)| *weight != 0.0)
            .collect();
        let learned_scale = self.learned_scale[row] as f64;
        if learned_scale > 0.0 {
            for (dimension, value) in self.learned_of_row(row).iter().enumerate() {
                vector.insert(
                    tag_vectors::learned_key(dimension),
                    *value as f64 * learned_scale,
                );
            }
        }
        Some(vector)
    }

    pub fn row_of(&self, id: i64) -> Option<usize> {
        self.rows.get(&id).copied()
    }

    pub fn tags_of_row(&self, row: usize) -> Vec<i64> {
        self.entries(row)
            .map(|entry| self.tag_of_col[self.cols[entry] as usize])
            .collect()
    }

    pub fn score_of_row(&self, row: usize) -> i64 {
        self.scores[row]
    }

    pub fn posted_at_of_row(&self, row: usize) -> Option<i64> {
        self.posted_at[row]
    }

    pub fn rating_of_row(&self, row: usize) -> Rating {
        self.ratings[row]
    }

    pub fn tag_names_of(&self, id: i64) -> Vec<&str> {
        self.row_of(id).map_or_else(Vec::new, |row| {
            self.tags_of_row(row)
                .into_iter()
                .filter_map(|tag| self.tag_name(tag))
                .collect()
        })
    }

    pub fn rating_of(&self, id: i64) -> Option<Rating> {
        self.rows.get(&id).map(|row| self.ratings[*row])
    }

    pub fn media(&self, id: i64) -> Option<Media> {
        self.rows.get(&id).map(|row| self.media[*row])
    }

    pub fn score_level(&self, id: i64) -> Option<f64> {
        self.rows.get(&id).map(|row| self.score_percentiles[*row])
    }

    pub fn artists_of(&self, id: i64) -> Vec<i64> {
        self.rows
            .get(&id)
            .map_or_else(Vec::new, |row| self.artists_of_row(*row))
    }

    pub fn newest_id(&self) -> i64 {
        self.ids.iter().copied().max().unwrap_or(0)
    }

    fn artists_of_row(&self, row: usize) -> Vec<i64> {
        self.entries(row)
            .map(|entry| self.cols[entry] as usize)
            .filter(|col| self.artist_col[*col])
            .map(|col| self.tag_of_col[col])
            .collect()
    }

    pub fn tag_name(&self, tag: i64) -> Option<&str> {
        self.tags.get(&tag).map(|info| info.name.as_str())
    }

    pub fn site_total(&self) -> f64 {
        self.site_total
    }

    pub fn rows_with_tag(&self, tag: i64) -> Option<&[u32]> {
        let col = self.col_of_tag.get(&tag)?;
        self.postings()
            .map(|postings| postings[*col as usize].as_slice())
    }

    pub fn new_post_of_row(&self, row: usize) -> NewPost {
        NewPost {
            id: self.ids[row],
            rating: self.ratings[row],
            score: self.scores[row],
            md5: format!("{:032x}", self.md5s[row]),
            posted_at: self.posted_at[row],
            media: self.media[row],
            tags: self.tags_of_row(row),
        }
    }

    pub fn stored_posts(&self, tag: i64) -> usize {
        self.col_of_tag
            .get(&tag)
            .map_or(0, |col| self.posts_of_col[*col as usize])
    }

    pub fn tag_id(&self, name: &str) -> Option<i64> {
        self.tag_ids.get(name).copied()
    }

    // every post carrying one of the named tags, as (tag name, post id), in one pass over the index
    pub fn posts_tagged(&self, names: &[String]) -> Vec<(String, i64)> {
        let wanted: HashMap<i64, &String> = names
            .iter()
            .filter_map(|name| self.tag_id(name).map(|tag| (tag, name)))
            .collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let wanted = &wanted;
        (0..self.size())
            .flat_map(|row| {
                self.entries(row).filter_map(move |entry| {
                    let tag = self.tag_of_col[self.cols[entry] as usize];
                    wanted.get(&tag).map(|name| ((*name).clone(), self.ids[row]))
                })
            })
            .collect()
    }

    // artists by how much of their stored work matches, best first: their matching posts over all their posts within
    // the searched ratings, so an artist who mostly draws it beats a prolific one with a few matches
    pub fn signature_artists(&self, matching: &Mask, rated: &Mask) -> Vec<i64> {
        let mut counts = vec![(0usize, 0usize); self.tag_of_col.len()];
        let mut count = |col: usize, row: usize| {
            if rated[row] {
                counts[col].1 += 1;
                counts[col].0 += usize::from(matching[row]);
            }
        };
        match self.postings() {
            Some(postings) => {
                for (col, rows) in postings.iter().enumerate().filter(|(col, _)| self.artist_col[*col]) {
                    rows.iter().for_each(|row| count(col, *row as usize));
                }
            }
            None => {
                for row in 0..self.size() {
                    for entry in self.entries(row) {
                        let col = self.cols[entry] as usize;
                        if self.artist_col[col] {
                            count(col, row);
                        }
                    }
                }
            }
        }
        let mut ranked: Vec<(usize, f64)> = counts
            .into_iter()
            .enumerate()
            .filter(|(_, (matched, _))| *matched > 0)
            .map(|(col, (matched, total))| (col, matched as f64 / (total as f64 + SIGNATURE_HEAD_START)))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.into_iter().map(|(col, _)| self.tag_of_col[col]).collect()
    }

    pub fn mask_of(&self, ids: impl IntoIterator<Item = i64>) -> Mask {
        let mut mask = vec![false; self.size()];
        for id in ids {
            if let Some(row) = self.rows.get(&id) {
                mask[*row] = true;
            }
        }
        mask
    }

    pub fn same_files_mask(&self, ids: &[i64]) -> Mask {
        let md5s: HashSet<u128> = ids
            .iter()
            .filter_map(|id| self.rows.get(id))
            .map(|row| self.md5s[*row])
            .collect();
        self.md5s.iter().map(|md5| md5s.contains(md5)).collect()
    }

    pub fn rating_mask(&self, ratings: Ratings) -> Mask {
        self.ratings
            .iter()
            .map(|rating| ratings.contains(*rating))
            .collect()
    }

    fn tag_hits(&self, tags: &[i64]) -> Vec<usize> {
        if let Some(postings) = self.postings() {
            let mut hits = vec![0; self.size()];
            for col in tags.iter().filter_map(|tag| self.col_of_tag.get(tag)) {
                for row in &postings[*col as usize] {
                    hits[*row as usize] += 1;
                }
            }
            return hits;
        }
        let mut hit = vec![false; self.tag_of_col.len()];
        for tag in tags {
            if let Some(col) = self.col_of_tag.get(tag) {
                hit[*col as usize] = true;
            }
        }
        (0..self.size())
            .into_par_iter()
            .map(|row| {
                self.entries(row)
                    .filter(|entry| hit[self.cols[*entry] as usize])
                    .count()
            })
            .collect()
    }

    pub fn any_tag_mask(&self, names: &[String]) -> Mask {
        let tags: Vec<i64> = names.iter().filter_map(|name| self.tag_id(name)).collect();
        if tags.is_empty() {
            return vec![false; self.size()];
        }
        self.tag_hits(&tags)
            .into_iter()
            .map(|hits| hits > 0)
            .collect()
    }

    pub fn search_mask(&self, search: &Search) -> Mask {
        let mut mask = self.rating_mask(search.ratings());
        for (row, allowed) in mask.iter_mut().enumerate() {
            *allowed &= search
                .scores
                .iter()
                .all(|bound| bound.allows(self.scores[row]))
                && search.ids.iter().all(|bound| bound.allows(self.ids[row]));
        }
        let include: Option<Vec<i64>> = search
            .include
            .iter()
            .map(|name| self.tag_id(name))
            .collect();
        let Some(include) = include else {
            return vec![false; self.size()];
        };
        if !include.is_empty() {
            for (allowed, hits) in mask.iter_mut().zip(self.tag_hits(&include)) {
                *allowed &= hits == include.len();
            }
        }
        let exclude: Vec<i64> = search
            .exclude
            .iter()
            .filter_map(|name| self.tag_id(name))
            .collect();
        if !exclude.is_empty() {
            for (allowed, hits) in mask.iter_mut().zip(self.tag_hits(&exclude)) {
                *allowed &= hits == 0;
            }
        }
        mask
    }

    pub fn percentile_by_rating(&self, mask: &Mask) -> Vec<f64> {
        self.percentile_of(mask, |row| self.scores[row] as f64)
    }

    pub fn sort_keys(&self, order: Order, mask: &Mask, seed: u64) -> HashMap<i64, f64> {
        let now = now_ms();
        (0..self.size())
            .filter(|row| mask[*row])
            .map(|row| (self.ids[row], self.sort_key(order, row, now, seed)))
            .collect()
    }

    pub fn sort_percentiles(&self, order: Order, mask: &Mask, seed: u64) -> Vec<f64> {
        let now = now_ms();
        self.percentile_of(mask, |row| self.sort_key(order, row, now, seed))
    }

    fn sort_key(&self, order: Order, row: usize, now: i64, seed: u64) -> f64 {
        match order {
            Order::Score => self.scores[row] as f64,
            Order::Popular => engine::hotness(self.scores[row], self.posted_at[row], now),
            Order::Newest => self.ids[row] as f64,
            Order::Random => {
                let mut hasher = DefaultHasher::new();
                (self.ids[row], seed).hash(&mut hasher);
                hasher.finish() as f64
            }
        }
    }

    fn percentile_of(&self, mask: &Mask, key: impl Fn(usize) -> f64) -> Vec<f64> {
        let mut out = vec![0.0; self.size()];
        for rating in Rating::ALL {
            let mut rows: Vec<usize> = (0..self.size())
                .filter(|row| mask[*row] && self.ratings[*row] == rating)
                .collect();
            rows.sort_by(|a, b| {
                key(*a)
                    .total_cmp(&key(*b))
                    .then(self.ids[*a].cmp(&self.ids[*b]))
            });
            let count = rows.len() as f64;
            for (rank, row) in rows.into_iter().enumerate() {
                out[row] = (rank + 1) as f64 / count;
            }
        }
        out
    }

    pub fn ids_where(&self, mask: &Mask) -> Vec<i64> {
        self.ids
            .iter()
            .zip(mask)
            .filter(|(_, allowed)| **allowed)
            .map(|(id, _)| *id)
            .collect()
    }

    fn retrieve(
        &self,
        weighted: &[(usize, f64, f64)],
        postings: &[Vec<u32>],
        mask: &Mask,
    ) -> Option<Vec<usize>> {
        let mut ordered: Vec<&(usize, f64, f64)> = weighted
            .iter()
            .filter(|(col, weight, _)| *weight > 0.0 && postings[*col].len() <= MAX_DEFINING_POSTS)
            .collect();
        ordered.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        let strongest: Vec<(usize, f64)> = ordered
            .into_iter()
            .enumerate()
            .filter(|(rank, (col, _, _))| {
                *rank < DEFINING_TAGS || postings[*col].len() <= MAX_TAG_POSTS
            })
            .map(|(_, (col, _, contribution))| (*col, *contribution))
            .take(TASTE_TAGS)
            .collect();
        if strongest.is_empty() {
            return None;
        }
        let mut rough: Vec<(usize, f32)> = PARTIAL.with_borrow_mut(|partial| {
            if partial.len() < self.size() {
                partial.resize(self.size(), 0.0);
            }
            let mut touched: Vec<usize> = Vec::new();
            for (col, weight) in strongest {
                for row in &postings[col] {
                    let row = *row as usize;
                    if !mask[row] {
                        continue;
                    }
                    if partial[row] == 0.0 {
                        touched.push(row);
                    }
                    partial[row] += weight as f32;
                }
            }
            touched
                .into_iter()
                .map(|row| (row, std::mem::take(&mut partial[row]) * self.row_scale[row]))
                .collect()
        });
        if rough.len() > CANDIDATES {
            rough.select_nth_unstable_by(CANDIDATES - 1, |a, b| b.1.total_cmp(&a.1));
            rough.truncate(CANDIDATES);
        }
        Some(rough.into_iter().map(|(row, _)| row).collect())
    }

    pub fn sample(&self, wanted: usize, accept: impl Fn(usize) -> bool) -> Vec<i64> {
        let mut rng = rand::rng();
        if self.size() > FULL_SCAN_LIMIT {
            let mut picked = HashSet::new();
            for _ in 0..wanted * SAMPLE_TRIES {
                if picked.len() == wanted {
                    break;
                }
                let row = rng.random_range(0..self.size());
                if accept(row) {
                    picked.insert(self.ids[row]);
                }
            }
            if picked.len() == wanted {
                return picked.into_iter().collect();
            }
        }
        (0..self.size())
            .filter(|row| accept(*row))
            .map(|row| self.ids[row])
            .choose_multiple(&mut rng, wanted)
    }

    fn adjustment(&self, row: usize, adjust: &Adjust) -> f64 {
        adjust.posts.get(&self.ids[row]).copied().unwrap_or(1.0)
    }

    fn scored(&self, taste: Taste, mask: &Mask, adjust: &Adjust) -> Scored {
        let mut dense = vec![0.0; self.tag_of_col.len()];
        let mut learned = vec![0.0; tag_vectors::bundled().dimensions];
        let mut weighted = Vec::with_capacity(taste.profile.len());
        for (tag, weight) in taste.profile {
            if let Some(dimension) = tag_vectors::dimension_of(*tag) {
                learned[dimension] = *weight;
            } else if let Some(col) = self.col_of_tag.get(tag) {
                let col = *col as usize;
                dense[col] = *weight * self.idf_of_col[col];
                weighted.push((col, *weight, dense[col]));
            }
        }
        let score_row = |row: usize| -> f64 {
            let tags: f64 = self
                .entries(row)
                .map(|entry| dense[self.cols[entry] as usize])
                .sum();
            let learned_part: f64 = self
                .learned_of_row(row)
                .iter()
                .zip(&learned)
                .map(|(value, weight)| *value as f64 * weight)
                .sum();
            let mut score =
                tags * self.row_scale[row] as f64 + learned_part * self.learned_scale[row] as f64;
            if let Some(fit) = taste.fit {
                score *= fit.factor(self.score_percentiles[row]);
            }
            score * self.adjustment(row, adjust)
        };
        let candidates = self
            .postings()
            .and_then(|postings| self.retrieve(&weighted, postings, mask));
        let keep = |row: &usize| mask[*row];
        let scored: Scored = match candidates {
            Some(rows) => rows
                .into_par_iter()
                .filter(keep)
                .map(|row| (row, score_row(row)))
                .collect(),
            None => (0..self.size())
                .into_par_iter()
                .filter(keep)
                .map(|row| (row, score_row(row)))
                .collect(),
        };
        scored
            .into_iter()
            .filter(|(_, score)| *score > 0.0)
            .collect()
    }

    fn best(&self, scored: &Scored, count: usize) -> Scored {
        let mut rows = scored.clone();
        let order = |a: &(usize, f64), b: &(usize, f64)| {
            b.1.total_cmp(&a.1).then(self.ids[a.0].cmp(&self.ids[b.0]))
        };
        if rows.len() > count && count > 0 {
            rows.select_nth_unstable_by(count - 1, order);
            rows.truncate(count);
        }
        rows.par_sort_unstable_by(order);
        rows.truncate(count);
        rows
    }

    // each further post by the same artist keeps repeat_kept of its score, so the artists already ranked highest
    // make room for similar posts by others. 1.0 leaves the order as scored
    fn spread_artists(&self, best: Scored, repeat_kept: f64) -> Vec<usize> {
        if repeat_kept >= 1.0 {
            return best.into_iter().map(|(row, _)| row).collect();
        }
        let mut placed: HashMap<i64, i32> = HashMap::new();
        let mut spread: Scored = best
            .into_iter()
            .map(|(row, score)| {
                let artists = self.artists_of_row(row);
                let earlier = artists
                    .iter()
                    .map(|artist| placed.get(artist).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                for artist in artists {
                    *placed.entry(artist).or_default() += 1;
                }
                (row, score * repeat_kept.powi(earlier))
            })
            .collect();
        spread.sort_by(|a, b| b.1.total_cmp(&a.1).then(self.ids[a.0].cmp(&self.ids[b.0])));
        spread.into_iter().map(|(row, _)| row).collect()
    }

    pub fn score_level_of_row(&self, row: usize) -> f64 {
        self.score_percentiles[row]
    }

    pub fn top(
        &self,
        taste: Taste,
        mask: &Mask,
        count: usize,
        cap: usize,
        adjust: &Adjust,
    ) -> Vec<i64> {
        let scored = self.scored(taste, mask, adjust);
        let mut counts: HashMap<i64, usize> = HashMap::new();
        let mut results = Vec::with_capacity(count);
        for (row, _) in self.best(&scored, count * 4) {
            let artists = self.artists_of_row(row);
            if artists
                .iter()
                .any(|artist| counts.get(artist).copied().unwrap_or(0) >= cap)
            {
                continue;
            }
            for artist in artists {
                *counts.entry(artist).or_default() += 1;
            }
            results.push(self.ids[row]);
            if results.len() == count {
                break;
            }
        }
        results
    }

    pub fn rank(
        &self,
        taste: Taste,
        mask: &Mask,
        limit: usize,
        cap: usize,
        repeat_kept: f64,
        adjust: &Adjust,
    ) -> Vec<i64> {
        self.rank_scored(&self.scored(taste, mask, adjust), limit, cap, repeat_kept)
    }

    pub fn rank_popular(&self, mask: &Mask, limit: usize, cap: usize, adjust: &Adjust) -> Vec<i64> {
        let scored: Scored = (0..self.size())
            .into_par_iter()
            .filter(|row| mask[*row])
            .map(|row| {
                let factor = self.adjustment(row, adjust);
                (row, self.score_percentiles[row] * factor)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();
        self.rank_scored(&scored, limit, cap, 1.0)
    }

    fn rank_scored(&self, scored: &Scored, limit: usize, cap: usize, repeat_kept: f64) -> Vec<i64> {
        let positive = scored.len();
        let mut count = limit * 2;
        loop {
            let rows = self.spread_artists(self.best(scored, count), repeat_kept);
            let (ordered, complete) =
                diversify_prefix(&rows, |row| self.artists_of_row(row), cap, PAGE_SIZE, limit);
            if complete || count >= positive {
                return ordered.into_iter().map(|row| self.ids[row]).collect();
            }
            count *= 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: i64, tags: Vec<i64>) -> NewPost {
        NewPost {
            id,
            rating: Rating::General,
            score: 1,
            md5: format!("{id:032x}"),
            posted_at: None,
            media: Media::Image,
            tags,
        }
    }

    #[test]
    fn repeat_posts_by_an_artist_make_room_for_others() {
        let (known, new, sky) = (1, 2, 3);
        let tag = |name: &str, kind| TagInfo {
            name: name.to_owned(),
            kind: Some(kind),
            count: Some(10),
        };
        let mut corpus = Corpus::new();
        corpus.set_tags(
            1_000_000.0,
            [
                (known, tag("known_artist", TagKind::Artist)),
                (new, tag("new_artist", TagKind::Artist)),
                (sky, tag("sky", TagKind::General)),
            ],
        );
        corpus.append(vec![
            post(10, vec![known, sky]),
            post(11, vec![known, sky]),
            post(12, vec![known, sky]),
            post(20, vec![new, sky]),
        ]);
        let row = |id| corpus.row_of(id).unwrap();
        let best = vec![(row(10), 1.0), (row(11), 0.95), (row(12), 0.9), (row(20), 0.85)];

        let ids = |rows: Vec<usize>| -> Vec<i64> { rows.into_iter().map(|row| corpus.ids[row]).collect() };
        assert_eq!(ids(corpus.spread_artists(best.clone(), 1.0)), [10, 11, 12, 20]);
        // the known artist's 2nd post keeps 0.76 and its 3rd 0.58, so the new artist's 0.85 moves up to second
        assert_eq!(ids(corpus.spread_artists(best, 0.8)), [10, 20, 11, 12]);

        let mut tagged = corpus.posts_tagged(&["new_artist".to_owned(), "unknown_artist".to_owned()]);
        tagged.sort();
        assert_eq!(tagged, [("new_artist".to_owned(), 20)]);
    }

    #[test]
    fn signature_artists_put_who_mostly_draws_it_first() {
        let (prolific, devoted, lucky, armor, sky) = (1, 2, 3, 4, 5);
        let tag = |name: &str, kind| TagInfo {
            name: name.to_owned(),
            kind: Some(kind),
            count: Some(10),
        };
        let mut corpus = Corpus::new();
        corpus.set_tags(
            1_000_000.0,
            [
                (prolific, tag("prolific", TagKind::Artist)),
                (devoted, tag("devoted", TagKind::Artist)),
                (lucky, tag("lucky", TagKind::Artist)),
                (armor, tag("armor", TagKind::General)),
                (sky, tag("sky", TagKind::General)),
            ],
        );
        // prolific: 4 of 40 posts have armor, devoted: 8 of 10, lucky: its 1 post
        let mut posts = Vec::new();
        for id in 0..40 {
            posts.push(post(100 + id, if id < 4 { vec![prolific, armor] } else { vec![prolific, sky] }));
        }
        for id in 0..10 {
            posts.push(post(200 + id, if id < 8 { vec![devoted, armor] } else { vec![devoted, sky] }));
        }
        posts.push(post(300, vec![lucky, armor]));
        corpus.append(posts);

        let rated = corpus.rating_mask(Ratings::ALL);
        let matching = corpus.search_mask(&Search::parse(&["armor".to_owned()]));
        let names = |tags: Vec<i64>| -> Vec<String> {
            tags.into_iter().map(|tag| corpus.tag_name(tag).unwrap().to_owned()).collect()
        };
        // devoted 8/15, lucky 1/6, prolific 4/45
        assert_eq!(names(corpus.signature_artists(&matching, &rated)), ["devoted", "lucky", "prolific"]);
        // an artist with no matching posts isn't listed at all
        let none = corpus.search_mask(&Search::parse(&["lucky".to_owned(), "sky".to_owned()]));
        assert!(corpus.signature_artists(&none, &rated).is_empty());
    }
}
