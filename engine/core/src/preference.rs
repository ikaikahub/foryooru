use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// step size when a tag's learned weight changes
const LEARNING_RATE: f64 = 0.1;
// pulls weights of rarely seen tags back towards zero
const L2: f64 = 0.01;
// no single tag can push a post up or down more than this
const MAX_WEIGHT: f64 = 3.0;
// keeps the learning maths from dividing by zero
const MIN_SQUARED: f64 = 1e-6;
// tags with smaller learned weights are ignored when ranking
pub const MEANINGFUL: f64 = 0.05;

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
struct Learned {
    weight: f64,
    squared: f64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TagPreference {
    bias: Learned,
    tags: HashMap<String, Learned>,
}

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

fn step(learned: &mut Learned, gradient: f64) {
    learned.squared += gradient * gradient;
    learned.weight -= LEARNING_RATE * gradient / learned.squared.max(MIN_SQUARED).sqrt();
    learned.weight = learned.weight.clamp(-MAX_WEIGHT, MAX_WEIGHT);
}

impl TagPreference {
    pub fn study(&mut self, tags: &[&str], engaged: bool, importance: f64) {
        let total = self.bias.weight
            + tags
                .iter()
                .filter_map(|tag| self.tags.get(*tag))
                .map(|learned| learned.weight)
                .sum::<f64>();
        let error = (sigmoid(total) - if engaged { 1.0 } else { 0.0 }) * importance;
        step(&mut self.bias, error);
        for tag in tags {
            let learned = self.tags.entry((*tag).to_owned()).or_default();
            step(learned, error + L2 * learned.weight);
        }
    }

    pub fn weights(&self) -> impl Iterator<Item = (&str, f64)> {
        self.tags
            .iter()
            .filter(|(_, learned)| learned.weight.abs() >= MEANINGFUL)
            .map(|(tag, learned)| (tag.as_str(), learned.weight))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_common_tag_that_keeps_getting_disliked_turns_negative() {
        let mut preference = TagPreference::default();
        for round in 0..60 {
            preference.study(&["1boy", "solo", &format!("artist_{round}")], false, 3.0);
            preference.study(&["1girl", "solo", &format!("other_{round}")], true, 3.0);
        }
        let weight = |tag: &str| {
            preference
                .tags
                .get(tag)
                .map_or(0.0, |learned| learned.weight)
        };
        assert!(weight("1boy") < -1.0, "1boy {}", weight("1boy"));
        assert!(weight("1girl") > 1.0, "1girl {}", weight("1girl"));
        assert!(weight("solo").abs() < weight("1boy").abs());
    }

    #[test]
    fn little_evidence_stays_small() {
        let mut preference = TagPreference::default();
        preference.study(&["glasses"], false, 1.0);
        assert!(preference.tags["glasses"].weight.abs() < 0.2);
    }
}
