//! M44 table state. Prepared scenes, geometry and hidden tokens are filtered here,
//! never in a client. All mutations share table revisions and durable receipts.
use crate::{
    AppState,
    auth::now,
    campaigns::{identifier, text},
    db::Database,
    error::{ApiError, ErrorBody, Result},
    routes::{body, token},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

pub(crate) const MAX_MAP: usize = 32 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Distance {
    pub value: f64,
    pub unit: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grid {
    pub kind: String,
    pub pixels: f64,
    pub offset: Point,
    pub color: String,
    pub opacity: f64,
    pub distance: Distance,
    pub diagonal: String,
    pub conversion: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub map_id: Option<String>,
    pub grid: Grid,
    pub free_move: bool,
    pub difficult: Vec<Point>,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardToken {
    #[serde(default)]
    pub artwork: bool,
    pub id: String,
    pub scene_id: String,
    pub owner_id: String,
    pub name: String,
    pub position: Point,
    pub size: f64,
    pub speed: Distance,
    pub hidden: bool,
    pub portrait_id: Option<String>,
    pub character_id: Option<String>,
    pub spent: f64,
    pub diagonals: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaTemplate {
    pub id: String,
    pub scene_id: String,
    pub owner_id: String,
    pub shape: String,
    pub origin: Point,
    pub angle: f64,
    pub size: Distance,
    pub coverage: String,
    pub hidden: bool,
}
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BoardAction {
    ImportVision {
        scene_id: String,
    },
    SaveAsset {
        asset: crate::library::LibraryAsset,
    },
    SaveMonster {
        monster: crate::library::Monster,
    },
    SaveEncounter {
        encounter: crate::library::Encounter,
    },
    RemoveLibrary {
        id: String,
        kind: String,
    },
    DeployEncounter {
        scene_id: String,
        encounter_id: String,
    },
    SaveFog {
        fog: crate::fog::Fog,
    },
    Combat {
        action: crate::combat::CombatAction,
    },
    SaveTemplate {
        template: AreaTemplate,
    },
    RemoveTemplate {
        template_id: String,
    },
    SaveScene {
        scene: Scene,
    },
    ImportScene {
        scene: Scene,
        geometry: String,
    },
    ActivateScene {
        scene_id: String,
    },
    SaveToken {
        token: BoardToken,
    },
    RemoveToken {
        token_id: String,
    },
    Move {
        token_id: String,
        path: Vec<Point>,
    },
}
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardOperation {
    pub operation_id: String,
    pub base_revision: i64,
    pub command: BoardAction,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoardAck {
    pub revision: i64,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MapAsset {
    pub id: String,
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoardView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Vec<crate::library::LibraryAsset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monsters: Option<Vec<crate::library::Monster>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encounters: Option<Vec<crate::library::Encounter>>,
    pub fog: Vec<crate::fog::FogView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_editor: Option<Vec<crate::fog::Fog>>,
    pub revision: i64,
    pub active_scene: Option<String>,
    pub scenes: Vec<Scene>,
    pub tokens: Vec<BoardToken>,
    pub templates: Vec<AreaTemplate>,
    pub combat: Option<crate::combat::CombatState>,
    pub vitals: Vec<crate::combat::VitalsView>,
    pub combat_log: Vec<crate::combat::CombatLog>,
    /// Original imported geometry remains GM-only; editable vision has a separate policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Vec<SceneGeometry>>,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SceneGeometry {
    pub scene_id: String,
    pub data: String,
}

pub(crate) fn access(conn: &Connection, table: &str, account: &str) -> Result<String> {
    identifier(table)?;
    conn.query_row("SELECT s.role FROM table_seats s JOIN accounts a ON a.id=s.account_id WHERE table_id=?1 AND account_id=?2 AND a.disabled=0",params![table,account],|r|r.get(0))
        .optional()?.ok_or(ApiError(StatusCode::FORBIDDEN,"Table access denied."))
}
pub(crate) fn gm(role: &str) -> Result<()> {
    if role != "gm" {
        return Err(ApiError(StatusCode::FORBIDDEN, "GM seat required."));
    }
    Ok(())
}
fn finite(v: f64, low: f64, high: f64) -> Result<()> {
    if !v.is_finite() || v < low || v > high {
        return Err(ApiError::invalid());
    }
    Ok(())
}
impl Distance {
    fn validate(&self) -> Result<()> {
        finite(self.value, 0.0, 100000.0)?;
        if !["ft", "m"].contains(&self.unit.as_str()) {
            return Err(ApiError::invalid());
        }
        Ok(())
    }
}
impl Grid {
    fn validate(&self) -> Result<()> {
        if !["square", "hexFlat", "hexPointy", "none"].contains(&self.kind.as_str())
            || !["equal", "alternating", "euclidean"].contains(&self.diagonal.as_str())
            || !["table", "exact"].contains(&self.conversion.as_str())
        {
            return Err(ApiError::invalid());
        }
        finite(self.pixels, 8.0, 1024.0)?;
        finite(self.opacity, 0.0, 1.0)?;
        finite(self.offset.x, -16384.0, 16384.0)?;
        finite(self.offset.y, -16384.0, 16384.0)?;
        self.distance.validate()?;
        finite(self.distance.value, 0.01, 10000.0)?;
        if self.color.len() != 7
            || !self.color.starts_with('#')
            || !self.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ApiError::invalid());
        }
        Ok(())
    }
}
fn scene(conn: &Connection, table: &str, id: &str) -> Result<Scene> {
    identifier(id)?;
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM table_scenes WHERE table_id=?1 AND id=?2",
            params![table, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&data.ok_or(ApiError(StatusCode::NOT_FOUND, "Scene not found."))?)
        .map_err(|_| ApiError::internal())
}
pub(crate) fn board_token(conn: &Connection, table: &str, id: &str) -> Result<BoardToken> {
    identifier(id)?;
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM table_tokens WHERE table_id=?1 AND id=?2",
            params![table, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&data.ok_or(ApiError(StatusCode::NOT_FOUND, "Token not found."))?)
        .map_err(|_| ApiError::internal())
}
pub(crate) fn save_token(conn: &Connection, table: &str, t: &BoardToken) -> Result<()> {
    conn.execute("INSERT INTO table_tokens(id,table_id,scene_id,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET scene_id=excluded.scene_id,data=excluded.data WHERE table_tokens.table_id=excluded.table_id",params![t.id,table,t.scene_id,serde_json::to_string(t).map_err(|_|ApiError::internal())?])?;
    Ok(())
}
fn map_exists(conn: &Connection, table: &str, id: &Option<String>) -> Result<()> {
    if let Some(id) = id {
        identifier(id)?;
        let yes: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM table_map_assets WHERE id=?1 AND table_id=?2)",
            params![id, table],
            |r| r.get(0),
        )?;
        if !yes {
            return Err(ApiError::invalid());
        }
    }
    Ok(())
}
/// Grid points are axial coordinates for hex grids; pixel-space for no-grid scenes.
pub fn pixel(p: Point, g: &Grid) -> Point {
    let (x, y) = match g.kind.as_str() {
        "hexPointy" => (
            3f64.sqrt() * (p.x + p.y / 2.0) * g.pixels / 2.0,
            1.5 * p.y * g.pixels / 2.0,
        ),
        "hexFlat" => (
            1.5 * p.x * g.pixels / 2.0,
            3f64.sqrt() * (p.y + p.x / 2.0) * g.pixels / 2.0,
        ),
        "none" => (p.x, p.y),
        _ => (p.x * g.pixels, p.y * g.pixels),
    };
    Point {
        x: x + g.offset.x,
        y: y + g.offset.y,
    }
}
fn position(p: Point, s: &Scene) -> Result<()> {
    finite(p.x, -2048.0, 16384.0)?;
    finite(p.y, -2048.0, 16384.0)?;
    if s.grid.kind != "none" && (p.x.fract() != 0.0 || p.y.fract() != 0.0) {
        return Err(ApiError::invalid());
    }
    let p = pixel(p, &s.grid);
    finite(p.x, 0.0, f64::from(s.width) - 1.0)?;
    finite(p.y, 0.0, f64::from(s.height) - 1.0)
}
pub fn convert(v: f64, from: &str, to: &str, convention: &str) -> f64 {
    if from == to {
        v
    } else {
        let ratio = if convention == "exact" { 0.3048 } else { 0.3 };
        if from == "ft" { v * ratio } else { v / ratio }
    }
}
fn move_token(
    conn: &Connection,
    table: &str,
    account: &str,
    role: &str,
    id: &str,
    path: &[Point],
) -> Result<()> {
    let mut t = board_token(conn, table, id)?;
    let s = scene(conn, table, &t.scene_id)?;
    if role != "gm" {
        let active: Option<String> = conn
            .query_row(
                "SELECT active_scene FROM table_boards WHERE table_id=?1",
                [table],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if t.owner_id != account
            || t.hidden
            || active.as_deref() != Some(&s.id)
            || !crate::combat::movement_allowed(conn, table, &t, s.free_move)?
        {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Token movement not allowed.",
            ));
        }
    }
    if path.is_empty() || path.len() > 100 {
        return Err(ApiError::invalid());
    }
    let fog = if role == "gm" {
        None
    } else {
        crate::fog::projection(conn, table, &s, account)?
    };
    let mut previous = t.position;
    for p in path {
        position(*p, &s)?;
        if fog
            .as_ref()
            .is_some_and(|f| !f.contains(&s, pixel(*p, &s.grid)))
        {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Token movement not allowed.",
            ));
        }
        let dx = (p.x - previous.x).abs();
        let dy = (p.y - previous.y).abs();
        let cells = match s.grid.kind.as_str() {
            "none" => dx.hypot(dy) / s.grid.pixels,
            "hexPointy" | "hexFlat" => {
                let d = (dx + dy + ((p.x - previous.x) + (p.y - previous.y)).abs()) / 2.0;
                if d > 1.0 {
                    return Err(ApiError::invalid());
                }
                d
            }
            _ => {
                if dx > 1.0 || dy > 1.0 {
                    return Err(ApiError::invalid());
                }
                if dx == 1.0 && dy == 1.0 {
                    t.diagonals += 1;
                    match s.grid.diagonal.as_str() {
                        "alternating" => {
                            if t.diagonals.is_multiple_of(2) {
                                2.0
                            } else {
                                1.0
                            }
                        }
                        "euclidean" => 2f64.sqrt(),
                        _ => 1.0,
                    }
                } else {
                    dx.max(dy)
                }
            }
        };
        let terrain = if s.difficult.iter().any(|q| q.x == p.x && q.y == p.y) {
            2.0
        } else {
            1.0
        };
        t.spent += cells * s.grid.distance.value * terrain;
        previous = *p;
    }
    t.position = previous;
    crate::combat::log(conn, table, Some(&t), "public", ("move", None, vec![], ""))?;
    save_token(conn, table, &t)
}
pub(crate) fn apply(
    conn: &Connection,
    table: &str,
    account: &str,
    role: &str,
    action: &BoardAction,
) -> Result<()> {
    match action {
        BoardAction::ImportVision { scene_id } => {
            gm(role)?;
            let s = scene(conn, table, scene_id)?;
            let geometry: String = conn.query_row(
                "SELECT geometry FROM table_scenes WHERE table_id=?1 AND id=?2",
                params![table, scene_id],
                |r| r.get(0),
            )?;
            let mut fog = crate::fog::load(conn, table, scene_id)?.unwrap_or(crate::fog::Fog {
                scene_id: scene_id.clone(),
                enabled: false,
                vision: false,
                strokes: vec![],
                walls: vec![],
                eyes: vec![],
                lights: vec![],
            });
            crate::uvtt::apply_vision(&geometry, &s, &mut fog)?;
            crate::fog::save(conn, table, &s, &fog)?;
        }
        BoardAction::SaveAsset { asset } => {
            gm(role)?;
            crate::library::save_asset(conn, table, asset)?;
        }
        BoardAction::SaveMonster { monster } => {
            gm(role)?;
            crate::library::save_monster(conn, table, monster)?;
        }
        BoardAction::SaveEncounter { encounter } => {
            gm(role)?;
            crate::library::save_encounter(conn, table, encounter)?;
        }
        BoardAction::RemoveLibrary { id, kind } => {
            gm(role)?;
            if !["monster", "encounter"].contains(&kind.as_str()) {
                return Err(ApiError::invalid());
            }
            crate::library::remove(conn, table, id, kind)?;
        }
        BoardAction::DeployEncounter {
            scene_id,
            encounter_id,
        } => {
            gm(role)?;
            scene(conn, table, scene_id)?;
            crate::library::deploy(conn, table, account, scene_id, encounter_id)?;
        }
        BoardAction::SaveFog { fog } => {
            gm(role)?;
            let s = scene(conn, table, &fog.scene_id)?;
            crate::fog::save(conn, table, &s, fog)?;
        }
        BoardAction::Combat { action } => crate::combat::apply(conn, table, account, role, action)?,
        BoardAction::SaveTemplate { template: t } => {
            identifier(&t.id)?;
            identifier(&t.owner_id)?;
            let s = scene(conn, table, &t.scene_id)?;
            position(t.origin, &s)?;
            t.size.validate()?;
            finite(t.angle, -360.0, 360.0)?;
            if !["circle", "cone", "square", "line", "cylinder"].contains(&t.shape.as_str())
                || !["center", "touch"].contains(&t.coverage.as_str())
                || t.owner_id != account
                || (t.hidden && role != "gm")
            {
                return Err(ApiError::invalid());
            }
            if role != "gm" {
                let active: Option<String> = conn
                    .query_row(
                        "SELECT active_scene FROM table_boards WHERE table_id=?1",
                        [table],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                if active.as_deref() != Some(&t.scene_id) {
                    return Err(ApiError(StatusCode::FORBIDDEN, "Scene unavailable."));
                }
            }
            let count: i64 = conn.query_row(
                "SELECT count(*) FROM table_templates WHERE table_id=?1",
                [table],
                |r| r.get(0),
            )?;
            if count >= 100 {
                return Err(ApiError::limited());
            }
            let changed=conn.execute("INSERT INTO table_templates(id,table_id,scene_id,owner_id,data) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET data=excluded.data WHERE table_templates.table_id=excluded.table_id AND table_templates.owner_id=excluded.owner_id",params![t.id,table,t.scene_id,account,serde_json::to_string(t).map_err(|_|ApiError::invalid())?])?;
            if changed != 1 {
                return Err(ApiError::invalid());
            }
        }
        BoardAction::RemoveTemplate { template_id } => {
            identifier(template_id)?;
            let count=conn.execute("DELETE FROM table_templates WHERE id=?1 AND table_id=?2 AND (owner_id=?3 OR ?4='gm')",params![template_id,table,account,role])?;
            if count != 1 {
                return Err(ApiError(StatusCode::NOT_FOUND, "Template not found."));
            }
        }
        BoardAction::SaveScene { scene: s } => {
            gm(role)?;
            identifier(&s.id)?;
            text(&s.name, 1, 200)?;
            s.grid.validate()?;
            if !(1..=16384).contains(&s.width)
                || !(1..=16384).contains(&s.height)
                || s.difficult.len() > 10000
            {
                return Err(ApiError::invalid());
            }
            map_exists(conn, table, &s.map_id)?;
            for p in &s.difficult {
                position(*p, s)?;
            }
            let count: i64 = conn.query_row(
                "SELECT count(*) FROM table_scenes WHERE table_id=?1",
                [table],
                |r| r.get(0),
            )?;
            if count >= 50 && scene(conn, table, &s.id).is_err() {
                return Err(ApiError::limited());
            }
            // IDs cannot overwrite scenes belonging to another table.
            let changed=conn.execute("INSERT INTO table_scenes(id,table_id,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data WHERE table_scenes.table_id=excluded.table_id",params![s.id,table,serde_json::to_string(s).map_err(|_|ApiError::internal())?])?;
            if changed != 1 {
                return Err(ApiError::invalid());
            }
        }
        BoardAction::ImportScene { scene, geometry } => {
            gm(role)?;
            crate::uvtt::geometry(geometry)?;
            apply(
                conn,
                table,
                account,
                role,
                &BoardAction::SaveScene {
                    scene: scene.clone(),
                },
            )?;
            conn.execute(
                "UPDATE table_scenes SET geometry=?1 WHERE id=?2 AND table_id=?3",
                params![geometry, scene.id, table],
            )?;
        }
        BoardAction::ActivateScene { scene_id } => {
            gm(role)?;
            scene(conn, table, scene_id)?;
            conn.execute("INSERT INTO table_boards(table_id,active_scene) VALUES(?1,?2) ON CONFLICT(table_id) DO UPDATE SET active_scene=excluded.active_scene",params![table,scene_id])?;
        }
        BoardAction::SaveToken { token: t } => {
            gm(role)?;
            identifier(&t.id)?;
            identifier(&t.owner_id)?;
            text(&t.name, 1, 100)?;
            let owner = access(conn, table, &t.owner_id)?;
            if owner == "spectator" {
                return Err(ApiError::invalid());
            }
            let s = scene(conn, table, &t.scene_id)?;
            position(t.position, &s)?;
            t.speed.validate()?;
            finite(t.size, 0.5, 6.0)?;
            map_exists(conn, table, &t.portrait_id)?;
            if let Some(id) = &t.character_id {
                identifier(id)?;
            }
            let foreign: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM table_tokens WHERE id=?1 AND table_id!=?2)",
                params![t.id, table],
                |r| r.get(0),
            )?;
            if foreign {
                return Err(ApiError::invalid());
            }
            let count: i64 = conn.query_row(
                "SELECT count(*) FROM table_tokens WHERE scene_id=?1",
                [&t.scene_id],
                |r| r.get(0),
            )?;
            if count >= 200 && board_token(conn, table, &t.id).is_err() {
                return Err(ApiError::limited());
            }
            let total: i64 = conn.query_row(
                "SELECT count(*) FROM table_tokens WHERE table_id=?1",
                [table],
                |r| r.get(0),
            )?;
            if total >= 1000 && board_token(conn, table, &t.id).is_err() {
                return Err(ApiError::limited());
            }
            let mut t = t.clone();
            let old = board_token(conn, table, &t.id).ok();
            if let Some(old) = &old
                && old.owner_id != t.owner_id
                && !access(conn, table, &old.owner_id).is_ok_and(|r| r == "gm")
            {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "Player token ownership requires its author's consent.",
                ));
            }

            t.spent = old.as_ref().map_or(0.0, |v| v.spent);
            t.diagonals = old.map_or(0, |v| v.diagonals);
            save_token(conn, table, &t)?;
        }
        BoardAction::RemoveToken { token_id } => {
            gm(role)?;
            board_token(conn, table, token_id)?;
            crate::combat::remove_token(conn, table, token_id)?;
            conn.execute(
                "DELETE FROM table_tokens WHERE table_id=?1 AND id=?2",
                params![table, token_id],
            )?;
        }
        BoardAction::Move { token_id, path } => {
            move_token(conn, table, account, role, token_id, path)?
        }
    }
    Ok(())
}
impl Database {
    pub fn board_operation(
        &self,
        bearer: &str,
        table: &str,
        op: &BoardOperation,
    ) -> Result<BoardAck> {
        let identity = self.identity(bearer)?;
        self.board_write(table, &identity.account_id, op)
    }
    pub(crate) fn board_write(
        &self,
        table: &str,
        account: &str,
        op: &BoardOperation,
    ) -> Result<BoardAck> {
        identifier(table)?;
        identifier(&op.operation_id)?;
        if Uuid::parse_str(&op.operation_id).is_ok_and(|u| u.get_version_num() != 7)
            || op.base_revision < 1
        {
            return Err(ApiError::invalid());
        }
        let payload = serde_json::to_string(op).map_err(|_| ApiError::invalid())?;
        if payload.len() > 256 * 1024 {
            return Err(ApiError::invalid());
        }
        self.with(|conn|{
            let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let role=access(&tx,table,account)?;if role=="spectator"{return Err(ApiError(StatusCode::FORBIDDEN,"Read-only seat."));}
            let old:Option<(String,String,i64)>=tx.query_row("SELECT account_id,payload,revision FROM table_operation_receipts WHERE table_id=?1 AND operation_id=?2",params![table,op.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if let Some((author,original,revision))=old {if author==account&&payload==original{return Ok(BoardAck{revision});}return Err(ApiError(StatusCode::CONFLICT,"Operation ID already used."));}
            let current:i64=tx.query_row("SELECT revision FROM game_tables WHERE id=?1",[table],|r|r.get(0))?;
            if current!=op.base_revision{return Err(ApiError(StatusCode::CONFLICT,"Table revision changed; reload before retrying."));}
            let (total,recent,receipt_bytes):(i64,i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(account_id=?2 AND created_at>?3),0),coalesce(sum(length(payload)),0) FROM table_operation_receipts WHERE table_id=?1",params![table,account,now()-60],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if receipt_bytes+payload.len() as i64>16*1024*1024||total>=50000||recent>=120{return Err(ApiError::limited());}
            apply(&tx,table,account,&role,&op.command)?;
            let revision=current+1;
            tx.execute("UPDATE game_tables SET revision=?1 WHERE id=?2",params![revision,table])?;
            tx.execute("INSERT INTO table_operation_receipts(table_id,operation_id,account_id,payload,revision,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![table,op.operation_id,account,payload,revision,now()])?;
            tx.commit()?;Ok(BoardAck{revision})
        })
    }
    pub fn board_view(&self, bearer: &str, table: &str) -> Result<BoardView> {
        let user = self.identity(bearer)?;
        self.board_for_account(table, &user.account_id)
    }
    pub(crate) fn board_for_account(&self, table: &str, account: &str) -> Result<BoardView> {
        self.with(|conn| view(conn, table, account))
    }
}
pub(crate) fn view(conn: &Connection, table: &str, account: &str) -> Result<BoardView> {
    let role = access(conn, table, account)?;
    let revision = conn.query_row(
        "SELECT revision FROM game_tables WHERE id=?1",
        [table],
        |r| r.get(0),
    )?;
    let active: Option<String> = conn
        .query_row(
            "SELECT active_scene FROM table_boards WHERE table_id=?1",
            [table],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let mut q = conn.prepare(
        "SELECT id,data,geometry FROM table_scenes WHERE table_id=?1 ORDER BY id LIMIT 50",
    )?;
    let rows = q
        .query_map([table], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut scenes: Vec<Scene> = Vec::new();
    let mut geometry = Vec::new();
    for (id, data, geo) in rows {
        if role == "gm" || active.as_deref() == Some(&id) {
            scenes.push(serde_json::from_str(&data).map_err(|_| ApiError::internal())?);
            if role == "gm" {
                geometry.push(SceneGeometry {
                    scene_id: id,
                    data: geo,
                });
            }
        }
    }
    let mut fog = Vec::new();
    let mut fog_editor = Vec::new();
    for s in &scenes {
        if let Some(value) = crate::fog::projection(conn, table, s, account)? {
            fog.push(value);
        }
        if role == "gm"
            && let Some(value) = crate::fog::load(conn, table, &s.id)?
        {
            fog_editor.push(value);
        }
    }
    let visible = |scene_id: &str, p: Point| -> bool {
        scenes.iter().find(|s| s.id == scene_id).is_some_and(|s| {
            fog.iter()
                .find(|f| f.scene_id == s.id)
                .is_none_or(|f| f.contains(s, p))
        })
    };
    let mut q =
        conn.prepare("SELECT data FROM table_tokens WHERE table_id=?1 ORDER BY id LIMIT 10000")?;
    let rows = q
        .query_map([table], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut tokens = Vec::new();
    for data in rows {
        let mut t: BoardToken = serde_json::from_str(&data).map_err(|_| ApiError::internal())?;
        let s = scenes.iter().find(|s| s.id == t.scene_id);
        let revealed = s.is_some_and(|s| visible(&t.scene_id, pixel(t.position, &s.grid)));
        if role == "gm" || (!t.hidden && active.as_deref() == Some(&t.scene_id) && revealed) {
            if role != "gm" && t.owner_id != account {
                t.character_id = None;
            }
            tokens.push(t);
        }
    }
    let mut q =
        conn.prepare("SELECT data FROM table_templates WHERE table_id=?1 ORDER BY id LIMIT 100")?;
    let rows = q
        .query_map([table], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut templates = Vec::new();
    for data in rows {
        let t: AreaTemplate = serde_json::from_str(&data).map_err(|_| ApiError::internal())?;
        let revealed = scenes
            .iter()
            .find(|s| s.id == t.scene_id)
            .is_some_and(|s| visible(&t.scene_id, pixel(t.origin, &s.grid)));
        if role == "gm" || (!t.hidden && active.as_deref() == Some(&t.scene_id) && revealed) {
            templates.push(t);
        }
    }
    let (combat, vitals, combat_log) =
        crate::combat::projection(conn, table, account, &role, &tokens, &active)?;
    if role != "gm" {
        for s in &mut scenes {
            if let Some(mask) = fog.iter().find(|f| f.scene_id == s.id) {
                let grid = s.grid.clone();
                let allowed = s
                    .difficult
                    .iter()
                    .copied()
                    .filter(|p| mask.contains(s, pixel(*p, &grid)))
                    .collect();
                s.difficult = allowed;
            }
        }
    }
    Ok(BoardView {
        assets: if role == "gm" {
            Some(crate::library::list(conn, table, "asset")?)
        } else {
            None
        },
        monsters: if role == "gm" {
            Some(crate::library::list(conn, table, "monster")?)
        } else {
            None
        },
        encounters: if role == "gm" {
            Some(crate::library::list(conn, table, "encounter")?)
        } else {
            None
        },
        fog,
        fog_editor: if role == "gm" { Some(fog_editor) } else { None },
        revision,
        active_scene: active,
        scenes,
        tokens,
        templates,
        combat,
        vitals,
        combat_log,
        geometry: if role == "gm" { Some(geometry) } else { None },
    })
}

fn decode_map(bytes: &[u8], mime: &str) -> Result<(u32, u32)> {
    use image::{ImageDecoder, ImageFormat, ImageReader};
    if bytes.is_empty() || bytes.len() > MAX_MAP {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Map exceeds 32 MiB.",
        ));
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ApiError::invalid())?;
    let expected = match reader.format() {
        Some(ImageFormat::Png) => "image/png",
        Some(ImageFormat::Jpeg) => "image/jpeg",
        Some(ImageFormat::WebP) => "image/webp",
        _ => return Err(ApiError::invalid()),
    };
    if mime != expected {
        return Err(ApiError::invalid());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|_| ApiError::invalid())?;
    let (w, h) = decoder.dimensions();
    if w == 0
        || h == 0
        || u64::from(w) * u64::from(h) > 64_000_000
        || decoder.total_bytes() > 256 * 1024 * 1024
    {
        return Err(ApiError::invalid());
    }
    image::DynamicImage::from_decoder(decoder).map_err(|_| ApiError::invalid())?;
    Ok((w, h))
}
impl Database {
    pub fn put_map(&self, bearer: &str, table: &str, bytes: &[u8], mime: &str) -> Result<MapAsset> {
        let user = self.identity(bearer)?;
        self.with(|c| gm(&access(c, table, &user.account_id)?))?;
        let (width, height) = decode_map(bytes, mime)?;
        let checksum = format!("{:x}", Sha256::digest(bytes));
        self.with(|conn|{let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate)?;gm(&access(&tx,table,&user.account_id)?)?;
            let existing:Option<String>=tx.query_row("SELECT id FROM table_map_assets WHERE table_id=?1 AND sha256=?2",params![table,checksum],|r|r.get(0)).optional()?;
            let id=if let Some(id)=existing{id}else{
                let(count,size):(i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(length(original)),0) FROM table_map_assets WHERE table_id=?1",[table],|r|Ok((r.get(0)?,r.get(1)?)))?;
                if count>=100||size+bytes.len() as i64>256*1024*1024{return Err(ApiError::limited());}
                let id=Uuid::new_v4().to_string();tx.execute("INSERT INTO table_map_assets(id,table_id,sha256,mime,width,height,original) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,table,checksum,mime,width,height,bytes])?;id};
            tx.commit()?;Ok(MapAsset{id,sha256:checksum,mime:mime.into(),width,height})})
    }
    pub fn get_map(&self, bearer: &str, table: &str, id: &str) -> Result<(String, Vec<u8>)> {
        identifier(id)?;
        let user = self.identity(bearer)?;
        self.with(|conn| {
            let role = access(conn, table, &user.account_id)?;
            if role != "gm" {
                let board = view(conn, table, &user.account_id)?;
                if !board.scenes.iter().any(|s| s.map_id.as_deref() == Some(id))
                    && !board
                        .tokens
                        .iter()
                        .any(|t| t.portrait_id.as_deref() == Some(id))
                {
                    return Err(ApiError(StatusCode::NOT_FOUND, "Map not found."));
                }
            }
            let row: Option<(String, Vec<u8>, String)> = conn
                .query_row(
                    "SELECT mime,original,sha256 FROM table_map_assets WHERE table_id=?1 AND id=?2",
                    params![table, id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (mime, bytes, checksum) =
                row.ok_or(ApiError(StatusCode::NOT_FOUND, "Map not found."))?;
            if format!("{:x}", Sha256::digest(&bytes)) != checksum {
                return Err(ApiError::internal());
            }
            if role != "gm" {
                let board = view(conn, table, &user.account_id)?;
                // An asset used as a fogged background can never be fetched as a portrait.
                let protected: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM table_scenes s JOIN table_fog f ON f.scene_id=s.id WHERE s.table_id=?1 AND json_extract(s.data,'$.mapId')=?2 AND json_extract(f.data,'$.enabled')=1)",params![table,id],|r|r.get(0))?;
                if protected {
                    let s = board.scenes.iter().find(|s|s.map_id.as_deref()==Some(id)).ok_or(ApiError(StatusCode::NOT_FOUND,"Map not found."))?;
                    let fog = board.fog.iter().find(|f|f.scene_id==s.id).ok_or(ApiError(StatusCode::NOT_FOUND,"Map not found."))?;
                    return Ok(("image/png".into(), fog.redact(&bytes)?));
                }
            }
            Ok((mime, bytes))
        })
    }
}

pub(crate) fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/tables/{tableId}/board", get(board))
        .route(
            "/api/v1/tables/{tableId}/board/operations",
            post(operation).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/v1/tables/{tableId}/maps",
            post(upload)
                .layer(DefaultBodyLimit::max(MAX_MAP))
                .layer(axum::middleware::from_fn_with_state(state, asset_admission)),
        )
        .route("/api/v1/tables/{tableId}/maps/{assetId}", get(download))
        .route(
            "/api/v1/tables/{tableId}/maps/{assetId}/thumbnail",
            get(thumbnail),
        )
}
#[utoipa::path(get,path="/api/v1/tables/{tableId}/board",params(("tableId"=String,Path)),security(("bearer"=[])),responses((status=200,body=BoardView),(status=403,body=ErrorBody)),tag="table-board")]
async fn board(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<BoardView>> {
    let bearer = token(&headers)?;
    state
        .blocking(move |db| db.board_view(&bearer, &id))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/board/operations",params(("tableId"=String,Path)),request_body=BoardOperation,security(("bearer"=[])),responses((status=200,body=BoardAck),(status=409,body=ErrorBody)),tag="table-board")]
async fn operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: std::result::Result<Json<BoardOperation>, JsonRejection>,
) -> Result<Json<BoardAck>> {
    let bearer = token(&headers)?;
    let op = body(input)?;
    let table = id.clone();
    let ack = state
        .blocking(move |db| db.board_operation(&bearer, &id, &op))
        .await?;
    state.table_live.notify(&table);
    Ok(Json(ack))
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/maps",params(("tableId"=String,Path)),request_body(content=String,content_type="application/octet-stream"),security(("bearer"=[])),responses((status=200,body=MapAsset)),tag="table-board")]
async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Result<Json<MapAsset>> {
    let bearer = token(&headers)?;
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    state
        .blocking(move |db| db.put_map(&bearer, &id, &bytes, &mime))
        .await
        .map(Json)
}
#[utoipa::path(get,path="/api/v1/tables/{tableId}/maps/{assetId}/thumbnail",params(("tableId"=String,Path),("assetId"=String,Path)),security(("bearer"=[])),responses((status=200,body=String,content_type="image/png"),(status=404,body=ErrorBody)),tag="table-board")]
async fn thumbnail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table, id)): Path<(String, String)>,
) -> Result<Response> {
    let bearer = token(&headers)?;
    let bytes = state
        .blocking(move |db| {
            let (_, bytes) = db.get_map(&bearer, &table, &id)?;
            let image = image::load_from_memory(&bytes)
                .map_err(|_| ApiError::internal())?
                .thumbnail(128, 128);
            let mut out = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut out, image::ImageFormat::Png)
                .map_err(|_| ApiError::internal())?;
            Ok(out.into_inner())
        })
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
#[utoipa::path(get,path="/api/v1/tables/{tableId}/maps/{assetId}",params(("tableId"=String,Path),("assetId"=String,Path)),security(("bearer"=[])),responses((status=200,body=String,content_type="application/octet-stream"),(status=404,body=ErrorBody)),tag="table-board")]
async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((table, id)): Path<(String, String)>,
) -> Result<Response> {
    let bearer = token(&headers)?;
    let (mime, bytes) = state
        .blocking(move |db| db.get_map(&bearer, &table, &id))
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_DISPOSITION, "inline".into()),
        ],
        bytes,
    )
        .into_response())
}
#[derive(OpenApi)]
#[openapi(
    paths(board, operation, upload, download, thumbnail),
    components(schemas(
        AreaTemplate,
        Point,
        Distance,
        Grid,
        Scene,
        BoardToken,
        BoardAction,
        BoardOperation,
        BoardAck,
        BoardView,
        SceneGeometry,
        MapAsset,
        ErrorBody
    ))
)]
pub(crate) struct BoardContract;

/// Authenticate the GM and reserve bounded decoding/body memory before collecting uploads.
pub(crate) async fn asset_admission(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let bearer = token(request.headers());
    let table = request
        .uri()
        .path()
        .split('/')
        .nth(4)
        .map(String::from)
        .ok_or_else(ApiError::invalid);
    let result = async {
        let bearer = bearer?;
        let table = table?;
        let permit = state
            .transfers
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::limited())?;
        state
            .blocking(move |db| {
                let user = db.identity(&bearer)?;
                db.with(|c| gm(&access(c, &table, &user.account_id)?))
            })
            .await?;
        Ok::<_, ApiError>(permit)
    }
    .await;
    match result {
        Ok(_permit) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
