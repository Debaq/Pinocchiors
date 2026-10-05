//! Sketch 2D: puntos, entidades que los comparten y restricciones.
//!
//! Los ids son estables (no se reutilizan): la UI y las referencias de otras
//! operaciones (eje de revolución = una línea del sketch) no se rompen al borrar.
//! Las entidades comparten puntos por id: un rectángulo son 4 puntos y 4 líneas.

use std::collections::HashMap;

use cad_solver::{Constraint, ConstraintSystem, Point2, SolveStatus};
use serde::{Deserialize, Serialize};

use crate::geom::{P2, dist2};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchPoint {
    pub id: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchEntity {
    pub id: u32,
    /// Geometría de construcción: restringe pero no forma perfiles.
    #[serde(default)]
    pub construction: bool,
    pub geometry: Geometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry {
    Line { start: u32, end: u32 },
    Circle { center: u32, radius: f64 },
    /// Arco antihorario (visto desde la normal del plano) de `start` a `end`.
    Arc { center: u32, start: u32, end: u32 },
    /// Spline interpolada por los puntos; `closed` une el último con el primero.
    Spline { points: Vec<u32>, closed: bool },
}

impl Geometry {
    pub fn point_ids(&self) -> Vec<u32> {
        match self {
            Geometry::Line { start, end } => vec![*start, *end],
            Geometry::Circle { center, .. } => vec![*center],
            Geometry::Arc { center, start, end } => vec![*center, *start, *end],
            Geometry::Spline { points, .. } => points.clone(),
        }
    }
}

/// Restricciones de alto nivel (las que ve el usuario). Distancias en mm,
/// ángulos en grados. Las cotas con `reference` no restringen: muestran la
/// medida (se actualiza al resolver), como las cotas entre paréntesis de Onshape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SketchConstraint {
    Coincident { a: u32, b: u32 },
    Fixed { point: u32, x: f64, y: f64 },
    Horizontal { line: u32 },
    Vertical { line: u32 },
    /// Dos puntos a la misma altura (`a.y = b.y`).
    HorizontalPoints { a: u32, b: u32 },
    /// Dos puntos en la misma vertical (`a.x = b.x`).
    VerticalPoints { a: u32, b: u32 },
    Parallel { a: u32, b: u32 },
    Perpendicular { a: u32, b: u32 },
    /// Líneas del mismo largo, o círculos/arcos del mismo radio.
    Equal { a: u32, b: u32 },
    /// Tangencia entre línea y círculo/arco, o entre dos círculos/arcos (por
    /// fuera o por dentro, según cómo estén al resolver).
    Tangent { a: u32, b: u32 },
    /// Círculos o arcos con el mismo centro.
    Concentric { a: u32, b: u32 },
    PointOnLine { point: u32, line: u32 },
    PointOnCircle { point: u32, circle: u32 },
    Midpoint { point: u32, line: u32 },
    Symmetric { a: u32, b: u32, line: u32 },
    Distance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    /// `b.x − a.x = value`
    HorizontalDistance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    /// `b.y − a.y = value`
    VerticalDistance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    Length { line: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    Radius { entity: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    Diameter { entity: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
    Angle { a: u32, b: u32, degrees: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool },
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl SketchConstraint {
    /// Restricción con valor editable (cota).
    pub fn value(&self) -> Option<f64> {
        use SketchConstraint::*;
        match self {
            Distance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. } => Some(*value),
            Angle { degrees, .. } => Some(*degrees),
            _ => None,
        }
    }

    /// Cota de referencia (no restringe).
    pub fn is_reference(&self) -> bool {
        use SketchConstraint::*;
        matches!(
            self,
            Distance { reference: true, .. }
                | HorizontalDistance { reference: true, .. }
                | VerticalDistance { reference: true, .. }
                | Length { reference: true, .. }
                | Radius { reference: true, .. }
                | Diameter { reference: true, .. }
                | Angle { reference: true, .. }
        )
    }

    pub fn set_value(&mut self, v: f64) -> bool {
        use SketchConstraint::*;
        match self {
            Distance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. } => *value = v,
            Angle { degrees, .. } => *degrees = v,
            _ => return false,
        }
        true
    }

    /// Ids (de puntos o entidades) que menciona: para borrar en cascada.
    fn mentions(&self, point: Option<u32>, entity: Option<u32>) -> bool {
        use SketchConstraint::*;
        let p = |id: &u32| Some(*id) == point;
        let e = |id: &u32| Some(*id) == entity;
        match self {
            Coincident { a, b } | HorizontalPoints { a, b } | VerticalPoints { a, b } => p(a) || p(b),
            Fixed { point: q, .. } => p(q),
            Horizontal { line } | Vertical { line } | Length { line, .. } => e(line),
            Parallel { a, b } | Perpendicular { a, b } | Equal { a, b } | Tangent { a, b } | Concentric { a, b } | Angle { a, b, .. } => {
                e(a) || e(b)
            }
            PointOnLine { point: q, line } | Midpoint { point: q, line } => p(q) || e(line),
            PointOnCircle { point: q, circle } => p(q) || e(circle),
            Symmetric { a, b, line } => p(a) || p(b) || e(line),
            Distance { a, b, .. } | HorizontalDistance { a, b, .. } | VerticalDistance { a, b, .. } => p(a) || p(b),
            Radius { entity: x, .. } | Diameter { entity: x, .. } => e(x),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchStatus {
    /// Totalmente definido.
    WellConstrained,
    /// Resuelto pero con grados de libertad.
    UnderConstrained,
    /// Restricciones en conflicto (las de `conflicting`).
    OverConstrained,
    /// El solver no convergió.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveReport {
    pub status: SketchStatus,
    pub dof: i32,
    pub residual: f64,
    /// Índices en `Sketch::constraints` de las restricciones en conflicto.
    pub conflicting: Vec<usize>,
    /// Puntos que todavía pueden moverse.
    pub free_points: Vec<u32>,
    /// Entidades que todavía pueden moverse o cambiar de tamaño (alguno de
    /// sus puntos, o su radio, está libre).
    #[serde(default)]
    pub free_entities: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SketchError {
    #[error("el punto {0} no existe")]
    NoPoint(u32),
    #[error("la entidad {0} no existe")]
    NoEntity(u32),
    #[error("la entidad {0} no es {1}")]
    WrongKind(u32, &'static str),
    #[error("restricción no soportada: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sketch {
    pub points: Vec<SketchPoint>,
    pub entities: Vec<SketchEntity>,
    pub constraints: Vec<SketchConstraint>,
    #[serde(default)]
    pub next_id: u32,
    /// Punto origen del sketch, fijo en (0, 0): se usa como cualquier otro
    /// punto (anclar, acotar) pero no se mueve ni se borra. Los sketches
    /// viejos no lo tienen hasta que se editan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<u32>,
}

impl Sketch {
    pub fn new() -> Self {
        Self::default()
    }

    fn fresh_id(&mut self) -> u32 {
        // Tolerar documentos armados a mano sin next_id
        let max = self.points.iter().map(|p| p.id).chain(self.entities.iter().map(|e| e.id)).max();
        self.next_id = self.next_id.max(max.map_or(0, |m| m + 1));
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // --- Construcción ---

    /// Id del punto origen; lo crea en (0, 0) si el sketch no lo tiene.
    pub fn ensure_origin(&mut self) -> u32 {
        if let Some(id) = self.origin.filter(|id| self.points.iter().any(|p| p.id == *id)) {
            return id;
        }
        let id = self.add_point(0.0, 0.0);
        self.origin = Some(id);
        id
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> u32 {
        let id = self.fresh_id();
        self.points.push(SketchPoint { id, x, y });
        id
    }

    pub fn add_entity(&mut self, geometry: Geometry) -> u32 {
        let id = self.fresh_id();
        self.entities.push(SketchEntity { id, construction: false, geometry });
        id
    }

    pub fn add_line(&mut self, a: u32, b: u32) -> u32 {
        self.add_entity(Geometry::Line { start: a, end: b })
    }

    /// Línea con puntos nuevos.
    pub fn line(&mut self, a: P2, b: P2) -> u32 {
        let (p, q) = (self.add_point(a[0], a[1]), self.add_point(b[0], b[1]));
        self.add_line(p, q)
    }

    pub fn circle(&mut self, center: P2, radius: f64) -> u32 {
        let c = self.add_point(center[0], center[1]);
        self.add_entity(Geometry::Circle { center: c, radius })
    }

    /// Arco antihorario de `start` a `end` alrededor de `center`.
    pub fn arc(&mut self, center: P2, start: P2, end: P2) -> u32 {
        let c = self.add_point(center[0], center[1]);
        let s = self.add_point(start[0], start[1]);
        let e = self.add_point(end[0], end[1]);
        self.add_entity(Geometry::Arc { center: c, start: s, end: e })
    }

    /// Rectángulo con esquinas compartidas y restricciones horizontal/vertical.
    /// Devuelve las 4 líneas (abajo, derecha, arriba, izquierda).
    pub fn rectangle(&mut self, a: P2, b: P2) -> [u32; 4] {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        let p = [
            self.add_point(x0, y0),
            self.add_point(x1, y0),
            self.add_point(x1, y1),
            self.add_point(x0, y1),
        ];
        let l = [
            self.add_line(p[0], p[1]),
            self.add_line(p[1], p[2]),
            self.add_line(p[2], p[3]),
            self.add_line(p[3], p[0]),
        ];
        self.constraints.push(SketchConstraint::Horizontal { line: l[0] });
        self.constraints.push(SketchConstraint::Horizontal { line: l[2] });
        self.constraints.push(SketchConstraint::Vertical { line: l[1] });
        self.constraints.push(SketchConstraint::Vertical { line: l[3] });
        l
    }

    /// Polígono cerrado de líneas (puntos compartidos).
    pub fn polyline(&mut self, pts: &[P2]) -> Vec<u32> {
        let ids: Vec<u32> = pts.iter().map(|p| self.add_point(p[0], p[1])).collect();
        (0..ids.len()).map(|i| self.add_line(ids[i], ids[(i + 1) % ids.len()])).collect()
    }

    pub fn constrain(&mut self, c: SketchConstraint) -> usize {
        self.constraints.push(c);
        self.constraints.len() - 1
    }

    /// Borra una entidad, las restricciones que la nombran y los puntos que
    /// quedan sin usar.
    pub fn remove_entity(&mut self, id: u32) -> Result<(), SketchError> {
        let pos = self.entities.iter().position(|e| e.id == id).ok_or(SketchError::NoEntity(id))?;
        let removed = self.entities.remove(pos);
        self.constraints.retain(|c| !c.mentions(None, Some(id)));
        for p in removed.geometry.point_ids() {
            if Some(p) != self.origin && !self.entities.iter().any(|e| e.geometry.point_ids().contains(&p)) {
                self.points.retain(|q| q.id != p);
                self.constraints.retain(|c| !c.mentions(Some(p), None));
            }
        }
        Ok(())
    }

    // --- Consultas ---

    pub fn point(&self, id: u32) -> Result<P2, SketchError> {
        self.points.iter().find(|p| p.id == id).map(|p| [p.x, p.y]).ok_or(SketchError::NoPoint(id))
    }

    pub fn entity(&self, id: u32) -> Result<&SketchEntity, SketchError> {
        self.entities.iter().find(|e| e.id == id).ok_or(SketchError::NoEntity(id))
    }

    fn line_points(&self, id: u32) -> Result<(u32, u32), SketchError> {
        match self.entity(id)?.geometry {
            Geometry::Line { start, end } => Ok((start, end)),
            _ => Err(SketchError::WrongKind(id, "una línea")),
        }
    }

    /// Radio actual de un círculo o arco.
    pub fn radius(&self, id: u32) -> Result<f64, SketchError> {
        match self.entity(id)?.geometry {
            Geometry::Circle { radius, .. } => Ok(radius),
            Geometry::Arc { center, start, .. } => Ok(dist2(self.point(center)?, self.point(start)?)),
            _ => Err(SketchError::WrongKind(id, "un círculo o arco")),
        }
    }

    // --- Resolver ---

    /// Ajusta los puntos para cumplir las restricciones.
    pub fn solve(&mut self) -> Result<SolveReport, SketchError> {
        self.run_solver(None)
    }

    /// Resuelve con `point` arrastrado hacia `target` (lo más cerca posible
    /// si las restricciones no lo dejan llegar).
    pub fn solve_drag(&mut self, point: u32, target: P2) -> Result<SolveReport, SketchError> {
        self.point(point)?;
        self.run_solver(Some((point, target)))
    }

    fn run_solver(&mut self, drag: Option<(u32, P2)>) -> Result<SolveReport, SketchError> {
        let index: HashMap<u32, usize> = self.points.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        let mut sys = ConstraintSystem::new();
        for p in &self.points {
            sys.add_point(p.x, p.y);
        }
        let ix = |id: u32| index.get(&id).copied().ok_or(SketchError::NoPoint(id));

        // El radio es una incógnita más: cada círculo lleva un punto oculto en
        // su borde, a la derecha del centro (radio = distancia al centro). En
        // los arcos el borde es el inicio.
        let mut rims: HashMap<u32, usize> = HashMap::new();
        for e in &self.entities {
            match e.geometry {
                Geometry::Circle { center, radius } => {
                    let c = ix(center)?;
                    let p = self.point(center)?;
                    let rim = sys.add_point(p[0] + radius.abs().max(1e-9), p[1]);
                    sys.add_constraint(Constraint::Horizontal { p1_idx: c, p2_idx: rim });
                    rims.insert(e.id, rim);
                }
                Geometry::Arc { start, .. } => {
                    rims.insert(e.id, ix(start)?);
                }
                _ => {}
            }
        }
        // Implícitas: los extremos de un arco equidistan del centro
        for e in &self.entities {
            if let Geometry::Arc { center, start, end } = e.geometry {
                let (c, s, t) = (ix(center)?, ix(start)?, ix(end)?);
                sys.add_constraint(Constraint::EqualLength { l1_p1: c, l1_p2: s, l2_p1: c, l2_p2: t });
            }
        }
        if let Some(o) = self.origin {
            sys.add_constraint(Constraint::Fixed { p_idx: ix(o)?, position: Point2::new(0.0, 0.0) });
        }
        // Qué restricción de alto nivel generó cada ecuación del solver
        let implicit = sys.constraints.len();
        let mut origin: Vec<usize> = Vec::new();
        for (ci, c) in self.constraints.iter().enumerate() {
            for low in self.lower(c, &ix, &rims, &sys.points)? {
                sys.add_constraint(low);
                origin.push(ci);
            }
        }

        let result = match drag {
            Some((p, t)) => cad_solver::solve_drag(&mut sys, ix(p)?, Point2::new(t[0], t[1])),
            None => cad_solver::solve(&mut sys, &cad_solver::SolverParams::default()),
        };
        let (status, dof, residual) = match &result {
            Ok(r) => {
                for (p, q) in self.points.iter_mut().zip(&r.points) {
                    p.x = q.x();
                    p.y = q.y();
                }
                for e in &mut self.entities {
                    if let Geometry::Circle { center, radius } = &mut e.geometry
                        && let (Some(&rim), Some(&c)) = (rims.get(&e.id), index.get(center))
                    {
                        *radius = (r.points[rim].co - r.points[c].co).norm();
                    }
                }
                // El origen queda exacto (el solver lo deja a 1e-13)
                if let Some(o) = self.points.iter_mut().find(|p| Some(p.id) == self.origin) {
                    (o.x, o.y) = (0.0, 0.0);
                }
                (r.status, r.dof, r.residual)
            }
            Err(_) => (SolveStatus::NotConverged, 0, f64::INFINITY),
        };

        let mut conflicting = Vec::new();
        let mut free_points = Vec::new();
        let diag = cad_solver::diagnose(&sys);
        if status != SolveStatus::Converged || residual > 1e-6 {
            for &i in diag.minimal_conflict_set.iter().chain(&diag.conflicting) {
                if i >= implicit {
                    let ci = origin[i - implicit];
                    if !conflicting.contains(&ci) {
                        conflicting.push(ci);
                    }
                }
            }
        }
        // Consistente pero con una cota que no agrega nada: también sobra
        // (Onshape la marca igual); la más nueva es la que se ofrece quitar
        let mut redundant_dim = false;
        if conflicting.is_empty()
            && let Some(ci) = diag
                .redundant
                .iter()
                .filter(|&&i| i >= implicit)
                .map(|&i| origin[i - implicit])
                .filter(|&ci| self.constraints[ci].value().is_some() && !self.constraints[ci].is_reference())
                .max()
        {
            conflicting.push(ci);
            redundant_dim = true;
        }
        let free: std::collections::HashSet<usize> = diag.dof_per_point.iter().filter(|(_, d)| *d > 0).map(|(pi, _)| *pi).collect();
        for &pi in &free {
            if pi < self.points.len() {
                free_points.push(self.points[pi].id);
            }
        }
        free_points.sort_unstable();
        let mut free_entities = Vec::new();
        for e in &self.entities {
            let mut idx: Vec<usize> = e.geometry.point_ids().iter().filter_map(|id| index.get(id).copied()).collect();
            idx.extend(rims.get(&e.id));
            if idx.iter().any(|i| free.contains(i)) {
                free_entities.push(e.id);
            }
        }
        // Las cotas de referencia muestran lo que mide el sketch resuelto
        for i in 0..self.constraints.len() {
            if self.constraints[i].is_reference()
                && let Some(v) = self.measure(&self.constraints[i])
            {
                self.constraints[i].set_value(v);
            }
        }
        let status = if redundant_dim || residual > 1e-6 || matches!(status, SolveStatus::OverConstrained) && residual > 1e-9 {
            SketchStatus::OverConstrained
        } else {
            match status {
                SolveStatus::NotConverged => SketchStatus::Failed,
                _ if dof > 0 => SketchStatus::UnderConstrained,
                _ => SketchStatus::WellConstrained,
            }
        };
        Ok(SolveReport { status, dof, residual, conflicting, free_points, free_entities })
    }

    /// Lo que mide una cota en la geometría actual.
    pub fn measure(&self, c: &SketchConstraint) -> Option<f64> {
        use SketchConstraint as S;
        let p = |id: u32| self.point(id).ok();
        let dir = |id: u32| -> Option<P2> {
            let (a, b) = self.line_points(id).ok()?;
            let (a, b) = (p(a)?, p(b)?);
            Some([b[0] - a[0], b[1] - a[1]])
        };
        Some(match *c {
            S::Distance { a, b, .. } => dist2(p(a)?, p(b)?),
            S::HorizontalDistance { a, b, .. } => p(b)?[0] - p(a)?[0],
            S::VerticalDistance { a, b, .. } => p(b)?[1] - p(a)?[1],
            S::Length { line, .. } => {
                let d = dir(line)?;
                d[0].hypot(d[1])
            }
            S::Radius { entity, .. } => self.radius(entity).ok()?,
            S::Diameter { entity, .. } => 2.0 * self.radius(entity).ok()?,
            S::Angle { a, b, .. } => {
                let (d1, d2) = (dir(a)?, dir(b)?);
                (d1[0] * d2[1] - d1[1] * d2[0]).atan2(d1[0] * d2[0] + d1[1] * d2[1]).to_degrees()
            }
            _ => return None,
        })
    }

    /// Traduce una restricción a ecuaciones del solver.
    fn lower(
        &self,
        c: &SketchConstraint,
        ix: &dyn Fn(u32) -> Result<usize, SketchError>,
        rims: &HashMap<u32, usize>,
        at: &[Point2],
    ) -> Result<Vec<Constraint>, SketchError> {
        use SketchConstraint as S;
        if c.is_reference() {
            return Ok(vec![]);
        }
        // Centro y punto de borde (radio variable) de un círculo o arco
        let round = |id: u32| -> Result<(usize, usize), SketchError> {
            match (&self.entity(id)?.geometry, rims.get(&id)) {
                (Geometry::Circle { center, .. } | Geometry::Arc { center, .. }, Some(&rim)) => Ok((ix(*center)?, rim)),
                _ => Err(SketchError::WrongKind(id, "un círculo o arco")),
            }
        };
        let line = |id| -> Result<(usize, usize), SketchError> {
            let (a, b) = self.line_points(id)?;
            Ok((ix(a)?, ix(b)?))
        };
        Ok(match *c {
            S::Coincident { a, b } => vec![Constraint::Coincident { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::Fixed { point, x, y } => vec![Constraint::Fixed { p_idx: ix(point)?, position: Point2::new(x, y) }],
            S::Horizontal { line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Horizontal { p1_idx: a, p2_idx: b }]
            }
            S::Vertical { line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Vertical { p1_idx: a, p2_idx: b }]
            }
            S::HorizontalPoints { a, b } => vec![Constraint::Horizontal { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::VerticalPoints { a, b } => vec![Constraint::Vertical { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::Parallel { a, b } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![Constraint::Parallel { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2 }]
            }
            S::Perpendicular { a, b } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![Constraint::Perpendicular { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2 }]
            }
            S::Angle { a, b, degrees, .. } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![Constraint::Angle { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2, angle_rad: degrees.to_radians() }]
            }
            S::Equal { a, b } => match (&self.entity(a)?.geometry, &self.entity(b)?.geometry) {
                (Geometry::Line { start: a1, end: a2 }, Geometry::Line { start: b1, end: b2 }) => {
                    vec![Constraint::EqualLength { l1_p1: ix(*a1)?, l1_p2: ix(*a2)?, l2_p1: ix(*b1)?, l2_p2: ix(*b2)? }]
                }
                (Geometry::Circle { .. } | Geometry::Arc { .. }, Geometry::Circle { .. } | Geometry::Arc { .. }) => {
                    let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                    vec![Constraint::EqualLength { l1_p1: c1, l1_p2: r1, l2_p1: c2, l2_p2: r2 }]
                }
                _ => return Err(SketchError::Unsupported("igualdad entre entidades de distinto tipo".into())),
            },
            S::Tangent { a, b } => {
                // Dos arcos que comparten un extremo: los centros quedan alineados
                // con el punto de contacto (tangencia en ese punto)
                if let (Geometry::Arc { center: c1, start: s1, end: e1 }, Geometry::Arc { center: c2, start: s2, end: e2 }) =
                    (&self.entity(a)?.geometry, &self.entity(b)?.geometry)
                {
                    let shared = [s1, e1].into_iter().find(|p| *p == s2 || *p == e2).ok_or_else(|| {
                        SketchError::Unsupported("arcos tangentes sin un extremo en común".into())
                    })?;
                    return Ok(vec![Constraint::PointOnLine { p_idx: ix(*c2)?, line_p1: ix(*c1)?, line_p2: ix(*shared)? }]);
                }
                let (l, other) = match (&self.entity(a)?.geometry, &self.entity(b)?.geometry) {
                    (Geometry::Line { .. }, _) => (a, b),
                    (_, Geometry::Line { .. }) => (b, a),
                    _ => {
                        // Dos curvas: por fuera o por dentro, según cómo están ahora
                        let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                        let len = |p: usize, q: usize| (at[q].co - at[p].co).norm();
                        let (d, ra, rb) = (len(c1, c2), len(c1, r1), len(c2, r2));
                        let internal = (d - (ra - rb).abs()).abs() < (d - (ra + rb)).abs();
                        return Ok(vec![Constraint::TangentCircles { c1, rim1: r1, c2, rim2: r2, internal }]);
                    }
                };
                let (l1, l2) = self.line_points(l)?;
                match self.entity(other)?.geometry {
                    Geometry::Circle { .. } => {
                        let (center_idx, rim_idx) = round(other)?;
                        vec![Constraint::TangentLineCircleVar { line_p1: ix(l1)?, line_p2: ix(l2)?, center_idx, rim_idx }]
                    }
                    Geometry::Arc { center, start, end } => {
                        // Si comparten extremo, tangencia exacta: la línea es
                        // perpendicular al radio en ese punto (sin fijar el radio).
                        let shared = [start, end].into_iter().find(|p| *p == l1 || *p == l2);
                        match shared {
                            Some(p) => vec![Constraint::Perpendicular {
                                l1_p1: ix(l1)?,
                                l1_p2: ix(l2)?,
                                l2_p1: ix(center)?,
                                l2_p2: ix(p)?,
                            }],
                            None => vec![Constraint::TangentLineCircleVar {
                                line_p1: ix(l1)?,
                                line_p2: ix(l2)?,
                                center_idx: ix(center)?,
                                rim_idx: ix(start)?,
                            }],
                        }
                    }
                    _ => return Err(SketchError::WrongKind(other, "un círculo o arco")),
                }
            }
            S::PointOnLine { point, line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::PointOnLine { p_idx: ix(point)?, line_p1: a, line_p2: b }]
            }
            S::PointOnCircle { point, circle } => {
                let (c, rim) = round(circle)?;
                vec![Constraint::EqualLength { l1_p1: c, l1_p2: ix(point)?, l2_p1: c, l2_p2: rim }]
            }
            S::Concentric { a, b } => {
                let ((c1, _), (c2, _)) = (round(a)?, round(b)?);
                vec![Constraint::Coincident { p1_idx: c1, p2_idx: c2 }]
            }
            S::Midpoint { point, line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Midpoint { p_idx: ix(point)?, line_p1: a, line_p2: b }]
            }
            S::Symmetric { a, b, line: l } => {
                let (l1, l2) = line(l)?;
                vec![Constraint::Symmetric { p1_idx: ix(a)?, p2_idx: ix(b)?, line_p1: l1, line_p2: l2 }]
            }
            S::Distance { a, b, value, .. } => vec![Constraint::Distance { p1_idx: ix(a)?, p2_idx: ix(b)?, distance: value }],
            S::HorizontalDistance { a, b, value, .. } => {
                vec![Constraint::HorizontalDist { p1_idx: ix(b)?, p2_idx: ix(a)?, distance: value }]
            }
            S::VerticalDistance { a, b, value, .. } => {
                vec![Constraint::VerticalDist { p1_idx: ix(b)?, p2_idx: ix(a)?, distance: value }]
            }
            S::Length { line: l, value, .. } => {
                let (a, b) = line(l)?;
                vec![Constraint::Distance { p1_idx: a, p2_idx: b, distance: value }]
            }
            S::Radius { entity, value, .. } | S::Diameter { entity, value, .. } => {
                let r = if matches!(c, S::Diameter { .. }) { value / 2.0 } else { value };
                let (center, rim) = round(entity)?;
                vec![Constraint::Distance { p1_idx: center, p2_idx: rim, distance: r }]
            }
        })
    }
}
