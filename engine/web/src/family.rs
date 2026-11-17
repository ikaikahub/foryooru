use crate::booru::{Lineage, SiteQuery};
use crate::catalogue::{self, DAY_MS};
use crate::core::Core;
use crate::error::Result;
use crate::library::{self, TagList};
use crate::search::Ratings;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Arc;

const STORED_FOR: i64 = DAY_MS;

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Relation {
    Family,
    Similar,
}

fn stored_lineage(conn: &Connection, id: i64) -> Result<Option<Lineage>> {
    Ok(conn
        .query_row(
            "SELECT parent_id, has_children FROM posts WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<bool>>(1)?,
                ))
            },
        )
        .optional()?
        .and_then(|(parent, has_children)| {
            has_children.map(|has_children| Lineage {
                parent,
                has_children,
            })
        }))
}

async fn fetch(core: &Arc<Core>, tag: String) -> Result<Vec<i64>> {
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let query = core
        .booru
        .render(&SiteQuery {
            tags: vec![tag],
            ratings: Ratings::ALL,
            blacklist: &blacklist,
            order: None,
        })
        .await?;
    Ok(catalogue::search_page(core, &query, 0, STORED_FOR)
        .await?
        .ids)
}

async fn lineage(core: &Arc<Core>, id: i64) -> Result<Option<Lineage>> {
    if let Some(lineage) = stored_lineage(&core.db.conn(), id)? {
        return Ok(Some(lineage));
    }
    fetch(core, format!("id:{id}")).await?;
    stored_lineage(&core.db.conn(), id)
}

pub async fn members(core: &Arc<Core>, id: i64) -> Result<Option<Vec<i64>>> {
    let Some(lineage) = lineage(core, id).await? else {
        return Ok(None);
    };
    let Some(root) = lineage.parent.or(lineage.has_children.then_some(id)) else {
        return Ok(None);
    };
    let mut children = fetch(core, format!("parent:{root}")).await?;
    children.retain(|child| *child != root);
    children.sort_unstable();
    children.dedup();
    if children.is_empty() {
        return Ok(None);
    }
    if stored_lineage(&core.db.conn(), root)?.is_none() {
        fetch(core, format!("id:{root}")).await?;
    }
    Ok(Some([root].into_iter().chain(children).collect()))
}
