// Opt-in timing records for profiling on a device without a debugger. Off unless the debug setting is on.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const KEPT: usize = 500;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    // seconds since the engine started, so a copied report reads in order
    pub at: f64,
    pub name: &'static str,
    pub ms: f64,
    pub note: String,
}

static ENABLED: AtomicBool = AtomicBool::new(false);

struct Kept {
    started: Instant,
    spans: VecDeque<Span>,
}

fn kept() -> &'static Mutex<Kept> {
    static KEPT_SPANS: OnceLock<Mutex<Kept>> = OnceLock::new();
    KEPT_SPANS.get_or_init(|| {
        Mutex::new(Kept {
            started: Instant::now(),
            spans: VecDeque::with_capacity(KEPT),
        })
    })
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        kept()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .spans
            .clear();
    }
}

pub fn now() -> Instant {
    Instant::now()
}

pub fn record(name: &'static str, took: Duration, note: impl Into<String>) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let mut kept = kept().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if kept.spans.len() == KEPT {
        kept.spans.pop_front();
    }
    let at = kept.started.elapsed().as_secs_f64();
    kept.spans.push_back(Span {
        at,
        name,
        ms: took.as_secs_f64() * 1000.0,
        note: note.into(),
    });
}

// measures a whole scope, including any awaits inside it
pub fn measure(name: &'static str, note: impl Into<String>) -> Measure {
    Measure {
        name,
        note: note.into(),
        started: Instant::now(),
    }
}

pub struct Measure {
    name: &'static str,
    note: String,
    started: Instant,
}

impl Drop for Measure {
    fn drop(&mut self) {
        record(self.name, self.started.elapsed(), std::mem::take(&mut self.note));
    }
}

// hands the collected spans to the page and starts a fresh buffer
pub fn drain() -> Vec<Span> {
    kept()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .spans
        .drain(..)
        .collect()
}
