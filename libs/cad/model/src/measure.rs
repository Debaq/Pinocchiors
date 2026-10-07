//! Medir lo elegido, como el panel de medidas de Onshape: de una cosa sus
//! datos (área, largo, radio, coordenadas); de dos, la distancia mínima con
//! sus puntos, la distancia entre centros y el ángulo si tiene sentido.

use cad_occt::{CurveKind, Shape, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::geom::*;

/// Algo elegido en el sólido.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeasureItem {
    Face { index: usize },
    Edge { index: usize },
    Vertex { point: P3 },
}

/// Datos de una cosa elegida.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ItemMeasure {
    /// "plane", "cylinder"… para caras; "line", "circle"… para aristas; "vertex".
    pub kind: String,
    pub area: Option<f64>,
    pub length: Option<f64>,
    pub radius: Option<f64>,
    /// Centro: de masa en caras planas, del círculo en aristas circulares, el
    /// punto en vértices, el medio en rectas.
    pub center: P3,
    /// Normal de caras planas; dirección de aristas rectas.
    pub direction: Option<P3>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Distance {
    pub value: f64,
    /// Puntos más cercanos (en el primero y en el segundo).
    pub a: P3,
    pub b: P3,
    /// b − a por eje.
    pub delta: P3,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Measurement {
    pub items: Vec<ItemMeasure>,
    /// Con dos cosas: distancia mínima.
    pub distance: Option<Distance>,
    pub center_distance: Option<f64>,
    /// Entre caras planas y aristas rectas (0..90°).
    pub angle: Option<f64>,
}

type R<T> = Result<T, String>;

fn surface_name(s: SurfaceKind) -> String {
    format!("{s:?}").to_lowercase()
}

fn curve_name(c: CurveKind) -> String {
    format!("{c:?}").to_lowercase()
}

fn item(body: &Shape, it: &MeasureItem) -> R<(ItemMeasure, Shape)> {
    let e = |x: cad_occt::Error| x.to_string();
    Ok(match *it {
        MeasureItem::Face { index } => {
            let f = body.face_info(index).map_err(e)?;
            let plane = f.surface == SurfaceKind::Plane;
            let center = match (f.surface, f.axis) {
                (SurfaceKind::Sphere, Some(a)) => a.origin,
                _ => f.center,
            };
            (
                ItemMeasure {
                    kind: surface_name(f.surface),
                    area: Some(f.area),
                    radius: f.radius,
                    center,
                    direction: plane.then_some(normalize(f.normal)),
                    ..Default::default()
                },
                body.face_shape(index).map_err(e)?,
            )
        }
        MeasureItem::Edge { index } => {
            let g = body.edge_info(index).map_err(e)?;
            let line = g.curve == CurveKind::Line;
            (
                ItemMeasure {
                    kind: curve_name(g.curve),
                    length: Some(g.length),
                    radius: g.circle.map(|c| c.2),
                    center: g.circle.map_or(g.mid, |c| c.0),
                    direction: line.then(|| normalize(sub(g.end, g.start))),
                    ..Default::default()
                },
                body.edge_shape(index).map_err(e)?,
            )
        }
        MeasureItem::Vertex { point } => (
            ItemMeasure { kind: "vertex".into(), center: point, ..Default::default() },
            Shape::vertex(point).map_err(e)?,
        ),
    })
}

/// Ángulo en grados (0..90) entre dos cosas con dirección: caras planas
/// (normal) y aristas rectas (dirección).
fn angle(a: &ItemMeasure, b: &ItemMeasure) -> Option<f64> {
    let (da, db) = (a.direction?, b.direction?);
    let c = dot(da, db).abs().min(1.0);
    let face = |m: &ItemMeasure| m.area.is_some();
    // Recta con plano: el complemento del ángulo con la normal
    let deg = if face(a) != face(b) { 90.0 - c.acos().to_degrees() } else { c.acos().to_degrees() };
    Some(deg.clamp(0.0, 90.0))
}

pub fn measure(body: &Shape, items: &[MeasureItem]) -> R<Measurement> {
    let parts = items.iter().map(|it| item(body, it)).collect::<R<Vec<_>>>()?;
    let mut out = Measurement { items: parts.iter().map(|(m, _)| m.clone()).collect(), ..Default::default() };
    if let [(ma, sa), (mb, sb)] = &parts[..] {
        if let Some((value, a, b)) = sa.min_distance(sb) {
            out.distance = Some(Distance { value, a, b, delta: sub(b, a) });
        }
        out.center_distance = Some(norm(sub(mb.center, ma.center)));
        out.angle = angle(ma, mb);
    }
    Ok(out)
}
