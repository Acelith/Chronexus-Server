//! GM-owned editable monster copies and encounter presets, never journal facts.
use crate::{
    combat::CombatAction,
    error::{ApiError, Result},
    vtt::{BoardAction, BoardToken, Distance, Point},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
#[derive(Clone, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attribution {
    pub url: String,
    pub author: String,
    pub license: String,
    pub edition: String,
    pub original: String,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryAsset {
    pub id: String,
    pub name: String,
    pub source: Option<Attribution>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Monster {
    pub id: String,
    pub name: String,
    pub hp: i32,
    pub ac: i32,
    pub speed: Distance,
    pub size: f64,
    pub stat_block: String,
    pub source: Option<Attribution>,
    pub portrait_id: Option<String>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Encounter {
    pub id: String,
    pub name: String,
    pub members: Vec<Spawn>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spawn {
    pub monster_id: String,
    pub position: Point,
    pub hidden: bool,
}
pub(crate) fn list<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    table: &str,
    kind: &str,
) -> Result<Vec<T>> {
    let mut q = conn.prepare(
        "SELECT data FROM table_library WHERE table_id=?1 AND kind=?2 ORDER BY id LIMIT 200",
    )?;
    let rows = q
        .query_map(params![table, kind], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.iter()
        .map(|v| serde_json::from_str(v).map_err(|_| ApiError::internal()))
        .collect()
}
fn get<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    table: &str,
    kind: &str,
    id: &str,
) -> Result<T> {
    crate::campaigns::identifier(id)?;
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM table_library WHERE table_id=?1 AND kind=?2 AND id=?3",
            params![table, kind, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&data.ok_or(ApiError::invalid())?).map_err(|_| ApiError::internal())
}
fn put<T: Serialize>(conn: &Connection, table: &str, kind: &str, id: &str, data: &T) -> Result<()> {
    crate::campaigns::identifier(id)?;
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM table_library WHERE table_id=?1 AND kind=?2 AND id!=?3",
        params![table, kind, id],
        |r| r.get(0),
    )?;
    if n >= 200 {
        return Err(ApiError::limited());
    }
    let bytes = serde_json::to_string(data).map_err(|_| ApiError::invalid())?;
    let total: i64 = conn.query_row(
        "SELECT coalesce(sum(length(CAST(data AS BLOB))),0) FROM table_library WHERE table_id=?1 AND id!=?2",
        params![table, id],
        |r| r.get(0),
    )?;
    if total + bytes.len() as i64 > 2 * 1024 * 1024 {
        return Err(ApiError::limited());
    }
    let changed=conn.execute("INSERT INTO table_library(id,table_id,kind,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET data=excluded.data WHERE table_library.table_id=excluded.table_id AND table_library.kind=excluded.kind",params![id,table,kind,serde_json::to_string(data).map_err(|_|ApiError::invalid())?])?;
    if changed != 1 {
        return Err(ApiError::invalid());
    }
    Ok(())
}
pub(crate) fn save_monster(conn: &Connection, table: &str, m: &Monster) -> Result<()> {
    crate::campaigns::text(&m.name, 1, 100)?;
    crate::campaigns::text(&m.stat_block, 0, 32000)?;
    if !(1..=100000).contains(&m.hp)
        || !(0..=100).contains(&m.ac)
        || !m.size.is_finite()
        || !(0.5..=6.0).contains(&m.size)
        || !m.speed.value.is_finite()
        || !(0.0..=10000.0).contains(&m.speed.value)
        || !["ft", "m"].contains(&m.speed.unit.as_str())
    {
        return Err(ApiError::invalid());
    }
    if let Some(a) = &m.source {
        for v in [&a.url, &a.author, &a.license, &a.edition] {
            crate::campaigns::text(v, 1, 1000)?;
        }
        crate::campaigns::text(&a.original, 1, 64000)?;
    }
    if let Ok(old) = get::<Monster>(conn, table, "monster", &m.id)
        && old.source != m.source
    {
        return Err(ApiError::invalid());
    }
    if let Some(id) = &m.portrait_id {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM table_map_assets WHERE table_id=?1 AND id=?2)",
            params![table, id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(ApiError::invalid());
        }
    }
    put(conn, table, "monster", &m.id, m)
}
pub(crate) fn save_asset(conn: &Connection, table: &str, a: &LibraryAsset) -> Result<()> {
    crate::campaigns::text(&a.name, 1, 100)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM table_map_assets WHERE table_id=?1 AND id=?2)",
        params![table, a.id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(ApiError::invalid());
    }
    if let Some(source) = &a.source {
        for text in [
            &source.url,
            &source.author,
            &source.license,
            &source.edition,
            &source.original,
        ] {
            crate::campaigns::text(text, 1, 1000)?;
        }
    }
    if let Ok(old) = get::<LibraryAsset>(conn, table, "asset", &a.id)
        && old.source != a.source
    {
        return Err(ApiError::invalid());
    }
    put(conn, table, "asset", &a.id, a)
}
pub(crate) fn save_encounter(conn: &Connection, table: &str, e: &Encounter) -> Result<()> {
    crate::campaigns::text(&e.name, 1, 100)?;
    if e.members.is_empty() || e.members.len() > 32 {
        return Err(ApiError::invalid());
    }
    for member in &e.members {
        let _: Monster = get(conn, table, "monster", &member.monster_id)?;
        if !member.position.x.is_finite() || !member.position.y.is_finite() {
            return Err(ApiError::invalid());
        }
    }
    put(conn, table, "encounter", &e.id, e)
}
pub(crate) fn remove(conn: &Connection, table: &str, id: &str, kind: &str) -> Result<()> {
    crate::campaigns::identifier(id)?;
    if kind == "monster"
        && list::<Encounter>(conn, table, "encounter")?
            .iter()
            .any(|e| e.members.iter().any(|m| m.monster_id == id))
    {
        return Err(ApiError::invalid());
    }
    conn.execute(
        "DELETE FROM table_library WHERE table_id=?1 AND id=?2 AND kind=?3",
        params![table, id, kind],
    )?;
    Ok(())
}
pub(crate) fn deploy(
    conn: &Connection,
    table: &str,
    account: &str,
    scene: &str,
    id: &str,
) -> Result<()> {
    let e: Encounter = get(conn, table, "encounter", id)?;
    for member in e.members {
        let m: Monster = get(conn, table, "monster", &member.monster_id)?;
        let token = BoardToken {
            id: Uuid::new_v4().to_string(),
            scene_id: scene.into(),
            owner_id: account.into(),
            name: m.name,
            position: member.position,
            size: m.size,
            speed: m.speed,
            hidden: member.hidden,
            portrait_id: m.portrait_id,
            character_id: None,
            spent: 0.0,
            diagonals: 0,
            artwork: false,
        };
        crate::vtt::apply(
            conn,
            table,
            account,
            "gm",
            &BoardAction::SaveToken {
                token: token.clone(),
            },
        )?;
        crate::combat::apply(
            conn,
            table,
            account,
            "gm",
            &CombatAction::SetVitals {
                token_id: token.id,
                hp: m.hp,
                max_hp: m.hp,
                ac: Some(m.ac),
                show_hp: false,
                gm_note: Some(m.stat_block),
                status: None,
            },
        )?;
    }
    Ok(())
}
