pub mod activity;
pub mod corpus;
pub mod engine;
pub mod feed;
pub mod fetch_plan;
pub mod interests;
pub mod kinds;
pub mod payoff;
pub mod protocol;
pub mod recommend;
pub mod search;
pub mod signals;
pub mod tag_vectors;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
