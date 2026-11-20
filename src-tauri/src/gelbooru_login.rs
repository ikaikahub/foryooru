use crate::error::Result;
use crate::gelbooru::GelbooruSession;
use tauri::AppHandle;

#[cfg(desktop)]
mod platform {
    use super::*;
    use crate::error::Error;
    use crate::gelbooru::{LOGIN_URL, SITE_URL};
    use std::time::Duration;
    use tauri::{Manager, Url, WebviewUrl, WebviewWindowBuilder};

    const WINDOW: &str = "gelbooru-login";
    const MAIN_WINDOW: &str = "main";
    const POLL: Duration = Duration::from_secs(1);

    fn url(text: &str) -> Result<Url> {
        text.parse::<Url>()
            .map_err(|error| Error::LoginWindow(error.to_string()))
    }

    pub async fn login(app: &AppHandle) -> Result<Option<GelbooruSession>> {
        let window = match app.get_webview_window(WINDOW) {
            Some(window) => {
                window.set_focus()?;
                window
            }
            None => WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url(LOGIN_URL)?))
                .title("Log in to Gelbooru")
                .inner_size(480.0, 720.0)
                .build()?,
        };
        let site = url(SITE_URL)?;
        loop {
            tokio::time::sleep(POLL).await;
            if app.get_webview_window(WINDOW).is_none() {
                return Ok(None);
            }
            let cookies = window.cookies_for_url(site.clone())?;
            if let Some(session) = GelbooruSession::from_cookies(
                cookies.iter().map(|cookie| (cookie.name(), cookie.value())),
            ) {
                window.close()?;
                return Ok(Some(session));
            }
        }
    }

    pub async fn logout(app: &AppHandle) -> Result<()> {
        let webview = app
            .get_webview_window(MAIN_WINDOW)
            .ok_or_else(|| Error::LoginWindow("the main window is missing".into()))?;
        for cookie in webview.cookies_for_url(url(SITE_URL)?)? {
            webview.delete_cookie(cookie)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "android")]
mod platform {
    use super::*;
    use serde::Deserialize;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{Manager, Wry};

    struct LoginPlugin(PluginHandle<Wry>);

    #[derive(Deserialize)]
    struct Answer {
        session: Option<GelbooruSession>,
    }

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("gelbooru-login")
            .setup(|app, api| {
                app.manage(LoginPlugin(api.register_android_plugin(
                    "com.j.foryooru",
                    "GelbooruLoginPlugin",
                )?));
                Ok(())
            })
            .build()
    }

    pub async fn login(app: &AppHandle) -> Result<Option<GelbooruSession>> {
        let handle = app.state::<LoginPlugin>().0.clone();
        let answer = tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<Answer>("login", ())
        })
        .await??;
        Ok(answer.session)
    }

    pub async fn logout(app: &AppHandle) -> Result<()> {
        let handle = app.state::<LoginPlugin>().0.clone();
        tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<serde_json::Value>("logout", ())
        })
        .await??;
        Ok(())
    }
}

#[cfg(target_os = "android")]
pub use platform::plugin;
pub use platform::{login, logout};
