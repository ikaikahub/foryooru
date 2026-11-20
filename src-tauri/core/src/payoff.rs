use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize)]
pub struct Payoff(pub HashMap<String, f64>);

impl Payoff {
    pub fn factor(&self, tag: &str) -> f64 {
        self.0.get(tag).copied().unwrap_or(1.0)
    }
}
