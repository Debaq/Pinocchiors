//! Sketch 3D: puntos en el espacio, líneas, arcos por tres puntos y splines,
//! con restricciones 3D (paralela a un eje, sobre un plano, sobre un punto del
//! modelo…). Lo resuelve un solver propio (Levenberg-Marquardt denso con
//! jacobiano numérico: son sketches chicos) y da alambres para caminos de
//! barrido.
//!
//! Las restricciones que nombran al modelo (un plano, un vértice) no guardan
//! la geometría: el historial la resuelve al recalcular y se la pasa al solver.

use std::collections::HashMap;

use cad_occt::Curve;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};

use crate::feature::{PlaneSpec, PointSpec};
use crate::geom::*;
use crate::sketch::SketchStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point3d {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3d {
    pub fn at(&self) -> P3 {
        [self.x, self.y, self.z]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity3d {
    pub id: u32,
    #[serde(default)]
    pub construction: bool,
    pub geometry: Geometry3d,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry3d {
    Line { start: u32, end: u32 },
    /// Arco por tres puntos: de `start` a `end` pasando por `mid`.
    Arc { start: u32, mid: u32, end: u32 },
    /// Spline interpolada por los puntos.
    Spline { points: Vec<u32> },
    Point { point: u32 },
}

impl Geometry3d {
    pub fn point_ids(&self) -> Vec<u32> {
        match self {
            Geometry3d::Line { start, end } => vec![*start, *end],
            Geometry3d::Arc { start, mid, end } => vec![*start, *mid, *end],
            Geometry3d::Spline { points } => points.clone(),
            Geometry3d::Point { point } => vec![*point],
        }
    }

    /// Extremos (comienzo y fin) de una curva.
    pub fn ends(&self) -> Option<(u32, u32)> {
        match self {
            Geometry3d::Line { start, end } | Geometry3d::Arc { start, end, .. } => Some((*start, *end)),
            Geometry3d::Spline { points } if points.len() >= 2 => Some((points[0], *points.last()?)),
            _ => None,
        }
    }
}

/// Eje del mundo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldAxis {
    X,
    Y,
    Z,
}

impl WorldAxis {
    fn index(self) -> usize {
        match self {
            WorldAxis::X => 0,
            WorldAxis::Y => 1,
            WorldAxis::Z => 2,
        }
    }
}

/// Restricciones del sketch 3D. Distancias en mm, ángulos en grados.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint3d {
    Coincident { a: u32, b: u32 },
    Fixed { point: u32, at: P3 },
    /// Sobre un punto del modelo: un vértice (extremo de arista), el centro de
    /// una arista circular o un punto de referencia.
    Attach { point: u32, target: PointSpec },
    /// Sobre un plano (base, cara plana o de referencia), corrido `offset` por
    /// su normal.
    OnPlane {
        point: u32,
        plane: PlaneSpec,
        #[serde(default)]
        offset: f64,
    },
    /// Línea paralela a un eje del mundo.
    AlongAxis { line: u32, axis: WorldAxis },
    Parallel { a: u32, b: u32 },
    Perpendicular { a: u32, b: u32 },
    /// Líneas del mismo largo.
    Equal { a: u32, b: u32 },
    /// Tangentes donde se juntan (líneas y arcos que comparten un extremo).
    Tangent { a: u32, b: u32 },
    Midpoint { point: u32, line: u32 },
    Length { line: u32, value: f64 },
    Distance { a: u32, b: u32, value: f64 },
    Angle { a: u32, b: u32, degrees: f64 },
}

impl Constraint3d {
    pub fn value(&self) -> Option<f64> {
        match self {
            Constraint3d::Length { value, .. } | Constraint3d::Distance { value, .. } => Some(*value),
            Constraint3d::Angle { degrees, .. } => Some(*degrees),
            _ => None,
        }
    }

    pub fn set_value(&mut self, v: f64) -> bool {
        match self {
            Constraint3d::Length { value, .. } | Constraint3d::Distance { value, .. } => *value = v,
            Constraint3d::Angle { degrees, .. } => *degrees = v,
            _ => return false,
        }
        true
    }

    /// Ids de puntos y entidades que nombra.
    pub fn ids(&self) -> Vec<u32> {
        use Constraint3d::*;
        match self {
            Coincident { a, b } | Parallel { a, b } | Perpendicular { a, b } | Equal { a, b } | Tangent { a, b } | Distance { a, b, .. } | Angle { a, b, .. } => vec![*a, *b],
            Fixed { point, .. } | Attach { point, .. } | OnPlane { point, .. } => vec![*point],
            AlongAxis { line, .. } | Length { line, .. } => vec![*line],
            Midpoint { point, line } => vec![*point, *line],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sketch3d {
    pub points: Vec<Point3d>,
    pub entities: Vec<Entity3d>,
    pub constraints: Vec<Constraint3d>,
    #[serde(default)]
    pub next_id: u32,
}

/// Geometría del modelo que nombran las restricciones (por índice de
/// restricción), resuelta por el historial.
#[derive(Debug, Clone, Default)]
pub struct Targets {
    pub points: HashMap<usize, P3>,
    pub planes: HashMap<usize, Plane>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report3d {
    pub status: SketchStatus,
    pub dof: i32,
    pub residual: f64,
    /// Restricciones que chocan (índices); las demás se cumplen.
    pub conflicting: Vec<usize>,
    /// Restricciones que nombran algo del modelo que no se encontró.
    #[serde(default)]
    pub missing: Vec<usize>,
    pub free_points: Vec<u32>,
    pub free_entities: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Sketch3dError {
    #[error("el punto {0} no existe")]
    NoPoint(u32),
    #[error("la entidad {0} no existe")]
    NoEntity(u32),
    #[error("la entidad {0} no es {1}")]
    WrongKind(u32, &'static str),
    #[error("{0}")]
    Invalid(String),
}

type R<T> = Result<T, Sketch3dError>;

/// Una ecuación del sistema (o varias filas) sobre el vector de incógnitas.
enum Eq3 {
    /// p − q (3 filas)
    Same(usize, usize),
    /// p − at (3)
    At(usize, P3),
    /// (p − o)·n (1)
    Plane(usize, P3, P3),
    /// Componentes de b − a fuera del eje (2)
    Along(usize, usize, usize),
    /// (b − a) × (d − c) (3)
    Parallel([usize; 4]),
    /// (b − a)·(d − c) (1)
    Perp([usize; 4]),
    /// |b − a| − |d − c| (1)
    EqualLen([usize; 4]),
    /// Tangentes en el punto común paralelas (3): dirección de cada lado
    Tangent(Side, Side),
    /// p − (a + b) / 2 (3)
    Mid(usize, usize, usize),
    /// |b − a| − v (1)
    Dist(usize, usize, f64),
    /// ángulo entre b − a y d − c − v (1)
    Angle([usize; 4], f64),
}

/// Dirección de una curva en uno de sus extremos (para la tangencia).
#[derive(Clone, Copy)]
enum Side {
    Line(usize, usize),
    /// Arco por (start, mid, end) y en cuál extremo
    Arc([usize; 3], bool),
}

impl Eq3 {
    fn rows(&self) -> usize {
        match self {
            Eq3::Same(..) | Eq3::At(..) | Eq3::Parallel(..) | Eq3::Tangent(..) | Eq3::Mid(..) => 3,
            Eq3::Along(..) => 2,
            _ => 1,
        }
    }

    fn eval(&self, x: &[f64], out: &mut Vec<f64>) {
        let p = |i: usize| [x[3 * i], x[3 * i + 1], x[3 * i + 2]];
        let d = |a: usize, b: usize| sub(p(b), p(a));
        match *self {
            Eq3::Same(a, b) => out.extend(sub(p(a), p(b))),
            Eq3::At(a, at) => out.extend(sub(p(a), at)),
            Eq3::Plane(a, o, n) => out.push(dot(sub(p(a), o), n)),
            Eq3::Along(a, b, axis) => {
                let v = d(a, b);
                out.extend((0..3).filter(|&k| k != axis).map(|k| v[k]));
            }
            Eq3::Parallel([a, b, c, e]) => out.extend(cross(unit(d(a, b)), unit(d(c, e)))),
            Eq3::Perp([a, b, c, e]) => out.push(dot(unit(d(a, b)), unit(d(c, e)))),
            Eq3::EqualLen([a, b, c, e]) => out.push(norm(d(a, b)) - norm(d(c, e))),
            Eq3::Tangent(s, t) => out.extend(cross(side_dir(&p, s), side_dir(&p, t))),
            Eq3::Mid(m, a, b) => out.extend(sub(p(m), scale(add(p(a), p(b)), 0.5))),
            Eq3::Dist(a, b, v) => out.push(norm(d(a, b)) - v),
            Eq3::Angle([a, b, c, e], v) => {
                let (u, w) = (d(a, b), d(c, e));
                out.push(norm(cross(u, w)).atan2(dot(u, w)) - v);
            }
        }
    }

    /// Índices de los puntos que toca (para el jacobiano numérico).
    fn points(&self) -> Vec<usize> {
        match *self {
            Eq3::Same(a, b) | Eq3::Along(a, b, _) | Eq3::Dist(a, b, _) => vec![a, b],
            Eq3::At(a, _) | Eq3::Plane(a, ..) => vec![a],
            Eq3::Parallel(q) | Eq3::Perp(q) | Eq3::EqualLen(q) | Eq3::Angle(q, _) => q.to_vec(),
            Eq3::Mid(m, a, b) => vec![m, a, b],
            Eq3::Tangent(s, t) => [s, t]
                .iter()
                .flat_map(|s| match *s {
                    Side::Line(a, b) => vec![a, b],
                    Side::Arc(q, _) => q.to_vec(),
                })
                .collect(),
        }
    }
}

fn unit(v: P3) -> P3 {
    let n = norm(v);
    if n < 1e-12 { v } else { scale(v, 1.0 / n) }
}

/// Centro del círculo por tres puntos.
pub fn circumcenter(a: P3, b: P3, c: P3) -> Option<P3> {
    let (ab, ac) = (sub(b, a), sub(c, a));
    let n = cross(ab, ac);
    let nn = dot(n, n);
    if nn < 1e-18 {
        return None;
    }
    let t = add(scale(cross(n, ab), dot(ac, ac)), scale(cross(ac, n), dot(ab, ab)));
    Some(add(a, scale(t, 0.5 / nn)))
}

fn side_dir(p: &dyn Fn(usize) -> P3, s: Side) -> P3 {
    match s {
        Side::Line(a, b) => unit(sub(p(b), p(a))),
        Side::Arc([a, m, b], at_start) => {
            let (pa, pm, pb) = (p(a), p(m), p(b));
            let Some(c) = circumcenter(pa, pm, pb) else { return unit(sub(pb, pa)) };
            let n = cross(sub(pm, pa), sub(pb, pa));
            let at = if at_start { pa } else { pb };
            unit(cross(n, sub(at, c)))
        }
    }
}

impl Sketch3d {
    pub fn point(&self, id: u32) -> R<P3> {
        self.points.iter().find(|p| p.id == id).map(Point3d::at).ok_or(Sketch3dError::NoPoint(id))
    }

    pub fn entity(&self, id: u32) -> R<&Entity3d> {
        self.entities.iter().find(|e| e.id == id).ok_or(Sketch3dError::NoEntity(id))
    }

    fn fresh_id(&mut self) -> u32 {
        let used = self.points.iter().map(|p| p.id).chain(self.entities.iter().map(|e| e.id)).max().map_or(0, |m| m + 1);
        let id = self.next_id.max(used);
        self.next_id = id + 1;
        id
    }

    pub fn add_point(&mut self, p: P3) -> u32 {
        let id = self.fresh_id();
        self.points.push(Point3d { id, x: p[0], y: p[1], z: p[2] });
        id
    }

    pub fn add_entity(&mut self, geometry: Geometry3d) -> u32 {
        let id = self.fresh_id();
        self.entities.push(Entity3d { id, construction: false, geometry });
        id
    }

    /// Polilínea de puntos nuevos unidos por líneas; devuelve las líneas.
    pub fn polyline(&mut self, pts: &[P3]) -> Vec<u32> {
        let ids: Vec<u32> = pts.iter().map(|&p| self.add_point(p)).collect();
        ids.windows(2).map(|w| self.add_entity(Geometry3d::Line { start: w[0], end: w[1] })).collect()
    }

    fn line_ends(&self, id: u32) -> R<(u32, u32)> {
        match self.entity(id)?.geometry {
            Geometry3d::Line { start, end } => Ok((start, end)),
            _ => Err(Sketch3dError::WrongKind(id, "una línea")),
        }
    }

    /// Ecuaciones de una restricción (con la geometría del modelo resuelta).
    fn lower(&self, i: usize, c: &Constraint3d, ix: &HashMap<u32, usize>, targets: &Targets) -> R<Option<Eq3>> {
        let p = |id: u32| ix.get(&id).copied().ok_or(Sketch3dError::NoPoint(id));
        let line = |id: u32| -> R<(usize, usize)> {
            let (a, b) = self.line_ends(id)?;
            Ok((p(a)?, p(b)?))
        };
        let quad = |a: u32, b: u32| -> R<[usize; 4]> {
            let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
            Ok([a1, a2, b1, b2])
        };
        Ok(Some(match c {
            Constraint3d::Coincident { a, b } => Eq3::Same(p(*a)?, p(*b)?),
            Constraint3d::Fixed { point, at } => Eq3::At(p(*point)?, *at),
            Constraint3d::Attach { point, .. } => match targets.points.get(&i) {
                Some(&at) => Eq3::At(p(*point)?, at),
                None => return Ok(None),
            },
            Constraint3d::OnPlane { point, offset, .. } => match targets.planes.get(&i) {
                Some(pl) => {
                    let n = unit(pl.normal);
                    Eq3::Plane(p(*point)?, add(pl.origin, scale(n, *offset)), n)
                }
                None => return Ok(None),
            },
            Constraint3d::AlongAxis { line: l, axis } => {
                let (a, b) = line(*l)?;
                Eq3::Along(a, b, axis.index())
            }
            Constraint3d::Parallel { a, b } => Eq3::Parallel(quad(*a, *b)?),
            Constraint3d::Perpendicular { a, b } => Eq3::Perp(quad(*a, *b)?),
            Constraint3d::Equal { a, b } => Eq3::EqualLen(quad(*a, *b)?),
            Constraint3d::Midpoint { point, line: l } => {
                let (a, b) = line(*l)?;
                Eq3::Mid(p(*point)?, a, b)
            }
            Constraint3d::Length { line: l, value } => {
                let (a, b) = line(*l)?;
                Eq3::Dist(a, b, *value)
            }
            Constraint3d::Distance { a, b, value } => Eq3::Dist(p(*a)?, p(*b)?, *value),
            Constraint3d::Angle { a, b, degrees } => Eq3::Angle(quad(*a, *b)?, degrees.to_radians()),
            Constraint3d::Tangent { a, b } => {
                let (ea, eb) = (self.entity(*a)?.geometry.ends(), self.entity(*b)?.geometry.ends());
                let (Some(ea), Some(eb)) = (ea, eb) else {
                    return Err(Sketch3dError::Invalid("la tangencia es entre líneas y arcos".into()));
                };
                let shared = [ea.0, ea.1].into_iter().find(|q| *q == eb.0 || *q == eb.1).ok_or_else(|| Sketch3dError::Invalid("tangentes sin un extremo en común".into()))?;
                let side = |id: u32| -> R<Side> {
                    match self.entity(id)?.geometry {
                        Geometry3d::Line { start, end } => Ok(Side::Line(p(start)?, p(end)?)),
                        Geometry3d::Arc { start, mid, end } => Ok(Side::Arc([p(start)?, p(mid)?, p(end)?], start == shared)),
                        _ => Err(Sketch3dError::WrongKind(id, "una línea o un arco")),
                    }
                };
                Eq3::Tangent(side(*a)?, side(*b)?)
            }
        }))
    }

    /// Resuelve con la geometría del modelo ya resuelta en `targets`.
    pub fn solve(&mut self, targets: &Targets) -> R<Report3d> {
        let ix: HashMap<u32, usize> = self.points.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        let mut eqs: Vec<(usize, Eq3)> = Vec::new();
        let mut missing = Vec::new();
        for (i, c) in self.constraints.iter().enumerate() {
            match self.lower(i, c, &ix, targets)? {
                Some(e) => eqs.push((i, e)),
                None => missing.push(i),
            }
        }
        let start: Vec<f64> = self.points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        let size = self.points.iter().flat_map(|p| [p.x.abs(), p.y.abs(), p.z.abs()]).fold(1.0f64, f64::max);
        let tol = 1e-9 * size;
        let mut x = start.clone();
        let mut residual = lm(&eqs, &mut x, size);
        // Conflicto: se saca la que más falla y se vuelve a empezar, hasta que
        // lo demás se cumpla (las sacadas son las que chocan)
        let mut conflicting = Vec::new();
        let mut active: Vec<usize> = (0..eqs.len()).collect();
        while residual > tol && conflicting.len() < 8 && !active.is_empty() {
            let worst = *active
                .iter()
                .max_by(|&&a, &&b| eq_norm(&eqs[a].1, &x).total_cmp(&eq_norm(&eqs[b].1, &x)))
                .expect("hay activas");
            conflicting.push(eqs[worst].0);
            active.retain(|&k| k != worst);
            let sub: Vec<(usize, Eq3)> = active.iter().map(|&k| (eqs[k].0, clone_eq(&eqs[k].1))).collect();
            let mut y = start.clone();
            let r = lm(&sub, &mut y, size);
            if r <= tol || active.is_empty() {
                x = y;
                residual = r;
            }
        }
        conflicting.sort_unstable();
        for (p, c) in self.points.iter_mut().zip(x.chunks_exact(3)) {
            (p.x, p.y, p.z) = (c[0], c[1], c[2]);
        }
        // Grados libres: espacio nulo del jacobiano de lo que se cumple
        let kept: Vec<(usize, Eq3)> = eqs.iter().filter(|(i, _)| !conflicting.contains(i)).map(|(i, e)| (*i, clone_eq(e))).collect();
        let jac = jacobian(&kept, &x);
        let n = x.len();
        let (rank, null) = rank_null(&jac, n);
        let dof = (n - rank) as i32;
        let free_pt = |i: usize| -> bool { (0..null.ncols()).any(|c| (0..3).any(|k| null[(3 * i + k, c)].abs() > 1e-6)) };
        let free: Vec<usize> = (0..self.points.len()).filter(|&i| free_pt(i)).collect();
        let free_points: Vec<u32> = free.iter().map(|&i| self.points[i].id).collect();
        let free_entities: Vec<u32> =
            self.entities.iter().filter(|e| e.geometry.point_ids().iter().any(|id| ix.get(id).is_some_and(|i| free.contains(i)))).map(|e| e.id).collect();
        let status = if !conflicting.is_empty() || residual > tol {
            SketchStatus::OverConstrained
        } else if dof > 0 {
            SketchStatus::UnderConstrained
        } else {
            SketchStatus::WellConstrained
        };
        Ok(Report3d { status, dof, residual, conflicting, missing, free_points, free_entities })
    }

    /// Curvas encadenadas por sus extremos (sin la construcción): una lista
    /// por camino, en orden. Error si un punto junta más de dos curvas.
    pub fn chains(&self) -> R<Vec<Vec<Curve>>> {
        let mut items = Vec::new();
        for e in self.entities.iter().filter(|e| !e.construction) {
            let Some((a, b)) = e.geometry.ends() else { continue };
            let curve = match &e.geometry {
                Geometry3d::Line { .. } => Curve::Line(self.point(a)?, self.point(b)?),
                Geometry3d::Arc { mid, .. } => {
                    let (pa, pm, pb) = (self.point(a)?, self.point(*mid)?, self.point(b)?);
                    if circumcenter(pa, pm, pb).is_none() {
                        return Err(Sketch3dError::Invalid(format!("el arco {} tiene los tres puntos alineados", e.id)));
                    }
                    Curve::Arc(pa, pm, pb)
                }
                Geometry3d::Spline { points } => Curve::Spline(points.iter().map(|&q| self.point(q)).collect::<R<_>>()?),
                Geometry3d::Point { .. } => continue,
            };
            items.push((a, b, curve));
        }
        chain_curves(items).map_err(Sketch3dError::Invalid)
    }
}

/// Ordena curvas (extremos por id) en caminos: cada camino arranca en un
/// extremo suelto (o en cualquiera si es cerrado) y da vuelta las curvas que
/// van al revés.
pub fn chain_curves(items: Vec<(u32, u32, Curve)>) -> Result<Vec<Vec<Curve>>, String> {
    let mut at: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, (a, b, _)) in items.iter().enumerate() {
        at.entry(*a).or_default().push(i);
        at.entry(*b).or_default().push(i);
    }
    if at.values().any(|v| v.len() > 2) {
        return Err("un punto junta más de dos curvas: el camino tiene ramas".into());
    }
    let mut used = vec![false; items.len()];
    let mut out = Vec::new();
    // Primero los abiertos (desde un extremo suelto), después los cerrados
    let mut starts: Vec<(u32, usize)> = at.iter().filter(|(_, v)| v.len() == 1).map(|(p, v)| (*p, v[0])).collect();
    starts.sort_unstable();
    starts.extend((0..items.len()).map(|i| (items[i].0, i)));
    for (from, first) in starts {
        if used[first] {
            continue;
        }
        let mut chain = Vec::new();
        let (mut p, mut i) = (from, first);
        loop {
            used[i] = true;
            let (a, b, c) = &items[i];
            let (c, next) = if *a == p { (c.clone(), *b) } else { (reverse(c), *a) };
            chain.push(c);
            p = next;
            match at[&p].iter().find(|&&k| !used[k]) {
                Some(&k) => i = k,
                None => break,
            }
        }
        out.push(chain);
    }
    Ok(out)
}

/// La misma curva recorrida al revés.
pub fn reverse(c: &Curve) -> Curve {
    match c {
        Curve::Line(a, b) => Curve::Line(*b, *a),
        Curve::Arc(a, m, b) => Curve::Arc(*b, *m, *a),
        Curve::Spline(p) => Curve::Spline(p.iter().rev().copied().collect()),
        other => other.clone(),
    }
}

fn clone_eq(e: &Eq3) -> Eq3 {
    match *e {
        Eq3::Same(a, b) => Eq3::Same(a, b),
        Eq3::At(a, p) => Eq3::At(a, p),
        Eq3::Plane(a, o, n) => Eq3::Plane(a, o, n),
        Eq3::Along(a, b, k) => Eq3::Along(a, b, k),
        Eq3::Parallel(q) => Eq3::Parallel(q),
        Eq3::Perp(q) => Eq3::Perp(q),
        Eq3::EqualLen(q) => Eq3::EqualLen(q),
        Eq3::Tangent(s, t) => Eq3::Tangent(s, t),
        Eq3::Mid(m, a, b) => Eq3::Mid(m, a, b),
        Eq3::Dist(a, b, v) => Eq3::Dist(a, b, v),
        Eq3::Angle(q, v) => Eq3::Angle(q, v),
    }
}

fn eq_norm(e: &Eq3, x: &[f64]) -> f64 {
    let mut r = Vec::new();
    e.eval(x, &mut r);
    r.iter().map(|v| v * v).sum::<f64>().sqrt()
}

fn residuals(eqs: &[(usize, Eq3)], x: &[f64]) -> DVector<f64> {
    let mut r = Vec::new();
    for (_, e) in eqs {
        e.eval(x, &mut r);
    }
    DVector::from_vec(r)
}

/// Jacobiano por diferencias centradas (solo las columnas de los puntos de
/// cada ecuación).
fn jacobian(eqs: &[(usize, Eq3)], x: &[f64]) -> DMatrix<f64> {
    let rows: usize = eqs.iter().map(|(_, e)| e.rows()).sum();
    let mut j = DMatrix::zeros(rows, x.len());
    let mut y = x.to_vec();
    let mut row = 0;
    let (mut plus, mut minus) = (Vec::new(), Vec::new());
    for (_, e) in eqs {
        let mut pts = e.points();
        pts.sort_unstable();
        pts.dedup();
        for p in pts {
            for k in 0..3 {
                let col = 3 * p + k;
                let h = 1e-7 * x[col].abs().max(1.0);
                y[col] = x[col] + h;
                plus.clear();
                e.eval(&y, &mut plus);
                y[col] = x[col] - h;
                minus.clear();
                e.eval(&y, &mut minus);
                y[col] = x[col];
                for r in 0..plus.len() {
                    j[(row + r, col)] = (plus[r] - minus[r]) / (2.0 * h);
                }
            }
        }
        row += e.rows();
    }
    j
}

/// Levenberg-Marquardt; devuelve la norma del residuo final.
fn lm(eqs: &[(usize, Eq3)], x: &mut [f64], size: f64) -> f64 {
    let n = x.len();
    let mut r = residuals(eqs, x);
    let mut cost = r.norm();
    let mut lambda = 1e-3;
    let tol = 1e-12 * size;
    for _ in 0..200 {
        if cost < tol || r.is_empty() {
            break;
        }
        let j = jacobian(eqs, x);
        let jt = j.transpose();
        let g = &jt * &r;
        if g.norm() < 1e-16 * size {
            break;
        }
        let jtj = &jt * &j;
        let mut improved = false;
        for _ in 0..12 {
            let mut a = jtj.clone();
            for i in 0..n {
                a[(i, i)] += lambda * (1.0 + jtj[(i, i)]);
            }
            let Some(step) = a.cholesky().map(|c| c.solve(&(-&g))) else {
                lambda *= 10.0;
                continue;
            };
            let trial: Vec<f64> = x.iter().zip(step.iter()).map(|(a, b)| a + b).collect();
            let rt = residuals(eqs, &trial);
            if rt.norm() < cost {
                x.copy_from_slice(&trial);
                r = rt;
                cost = r.norm();
                lambda = (lambda / 10.0).max(1e-12);
                improved = true;
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    cost
}

/// Rango del jacobiano y base de su espacio nulo (columnas).
fn rank_null(j: &DMatrix<f64>, n: usize) -> (usize, DMatrix<f64>) {
    if j.nrows() == 0 {
        return (0, DMatrix::identity(n, n));
    }
    let null = cad_solver::nullspace(j);
    (n - null.ncols(), null)
}

/// Qué nombra del modelo cada restricción (para que el historial lo resuelva).
pub enum ModelRef<'a> {
    Point(&'a PointSpec),
    Plane(&'a PlaneSpec),
}

impl Sketch3d {
    pub fn model_refs(&self) -> Vec<(usize, ModelRef<'_>)> {
        self.constraints
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Constraint3d::Attach { target, .. } => Some((i, ModelRef::Point(target))),
                Constraint3d::OnPlane { plane, .. } => Some((i, ModelRef::Plane(plane))),
                _ => None,
            })
            .collect()
    }
}
