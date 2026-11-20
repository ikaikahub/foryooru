use crate::booru::SETTINGS_STORE;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const FOLDER_NAME: &str = "Foryooru";

#[derive(Serialize, Deserialize)]
pub struct SaveFolder {
    pub location: String,
    pub label: String,
}

#[derive(Serialize)]
pub struct SavedFile {
    pub name: String,
    pub folder: String,
}

pub struct PickedFile {
    pub path: PathBuf,
    copied: bool,
}

impl Drop for PickedFile {
    fn drop(&mut self) {
        if self.copied
            && let Err(error) = std::fs::remove_file(&self.path)
        {
            eprintln!("Removing the copied file failed: {error}");
        }
    }
}

fn chosen(app: &AppHandle) -> Result<Option<SaveFolder>> {
    Ok(app
        .store(SETTINGS_STORE)?
        .get("saveFolder")
        .and_then(|value| serde_json::from_value(value).ok()))
}

#[cfg(target_os = "android")]
mod platform {
    use super::*;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{Manager, Wry};

    struct Gallery(PluginHandle<Wry>);

    #[derive(Serialize)]
    struct SaveArgs {
        path: String,
        name: String,
        folder: Option<String>,
    }

    #[derive(Deserialize)]
    struct PluginSaved {
        folder: String,
    }

    #[derive(Deserialize)]
    struct Picked {
        folder: Option<SaveFolder>,
    }

    #[derive(Deserialize)]
    struct PickedZip {
        path: Option<String>,
    }

    #[derive(Serialize)]
    struct CopyArgs {
        path: String,
    }

    pub async fn copy_image(app: &AppHandle, file: &std::path::Path) -> Result<()> {
        let handle = app.state::<Gallery>().0.clone();
        let args = CopyArgs {
            path: file.to_string_lossy().into_owned(),
        };
        tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<serde_json::Value>("copyImage", args)
        })
        .await??;
        Ok(())
    }

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("gallery")
            .setup(|app, api| {
                app.manage(Gallery(
                    api.register_android_plugin("com.j.foryooru", "GalleryPlugin")?,
                ));
                Ok(())
            })
            .build()
    }

    pub fn default_label(_app: &AppHandle) -> Result<String> {
        Ok(format!("Download/{FOLDER_NAME}"))
    }

    pub async fn save(app: &AppHandle, name: &str, bytes: &[u8]) -> Result<SavedFile> {
        let staging = app.path().app_cache_dir()?.join("saving");
        std::fs::create_dir_all(&staging)?;
        let temp = staging.join(name);
        std::fs::write(&temp, bytes)?;
        let handle = app.state::<Gallery>().0.clone();
        let args = SaveArgs {
            path: temp.to_string_lossy().into_owned(),
            name: name.to_owned(),
            folder: chosen(app)?.map(|folder| folder.location),
        };
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<PluginSaved>("save", args)
        })
        .await;
        std::fs::remove_file(&temp)?;
        Ok(SavedFile {
            name: name.to_owned(),
            folder: outcome??.folder,
        })
    }

    pub async fn pick_folder(app: &AppHandle) -> Result<Option<SaveFolder>> {
        let handle = app.state::<Gallery>().0.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<Picked>("pickFolder", ())
        })
        .await??;
        Ok(picked.folder)
    }

    pub async fn pick_zip(app: &AppHandle) -> Result<Option<PickedFile>> {
        let handle = app.state::<Gallery>().0.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            handle.run_mobile_plugin::<PickedZip>("pickZip", ())
        })
        .await??;
        Ok(picked.path.map(|path| PickedFile {
            path: PathBuf::from(path),
            copied: true,
        }))
    }
}

#[cfg(desktop)]
mod platform {
    use super::*;
    use crate::error::Error;
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    fn default_folder(app: &AppHandle) -> Result<PathBuf> {
        Ok(app
            .path()
            .download_dir()
            .map_err(|_| Error::NoDownloadsFolder)?
            .join(FOLDER_NAME))
    }

    pub fn folder(app: &AppHandle) -> Result<PathBuf> {
        match chosen(app)? {
            Some(folder) => Ok(PathBuf::from(folder.location)),
            None => default_folder(app),
        }
    }

    pub fn default_label(app: &AppHandle) -> Result<String> {
        Ok(default_folder(app)?.to_string_lossy().into_owned())
    }

    pub async fn save(app: &AppHandle, name: &str, bytes: &[u8]) -> Result<SavedFile> {
        let folder = folder(app)?;
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(name), bytes)?;
        Ok(SavedFile {
            name: name.to_owned(),
            folder: folder.to_string_lossy().into_owned(),
        })
    }

    pub async fn pick_folder(app: &AppHandle) -> Result<Option<SaveFolder>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_directory(folder(app)?)
            .pick_folder(move |picked| {
                let _ = sender.send(picked);
            });
        let Some(picked) = receiver.await.ok().flatten() else {
            return Ok(None);
        };
        let path = picked
            .as_path()
            .ok_or(Error::UnsupportedSaveLocation)?
            .to_string_lossy()
            .into_owned();
        Ok(Some(SaveFolder {
            label: path.clone(),
            location: path,
        }))
    }

    pub async fn pick_zip(app: &AppHandle) -> Result<Option<PickedFile>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .add_filter("Backup", &["zip"])
            .pick_file(move |picked| {
                let _ = sender.send(picked);
            });
        let Some(picked) = receiver.await.ok().flatten() else {
            return Ok(None);
        };
        Ok(Some(PickedFile {
            path: picked.into_path().map_err(|_| Error::UnsupportedFile)?,
            copied: false,
        }))
    }
}

#[cfg(desktop)]
pub use platform::folder;
#[cfg(target_os = "android")]
pub use platform::{copy_image, plugin};
pub use platform::{default_label, pick_folder, pick_zip, save};
