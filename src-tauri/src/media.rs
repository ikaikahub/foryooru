use crate::booru::Site;
use crate::catalogue::{self, Media, MediaUrls, extension, is_still};
use crate::core::Core;
use crate::danbooru::CDN as DANBOORU_CDN;
use crate::error::{Error, Result};
use crate::gelbooru::CDN;
use crate::images::content_type;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use std::sync::Arc;
use tauri::http::{Request, Response, StatusCode, header};

pub struct VideoBase(pub Option<String>);

const ORIGINAL_EXTENSIONS: [&str; 7] = ["jpg", "png", "jpeg", "gif", "webp", "mp4", "webm"];
const SAMPLE_WIDTH: i64 = 850;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Thumbnail,
    Sample,
    Original,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Thumbnail => "thumbnail",
            Kind::Sample => "sample",
            Kind::Original => "original",
        }
    }
}

// every site's urls for the post, Danbooru's first
async fn resolve(core: &Arc<Core>, id: i64) -> Result<Vec<MediaUrls>> {
    let mut resolved = Vec::with_capacity(2);
    let rows = catalogue::media_urls(&core.db.conn(), id)?;
    for urls in rows {
        resolved.push(resolve_partial(core, urls).await?);
    }
    if resolved.is_empty() {
        return Err(Error::PostNotFound);
    }
    Ok(resolved)
}

// posts from a BooruView file only have their md5, so their Gelbooru urls are found by asking its CDN
async fn resolve_partial(core: &Arc<Core>, mut urls: MediaUrls) -> Result<MediaUrls> {
    if !urls.partial || urls.media_resolved || urls.site != Site::Gelbooru {
        return Ok(urls);
    }
    let directory = format!("{}/{}", &urls.md5[..2], &urls.md5[2..4]);
    let at = |folder: &str, file: String| format!("{CDN}/{folder}/{directory}/{file}");
    if urls.sample_url.is_empty() && urls.width > SAMPLE_WIDTH {
        let sample = at("samples", format!("sample_{}.jpg", urls.md5));
        match core.images.exists(&sample).await {
            None => return Ok(urls),
            Some(true) => urls.sample_url = sample,
            Some(false) => {}
        }
    }
    if urls.file_url.is_empty() {
        for ext in ORIGINAL_EXTENSIONS {
            let original = at("images", format!("{}.{ext}", urls.md5));
            match core.images.exists(&original).await {
                None => return Ok(urls),
                Some(true) => {
                    urls.file_url = original;
                    break;
                }
                Some(false) => {}
            }
        }
    }
    urls.media_resolved = !urls.file_url.is_empty();
    catalogue::save_resolved_media(&core.db.conn(), &urls)?;
    Ok(urls)
}

// fetches the site's own copy of rows stored from partial listings
pub async fn complete_partial(core: &Arc<Core>, id: i64) -> Result<()> {
    let rows = catalogue::partial_rows(&core.db.conn(), id)?;
    for (site, site_id) in rows {
        if !core.can_search(site) {
            continue;
        }
        let (raws, _) = core.booru(site).search(&format!("id:{site_id}"), 0, None).await?;
        if !raws.is_empty() {
            let stored = catalogue::store_posts(&mut core.db.conn(), site, &raws, false)?;
            catalogue::after_store(core, &stored);
        }
    }
    Ok(())
}

pub async fn original_url(core: &Arc<Core>, id: i64) -> Result<String> {
    let _ = complete_partial(core, id).await;
    Ok(resolve(core, id)
        .await?
        .into_iter()
        .map(|urls| urls.file_url)
        .find(|url| !url.is_empty())
        .unwrap_or_default())
}

struct Source {
    url: String,
    site: Site,
    guessed: bool,
}

// both CDNs keep a file under its md5, so a post known from one site can often be fetched from the other's CDN
fn guessed_url(site: Site, kind: Kind, urls: &MediaUrls) -> Option<String> {
    let md5 = &urls.md5;
    if md5.len() != 32 {
        return None;
    }
    let directory = format!("{}/{}", &md5[..2], &md5[2..4]);
    let ext = extension(&urls.file_url);
    Some(match (kind, site) {
        (Kind::Thumbnail, Site::Gelbooru) => format!("{CDN}/thumbnails/{directory}/thumbnail_{md5}.jpg"),
        (Kind::Thumbnail, Site::Danbooru) => format!("{DANBOORU_CDN}/180x180/{directory}/{md5}.jpg"),
        // videos and gifs show a still that differs between the sites, so only the known one is used
        (Kind::Sample, _) if !is_still(&urls.file_url) => return None,
        (Kind::Sample, Site::Gelbooru) if urls.width > SAMPLE_WIDTH => {
            format!("{CDN}/samples/{directory}/sample_{md5}.jpg")
        }
        (Kind::Sample, Site::Danbooru) if urls.width > SAMPLE_WIDTH => {
            format!("{DANBOORU_CDN}/sample/{directory}/sample-{md5}.jpg")
        }
        _ if ext.is_empty() => return None,
        (_, Site::Gelbooru) => format!("{CDN}/images/{directory}/{md5}.{ext}"),
        (_, Site::Danbooru) => format!("{DANBOORU_CDN}/original/{directory}/{md5}.{ext}"),
    })
}

fn known_url(kind: Kind, urls: &MediaUrls) -> String {
    match kind {
        Kind::Thumbnail if urls.preview_url.is_empty() => urls.display_url().to_owned(),
        Kind::Thumbnail => urls.preview_url.clone(),
        Kind::Sample => urls.display_url().to_owned(),
        Kind::Original => urls.file_url.clone(),
    }
}

// the CDNs to try, the one that can answer soonest first. ties split by md5 so a page's downloads go half to each
async fn sources(core: &Arc<Core>, id: i64, kind: Kind) -> Result<(MediaUrls, Vec<Source>)> {
    let mut rows = resolve(core, id).await?;
    let mut sources = Vec::with_capacity(2);
    for urls in &rows {
        let known = known_url(kind, urls);
        if !known.is_empty() {
            sources.push(Source {
                site: Site::of_url(&known),
                url: known,
                guessed: false,
            });
        }
    }
    let known_sites: Vec<Site> = rows.iter().map(|urls| urls.site).collect();
    let urls = rows.swap_remove(0);
    for site in Site::ALL {
        if known_sites.contains(&site) {
            continue;
        }
        let has_file = catalogue::site_has_file(&core.db.conn(), id, site)?;
        // samples and originals are only guessed once a thumbnail showed the site has the file
        let worth_guessing = match kind {
            Kind::Thumbnail => has_file != Some(false),
            Kind::Sample | Kind::Original => has_file == Some(true),
        };
        if worth_guessing && let Some(url) = guessed_url(site, kind, &urls) {
            sources.push(Source {
                url,
                site,
                guessed: true,
            });
        }
    }
    let danbooru_first = urls.md5.as_bytes().first().is_some_and(|digit| digit % 2 == 0);
    sources.sort_by_key(|source| {
        let preferred = (source.site == Site::Danbooru) == danbooru_first;
        (core.images.wait_estimate(&source.url), !preferred)
    });
    Ok((urls, sources))
}

fn cache_key(urls: &MediaUrls, kind: Kind, fallback: &str) -> String {
    if urls.md5.len() == 32 {
        format!("{}-{}", urls.md5, kind.name())
    } else {
        fallback.to_owned()
    }
}

// either CDN may have filled the cache, so the bytes say what they are rather than the url
fn sniffed_type(bytes: &[u8], url: &str) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [0xFF, 0xD8, ..] => "image/jpeg",
        _ => content_type(url),
    }
}

// the bytes and the url they came from
async fn fetch(core: &Arc<Core>, id: i64, kind: Kind) -> Result<(Vec<u8>, String)> {
    let (urls, sources) = sources(core, id, kind).await?;
    let Some(first) = sources.first() else {
        return Err(Error::PostNotFound);
    };
    let key = cache_key(&urls, kind, &first.url);
    if let Some(bytes) = core.images.cached(&key) {
        return Ok((bytes, first.url.clone()));
    }
    let mut failure = Error::CdnBusy;
    for source in sources {
        let fetched = match kind {
            Kind::Original => core.images.get_original(&key, &source.url, source.guessed).await,
            Kind::Thumbnail | Kind::Sample => core.images.get(&key, &source.url, source.guessed).await,
        };
        match fetched {
            Ok(bytes) => {
                if source.guessed {
                    catalogue::remember_file(&core.db.conn(), id, source.site, true)?;
                }
                return Ok((bytes, source.url));
            }
            Err(error) => {
                // a missing guessed sample only means that site made no sample, not that it lacks the file
                if source.guessed && matches!(error, Error::NotOnCdn) && kind != Kind::Sample {
                    catalogue::remember_file(&core.db.conn(), id, source.site, false)?;
                }
                failure = error;
            }
        }
    }
    Err(failure)
}

pub async fn sample(core: &Arc<Core>, id: i64) -> Result<(Vec<u8>, String)> {
    fetch(core, id, Kind::Sample).await
}

fn parse(path: &str) -> Option<(Kind, i64)> {
    let mut parts = path.trim_matches('/').split('/');
    let kind = match parts.next()? {
        "thumbnail" => Kind::Thumbnail,
        "sample" => Kind::Sample,
        "original" => Kind::Original,
        _ => return None,
    };
    Some((kind, parts.next()?.parse().ok()?))
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store");
    if status == StatusCode::SERVICE_UNAVAILABLE {
        builder = builder.header(header::RETRY_AFTER, "10");
    }
    builder.body(Vec::new()).unwrap_or_default()
}

pub async fn respond(core: Arc<Core>, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    respond_to(core, request.uri().path(), range).await
}

pub async fn respond_to(core: Arc<Core>, path: &str, range: Option<&str>) -> Response<Vec<u8>> {
    let Some((kind, id)) = parse(path) else {
        return empty(StatusCode::NOT_FOUND);
    };
    match serve(&core, kind, id, range).await {
        Ok(response) => response,
        Err(Error::CdnBusy | Error::NotOnCdn) => empty(StatusCode::SERVICE_UNAVAILABLE),
        Err(Error::PostNotFound) => empty(StatusCode::NOT_FOUND),
        Err(error) => {
            eprintln!("Image {id} failed: {error}");
            empty(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn serve(
    core: &Arc<Core>,
    kind: Kind,
    id: i64,
    range: Option<&str>,
) -> Result<Response<Vec<u8>>> {
    if kind == Kind::Original {
        return serve_original(core, id, range).await;
    }
    let (bytes, url) = fetch(core, id, kind).await?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, sniffed_type(&bytes, &url))
        .body(bytes)
        .unwrap_or_default())
}

async fn serve_original(
    core: &Arc<Core>,
    id: i64,
    range: Option<&str>,
) -> Result<Response<Vec<u8>>> {
    let _ = complete_partial(core, id).await;
    let (urls, sources) = sources(core, id, Kind::Original).await?;
    if sources.is_empty() {
        return Err(Error::PostNotFound);
    }
    if range.is_none() && catalogue::media_of(&urls.file_url) == Media::Gif {
        let (bytes, url) = fetch(core, id, Kind::Original).await?;
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, content_type(&url))
            .body(bytes)
            .unwrap_or_default());
    }
    // both CDNs hold the same bytes, so a video's ranges can come from either
    let mut failure = Error::PostNotFound;
    for source in sources {
        let upstream = match core.images.original(&source.url, range).await {
            Ok(upstream) => upstream,
            Err(error) => {
                failure = error;
                continue;
            }
        };
        let mut builder = Response::builder().status(upstream.status().as_u16());
        for name in [CONTENT_TYPE, CONTENT_LENGTH, CONTENT_RANGE, ACCEPT_RANGES] {
            if let Some(value) = upstream.headers().get(&name) {
                builder = builder.header(name.as_str(), value.as_bytes());
            }
        }
        let bytes = upstream.bytes().await?.to_vec();
        return Ok(builder.body(bytes).unwrap_or_default());
    }
    Err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(file: &str, width: i64) -> MediaUrls {
        MediaUrls {
            site: Site::Danbooru,
            site_id: 1,
            md5: "e4949966c2e5037c4faf887cb29abb15".to_owned(),
            preview_url: String::new(),
            sample_url: String::new(),
            file_url: format!("https://example.com/{file}"),
            width,
            partial: false,
            media_resolved: false,
        }
    }

    #[test]
    fn guesses_follow_each_cdns_layout() {
        let large = urls("file.png", 2000);
        assert_eq!(
            guessed_url(Site::Danbooru, Kind::Thumbnail, &large).unwrap(),
            "https://cdn.donmai.us/180x180/e4/94/e4949966c2e5037c4faf887cb29abb15.jpg"
        );
        assert_eq!(
            guessed_url(Site::Gelbooru, Kind::Thumbnail, &large).unwrap(),
            "https://img4.gelbooru.com/thumbnails/e4/94/thumbnail_e4949966c2e5037c4faf887cb29abb15.jpg"
        );
        assert_eq!(
            guessed_url(Site::Danbooru, Kind::Sample, &large).unwrap(),
            "https://cdn.donmai.us/sample/e4/94/sample-e4949966c2e5037c4faf887cb29abb15.jpg"
        );
        assert_eq!(
            guessed_url(Site::Gelbooru, Kind::Original, &large).unwrap(),
            "https://img4.gelbooru.com/images/e4/94/e4949966c2e5037c4faf887cb29abb15.png"
        );
    }

    #[test]
    fn small_stills_use_the_original_as_their_sample() {
        assert_eq!(
            guessed_url(Site::Danbooru, Kind::Sample, &urls("file.jpg", 600)).unwrap(),
            "https://cdn.donmai.us/original/e4/94/e4949966c2e5037c4faf887cb29abb15.jpg"
        );
    }

    #[test]
    fn video_samples_and_md5_less_posts_are_not_guessed() {
        assert!(guessed_url(Site::Gelbooru, Kind::Sample, &urls("file.mp4", 1920)).is_none());
        let mut unknown = urls("file.jpg", 2000);
        unknown.md5.clear();
        assert!(guessed_url(Site::Gelbooru, Kind::Thumbnail, &unknown).is_none());
    }

    #[test]
    fn a_missed_guess_is_remembered_until_it_is_due_again() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        assert_eq!(catalogue::site_has_file(&conn, 7, Site::Danbooru).unwrap(), None);
        catalogue::remember_file(&conn, 7, Site::Danbooru, false).unwrap();
        assert_eq!(catalogue::site_has_file(&conn, 7, Site::Danbooru).unwrap(), Some(false));
        conn.execute("UPDATE site_files SET checked_at = 0", []).unwrap();
        assert_eq!(catalogue::site_has_file(&conn, 7, Site::Danbooru).unwrap(), None);
    }

    #[test]
    fn cached_bytes_name_their_own_type() {
        assert_eq!(sniffed_type(&[0x89, b'P', b'N', b'G', 0], "a.jpg"), "image/png");
        assert_eq!(sniffed_type(&[0xFF, 0xD8, 0xFF], "a.png"), "image/jpeg");
    }
}
