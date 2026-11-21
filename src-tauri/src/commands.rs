use crate::activity::{self, ActivityPage};
use crate::autocomplete;
use crate::backup;
use crate::booru::{Order, Site};
use crate::catalogue::{self, Post, PostDetail, SEARCH_PAGE_MAX_AGE, Tag};
use crate::core::{Core, CurrentCore, Startup};
use crate::corpus::Corpus;
use crate::db::now_ms;
use crate::engine;
use crate::error::{Error, Result};
use crate::family::{self, Relation};
use crate::feed::{self, FeedOptions};
use crate::fetching;
use crate::gallery;
use crate::images::content_type;
use crate::import::{self, ImportStatus};
use crate::impressions::{self, Impression};
use crate::library::{
    self, ArtistRow, FollowedOrder, LISTING_PAGE_SIZE, Listing, Matching, PostState, SavedSearch, TagList,
};
use crate::media;
use crate::recommend::{self, RANKING_LIMIT, UserData};
use crate::search::{self, Rating, Search};
use crate::storage::{self, StorageUsage};
use crate::sync::{self, SyncStatus};
use crate::tables::{self, TablePage, TableSize};
use crate::timing;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tauri::{AppHandle, State};

// ranking is CPU-heavy, so it runs on the blocking pool instead of the command thread
async fn with_corpus<T: Send + 'static>(
    core: &Arc<Core>,
    work: impl FnOnce(&Corpus, &UserData) -> T + Send + 'static,
) -> Result<T> {
    let core = core.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = timing::now();
        let corpus = core.corpus.get(&core.db)?;
        timing::record("corpus", started.elapsed(), "");
        let user = recommend::load_user(&core.db.conn())?;
        Ok(work(&corpus, &user))
    })
    .await?
}

#[tauri::command]
pub async fn browse(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let core = core.inner();
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let search = Search::parse(&chips);
    let (site, query) = core.route(&search.site_query(&blacklist), page).await?;
    let stored = catalogue::search_page(core, site, &query, page, SEARCH_PAGE_MAX_AGE).await?;
    fetching::seed_popular(core);
    let conn = core.db.conn();
    let (kept, hidden) = library::without_blacklisted(&conn, stored.ids)?;
    let mut posts = catalogue::posts(&conn, &kept)?;
    if search.sort == Some(Order::Popular) && !core.booru(site).ranks_popular() {
        let now = now_ms();
        posts.sort_by(|a, b| {
            engine::hotness(b.score, b.posted_at, now).total_cmp(&engine::hotness(
                a.score,
                a.posted_at,
                now,
            ))
        });
    }
    Ok(Listing {
        posts,
        pages: ((stored.total.min(core.booru(site).max_offset()) + LISTING_PAGE_SIZE - 1)
            / LISTING_PAGE_SIZE)
            .max(1),
        hidden,
    })
}

#[derive(Serialize)]
pub struct Ranked {
    id: i64,
    explore: bool,
}

#[derive(Serialize)]
pub struct MoreRanked {
    ranked: Vec<Ranked>,
    broadened: Option<Vec<String>>,
}

const MORE_AT_LEAST: usize = 20;

#[tauri::command]
pub async fn for_you_more(
    core: CurrentCore,
    chips: Vec<String>,
    hide_liked: bool,
    discover: bool,
    shown: Vec<i64>,
) -> Result<MoreRanked> {
    let core = core.inner();
    let shown: HashSet<i64> = shown.into_iter().collect();
    let rank = |chips: Vec<String>| {
        let shown = shown.clone();
        with_corpus(core, move |corpus, user| {
            recommend::for_you(
                corpus,
                user,
                &Search::parse(&chips),
                hide_liked,
                discover,
            )
            .into_iter()
            .filter(|(id, _)| !shown.contains(id))
            .map(|(id, explore)| Ranked { id, explore })
            .collect::<Vec<_>>()
        })
    };
    let fill = async |chips: &[String]| -> Result<Vec<Ranked>> {
        let ranked = rank(chips.to_vec()).await?;
        if ranked.len() >= MORE_AT_LEAST {
            return Ok(ranked);
        }
        let search_chips = chips.to_vec();
        let leads = with_corpus(core, move |corpus, user| {
            feed::leads_for(corpus, user, &Search::parse(&search_chips), hide_liked)
        })
        .await?;
        if let Err(error) = fetching::fetch_more_now(core, chips, leads).await {
            crate::platform::log(&format!("Fetching more for For you failed: {error}"));
        }
        rank(chips.to_vec()).await
    };
    let mut ranked = fill(&chips).await?;
    let mut broadened = None;
    if ranked.len() < MORE_AT_LEAST {
        let (current, unseen) = (chips.clone(), shown.clone());
        let wider = with_corpus(core, move |corpus, user| {
            feed::broaden(corpus, user, &current, &unseen, hide_liked)
        })
        .await?;
        if let Some(wider) = wider {
            ranked = fill(&wider).await?;
            broadened = Some(wider);
        }
    }
    Ok(MoreRanked { ranked, broadened })
}

#[tauri::command]
pub async fn similar_more(
    core: CurrentCore,
    chips: Vec<String>,
    per_artist: Option<usize>,
    shown: Vec<i64>,
) -> Result<Vec<i64>> {
    let core = core.inner();
    let shown: HashSet<i64> = shown.into_iter().collect();
    let rank = || {
        let (chips, shown) = (chips.clone(), shown.clone());
        with_corpus(core, move |corpus, user| {
            recommend::similar(
                corpus,
                user,
                &Search::parse(&chips),
                per_artist,
                RANKING_LIMIT,
            )
            .map(|ranked| {
                ranked
                    .into_iter()
                    .filter(|id| !shown.contains(id))
                    .collect::<Vec<_>>()
            })
        })
    };
    let ranked = rank().await?.ok_or(Error::PostNotFound)?;
    if ranked.len() >= MORE_AT_LEAST {
        return Ok(ranked);
    }
    if let Err(error) = fetching::fetch_more_similar(core, &Search::parse(&chips)).await {
        crate::platform::log(&format!("Fetching more similar posts failed: {error}"));
    }
    rank().await?.ok_or(Error::PostNotFound)
}

#[tauri::command]
pub async fn for_you(
    core: CurrentCore,
    chips: Vec<String>,
    hide_liked: bool,
    discover: bool,
) -> Result<Vec<Ranked>> {
    let search = Search::parse(&chips);
    fetching::for_similar(&core, &search).await;
    let ranking = with_corpus(core.inner(), move |corpus, user| {
        recommend::for_you(corpus, user, &search, hide_liked, discover)
    })
    .await?;
    Ok(ranking
        .into_iter()
        .map(|(id, explore)| Ranked { id, explore })
        .collect())
}

#[tauri::command]
pub async fn similar(
    core: CurrentCore,
    chips: Vec<String>,
    per_artist: Option<usize>,
) -> Result<Vec<i64>> {
    let search = Search::parse(&chips);
    fetching::for_similar(&core, &search).await;
    with_corpus(core.inner(), move |corpus, user| {
        recommend::similar(corpus, user, &search, per_artist, RANKING_LIMIT)
    })
    .await?
    .ok_or(Error::PostNotFound)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedBatch {
    posts: Vec<PostDetail>,
    exhausted: bool,
    broadened: Option<Vec<String>>,
}

#[tauri::command]
pub async fn feed_next(
    core: CurrentCore,
    chips: Vec<String>,
    seen: Vec<i64>,
    since: i64,
    options: FeedOptions,
) -> Result<FeedBatch> {
    let core = core.inner();
    let search = Search::parse(&chips);
    let session_likes = if search.has_similar() {
        library::likes_since(&core.db.conn(), since)?
    } else {
        Vec::new()
    };
    if seen.is_empty() {
        fetching::for_similar(core, &search).await;
    }
    let seen: HashSet<i64> = seen.into_iter().collect();
    let hide_liked = options.hide_liked;
    let rank = |chips: Vec<String>| {
        let (seen, session_likes, options) = (seen.clone(), session_likes.clone(), options.clone());
        with_corpus(core, move |corpus, user| {
            feed::next_batch(
                corpus,
                user,
                &Search::parse(&chips),
                feed::Scrolled {
                    seen: &seen,
                    since,
                    session_likes: &session_likes,
                },
                options,
            )
        })
    };
    let fill = async |chips: &[String]| -> Result<feed::Batch> {
        let batch = rank(chips.to_vec()).await?;
        if !batch.exhausted {
            return Ok(batch);
        }
        if let Err(error) = fetching::fetch_more_now(core, chips, batch.leads.clone()).await {
            crate::platform::log(&format!("Fetching more for TikTok failed: {error}"));
        }
        rank(chips.to_vec()).await
    };
    let mut chips = chips;
    let mut broadened = None;
    let mut batch = fill(&chips).await?;
    if batch.exhausted {
        let (current, unseen) = (chips.clone(), seen.clone());
        let wider = with_corpus(core, move |corpus, user| {
            feed::broaden(corpus, user, &current, &unseen, hide_liked)
        })
        .await?;
        if let Some(wider) = wider {
            batch = fill(&wider).await?;
            chips = wider;
            broadened = Some(chips.clone());
        }
    }
    fetching::after_batch(core, chips, batch.leads);
    Ok(FeedBatch {
        posts: catalogue::post_details(&core.db.conn(), &batch.ids)?,
        exhausted: batch.exhausted,
        broadened,
    })
}

#[tauri::command]
pub fn posts(core: CurrentCore, ids: Vec<i64>) -> Result<Vec<Post>> {
    catalogue::posts(&core.db.conn(), &ids)
}

#[derive(Serialize)]
pub struct PostFamily {
    relation: Relation,
    posts: Vec<Post>,
}

const SIMILAR_RELATIVES: usize = 10;

#[tauri::command]
pub async fn post_family(core: CurrentCore, id: i64, chips: Vec<String>) -> Result<PostFamily> {
    let core = core.inner();
    let ratings = Search::parse(&chips).ratings();
    let members = match family::members(core, id).await {
        Ok(members) => members,
        Err(error) => {
            crate::platform::log(&format!("Finding the family of post {id} failed: {error}"));
            None
        }
    };
    let (relation, ids) = match members {
        Some(members) => (Relation::Family, members),
        None => {
            let similar = Search::parse(&[format!("similar:{id}")]);
            let ranked = with_corpus(core, move |corpus, user| {
                recommend::similar(corpus, user, &similar, None, SIMILAR_RELATIVES * 2)
                    .unwrap_or_default()
            })
            .await?;
            let ids = [id]
                .into_iter()
                .chain(ranked.into_iter().filter(|other| *other != id))
                .take(SIMILAR_RELATIVES * 2)
                .collect();
            (Relation::Similar, ids)
        }
    };
    let conn = core.db.conn();
    let (kept, _) = library::without_blacklisted(&conn, ids)?;
    let mut posts: Vec<Post> = catalogue::posts(&conn, &kept)?
        .into_iter()
        .filter(|post| Rating::parse(&post.rating).is_some_and(|rating| ratings.contains(rating)))
        .collect();
    if let Relation::Similar = relation {
        posts.truncate(SIMILAR_RELATIVES + 1);
    }
    Ok(PostFamily { relation, posts })
}

#[tauri::command]
pub async fn post_detail(core: CurrentCore, id: i64) -> Result<PostDetail> {
    if let Err(error) = media::complete_partial(core.inner(), id).await {
        crate::platform::log(&format!("Couldn't complete post {id}: {error}"));
    }
    catalogue::post_details(&core.db.conn(), &[id])?
        .pop()
        .ok_or(Error::PostNotFound)
}

// opens one post by id, for an address the feed was left at
#[tauri::command]
pub async fn open_post(core: CurrentCore, id: i64) -> Result<PostDetail> {
    post_detail(core, id).await
}

#[tauri::command]
pub fn toggle_like(core: CurrentCore, id: i64, chips: Vec<String>) -> Result<PostState> {
    let state = library::toggle_like(&mut core.db.conn(), id)?;
    sync::sync_soon(core.inner());
    if state.liked {
        fetching::after_like(core.inner(), id, chips);
    }
    Ok(state)
}

// favourites the post on every site the user is logged in to that has it. a site that hasn't sent the post is asked
// for it by md5 first. taking a favourite back only needs the ids already known, as those are the ones favourited
#[tauri::command]
pub async fn set_site_favourite(core: CurrentCore, id: i64, favourite: bool) -> Result<()> {
    let ids = catalogue::site_ids(&core.db.conn(), id)?;
    let mut failure = None;
    for site in core.logged_in_sites() {
        let site_id = match (ids.get(site), favourite) {
            (Some(site_id), _) => Some(site_id),
            (None, true) => match catalogue::find_on_site(core.inner(), id, site).await {
                Ok(site_id) => site_id,
                Err(error) => {
                    failure = Some(error);
                    None
                }
            },
            (None, false) => None,
        };
        let Some(site_id) = site_id else {
            continue;
        };
        if let Err(error) = core.booru(site).set_favourite(site_id, favourite).await {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

#[tauri::command]
pub fn toggle_dislike(core: CurrentCore, id: i64) -> Result<PostState> {
    let state = library::toggle_dislike(&mut core.db.conn(), id)?;
    sync::sync_soon(core.inner());
    Ok(state)
}

#[tauri::command]
pub async fn liked_posts(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let matching = matching_posts(core.inner(), &chips).await?;
    library::liked_listing(&core.db.conn(), &matching, page)
}

#[tauri::command]
pub async fn followed_rows(core: CurrentCore, chips: Vec<String>, order: FollowedOrder) -> Result<Vec<ArtistRow>> {
    let matching = matching_posts(core.inner(), &chips).await?;
    let tagged = followed_tagged(core.inner()).await?;
    library::followed_rows(&core.db.conn(), &matching, order, &tagged)
}

#[tauri::command]
pub async fn followed_posts(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let matching = matching_posts(core.inner(), &chips).await?;
    let tagged = followed_tagged(core.inner()).await?;
    library::followed_listing(&core.db.conn(), &matching, page, &tagged)
}

async fn followed_tagged(core: &Arc<Core>) -> Result<Vec<(String, i64)>> {
    let names = library::tag_names(&core.db.conn(), TagList::FollowedArtists)?;
    with_corpus(core, move |corpus, _| corpus.posts_tagged(&names)).await
}

#[tauri::command]
pub async fn refresh_followed(core: CurrentCore, chips: Vec<String>) -> Result<()> {
    fetching::refresh_followed(core.inner(), &chips).await
}

// the artists whose stored work most matches the artist search, within the ratings of the post search, skipping ones
// already shown. the first call for a search fetches its first page, so there are posts to find artists in
#[tauri::command]
pub async fn relevant_artists(
    core: CurrentCore,
    artist_chips: Vec<String>,
    chips: Vec<String>,
    skip: Vec<String>,
    limit: usize,
) -> Result<Vec<String>> {
    let core = core.inner();
    let ratings = Search::parse(&chips).ratings();
    if skip.is_empty() {
        let rating_chips = chips.iter().filter(|chip| chip.trim_start_matches('-').starts_with("rating:")).cloned();
        let fetched: Vec<String> = artist_chips.iter().cloned().chain(rating_chips).collect();
        if let Err(error) = fetching::fetch_search_page(core, &fetched).await {
            crate::platform::log(&format!("Fetching the search for the artists page failed: {error}"));
        }
    }
    // a search for one artist shows that artist, then the artists their fans also like
    if let [single] = artist_chips.as_slice()
        && !single.starts_with('-')
    {
        let similar = foryooru_core::artist_neighbours::bundled().of(single);
        if !similar.is_empty() {
            let skip: HashSet<&str> = skip.iter().map(String::as_str).collect();
            return Ok(std::iter::once(single.as_str())
                .chain(similar)
                .filter(|name| !skip.contains(name))
                .take(limit)
                .map(str::to_owned)
                .collect());
        }
    }
    let search = Search::parse(&artist_chips);
    with_corpus(core, move |corpus, _| {
        let skip: HashSet<String> = skip.into_iter().collect();
        let matching = corpus.search_mask(&search);
        let rated = corpus.rating_mask(ratings);
        corpus
            .signature_artists(&matching, &rated)
            .into_iter()
            .filter_map(|tag| corpus.tag_name(tag).map(str::to_owned))
            .filter(|name| !skip.contains(name))
            .take(limit)
            .collect()
    })
    .await
}

// each named artist's newest posts that match the post search, in the order given
#[tauri::command]
pub async fn artist_rows(core: CurrentCore, chips: Vec<String>, names: Vec<String>) -> Result<Vec<ArtistRow>> {
    let matching = matching_posts(core.inner(), &chips).await?;
    let wanted = names.clone();
    let tagged = with_corpus(core.inner(), move |corpus, _| corpus.posts_tagged(&wanted)).await?;
    library::artist_rows(&core.db.conn(), &matching, names, &tagged)
}

#[tauri::command]
pub async fn refresh_artists(core: CurrentCore, chips: Vec<String>, names: Vec<String>) -> Result<()> {
    fetching::refresh_artists(core.inner(), &names, &chips).await
}

#[tauri::command]
pub async fn disliked_posts(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let matching = matching_posts(core.inner(), &chips).await?;
    library::disliked_listing(&core.db.conn(), &matching, page)
}

async fn matching_posts(core: &Arc<Core>, chips: &[String]) -> Result<Matching> {
    let search = Search::parse(chips);
    let mut hasher = DefaultHasher::new();
    chips.hash(&mut hasher);
    let seed = hasher.finish();
    with_corpus(core, move |corpus, _| {
        let mask = corpus.search_mask(&search);
        match search.sort {
            Some(order) => Matching::SortedBy(corpus.sort_keys(order, &mask, seed)),
            None => Matching::InOrder(corpus.ids_where(&mask).into_iter().collect()),
        }
    })
    .await
}

#[tauri::command]
pub fn clear_likes(core: CurrentCore) -> Result<()> {
    library::clear_likes(&core.db.conn())?;
    sync::sync_soon(core.inner());
    Ok(())
}

#[tauri::command]
pub fn suggest_tags(core: CurrentCore, typed: String) -> Result<Vec<Tag>> {
    match search::meta_completions(&typed) {
        Some(names) => Ok(names
            .into_iter()
            .map(|name| Tag {
                name,
                kind: None,
                count: None,
            })
            .collect()),
        None => catalogue::suggest(&core.db.conn(), &typed),
    }
}

// the sites' own autocomplete, to add to suggest_tags once it answers; nothing for meta tags like sort:
#[tauri::command]
pub async fn suggest_tags_online(core: CurrentCore, typed: String) -> Result<Vec<autocomplete::Suggestion>> {
    if search::meta_completions(&typed).is_some() {
        return Ok(Vec::new());
    }
    let site = if core.can_search(Site::Gelbooru) {
        Site::Gelbooru
    } else {
        Site::Danbooru
    };
    Ok(autocomplete::suggest(&core.inner().http, site, &typed).await)
}

#[tauri::command]
pub fn tags_named(core: CurrentCore, names: Vec<String>) -> Result<Vec<Tag>> {
    catalogue::tags_named(&core.db.conn(), &names)
}

#[tauri::command]
pub fn tag_list(core: CurrentCore, list: TagList) -> Result<Vec<Tag>> {
    library::tags(&core.db.conn(), list)
}

#[tauri::command]
pub fn toggle_tag(core: CurrentCore, list: TagList, name: String) -> Result<Vec<Tag>> {
    let conn = core.db.conn();
    library::toggle_tag(&conn, list, &name)?;
    sync::sync_soon(core.inner());
    library::tags(&conn, list)
}

#[tauri::command]
pub fn add_tags(core: CurrentCore, list: TagList, names: String) -> Result<Vec<Tag>> {
    let conn = core.db.conn();
    library::add_tags(&conn, list, &names)?;
    sync::sync_soon(core.inner());
    library::tags(&conn, list)
}

#[tauri::command]
pub fn saved_searches(core: CurrentCore) -> Result<Vec<SavedSearch>> {
    library::saved_searches(&core.db.conn())
}

#[tauri::command]
pub fn toggle_saved_search(core: CurrentCore, query: String) -> Result<Vec<SavedSearch>> {
    let conn = core.db.conn();
    library::toggle_saved_search(&conn, &query)?;
    sync::sync_soon(core.inner());
    library::saved_searches(&conn)
}

#[tauri::command]
pub fn record_impressions(core: CurrentCore, impressions: Vec<Impression>) -> Result<()> {
    impressions::record(&mut core.db.conn(), &impressions)
}

#[tauri::command]
pub fn record_signal(
    core: CurrentCore,
    post: i64,
    signal: activity::Signal,
    tag: Option<String>,
) -> Result<()> {
    activity::record_signal(&core.db.conn(), post, signal, tag.as_deref())
}

#[tauri::command]
pub fn record_signals(core: CurrentCore, signals: Vec<activity::SignalEntry>) -> Result<()> {
    activity::record_signals(&core.db.conn(), &signals)
}

#[tauri::command]
pub fn record_search(core: CurrentCore, query: String) -> Result<()> {
    activity::record_search(&core.db.conn(), &query)
}

#[tauri::command]
pub fn activity(core: CurrentCore, filter: activity::Filter, page: i64) -> Result<ActivityPage> {
    activity::page(&core.db.conn(), filter, page)
}

#[tauri::command]
pub fn table_sizes(core: CurrentCore) -> Result<Vec<TableSize>> {
    tables::sizes(&core.db.conn())
}

#[tauri::command]
pub fn table_page(core: CurrentCore, table: String, page: i64) -> Result<TablePage> {
    tables::page(&core.db.conn(), &table, page)
}

#[tauri::command]
pub fn import_status(core: CurrentCore) -> ImportStatus {
    core.import.get()
}

#[tauri::command]
pub fn import_favourites(core: CurrentCore, site: Site, user: String) -> Result<()> {
    import::favourites(core.inner(), site, user)
}

#[tauri::command]
pub fn account(app: AppHandle) -> Option<String> {
    sync::username(&app)
}

#[tauri::command]
pub fn sync_status(core: CurrentCore) -> Result<SyncStatus> {
    sync::status(&core)
}

#[tauri::command]
pub async fn sign_up(core: CurrentCore, username: String, password: String) -> Result<String> {
    sync::log_in(core.inner(), &username, &password, true).await
}

#[tauri::command]
pub async fn log_in(core: CurrentCore, username: String, password: String) -> Result<String> {
    sync::log_in(core.inner(), &username, &password, false).await
}

#[tauri::command]
pub async fn log_out(core: CurrentCore) -> Result<()> {
    sync::log_out(&core).await
}

#[tauri::command]
pub fn import_from_file(core: CurrentCore, name: String, contents: String) -> Result<()> {
    import::from_file(core.inner(), name, &contents)
}

#[tauri::command]
pub fn storage_usage(core: CurrentCore) -> Result<StorageUsage> {
    storage::usage(&core)
}

#[tauri::command]
pub fn timings() -> Vec<crate::timing::Span> {
    crate::timing::drain()
}

#[tauri::command]
pub fn set_timings(enabled: bool) {
    crate::timing::set_enabled(enabled);
}

#[tauri::command]
pub async fn wait_until_ready(startup: State<'_, Startup>) -> Result<()> {
    startup.wait().await;
    Ok(())
}

#[tauri::command]
pub async fn file_size(core: CurrentCore, id: i64) -> Result<Option<u64>> {
    let url = media::original_url(core.inner(), id).await?;
    if url.is_empty() {
        return Err(Error::PostNotFound);
    }
    core.images.content_length(&url).await
}

#[tauri::command]
pub async fn gelbooru_login(app: AppHandle) -> Result<Option<crate::gelbooru::GelbooruSession>> {
    crate::gelbooru_login::login(&app).await
}

#[tauri::command]
pub async fn gelbooru_logout(app: AppHandle) -> Result<()> {
    crate::gelbooru_login::logout(&app).await
}


#[tauri::command]
pub async fn pick_save_folder(app: AppHandle) -> Result<Option<gallery::SaveFolder>> {
    gallery::pick_folder(&app).await
}

#[tauri::command]
pub fn default_save_folder(app: AppHandle) -> Result<String> {
    gallery::default_label(&app)
}

#[tauri::command]
pub fn video_base(base: State<'_, media::VideoBase>) -> Option<String> {
    base.0.clone()
}


#[tauri::command]
pub async fn export_data(app: AppHandle) -> Result<gallery::SavedFile> {
    backup::export(&app).await
}

#[tauri::command]
pub async fn import_data(app: AppHandle) -> Result<bool> {
    let Some(picked) = gallery::pick_zip(&app).await? else {
        return Ok(false);
    };
    tauri::async_runtime::spawn_blocking(move || backup::import(&app, &picked.path)).await??;
    Ok(true)
}


async fn off_main_thread(
    core: &Arc<Core>,
    work: impl FnOnce(&Core) -> Result<()> + Send + 'static,
) -> Result<()> {
    let core = core.clone();
    tauri::async_runtime::spawn_blocking(move || work(&core)).await?
}

#[tauri::command]
pub async fn reset_database(core: CurrentCore) -> Result<()> {
    off_main_thread(core.inner(), storage::reset_database).await
}

#[tauri::command]
pub async fn clear_cache(core: CurrentCore) -> Result<()> {
    off_main_thread(core.inner(), storage::clear_cache).await
}

#[tauri::command]
pub fn set_cache_limit(core: CurrentCore, megabytes: u64) -> Result<()> {
    core.images.set_limit_mb(megabytes)
}


struct Original {
    url: String,
    file_name: String,
}

fn file_name_for(id: i64, url: &str) -> String {
    let extension = url
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or(content_type(url).rsplit('/').next().unwrap_or("jpg"));
    format!("{id}.{extension}")
}

async fn original(core: &Arc<Core>, id: i64) -> Result<Original> {
    let url = media::original_url(core, id).await?;
    if url.is_empty() {
        return Err(Error::PostNotFound);
    }
    Ok(Original {
        file_name: file_name_for(id, &url),
        url,
    })
}

fn clipboard_file(core: &Core, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf> {
    use tauri::Manager;
    let folder = core.app.path().app_cache_dir()?.join("clipboard");
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(name);
    std::fs::write(&file, bytes)?;
    Ok(file)
}

async fn download(core: &Core, url: &str) -> Result<Vec<u8>> {
    Ok(core
        .images
        .original(url, None)
        .await?
        .bytes()
        .await?
        .to_vec())
}

#[tauri::command]
pub async fn save_sample(core: CurrentCore, id: i64) -> Result<gallery::SavedFile> {
    let core = core.inner();
    let (bytes, url) = media::sample(core, id).await?;
    gallery::save(&core.app, &file_name_for(id, &url), &bytes).await
}

#[tauri::command]
pub async fn save_original(core: CurrentCore, id: i64) -> Result<gallery::SavedFile> {
    let core = core.inner();
    let original = original(core, id).await?;
    let bytes = download(core, &original.url).await?;
    gallery::save(&core.app, &original.file_name, &bytes).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn save_original_as(core: CurrentCore, id: i64) -> Result<Option<gallery::SavedFile>> {
    use tauri_plugin_dialog::DialogExt;
    let core = core.inner();
    let original = original(core, id).await?;
    let dialog = core
        .app
        .dialog()
        .file()
        .set_file_name(&original.file_name)
        .set_directory(gallery::folder(&core.app)?);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    dialog.save_file(move |chosen| {
        let _ = sender.send(chosen);
    });
    let Some(chosen) = receiver.await.ok().flatten() else {
        return Ok(None);
    };
    let path = chosen
        .as_path()
        .ok_or(Error::UnsupportedSaveLocation)?
        .to_owned();
    std::fs::write(&path, download(core, &original.url).await?)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let folder = path.parent().unwrap_or(&path).to_string_lossy();
    Ok(Some(gallery::SavedFile {
        name: name.into_owned(),
        folder: folder.into_owned(),
    }))
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn copy_image(core: CurrentCore, id: i64) -> Result<()> {
    let core = core.inner();
    let original = original(core, id).await?;
    let file = if catalogue::media_of(&original.url) == catalogue::Media::Gif {
        clipboard_file(
            core,
            &original.file_name,
            &download(core, &original.url).await?,
        )?
    } else {
        let (bytes, url) = media::sample(core, id).await?;
        clipboard_file(core, &file_name_for(id, &url), &bytes)?
    };
    gallery::copy_image(&core.app, &file).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn copy_image(
    core: CurrentCore,
    clipboard: State<'_, crate::clipboard::SystemClipboard>,
    id: i64,
) -> Result<()> {
    use clipboard_rs::common::RustImage;
    use clipboard_rs::{ClipboardContent, RustImageData};
    let core = core.inner();
    let original = original(core, id).await?;
    let mut contents = Vec::new();
    let still = if catalogue::media_of(&original.url) == catalogue::Media::Gif {
        let bytes = download(core, &original.url).await?;
        let file = clipboard_file(core, &original.file_name, &bytes)?;
        contents.push(ClipboardContent::Files(vec![
            file.to_string_lossy().into_owned(),
        ]));
        image::load_from_memory(&bytes)?
    } else {
        image::load_from_memory(&media::sample(core, id).await?.0)?
    };
    contents.push(ClipboardContent::Image(RustImageData::from_dynamic_image(
        still,
    )));
    clipboard.set(contents)
}

