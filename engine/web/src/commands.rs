use crate::activity::{self, ActivityPage};
use crate::booru::{Booru, Order, Site};
use crate::catalogue::{self, Post, PostDetail, SEARCH_PAGE_MAX_AGE, Tag};
use crate::core::{self as sites, Core};
use crate::corpus::Corpus;
use crate::db::now_ms;
use crate::engine;
use crate::error::{Error, Result};
use crate::family::{self, Relation};
use crate::feed::{self, FeedOptions};
use crate::fetching;
use crate::import::{self, ImportStatus};
use crate::impressions::{self, Impression};
use crate::library::{self, LISTING_PAGE_SIZE, Listing, Matching, PostState, SavedSearch, TagList};
use crate::model;
use crate::overview;
use crate::payoff;
use crate::recommend::{self, RANKING_LIMIT, UserData};
use crate::search::{self, Rating, Search};
use crate::storage::{self, StorageUsage};
use crate::tables::{self, TablePage, TableSize};
use serde::Serialize;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::ops::Deref;

// the open site, handed to each command like Tauri's injected state was
pub struct CurrentCore(Arc<Core>);

impl CurrentCore {
    pub fn get() -> Result<CurrentCore> {
        Ok(CurrentCore(sites::current()?))
    }

    pub fn inner(&self) -> &Arc<Core> {
        &self.0
    }
}

impl Deref for CurrentCore {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.0
    }
}

// the engine already runs on a Web Worker, so ranking work runs inline instead of on a blocking pool
async fn with_corpus<T>(core: &Arc<Core>, work: impl FnOnce(&Corpus, &UserData) -> T) -> Result<T> {
    let corpus = core.corpus.get(&core.db)?;
    let user = recommend::load_user(&core.db.conn())?;
    Ok(work(&corpus, &user))
}

async fn complete_partial(core: &Arc<Core>, id: i64) -> Result<()> {
    if !catalogue::is_partial(&core.db.conn(), id)? {
        return Ok(());
    }
    let (raws, _) = core.booru.search(&format!("id:{id}"), 0, None).await?;
    if !raws.is_empty() {
        let stored = catalogue::store_posts(&mut core.db.conn(), &raws, false)?;
        catalogue::after_store(core, &stored);
    }
    Ok(())
}

pub async fn browse(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let core = core.inner();
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let search = Search::parse(&chips);
    let query = core.booru.render(&search.site_query(&blacklist)).await?;
    let stored = catalogue::search_page(core, &query, page, SEARCH_PAGE_MAX_AGE).await?;
    fetching::seed_popular(core);
    let conn = core.db.conn();
    let (kept, hidden) = library::without_blacklisted(&conn, stored.ids)?;
    let mut posts = catalogue::posts(&conn, &kept)?;
    if search.sort == Some(Order::Popular) && !core.booru.ranks_popular() {
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
        pages: ((stored.total.min(core.booru.max_offset()) + LISTING_PAGE_SIZE - 1)
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

pub async fn for_you_more(
    core: CurrentCore,
    chips: Vec<String>,
    hide_liked: bool,
    discover: bool,
    shown: Vec<i64>,
) -> Result<MoreRanked> {
    let core = core.inner();
    let shown: HashSet<i64> = shown.into_iter().collect();
    let model = model::load(&core.db.conn())?;
    let rank = |chips: Vec<String>| {
        let (shown, model) = (shown.clone(), model.clone());
        with_corpus(core, move |corpus, user| {
            recommend::for_you(
                corpus,
                user,
                &Search::parse(&chips),
                hide_liked,
                discover,
                &model,
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

pub async fn for_you(
    core: CurrentCore,
    chips: Vec<String>,
    hide_liked: bool,
    discover: bool,
) -> Result<Vec<Ranked>> {
    let search = Search::parse(&chips);
    fetching::for_similar(&core, &search).await;
    let model = model::load(&core.db.conn())?;
    let ranking = with_corpus(core.inner(), move |corpus, user| {
        recommend::for_you(corpus, user, &search, hide_liked, discover, &model)
    })
    .await?;
    Ok(ranking
        .into_iter()
        .map(|(id, explore)| Ranked { id, explore })
        .collect())
}

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
    let model = model::load(&core.db.conn())?;
    let rank = |chips: Vec<String>| {
        let (seen, session_likes, options, model) = (
            seen.clone(),
            session_likes.clone(),
            options.clone(),
            model.clone(),
        );
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
                &model,
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
    model::learn_in_background(core.clone());
    Ok(FeedBatch {
        posts: catalogue::post_details(&core.db.conn(), &batch.ids)?,
        exhausted: batch.exhausted,
        broadened,
    })
}

pub fn posts(core: CurrentCore, ids: Vec<i64>) -> Result<Vec<Post>> {
    catalogue::posts(&core.db.conn(), &ids)
}

#[derive(Serialize)]
pub struct PostFamily {
    relation: Relation,
    posts: Vec<Post>,
}

const SIMILAR_RELATIVES: usize = 10;

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

pub async fn post_detail(core: CurrentCore, id: i64) -> Result<PostDetail> {
    if let Err(error) = complete_partial(core.inner(), id).await {
        crate::platform::log(&format!("Couldn't complete post {id}: {error}"));
    }
    catalogue::post_details(&core.db.conn(), &[id])?
        .pop()
        .ok_or(Error::PostNotFound)
}

// a shared link opens one post even when this database has never seen it
pub async fn open_post(core: CurrentCore, id: i64) -> Result<PostDetail> {
    if catalogue::posts(&core.db.conn(), &[id])?.is_empty() {
        let (raws, _) = core.booru.search(&format!("id:{id}"), 0, None).await?;
        if !raws.is_empty() {
            let stored = catalogue::store_posts(&mut core.db.conn(), &raws, false)?;
            catalogue::after_store(core.inner(), &stored);
        }
    }
    post_detail(core, id).await
}

// a shared link to the other site's post is shown as-is, without touching this site's database
pub async fn preview_post(core: CurrentCore, site: Site, id: i64) -> Result<PostDetail> {
    let booru = Booru::new(site, core.app, core.db.clone(), sites::http()?);
    Ok(catalogue::detail_from_raw(booru.preview(id).await?))
}

pub fn toggle_like(core: CurrentCore, id: i64, chips: Vec<String>) -> Result<PostState> {
    let state = library::toggle_like(&core.db.conn(), id)?;
    if state.liked {
        fetching::after_like(core.inner(), id, chips);
    }
    Ok(state)
}

pub async fn set_site_favourite(core: CurrentCore, id: i64, favourite: bool) -> Result<()> {
    core.booru.set_favourite(id, favourite).await
}

pub fn toggle_dislike(core: CurrentCore, id: i64) -> Result<PostState> {
    library::toggle_dislike(&core.db.conn(), id)
}

pub async fn liked_posts(core: CurrentCore, chips: Vec<String>, page: i64) -> Result<Listing> {
    let matching = matching_posts(core.inner(), &chips).await?;
    library::liked_listing(&core.db.conn(), &matching, page)
}

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

pub fn clear_likes(core: CurrentCore) -> Result<()> {
    library::clear_likes(&core.db.conn())
}

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

pub fn tags_named(core: CurrentCore, names: Vec<String>) -> Result<Vec<Tag>> {
    catalogue::tags_named(&core.db.conn(), &names)
}

pub fn tag_list(core: CurrentCore, list: TagList) -> Result<Vec<Tag>> {
    library::tags(&core.db.conn(), list)
}

pub fn toggle_tag(core: CurrentCore, list: TagList, name: String) -> Result<Vec<Tag>> {
    let conn = core.db.conn();
    library::toggle_tag(&conn, list, &name)?;
    library::tags(&conn, list)
}

pub fn add_tags(core: CurrentCore, list: TagList, names: String) -> Result<Vec<Tag>> {
    let conn = core.db.conn();
    library::add_tags(&conn, list, &names)?;
    library::tags(&conn, list)
}

pub fn saved_searches(core: CurrentCore) -> Result<Vec<SavedSearch>> {
    library::saved_searches(&core.db.conn())
}

pub fn toggle_saved_search(core: CurrentCore, query: String) -> Result<Vec<SavedSearch>> {
    let conn = core.db.conn();
    library::toggle_saved_search(&conn, &query)?;
    library::saved_searches(&conn)
}

pub fn record_impressions(
    core: CurrentCore,
    impressions: Vec<Impression>,
    chips: Vec<String>,
) -> Result<()> {
    let long_looks = impressions::record(&mut core.db.conn(), &impressions)?;
    fetching::after_long_looks(core.inner(), long_looks, chips);
    Ok(())
}

pub fn record_signal(
    core: CurrentCore,
    post: i64,
    signal: activity::Signal,
    tag: Option<String>,
) -> Result<()> {
    activity::record_signal(&core.db.conn(), post, signal, tag.as_deref())
}

pub fn record_search(core: CurrentCore, query: String) -> Result<()> {
    activity::record_search(&core.db.conn(), &query)
}

pub fn activity(core: CurrentCore, filter: activity::Filter, page: i64) -> Result<ActivityPage> {
    activity::page(&core.db.conn(), filter, page)
}

pub fn table_sizes(core: CurrentCore) -> Result<Vec<TableSize>> {
    tables::sizes(&core.db.conn())
}

pub fn table_page(core: CurrentCore, table: String, page: i64) -> Result<TablePage> {
    tables::page(&core.db.conn(), &table, page)
}

pub async fn algorithm_overview(
    core: CurrentCore,
    chips: Vec<String>,
    hide_liked: bool,
) -> Result<overview::Overview> {
    let (model, fetching) = {
        let conn = core.db.conn();
        (
            model::load(&conn)?.status(),
            payoff::FetchStats::read(&conn)?,
        )
    };
    with_corpus(core.inner(), move |corpus, user| {
        overview::overview(
            corpus,
            user,
            &Search::parse(&chips),
            hide_liked,
            model,
            fetching,
        )
    })
    .await
}

pub fn import_status(core: CurrentCore) -> ImportStatus {
    core.import.get()
}

pub fn import_favourites(core: CurrentCore, user: String) -> Result<()> {
    import::favourites(core.inner(), user)
}

pub fn set_site(site: Site) -> Result<()> {
    sites::switch(site)
}

pub fn import_from_file(core: CurrentCore, name: String, contents: String) -> Result<()> {
    import::from_file(core.inner(), name, &contents)
}

pub fn storage_usage(core: CurrentCore) -> Result<StorageUsage> {
    storage::usage(&core)
}

pub async fn reset_database(core: CurrentCore) -> Result<()> {
    storage::reset_database(&core)
}

pub async fn reset_all_databases(core: CurrentCore) -> Result<()> {
    storage::reset_all_databases(&core)
}

