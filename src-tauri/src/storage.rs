use crate::core::Core;
use crate::db::TABLES;
use crate::error::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::Manager;

#[derive(Serialize, Default)]
pub struct Usage {
    files: u64,
    bytes: u64,
}

impl Usage {
    fn add_file(&mut self, path: &Path) {
        if let Ok(meta) = fs::metadata(path) {
            self.files += 1;
            self.bytes += meta.len();
        }
    }

    fn add_dir(&mut self, path: &Path) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => self.add_dir(&entry.path()),
                Ok(meta) => {
                    self.files += 1;
                    self.bytes += meta.len();
                }
                Err(_) => {}
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    database: Usage,
    settings: Usage,
    cache: Usage,
    cache_limit_mb: u64,
}

pub fn usage(core: &Core) -> Result<StorageUsage> {
    let mut database = Usage::default();
    for file in core.db.files() {
        database.add_file(&file);
    }
    let mut settings = Usage::default();
    if let Ok(entries) = fs::read_dir(core.app.path().app_data_dir()?) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                settings.add_file(&entry.path());
            }
        }
    }
    let mut cache = Usage::default();
    cache.add_dir(core.images.dir());
    Ok(StorageUsage {
        database,
        settings,
        cache,
        cache_limit_mb: core.images.limit_mb(),
    })
}

pub fn reset_database(core: &Core) -> Result<()> {
    {
        let mut conn = core.db.conn();
        conn.pragma_update(None, "foreign_keys", false)?;
        let emptied = (|| -> Result<()> {
            let tx = conn.transaction()?;
            for table in TABLES {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            tx.commit()?;
            Ok(())
        })();
        conn.pragma_update(None, "foreign_keys", true)?;
        emptied?;
        conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    core.corpus.clear();
    Ok(())
}

pub fn clear_cache(core: &Core) -> Result<()> {
    match fs::remove_dir_all(core.images.dir()) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
