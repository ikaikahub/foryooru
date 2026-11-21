use crate::booru::{Order, PAGE_SIZE, POPULAR_WINDOW, Site, SiteQuery};
use crate::catalogue::{self, DAY_MS, TagKind};
use crate::core::Core;
use crate::error::Result;
use crate::feed::Leads;
use crate::library::{self, TagList};
use crate::queue::{Job, PageSearch, Priority};
use crate::search::{Ratings, Search};
use foryooru_core::fetch_plan::{self, PostTag};
use rusqlite::OptionalExtension;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const STORED_FOR: i64 = 30 * DAY_MS;
const RANDOM_STORED_FOR: i64 = DAY_MS;
const RECENT_STORED_FOR: i64 = DAY_MS;
const ALWAYS_FETCH: i64 = 0;

fn tags_of(core: &Core, post_id: i64) -> Result<Vec<PostTag>> {
    let conn = core.db.conn();
    let mut statement = conn.prepare_cached(
        "SELECT t.name, t.type, t.site_count, t.post_count FROM tags t
         WHERE t.id IN (SELECT tag_id FROM post_tags WHERE post_id = ?1)",
    )?;
    Ok(statement
        .query_map([post_id], |row| {
            Ok(PostTag {
                name: row.get(0)?,
                kind: row.get::<_, Option<i64>>(1)?.and_then(TagKind::from_code),
                count: row.get(2)?,
                stored: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

// the site a first page would go to and the search in its terms, to plan pages with, and the search for a job
async fn site_query(
    core: &Core,
    tags: Vec<String>,
    ratings: Ratings,
    order: Option<Order>,
) -> Result<(Site, String, PageSearch)> {
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let query = SiteQuery {
        tags,
        ratings,
        blacklist: &blacklist,
        order,
    };
    let (site, rendered) = core.route(&query, 0).await?;
    Ok((site, rendered, page_search(core, &query).await?))
}

async fn page_search(core: &Core, query: &SiteQuery<'_>) -> Result<PageSearch> {
    let only = (core.can_search(Site::Gelbooru) && !core.danbooru.takes_whole(query).await?).then_some(Site::Gelbooru);
    Ok(PageSearch {
        tags: query.tags.clone(),
        ratings: query.ratings,
        order: query.order,
        only,
    })
}

fn page_job(search: PageSearch, page: i64, max_age: i64, lead: Option<String>) -> Job {
    Job::Page {
        search,
        page,
        max_age,
        lead,
    }
}

fn spawn(what: &'static str, task: impl Future<Output = Result<()>> + Send + 'static) {
    crate::platform::spawn(async move {
        if let Err(error) = task.await {
            crate::platform::log(&format!("Background fetch ({what}) failed: {error}"));
        }
    });
}

#[derive(Default)]
struct Around {
    first: Vec<Job>,
    rest: Vec<Job>,
}

async fn target_jobs(core: &Core, tag: &PostTag, ratings: Ratings) -> Result<Vec<Job>> {
    let fetch_plan::TagFetch { order, pages } = fetch_plan::tag_fetch(tag);
    let (site, query, search) = site_query(core, vec![tag.name.clone()], ratings, order).await?;
    Ok(unfetched_pages(core, site, &query, STORED_FOR, pages, tag.count)?
        .into_iter()
        .map(|page| page_job(search.clone(), page, STORED_FOR, Some(tag.name.clone())))
        .collect())
}

async fn around(core: &Core, post_id: i64, ratings: Ratings) -> Result<Around> {
    let tags = tags_of(core, post_id)?;
    let mut around = Around::default();
    for tag in fetch_plan::targets(&tags) {
        let mut jobs = target_jobs(core, tag, ratings).await?.into_iter();
        around.first.extend(jobs.next());
        around.rest.extend(jobs);
    }
    Ok(around)
}

// each artist's newest page is fetched again after this long
const ARTIST_STORED_FOR: i64 = 60 * 60 * 1000;

pub async fn refresh_followed(core: &Arc<Core>, chips: &[String]) -> Result<()> {
    let artists = library::tag_names(&core.db.conn(), TagList::FollowedArtists)?;
    refresh_artists(core, &artists, chips).await
}

// one request per artist whose newest page in the searched ratings is older than ARTIST_STORED_FOR. each site that
// can search takes the next artist as soon as its last one is done, so their rate limits add up
pub async fn refresh_artists(core: &Arc<Core>, artists: &[String], chips: &[String]) -> Result<()> {
    let ratings = Search::parse(chips).ratings();
    let blacklist = Arc::new(library::tag_names(&core.db.conn(), TagList::Blacklist)?);
    let sites: Arc<Vec<Site>> = Arc::new(Site::ALL.into_iter().filter(|site| core.can_search(*site)).collect());
    let artists = Arc::new(artists.to_vec());
    let next = Arc::new(AtomicUsize::new(0));
    let workers: Vec<_> = sites
        .iter()
        .map(|site| {
            let (core, site, sites, artists, next, blacklist) =
                (core.clone(), *site, sites.clone(), artists.clone(), next.clone(), blacklist.clone());
            tauri::async_runtime::spawn(async move {
                while let Some(artist) = artists.get(next.fetch_add(1, Ordering::SeqCst)) {
                    if let Err(error) = refresh_artist(&core, site, &sites, artist, ratings, &blacklist).await {
                        crate::platform::log(&format!("Fetching {artist}'s newest posts failed: {error}"));
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.await?;
    }
    Ok(())
}

// fetches the artist's newest page from this site, unless either site fetched it recently
async fn refresh_artist(
    core: &Arc<Core>,
    site: Site,
    sites: &[Site],
    artist: &str,
    ratings: Ratings,
    blacklist: &[String],
) -> Result<()> {
    let search = SiteQuery {
        tags: vec![artist.to_owned()],
        ratings,
        blacklist,
        order: None,
    };
    for other in sites {
        let query = core.booru(*other).render(&search).await?;
        if catalogue::is_stored(&core.db.conn(), *other, &query, 0, ARTIST_STORED_FOR)? {
            return Ok(());
        }
    }
    let query = core.booru(site).render(&search).await?;
    catalogue::fetch_page(core, site, &query, 0, ARTIST_STORED_FOR).await?;
    Ok(())
}

// the first page of a search, so the artists page has posts to find the search's artists in
pub async fn fetch_search_page(core: &Arc<Core>, chips: &[String]) -> Result<()> {
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let (site, query) = core.route(&Search::parse(chips).site_query(&blacklist), 0).await?;
    catalogue::fetch_page(core, site, &query, 0, catalogue::SEARCH_PAGE_MAX_AGE).await?;
    Ok(())
}

pub fn after_like(core: &Arc<Core>, post_id: i64, chips: Vec<String>) {
    let core = core.clone();
    spawn("after a like", async move {
        let around = around(&core, post_id, Search::parse(&chips).ratings()).await?;
        for job in around.first.into_iter().chain(around.rest) {
            core.queue.push(job, Priority::FollowUp);
        }
        Ok(())
    });
}

fn popular_key(site: Site) -> String {
    format!("popular_seeded:{}", site.key())
}

fn popular_seeded(core: &Core, site: Site) -> Result<bool> {
    let count: i64 = core.db.conn().query_row(
        "SELECT COUNT(*) FROM meta WHERE key = ?1",
        [popular_key(site)],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn mark_popular_seeded(core: &Core, site: Site, floor: i64) -> Result<()> {
    core.db.conn().execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        [popular_key(site), floor.to_string()],
    )?;
    Ok(())
}

// one page of each site's latest 1000 posts sorted by score across every rating, once per site
pub fn seed_popular(core: &Arc<Core>) {
    let core = core.clone();
    spawn("popular seed", async move {
        if core.popular_seeding.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let result: Result<()> = async {
            for site in Site::ALL {
                if !core.can_search(site) || popular_seeded(&core, site)? {
                    continue;
                }
                // the newest id this site has sent, as its ids are only comparable with each other
                let newest: Option<i64> = core
                    .db
                    .conn()
                    .query_row("SELECT MAX(site_id) FROM site_posts WHERE site = ?1", [site.key()], |row| row.get(0))
                    .optional()?
                    .flatten();
                let Some(newest) = newest else {
                    continue;
                };
                let floor = (newest - POPULAR_WINDOW).max(0);
                let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
                let query = core
                    .booru(site)
                    .render(&SiteQuery {
                        tags: vec![format!("id:>{floor}")],
                        ratings: Ratings::ALL,
                        blacklist: &blacklist,
                        order: Some(Order::Score),
                    })
                    .await?;
                catalogue::fetch_page(&core, site, &query, 0, ALWAYS_FETCH).await?;
                mark_popular_seeded(&core, site, floor)?;
            }
            Ok(())
        }
        .await;
        core.popular_seeding.store(false, Ordering::SeqCst);
        result
    });
}

pub async fn for_similar(core: &Core, search: &Search) {
    let mut first = Vec::new();
    for post_id in &search.similar_towards {
        match around(core, *post_id, search.ratings()).await {
            Ok(around) => {
                first.extend(around.first);
                for job in around.rest {
                    core.queue.push(job, Priority::FollowUp);
                }
            }
            Err(error) => eprintln!("Fetching posts similar to {post_id} failed: {error}"),
        }
    }
    core.queue.run_now(first).await;
}

async fn shuffled_search_job(core: &Core, search: &Search) -> Result<Option<Job>> {
    if !search.has_tag() {
        return Ok(None);
    }
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let sorted = search
        .chips
        .iter()
        .any(|chip| chip.starts_with("sort:") || chip.starts_with("order:"));
    let base = search.site_query(&blacklist);
    let site_query = SiteQuery {
        order: base.order.or((!sorted).then_some(Order::Random)),
        ..base
    };
    let (site, query) = core.route(&site_query, 0).await?;
    let job_search = page_search(core, &site_query).await?;
    let random = !sorted && core.booru(site).order_fits(site_query.tags.len()).await?;
    if random {
        return Ok(Some(page_job(job_search, 0, ALWAYS_FETCH, None)));
    }
    next_page_job(core, site, &query, job_search, RANDOM_STORED_FOR)
}

fn unfetched_pages(
    core: &Core,
    site: Site,
    query: &str,
    max_age: i64,
    limit: usize,
    expected_posts: Option<i64>,
) -> Result<Vec<i64>> {
    let conn = core.db.conn();
    let total = catalogue::stored_total(&conn, site, query)?
        .or(expected_posts)
        .unwrap_or(PAGE_SIZE);
    let pages = ((total.min(core.booru(site).max_offset()) + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    let mut unfetched = Vec::new();
    for page in 0..pages {
        if unfetched.len() == limit {
            break;
        }
        if !catalogue::is_stored(&conn, site, query, page, max_age)? {
            unfetched.push(page);
        }
    }
    Ok(unfetched)
}

fn next_page_job(core: &Core, site: Site, query: &str, search: PageSearch, max_age: i64) -> Result<Option<Job>> {
    let page = unfetched_pages(core, site, query, max_age, 1, None)?
        .into_iter()
        .next();
    Ok(page.map(|page| page_job(search, page, max_age, None)))
}

pub async fn fetch_more_similar(core: &Core, search: &Search) -> Result<()> {
    let ratings = search.ratings();
    let mut jobs: Vec<Job> = shuffled_search_job(core, search)
        .await?
        .into_iter()
        .collect();
    for post_id in &search.similar_towards {
        let tags = tags_of(core, *post_id)?;
        for tag in fetch_plan::targets(&tags) {
            jobs.extend(target_jobs(core, tag, ratings).await?.into_iter().next());
        }
    }
    core.queue.run_now(jobs).await;
    Ok(())
}

async fn lead_job(core: &Core, tag: String, ratings: Ratings) -> Result<Job> {
    let (_, _, search) = site_query(core, vec![tag.clone()], ratings, Some(Order::Random)).await?;
    Ok(page_job(search, 0, ALWAYS_FETCH, Some(tag)))
}

async fn refill_jobs(core: &Core, chips: &[String], leads: Leads) -> Result<Vec<Job>> {
    let search = Search::parse(chips);
    if search.has_tag() {
        return Ok(shuffled_search_job(core, &search)
            .await?
            .into_iter()
            .collect());
    }
    let ratings = search.ratings();
    let likes: i64 = core
        .db
        .conn()
        .query_row("SELECT COUNT(*) FROM likes", [], |row| row.get(0))?;
    let tags = fetch_plan::lead_tags(leads, likes as usize, &mut rand::rng());
    let mut jobs = Vec::new();
    for tag in tags {
        jobs.push(lead_job(core, tag, ratings).await?);
    }
    if jobs.is_empty() {
        let (site, recent, search) = site_query(core, Vec::new(), ratings, None).await?;
        jobs.extend(next_page_job(core, site, &recent, search, RECENT_STORED_FOR)?);
    }
    Ok(jobs)
}

pub async fn fetch_more_now(core: &Arc<Core>, chips: &[String], leads: Leads) -> Result<()> {
    seed_popular(core);
    let jobs = refill_jobs(core, chips, leads).await?;
    core.queue.run_now(jobs).await;
    Ok(())
}

pub fn after_batch(core: &Arc<Core>, chips: Vec<String>, leads: Leads) {
    seed_popular(core);
    if core.queue.has(Priority::Refill) {
        return;
    }
    let core = core.clone();
    spawn("after a TikTok batch", async move {
        for job in refill_jobs(&core, &chips, leads).await? {
            core.queue.push(job, Priority::Refill);
        }
        Ok(())
    });
}
