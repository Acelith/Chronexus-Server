//! Server-authoritative manual fog. Pixel rectangles are also used to redact map downloads.
use crate::{
    error::{ApiError, Result},
    vtt::{Point, Scene},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fog {
    pub scene_id: String,
    pub enabled: bool,
    /// Ordered paint strokes: reveal=true uncovers, false covers again.
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub eyes: Vec<Eye>,
    #[serde(default)]
    pub lights: Vec<Light>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Wall {
    pub a: Point,
    pub b: Point,
    pub door: bool,
    pub open: bool,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Eye {
    pub token_id: String,
    pub range: crate::vtt::Distance,
    pub darkvision: bool,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Light {
    pub position: Point,
    pub range: crate::vtt::Distance,
}
fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
    let orient =
        |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (u, v, w, z) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    u * v <= 0.0
        && w * z <= 0.0
        && a.x.min(b.x) <= c.x.max(d.x)
        && c.x.min(d.x) <= a.x.max(b.x)
        && a.y.min(b.y) <= c.y.max(d.y)
        && c.y.min(d.y) <= a.y.max(b.y)
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stroke {
    pub reveal: bool,
    pub points: Vec<Point>,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FogView {
    pub scene_id: String,
    pub columns: u32,
    pub rows: u32,
    /// Indices of visible tiles only; no unrevealed geometry is disclosed.
    pub visible: Vec<u32>,
}
fn inside(p: Point, polygon: &[Point]) -> bool {
    let mut yes = false;
    let mut previous = polygon[polygon.len() - 1];
    for current in polygon {
        if (current.y > p.y) != (previous.y > p.y)
            && p.x
                < (previous.x - current.x) * (p.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            yes = !yes;
        }
        previous = *current;
    }
    yes
}
pub(crate) fn load(conn: &Connection, table: &str, scene: &str) -> Result<Option<Fog>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM table_fog WHERE table_id=?1 AND scene_id=?2",
            params![table, scene],
            |r| r.get(0),
        )
        .optional()?;
    data.map(|d| serde_json::from_str(&d).map_err(|_| ApiError::internal()))
        .transpose()
}
pub(crate) fn save(conn: &Connection, table: &str, s: &Scene, fog: &Fog) -> Result<()> {
    if fog.scene_id != s.id
        || fog.strokes.len() > 128
        || fog.walls.len() > 64
        || fog.eyes.len() > 16
        || fog.lights.len() > 16
    {
        return Err(ApiError::invalid());
    }
    let point = |p: Point| {
        p.x.is_finite()
            && p.y.is_finite()
            && p.x >= 0.0
            && p.y >= 0.0
            && p.x <= f64::from(s.width)
            && p.y <= f64::from(s.height)
    };
    let distance = |d: &crate::vtt::Distance| {
        d.value.is_finite()
            && (0.0..=10000.0).contains(&d.value)
            && ["ft", "m"].contains(&d.unit.as_str())
    };
    for w in &fog.walls {
        if !point(w.a) || !point(w.b) || (!w.door && w.open) {
            return Err(ApiError::invalid());
        }
    }
    for e in &fog.eyes {
        let t = crate::vtt::board_token(conn, table, &e.token_id)?;
        if t.scene_id != s.id || !distance(&e.range) {
            return Err(ApiError::invalid());
        }
    }
    for l in &fog.lights {
        if !point(l.position) || !distance(&l.range) {
            return Err(ApiError::invalid());
        }
    }
    for stroke in &fog.strokes {
        if !(3..=64).contains(&stroke.points.len())
            || stroke.points.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x < 0.0
                    || p.y < 0.0
                    || p.x > f64::from(s.width)
                    || p.y > f64::from(s.height)
            })
        {
            return Err(ApiError::invalid());
        }
    }
    conn.execute("INSERT INTO table_fog(scene_id,table_id,data) VALUES(?1,?2,?3) ON CONFLICT(scene_id) DO UPDATE SET data=excluded.data WHERE table_fog.table_id=excluded.table_id",params![s.id,table,serde_json::to_string(fog).map_err(|_|ApiError::invalid())?])?;
    Ok(())
}
pub(crate) fn projection(
    conn: &Connection,
    table: &str,
    s: &Scene,
    account: &str,
) -> Result<Option<FogView>> {
    let Some(fog) = load(conn, table, &s.id)?.filter(|f| f.enabled) else {
        return Ok(None);
    };
    // Stable bounded resolution; each tile is explicitly painted as a whole.
    let columns = 32u32.min(s.width);
    let rows = 32u32.min(s.height);
    let radius = |d: &crate::vtt::Distance| {
        crate::vtt::convert(d.value, &d.unit, &s.grid.distance.unit, &s.grid.conversion)
            / s.grid.distance.value
            * s.grid.pixels
    };
    let clear = |a: Point, b: Point| !fog.walls.iter().any(|w| !w.open && crosses(a, b, w.a, w.b));
    let mut eyes = Vec::new();
    for eye in &fog.eyes {
        if let Ok(t) = crate::vtt::board_token(conn, table, &eye.token_id)
            && t.owner_id == account
            && !t.hidden
            && t.scene_id == s.id
        {
            eyes.push((crate::vtt::pixel(t.position, &s.grid), eye));
        }
    }
    let mut visible = Vec::new();
    for y in 0..rows {
        for x in 0..columns {
            let p = Point {
                x: (f64::from(x) + 0.5) * f64::from(s.width) / f64::from(columns),
                y: (f64::from(y) + 0.5) * f64::from(s.height) / f64::from(rows),
            };
            let mut revealed = false;
            for stroke in &fog.strokes {
                if inside(p, &stroke.points) {
                    revealed = stroke.reveal;
                }
            }
            let seen = !fog.vision
                || eyes.iter().any(|(origin, eye)| {
                    [
                        (0.001, 0.001),
                        (0.999, 0.001),
                        (0.001, 0.999),
                        (0.999, 0.999),
                    ]
                    .iter()
                    .all(|(dx, dy)| {
                        let q = Point {
                            x: (f64::from(x) + dx) * f64::from(s.width) / f64::from(columns),
                            y: (f64::from(y) + dy) * f64::from(s.height) / f64::from(rows),
                        };
                        (q.x - origin.x).hypot(q.y - origin.y) <= radius(&eye.range)
                            && clear(*origin, q)
                            && (eye.darkvision
                                || fog.lights.iter().any(|l| {
                                    (q.x - l.position.x).hypot(q.y - l.position.y)
                                        <= radius(&l.range)
                                        && clear(l.position, q)
                                }))
                    })
                });
            if revealed && seen {
                visible.push(y * columns + x);
            }
        }
    }
    Ok(Some(FogView {
        scene_id: s.id.clone(),
        columns,
        rows,
        visible,
    }))
}
impl FogView {
    pub(crate) fn contains(&self, s: &Scene, p: Point) -> bool {
        if p.x < 0.0 || p.y < 0.0 || p.x >= f64::from(s.width) || p.y >= f64::from(s.height) {
            return false;
        }
        let x = (p.x * f64::from(self.columns) / f64::from(s.width)) as u32;
        let y = (p.y * f64::from(self.rows) / f64::from(s.height)) as u32;
        self.visible.binary_search(&(y * self.columns + x)).is_ok()
    }
    pub(crate) fn redact(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut image = image::load_from_memory(bytes)
            .map_err(|_| ApiError::internal())?
            .into_rgba8();
        let (w, h) = image.dimensions();
        let mut visible = vec![false; (self.columns * self.rows) as usize];
        for index in &self.visible {
            visible[*index as usize] = true;
        }
        let xs: Vec<usize> = (0..w)
            .map(|x| (u64::from(x) * u64::from(self.columns) / u64::from(w)) as usize)
            .collect();
        let ys: Vec<usize> = (0..h)
            .map(|y| {
                (u64::from(y) * u64::from(self.rows) / u64::from(h)) as usize
                    * self.columns as usize
            })
            .collect();
        for (x, y, p) in image.enumerate_pixels_mut() {
            if !visible[ys[y as usize] + xs[x as usize]] {
                *p = image::Rgba([0, 0, 0, 255]);
            }
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|_| ApiError::internal())?;
        Ok(out.into_inner())
    }
}
