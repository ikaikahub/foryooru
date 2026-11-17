use crate::activity;
use crate::error::Result;
use crate::impressions;
use crate::library::{self, TagList};
use crate::payoff;
use rusqlite::Connection;
use std::collections::HashSet;

pub use foryooru_core::recommend::*;

pub fn load_user(conn: &Connection) -> Result<UserData> {
    let likes = library::likes(conn)?;
    let dislikes = library::dislikes(conn)?;
    let reacted: HashSet<i64> = likes
        .iter()
        .chain(&dislikes)
        .map(|reaction| reaction.id)
        .collect();
    Ok(UserData {
        signals: impressions::read_signals(conn, &reacted)?,
        payoff: payoff::read(conn)?,
        blacklist: library::tag_names(conn, TagList::Blacklist)?,
        session: activity::session(conn)?,
        likes,
        dislikes,
    })
}
