//! Inert Universal VTT 0.2/0.3 import. Unknown producer metadata is discarded.
use crate::{
    AppState,
    error::{ApiError, Result},
    vtt::{BoardAck, BoardAction, BoardOperation, Distance, Grid, Point, Scene},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::HeaderMap,
    routing::post,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Import {
    operation_id: String,
    base_revision: i64,
    scene_id: String,
    name: String,
    document: Document,
}
#[derive(Deserialize, ToSchema)]
struct Document {
    format: f64,
    resolution: Resolution,
    image: String,
    #[serde(flatten)]
    geometry: Geometry,
}
#[derive(Deserialize, ToSchema)]
struct Resolution {
    map_origin: Point,
    map_size: Point,
    pixels_per_grid: f64,
}
#[derive(Default, Deserialize, Serialize, ToSchema)]
pub(crate) struct Geometry {
    #[serde(default)]
    line_of_sight: Vec<Vec<Point>>,
    #[serde(default)]
    objects_line_of_sight: Vec<Vec<Point>>,
    #[serde(default)]
    portals: Vec<Portal>,
    #[serde(default)]
    lights: Vec<Light>,
}
#[derive(Deserialize, Serialize, ToSchema)]
struct Portal {
    position: Point,
    bounds: Vec<Point>,
    #[serde(default)]
    closed: bool,
}
#[derive(Deserialize, Serialize, ToSchema)]
struct Light {
    position: Point,
    range: f64,
    #[serde(default)]
    color: String,
    #[serde(default)]
    intensity: f64,
    #[serde(default)]
    shadows: bool,
}
fn point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 100000.0 && p.y.abs() <= 100000.0
}
pub(crate) fn apply_vision(data: &str, s: &Scene, fog: &mut crate::fog::Fog) -> Result<()> {
    let g: Geometry = serde_json::from_str(data).map_err(|_| ApiError::invalid())?;
    g.validate()?;
    let pixel = |p: Point| {
        crate::vtt::pixel(
            p,
            &Grid {
                kind: "square".into(),
                ..s.grid.clone()
            },
        )
    };
    let mut walls = Vec::new();
    for line in g.line_of_sight.iter().chain(&g.objects_line_of_sight) {
        for pair in line.windows(2) {
            walls.push(crate::fog::Wall {
                a: pixel(pair[0]),
                b: pixel(pair[1]),
                door: false,
                open: false,
            });
        }
    }
    for portal in &g.portals {
        walls.push(crate::fog::Wall {
            a: pixel(portal.bounds[0]),
            b: pixel(portal.bounds[1]),
            door: true,
            open: !portal.closed,
        });
    }
    fog.walls = walls;
    fog.lights = g
        .lights
        .iter()
        .map(|l| crate::fog::Light {
            position: pixel(l.position),
            range: Distance {
                value: l.range * s.grid.distance.value,
                unit: s.grid.distance.unit.clone(),
            },
        })
        .collect();
    Ok(())
}
impl Geometry {
    pub(crate) fn validate(&self) -> Result<()> {
        let points: usize = self
            .line_of_sight
            .iter()
            .chain(&self.objects_line_of_sight)
            .map(Vec::len)
            .sum();
        if points > 10000 || self.portals.len() > 1000 || self.lights.len() > 1000 {
            return Err(ApiError::invalid());
        }
        if self
            .line_of_sight
            .iter()
            .chain(&self.objects_line_of_sight)
            .any(|line| line.len() < 2 || !line.iter().copied().all(point))
            || self.portals.iter().any(|p| {
                !point(p.position) || p.bounds.len() != 2 || !p.bounds.iter().copied().all(point)
            })
            || self.lights.iter().any(|l| {
                !point(l.position)
                    || !l.range.is_finite()
                    || l.range < 0.0
                    || l.range > 100000.0
                    || !l.intensity.is_finite()
                    || l.color.len() > 16
            })
        {
            return Err(ApiError::invalid());
        }
        Ok(())
    }
}
pub(crate) fn geometry(value: &str) -> Result<()> {
    if value.len() > 200000 {
        return Err(ApiError::invalid());
    }
    serde_json::from_str::<Geometry>(value)
        .map_err(|_| ApiError::invalid())?
        .validate()
}
fn import(db: &crate::db::Database, bearer: &str, table: &str, bytes: &[u8]) -> Result<BoardAck> {
    let input: Import = serde_json::from_slice(bytes).map_err(|_| ApiError::invalid())?;
    crate::campaigns::identifier(&input.scene_id)?;
    if ![0.2, 0.3].contains(&input.document.format) {
        return Err(ApiError::invalid());
    }
    let r = &input.document.resolution;
    if !point(r.map_origin)
        || !point(r.map_size)
        || r.map_size.x <= 0.0
        || r.map_size.y <= 0.0
        || !r.pixels_per_grid.is_finite()
        || !(8.0..=1024.0).contains(&r.pixels_per_grid)
    {
        return Err(ApiError::invalid());
    }
    input.document.geometry.validate()?;
    if input.document.image.len() > 45 * 1024 * 1024 {
        return Err(ApiError::invalid());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.document.image)
        .map_err(|_| ApiError::invalid())?;
    // UVTT embeds PNG; no paths, URLs, scripts or producer configurations are followed.
    let asset = db.put_map(bearer, table, &bytes, "image/png")?;
    if (r.map_size.x * r.pixels_per_grid - f64::from(asset.width)).abs() > 1.0
        || (r.map_size.y * r.pixels_per_grid - f64::from(asset.height)).abs() > 1.0
    {
        return Err(ApiError::invalid());
    }
    let scene = Scene {
        id: input.scene_id,
        name: input.name,
        width: asset.width,
        height: asset.height,
        map_id: Some(asset.id),
        grid: Grid {
            kind: "square".into(),
            pixels: r.pixels_per_grid,
            offset: Point {
                x: -r.map_origin.x * r.pixels_per_grid,
                y: -r.map_origin.y * r.pixels_per_grid,
            },
            color: "#cab582".into(),
            opacity: 0.4,
            distance: Distance {
                value: 5.0,
                unit: "ft".into(),
            },
            diagonal: "equal".into(),
            conversion: "table".into(),
        },
        free_move: true,
        difficult: vec![],
    };
    db.board_operation(
        bearer,
        table,
        &BoardOperation {
            operation_id: input.operation_id,
            base_revision: input.base_revision,
            command: BoardAction::ImportScene {
                scene,
                geometry: serde_json::to_string(&input.document.geometry)
                    .map_err(|_| ApiError::invalid())?,
            },
        },
    )
}
pub(crate) fn router(state: AppState) -> Router<AppState> {
    Router::new().route(
        "/api/v1/tables/{tableId}/uvtt",
        post(upload)
            .layer(DefaultBodyLimit::max(46 * 1024 * 1024))
            .layer(axum::middleware::from_fn_with_state(
                state,
                crate::vtt::asset_admission,
            )),
    )
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/uvtt",params(("tableId"=String,Path)),request_body=Import,security(("bearer"=[])),responses((status=200,body=BoardAck)),tag="table-board")]
async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Result<Json<BoardAck>> {
    let bearer = crate::routes::token(&headers)?;
    let table = id.clone();
    let ack = state
        .blocking(move |db| import(db, &bearer, &id, &bytes))
        .await?;
    state.table_live.notify(&table);
    Ok(Json(ack))
}
#[derive(OpenApi)]
#[openapi(paths(upload))]
pub(crate) struct ImportContract;
