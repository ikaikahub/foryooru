// Same zip layout as the desktop app (one database per site plus settings.json), so backups move between them.
// The page picks and saves the file; this only turns databases into bytes and back.
use crate::booru::{SETTINGS_STORE, Site};
use crate::core;
use crate::error::{Error, Result};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const RESTORING: &str = "restoring.db";

fn storage_error(error: impl std::fmt::Debug) -> Error {
    Error::Storage(format!("{error:?}"))
}

pub fn export(settings: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<u8>> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for site in Site::ALL {
        let name = site.database_file();
        if !core::with_files(|files| files.exists(name))?.map_err(storage_error)? {
            continue;
        }
        let bytes = core::with_files(|files| files.export_db(name))?.map_err(storage_error)?;
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    zip.start_file(SETTINGS_STORE, options)?;
    zip.write_all(&serde_json::to_vec(settings)?)?;
    Ok(zip.finish()?.into_inner())
}

// returns the backup's settings so the page can apply them
pub fn import(bytes: &[u8]) -> Result<Option<serde_json::Map<String, serde_json::Value>>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let settings: Option<serde_json::Map<String, serde_json::Value>> = match archive.by_name(SETTINGS_STORE) {
        Ok(mut entry) => {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            Some(serde_json::from_str(&text)?)
        }
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(error) => return Err(error.into()),
    };
    let databases: Vec<Site> = Site::ALL
        .into_iter()
        .filter(|site| archive.index_for_name(site.database_file()).is_some())
        .collect();
    if settings.is_none() && databases.is_empty() {
        return Err(Error::NotABackup);
    }

    let current = core::current()?;
    for site in databases {
        let name = site.database_file();
        let mut data = Vec::new();
        archive.by_name(name)?.read_to_end(&mut data)?;
        if current.site == site {
            // the open database is restored in place through SQLite's backup API, from a temporary copy
            core::with_files(|files| files.import_db(RESTORING, &data))?.map_err(storage_error)?;
            current.db.restore(Path::new(RESTORING))?;
            current.corpus.clear();
            core::with_files(|files| files.delete_db(RESTORING))?.map_err(storage_error)?;
        } else {
            core::with_files(|files| {
                files.delete_db(name)?;
                files.import_db(name, &data)
            })?
            .map_err(storage_error)?;
        }
    }
    Ok(settings)
}
