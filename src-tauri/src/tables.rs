use crate::db::{TABLES, page_count};
use crate::error::{Error, Result};
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

const PAGE_SIZE: i64 = 50;

#[derive(Serialize)]
pub struct TableSize {
    name: &'static str,
    rows: i64,
    bytes: i64,
}

#[derive(Serialize)]
pub struct TablePage {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    total: i64,
    pages: i64,
}

fn row_count(conn: &Connection, table: &str) -> Result<i64> {
    Ok(
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}

fn to_json(value: ValueRef) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => number.into(),
        ValueRef::Real(number) => number.into(),
        ValueRef::Text(text) => String::from_utf8_lossy(text).into(),
        ValueRef::Blob(blob) => format!("<{} bytes>", blob.len()).into(),
    }
}

fn bytes_by_table(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut statement = conn.prepare(
        "SELECT schema.tbl_name, SUM(stat.pgsize) FROM dbstat stat
         JOIN sqlite_schema schema ON schema.name = stat.name GROUP BY schema.tbl_name",
    )?;
    let bytes = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(bytes)
}

pub fn sizes(conn: &Connection) -> Result<Vec<TableSize>> {
    let bytes = bytes_by_table(conn)?;
    let mut sizes = TABLES
        .iter()
        .map(|name| {
            Ok(TableSize {
                name,
                rows: row_count(conn, name)?,
                bytes: bytes.get(*name).copied().unwrap_or(0),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    sizes.sort_unstable_by_key(|size| size.name);
    Ok(sizes)
}

pub fn page(conn: &Connection, table: &str, page: i64) -> Result<TablePage> {
    let name = TABLES
        .iter()
        .find(|name| **name == table)
        .ok_or_else(|| Error::UnknownTable(table.to_owned()))?;
    let total = row_count(conn, name)?;
    let mut statement = conn.prepare(&format!("SELECT * FROM {name} LIMIT ?1 OFFSET ?2"))?;
    let columns: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let rows = statement
        .query_map([PAGE_SIZE, page * PAGE_SIZE], |row| {
            (0..columns.len())
                .map(|index| row.get_ref(index).map(to_json))
                .collect()
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(TablePage {
        columns,
        rows,
        total,
        pages: page_count(total, PAGE_SIZE),
    })
}
