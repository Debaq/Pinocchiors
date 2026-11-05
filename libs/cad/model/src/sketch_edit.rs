//! Edición del sketch en Rust (para el MCP): borrar, transformar, espejo,
//! patrones, equidistante y recortar. Porta la lógica de apps/web/src/lib
//! (sketchTransform.ts, cad.ts, sketchPatterns.ts, sketchEdit.ts) para que la
//! interfaz siga editando lo que se crea acá: mismos ids, restricciones y
//! registro de patrones (`Sketch::patterns`).
//!
//! Todas son funciones puras sobre `&mut Sketch` que devuelven `Err` con un
//! mensaje corto cuando no se puede (sin tocar nada salvo donde se indica).

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::geom::P2;
use crate::sketch::{DimOpts, Geometry, Sketch, SketchConstraint, SketchEntity, SketchPoint, SketchText};

type R<T> = Result<T, String>;

// ─── Utilidades ───────────────────────────────────────────────────────────

fn pos(s: &Sketch, id: u32) -> Option<P2> {
    s.points.iter().find(|p| p.id == id).map(|p| [p.x, p.y])
}

fn pt(s: &Sketch, id: u32) -> R<P2> {
    pos(s, id).ok_or_else(|| format!("el punto {id} no existe"))
}

fn ent(s: &Sketch, id: u32) -> Option<&SketchEntity> {
    s.entities.iter().find(|e| e.id == id)
}

fn ent_mut(s: &mut Sketch, id: u32) -> Option<&mut SketchEntity> {
    s.entities.iter_mut().find(|e| e.id == id)
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: P2, k: f64) -> P2 {
    [a[0] * k, a[1] * k]
}
fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}
fn dist(a: P2, b: P2) -> f64 {
    len(sub(a, b))
}
fn unit(a: P2) -> P2 {
    let l = len(a);
    mul(a, 1.0 / if l == 0.0 { 1.0 } else { l })
}
/// Normal izquierda (90° antihorario).
fn left(a: P2) -> P2 {
    [-a[1], a[0]]
}

/// Redondeo a `n` decimales (el `+x.toFixed(n)` de la interfaz).
fn round(x: f64, n: i32) -> f64 {
    let k = 10f64.powi(n);
    let r = (x * k).round() / k;
    if r == 0.0 { 0.0 } else { r }
}

/// Tamaño del sketch para las tolerancias.
fn span(s: &Sketch) -> f64 {
    s.points.iter().fold(1.0, |m, p| m.max(p.x.abs()).max(p.y.abs()))
}

/// Ángulo antihorario de `a` a `b` alrededor de `c`, en (0, 2π].
fn ccw_sweep(c: P2, a: P2, b: P2) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut t = (b[1] - c[1]).atan2(b[0] - c[0]) - (a[1] - c[1]).atan2(a[0] - c[0]);
    while t <= 1e-12 {
        t += tau;
    }
    while t > tau {
        t -= tau;
    }
    t
}

/// Ids (de puntos o entidades) que nombra una restricción; los campos que son
/// números (`value`, `degrees`, `x`, `y`) no cuentan, ni `curve` de una perforación.
pub fn constraint_ids(c: &SketchConstraint) -> Vec<u32> {
    let Ok(Value::Object(m)) = serde_json::to_value(c) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, v) in &m {
        if skip_key(c, k) {
            continue;
        }
        match v {
            Value::Number(n) => out.extend(n.as_u64().map(|x| x as u32)),
            Value::Array(a) if k == "entities" => out.extend(a.iter().filter_map(|x| x.as_u64().map(|x| x as u32))),
            Value::Array(a) if k == "pairs" => {
                for p in a {
                    out.extend(p.as_array().into_iter().flatten().filter_map(|x| x.as_u64().map(|x| x as u32)));
                }
            }
            _ => {}
        }
    }
    out
}

fn skip_key(c: &SketchConstraint, k: &str) -> bool {
    matches!(k, "value" | "degrees" | "x" | "y") || (matches!(c, SketchConstraint::Pierce { .. }) && k == "curve")
}

/// Copia de una restricción con sus ids cambiados por `f`.
pub fn map_constraint(c: &SketchConstraint, f: &mut dyn FnMut(u32) -> u32) -> SketchConstraint {
    let Ok(Value::Object(mut m)) = serde_json::to_value(c) else { return c.clone() };
    let keys: Vec<String> = m.keys().cloned().collect();
    for k in keys {
        if skip_key(c, &k) {
            continue;
        }
        let v = m[&k].clone();
        let nv = match v {
            Value::Number(n) if n.is_u64() => json!(f(n.as_u64().unwrap() as u32)),
            Value::Array(a) if k == "entities" => Value::Array(a.iter().map(|x| x.as_u64().map_or(x.clone(), |x| json!(f(x as u32)))).collect()),
            Value::Array(a) if k == "pairs" => Value::Array(
                a.iter()
                    .map(|p| Value::Array(p.as_array().into_iter().flatten().map(|x| x.as_u64().map_or(x.clone(), |x| json!(f(x as u32)))).collect()))
                    .collect(),
            ),
            other => other,
        };
        m.insert(k, nv);
    }
    serde_json::from_value(Value::Object(m)).unwrap_or_else(|_| c.clone())
}

fn mentions(c: &SketchConstraint, ids: &HashSet<u32>) -> bool {
    constraint_ids(c).iter().any(|i| ids.contains(i))
}

/// Si la cota tiene el candado puesto (no la cambia transformar).
fn dim_locked(c: &SketchConstraint) -> bool {
    serde_json::to_value(c).ok().and_then(|v| v["opts"]["locked"].as_bool()).unwrap_or(false)
}

/// Copia de la geometría con sus puntos pasados por `map`; `flip` da vuelta
/// el sentido de los arcos (una simetría lo invierte).
pub fn map_geometry(g: &Geometry, map: &mut dyn FnMut(u32) -> u32, flip: bool) -> Geometry {
    match g {
        Geometry::Line { start, end } => Geometry::Line { start: map(*start), end: map(*end) },
        Geometry::Circle { center, radius } => Geometry::Circle { center: map(*center), radius: *radius },
        Geometry::Arc { center, start, end } => {
            let (a, b) = if flip { (*end, *start) } else { (*start, *end) };
            Geometry::Arc { center: map(*center), start: map(a), end: map(b) }
        }
        Geometry::Spline { points, closed, start_handle, end_handle, handles } => Geometry::Spline {
            points: points.iter().map(|p| map(*p)).collect(),
            closed: *closed,
            start_handle: start_handle.map(&mut *map),
            end_handle: end_handle.map(&mut *map),
            handles: handles.iter().map(|h| [h[0], map(h[1])]).collect(),
        },
        Geometry::Point { point } => Geometry::Point { point: map(*point) },
        Geometry::Unknown => Geometry::Unknown,
        Geometry::Ellipse { center, major, minor } => Geometry::Ellipse { center: map(*center), major: map(*major), minor: map(*minor) },
        Geometry::EllipseArc { center, major, minor, start, end } => {
            let (a, b) = if flip { (*end, *start) } else { (*start, *end) };
            Geometry::EllipseArc { center: map(*center), major: map(*major), minor: map(*minor), start: map(a), end: map(b) }
        }
        Geometry::BSpline { poles, degree, closed, weights, knots } => Geometry::BSpline {
            poles: poles.iter().map(|p| map(*p)).collect(),
            degree: *degree,
            closed: *closed,
            weights: weights.clone(),
            knots: knots.clone(),
        },
    }
}

/// Pasa a la entidad `to` las marcas de `from` (construcción, capa, eje,
/// infinita): el tramo nuevo de un recorte o un corte es como el original.
fn inherit_marks(s: &mut Sketch, from: &SketchEntity, to: u32) {
    if from.id == to {
        return;
    }
    if let Some(e) = ent_mut(s, to) {
        e.construction |= from.construction;
        e.axis |= from.axis;
        e.infinite |= from.infinite;
        if from.layer.is_some() {
            e.layer = from.layer;
        }
    }
}

fn set_construction(s: &mut Sketch, id: u32) -> u32 {
    if let Some(e) = ent_mut(s, id) {
        e.construction = true;
    }
    id
}

/// Puntos de las entidades, sin repetir, en orden.
fn points_of_entities(s: &Sketch, ids: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    for id in ids {
        for p in ent(s, *id).map(|e| e.geometry.point_ids()).unwrap_or_default() {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Quita puntos que ninguna entidad usa (el origen y las anclas de textos se quedan).
fn drop_loose_points(s: &mut Sketch) {
    let mut used: HashSet<u32> = s.entities.iter().flat_map(|e| e.geometry.point_ids()).collect();
    used.extend(s.origin);
    used.extend(s.texts.iter().map(|t| t.anchor));
    let loose: HashSet<u32> = s.points.iter().filter(|q| !used.contains(&q.id)).map(|q| q.id).collect();
    if loose.is_empty() {
        return;
    }
    s.points.retain(|q| !loose.contains(&q.id));
    s.constraints.retain(|k| !mentions(k, &loose));
}

/// Borra una entidad como la interfaz: además de `Sketch::remove_entity`, los
/// centros de patrones circulares se quedan y un patrón sin nada de lo suyo se olvida.
fn remove_entity(s: &mut Sketch, id: u32) -> R<()> {
    let centers: Vec<u32> = s.patterns.iter().filter(|p| p["spec"]["kind"] == "circular").filter_map(|p| p["spec"]["center"].as_u64().map(|c| c as u32)).collect();
    let saved: Vec<(SketchPoint, Vec<SketchConstraint>)> = centers
        .iter()
        .filter_map(|c| {
            let p = s.points.iter().find(|p| p.id == *c)?.clone();
            let one: HashSet<u32> = [*c].into();
            Some((p, s.constraints.iter().filter(|k| mentions(k, &one)).cloned().collect()))
        })
        .collect();
    s.remove_entity(id).map_err(|e| e.to_string())?;
    for (p, ks) in saved {
        if !s.points.iter().any(|q| q.id == p.id) {
            s.points.push(p);
            for k in ks {
                if constraint_ids(&k).iter().all(|i| pos(s, *i).is_some() || ent(s, *i).is_some()) {
                    s.constraints.push(k);
                }
            }
        }
    }
    let alive: HashSet<u32> = s.entities.iter().map(|e| e.id).collect();
    let has = |p: &Value, k: &str| p[k].as_array().is_some_and(|a| a.iter().any(|x| x.as_u64().is_some_and(|x| alive.contains(&(x as u32)))));
    s.patterns.retain(|p| has(p, "source") || has(p, "owned"));
    Ok(())
}

// ─── Borrar ───────────────────────────────────────────────────────────────

/// Borra entidades o puntos: un punto borra las entidades que lo usan (y si
/// no lo usa nadie, el punto suelto). Devuelve las entidades borradas. Todo o
/// nada: si un id no existe o es el origen, no toca nada.
pub fn delete(s: &mut Sketch, ids: &[u32]) -> R<Vec<u32>> {
    let mut ents: Vec<u32> = Vec::new();
    let mut loose: Vec<u32> = Vec::new();
    for &id in ids {
        if ent(s, id).is_some() {
            ents.push(id);
        } else if pos(s, id).is_some() {
            if Some(id) == s.origin {
                return Err("el origen no se borra".into());
            }
            let users: Vec<u32> = s.entities.iter().filter(|e| e.geometry.point_ids().contains(&id)).map(|e| e.id).collect();
            if users.is_empty() {
                loose.push(id);
            }
            ents.extend(users);
        } else {
            return Err(format!("no existe el id {id}"));
        }
    }
    ents.sort_unstable();
    ents.dedup();
    for &e in &ents {
        remove_entity(s, e)?;
    }
    // Puntos sueltos elegidos: solo si nadie más los usa (ni texto ni patrón)
    for p in loose {
        let anchored = s.texts.iter().any(|t| t.anchor == p || t.points.contains(&p))
            || s.patterns.iter().any(|q| q["spec"]["kind"] == "circular" && q["spec"]["center"].as_u64() == Some(p as u64));
        if !anchored && !s.entities.iter().any(|e| e.geometry.point_ids().contains(&p)) {
            let one: HashSet<u32> = [p].into();
            s.points.retain(|q| q.id != p);
            s.constraints.retain(|k| !mentions(k, &one));
        }
    }
    Ok(ents)
}

// ─── Transformar ──────────────────────────────────────────────────────────

/// Transformación afín del plano del sketch: `p' = m · p + t` (con `m` por filas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xform {
    pub m: [f64; 4],
    pub t: P2,
}

impl Xform {
    pub fn identity() -> Self {
        Self { m: [1.0, 0.0, 0.0, 1.0], t: [0.0, 0.0] }
    }

    /// Mover `d`.
    pub fn translation(d: P2) -> Self {
        Self { m: [1.0, 0.0, 0.0, 1.0], t: d }
    }

    /// Giro de `degrees` (antihorario) alrededor de `c`.
    pub fn rotation(c: P2, degrees: f64) -> Self {
        let a = degrees.to_radians();
        let (sin, cos) = a.sin_cos();
        let m = [cos, -sin, sin, cos];
        Self { m, t: [c[0] - (m[0] * c[0] + m[1] * c[1]), c[1] - (m[2] * c[0] + m[3] * c[1])] }
    }

    /// Escala `k` desde `c`.
    pub fn scaling(c: P2, k: f64) -> Self {
        Self { m: [k, 0.0, 0.0, k], t: [c[0] * (1.0 - k), c[1] * (1.0 - k)] }
    }

    pub fn apply(&self, p: P2) -> P2 {
        [self.m[0] * p[0] + self.m[1] * p[1] + self.t[0], self.m[2] * p[0] + self.m[3] * p[1] + self.t[1]]
    }

    /// Factor de escala (la raíz del determinante).
    pub fn scale(&self) -> f64 {
        (self.m[0] * self.m[3] - self.m[1] * self.m[2]).abs().sqrt()
    }

    /// Parte lineal sin la escala (giro o reflejo puro).
    fn linear_part(&self) -> [f64; 4] {
        let k = self.scale();
        let k = if k == 0.0 { 1.0 } else { k };
        self.m.map(|v| v / k)
    }
}

/// `m · n`
fn compose_frame(m: [f64; 4], n: [f64; 4]) -> [f64; 4] {
    [m[0] * n[0] + m[1] * n[2], m[0] * n[1] + m[1] * n[3], m[2] * n[0] + m[3] * n[2], m[2] * n[1] + m[3] * n[3]]
}

/// Le suma al marco del texto un giro o reflejo (sin guardar la identidad).
fn turn_text(t: &mut SketchText, m: [f64; 4]) {
    let f = compose_frame(m, t.style.frame.unwrap_or([1.0, 0.0, 0.0, 1.0]));
    let identity = (f[0] - 1.0).abs() < 1e-9 && f[1].abs() < 1e-9 && f[2].abs() < 1e-9 && (f[3] - 1.0).abs() < 1e-9;
    t.style.frame = if identity { None } else { Some(f.map(|v| round(v, 12))) };
}

/// Índices de los textos que toca la selección (por una de sus curvas o su ancla).
fn texts_of(s: &Sketch, ids: &HashSet<u32>) -> Vec<usize> {
    (0..s.texts.len()).filter(|&i| std::iter::once(&s.texts[i].anchor).chain(&s.texts[i].entities).any(|x| ids.contains(x))).collect()
}

/// Entidades elegidas: las nombradas, las de un punto suelto elegido por su
/// punto y las de los textos tocados (un texto va entero).
fn selected_entities(s: &Sketch, ids: &[u32]) -> Vec<u32> {
    let mut set: HashSet<u32> = ids.iter().copied().collect();
    for i in texts_of(s, &set.clone()) {
        set.extend(s.texts[i].entities.iter().copied());
    }
    s.entities.iter().filter(|e| set.contains(&e.id) || matches!(e.geometry, Geometry::Point { point } if set.contains(&point))).map(|e| e.id).collect()
}

/// Entidades que no se mueven: ligadas al sólido o bloqueadas (también por capa).
fn held_entities(s: &Sketch) -> HashSet<u32> {
    let mut out: HashSet<u32> = s.uses.iter().map(|u| u.entity).collect();
    for c in &s.constraints {
        if let SketchConstraint::Lock { entity } = c {
            out.insert(*entity);
        }
    }
    let locked: HashSet<u32> = s.layers.iter().filter(|l| l.locked).map(|l| l.id).collect();
    for e in &s.entities {
        if e.layer.is_some_and(|l| locked.contains(&l)) {
            out.insert(e.id);
        }
    }
    out
}

/// Puntos que mueve la selección (sin el origen, lo ligado al sólido ni lo bloqueado).
fn selection_points(s: &Sketch, ids: &[u32]) -> Vec<u32> {
    let used = held_entities(s);
    let mut out: HashSet<u32> = HashSet::new();
    for id in selected_entities(s, ids) {
        if !used.contains(&id) {
            out.extend(ent(s, id).unwrap().geometry.point_ids());
        }
    }
    let set: HashSet<u32> = ids.iter().copied().collect();
    for i in texts_of(s, &set) {
        out.insert(s.texts[i].anchor);
        out.extend(s.texts[i].points.iter().copied());
    }
    // Puntos elegidos sueltos (el extremo de una línea, por ejemplo)
    out.extend(ids.iter().copied().filter(|id| pos(s, *id).is_some()));
    if let Some(o) = s.origin {
        out.remove(&o);
    }
    // Lo ligado al sólido y lo bloqueado se queda donde está (también un extremo compartido)
    for e in &s.entities {
        if used.contains(&e.id) {
            for p in e.geometry.point_ids() {
                out.remove(&p);
            }
        }
    }
    s.points.iter().map(|p| p.id).filter(|id| out.contains(id)).collect()
}

fn line_of(s: &Sketch, id: u32) -> Option<(P2, P2)> {
    match ent(s, id)?.geometry {
        Geometry::Line { start, end } => Some((pos(s, start)?, pos(s, end)?)),
        _ => None,
    }
}

fn center_of(s: &Sketch, id: u32) -> Option<P2> {
    match ent(s, id)?.geometry {
        Geometry::Circle { center, .. } | Geometry::Arc { center, .. } | Geometry::Ellipse { center, .. } => pos(s, center),
        _ => None,
    }
}

/// Distancia de `p` a la recta de la línea.
fn to_line(p: P2, (a, b): (P2, P2)) -> f64 {
    let d = sub(b, a);
    let l = len(d);
    if l > 0.0 { cross(d, sub(p, a)).abs() / l } else { len(sub(p, a)) }
}

/// Si un punto está sobre una curva (las splines se dan por cumplidas).
fn on_curve(s: &Sketch, point: u32, curve: u32, tol: f64) -> bool {
    let (Some(p), Some(e)) = (pos(s, point), ent(s, curve)) else { return true };
    match &e.geometry {
        Geometry::Line { .. } => line_of(s, curve).is_none_or(|l| to_line(p, l) <= tol),
        Geometry::Circle { .. } | Geometry::Arc { .. } => match (center_of(s, curve), s.radius(curve).ok()) {
            (Some(k), Some(r)) => (len(sub(p, k)) - r).abs() <= tol,
            _ => true,
        },
        Geometry::Ellipse { center, major, minor } => {
            let (Some(k), Some(a), Some(b)) = (pos(s, *center), pos(s, *major), pos(s, *minor)) else { return true };
            let u = sub(a, k);
            let (ra, rb) = (len(u), len(sub(b, k)));
            if ra == 0.0 || rb == 0.0 {
                return true;
            }
            let q = sub(p, k);
            let (x, y) = (dot(q, u) / ra, cross(u, q) / ra);
            ((x / ra).hypot(y / rb) - 1.0).abs() * (ra * rb).sqrt() <= tol
        }
        _ => true,
    }
}

/// Si una restricción geométrica se cumple en la geometría actual (`tol` en mm
/// para distancias, en seno del ángulo para direcciones). Las que no se saben
/// evaluar se dan por cumplidas.
fn holds(s: &Sketch, c: &SketchConstraint, tol: f64) -> bool {
    use SketchConstraint as C;
    let ang_tol = 1e-6;
    let p = |id: u32| pos(s, id);
    match c {
        C::Coincident { a, b } => match (p(*a), p(*b)) {
            (Some(a), Some(b)) => len(sub(a, b)) <= tol,
            _ => true,
        },
        C::Horizontal { line } | C::Vertical { line } => line_of(s, *line).is_none_or(|l| {
            let d = sub(l.1, l.0);
            let v = if matches!(c, C::Horizontal { .. }) { d[1] } else { d[0] };
            v.abs() <= tol.max(ang_tol * len(d))
        }),
        C::HorizontalPoints { a, b } | C::VerticalPoints { a, b } => match (p(*a), p(*b)) {
            (Some(a), Some(b)) => (if matches!(c, C::HorizontalPoints { .. }) { a[1] - b[1] } else { a[0] - b[0] }).abs() <= tol,
            _ => true,
        },
        C::Parallel { a, b } | C::Perpendicular { a, b } => match (line_of(s, *a), line_of(s, *b)) {
            (Some(l1), Some(l2)) => {
                let (d1, d2) = (sub(l1.1, l1.0), sub(l2.1, l2.0));
                let n = len(d1) * len(d2);
                n == 0.0 || (if matches!(c, C::Parallel { .. }) { cross(d1, d2) } else { dot(d1, d2) }).abs() / n <= ang_tol
            }
            _ => true,
        },
        C::Collinear { a, b } => match (line_of(s, *a), line_of(s, *b)) {
            (Some(l1), Some(l2)) => holds(s, &C::Parallel { a: *a, b: *b }, tol) && to_line(l2.0, l1) <= tol,
            _ => true,
        },
        C::Equal { a, b } => {
            if let (Some(l1), Some(l2)) = (line_of(s, *a), line_of(s, *b)) {
                return (dist(l1.0, l1.1) - dist(l2.0, l2.1)).abs() <= tol;
            }
            match (s.radius(*a), s.radius(*b)) {
                (Ok(r1), Ok(r2)) => (r1 - r2).abs() <= tol,
                _ => true,
            }
        }
        C::Tangent { a, b } => {
            let (la, lb) = (line_of(s, *a), line_of(s, *b));
            let (ca, cb) = (center_of(s, *a), center_of(s, *b));
            let (ra, rb) = (s.radius(*a).ok(), s.radius(*b).ok());
            if let (Some(la), Some(cb), Some(rb)) = (la, cb, rb) {
                return (to_line(cb, la) - rb).abs() <= tol;
            }
            if let (Some(lb), Some(ca), Some(ra)) = (lb, ca, ra) {
                return (to_line(ca, lb) - ra).abs() <= tol;
            }
            if let (Some(ca), Some(cb), Some(ra), Some(rb)) = (ca, cb, ra, rb) {
                let d = len(sub(ca, cb));
                return (d - (ra + rb)).abs() <= tol || (d - (ra - rb).abs()).abs() <= tol;
            }
            true
        }
        C::Concentric { a, b } => match (center_of(s, *a), center_of(s, *b)) {
            (Some(a), Some(b)) => len(sub(a, b)) <= tol,
            _ => true,
        },
        C::PointOnLine { point, line } => match (p(*point), line_of(s, *line)) {
            (Some(p), Some(l)) => to_line(p, l) <= tol,
            _ => true,
        },
        C::PointOnCircle { point, circle } => match (p(*point), center_of(s, *circle), s.radius(*circle).ok()) {
            (Some(p), Some(k), Some(r)) => (len(sub(p, k)) - r).abs() <= tol,
            _ => true,
        },
        C::PointOnCurve { point, curve } => on_curve(s, *point, *curve, tol),
        C::Intersection { point, a, b } => on_curve(s, *point, *a, tol) && on_curve(s, *point, *b, tol),
        C::Coradial { a, b } => match (center_of(s, *a), center_of(s, *b), s.radius(*a).ok(), s.radius(*b).ok()) {
            (Some(a), Some(b), Some(ra), Some(rb)) => len(sub(a, b)) <= tol && (ra - rb).abs() <= tol,
            _ => true,
        },
        C::Midpoint { point, line } => match (p(*point), line_of(s, *line)) {
            (Some(p), Some(l)) => len(sub(p, mul(add(l.0, l.1), 0.5))) <= tol,
            _ => true,
        },
        C::Symmetric { a, b, line } => match (p(*a), p(*b), line_of(s, *line)) {
            (Some(a), Some(b), Some(l)) => {
                let d = sub(l.1, l.0);
                let t = dot(sub(a, l.0), d) / dot(d, d);
                let foot = add(l.0, mul(d, t));
                len(sub(sub(mul(foot, 2.0), a), b)) <= tol
            }
            _ => true,
        },
        C::EqualOffset { a1, a2, b1, b2 } => match (p(*a1), p(*a2), p(*b1), p(*b2)) {
            (Some(a1), Some(a2), Some(b1), Some(b2)) => len(sub(sub(a2, a1), sub(b2, b1))) <= tol,
            _ => true,
        },
        C::EqualRotation { center, a1, a2, b1, b2 } => match (p(*center), p(*a1), p(*a2), p(*b1), p(*b2)) {
            (Some(o), Some(a1), Some(a2), Some(b1), Some(b2)) => {
                let ang = |u: P2, v: P2| cross(sub(u, o), sub(v, o)).atan2(dot(sub(u, o), sub(v, o)));
                let da = ang(a1, a2) - ang(b1, b2);
                da.sin().atan2(da.cos()).abs() <= 1e-6 && (len(sub(a1, o)) - len(sub(a2, o))).abs() <= tol && (len(sub(b1, o)) - len(sub(b2, o))).abs() <= tol
            }
            _ => true,
        },
        _ => true,
    }
}

/// Después de mover o escalar: los fijos pasan a su lugar nuevo, las cotas
/// toman lo que miden ahora (salvo las bloqueadas) y las restricciones que
/// dejaron de cumplirse se quitan (una horizontal girada 90° pasa a vertical).
/// Solo mira las desde `start`; con `moved`, solo las que tocan lo movido o
/// escalado (`scaled`); sin él, todas.
fn retune(s: &mut Sketch, start: usize, moved: Option<&HashSet<u32>>, scaled: &HashSet<u32>) {
    let tol = 1e-6 * span(s);
    let all = std::mem::take(&mut s.constraints);
    let mut keep: Vec<SketchConstraint> = Vec::with_capacity(all.len());
    for (i, mut c) in all.into_iter().enumerate() {
        let touched = i >= start
            && match moved {
                None => true,
                Some(moved) => {
                    let ids = constraint_ids(&c);
                    ids.iter().flat_map(|id| ent(s, *id).map(|e| e.geometry.point_ids()).unwrap_or_else(|| vec![*id])).any(|p| moved.contains(&p))
                        || ids.iter().any(|id| scaled.contains(id))
                }
            };
        if !touched {
            keep.push(c);
            continue;
        }
        if let SketchConstraint::Fixed { point, x, y } = &mut c {
            if let Some(p) = pos(s, *point) {
                (*x, *y) = (p[0], p[1]);
            }
            keep.push(c);
            continue;
        }
        if let Some(v) = s.measure(&c) {
            if !dim_locked(&c) {
                c.set_value(round(v, 9));
            }
            keep.push(c);
            continue;
        }
        if holds(s, &c, tol) {
            keep.push(c);
            continue;
        }
        let swapped = match &c {
            SketchConstraint::Horizontal { line } => Some(SketchConstraint::Vertical { line: *line }),
            SketchConstraint::Vertical { line } => Some(SketchConstraint::Horizontal { line: *line }),
            SketchConstraint::HorizontalPoints { a, b } => Some(SketchConstraint::VerticalPoints { a: *a, b: *b }),
            SketchConstraint::VerticalPoints { a, b } => Some(SketchConstraint::HorizontalPoints { a: *a, b: *b }),
            _ => None,
        };
        if let Some(w) = swapped.filter(|w| holds(s, w, tol)) {
            keep.push(w);
        }
    }
    s.constraints = keep;
}

/// Lleva lo elegido por `x`. Los puntos compartidos con lo no elegido se
/// mueven igual (lo de al lado se estira). Los círculos elegidos escalan su
/// radio (salvo los ligados o bloqueados) y los textos su tamaño.
fn transform_selection(s: &mut Sketch, ids: &[u32], x: &Xform) -> R<Vec<u32>> {
    let pts = selection_points(s, ids);
    if pts.is_empty() {
        return Err("elegir primero lo que se transforma (lo ligado al sólido y lo bloqueado no se mueven)".into());
    }
    let moved: HashSet<u32> = pts.iter().copied().collect();
    for p in &mut s.points {
        if moved.contains(&p.id) {
            [p.x, p.y] = x.apply([p.x, p.y]);
        }
    }
    let selected = selected_entities(s, ids);
    let idset: HashSet<u32> = ids.iter().copied().collect();
    let text_idx = texts_of(s, &idset);
    let k = x.scale();
    let scaling = (k - 1.0).abs() > 1e-12;
    if scaling {
        let held = held_entities(s);
        for id in &selected {
            if held.contains(id) {
                continue;
            }
            if let Some(SketchEntity { geometry: Geometry::Circle { radius, .. }, .. }) = ent_mut(s, *id) {
                *radius *= k;
            }
        }
        for &i in &text_idx {
            s.texts[i].size *= k;
        }
    }
    // Los textos recuerdan el giro o reflejo para rehacerse igual
    let lin = x.linear_part();
    if (lin[0] - 1.0).abs() > 1e-12 || lin[1].abs() > 1e-12 || lin[2].abs() > 1e-12 || (lin[3] - 1.0).abs() > 1e-12 {
        for &i in &text_idx {
            turn_text(&mut s.texts[i], lin);
        }
    }
    let scaled: HashSet<u32> = if scaling { selected.iter().copied().collect() } else { HashSet::new() };
    retune(s, 0, Some(&moved), &scaled);
    Ok(selected)
}

/// Parte de un sketch copiada: entidades, sus puntos y las restricciones entre ellas.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub points: Vec<SketchPoint>,
    pub entities: Vec<SketchEntity>,
    pub constraints: Vec<SketchConstraint>,
    pub texts: Vec<SketchText>,
}

/// Copia lo elegido: entidades (un texto va entero), sus puntos y las
/// restricciones que solo nombran cosas de adentro. Los fijos y bloqueos no se copian.
pub fn extract_clip(s: &Sketch, ids: &[u32]) -> Option<Clip> {
    let ents = selected_entities(s, ids);
    if ents.is_empty() {
        return None;
    }
    let mut pts: HashSet<u32> = HashSet::new();
    for id in &ents {
        pts.extend(ent(s, *id)?.geometry.point_ids());
    }
    let set: HashSet<u32> = ids.iter().copied().collect();
    let texts: Vec<SketchText> = texts_of(s, &set).into_iter().map(|i| s.texts[i].clone()).collect();
    for t in &texts {
        pts.insert(t.anchor);
        pts.extend(t.points.iter().copied());
    }
    let mut inside = pts.clone();
    inside.extend(ents.iter().copied());
    Some(Clip {
        points: s.points.iter().filter(|p| pts.contains(&p.id)).cloned().collect(),
        entities: ents.iter().filter_map(|id| ent(s, *id).cloned()).collect(),
        constraints: s
            .constraints
            .iter()
            .filter(|c| !matches!(c, SketchConstraint::Fixed { .. } | SketchConstraint::Lock { .. }) && constraint_ids(c).iter().all(|i| inside.contains(i)))
            .cloned()
            .collect(),
        texts,
    })
}

/// Pega un portapapeles con sus puntos llevados por `x`, con ids nuevos.
/// Devuelve las entidades nuevas (en el orden del portapapeles).
pub fn insert_clip(s: &mut Sketch, clip: &Clip, x: &Xform) -> Vec<u32> {
    let mut map: HashMap<u32, u32> = HashMap::new();
    for p in &clip.points {
        map.insert(p.id, { let q = x.apply([p.x, p.y]); s.add_point(q[0], q[1]) });
    }
    let mut out = Vec::new();
    for e in &clip.entities {
        let g = map_geometry(&e.geometry, &mut |p| *map.get(&p).unwrap_or(&p), false);
        let n = s.add_entity(g);
        inherit_marks(s, e, n);
        map.insert(e.id, n);
        out.push(n);
    }
    for c in &clip.constraints {
        let m = map_constraint(c, &mut |id| *map.get(&id).unwrap_or(&id));
        s.constraints.push(m);
    }
    for t in &clip.texts {
        let id = s.texts.iter().map(|x| x.id).max().unwrap_or(0) + 1;
        let f = |x: u32| *map.get(&x).unwrap_or(&x);
        let mut n = t.clone();
        n.id = id;
        n.anchor = f(t.anchor);
        n.entities = t.entities.iter().map(|x| f(*x)).collect();
        n.points = t.points.iter().map(|x| f(*x)).collect();
        s.texts.push(n);
    }
    out
}

/// Mueve, gira o escala lo elegido (`ids`: entidades o puntos). Con `copy`
/// deja el original y hace una copia transformada (con ids nuevos, sus
/// restricciones internas y las cotas re-medidas); devuelve las entidades
/// nuevas. Sin `copy`, mueve y devuelve las entidades afectadas. Lo ligado al
/// sólido, lo bloqueado y el origen no se mueven.
pub fn transform(s: &mut Sketch, ids: &[u32], xform: &Xform, copy: bool) -> R<Vec<u32>> {
    if !copy {
        return transform_selection(s, ids, xform);
    }
    let Some(clip) = extract_clip(s, ids) else { return Err("elegir primero lo que se copia".into()) };
    let first_constraint = s.constraints.len();
    let first_text = s.texts.len();
    let out = insert_clip(s, &clip, xform);
    let k = xform.scale();
    let scaling = (k - 1.0).abs() > 1e-12;
    let mut scaled = HashSet::new();
    if scaling {
        for &id in &out {
            if let Some(SketchEntity { geometry: Geometry::Circle { radius, .. }, .. }) = ent_mut(s, id) {
                *radius *= k;
            }
            scaled.insert(id);
        }
    }
    let lin = xform.linear_part();
    let turned = (lin[0] - 1.0).abs() > 1e-12 || lin[1].abs() > 1e-12 || lin[2].abs() > 1e-12 || (lin[3] - 1.0).abs() > 1e-12;
    for t in &mut s.texts[first_text..] {
        if scaling {
            t.size *= k;
        }
        if turned {
            turn_text(t, lin);
        }
    }
    retune(s, first_constraint, None, &scaled);
    Ok(out)
}

// ─── Simetría y patrones ──────────────────────────────────────────────────

/// Copia de las entidades con sus puntos pasados por `map` (los que no están
/// en `map` se comparten). Los círculos copiados quedan con el mismo radio
/// (restricción `equal`); `flip` invierte los arcos. Anota la copia en `log`.
fn copy_entities(s: &mut Sketch, ids: &[u32], map: &HashMap<u32, u32>, flip: bool, log: &mut Vec<Vec<u32>>) -> Vec<u32> {
    let mut out = Vec::new();
    for &id in ids {
        let Some(e) = ent(s, id).cloned() else { continue };
        let c = s.add_entity(map_geometry(&e.geometry, &mut |p| *map.get(&p).unwrap_or(&p), flip));
        if e.construction {
            set_construction(s, c);
        }
        if matches!(e.geometry, Geometry::Circle { .. }) {
            s.constraints.push(SketchConstraint::Equal { a: id, b: c });
        }
        out.push(c);
    }
    log.push(out.clone());
    out
}

/// Simetría: copia las entidades reflejadas respecto de la línea `axis_line`.
/// Cada punto copiado queda simétrico de su original (restricción `symmetric`);
/// los que están sobre el eje se comparten. Devuelve las entidades nuevas.
pub fn mirror(s: &mut Sketch, ids: &[u32], axis_line: u32) -> R<Vec<u32>> {
    let Some(SketchEntity { geometry: Geometry::Line { start, end }, .. }) = ent(s, axis_line).cloned() else {
        return Err("el eje de la simetría tiene que ser una línea".into());
    };
    let (a, b) = (pt(s, start)?, pt(s, end)?);
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 == 0.0 {
        return Err("el eje no tiene largo".into());
    }
    let items: Vec<u32> = ids.iter().copied().filter(|id| *id != axis_line && ent(s, *id).is_some()).collect();
    if items.is_empty() {
        return Err("elegir también lo que se refleja".into());
    }
    let tol = 1e-9 * l2.sqrt().max(1.0);
    let mut made: HashMap<u32, u32> = HashMap::new();
    for p in points_of_entities(s, &items) {
        let q0 = pt(s, p)?;
        let t = dot(sub(q0, a), d) / l2;
        let foot = add(a, mul(d, t));
        if dist(q0, foot) <= tol {
            made.insert(p, p);
            continue;
        }
        // Ya reflejado antes (la simetría dinámica refleja de a un tramo): el mismo punto
        let twin = s.constraints.iter().find_map(|k| match k {
            SketchConstraint::Symmetric { a, b, line } if *line == axis_line && (*a == p || *b == p) => Some(if *a == p { *b } else { *a }),
            _ => None,
        });
        let q = match twin {
            Some(q) => q,
            None => {
                let q = s.add_point(2.0 * foot[0] - q0[0], 2.0 * foot[1] - q0[1]);
                s.constraints.push(SketchConstraint::Symmetric { a: p, b: q, line: axis_line });
                q
            }
        };
        made.insert(p, q);
    }
    Ok(copy_entities(s, &items, &made, true, &mut Vec::new()))
}

/// Una copia de las entidades con sus puntos llevados por `place` (recibe la
/// posición original); devuelve original → copia. Los de `keep` se comparten.
fn place_copy(s: &mut Sketch, ids: &[u32], pts: &[u32], place: &dyn Fn(P2) -> P2, keep: &[u32], log: &mut Vec<Vec<u32>>) -> R<HashMap<u32, u32>> {
    let mut map = HashMap::new();
    for &p in pts {
        if keep.contains(&p) {
            map.insert(p, p);
        } else {
            let q = place(pt(s, p)?);
            map.insert(p, s.add_point(q[0], q[1]));
        }
    }
    copy_entities(s, ids, &map, false, log);
    Ok(map)
}

/// Los demás puntos de la copia guardan la forma del original: mismo desplazamiento que `lead`.
fn keep_shape(s: &mut Sketch, pts: &[u32], lead: u32, map: &HashMap<u32, u32>) {
    for &p in pts {
        if p != lead {
            s.constraints.push(SketchConstraint::EqualOffset { a1: lead, a2: p, b1: map[&lead], b2: map[&p] });
        }
    }
}

/// Resultado de armar un patrón.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternResult {
    /// Id del registro en `Sketch::patterns`.
    pub pattern: u32,
    /// Entidades de cada copia (en el orden del original).
    pub copies: Vec<Vec<u32>>,
    /// Todo lo que armó el patrón (copias y guías).
    pub owned: Vec<u32>,
}

/// Arma un patrón y lo anota en `Sketch::patterns` (como `makePattern` de la interfaz).
fn make_pattern(s: &mut Sketch, source: &[u32], spec: Value, run: impl FnOnce(&mut Sketch, &mut Vec<Vec<u32>>) -> R<()>) -> R<PatternResult> {
    let before: HashSet<u32> = s.entities.iter().map(|e| e.id).collect();
    let mut copies = Vec::new();
    run(s, &mut copies)?;
    let owned: Vec<u32> = s.entities.iter().filter(|e| !before.contains(&e.id)).map(|e| e.id).collect();
    let id = s.patterns.iter().filter_map(|p| p["id"].as_u64()).max().unwrap_or(0) as u32 + 1;
    s.patterns.push(json!({ "id": id, "spec": spec, "source": source, "owned": owned, "copies": copies, "skip": Vec::<u32>::new() }));
    Ok(PatternResult { pattern: id, copies, owned })
}

/// Patrón lineal: `count` en total por fila (el original y count − 1 copias)
/// corridas de a `offset`; con `rows` > 1, filas corridas de a `offset2`. Solo
/// el primer par de cada dirección lleva cotas (distancia horizontal y
/// vertical); el resto sigue con "mismo desplazamiento". Anota el patrón.
pub fn linear_pattern(s: &mut Sketch, ids: &[u32], count: u32, offset: P2, rows: u32, offset2: P2) -> R<PatternResult> {
    let source: Vec<u32> = ids.iter().copied().filter(|id| ent(s, *id).is_some()).collect();
    let pts = points_of_entities(s, &source);
    if pts.is_empty() || count < 1 || rows < 1 || count * rows < 2 {
        return Err("elegir qué repetir y al menos 2 en total".into());
    }
    let spec = json!({ "kind": "linear", "count": count, "offset": offset, "rows": rows, "offset2": offset2 });
    make_pattern(s, &source.clone(), spec, |s, log| {
        let mut grid: HashMap<(u32, u32), HashMap<u32, u32>> = HashMap::new();
        grid.insert((0, 0), pts.iter().map(|p| (*p, *p)).collect());
        let mut masters: [Option<(u32, u32)>; 2] = [None, None];
        for j in 0..rows {
            for k in 0..count {
                if j == 0 && k == 0 {
                    continue;
                }
                let d = [k as f64 * offset[0] + j as f64 * offset2[0], k as f64 * offset[1] + j as f64 * offset2[1]];
                let map = place_copy(s, &source, &pts, &|p| add(p, d), &[], log)?;
                // Cada copia sigue a la anterior de su fila (o a la de la fila anterior si es la primera)
                let dir = if k > 0 { 0 } else { 1 };
                let parent = if k > 0 { &grid[&(k - 1, j)] } else { &grid[&(k, j - 1)] }.clone();
                for &p in &pts {
                    let (from, q) = (parent[&p], map[&p]);
                    if let Some(m) = masters[dir] {
                        s.constraints.push(SketchConstraint::EqualOffset { a1: m.0, a2: m.1, b1: from, b2: q });
                    } else {
                        let off = if dir == 1 { offset2 } else { offset };
                        masters[dir] = Some((from, q));
                        s.constraints.push(SketchConstraint::HorizontalDistance { a: from, b: q, value: off[0], reference: false, opts: DimOpts::default() });
                        s.constraints.push(SketchConstraint::VerticalDistance { a: from, b: q, value: off[1], reference: false, opts: DimOpts::default() });
                    }
                }
                grid.insert((k, j), map);
            }
        }
        Ok(())
    })
}

/// Patrón circular alrededor del punto `center_point`: `count` en total
/// repartidos en `span_deg` grados (360: la vuelta entera; menos: de punta a
/// punta; negativo: horario). El primer par lleva radios iguales y una cota
/// de ángulo (entre dos líneas de construcción); el resto sigue con "mismo
/// giro". Sin `rotate` las copias se trasladan sin girar. Anota el patrón.
pub fn circular_pattern(s: &mut Sketch, ids: &[u32], count: u32, center_point: u32, span_deg: f64, rotate: bool) -> R<PatternResult> {
    let source: Vec<u32> = ids.iter().copied().filter(|id| ent(s, *id).is_some()).collect();
    let pts = points_of_entities(s, &source);
    let Some(lead) = pts.iter().copied().find(|p| *p != center_point) else { return Err("elegir qué repetir y al menos 2 en total".into()) };
    if count < 2 {
        return Err("elegir qué repetir y al menos 2 en total".into());
    }
    let c = pos(s, center_point).ok_or("el centro del patrón no existe")?;
    if span_deg == 0.0 {
        return Err("el ángulo no puede ser 0".into());
    }
    let full = span_deg.abs() >= 360.0 - 1e-9;
    let step = (if full { span_deg.signum() * 360.0 } else { span_deg }) / (if full { count as f64 } else { (count - 1) as f64 }) * (std::f64::consts::PI / 180.0);
    let half = (step.abs() - std::f64::consts::PI).abs() < 1e-9;
    if !half && step.abs() > std::f64::consts::PI {
        return Err("entre una copia y la siguiente tiene que haber menos de 180°".into());
    }
    let rot = move |p: P2, a: f64| {
        let (x, y) = (p[0] - c[0], p[1] - c[1]);
        [c[0] + x * a.cos() - y * a.sin(), c[1] + x * a.sin() + y * a.cos()]
    };
    let l = pt(s, lead)?;
    let spec = json!({ "kind": "circular", "count": count, "center": center_point, "span": span_deg, "rotate": rotate });
    make_pattern(s, &source.clone(), spec, |s, log| {
        let mut prev: HashMap<u32, u32> = pts.iter().map(|p| (*p, *p)).collect();
        let mut master: Option<(u32, u32)> = None;
        for k in 1..count {
            let a = k as f64 * step;
            // Girando, cada punto da la vuelta; sin girar, todos siguen al principal
            let shift = sub(rot(l, a), l);
            let place = move |p: P2| if rotate { rot(p, a) } else { add(p, shift) };
            let keep: &[u32] = if rotate { &[center_point] } else { &[] };
            let next = place_copy(s, &source, &pts, &place, keep, log)?;
            if master.is_none() {
                let m = (lead, next[&lead]);
                master = Some(m);
                if half {
                    // Media vuelta: el centro es el punto medio (un ángulo de 180° es inestable en el solver)
                    let d = s.add_line(m.0, m.1);
                    set_construction(s, d);
                    s.constraints.push(SketchConstraint::Midpoint { point: center_point, line: d });
                } else {
                    let r0 = s.add_line(center_point, m.0);
                    set_construction(s, r0);
                    let r1 = s.add_line(center_point, m.1);
                    set_construction(s, r1);
                    s.constraints.push(SketchConstraint::Equal { a: r0, b: r1 });
                    s.constraints.push(SketchConstraint::Angle {
                        a: r0,
                        b: r1,
                        degrees: round(step.to_degrees(), 6),
                        supplementary: false,
                        a_end: None,
                        b_end: None,
                        reference: false,
                        opts: DimOpts::default(),
                    });
                }
            }
            let m = master.unwrap();
            let which: Vec<u32> = if rotate { pts.clone() } else { vec![lead] };
            for p in which {
                if p == center_point || (k == 1 && p == lead) {
                    continue;
                }
                s.constraints.push(SketchConstraint::EqualRotation { center: center_point, a1: m.0, a2: m.1, b1: prev[&p], b2: next[&p] });
            }
            if !rotate {
                keep_shape(s, &pts, lead, &next);
            }
            prev = next;
        }
        Ok(())
    })
}

// ─── Equidistante de cadena ───────────────────────────────────────────────

/// Cómo cerrar los extremos de una cadena abierta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetCaps {
    /// Sin cerrar.
    Open,
    /// Con arcos alrededor de las puntas (a los dos lados) o líneas (a uno).
    Round,
    /// Con líneas.
    Line,
}

/// Tramo de una cadena recorrido en orden.
#[derive(Debug, Clone, Copy)]
struct Piece {
    entity: u32,
    reversed: bool,
}

/// Línea o arco de una cadena: `(start, end)` de su geometría.
fn chain_ends(s: &Sketch, id: u32) -> Option<(u32, u32)> {
    match ent(s, id)?.geometry {
        Geometry::Line { start, end } | Geometry::Arc { start, end, .. } => Some((start, end)),
        _ => None,
    }
}

/// Ordena las entidades (líneas y arcos) en una cadena de extremo a extremo.
/// Devuelve los tramos en orden y si cierra.
fn order_chain(s: &Sketch, ids: &[u32]) -> R<(Vec<Piece>, bool)> {
    let mut ents: Vec<u32> = Vec::new();
    for &id in ids {
        if ent(s, id).is_some() && !ents.contains(&id) {
            ents.push(id);
        }
    }
    if ents.is_empty() {
        return Err("elegir líneas o arcos".into());
    }
    if ents.iter().any(|id| chain_ends(s, *id).is_none()) {
        return Err("la cadena es de líneas y arcos".into());
    }
    let tol = span(s) * 1e-9;
    let ends = |id: u32, rev: bool| -> R<(P2, P2)> {
        let (a, b) = chain_ends(s, id).unwrap();
        let (a, b) = (pt(s, a)?, pt(s, b)?);
        Ok(if rev { (b, a) } else { (a, b) })
    };
    let mut left: Vec<u32> = ents[1..].to_vec();
    let mut pieces = vec![Piece { entity: ents[0], reversed: false }];
    // Hacia adelante y hacia atrás desde la primera
    for forward in [true, false] {
        loop {
            let tip = if forward {
                let l = pieces[pieces.len() - 1];
                ends(l.entity, l.reversed)?.1
            } else {
                ends(pieces[0].entity, pieces[0].reversed)?.0
            };
            let mut found: Option<Piece> = None;
            for &id in &left {
                let (a, b) = ends(id, false)?;
                if dist(if forward { a } else { b }, tip) <= tol {
                    found = Some(Piece { entity: id, reversed: false });
                } else if dist(if forward { b } else { a }, tip) <= tol {
                    found = Some(Piece { entity: id, reversed: true });
                }
                if found.is_some() {
                    break;
                }
            }
            let Some(f) = found else { break };
            left.retain(|x| *x != f.entity);
            if forward {
                pieces.push(f);
            } else {
                pieces.insert(0, f);
            }
        }
    }
    if !left.is_empty() {
        return Err("lo elegido no forma una sola cadena".into());
    }
    let head = ends(pieces[0].entity, pieces[0].reversed)?.0;
    let l = pieces[pieces.len() - 1];
    let tail = ends(l.entity, l.reversed)?.1;
    Ok((pieces.clone(), pieces.len() > 1 && dist(head, tail) <= tol))
}

#[derive(Debug, Clone, Copy)]
struct ArcInfo {
    c: P2,
    center: u32,
    r: f64,
    ccw: bool,
}

/// Tramo desplazado: recta o arco (con su sentido de recorrido).
#[derive(Debug, Clone, Copy)]
struct OffPiece {
    a: P2,
    b: P2,
    arc: Option<ArcInfo>,
    from: u32,
}

fn offset_piece(s: &Sketch, piece: Piece, d: f64) -> R<OffPiece> {
    let e = ent(s, piece.entity).unwrap();
    match e.geometry {
        Geometry::Line { start, end } => {
            let (a, b) = if piece.reversed { (pt(s, end)?, pt(s, start)?) } else { (pt(s, start)?, pt(s, end)?) };
            let n = mul(left(unit(sub(b, a))), d);
            Ok(OffPiece { a: add(a, n), b: add(b, n), arc: None, from: e.id })
        }
        Geometry::Arc { center, start, end } => {
            let c = pt(s, center)?;
            let (ps, pe) = (pt(s, start)?, pt(s, end)?);
            let r = dist(ps, c);
            // Antihorario, la izquierda es hacia el centro
            let ccw = !piece.reversed;
            let r2 = if ccw { r - d } else { r + d };
            if r2 <= 1e-9 * span(s) {
                return Err("la distancia es mayor que el radio de un arco".into());
            }
            let (a, b) = if ccw { (ps, pe) } else { (pe, ps) };
            let on = |p: P2| add(c, mul(unit(sub(p, c)), r2));
            Ok(OffPiece { a: on(a), b: on(b), arc: Some(ArcInfo { c, center, r: r2, ccw }), from: e.id })
        }
        _ => Err("la cadena es de líneas y arcos".into()),
    }
}

/// Cruces de las curvas completas (recta infinita o círculo) de dos tramos.
fn curve_crossings(p: &OffPiece, q: &OffPiece) -> Vec<P2> {
    let line_circle = |a: P2, b: P2, c: P2, r: f64| -> Vec<P2> {
        let d = unit(sub(b, a));
        let m = sub(a, c);
        let bb = dot(m, d);
        let disc = bb * bb - (dot(m, m) - r * r);
        if disc < 0.0 {
            return Vec::new();
        }
        [-bb - disc.sqrt(), -bb + disc.sqrt()].iter().map(|t| add(a, mul(d, *t))).collect()
    };
    match (p.arc, q.arc) {
        (None, None) => {
            let (d, f) = (sub(p.b, p.a), sub(q.b, q.a));
            let den = cross(d, f);
            if den.abs() < 1e-12 * len(d) * len(f) {
                return Vec::new();
            }
            vec![add(p.a, mul(d, cross(sub(q.a, p.a), f) / den))]
        }
        (None, Some(k)) => line_circle(p.a, p.b, k.c, k.r),
        (Some(k), None) => line_circle(q.a, q.b, k.c, k.r),
        (Some(a), Some(b)) => {
            let dd = dist(a.c, b.c);
            if dd < 1e-12 || dd > a.r + b.r || dd < (a.r - b.r).abs() {
                return Vec::new();
            }
            let x = (a.r * a.r - b.r * b.r + dd * dd) / (2.0 * dd);
            let h = (a.r * a.r - x * x).max(0.0).sqrt();
            let ex = unit(sub(b.c, a.c));
            let base = add(a.c, mul(ex, x));
            vec![add(base, mul(left(ex), h)), add(base, mul(left(ex), -h))]
        }
    }
}

/// Punto del medio de una pieza de la equidistante.
fn piece_mid(o: &OffPiece) -> P2 {
    match o.arc {
        None => mul(add(o.a, o.b), 0.5),
        Some(k) => {
            let (from, to) = if k.ccw { (o.a, o.b) } else { (o.b, o.a) };
            let a0 = (from[1] - k.c[1]).atan2(from[0] - k.c[0]);
            let t = a0 + ccw_sweep(k.c, from, to) / 2.0;
            [k.c[0] + k.r * t.cos(), k.c[1] + k.r * t.sin()]
        }
    }
}

/// Arco alrededor de una esquina original cuando los tramos desplazados no se tocan.
#[derive(Debug, Clone, Copy)]
struct Round {
    at: usize,
    a: P2,
    b: P2,
}

/// Lleva los tramos desplazados a sus esquinas: donde se cruzan sus curvas
/// (lo más cerca de donde estaban) o, si no se tocan, con un arco alrededor
/// del vértice original (`at` = tramo que empieza en ese vértice).
fn join_offsets(pieces: &mut [OffPiece], closed: bool, tol: f64) -> Vec<Round> {
    let mut rounds = Vec::new();
    let n = pieces.len();
    for i in 0..(if closed { n } else { n - 1 }) {
        let j = (i + 1) % n;
        let (p, q) = (pieces[i], pieces[j]);
        if dist(p.b, q.a) <= tol {
            pieces[j].a = p.b;
            continue;
        }
        // Dos rectas sobre la misma recta (quedó afuera lo del medio): se siguen
        if p.arc.is_none() && q.arc.is_none() {
            let dp = sub(p.b, p.a);
            let l = if len(dp) == 0.0 { 1.0 } else { len(dp) };
            let dq = sub(q.b, q.a);
            if cross(dp, dq).abs() <= tol * l * len(dq) + 1e-12 && cross(dp, sub(q.a, p.a)).abs() / l <= tol * 10.0 {
                pieces[j].a = p.b;
                continue;
            }
        }
        let guess = mul(add(p.b, q.a), 0.5);
        let mut xs = curve_crossings(&p, &q);
        xs.sort_by(|x, y| dist(*x, guess).total_cmp(&dist(*y, guess)));
        // Un cruce lejos (rectas casi paralelas) no sirve: mejor redondear
        match xs.first() {
            Some(&near) if dist(near, guess) <= 4.0 * dist(p.b, q.a) + tol => {
                pieces[i].b = near;
                pieces[j].a = near;
            }
            _ => rounds.push(Round { at: j, a: p.b, b: q.a }),
        }
    }
    rounds
}

fn seg_dist(p: P2, a: P2, b: P2) -> f64 {
    let d = sub(b, a);
    let l2 = if dot(d, d) == 0.0 { 1.0 } else { dot(d, d) };
    let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
    dist(p, add(a, mul(d, t)))
}

/// Polilínea de una línea o arco de la cadena (el original, para ver qué
/// queda demasiado cerca de él).
fn polyline_of(s: &Sketch, id: u32) -> R<Vec<P2>> {
    match ent(s, id).map(|e| e.geometry.clone()) {
        Some(Geometry::Line { start, end }) => Ok(vec![pt(s, start)?, pt(s, end)?]),
        Some(Geometry::Arc { center, start, end }) => {
            let (c, a, b) = (pt(s, center)?, pt(s, start)?, pt(s, end)?);
            let r = dist(a, c);
            let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
            let sweep = ccw_sweep(c, a, b);
            let mut out = vec![a];
            out.extend((0..31).map(|i| {
                let t = a0 + sweep * (i + 1) as f64 / 32.0;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            }));
            out.push(b);
            Ok(out)
        }
        _ => Err("la cadena es de líneas y arcos".into()),
    }
}

/// Equidistante de una cadena de líneas y arcos a distancia `distance`
/// (positiva = a la izquierda del recorrido; en un lazo cerrado antihorario,
/// hacia adentro). `both`: a los dos lados. `caps` cierra una cadena abierta:
/// con arcos alrededor de las puntas (a los dos lados) o con líneas. Los arcos
/// desplazados comparten el centro con el original; las líneas quedan
/// paralelas (o horizontales/verticales como la original). Queda atada a la
/// original por una sola cota `offset`. Devuelve las entidades nuevas.
pub fn offset_chain(s: &mut Sketch, ids: &[u32], distance: f64, both: bool, caps: OffsetCaps) -> R<Vec<u32>> {
    let (pieces, closed) = order_chain(s, ids)?;
    if !(distance.abs() > 0.0) {
        return Err("la distancia no puede ser cero".into());
    }
    let tol = span(s) * 1e-9;
    let vertex_ids: Vec<u32> = pieces
        .iter()
        .map(|pc| {
            let (a, b) = chain_ends(s, pc.entity).unwrap();
            if pc.reversed { b } else { a }
        })
        .collect();
    let last = pieces[pieces.len() - 1];
    let (ls, le) = chain_ends(s, last.entity).unwrap();
    let tail_id = if last.reversed { ls } else { le };
    let sides: Vec<f64> = if both { vec![distance.abs(), -distance.abs()] } else { vec![distance] };
    let mut made: Vec<u32> = Vec::new();
    // [original, copia] para la cota: tramo y su copia, o vértice y el arco que lo rodea
    let mut pairs: Vec<[u32; 2]> = Vec::new();
    let mut side_ends: Vec<(u32, u32)> = Vec::new();
    // El original, para ver qué queda demasiado cerca de él
    let source: Vec<Vec<P2>> = pieces.iter().map(|pc| polyline_of(s, pc.entity)).collect::<R<_>>()?;
    let near_source = |p: P2| source.iter().map(|pl| pl.windows(2).fold(f64::INFINITY, |m, w| m.min(seg_dist(p, w[0], w[1])))).fold(f64::INFINITY, f64::min);
    // Calcular todo antes de tocar el sketch: si un lado falla no queda nada a medias
    struct SidePlan {
        off: Vec<OffPiece>,
        rounds: Vec<Round>,
        verts: Vec<u32>,
    }
    let mut plans: Vec<(f64, SidePlan)> = Vec::new();
    for &side in &sides {
        let mut off: Vec<OffPiece> = pieces.iter().map(|pc| offset_piece(s, *pc, side)).collect::<R<_>>()?;
        // Una pieza que, ya unida a sus vecinas, queda más cerca del original que
        // la distancia (una muesca angosta, una línea corta que se da vuelta) hace
        // que la equidistante se cruce consigo misma: se saca y se unen las vecinas
        let mut verts = vertex_ids.clone();
        let mut rounds: Vec<Round> = Vec::new();
        let slack = side.abs() * 1e-3 + tol;
        for _ in 0..=pieces.len() {
            let mut trial = off.clone();
            rounds = join_offsets(&mut trial, closed, tol);
            let mut worst: Option<usize> = None;
            let mut worst_gap = 0.0;
            for (i, o) in trial.iter().enumerate() {
                // Una recta que quedó sin largo también sobra
                let gap = if o.arc.is_none() && dist(o.a, o.b) <= slack { 2.0 * slack } else { side.abs() - near_source(piece_mid(o)) };
                if gap > slack && gap > worst_gap {
                    worst = Some(i);
                    worst_gap = gap;
                }
            }
            match worst {
                Some(w) if off.len() > 2 => {
                    off.remove(w);
                    // Su esquina de comienzo también (la que sigue conserva la suya)
                    verts.remove(w);
                }
                _ => {
                    off = trial;
                    break;
                }
            }
        }
        plans.push((side, SidePlan { off, rounds, verts }));
    }
    for (side, plan) in plans {
        // Puntos: uno por esquina compartida
        let mut point_at: HashMap<String, u32> = HashMap::new();
        let mut pid = |s: &mut Sketch, p: P2| -> u32 {
            let key = format!("{:.9},{:.9}", p[0], p[1]);
            *point_at.entry(key).or_insert_with(|| s.add_point(p[0], p[1]))
        };
        let rounds_at: HashMap<usize, Round> = plan.rounds.iter().map(|r| (r.at, *r)).collect();
        for (i, o) in plan.off.iter().enumerate() {
            if let Some(r) = rounds_at.get(&i).filter(|_| closed || i > 0) {
                // Arco alrededor del vértice original: antihorario según de qué lado va
                let v = plan.verts.get(i).copied().unwrap_or(vertex_ids[i]);
                let (a, b) = if side > 0.0 { (r.b, r.a) } else { (r.a, r.b) };
                let (pa, pb) = (pid(s, a), pid(s, b));
                let arc = s.add_entity(Geometry::Arc { center: v, start: pa, end: pb });
                made.push(arc);
                pairs.push([v, arc]);
            }
            let (a, b) = (pid(s, o.a), pid(s, o.b));
            match o.arc {
                None => {
                    let id = s.add_line(a, b);
                    let mut axis = false;
                    for k in s.constraints.clone() {
                        match k {
                            SketchConstraint::Horizontal { line } if line == o.from => {
                                s.constraints.push(SketchConstraint::Horizontal { line: id });
                                axis = true;
                            }
                            SketchConstraint::Vertical { line } if line == o.from => {
                                s.constraints.push(SketchConstraint::Vertical { line: id });
                                axis = true;
                            }
                            _ => {}
                        }
                    }
                    if !axis {
                        s.constraints.push(SketchConstraint::Parallel { a: o.from, b: id });
                    }
                    made.push(id);
                    pairs.push([o.from, id]);
                }
                Some(k) => {
                    let g = if k.ccw { Geometry::Arc { center: k.center, start: a, end: b } } else { Geometry::Arc { center: k.center, start: b, end: a } };
                    let arc = s.add_entity(g);
                    made.push(arc);
                    pairs.push([o.from, arc]);
                }
            }
        }
        let head = pid(s, plan.off[0].a);
        let tail = pid(s, plan.off[plan.off.len() - 1].b);
        side_ends.push((head, tail));
    }
    // Extremos cerrados (solo en cadenas abiertas)
    let caps = if closed { OffsetCaps::Open } else { caps };
    if caps != OffsetCaps::Open {
        let head_id = vertex_ids[0];
        if sides.len() == 2 {
            let (l, r) = (side_ends[0], side_ends[1]);
            if caps == OffsetCaps::Round {
                // Al final: de la derecha a la izquierda por delante; al comienzo, al revés
                let tail = s.add_entity(Geometry::Arc { center: tail_id, start: r.1, end: l.1 });
                let head = s.add_entity(Geometry::Arc { center: head_id, start: l.0, end: r.0 });
                made.extend([tail, head]);
                pairs.push([tail_id, tail]);
                pairs.push([head_id, head]);
            } else {
                made.push(s.add_line(r.1, l.1));
                made.push(s.add_line(l.0, r.0));
            }
        } else {
            made.push(s.add_line(tail_id, side_ends[0].1));
            made.push(s.add_line(side_ends[0].0, head_id));
        }
    }
    if !pairs.is_empty() {
        s.constraints.push(SketchConstraint::Offset { pairs, value: round(distance.abs(), 6), reference: false, opts: DimOpts::default() });
    }
    Ok(made)
}

// ─── Recortar ─────────────────────────────────────────────────────────────

/// Con qué se cortó: línea (se parte en el punto), círculo o arco, u otra curva (spline, elipse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitKind {
    Line,
    Circle,
    Curve,
}

#[derive(Debug, Clone, Copy)]
struct Hit {
    entity: u32,
    kind: HitKind,
}

/// Polilínea de una spline por puntos (aproximada por sus puntos de paso) o
/// de una elipse entera; nada para lo demás.
fn free_curve_line(s: &Sketch, g: &Geometry) -> Option<Vec<P2>> {
    match g {
        Geometry::Spline { points, closed, .. } => {
            let mut v: Vec<P2> = points.iter().map(|p| pos(s, *p)).collect::<Option<_>>()?;
            if *closed && !v.is_empty() {
                v.push(v[0]);
            }
            Some(v)
        }
        Geometry::Ellipse { center, major, minor } => {
            let (c, a, b) = (pos(s, *center)?, pos(s, *major)?, pos(s, *minor)?);
            let (u, v) = (sub(a, c), sub(b, c));
            Some(
                (0..=64)
                    .map(|i| {
                        let t = i as f64 / 64.0 * std::f64::consts::TAU;
                        [c[0] + u[0] * t.cos() + v[0] * t.sin(), c[1] + u[1] * t.cos() + v[1] * t.sin()]
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Centro y radio de un círculo o arco; ángulo inicial y barrido si es arco.
struct Round2 {
    c: P2,
    r: f64,
    arc: Option<(f64, f64)>,
}

fn circle_of(s: &Sketch, g: &Geometry) -> Option<Round2> {
    match g {
        Geometry::Circle { center, radius } => Some(Round2 { c: pos(s, *center)?, r: *radius, arc: None }),
        Geometry::Arc { center, start, end } => {
            let (c, a, b) = (pos(s, *center)?, pos(s, *start)?, pos(s, *end)?);
            let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
            let mut sweep = (b[1] - c[1]).atan2(b[0] - c[0]) - a0;
            while sweep <= 1e-12 {
                sweep += std::f64::consts::TAU;
            }
            Some(Round2 { c, r: dist(a, c), arc: Some((a0, sweep)) })
        }
        _ => None,
    }
}

/// Ángulo relativo al inicio del arco, en [0, 2π).
fn rel_angle(p: P2, c: P2, a0: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut t = (p[1] - c[1]).atan2(p[0] - c[0]) - a0;
    while t < 0.0 {
        t += tau;
    }
    while t >= tau {
        t -= tau;
    }
    t
}

/// Puntos donde las demás entidades (no de construcción) cruzan un círculo o arco.
fn crossings_on_circle(s: &Sketch, id: u32, c: P2, r: f64) -> Vec<(P2, Hit)> {
    let mut out = Vec::new();
    for e in &s.entities {
        if e.id == id || e.construction {
            continue;
        }
        if let Geometry::Line { start, end } = e.geometry {
            let (Some(a), Some(b)) = (pos(s, start), pos(s, end)) else { continue };
            let d = sub(b, a);
            let m = sub(a, c);
            let qa = dot(d, d);
            let qb = 2.0 * dot(m, d);
            let qc = dot(m, m) - r * r;
            let disc = qb * qb - 4.0 * qa * qc;
            if disc < 0.0 || qa == 0.0 {
                continue;
            }
            for u in [(-qb - disc.sqrt()) / (2.0 * qa), (-qb + disc.sqrt()) / (2.0 * qa)] {
                if (-1e-9..=1.0 + 1e-9).contains(&u) {
                    out.push((add(a, mul(d, u)), Hit { entity: e.id, kind: HitKind::Line }));
                }
            }
        } else if let Some(pl) = free_curve_line(s, &e.geometry) {
            for p in polyline_on_circle(c, r, &pl) {
                out.push((p, Hit { entity: e.id, kind: HitKind::Curve }));
            }
        } else if let Some(o) = circle_of(s, &e.geometry) {
            let (dx, dy) = (o.c[0] - c[0], o.c[1] - c[1]);
            let dd = dx.hypot(dy);
            if dd < 1e-12 || dd > r + o.r || dd < (r - o.r).abs() {
                continue;
            }
            let a = (r * r - o.r * o.r + dd * dd) / (2.0 * dd);
            let h = (r * r - a * a).max(0.0).sqrt();
            let base = [c[0] + a * dx / dd, c[1] + a * dy / dd];
            for sgn in if h > 1e-12 { vec![1.0, -1.0] } else { vec![1.0] } {
                let p = [base[0] - sgn * h * dy / dd, base[1] + sgn * h * dx / dd];
                if let Some((a0, sweep)) = o.arc
                    && rel_angle(p, o.c, a0) > sweep + 1e-9
                {
                    continue;
                }
                out.push((p, Hit { entity: e.id, kind: HitKind::Circle }));
            }
        }
    }
    out
}

/// Puntos donde una polilínea cruza el círculo (c, r).
fn polyline_on_circle(c: P2, r: f64, pl: &[P2]) -> Vec<P2> {
    let mut out = Vec::new();
    for w in pl.windows(2) {
        let (cc, dd) = (w[0], w[1]);
        let f = sub(dd, cc);
        let m = sub(cc, c);
        let a = dot(f, f);
        if a < 1e-18 {
            continue;
        }
        let b = 2.0 * dot(m, f);
        let disc = b * b - 4.0 * a * (dot(m, m) - r * r);
        if disc < 0.0 {
            continue;
        }
        for u in [(-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a)] {
            if (0.0..1.0).contains(&u) {
                out.push(add(cc, mul(f, u)));
            }
        }
    }
    out
}

/// Parámetros t de A + t·d donde la recta cruza los tramos de una polilínea.
fn polyline_on_line(a: P2, d: P2, pl: &[P2]) -> Vec<f64> {
    let mut out = Vec::new();
    for w in pl.windows(2) {
        let (c, dd) = (w[0], w[1]);
        let f = sub(dd, c);
        let den = cross(d, f);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = cross(sub(c, a), f) / den;
        let u = cross(sub(c, a), d) / den;
        if (-1e-9..=1.0 + 1e-9).contains(&u) {
            out.push(t);
        }
    }
    out
}

/// Parte la línea `line_id` en el punto `point` (que debe estar sobre ella):
/// dos líneas que lo comparten, así las regiones se cierran en ese cruce.
/// Copia horizontal/vertical (o deja las dos mitades paralelas); la cota de
/// largo de la línea entera pasa a ser la distancia entre sus extremos.
/// Devuelve la mitad nueva.
fn split_line_at(s: &mut Sketch, line_id: u32, point: u32) -> Option<u32> {
    let line = ent(s, line_id)?.clone();
    let Geometry::Line { start, end: old_end } = line.geometry else { return None };
    if start == point || old_end == point {
        return None;
    }
    if let Some(SketchEntity { geometry: Geometry::Line { end, .. }, .. }) = ent_mut(s, line_id) {
        *end = point;
    }
    let rest = s.add_line(point, old_end);
    inherit_marks(s, &line, rest);
    let mut axis = false;
    for k in s.constraints.clone() {
        match k {
            SketchConstraint::Horizontal { line } if line == line_id => {
                s.constraints.push(SketchConstraint::Horizontal { line: rest });
                axis = true;
            }
            SketchConstraint::Vertical { line } if line == line_id => {
                s.constraints.push(SketchConstraint::Vertical { line: rest });
                axis = true;
            }
            _ => {}
        }
    }
    if !axis {
        s.constraints.push(SketchConstraint::Parallel { a: line_id, b: rest });
    }
    // En el lugar (mismo índice)
    for k in &mut s.constraints {
        if let SketchConstraint::Length { line, value, reference, opts } = k
            && *line == line_id
        {
            *k = SketchConstraint::Distance { a: start, b: old_end, value: *value, reference: *reference, opts: opts.clone() };
        }
    }
    // El punto ya es extremo compartido: no necesita "punto en línea"
    s.constraints.retain(|k| !matches!(k, SketchConstraint::PointOnLine { point: p, line } if *p == point && *line == line_id));
    Some(rest)
}

/// Deja el punto nuevo sobre lo que cruza: parte la línea o lo pega a la
/// curva. Si la línea ya se partió antes (dos cruces sobre el mismo lado),
/// se parte el tramo que de verdad contiene el punto.
fn stick_to(s: &mut Sketch, point: u32, hit: Hit) {
    match hit.kind {
        HitKind::Curve => {
            s.constraints.push(SketchConstraint::PointOnCurve { point, curve: hit.entity });
            return;
        }
        HitKind::Circle => {
            s.constraints.push(SketchConstraint::PointOnCircle { point, circle: hit.entity });
            return;
        }
        HitKind::Line => {}
    }
    let Some(p) = pos(s, point) else { return };
    let on_segment = |s: &Sketch, id: u32| -> f64 {
        let Some((a, b)) = line_of(s, id) else { return f64::INFINITY };
        let d = sub(b, a);
        let l2 = if dot(d, d) == 0.0 { 1.0 } else { dot(d, d) };
        let t = dot(sub(p, a), d) / l2;
        if !(-1e-9..=1.0 + 1e-9).contains(&t) {
            return f64::INFINITY;
        }
        len(sub(p, add(a, mul(d, t))))
    };
    let tol = 1e-6 * len(p).max(1.0);
    if on_segment(s, hit.entity) <= tol {
        split_line_at(s, hit.entity, point);
        return;
    }
    // La línea cruzada ya se partió: el tramo que contiene el punto (sin
    // contar las que ya lo tienen como extremo)
    let mut best: Option<(u32, f64)> = None;
    for e in &s.entities {
        if let Geometry::Line { start, end } = e.geometry
            && start != point
            && end != point
        {
            let d = on_segment(s, e.id);
            if d <= tol && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((e.id, d));
            }
        }
    }
    match best {
        Some((id, _)) => {
            split_line_at(s, id, point);
        }
        None => s.constraints.push(SketchConstraint::PointOnLine { point, line: hit.entity }),
    }
}

fn drop_length(s: &mut Sketch, line_id: u32) {
    s.constraints.retain(|k| !matches!(k, SketchConstraint::Length { line, .. } if *line == line_id));
}

/// Recorta una línea: quita el tramo entre cruces (con otras líneas, círculos,
/// arcos, elipses o splines) donde cayó el clic `p`.
fn trim_line(s: &mut Sketch, line_id: u32, p: P2) -> R<()> {
    let line = ent(s, line_id).ok_or("no existe esa entidad")?.clone();
    let Geometry::Line { start, end } = line.geometry else { return Err("solo se recortan líneas".into()) };
    let (a, b) = (pt(s, start)?, pt(s, end)?);
    let d = sub(b, a);
    let len2 = dot(d, d);
    let eps = 1e-9;
    // Cruces: parámetro sobre la línea y con qué entidad
    let mut cuts: Vec<(f64, Hit)> = Vec::new();
    for e in &s.entities {
        if e.id == line_id || e.construction {
            continue;
        }
        match &e.geometry {
            Geometry::Line { start: s2, end: e2 } => {
                let (Some(c), Some(dd)) = (pos(s, *s2), pos(s, *e2)) else { continue };
                let f = sub(dd, c);
                let den = d[0] * f[1] - d[1] * f[0];
                if den.abs() < eps {
                    continue;
                }
                let t = ((c[0] - a[0]) * f[1] - (c[1] - a[1]) * f[0]) / den;
                let u = ((c[0] - a[0]) * d[1] - (c[1] - a[1]) * d[0]) / den;
                if t > eps && t < 1.0 - eps && u >= -eps && u <= 1.0 + eps {
                    cuts.push((t, Hit { entity: e.id, kind: HitKind::Line }));
                }
            }
            g @ (Geometry::Circle { .. } | Geometry::Arc { .. }) => {
                let Some(o) = circle_of(s, g) else { continue };
                let m = sub(a, o.c);
                let bb = 2.0 * dot(m, d);
                let cc = dot(m, m) - o.r * o.r;
                let disc = bb * bb - 4.0 * len2 * cc;
                if disc < 0.0 {
                    continue;
                }
                for t in [(-bb - disc.sqrt()) / (2.0 * len2), (-bb + disc.sqrt()) / (2.0 * len2)] {
                    if t <= eps || t >= 1.0 - eps {
                        continue;
                    }
                    if let Some((a0, sweep)) = o.arc {
                        // ¿El cruce cae dentro del barrido del arco?
                        let q = [a[0] + t * d[0], a[1] + t * d[1]];
                        if rel_angle(q, o.c, a0) > sweep {
                            continue;
                        }
                    }
                    cuts.push((t, Hit { entity: e.id, kind: HitKind::Circle }));
                }
            }
            g => {
                if let Some(pl) = free_curve_line(s, g) {
                    for t in polyline_on_line(a, d, &[pl.as_slice()].concat()) {
                        if t > eps && t < 1.0 - eps {
                            cuts.push((t, Hit { entity: e.id, kind: HitKind::Curve }));
                        }
                    }
                }
            }
        }
    }
    if cuts.is_empty() {
        // Sin cruces: el tramo entero se va
        return remove_entity(s, line_id);
    }
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let tc = dot(sub(p, a), d) / len2;
    let before = cuts.iter().rev().find(|c| c.0 < tc).copied();
    let after = cuts.iter().find(|c| c.0 > tc).copied();
    let at = |t: f64| [a[0] + t * d[0], a[1] + t * d[1]];
    match (before, after) {
        (Some(bf), Some(af)) => {
            // Tramo del medio: la línea queda partida en dos
            let p1 = s.add_point(at(bf.0)[0], at(bf.0)[1]);
            let p2 = s.add_point(at(af.0)[0], at(af.0)[1]);
            if let Some(SketchEntity { geometry: Geometry::Line { end, .. }, .. }) = ent_mut(s, line_id) {
                *end = p1;
            }
            let rest = s.add_line(p2, end);
            inherit_marks(s, &line, rest);
            stick_to(s, p1, bf.1);
            stick_to(s, p2, af.1);
            // Horizontal/vertical valen para las dos partes
            for k in s.constraints.clone() {
                match k {
                    SketchConstraint::Horizontal { line } if line == line_id => s.constraints.push(SketchConstraint::Horizontal { line: rest }),
                    SketchConstraint::Vertical { line } if line == line_id => s.constraints.push(SketchConstraint::Vertical { line: rest }),
                    _ => {}
                }
            }
            drop_length(s, line_id);
        }
        (None, Some(af)) => {
            let np = s.add_point(at(af.0)[0], at(af.0)[1]);
            if let Some(SketchEntity { geometry: Geometry::Line { start, .. }, .. }) = ent_mut(s, line_id) {
                *start = np;
            }
            stick_to(s, np, af.1);
            drop_length(s, line_id);
        }
        (Some(bf), None) => {
            let np = s.add_point(at(bf.0)[0], at(bf.0)[1]);
            if let Some(SketchEntity { geometry: Geometry::Line { end, .. }, .. }) = ent_mut(s, line_id) {
                *end = np;
            }
            stick_to(s, np, bf.1);
            drop_length(s, line_id);
        }
        (None, None) => {}
    }
    drop_loose_points(s);
    Ok(())
}

/// Recorta un círculo o un arco en el tramo entre cruces donde cayó `p`: el
/// círculo pasa a ser arco; el arco se acorta o se parte en dos. Sin cruces
/// se borra entero.
fn trim_curve(s: &mut Sketch, id: u32, p: P2) -> R<()> {
    let e = ent(s, id).ok_or("no existe esa entidad")?.clone();
    let o = circle_of(s, &e.geometry).ok_or("solo círculos y arcos")?;
    let (a0, sweep) = o.arc.unwrap_or((0.0, std::f64::consts::TAU));
    let mut cuts: Vec<(f64, P2, Hit)> = crossings_on_circle(s, id, o.c, o.r)
        .into_iter()
        .map(|(q, h)| (rel_angle(q, o.c, a0), q, h))
        .filter(|(t, _, _)| o.arc.is_none() || (*t > 1e-9 && *t < sweep - 1e-9))
        .collect();
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
    if cuts.is_empty() {
        return remove_entity(s, id);
    }
    let tc = rel_angle(p, o.c, a0);
    let before = cuts.iter().rev().find(|c| c.0 < tc).copied();
    let after = cuts.iter().find(|c| c.0 > tc).copied();
    match e.geometry {
        Geometry::Circle { center, .. } => {
            if cuts.len() < 2 {
                return Err("el círculo necesita al menos dos cruces para recortarlo".into());
            }
            // Tramo quitado: entre el último cruce antes del clic y el primero después
            let af = after.unwrap_or(cuts[0]);
            let bf = before.unwrap_or(cuts[cuts.len() - 1]);
            let ps = s.add_point(af.1[0], af.1[1]);
            let pe = s.add_point(bf.1[0], bf.1[1]);
            ent_mut(s, id).unwrap().geometry = Geometry::Arc { center, start: ps, end: pe };
            stick_to(s, ps, af.2);
            stick_to(s, pe, bf.2);
            Ok(())
        }
        Geometry::Arc { center, end: old_end, .. } => {
            match (before, after) {
                (Some(bf), Some(af)) => {
                    let pb = s.add_point(bf.1[0], bf.1[1]);
                    let pa = s.add_point(af.1[0], af.1[1]);
                    if let Some(SketchEntity { geometry: Geometry::Arc { end, .. }, .. }) = ent_mut(s, id) {
                        *end = pb;
                    }
                    let rest = s.add_entity(Geometry::Arc { center, start: pa, end: old_end });
                    inherit_marks(s, &e, rest);
                    stick_to(s, pb, bf.2);
                    stick_to(s, pa, af.2);
                }
                (None, Some(af)) => {
                    let pa = s.add_point(af.1[0], af.1[1]);
                    if let Some(SketchEntity { geometry: Geometry::Arc { start, .. }, .. }) = ent_mut(s, id) {
                        *start = pa;
                    }
                    stick_to(s, pa, af.2);
                }
                (Some(bf), None) => {
                    let pb = s.add_point(bf.1[0], bf.1[1]);
                    if let Some(SketchEntity { geometry: Geometry::Arc { end, .. }, .. }) = ent_mut(s, id) {
                        *end = pb;
                    }
                    stick_to(s, pb, bf.2);
                }
                (None, None) => {}
            }
            drop_loose_points(s);
            Ok(())
        }
        _ => Err("solo círculos y arcos".into()),
    }
}

/// Recorta lo que haya en el punto `at` de la entidad `entity` (línea, arco o
/// círculo): quita el tramo entre los cruces con el resto del dibujo (no de
/// construcción) donde cae `at`. Los extremos nuevos quedan pegados a lo que
/// cruzan (las líneas cruzadas se parten ahí); las marcas (construcción,
/// capa, eje, infinita) pasan a los tramos nuevos. Sin cruces, se borra la
/// entidad entera.
pub fn trim(s: &mut Sketch, entity: u32, at: P2) -> R<()> {
    match ent(s, entity).map(|e| &e.geometry) {
        None => Err(format!("no existe la entidad {entity}")),
        Some(Geometry::Line { .. }) => trim_line(s, entity, at),
        Some(Geometry::Circle { .. } | Geometry::Arc { .. }) => trim_curve(s, entity, at),
        Some(_) => Err("solo se recortan líneas, arcos y círculos".into()),
    }
}
