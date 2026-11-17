use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// a gap this long without activity ends a visit
pub const SESSION_GAP_MS: i64 = 30 * 60_000;
// the most recent events read when rebuilding a visit
pub const SESSION_EVENTS: usize = 300;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Feed,
    Grid,
    Viewer,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Feed => "feed",
            Source::Grid => "grid",
            Source::Viewer => "viewer",
        }
    }

    pub fn has_dwell(self) -> bool {
        self != Source::Grid
    }

    pub fn parse(name: &str) -> Option<Source> {
        [Source::Feed, Source::Grid, Source::Viewer]
            .into_iter()
            .find(|source| source.name() == name)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Reacted {
    Liked,
    Disliked,
    Engaged,
    Searched,
    LongLook,
    QuickSkip,
    ViewerLongLook,
    ViewerQuickSkip,
    Viewed,
}

impl Reacted {
    pub fn is_view(self) -> bool {
        matches!(
            self,
            Reacted::LongLook
                | Reacted::QuickSkip
                | Reacted::ViewerLongLook
                | Reacted::ViewerQuickSkip
                | Reacted::Viewed
        )
    }
}

#[derive(Serialize, Deserialize)]
pub enum Subject {
    Post(i64),
    Search(String),
}

#[derive(Serialize, Deserialize)]
pub struct Moment {
    pub subject: Subject,
    pub reacted: Reacted,
}

impl Moment {
    pub fn post(&self) -> Option<i64> {
        match self.subject {
            Subject::Post(post) => Some(post),
            Subject::Search(_) => None,
        }
    }
}

pub struct Searched {
    pub at: i64,
    pub query: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Happened {
    View,
    Like,
    Unlike,
    Dislike,
    Undislike,
    Engaged,
}

pub struct Logged {
    pub at: i64,
    pub happened: Happened,
    pub post: i64,
    pub source: Option<Source>,
    pub quick_skip: bool,
    pub long_look: bool,
}

impl Logged {
    pub fn is_feed_view(&self) -> bool {
        self.happened == Happened::View && self.source == Some(Source::Feed)
    }
}

pub fn moments<'a>(
    newest_first: impl Iterator<Item = &'a Logged>,
    searches: &[Searched],
    now: i64,
) -> Vec<Moment> {
    let mut moments = Vec::new();
    let mut settled = HashSet::new();
    let mut later = now;
    let mut searches = searches.iter().peekable();
    let mut add_searches = |moments: &mut Vec<Moment>, later: &mut i64, until: i64| {
        while let Some(search) = searches.next_if(|search| search.at >= until) {
            if *later - search.at > SESSION_GAP_MS {
                return false;
            }
            *later = search.at;
            moments.push(Moment {
                subject: Subject::Search(search.query.clone()),
                reacted: Reacted::Searched,
            });
        }
        true
    };
    for logged in newest_first.take(SESSION_EVENTS) {
        if !add_searches(&mut moments, &mut later, logged.at) || later - logged.at > SESSION_GAP_MS
        {
            return moments;
        }
        later = logged.at;
        let viewer = logged.source == Some(Source::Viewer);
        let reacted = match logged.happened {
            Happened::View if logged.long_look && viewer => Reacted::ViewerLongLook,
            Happened::View if logged.quick_skip && viewer => Reacted::ViewerQuickSkip,
            Happened::View if logged.long_look => Reacted::LongLook,
            Happened::View if logged.quick_skip => Reacted::QuickSkip,
            Happened::View => Reacted::Viewed,
            Happened::Engaged => Reacted::Engaged,
            _ if !settled.insert(logged.post) => continue,
            Happened::Like => Reacted::Liked,
            Happened::Dislike => Reacted::Disliked,
            Happened::Unlike | Happened::Undislike => continue,
        };
        moments.push(Moment {
            subject: Subject::Post(logged.post),
            reacted,
        });
    }
    add_searches(&mut moments, &mut later, i64::MIN);
    moments
}
