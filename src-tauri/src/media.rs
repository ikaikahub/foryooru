use crate::catalogue::{self, Media, MediaUrls};
use crate::core::Core;
use crate::error::{Error, Result};
use crate::gelbooru::CDN;
use crate::images::content_type;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use std::sync::Arc;
use tauri::http::{Request, Response, StatusCode, header};

pub struct VideoBase(pub Option<String>);

const ORIGINAL_EXTENSIONS: [&str; 7] = ["jpg", "png", "jpeg", "gif", "webp", "mp4", "webm"];
const SAMPLE_WIDTH: i64 = 850;

enum Kind {
    Thumbnail,
    Sample,
    Original,
}

async fn resolve(core: &Arc<Core>, id: i64) -> Result<MediaUrls> {
    let mut urls = catalogue::media_urls(&core.db.conn(), id)?.ok_or(Error::PostNotFound)?;
    if !urls.partial || urls.media_resolved {
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
    catalogue::save_resolved_media(
        &core.db.conn(),
        id,
        &urls.sample_url,
        &urls.file_url,
        urls.media_resolved,
    )?;
    Ok(urls)
}

pub async fn complete_partial(core: &Arc<Core>, id: i64) -> Result<()> {
    let partial = catalogue::media_urls(&core.db.conn(), id)?.is_some_and(|urls| urls.partial);
    if !partial {
        return Ok(());
    }
    let (raws, _) = core.booru.search(&format!("id:{id}"), 0, None).await?;
    if !raws.is_empty() {
        let stored = catalogue::store_posts(&mut core.db.conn(), &raws, false)?;
        catalogue::after_store(core, &stored);
    }
    Ok(())
}

pub async fn original_url(core: &Arc<Core>, id: i64) -> Result<String> {
    let _ = complete_partial(core, id).await;
    Ok(resolve(core, id).await?.file_url)
}

pub async fn sample_url(core: &Arc<Core>, id: i64) -> Result<String> {
    Ok(resolve(core, id).await?.display_url().to_owned())
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
        Err(Error::CdnBusy) => empty(StatusCode::SERVICE_UNAVAILABLE),
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
    let url = match kind {
        Kind::Thumbnail => {
            let urls = resolve(core, id).await?;
            if urls.preview_url.is_empty() {
                urls.display_url().to_owned()
            } else {
                urls.preview_url
            }
        }
        Kind::Sample => sample_url(core, id).await?,
        Kind::Original => return serve_original(core, id, range).await,
    };
    let bytes = core.images.get(&url).await?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type(&url))
        .body(bytes)
        .unwrap_or_default())
}

async fn serve_original(
    core: &Arc<Core>,
    id: i64,
    range: Option<&str>,
) -> Result<Response<Vec<u8>>> {
    let url = original_url(core, id).await?;
    if url.is_empty() {
        return Err(Error::PostNotFound);
    }
    if range.is_none() && catalogue::media_of(&url) == Media::Gif {
        let bytes = core.images.get_original(&url).await?;
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, content_type(&url))
            .body(bytes)
            .unwrap_or_default());
    }
    let upstream = core.images.original(&url, range).await?;
    let mut builder = Response::builder().status(upstream.status().as_u16());
    for name in [CONTENT_TYPE, CONTENT_LENGTH, CONTENT_RANGE, ACCEPT_RANGES] {
        if let Some(value) = upstream.headers().get(&name) {
            builder = builder.header(name.as_str(), value.as_bytes());
        }
    }
    let bytes = upstream.bytes().await?.to_vec();
    Ok(builder.body(bytes).unwrap_or_default())
}
