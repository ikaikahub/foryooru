use crate::booru::{SETTINGS_STORE, Site};
use crate::core::Core;
use crate::merge::{self, DATABASE};
use std::sync::Arc;
use crate::error::{Error, Result};
use crate::gallery::{self, SavedFile};
use rusqlite::{Connection, OpenFlags};
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn fresh_dir(app: &AppHandle, name: &str) -> Result<PathBuf> {
    let dir = app.path().app_cache_dir()?.join(name);
    match fs::remove_dir_all(&dir) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn archive(app: &AppHandle) -> Result<Vec<u8>> {
    let data_dir = app.path().app_data_dir()?;
    let staging = fresh_dir(app, "exporting")?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let copy = staging.join(DATABASE);
    Connection::open_with_flags(data_dir.join(DATABASE), OpenFlags::SQLITE_OPEN_READ_ONLY)?
        .execute("VACUUM INTO ?1", [copy.to_string_lossy()])?;
    zip.start_file(DATABASE, options)?;
    io::copy(&mut File::open(&copy)?, &mut zip)?;
    let settings: serde_json::Map<String, serde_json::Value> =
        app.store(SETTINGS_STORE)?.entries().into_iter().collect();
    zip.start_file(SETTINGS_STORE, options)?;
    zip.write_all(&serde_json::to_vec(&settings)?)?;
    let bytes = zip.finish()?.into_inner();
    fs::remove_dir_all(&staging)?;
    Ok(bytes)
}

pub async fn export(app: &AppHandle) -> Result<SavedFile> {
    let worker = app.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || archive(&worker)).await??;
    let name = chrono::Local::now()
        .format("foryooru-backup-%Y-%m-%d.zip")
        .to_string();
    gallery::save(app, &name, &bytes).await
}

fn restore_settings(
    app: &AppHandle,
    entries: serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let store = app.store(SETTINGS_STORE)?;
    store.clear();
    for (key, value) in entries {
        store.set(key, value);
    }
    store.save()?;
    Ok(())
}

pub fn import(app: &AppHandle, backup: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(File::open(backup)?)?;
    let settings: Option<serde_json::Map<String, serde_json::Value>> =
        match archive.by_name(SETTINGS_STORE) {
            Ok(mut entry) => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                Some(serde_json::from_str(&text)?)
            }
            Err(zip::result::ZipError::FileNotFound) => None,
            Err(error) => return Err(error.into()),
        };
    let merged = archive.index_for_name(DATABASE).is_some();
    // backups from before the merge hold one file per site, which are merged the same way the app's own were
    let site_files: Vec<Site> = Site::ALL
        .into_iter()
        .filter(|site| archive.index_for_name(site.database_file()).is_some())
        .collect();
    if settings.is_none() && !merged && site_files.is_empty() {
        return Err(Error::NotABackup);
    }

    let staging = fresh_dir(app, "importing")?;
    let restored = if merged {
        let staged = staging.join(DATABASE);
        io::copy(&mut archive.by_name(DATABASE)?, &mut File::create(&staged)?)?;
        Some(staged)
    } else if !site_files.is_empty() {
        let mut staged = Vec::new();
        for site in site_files {
            let path = staging.join(site.database_file());
            io::copy(&mut archive.by_name(site.database_file())?, &mut File::create(&path)?)?;
            staged.push((site, path));
        }
        let target = staging.join(DATABASE);
        merge::merge(&staged, &target)?;
        Some(target)
    } else {
        None
    };
    if let Some(restored) = restored {
        let core = app.state::<Arc<Core>>();
        core.db.restore(&restored)?;
        core.corpus.clear();
    }
    fs::remove_dir_all(&staging)?;

    if let Some(entries) = settings {
        restore_settings(app, entries)?;
    }
    Ok(())
}
