use crate::booru::{Lineage, Site, SiteQuery};
use crate::catalogue::{self, DAY_MS};
use crate::core::Core;
use crate::error::Result;
use crate::library::{self, TagList};
use crate::search::Ratings;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::sync::Arc;

const STORED_FOR: i64 = DAY_MS;

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Relation {
    Family,
    Similar,
}

// each site keeps its own parent and child links, in its own ids. Danbooru's first
fn stored_lineages(conn: &Connection, id: i64) -> Result<Vec<(Site, i64, Option<Lineage>)>> {
    let mut statement = conn.prepare_cached(
        "SELECT site, site_id, parent_id, has_children FROM site_posts WHERE post_id = ?1 ORDER BY site",
    )?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<bool>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(site, site_id, parent, has_children)| {
            let lineage = has_children.map(|has_children| Lineage { parent, has_children });
            Some((Site::from_key(&site)?, site_id, lineage))
        })
        .collect())
}

fn stored_lineage(conn: &Connection, site: Site, site_id: i64) -> Result<Option<Lineage>> {
    Ok(conn
        .query_row(
            "SELECT parent_id, has_children FROM site_posts WHERE site = ?1 AND site_id = ?2",
            params![site.key(), site_id],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<bool>>(1)?)),
        )
        .ok()
        .and_then(|(parent, has_children)| has_children.map(|has_children| Lineage { parent, has_children })))
}

async fn fetch(core: &Arc<Core>, site: Site, tag: String) -> Result<Vec<i64>> {
    let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
    let query = core
        .booru(site)
        .render(&SiteQuery {
            tags: vec![tag],
            ratings: Ratings::ALL,
            blacklist: &blacklist,
            order: None,
        })
        .await?;
    Ok(catalogue::search_page(core, site, &query, 0, STORED_FOR)
        .await?
        .ids)
}

// the site whose lineage for the post is known, fetching it from the first site that can search when none is
async fn lineage(core: &Arc<Core>, id: i64) -> Result<Option<(Site, i64, Lineage)>> {
    let rows = stored_lineages(&core.db.conn(), id)?;
    if let Some((site, site_id, Some(lineage))) = rows.iter().find(|(_, _, lineage)| lineage.is_some()) {
        return Ok(Some((*site, *site_id, *lineage)));
    }
    let Some((site, site_id, _)) = rows.into_iter().find(|(site, _, _)| core.can_search(*site)) else {
        return Ok(None);
    };
    fetch(core, site, format!("id:{site_id}")).await?;
    Ok(stored_lineage(&core.db.conn(), site, site_id)?.map(|lineage| (site, site_id, lineage)))
}

pub async fn members(core: &Arc<Core>, id: i64) -> Result<Option<Vec<i64>>> {
    let Some((site, site_id, lineage)) = lineage(core, id).await? else {
        return Ok(None);
    };
    if !core.can_search(site) {
        return Ok(None);
    }
    let Some(root) = lineage.parent.or(lineage.has_children.then_some(site_id)) else {
        return Ok(None);
    };
    let mut children = fetch(core, site, format!("parent:{root}")).await?;
    let mut root_id = catalogue::post_with_site_id(&core.db.conn(), site, root)?;
    if root_id.is_none() {
        root_id = fetch(core, site, format!("id:{root}")).await?.first().copied();
    }
    children.retain(|child| Some(*child) != root_id);
    children.sort_unstable();
    children.dedup();
    if children.is_empty() {
        return Ok(None);
    }
    Ok(Some(root_id.into_iter().chain(children).collect()))
}
