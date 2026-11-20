mod error;
mod fetched;
mod pool;
mod simulate;
mod snapshot;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use error::{Error, Result};
use foryooru_core::feed::{self, Scrolled};
use foryooru_core::protocol::{
    FeedRequest, FeedResponse, ForYouRequest, ForYouResponse, RankedPost, SearchRequest,
    SearchResponse, ServedTag, SimilarRequest, SimilarResponse, TagsRequest, VERSION, Version,
};
use foryooru_core::recommend;
use foryooru_core::search::Search;
use pool::Pool;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

const CONTAINER: &str = "kimochii-db";
const DEFAULT_ADDRESS: &str = "0.0.0.0:8790";
const PAGE_SIZE: usize = 100;

type Shared = State<Arc<Pool>>;

async fn blocking<T: Send + 'static>(
    name: &'static str,
    pool: Arc<Pool>,
    work: impl FnOnce(&Pool) -> T + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let done = work(&pool);
        eprintln!("{name} took {}ms", started.elapsed().as_millis());
        done
    })
    .await
    .map_err(|error| Error::Io(std::io::Error::other(error)))
}

async fn version(State(pool): Shared) -> Json<Version> {
    Json(Version {
        version: VERSION.to_owned(),
        posts: pool.corpus.size(),
    })
}

async fn search(
    State(pool): Shared,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>> {
    blocking("search", pool, move |pool| {
        let search = Search::parse(&request.chips);
        let mask = pool.corpus.search_mask(&search);
        let mut ids = pool.corpus.ids_where(&mask);
        match search.sort {
            Some(order) => {
                let keys = pool
                    .corpus
                    .sort_keys(order, &mask, request.chips.len() as u64);
                ids.sort_by(|a, b| keys[b].total_cmp(&keys[a]));
            }
            None => ids.reverse(),
        }
        let start = (request.page.max(0) as usize).saturating_mul(PAGE_SIZE);
        let page: Vec<i64> = ids.iter().skip(start).take(PAGE_SIZE).copied().collect();
        Json(SearchResponse {
            posts: pool.served_all(&page),
            total: ids.len() as i64,
        })
    })
    .await
}

async fn tags(State(pool): Shared, Json(request): Json<TagsRequest>) -> Json<Vec<ServedTag>> {
    Json(
        request
            .names
            .iter()
            .filter_map(|name| pool.tag(name))
            .collect(),
    )
}

async fn for_you(
    State(pool): Shared,
    Json(request): Json<ForYouRequest>,
) -> Result<Json<ForYouResponse>> {
    blocking("for-you", pool, move |pool| {
        let shown: HashSet<i64> = request.shown.into_iter().collect();
        let ranked: Vec<RankedPost> = recommend::for_you(
            &pool.corpus,
            &request.user,
            &Search::parse(&request.chips),
            request.hide_liked,
            request.discover,
            &request.model,
        )
        .into_iter()
        .filter(|(id, _)| !shown.contains(id))
        .take(request.limit)
        .map(|(id, explore)| RankedPost { id, explore })
        .collect();
        let ids: Vec<i64> = ranked.iter().map(|post| post.id).collect();
        Json(ForYouResponse {
            posts: pool.served_all(&ids),
            ranked,
        })
    })
    .await
}

async fn feed_next(
    State(pool): Shared,
    Json(request): Json<FeedRequest>,
) -> Result<Json<FeedResponse>> {
    blocking("feed", pool, move |pool| {
        let seen: HashSet<i64> = request.seen.into_iter().collect();
        let batch = feed::next_batch(
            &pool.corpus,
            &request.user,
            &Search::parse(&request.chips),
            Scrolled {
                seen: &seen,
                since: request.since,
                session_likes: &request.session_likes,
            },
            request.options,
            &request.model,
        );
        Json(FeedResponse {
            posts: pool.served_all(&batch.ids),
            ids: batch.ids,
            exhausted: batch.exhausted,
        })
    })
    .await
}

async fn similar(
    State(pool): Shared,
    Json(request): Json<SimilarRequest>,
) -> Result<Json<SimilarResponse>> {
    blocking("similar", pool, move |pool| {
        let ids: Vec<i64> = recommend::similar(
            &pool.corpus,
            &request.user,
            &Search::parse(&request.chips),
            request.per_artist,
            request.limit,
        )
        .unwrap_or_default();
        Json(SimilarResponse {
            posts: pool.served_all(&ids),
            ids,
        })
    })
    .await
}

async fn serve(snapshot: &Path, address: &str) -> Result<()> {
    let started = std::time::Instant::now();
    let pool = Arc::new(Pool::load(snapshot)?);
    pool.corpus.prepare();
    eprintln!(
        "{} posts ready in {:.0}s",
        pool.corpus.size(),
        started.elapsed().as_secs_f64()
    );
    let app = Router::new()
        .route("/version", get(version))
        .route("/search", post(search))
        .route("/tags", post(tags))
        .route("/for-you", post(for_you))
        .route("/feed", post(feed_next))
        .route("/similar", post(similar))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024))
        .with_state(pool);
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("listening on {address}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, target] if command == "convert" => {
            snapshot::convert(CONTAINER, Path::new(target))
        }
        [command, snapshot, rest @ ..] if command == "serve" && rest.len() <= 1 => {
            let address = rest.first().map_or(DEFAULT_ADDRESS, String::as_str);
            tokio::runtime::Runtime::new()?.block_on(serve(Path::new(snapshot), address))
        }
        [command, snapshot, report, personas, batches] if command == "simulate" => {
            let personas: usize = personas.parse().map_err(|_| Error::Usage)?;
            let batches: usize = batches.parse().map_err(|_| Error::Usage)?;
            let pool = Pool::load(Path::new(snapshot))?;
            std::fs::write(report, simulate::run(&pool, personas, batches))?;
            Ok(())
        }
        _ => Err(Error::Usage),
    }
}
