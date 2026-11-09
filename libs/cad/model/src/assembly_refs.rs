//! Conectores ligados a la pieza (PLAN_ENSAMBLE EN1): cada conector puede
//! guardar de qué cara, borde o vértice sale ([`ConnectorRef`]); en cada
//! recálculo se busca ese elemento en la pieza de su instancia (por sus
//! orígenes, como las referencias de las operaciones) y se rehace su sistema
//! de coordenadas. Si ya no está, queda el último guardado y la unión sale
//! como «perdida».

use serde::Serialize;

use crate::assembly::{Assembly, Connector, ConnectorRef};
use crate::eval::{Evaluation, Part};
use crate::feature::{EdgeRef, FaceRef, FaceTag};
use crate::geom::*;
use cad_occt::{CurveKind, SurfaceKind};

/// Un conector que no se pudo rehacer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LostConnector {
    pub mate: u32,
    pub instance: u32,
    pub message: String,
}

fn diag(part: &Part) -> f64 {
    part.shape.bounds().map_or(1.0, |(lo, hi)| norm(sub(hi, lo)).max(1.0))
}

/// Referencia estable a una cara de la pieza.
pub fn part_face_ref(part: &Part, face: usize) -> Option<FaceRef> {
    let info = part.shape.face_info(face).ok()?;
    Some(FaceRef { point: info.point, normal: info.normal, tags: part.tags.get(face).cloned().unwrap_or_default() })
}

/// Referencia estable a una arista de la pieza.
pub fn part_edge_ref(part: &Part, edge: usize) -> Option<EdgeRef> {
    let info = part.shape.edge_info(edge).ok()?;
    let pair = part.shape.edge_face_pairs().ok()?.get(edge).copied()?;
    let side = |f: Option<usize>| f.and_then(|f| part.tags.get(f).cloned()).unwrap_or_default();
    let sides = vec![side(pair[0]), side(pair[1])];
    let sides = if sides.iter().all(|s| !s.is_empty()) { sides } else { vec![] };
    Some(EdgeRef { point: info.mid, direction: info.tangent, sides })
}

/// La cara de la pieza que corresponde a la referencia.
pub fn resolve_face(part: &Part, r: &FaceRef) -> Result<usize, String> {
    let shape = &part.shape;
    for wanted in [&r.tags[..r.tags.len().min(1)], &r.tags[..]] {
        if wanted.is_empty() {
            continue;
        }
        let best = (0..part.tags.len())
            .filter(|&f| part.tags[f].iter().any(|t| wanted.contains(t)))
            .map(|f| (f, shape.face_distance(f, r.point).unwrap_or(f64::MAX)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((f, _)) = best {
            return Ok(f);
        }
    }
    let (i, d) = shape.closest_face(r.point, Some(r.normal), 0.9).ok_or("la cara de la unión ya no existe")?;
    if d > diag(part) * 0.5 {
        return Err("la cara de la unión ya no existe".into());
    }
    Ok(i)
}

/// La arista de la pieza que corresponde a la referencia.
pub fn resolve_edge(part: &Part, r: &EdgeRef) -> Result<usize, String> {
    let shape = &part.shape;
    if r.sides.len() == 2 && r.sides.iter().all(|s| !s.is_empty()) {
        let has = |f: Option<usize>, side: &[FaceTag]| f.and_then(|f| part.tags.get(f)).is_some_and(|ts| ts.iter().any(|t| side.contains(t)));
        if let Ok(pairs) = shape.edge_face_pairs() {
            let best = pairs
                .iter()
                .enumerate()
                .filter(|(_, [a, b])| (has(*a, &r.sides[0]) && has(*b, &r.sides[1])) || (has(*a, &r.sides[1]) && has(*b, &r.sides[0])))
                .map(|(e, _)| (e, shape.edge_distance(e, r.point).unwrap_or(f64::MAX)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((e, _)) = best {
                return Ok(e);
            }
        }
    }
    let (i, d) = shape.closest_edge(r.point, Some(r.direction), 0.9).ok_or("el borde de la unión ya no existe")?;
    if d > diag(part) * 0.5 {
        return Err("el borde de la unión ya no existe".into());
    }
    Ok(i)
}

/// Una X perpendicular a Z (la X del mundo si se puede).
pub fn helper_x(z: P3) -> P3 {
    let h = if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    normalize(sub(h, scale(z, dot(h, z))))
}

/// Sistema de base de una cara: centro y normal si es plana; un punto del eje (a la altura del
/// centro) y el eje si es cilíndrica o cónica. Con el radio de la cilíndrica.
pub fn face_frame(part: &Part, face: usize) -> Result<(P3, P3, P3, f64), String> {
    let info = part.shape.face_info(face).map_err(|e| e.to_string())?;
    let (origin, z, radius) = match info.axis {
        Some(ax) if info.surface != SurfaceKind::Plane => {
            let d = normalize(ax.dir);
            let t = dot(sub(info.center, ax.origin), d);
            (add(ax.origin, scale(d, t)), d, if info.surface == SurfaceKind::Cylinder { info.radius.unwrap_or(0.0) } else { 0.0 })
        }
        _ => (info.center, normalize(info.normal), 0.0),
    };
    Ok((origin, z, helper_x(z), radius))
}

/// Normal de la primera cara plana vecina de una arista.
fn edge_face_normal(part: &Part, edge: usize, along: Option<P3>) -> Option<P3> {
    let pair = part.shape.edge_face_pairs().ok()?.get(edge).copied()?;
    for f in pair.into_iter().flatten() {
        let info = part.shape.face_info(f).ok()?;
        if info.surface != SurfaceKind::Plane {
            continue;
        }
        match along {
            Some(a) if dot(normalize(info.normal), a).abs() < 0.99 => continue,
            _ => return Some(normalize(info.normal)),
        }
    }
    None
}

/// Sistema de base de un borde: centro y eje si es redondo (orientado como la cara plana
/// vecina), punto medio con X a lo largo si no.
pub fn edge_frame(part: &Part, edge: usize, center: bool) -> Result<(P3, P3, P3, f64), String> {
    let info = part.shape.edge_info(edge).map_err(|e| e.to_string())?;
    if center {
        let (c, axis, r) = info.circle.ok_or("ese borde no es redondo: elegir el punto medio")?;
        let axis = normalize(axis);
        let z = match edge_face_normal(part, edge, Some(axis)) {
            Some(n) if dot(n, axis) < 0.0 => scale(axis, -1.0),
            _ => axis,
        };
        return Ok((c, z, helper_x(z), r));
    }
    let t = normalize(info.tangent);
    let z = match info.curve {
        CurveKind::Line => edge_face_normal(part, edge, None).unwrap_or_else(|| helper_x(t)),
        _ => edge_face_normal(part, edge, None).unwrap_or_else(|| helper_x(t)),
    };
    let z = normalize(sub(z, scale(t, dot(z, t))));
    let x = normalize(sub(t, scale(z, dot(t, z))));
    Ok((info.mid, z, x, 0.0))
}

/// Sistema de base de un vértice: el más cercano a `point`.
pub fn vertex_frame(part: &Part, point: P3, normal: P3) -> Result<(P3, P3, P3, f64), String> {
    let v = part.shape.vertices().map_err(|e| e.to_string())?;
    let (best, d) = v
        .iter()
        .map(|p| (*p, norm(sub(*p, point))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .ok_or("la pieza no tiene vértices")?;
    if d > diag(part) * 0.25 {
        return Err("el vértice de la unión ya no existe".into());
    }
    let z = if norm(normal) > 1e-9 { normalize(normal) } else { [0.0, 0.0, 1.0] };
    Ok((best, z, helper_x(z), 0.0))
}

/// Sistema de base de una referencia sobre la pieza.
pub fn ref_frame(part: &Part, at: &ConnectorRef) -> Result<(P3, P3, P3, f64), String> {
    match at {
        ConnectorRef::Face { face } => face_frame(part, resolve_face(part, face)?),
        ConnectorRef::EdgeCenter { edge } => edge_frame(part, resolve_edge(part, edge)?, true),
        ConnectorRef::EdgeMid { edge } => edge_frame(part, resolve_edge(part, edge)?, false),
        ConnectorRef::Vertex { point, normal } => vertex_frame(part, *point, *normal),
    }
}

/// Rehace origen, Z, X y radio del conector desde su referencia (con corrimiento, giro y volteo).
pub fn refresh(part: &Part, c: &mut Connector) -> Result<(), String> {
    let Some(at) = &c.at else { return Ok(()) };
    let (o, z, x, r) = ref_frame(part, at)?;
    let (o, z, x) = c.adjusted(o, z, x);
    c.origin = o;
    c.z = z;
    c.x = x;
    c.radius = r;
    Ok(())
}

/// Conector nuevo sobre una referencia de la pieza.
pub fn connector_at(part: &Part, instance: u32, at: ConnectorRef) -> Result<Connector, String> {
    let mut c = Connector { instance, at: Some(at), ..Default::default() };
    refresh(part, &mut c)?;
    Ok(c)
}

/// La pieza de una instancia en la evaluación.
pub fn instance_part<'a>(ev: &'a Evaluation, asm: &Assembly, instance: u32) -> Option<&'a Part> {
    let inst = asm.instances.iter().find(|i| i.id == instance)?;
    ev.parts.iter().find(|p| p.id == inst.part)
}

/// Rehace los conectores ligados de todas las uniones contra las piezas de la evaluación.
/// Devuelve los que no se encontraron (quedan con lo último guardado).
pub fn resolve_connectors(ev: &Evaluation, asm: &mut Assembly) -> Vec<LostConnector> {
    let parts: Vec<(u32, Option<&Part>)> = asm.instances.iter().map(|i| (i.id, ev.parts.iter().find(|p| p.id == i.part))).collect();
    let mut lost = Vec::new();
    for m in &mut asm.mates {
        let id = m.id;
        for c in m.connectors_mut() {
            if c.at.is_none() {
                continue;
            }
            let Some((_, Some(part))) = parts.iter().find(|p| p.0 == c.instance) else { continue };
            if let Err(message) = refresh(part, c) {
                lost.push(LostConnector { mate: id, instance: c.instance, message });
            }
        }
    }
    lost
}

/// El ensamble con sus conectores rehechos, listo para resolver.
pub fn prepared(ev: &Evaluation, asm: &Assembly) -> (Assembly, Vec<LostConnector>) {
    let mut a = asm.clone();
    let lost = resolve_connectors(ev, &mut a);
    (a, lost)
}

/// Rehace los conectores y resuelve: el ensamble listo, su solución y los conectores perdidos.
pub fn solve_with(ev: &Evaluation, asm: &Assembly) -> (Assembly, crate::assembly::AssemblySolution, Vec<LostConnector>) {
    let (a, lost) = prepared(ev, asm);
    let sol = crate::assembly::solve(&a);
    (a, sol, lost)
}
