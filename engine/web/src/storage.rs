use crate::booru::Site;
use crate::core::{self, Core};
use crate::db::TABLES;
use crate::error::Result;
use serde::Serialize;

#[derive(Serialize, Default)]
pub struct Usage {
    files: u64,
    bytes: u64,
}

// settings live in the page's storage and images in the browser's own cache, so only the database is counted here
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    database: Usage,
    settings: Usage,
    cache: Usage,
    cache_limit_mb: u64,
}

pub fn usage(core: &Core) -> Result<StorageUsage> {
    let conn = core.db.conn();
    let pages: u64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let page_size: u64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    Ok(StorageUsage {
        database: Usage {
            files: 1,
            bytes: pages * page_size,
        },
        settings: Usage::default(),
        cache: Usage::default(),
        cache_limit_mb: 0,
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
        conn.execute_batch("VACUUM;")?;
    }
    core.corpus.clear();
    Ok(())
}

pub fn reset_all_databases(core: &Core) -> Result<()> {
    reset_database(core)?;
    for site in Site::ALL.into_iter().filter(|site| *site != core.site) {
        core::with_files(|files| files.delete_db(site.database_file()))?
            .map_err(|error| crate::error::Error::Storage(format!("{error:?}")))?;
    }
    Ok(())
}
