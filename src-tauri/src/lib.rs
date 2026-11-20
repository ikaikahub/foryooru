use foryooru_core::{engine, feed, search};
mod activity;
mod backup;
mod booru;
mod catalogue;
#[cfg(desktop)]
mod clipboard;
mod commands;
mod core;
mod corpus;
mod danbooru;
mod db;
mod error;
mod family;
mod fetching;
mod gallery;
mod gelbooru;
mod gelbooru_login;
mod images;
mod import;
mod impressions;
mod library;
mod limiter;
mod media;
mod payoff;
mod platform;
mod progress;
mod queue;
mod recommend;
mod storage;
mod sync;
mod tables;
mod timing;
#[cfg(target_os = "linux")]
mod video_server;

use crate::core::{Sites, Startup};
use tauri::Manager;
use tauri::http::{Response, StatusCode};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(clipboard::SystemClipboard::default());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(gallery::plugin())
        .plugin(gelbooru_login::plugin());

    builder
        .manage(Startup::default())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("img", |context, request, responder| {
            let Some(core) = context
                .app_handle()
                .try_state::<Sites>()
                .map(|sites| sites.current())
            else {
                let busy = Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Vec::new())
                    .unwrap_or_default();
                responder.respond(busy);
                return;
            };
            tauri::async_runtime::spawn(async move {
                responder.respond(media::respond(core, request).await)
            });
        })
        .setup(|app| {
            app.manage(Sites::start(app.handle())?);
            #[cfg(target_os = "linux")]
            let video_base = Some(video_server::start(app.handle().clone())?);
            #[cfg(not(target_os = "linux"))]
            let video_base = None;
            app.manage(media::VideoBase(video_base));
            app.state::<Startup>().finish();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::wait_until_ready,
            commands::browse,
            commands::for_you,
            commands::similar,
            commands::for_you_more,
            commands::similar_more,
            commands::feed_next,
            commands::posts,
            commands::post_detail,
            commands::open_post,
            commands::post_family,
            commands::file_size,
            commands::toggle_like,
            commands::toggle_dislike,
            commands::set_site_favourite,
            commands::gelbooru_login,
            commands::gelbooru_logout,
            commands::liked_posts,
            commands::disliked_posts,
            commands::clear_likes,
            commands::suggest_tags,
            commands::tags_named,
            commands::tag_list,
            commands::toggle_tag,
            commands::add_tags,
            commands::saved_searches,
            commands::toggle_saved_search,
            commands::record_impressions,
            commands::record_signal,
            commands::record_signals,
            commands::record_search,
            commands::activity,
            commands::import_status,
            commands::import_favourites,
            commands::import_from_file,
            commands::storage_usage,
            commands::table_sizes,
            commands::table_page,
            commands::timings,
            commands::set_timings,
            commands::reset_database,
            commands::reset_all_databases,
            commands::clear_cache,
            commands::export_data,
            commands::import_data,
            commands::set_cache_limit,
            commands::save_sample,
            commands::save_original,
            #[cfg(desktop)]
            commands::save_original_as,
            #[cfg(any(desktop, target_os = "android"))]
            commands::copy_image,
            commands::set_site,
            commands::video_base,
            commands::pick_save_folder,
            commands::default_save_folder,
            commands::account,
            commands::sync_status,
            commands::sign_up,
            commands::log_in,
            commands::log_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
