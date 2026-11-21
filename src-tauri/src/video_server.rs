use crate::core::Core;
use crate::error::{Error, Result};
use crate::media;
use rand::Rng;
use std::io::Cursor;
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Request, Response, Server, StatusCode};

const TOKEN_BYTES: usize = 16;

pub fn start(app: AppHandle) -> Result<String> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| Error::VideoServer(error.to_string()))?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| Error::VideoServer("no local port".into()))?
        .port();
    let token: String = (0..TOKEN_BYTES)
        .map(|_| format!("{:02x}", rand::rng().random::<u8>()))
        .collect();
    let prefix = format!("/{token}/");
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let (app, prefix) = (app.clone(), prefix.clone());
            std::thread::spawn(move || answer(&app, &prefix, request));
        }
    });
    Ok(format!("http://127.0.0.1:{port}/{token}/"))
}

fn answer(app: &AppHandle, prefix: &str, request: Request) {
    let Some(path) = request.url().strip_prefix(prefix).map(str::to_owned) else {
        let _ = request.respond(Response::empty(StatusCode(404)));
        return;
    };
    let range = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.to_string());
    let core = app.state::<std::sync::Arc<Core>>().inner().clone();
    let response = tauri::async_runtime::block_on(media::respond_to(core, &path, range.as_deref()));
    let (parts, body) = response.into_parts();
    let headers = parts
        .headers
        .iter()
        .filter(|(name, _)| *name != tauri::http::header::CONTENT_LENGTH)
        .filter_map(|(name, value)| Header::from_bytes(name.as_str(), value.as_bytes()).ok())
        .collect();
    let length = body.len();
    let _ = request.respond(Response::new(
        StatusCode(parts.status.as_u16()),
        headers,
        Cursor::new(body),
        Some(length),
        None,
    ));
}
