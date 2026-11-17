use crate::activity::{self, Logged};
use crate::core::Core;
use crate::corpus::Corpus;
use crate::error::Result;
use crate::library;
use crate::recommend;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Arc;

pub use foryooru_core::model::*;

pub fn load(conn: &Connection) -> Result<Model> {
    let saved: Option<String> = conn
        .query_row("SELECT state FROM model_state WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(saved.map_or_else(Model::fresh, |text| Model::restore(&text)))
}

fn save(conn: &Connection, model: &Model) -> Result<()> {
    conn.execute(
        "INSERT INTO model_state (id, state) VALUES (1, ?1) ON CONFLICT (id) DO UPDATE SET state = excluded.state",
        [serde_json::to_string(model)?],
    )?;
    Ok(())
}

fn catch_up(conn: &Connection, corpus: &Corpus) -> Result<()> {
    let mut model = load(conn)?;
    let logged = activity::logged_after(conn, model.learned_until)?;
    let Some(first) = logged
        .iter()
        .position(|event| event.at > model.learned_until)
    else {
        return Ok(());
    };
    for event in &logged[first..] {
        learn_preference(&mut model.tags, corpus, event);
    }
    if let Some(first_view) = logged[first..].iter().position(Logged::is_feed_view) {
        study_views(&mut model, conn, corpus, &logged, first + first_view)?;
    }
    model.learned_until = logged[logged.len() - 1].at;
    save(conn, &model)
}

fn study_views(
    model: &mut Model,
    conn: &Connection,
    corpus: &Corpus,
    logged: &[Logged],
    first: usize,
) -> Result<()> {
    let mut user = recommend::load_user(conn)?;
    user.likes = library::likes_before(conn, logged[first].at)?;
    user.dislikes = library::dislikes_before(conn, logged[first].at)?;
    let found = recommend::interests(corpus, &user);
    let liked = Liked::new(corpus, &user, &found);
    for index in first..logged.len() {
        let view = &logged[index];
        if !view.is_feed_view() {
            continue;
        }
        let Some((engaged, importance)) = outcome(logged, index) else {
            continue;
        };
        let moments = activity::moments(logged[..index].iter().rev(), &[], view.at);
        let context = Context::new(corpus, &liked, &moments);
        if let Some(features) = context.features(view.post) {
            model.study(&features, engaged, importance);
        }
    }
    Ok(())
}

pub fn learn_in_background(core: Arc<Core>) {
    crate::platform::spawn(async move {
        let Ok(_learning) = core.learning.try_lock() else {
            return;
        };
        let learned = core
            .corpus
            .get(&core.db)
            .and_then(|corpus| catch_up(&core.db.connect()?, &corpus));
        if let Err(error) = learned {
            crate::platform::log(&format!("Learning from your views failed: {error}"));
        }
    });
}
