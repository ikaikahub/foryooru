use crate::engine::{Vector, weighted_sum};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize)]
pub struct Signals {
    pub scores: HashMap<i64, f64>,
    pub views: HashMap<i64, i64>,
    pub active: usize,
}

impl Signals {
    pub fn penalties(&self, seen_drop: f64) -> HashMap<i64, f64> {
        let kept = 1.0 - seen_drop.clamp(0.0, 1.0);
        self.views
            .iter()
            .filter(|(_, views)| **views > 0)
            .map(|(id, views)| (*id, kept.powi(*views as i32)))
            .collect()
    }

    pub fn profile(&self, vector_of: impl Fn(i64) -> Option<Vector>) -> Vector {
        let parts: Vec<(Vector, f64)> = self
            .scores
            .iter()
            .filter_map(|(id, score)| vector_of(*id).map(|vector| (vector, *score)))
            .collect();
        weighted_sum(parts.iter().map(|(vector, score)| (vector, *score)))
    }
}
