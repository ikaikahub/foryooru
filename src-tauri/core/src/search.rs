use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    Score,
    Popular,
    Newest,
    Random,
}

pub struct SiteQuery<'a> {
    pub tags: Vec<String>,
    pub ratings: Ratings,
    pub blacklist: &'a [String],
    pub order: Option<Order>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    General,
    Sensitive,
    Questionable,
    Explicit,
}

impl Rating {
    pub const ALL: [Rating; 4] = [
        Rating::General,
        Rating::Sensitive,
        Rating::Questionable,
        Rating::Explicit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Rating::General => "general",
            Rating::Sensitive => "sensitive",
            Rating::Questionable => "questionable",
            Rating::Explicit => "explicit",
        }
    }

    pub fn parse(name: &str) -> Option<Rating> {
        Rating::ALL.into_iter().find(|rating| rating.name() == name)
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Ratings(u8);

impl Ratings {
    pub const NONE: Ratings = Ratings(0);
    pub const ALL: Ratings = Ratings(0b1111);

    pub fn contains(self, rating: Rating) -> bool {
        self.0 & rating.bit() != 0
    }

    fn with(self, rating: Rating) -> Ratings {
        Ratings(self.0 | rating.bit())
    }

    fn without(self, other: Ratings) -> Ratings {
        Ratings(self.0 & !other.0)
    }

    fn covered_by_chip(value: &str) -> Ratings {
        Rating::parse(value).map_or(Ratings::NONE, |rating| Ratings::NONE.with(rating))
    }
}

const SIMILAR: &str = "similar:";
const RATING: &str = "rating:";
const SCORE: &str = "score:";
const ID: &str = "id:";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Comparison {
    Less,
    LessOrEqual,
    Equal,
    NotEqual,
    GreaterOrEqual,
    Greater,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bound {
    comparison: Comparison,
    value: i64,
}

impl Bound {
    fn parse(text: &str) -> Option<Bound> {
        let (comparison, number) = [
            (">=", Comparison::GreaterOrEqual),
            ("<=", Comparison::LessOrEqual),
            (">", Comparison::Greater),
            ("<", Comparison::Less),
            ("=", Comparison::Equal),
        ]
        .into_iter()
        .find_map(|(prefix, comparison)| Some((comparison, text.strip_prefix(prefix)?)))
        .unwrap_or((Comparison::Equal, text));
        Some(Bound {
            comparison,
            value: number.parse().ok()?,
        })
    }

    fn negated(self) -> Bound {
        let comparison = match self.comparison {
            Comparison::Less => Comparison::GreaterOrEqual,
            Comparison::LessOrEqual => Comparison::Greater,
            Comparison::Equal => Comparison::NotEqual,
            Comparison::NotEqual => Comparison::Equal,
            Comparison::GreaterOrEqual => Comparison::Less,
            Comparison::Greater => Comparison::LessOrEqual,
        };
        Bound { comparison, ..self }
    }

    pub fn allows(self, value: i64) -> bool {
        match self.comparison {
            Comparison::Less => value < self.value,
            Comparison::LessOrEqual => value <= self.value,
            Comparison::Equal => value == self.value,
            Comparison::NotEqual => value != self.value,
            Comparison::GreaterOrEqual => value >= self.value,
            Comparison::Greater => value > self.value,
        }
    }
}

fn numeric_bound(name: &str, key: &str) -> Option<Bound> {
    Bound::parse(name.strip_prefix(key)?)
}
pub const MAX_QUERY_EXCLUDES: usize = 25;

pub fn similar_id(chip: &str) -> Option<i64> {
    chip.trim_start_matches('-')
        .strip_prefix(SIMILAR)?
        .parse()
        .ok()
}

const SORT_PREFIXES: [&str; 2] = ["sort", "order"];
const SORTS: [(&str, Order); 4] = [
    ("score", Order::Score),
    ("popular", Order::Popular),
    ("new", Order::Newest),
    ("random", Order::Random),
];

fn sort_of(chip: &str) -> Option<Order> {
    let (prefix, value) = chip.split_once(':')?;
    if !SORT_PREFIXES.contains(&prefix) {
        return None;
    }
    SORTS
        .iter()
        .find(|(name, _)| *name == value)
        .map(|(_, order)| *order)
}

pub fn meta_completions(typed: &str) -> Option<Vec<String>> {
    let (prefix, value) = typed.split_once(':')?;
    let values: Vec<&str> = if SORT_PREFIXES.contains(&prefix) {
        SORTS.iter().map(|(name, _)| *name).collect()
    } else if format!("{prefix}:") == RATING {
        Rating::ALL.iter().map(|rating| rating.name()).collect()
    } else {
        return None;
    };
    Some(
        values
            .into_iter()
            .filter(|name| name.starts_with(value))
            .map(|name| format!("{prefix}:{name}"))
            .collect(),
    )
}

const META_TAGS: [&str; 20] = [
    "sort", "order", "score", "id", "width", "height", "user", "fav", "md5", "source", "date",
    "age", "pool", "parent", "status", "filetype", "favcount", "tagcount", "mpixels", "limit",
];

fn is_meta_tag(name: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(prefix, _)| META_TAGS.contains(&prefix.to_ascii_lowercase().as_str()))
}

pub fn normalize_query(query: &str) -> String {
    let mut words: Vec<&str> = query.split_whitespace().collect();
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

#[derive(Default)]
pub struct Search {
    pub chips: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub similar_towards: Vec<i64>,
    pub similar_away: Vec<i64>,
    pub scores: Vec<Bound>,
    pub ids: Vec<Bound>,
    pub sort: Option<Order>,
    ratings_in: Option<Ratings>,
    ratings_out: Ratings,
}

impl Search {
    pub fn parse(chips: &[String]) -> Search {
        let mut search = Search {
            chips: chips.to_vec(),
            ..Search::default()
        };
        for chip in chips {
            let negated = chip.starts_with('-');
            let name = chip.trim_start_matches('-');
            if let Some(value) = name.strip_prefix(RATING) {
                let covered = Ratings::covered_by_chip(value);
                if negated {
                    search.ratings_out = Ratings(search.ratings_out.0 | covered.0);
                } else {
                    search.ratings_in = Some(Ratings(
                        search.ratings_in.unwrap_or(Ratings::NONE).0 | covered.0,
                    ));
                }
            } else if let Some(id) = similar_id(name) {
                if negated {
                    search.similar_away.push(id)
                } else {
                    search.similar_towards.push(id)
                }
            } else if let Some(bound) = numeric_bound(name, SCORE) {
                search
                    .scores
                    .push(if negated { bound.negated() } else { bound });
            } else if let Some(sort) = sort_of(chip) {
                search.sort = Some(sort);
            } else if let Some(bound) = numeric_bound(name, ID) {
                search
                    .ids
                    .push(if negated { bound.negated() } else { bound });
            } else if !is_meta_tag(name) {
                if negated {
                    search.exclude.push(name.to_owned())
                } else {
                    search.include.push(name.to_owned())
                }
            }
        }
        search
    }

    pub fn ratings(&self) -> Ratings {
        self.ratings_in
            .unwrap_or(Ratings::ALL)
            .without(self.ratings_out)
    }

    pub fn has_rating_chips(&self) -> bool {
        self.chips
            .iter()
            .any(|chip| chip.trim_start_matches('-').starts_with(RATING))
    }

    pub fn has_similar(&self) -> bool {
        !self.similar_towards.is_empty()
    }

    pub fn has_tag(&self) -> bool {
        !self.include.is_empty()
    }

    pub fn site_query<'a>(&self, blacklist: &'a [String]) -> SiteQuery<'a> {
        SiteQuery {
            tags: self
                .chips
                .iter()
                .filter(|chip| {
                    similar_id(chip).is_none()
                        && sort_of(chip).is_none()
                        && !chip.trim_start_matches('-').starts_with(RATING)
                })
                .cloned()
                .collect(),
            ratings: if self.has_rating_chips() {
                self.ratings()
            } else {
                Ratings::ALL
            },
            blacklist,
            order: self.sort,
        }
    }
}

pub fn blacklist_excludes(blacklist: &[String]) -> Vec<String> {
    let mut names: Vec<&String> = blacklist.iter().collect();
    names.sort();
    names
        .into_iter()
        .take(MAX_QUERY_EXCLUDES)
        .map(|name| format!("-{name}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(query: &str) -> Search {
        Search::parse(
            &query
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn score_and_id_comparisons_filter() {
        let search = parsed("score:>=50 id:<1000 -score:>200");
        let [at_least, at_most] = search.scores[..] else {
            panic!("expected two score bounds");
        };
        assert!(at_least.allows(50) && !at_least.allows(49));
        assert!(at_most.allows(200) && !at_most.allows(201));
        assert!(search.ids[0].allows(999) && !search.ids[0].allows(1000));
    }

    #[test]
    fn a_bare_number_means_equal() {
        let search = parsed("score:7");
        assert!(search.scores[0].allows(7) && !search.scores[0].allows(8));
    }

    #[test]
    fn unreadable_numbers_are_ignored() {
        let search = parsed("score:abc cat_ears");
        assert!(search.scores.is_empty());
        assert_eq!(search.include, ["cat_ears"]);
    }

    #[test]
    fn known_sorts_are_read_from_either_prefix() {
        assert_eq!(parsed("order:popular cat_ears").sort, Some(Order::Popular));
        assert_eq!(parsed("sort:updated").sort, None);
    }

    #[test]
    fn meta_values_complete_only_after_the_colon() {
        assert_eq!(meta_completions("so"), None);
        assert_eq!(
            meta_completions("sort:p"),
            Some(vec!["sort:popular".to_owned()])
        );
        assert_eq!(
            meta_completions("rating:"),
            Some(
                Rating::ALL
                    .iter()
                    .map(|rating| format!("rating:{}", rating.name()))
                    .collect()
            )
        );
    }

    #[test]
    fn tags_with_colons_are_still_tags() {
        let search = parsed("honkai:_star_rail re:zero sort:score");
        assert_eq!(search.include, ["honkai:_star_rail", "re:zero"]);
    }
}
