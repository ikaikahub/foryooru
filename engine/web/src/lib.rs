use foryooru_core::{engine, feed, search};
mod activity;
mod backup;
mod booru;
mod catalogue;
mod commands;
mod core;
mod corpus;
mod danbooru;
mod db;
mod error;
mod family;
mod fetching;
mod gelbooru;
mod import;
mod impressions;
mod library;
mod limiter;
mod payoff;
mod platform;
mod progress;
mod queue;
mod recommend;
mod storage;
mod tables;
mod timing;

use crate::booru::Site;
use crate::commands::CurrentCore;
use crate::error::{Error, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use wasm_bindgen::prelude::*;

// The Web Worker's way in: it calls these instead of Tauri's invoke handler.

fn to_js(value: &impl Serialize) -> std::result::Result<JsValue, JsValue> {
    // plain objects rather than JS Maps, so the page sees the same shapes Tauri sent
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn error_to_js(error: Error) -> JsValue {
    to_js(&error).unwrap_or_else(|_| JsValue::from_str(&error.to_string()))
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T> {
    Ok(serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))?)
}

fn current() -> Result<CurrentCore> {
    CurrentCore::get()
}

#[wasm_bindgen]
pub async fn start(site: JsValue, settings: JsValue, emit: js_sys::Function) -> std::result::Result<(), JsValue> {
    // Rust panics show up as a bare 'unreachable' in wasm, this prints their message to the console instead
    console_error_panic_hook::set_once();
    platform::set_emitter(emit);
    set_settings(settings)?;
    let site: Site = serde_wasm_bindgen::from_value(site).unwrap_or(Site::Danbooru);
    core::start(site).await.map_err(error_to_js)
}

#[wasm_bindgen]
pub fn set_settings(settings: JsValue) -> std::result::Result<(), JsValue> {
    let settings: serde_json::Map<String, Value> =
        serde_wasm_bindgen::from_value(settings).map_err(|error| JsValue::from_str(&error.to_string()))?;
    timing::set_enabled(settings.get("debug").and_then(Value::as_bool).unwrap_or(false));
    platform::replace_settings(settings);
    Ok(())
}

#[wasm_bindgen]
pub fn export_backup(settings: JsValue) -> std::result::Result<Vec<u8>, JsValue> {
    let settings: serde_json::Map<String, Value> =
        serde_wasm_bindgen::from_value(settings).map_err(|error| JsValue::from_str(&error.to_string()))?;
    backup::export(&settings).map_err(error_to_js)
}

// returns the backup's settings for the page to apply, or null when it had none
#[wasm_bindgen]
pub fn import_backup(bytes: &[u8]) -> std::result::Result<JsValue, JsValue> {
    let settings = backup::import(bytes).map_err(error_to_js)?;
    to_js(&settings)
}

#[wasm_bindgen]
pub async fn invoke(command: String, args: JsValue) -> std::result::Result<JsValue, JsValue> {
    let args: Value = if args.is_undefined() || args.is_null() {
        Value::Null
    } else {
        serde_wasm_bindgen::from_value(args).map_err(|error| JsValue::from_str(&error.to_string()))?
    };
    let started = timing::now();
    let result = dispatch(&command, &args).await;
    timing::record("command", started.elapsed(), command.clone());
    result.map_err(error_to_js)
}

macro_rules! reply {
    ($value:expr) => {
        to_js(&$value?).map_err(|error| Error::Storage(format!("{error:?}")))
    };
}

async fn dispatch(command: &str, a: &Value) -> Result<JsValue> {
    use commands as c;
    match command {
        "browse" => reply!(c::browse(current()?, arg(a, "chips")?, arg(a, "page")?).await),
        "for_you" => reply!(c::for_you(current()?, arg(a, "chips")?, arg(a, "hideLiked")?, arg(a, "discover")?).await),
        "similar" => reply!(c::similar(current()?, arg(a, "chips")?, arg(a, "perArtist")?).await),
        "for_you_more" => reply!(
            c::for_you_more(current()?, arg(a, "chips")?, arg(a, "hideLiked")?, arg(a, "discover")?, arg(a, "shown")?)
                .await
        ),
        "similar_more" => {
            reply!(c::similar_more(current()?, arg(a, "chips")?, arg(a, "perArtist")?, arg(a, "shown")?).await)
        }
        "feed_next" => reply!(
            c::feed_next(current()?, arg(a, "chips")?, arg(a, "seen")?, arg(a, "since")?, arg(a, "options")?).await
        ),
        "posts" => reply!(c::posts(current()?, arg(a, "ids")?)),
        "post_detail" => reply!(c::post_detail(current()?, arg(a, "id")?).await),
        "open_post" => reply!(c::open_post(current()?, arg(a, "id")?).await),
        "preview_post" => reply!(c::preview_post(current()?, arg(a, "site")?, arg(a, "id")?).await),
        "post_family" => reply!(c::post_family(current()?, arg(a, "id")?, arg(a, "chips")?).await),
        "toggle_like" => reply!(c::toggle_like(current()?, arg(a, "id")?, arg(a, "chips")?)),
        "toggle_dislike" => reply!(c::toggle_dislike(current()?, arg(a, "id")?)),
        "set_site_favourite" => reply!(c::set_site_favourite(current()?, arg(a, "id")?, arg(a, "favourite")?).await),
        "liked_posts" => reply!(c::liked_posts(current()?, arg(a, "chips")?, arg(a, "page")?).await),
        "disliked_posts" => reply!(c::disliked_posts(current()?, arg(a, "chips")?, arg(a, "page")?).await),
        "clear_likes" => reply!(c::clear_likes(current()?)),
        "suggest_tags" => reply!(c::suggest_tags(current()?, arg(a, "typed")?)),
        "tags_named" => reply!(c::tags_named(current()?, arg(a, "names")?)),
        "tag_list" => reply!(c::tag_list(current()?, arg(a, "list")?)),
        "toggle_tag" => reply!(c::toggle_tag(current()?, arg(a, "list")?, arg(a, "name")?)),
        "add_tags" => reply!(c::add_tags(current()?, arg(a, "list")?, arg(a, "names")?)),
        "saved_searches" => reply!(c::saved_searches(current()?)),
        "toggle_saved_search" => reply!(c::toggle_saved_search(current()?, arg(a, "query")?)),
        "record_impressions" => reply!(c::record_impressions(current()?, arg(a, "impressions")?, arg(a, "chips")?)),
        "record_signal" => reply!(c::record_signal(current()?, arg(a, "post")?, arg(a, "signal")?, arg(a, "tag")?)),
        "record_signals" => reply!(c::record_signals(current()?, arg(a, "signals")?)),
        "record_search" => reply!(c::record_search(current()?, arg(a, "query")?)),
        "activity" => reply!(c::activity(current()?, arg(a, "filter")?, arg(a, "page")?)),
        "table_sizes" => reply!(c::table_sizes(current()?)),
        "table_page" => reply!(c::table_page(current()?, arg(a, "table")?, arg(a, "page")?)),
        "import_status" => reply!(Ok::<_, Error>(c::import_status(current()?))),
        "import_favourites" => reply!(c::import_favourites(current()?, arg(a, "user")?)),
        "import_from_file" => reply!(c::import_from_file(current()?, arg(a, "name")?, arg(a, "contents")?)),
        "set_site" => reply!(c::set_site(arg(a, "site")?)),
        "storage_usage" => reply!(c::storage_usage(current()?)),
        "timings" => reply!(Ok::<_, Error>(crate::timing::drain())),
        "reset_database" => reply!(c::reset_database(current()?).await),
        "reset_all_databases" => reply!(c::reset_all_databases(current()?).await),
        _ => Err(Error::Storage(format!("Unknown command {command}"))),
    }
}
