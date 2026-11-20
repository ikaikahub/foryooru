use crate::fetched::Fetched;
use crate::pool::Pool;
use foryooru_core::activity::{Happened, Logged, Moment, Reacted, SESSION_EVENTS, Source, Subject};
use foryooru_core::corpus::Corpus;
use foryooru_core::feed::{self, FeedOptions, Scrolled};
use foryooru_core::kinds::TagKind;
use foryooru_core::model::{Context, Liked, Model, learn_preference};
use foryooru_core::now_ms;
use foryooru_core::recommend::{self, Reaction, UserData};
use foryooru_core::search::Search;
use foryooru_core::signals::Signals;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{Rng, SeedableRng};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

const NICHE_POSTS: (i64, i64) = (3_000, 60_000);
const NICHE_SAMPLE: usize = 4_000;
const LOVED_CHARACTERS: usize = 3;
const LOVED_GENERAL: usize = 3;
const HIT_AFFINITY: f64 = 0.5;
const LIKE_CHANCE: f64 = 0.6;
const DISLIKE_CHANCE: f64 = 0.4;
const QUICK_SKIP_MS: f64 = 600.0;
const LONG_LOOK_MS: f64 = 4_000.0;
const SEED_LIKES: usize = 10;
const FOR_YOU_TOP: usize = 60;
const TRIALS: usize = 4;
const SEARCH_LIKES: usize = 3;

#[derive(Clone)]
struct Craving {
    all: Vec<i64>,
    any: Vec<i64>,
}

impl Craving {
    fn matches(&self, tags: &[i64]) -> bool {
        self.all.iter().all(|tag| tags.contains(tag))
            && self.any.iter().any(|tag| tags.contains(tag))
    }
}

#[derive(Clone)]
struct Aversion {
    tag: i64,
    unless: Vec<i64>,
}

impl Aversion {
    fn matches(&self, tags: &[i64]) -> bool {
        tags.contains(&self.tag) && !self.unless.iter().any(|tag| tags.contains(tag))
    }
}

#[derive(Clone)]
struct Persona {
    name: String,
    loved: HashMap<i64, f64>,
    avoided: HashSet<i64>,
    craving: Option<Craving>,
    aversion: Option<Aversion>,
}

impl Persona {
    fn affinity(&self, corpus: &Corpus, id: i64) -> f64 {
        let Some(row) = corpus.row_of(id) else {
            return 0.0;
        };
        let tags = corpus.tags_of_row(row);
        if self
            .craving
            .as_ref()
            .is_some_and(|craving| craving.matches(&tags))
        {
            return 1.0;
        }
        tags.iter()
            .filter_map(|tag| self.loved.get(tag))
            .sum::<f64>()
            .min(1.0)
    }

    fn avoids(&self, corpus: &Corpus, id: i64) -> bool {
        corpus.row_of(id).is_some_and(|row| {
            let tags = corpus.tags_of_row(row);
            tags.iter().any(|tag| self.avoided.contains(tag))
                || self
                    .aversion
                    .as_ref()
                    .is_some_and(|aversion| aversion.matches(&tags))
        })
    }
}

struct Viewer {
    user: UserData,
    model: Model,
    seen: HashSet<i64>,
    since: i64,
}

impl Viewer {
    fn new() -> Viewer {
        Viewer {
            user: UserData {
                likes: Vec::new(),
                dislikes: Vec::new(),
                blacklist: Vec::new(),
                signals: Signals::default(),
                payoff: Default::default(),
                session: Vec::new(),
            },
            model: Model::fresh(),
            seen: HashSet::new(),
            since: now_ms(),
        }
    }

    fn remember(&mut self, id: i64, reacted: Reacted) {
        self.user.session.insert(
            0,
            Moment {
                subject: Subject::Post(id),
                reacted,
            },
        );
        self.user.session.truncate(SESSION_EVENTS);
    }

    fn react(
        &mut self,
        corpus: &Corpus,
        persona: &Persona,
        ids: &[i64],
        rng: &mut StdRng,
    ) -> Judged {
        let mut judged = Judged::default();
        let outcome = &mut judged.outcome;
        let found = recommend::interests(corpus, &self.user);
        let liked = Liked::new(corpus, &self.user, &found);
        let features: Vec<Option<Vec<f64>>> = {
            let context = Context::new(corpus, &liked, &self.user.session);
            ids.iter().map(|id| context.features(*id)).collect()
        };
        for (id, features) in ids.iter().zip(features) {
            let affinity = persona.affinity(corpus, *id);
            let avoided = persona.avoids(corpus, *id);
            let dwell = if avoided {
                300.0
            } else {
                400.0 + affinity * 8_000.0
            };
            let likes = !avoided && affinity >= HIT_AFFINITY && rng.random_bool(LIKE_CHANCE);
            let dislikes = avoided && rng.random_bool(DISLIKE_CHANCE);
            let reacted = if likes {
                self.user.likes.insert(0, Reaction { id: *id });
                Reacted::Liked
            } else if dislikes {
                self.user.dislikes.insert(0, Reaction { id: *id });
                Reacted::Disliked
            } else if dwell <= QUICK_SKIP_MS {
                Reacted::QuickSkip
            } else if dwell >= LONG_LOOK_MS {
                Reacted::LongLook
            } else {
                Reacted::Viewed
            };
            if let Some(features) = features {
                let engaged = likes || dwell >= LONG_LOOK_MS;
                let judged = likes || dislikes || dwell <= QUICK_SKIP_MS || dwell >= LONG_LOOK_MS;
                if judged {
                    self.model.study(
                        &features,
                        engaged,
                        if likes || dislikes { 3.0 } else { 1.0 },
                    );
                }
            }
            let view = Logged {
                at: 0,
                happened: Happened::View,
                post: *id,
                source: Some(Source::Feed),
                quick_skip: dwell <= QUICK_SKIP_MS,
                long_look: dwell >= LONG_LOOK_MS,
            };
            learn_preference(&mut self.model.tags, corpus, &view);
            if likes || dislikes {
                let reaction = Logged {
                    happened: if likes {
                        Happened::Like
                    } else {
                        Happened::Dislike
                    },
                    ..view
                };
                learn_preference(&mut self.model.tags, corpus, &reaction);
            }
            *self.user.signals.views.entry(*id).or_default() += 1;
            self.seen.insert(*id);
            self.remember(*id, reacted);
            if likes {
                judged.liked.push(*id);
            }
            if dwell >= LONG_LOOK_MS {
                judged.long_looks.push(*id);
            }
            outcome.shown += 1;
            outcome.hits += (affinity >= HIT_AFFINITY) as usize;
            outcome.likes += likes as usize;
            outcome.avoided += avoided as usize;
            outcome.affinity += affinity;
        }
        judged
    }

    fn feed_batch(&self, corpus: &Corpus, discover: bool) -> Vec<i64> {
        self.batch(corpus, discover).ids
    }

    fn batch(&self, corpus: &Corpus, discover: bool) -> feed::Batch {
        feed::next_batch(
            corpus,
            &self.user,
            &Search::parse(&[]),
            Scrolled {
                seen: &self.seen,
                since: self.since,
                session_likes: &[],
            },
            FeedOptions {
                hide_liked: false,
                similar_per_artist: None,
                discover,
            },
            &self.model,
        )
    }
}

#[derive(Default)]
struct Judged {
    outcome: Outcome,
    liked: Vec<i64>,
    long_looks: Vec<i64>,
}

#[derive(Default, Clone, Copy)]
struct Outcome {
    shown: usize,
    hits: usize,
    likes: usize,
    avoided: usize,
    affinity: f64,
}

impl Outcome {
    fn add(&mut self, other: Outcome) {
        self.shown += other.shown;
        self.hits += other.hits;
        self.likes += other.likes;
        self.avoided += other.avoided;
        self.affinity += other.affinity;
    }

    fn hit_rate(self) -> f64 {
        self.hits as f64 / self.shown.max(1) as f64
    }
}

fn kind_of(corpus: &Corpus, tag: i64) -> Option<TagKind> {
    corpus.tags.get(&tag).and_then(|info| info.kind)
}

fn personas(pool: &Pool, count: usize, rng: &mut StdRng) -> Vec<Persona> {
    let corpus = &pool.corpus;
    let mut copyrights: Vec<i64> = corpus
        .tags
        .iter()
        .filter(|(_, info)| {
            info.kind == Some(TagKind::Copyright)
                && info
                    .count
                    .is_some_and(|count| (NICHE_POSTS.0..=NICHE_POSTS.1).contains(&count))
        })
        .map(|(id, _)| *id)
        .collect();
    copyrights.sort_unstable();
    let mut chosen = Vec::new();
    while chosen.len() < count {
        let Some(copyright) = copyrights.choose(rng).copied() else {
            break;
        };
        let name = corpus.tag_name(copyright).unwrap_or_default().to_owned();
        let mask = corpus.search_mask(&Search::parse(std::slice::from_ref(&name)));
        let posts = corpus.ids_where(&mask);
        let sample: Vec<i64> = posts
            .choose_multiple(rng, NICHE_SAMPLE.min(posts.len()))
            .copied()
            .collect();
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for id in &sample {
            if let Some(row) = corpus.row_of(*id) {
                for tag in corpus.tags_of_row(row) {
                    *counts.entry(tag).or_default() += 1;
                }
            }
        }
        let mut ranked: Vec<(i64, usize)> = counts.into_iter().collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let characters: Vec<i64> = ranked
            .iter()
            .filter(|(tag, _)| kind_of(corpus, *tag) == Some(TagKind::Character))
            .take(LOVED_CHARACTERS)
            .map(|(tag, _)| *tag)
            .collect();
        let general: Vec<i64> = ranked
            .iter()
            .filter(|(tag, count)| {
                kind_of(corpus, *tag) == Some(TagKind::General) && *count < sample.len() / 3
            })
            .take(LOVED_GENERAL)
            .map(|(tag, _)| *tag)
            .collect();
        if characters.len() < LOVED_CHARACTERS {
            continue;
        }
        let mut loved = HashMap::from([(copyright, 0.35)]);
        for character in characters {
            loved.insert(character, 0.4);
        }
        for tag in general {
            loved.insert(tag, 0.15);
        }
        let avoided: HashSet<i64> = ["guro", "scat", "vore", "furry"]
            .iter()
            .filter_map(|name| corpus.tag_id(name))
            .collect();
        chosen.push(Persona {
            name,
            loved,
            avoided,
            craving: None,
            aversion: None,
        });
    }
    chosen
}

fn seed_likes(pool: &Pool, persona: &Persona, viewer: &mut Viewer, rng: &mut StdRng) {
    let corpus = &pool.corpus;
    let name = loved_character(corpus, persona).unwrap_or_else(|| persona.name.clone());
    let mask = corpus.search_mask(&Search::parse(&[name]));
    let liked: Vec<i64> = corpus
        .ids_where(&mask)
        .into_iter()
        .filter(|id| persona.affinity(corpus, *id) >= HIT_AFFINITY)
        .collect();
    for id in liked.choose_multiple(rng, SEED_LIKES) {
        viewer.user.likes.push(Reaction { id: *id });
    }
}

fn loved_character(corpus: &Corpus, persona: &Persona) -> Option<String> {
    persona
        .loved
        .keys()
        .filter(|tag| kind_of(corpus, **tag) == Some(TagKind::Character))
        .find_map(|tag| corpus.tag_name(*tag).map(str::to_owned))
}

fn run_feed(
    pool: &Pool,
    persona: &Persona,
    viewer: &mut Viewer,
    batches: usize,
    discover: bool,
    rng: &mut StdRng,
) -> Vec<Outcome> {
    (0..batches)
        .map(|_| {
            let started = std::time::Instant::now();
            let ids = viewer.feed_batch(&pool.corpus, discover);
            let ranked = started.elapsed().as_millis();
            let outcome = viewer.react(&pool.corpus, persona, &ids, rng).outcome;
            eprintln!(
                "batch ranked in {ranked}ms, judged in {}ms, {} hits",
                started.elapsed().as_millis() - ranked,
                outcome.hits
            );
            outcome
        })
        .collect()
}

fn for_you_precision(corpus: &Corpus, persona: &Persona, viewer: &Viewer) -> f64 {
    let ranked = recommend::for_you(
        corpus,
        &viewer.user,
        &Search::parse(&[]),
        false,
        false,
        &viewer.model,
    );
    let top: Vec<i64> = ranked.iter().take(FOR_YOU_TOP).map(|(id, _)| *id).collect();
    top.iter()
        .filter(|id| persona.affinity(corpus, **id) >= HIT_AFFINITY)
        .count() as f64
        / top.len().max(1) as f64
}

fn base_rate(pool: &Pool, persona: &Persona, rng: &mut StdRng) -> f64 {
    let ids = &pool.corpus.ids;
    let sample: Vec<i64> = ids.choose_multiple(rng, 20_000).copied().collect();
    sample
        .iter()
        .filter(|id| persona.affinity(&pool.corpus, **id) >= HIT_AFFINITY)
        .count() as f64
        / sample.len() as f64
}

fn rates(outcomes: &[Outcome], every: usize) -> String {
    outcomes
        .chunks(every)
        .map(|chunk| {
            let mut total = Outcome::default();
            for outcome in chunk {
                total.add(*outcome);
            }
            format!("{:.0}%", total.hit_rate() * 100.0)
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

pub fn run(pool: &Pool, count: usize, batches: usize) -> String {
    let mut rng = StdRng::seed_from_u64(7);
    let mut report = String::new();
    let chosen = personas(pool, count, &mut rng);
    let every = (batches / 6).max(1);
    for persona in &chosen {
        let section = simulate_persona(pool, persona, &chosen, batches, every, &mut rng);
        eprint!("{section}");
        report.push_str(&section);
    }
    for persona in &chosen {
        let section = adaptation(pool, persona, batches, every, &mut rng);
        eprint!("{section}");
        report.push_str(&section);
    }
    for persona in &chosen {
        let section = preferences(pool, persona, batches, &mut rng);
        eprint!("{section}");
        report.push_str(&section);
    }
    for persona in &chosen {
        let section = fetching(pool, persona, batches, every, &mut rng);
        eprint!("{section}");
        report.push_str(&section);
    }
    report
}

fn add_each(totals: &mut [Outcome], outcomes: Vec<Outcome>) {
    for (total, outcome) in totals.iter_mut().zip(outcomes) {
        total.add(outcome);
    }
}

fn run_fetched<'a>(
    pool: &'a Pool,
    persona: &Persona,
    seeds: &[i64],
    batches: usize,
    rng: &mut StdRng,
) -> (Vec<Outcome>, f64, Fetched<'a>) {
    let mut world = Fetched::new(&pool.corpus);
    world.store(seeds.iter().filter_map(|id| pool.corpus.row_of(*id)));
    for id in seeds {
        world.after_like(*id);
    }
    world.run_pending(rng);
    let mut viewer = Viewer::new();
    viewer.user.likes = seeds.iter().map(|id| Reaction { id: *id }).collect();
    let outcomes = (0..batches)
        .map(|_| {
            let mut batch = viewer.batch(&world.local, false);
            if batch.exhausted {
                world.refill(batch.leads, viewer.user.likes.len(), rng);
                world.run_pending(rng);
                batch = viewer.batch(&world.local, false);
            }
            world.refill(batch.leads, viewer.user.likes.len(), rng);
            let judged = viewer.react(&world.local, persona, &batch.ids, rng);
            for id in &judged.liked {
                world.after_like(*id);
            }
            world.after_long_looks(&judged.long_looks);
            world.run_pending(rng);
            judged.outcome
        })
        .collect();
    let precision = for_you_precision(&world.local, persona, &viewer);
    (outcomes, precision, world)
}

fn fetching(
    pool: &Pool,
    persona: &Persona,
    batches: usize,
    every: usize,
    rng: &mut StdRng,
) -> String {
    let mut pooled = vec![Outcome::default(); batches];
    let mut fetched = vec![Outcome::default(); batches];
    let (mut pooled_precision, mut fetched_precision) = (0.0, 0.0);
    let (mut posts, mut requests) = (0, 0);
    for _ in 0..TRIALS {
        let mut viewer = Viewer::new();
        seed_likes(pool, persona, &mut viewer, rng);
        let seeds: Vec<i64> = viewer.user.likes.iter().map(|like| like.id).collect();
        add_each(
            &mut pooled,
            run_feed(pool, persona, &mut viewer, batches, false, rng),
        );
        pooled_precision += for_you_precision(&pool.corpus, persona, &viewer);
        let (outcomes, precision, world) = run_fetched(pool, persona, &seeds, batches, rng);
        add_each(&mut fetched, outcomes);
        fetched_precision += precision;
        posts += world.local.size();
        requests += world.requests;
    }
    let trials = TRIALS as f64;
    let mut section = String::new();
    let _ = writeln!(section);
    let _ = writeln!(
        section,
        "### {} on Gelbooru: empty db vs the full dump ({TRIALS} trials, {SEED_LIKES} seed likes)",
        persona.name
    );
    let _ = writeln!(
        section,
        "- full dump: TikTok hit rate {}; For You precision@{FOR_YOU_TOP} afterwards {:.0}%",
        rates(&pooled, every),
        pooled_precision / trials * 100.0
    );
    let _ = writeln!(
        section,
        "- empty db: TikTok hit rate {}; For You precision@{FOR_YOU_TOP} afterwards {:.0}%; {} posts from {} requests",
        rates(&fetched, every),
        fetched_precision / trials * 100.0,
        posts / TRIALS,
        requests / TRIALS
    );
    section
}

fn shares(
    pool: &Pool,
    base: &Persona,
    persona: &Persona,
    batches: usize,
    rng: &mut StdRng,
    classify: impl Fn(&[i64]) -> Option<usize>,
    labels: &[&str],
) -> String {
    let every = (batches / 6).max(1);
    let mut counts = vec![vec![0usize; labels.len()]; batches.div_ceil(every)];
    let mut shown = vec![0usize; counts.len()];
    for _ in 0..TRIALS {
        let mut viewer = history(pool, base, batches / 2, rng);
        for batch in 0..batches {
            let ids = viewer.feed_batch(&pool.corpus, false);
            let phase = batch / every;
            for id in &ids {
                shown[phase] += 1;
                let tags = pool
                    .corpus
                    .row_of(*id)
                    .map(|row| pool.corpus.tags_of_row(row))
                    .unwrap_or_default();
                if let Some(label) = classify(&tags) {
                    counts[phase][label] += 1;
                }
            }
            viewer.react(&pool.corpus, persona, &ids, rng);
        }
    }
    counts
        .iter()
        .zip(&shown)
        .map(|(phase, shown)| {
            let parts: Vec<String> = phase
                .iter()
                .zip(labels)
                .map(|(count, label)| format!("{}% {label}", count * 100 / (*shown).max(1)))
                .collect();
            format!("  {}\n", parts.join(", "))
        })
        .collect()
}

fn preferences(pool: &Pool, base: &Persona, batches: usize, rng: &mut StdRng) -> String {
    let corpus = &pool.corpus;
    let mut section = String::new();
    if let (Some(boy), Some(trap)) = (corpus.tag_id("1boy"), corpus.tag_id("trap")) {
        let persona = Persona {
            aversion: Some(Aversion {
                tag: boy,
                unless: vec![trap],
            }),
            ..base.clone()
        };
        let _ = writeln!(section);
        let _ = writeln!(
            section,
            "### {} dislikes 1boy unless it's also trap ({TRIALS} trials)",
            base.name
        );
        section.push_str(&shares(
            pool,
            base,
            &persona,
            batches,
            rng,
            |tags| match (tags.contains(&boy), tags.contains(&trap)) {
                (true, false) => Some(0),
                (true, true) => Some(1),
                _ => None,
            },
            &["1boy without trap", "1boy with trap"],
        ));
    }
    if let Some(glasses) = corpus.tag_id("glasses") {
        let mut persona = base.clone();
        persona.loved.insert(glasses, 0.6);
        let _ = writeln!(section);
        let _ = writeln!(
            section,
            "### {} also loves glasses ({TRIALS} trials)",
            base.name
        );
        section.push_str(&shares(
            pool,
            base,
            &persona,
            batches,
            rng,
            |tags| tags.contains(&glasses).then_some(0),
            &["glasses"],
        ));
    }
    section
}

struct NewInterest {
    craving: Craving,
    search: String,
}

fn new_interest(corpus: &Corpus) -> Option<NewInterest> {
    let tag = |name: &str| corpus.tag_id(name);
    Some(NewInterest {
        craving: Craving {
            all: vec![tag("furry_female")?],
            any: vec![tag("pokemon")?, tag("zenless_zone_zero")?],
        },
        search: "furry_female pokemon".to_owned(),
    })
}

#[derive(Default, Clone, Copy)]
struct Mix {
    shown: usize,
    target: usize,
    furry_elsewhere: usize,
    series_without: usize,
    old: usize,
}

impl Mix {
    fn add(&mut self, other: Mix) {
        self.shown += other.shown;
        self.target += other.target;
        self.furry_elsewhere += other.furry_elsewhere;
        self.series_without += other.series_without;
        self.old += other.old;
    }

    fn describe(self) -> String {
        let share = |count: usize| count * 100 / self.shown.max(1);
        format!(
            "{}% target, {}% other furry, {}% series without furry, {}% old taste",
            share(self.target),
            share(self.furry_elsewhere),
            share(self.series_without),
            share(self.old)
        )
    }
}

fn mix_of(pool: &Pool, base: &Persona, craving: &Craving, ids: &[i64]) -> Mix {
    let mut mix = Mix::default();
    for id in ids {
        let Some(row) = pool.corpus.row_of(*id) else {
            continue;
        };
        let tags = pool.corpus.tags_of_row(row);
        let furry = craving.all.iter().all(|tag| tags.contains(tag));
        let series = craving.any.iter().any(|tag| tags.contains(tag));
        mix.shown += 1;
        mix.target += (furry && series) as usize;
        mix.furry_elsewhere += (furry && !series) as usize;
        mix.series_without += (!furry && series) as usize;
        mix.old += (base.affinity(&pool.corpus, *id) >= HIT_AFFINITY) as usize;
    }
    mix
}

fn history(pool: &Pool, base: &Persona, batches: usize, rng: &mut StdRng) -> Viewer {
    let mut viewer = Viewer::new();
    seed_likes(pool, base, &mut viewer, rng);
    run_feed(pool, base, &mut viewer, batches, false, rng);
    viewer
}

fn follow(
    pool: &Pool,
    base: &Persona,
    craving_persona: &Persona,
    viewer: &mut Viewer,
    batches: usize,
    rng: &mut StdRng,
) -> Vec<Mix> {
    let Some(craving) = &craving_persona.craving else {
        return Vec::new();
    };
    (0..batches)
        .map(|_| {
            let ids = viewer.feed_batch(&pool.corpus, false);
            let mix = mix_of(pool, base, craving, &ids);
            viewer.react(&pool.corpus, craving_persona, &ids, rng);
            mix
        })
        .collect()
}

fn summarize(trials: &[Vec<Mix>], every: usize) -> String {
    let reached: Vec<usize> = trials
        .iter()
        .filter_map(|mixes| mixes.iter().position(|mix| mix.target > 0))
        .collect();
    let first = if reached.is_empty() {
        "never".to_owned()
    } else {
        format!(
            "in {}/{} trials, on average at batch {:.0}",
            reached.len(),
            trials.len(),
            reached.iter().map(|batch| *batch as f64 + 1.0).sum::<f64>() / reached.len() as f64
        )
    };
    let batches = trials.first().map_or(0, Vec::len);
    let phases: Vec<String> = (0..batches)
        .step_by(every)
        .map(|start| {
            let mut total = Mix::default();
            for mixes in trials {
                for mix in mixes.iter().skip(start).take(every) {
                    total.add(*mix);
                }
            }
            total.describe()
        })
        .collect();
    format!("  first target post: {first}\n  {}\n", phases.join("\n  "))
}

fn searched(
    pool: &Pool,
    base: &Persona,
    interest: &NewInterest,
    batches: usize,
    rng: &mut StdRng,
) -> Viewer {
    let mut viewer = history(pool, base, batches / 2, rng);
    viewer.user.session.insert(
        0,
        Moment {
            subject: Subject::Search(interest.search.clone()),
            reacted: Reacted::Searched,
        },
    );
    let chips: Vec<String> = interest.search.split(' ').map(str::to_owned).collect();
    let found = pool
        .corpus
        .ids_where(&pool.corpus.search_mask(&Search::parse(&chips)));
    for id in found.choose_multiple(rng, SEARCH_LIKES) {
        viewer.user.likes.insert(0, Reaction { id: *id });
        viewer.remember(*id, Reacted::Liked);
    }
    viewer
}

fn adaptation(
    pool: &Pool,
    base: &Persona,
    batches: usize,
    every: usize,
    rng: &mut StdRng,
) -> String {
    let Some(interest) = new_interest(&pool.corpus) else {
        return "new interest tags missing from the pool\n".to_owned();
    };
    let furry = pool.corpus.tag_id("furry");
    let unaware = Persona {
        avoided: base
            .avoided
            .iter()
            .copied()
            .filter(|tag| Some(*tag) != furry)
            .collect(),
        ..base.clone()
    };
    let craving_persona = Persona {
        craving: Some(interest.craving.clone()),
        ..unaware.clone()
    };
    let mut section = String::new();
    let _ = writeln!(section);
    let _ = writeln!(
        section,
        "### {} suddenly wants furry_female, only from pokemon or zenless_zone_zero ({TRIALS} trials each)",
        base.name
    );
    let exploring: Vec<Vec<Mix>> = (0..TRIALS)
        .map(|_| {
            let mut viewer = history(pool, &unaware, batches / 2, rng);
            follow(pool, base, &craving_persona, &mut viewer, batches, rng)
        })
        .collect();
    let _ = writeln!(section, "- found only through the feed:");
    section.push_str(&summarize(&exploring, every));
    let searching: Vec<Vec<Mix>> = (0..TRIALS)
        .map(|_| {
            let mut viewer = searched(pool, &unaware, &interest, batches, rng);
            follow(pool, base, &craving_persona, &mut viewer, batches, rng)
        })
        .collect();
    let _ = writeln!(
        section,
        "- after searching \"{}\" and liking {SEARCH_LIKES} results:",
        interest.search
    );
    section.push_str(&summarize(&searching, every));
    section
}

fn simulate_persona(
    pool: &Pool,
    persona: &Persona,
    chosen: &[Persona],
    batches: usize,
    every: usize,
    rng: &mut StdRng,
) -> String {
    let mut section = String::new();
    let started = std::time::Instant::now();
    let loved: Vec<String> = persona
        .loved
        .keys()
        .filter_map(|tag| pool.corpus.tag_name(*tag).map(str::to_owned))
        .collect();
    let base = base_rate(pool, persona, rng);
    let _ = writeln!(section);
    let _ = writeln!(section, "### {}", persona.name);
    let _ = writeln!(section, "loves: {}", loved.join(", "));
    let _ = writeln!(section, "random post hit rate: {:.2}%", base * 100.0);

    let mut cold = Viewer::new();
    let cold_run = run_feed(pool, persona, &mut cold, batches, false, rng);
    let first_hit = cold_run.iter().position(|outcome| outcome.hits > 0);
    let _ = writeln!(
        section,
        "- cold start TikTok, hit rate per {every} batches: {}",
        rates(&cold_run, every)
    );
    let _ = writeln!(
        section,
        "  first hit in batch {}; likes {}; For You precision@{FOR_YOU_TOP} afterwards {:.0}%; model {}",
        first_hit.map_or("never".to_owned(), |batch| (batch + 1).to_string()),
        cold.user.likes.len(),
        for_you_precision(&pool.corpus, persona, &cold) * 100.0,
        serde_json::to_string(&cold.model.status()).unwrap_or_default(),
    );

    let mut warm = Viewer::new();
    seed_likes(pool, persona, &mut warm, rng);
    let warm_start = for_you_precision(&pool.corpus, persona, &warm);
    let warm_run = run_feed(pool, persona, &mut warm, batches, false, rng);
    let _ = writeln!(
        section,
        "- {SEED_LIKES} seed likes: For You precision@{FOR_YOU_TOP} {:.0}% at start; TikTok hit rate {}",
        warm_start * 100.0,
        rates(&warm_run, every),
    );

    let mut discovering = Viewer::new();
    seed_likes(pool, persona, &mut discovering, rng);
    let discover_run = run_feed(pool, persona, &mut discovering, batches, true, rng);
    let _ = writeln!(
        section,
        "- same seed likes with Discover: TikTok hit rate {}",
        rates(&discover_run, every)
    );

    if let Some(next) = chosen.iter().find(|other| other.name != persona.name) {
        let shift = run_feed(pool, next, &mut warm, batches, false, rng);
        let _ = writeln!(
            section,
            "- taste shift to {} after that: hit rate {}",
            next.name,
            rates(&shift, every)
        );
    }
    let _ = writeln!(section, "- took {:.0}s", started.elapsed().as_secs_f64());
    section
}
