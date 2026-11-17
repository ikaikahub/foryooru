use crate::activity::{Happened, Logged, Moment, Reacted, Source};
use crate::corpus::Corpus;
use crate::engine::{Vector, dot};
use crate::interests::Interest;
use crate::kinds::{Media, TagKind};
use crate::preference::TagPreference;
use crate::recommend::{self, Session, UserData};
use crate::search::Rating;
use crate::tag_vectors;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

const LEARNING_RATE: f64 = 0.15;
const L2: f64 = 0.0005;
const REACTION_WEIGHT: f64 = 3.0;
const ENGAGED_WEIGHT: f64 = 2.0;
const FEED_LOOK_WEIGHT: f64 = 1.0;
const VIEWER_LOOK_WEIGHT: f64 = 0.5;
const REACTION_WINDOW_MS: i64 = 120_000;
const STARTING_TRUST: f64 = 1.0;
const MIN_SQUARED: f64 = 1e-6;
const MIN_VARIANCE: f64 = 1e-6;
const GRADING_WINDOW: usize = 300;
const MIN_GRADED: usize = 100;
const TRUST_MARGIN: f64 = 0.05;
const RECENT_VIEWS: usize = 20;

#[derive(Clone, Copy)]
enum Dial {
    Bias,
    CurrentLogic,
    Towards,
    Away,
    TowardsTimesPull,
    AwayTimesPush,
    BestGroup,
    AllLikes,
    ArtistMatch,
    CharacterMatch,
    SeriesMatch,
    GeneralMatch,
    ScoreLevel,
    Newness,
    Gif,
    Video,
    Explicit,
    Questionable,
    ArtistSeen,
    ArtistLookedAt,
    ArtistSkipped,
    LastWasSkip,
    LastWasLongLook,
    ViewsThisSession,
}

const DIALS: usize = Dial::ViewsThisSession as usize + 1;

fn feature_count() -> usize {
    DIALS + tag_vectors::bundled().dimensions
}

pub struct Liked {
    groups: Vec<Vector>,
    all_likes: Vector,
}

impl Liked {
    pub fn new(corpus: &Corpus, user: &UserData, found: &[Interest]) -> Liked {
        Liked {
            groups: found
                .iter()
                .filter(|interest| interest.likes > 0)
                .map(|interest| interest.profile.clone())
                .collect(),
            all_likes: recommend::likes_profile(corpus, user),
        }
    }
}

pub struct Context<'a> {
    corpus: &'a Corpus,
    liked: &'a Liked,
    session: Session,
    steered: Vector,
    newest: f64,
    seen_artists: HashSet<i64>,
    looked_at_artists: HashSet<i64>,
    skipped_artists: HashSet<i64>,
    last_view: Option<Reacted>,
    views: usize,
}

impl<'a> Context<'a> {
    pub fn new(corpus: &'a Corpus, liked: &'a Liked, moments: &[Moment]) -> Context<'a> {
        let views: Vec<&Moment> = moments
            .iter()
            .filter(|moment| moment.reacted.is_view())
            .collect();
        let artists_where = |keep: fn(Reacted) -> bool| -> HashSet<i64> {
            views
                .iter()
                .take(RECENT_VIEWS)
                .filter(|moment| keep(moment.reacted))
                .filter_map(|moment| moment.post())
                .flat_map(|post| corpus.artists_of(post))
                .collect()
        };
        let session = recommend::session(corpus, moments);
        Context {
            corpus,
            liked,
            steered: session.avoid(&session.towards),
            session,
            newest: corpus.newest_id().max(1) as f64,
            seen_artists: artists_where(|_| true),
            looked_at_artists: artists_where(|reacted| matches!(reacted, Reacted::LongLook)),
            skipped_artists: artists_where(|reacted| matches!(reacted, Reacted::QuickSkip)),
            last_view: views.first().map(|moment| moment.reacted),
            views: views.len(),
        }
    }

    pub fn features(&self, id: i64) -> Option<Vec<f64>> {
        let vector = self.corpus.vector_of(id)?;
        let mut features = vec![0.0; feature_count()];
        let mut set = |dial: Dial, value: f64| features[dial as usize] = value;
        let best_group = self
            .liked
            .groups
            .iter()
            .map(|group| dot(&vector, group))
            .fold(0.0, f64::max);
        let Session {
            pull,
            push,
            towards: session_towards,
            away: session_away,
            ..
        } = &self.session;
        let (pull, push) = (*pull, *push);
        let towards = dot(&vector, session_towards);
        let away = dot(&vector, session_away);
        let artists = self.corpus.artists_of(id);
        let shares_artist = |set: &HashSet<i64>| artists.iter().any(|artist| set.contains(artist));
        let flag = |on: bool| on as u8 as f64;
        set(Dial::Bias, 1.0);
        set(
            Dial::CurrentLogic,
            (1.0 - pull) * best_group + pull * dot(&vector, &self.steered),
        );
        set(Dial::Towards, towards);
        set(Dial::Away, away);
        set(Dial::TowardsTimesPull, towards * pull);
        set(Dial::AwayTimesPush, away * push);
        set(Dial::BestGroup, best_group);
        set(Dial::AllLikes, dot(&vector, &self.liked.all_likes));
        set(Dial::ScoreLevel, self.corpus.score_level(id).unwrap_or(0.5));
        set(Dial::Newness, id as f64 / self.newest);
        set(Dial::Gif, flag(self.corpus.media(id) == Some(Media::Gif)));
        set(
            Dial::Video,
            flag(self.corpus.media(id) == Some(Media::Video)),
        );
        let rating = self.corpus.rating_of(id);
        set(Dial::Explicit, flag(rating == Some(Rating::Explicit)));
        set(
            Dial::Questionable,
            flag(rating == Some(Rating::Questionable)),
        );
        set(Dial::ArtistSeen, flag(shares_artist(&self.seen_artists)));
        set(
            Dial::ArtistLookedAt,
            flag(shares_artist(&self.looked_at_artists)),
        );
        set(
            Dial::ArtistSkipped,
            flag(shares_artist(&self.skipped_artists)),
        );
        set(
            Dial::LastWasSkip,
            flag(matches!(self.last_view, Some(Reacted::QuickSkip))),
        );
        set(
            Dial::LastWasLongLook,
            flag(matches!(self.last_view, Some(Reacted::LongLook))),
        );
        set(Dial::ViewsThisSession, (self.views as f64).ln_1p());
        for (key, weight) in &vector {
            if let Some(dimension) = tag_vectors::dimension_of(*key) {
                features[DIALS + dimension] = *weight;
                continue;
            }
            let dial = match self.corpus.tags.get(key).and_then(|info| info.kind) {
                Some(TagKind::Artist) => Dial::ArtistMatch,
                Some(TagKind::Character) => Dial::CharacterMatch,
                Some(TagKind::Copyright) => Dial::SeriesMatch,
                _ => Dial::GeneralMatch,
            };
            features[dial as usize] +=
                weight * self.liked.all_likes.get(key).copied().unwrap_or(0.0);
        }
        Some(features)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Graded {
    model: f64,
    current: f64,
    engaged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    learned: u64,
    graded: usize,
    needed: usize,
    model: Option<f64>,
    current: Option<f64>,
    trust: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Model {
    weights: Vec<f64>,
    squared: Vec<f64>,
    mean: Vec<f64>,
    spread: Vec<f64>,
    examples: u64,
    graded: VecDeque<Graded>,
    #[serde(default)]
    pub learned_until: i64,
    #[serde(default)]
    pub tags: TagPreference,
}

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

fn dot_slices(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn auc(mut scored: Vec<(f64, bool)>) -> Option<f64> {
    let positives = scored.iter().filter(|(_, engaged)| *engaged).count();
    let negatives = scored.len() - positives;
    if positives == 0 || negatives == 0 {
        return None;
    }
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut rank_sum = 0.0;
    let mut start = 0;
    while start < scored.len() {
        let end = start
            + scored[start..]
                .iter()
                .take_while(|(score, _)| *score == scored[start].0)
                .count();
        let tied_positives = scored[start..end]
            .iter()
            .filter(|(_, engaged)| *engaged)
            .count();
        rank_sum += tied_positives as f64 * (start + end + 1) as f64 / 2.0;
        start = end;
    }
    let lowest = (positives * (positives + 1)) as f64 / 2.0;
    Some((rank_sum - lowest) / (positives * negatives) as f64)
}

impl Model {
    pub fn restore(text: &str) -> Model {
        serde_json::from_str::<Model>(text)
            .ok()
            .filter(|model| model.weights.len() == feature_count())
            .unwrap_or_else(Model::fresh)
    }

    pub fn fresh() -> Model {
        let size = feature_count();
        let mut weights = vec![0.0; size];
        weights[Dial::CurrentLogic as usize] = STARTING_TRUST;
        Model {
            weights,
            squared: vec![MIN_SQUARED; size],
            mean: vec![0.0; size],
            spread: vec![1.0; size],
            examples: 0,
            graded: VecDeque::new(),
            learned_until: 0,
            tags: TagPreference::default(),
        }
    }

    fn scaled(&self, features: &[f64]) -> Vec<f64> {
        let mut scaled: Vec<f64> = features
            .iter()
            .zip(&self.mean)
            .zip(&self.spread)
            .map(|((value, mean), spread)| (value - mean) / spread)
            .collect();
        scaled[Dial::Bias as usize] = 1.0;
        scaled
    }

    fn predict(&self, features: &[f64]) -> f64 {
        sigmoid(dot_slices(&self.weights, &self.scaled(features)))
    }

    pub fn study(&mut self, features: &[f64], engaged: bool, importance: f64) {
        let scaled = self.scaled(features);
        let prediction = sigmoid(dot_slices(&self.weights, &scaled));
        self.graded.push_back(Graded {
            model: prediction,
            current: features[Dial::CurrentLogic as usize],
            engaged,
        });
        if self.graded.len() > GRADING_WINDOW {
            self.graded.pop_front();
        }
        let error = prediction - engaged as u8 as f64;
        self.examples += 1;
        let count = self.examples as f64;
        for (index, value) in features.iter().enumerate() {
            let delta = value - self.mean[index];
            self.mean[index] += delta / count;
            let variance = self.spread[index].powi(2);
            let updated = variance + (delta * (value - self.mean[index]) - variance) / count;
            self.spread[index] = updated.max(MIN_VARIANCE).sqrt();
            let gradient = importance * error * scaled[index] + L2 * self.weights[index];
            self.squared[index] += gradient * gradient;
            self.weights[index] -= LEARNING_RATE * gradient / self.squared[index].sqrt();
        }
    }

    fn accuracy(&self) -> (Option<f64>, Option<f64>) {
        let graded = |score: fn(&Graded) -> f64| {
            auc(self
                .graded
                .iter()
                .map(|graded| (score(graded), graded.engaged))
                .collect())
        };
        (
            graded(|graded| graded.model),
            graded(|graded| graded.current),
        )
    }

    fn trust(&self) -> f64 {
        if self.graded.len() < MIN_GRADED {
            return 0.0;
        }
        match self.accuracy() {
            (Some(model), Some(current)) => ((model - current) / TRUST_MARGIN).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    pub fn status(&self) -> ModelStatus {
        let (model, current) = self.accuracy();
        ModelStatus {
            learned: self.examples,
            graded: self.graded.len(),
            needed: MIN_GRADED,
            model,
            current,
            trust: self.trust(),
        }
    }

    pub fn rerank(&self, context: &Context, ids: Vec<i64>) -> Vec<i64> {
        let trust = self.trust();
        if trust <= 0.0 || ids.len() < 2 {
            return ids;
        }
        let predictions: Vec<f64> = ids
            .iter()
            .map(|id| {
                context
                    .features(*id)
                    .map_or(0.0, |features| self.predict(&features))
            })
            .collect();
        let mut by_prediction: Vec<usize> = (0..ids.len()).collect();
        by_prediction.sort_by(|a, b| predictions[*a].total_cmp(&predictions[*b]));
        let last = (ids.len() - 1) as f64;
        let mut model_rank = vec![0.0; ids.len()];
        for (rank, position) in by_prediction.into_iter().enumerate() {
            model_rank[position] = rank as f64 / last;
        }
        let mut keyed: Vec<(f64, i64)> = ids
            .iter()
            .enumerate()
            .map(|(position, id)| {
                let current_rank = 1.0 - position as f64 / last;
                (
                    (1.0 - trust) * current_rank + trust * model_rank[position],
                    *id,
                )
            })
            .collect();
        keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
        keyed.into_iter().map(|(_, id)| id).collect()
    }
}

pub fn learn_preference(preference: &mut TagPreference, corpus: &Corpus, logged: &Logged) {
    let looked = match logged.source {
        Some(Source::Feed) => FEED_LOOK_WEIGHT,
        Some(Source::Viewer) => VIEWER_LOOK_WEIGHT,
        _ => 0.0,
    };
    let (engaged, importance) = match logged.happened {
        Happened::Like => (true, REACTION_WEIGHT),
        Happened::Dislike => (false, REACTION_WEIGHT),
        Happened::View if logged.long_look && looked > 0.0 => (true, looked),
        Happened::View if logged.quick_skip && looked > 0.0 => (false, looked),
        _ => return,
    };
    let tags = corpus.tag_names_of(logged.post);
    if !tags.is_empty() {
        preference.study(&tags, engaged, importance);
    }
}

pub fn outcome(logged: &[Logged], index: usize) -> Option<(bool, f64)> {
    let view = &logged[index];
    let reacted: Vec<Happened> = logged
        .iter()
        .filter(|other| other.post == view.post && (other.at - view.at).abs() <= REACTION_WINDOW_MS)
        .map(|other| other.happened)
        .collect();
    if reacted.contains(&Happened::Like) {
        Some((true, REACTION_WEIGHT))
    } else if reacted.contains(&Happened::Dislike) {
        Some((false, REACTION_WEIGHT))
    } else if reacted.contains(&Happened::Engaged) {
        Some((true, ENGAGED_WEIGHT))
    } else if view.long_look {
        Some((true, 1.0))
    } else if view.quick_skip {
        Some((false, 1.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auc_ranks_and_ties() {
        assert_eq!(auc(vec![(0.1, false), (0.9, true)]), Some(1.0));
        assert_eq!(auc(vec![(0.9, false), (0.1, true)]), Some(0.0));
        assert_eq!(auc(vec![(0.5, false), (0.5, true)]), Some(0.5));
        assert_eq!(auc(vec![(0.5, true), (0.7, true)]), None);
    }

    #[test]
    fn learns_which_features_go_with_engagement() {
        let mut model = Model::fresh();
        let mut with_gif = vec![0.0; feature_count()];
        with_gif[Dial::Gif as usize] = 1.0;
        let without_gif = vec![0.0; feature_count()];
        for _ in 0..200 {
            model.study(&with_gif, true, 1.0);
            model.study(&without_gif, false, 1.0);
        }
        assert!(model.predict(&with_gif) > model.predict(&without_gif) + 0.2);
    }

    #[test]
    fn earns_trust_only_by_beating_the_current_logic() {
        let mut model = Model::fresh();
        assert_eq!(model.trust(), 0.0);
        model.graded = (0..MIN_GRADED - 1)
            .map(|index| Graded {
                model: (index % 2) as f64,
                current: 0.5,
                engaged: index % 2 == 1,
            })
            .collect();
        assert_eq!(model.trust(), 0.0);
        model.graded.push_back(Graded {
            model: 1.0,
            current: 0.5,
            engaged: true,
        });
        assert_eq!(model.trust(), 1.0);
        for graded in &mut model.graded {
            graded.model = 0.5;
        }
        assert_eq!(model.trust(), 0.0);
    }

    #[test]
    fn untrusted_model_keeps_the_current_order() {
        let model = Model::fresh();
        let corpus = Corpus::default();
        let liked = Liked {
            groups: Vec::new(),
            all_likes: Vector::new(),
        };
        let context = Context::new(&corpus, &liked, &[]);
        assert_eq!(model.rerank(&context, vec![3, 1, 2]), [3, 1, 2]);
    }

    #[test]
    fn reactions_near_a_view_decide_its_outcome() {
        let logged = |at: i64, happened: Happened, long_look: bool| Logged {
            at,
            happened,
            post: 7,
            source: Some(crate::activity::Source::Feed),
            quick_skip: !long_look,
            long_look,
        };
        let liked = [
            logged(0, Happened::View, false),
            logged(5_000, Happened::Like, false),
        ];
        assert_eq!(outcome(&liked, 0), Some((true, REACTION_WEIGHT)));
        let skipped = [logged(0, Happened::View, false)];
        assert_eq!(outcome(&skipped, 0), Some((false, 1.0)));
        let long_ago = [
            logged(0, Happened::View, true),
            logged(600_000, Happened::Dislike, false),
        ];
        assert_eq!(outcome(&long_ago, 0), Some((true, 1.0)));
    }
}
