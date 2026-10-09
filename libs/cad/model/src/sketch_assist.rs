//! Ayudas del solver: restricciones que faltan y definir el sketch entero.
//!
//! Las candidatas salen de la geometría (lo que está casi horizontal, casi
//! coincidente, del mismo largo…) y solo quedan las que restringen algo nuevo:
//! cada una se prueba contra los movimientos que el sketch todavía permite (el
//! espacio nulo del jacobiano), que se achica con cada una que se acepta.

use std::collections::HashSet;

use cad_solver::{Constraint, ConstraintSystem, Point2};
use serde::{Deserialize, Serialize};

use crate::geom::{P2, dist2};
use crate::sketch::{Built, Geometry, Sketch, SketchConstraint, SketchError};

/// Restricción sugerida y por qué.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub constraint: SketchConstraint,
    pub why: String,
}

/// Movimientos que el sketch todavía permite: columnas de `basis` (n_var × k),
/// proyectadas a medida que se suman ecuaciones (nunca se achica la matriz:
/// las columnas abarcan el espacio y `basis·basisᵀ` es su proyector).
struct Freedom {
    basis: nalgebra::DMatrix<f64>,
    points: Vec<Point2>,
}

impl Freedom {
    fn new(sys: &ConstraintSystem) -> Self {
        let n = sys.num_variables();
        let mut f = Freedom { basis: nalgebra::DMatrix::identity(n, n), points: sys.points.clone() };
        for c in &sys.constraints {
            f.absorb(std::slice::from_ref(c));
        }
        f
    }

    /// Filas del jacobiano de unas ecuaciones en la geometría actual.
    fn rows(&self, eqs: &[Constraint]) -> Vec<nalgebra::DVector<f64>> {
        let n = self.basis.nrows();
        let mut out = Vec::new();
        for c in eqs {
            let mut rows = vec![nalgebra::DVector::zeros(n); c.num_equations()];
            for (r, p, dx, dy) in c.jacobian_entries(&self.points) {
                if r < rows.len() && p * 2 + 1 < n {
                    rows[r][p * 2] += dx;
                    rows[r][p * 2 + 1] += dy;
                }
            }
            out.extend(rows);
        }
        out
    }

    /// Cuántas filas de estas ecuaciones son nuevas (no salen de las que ya
    /// hay): lo que restringen de nuevo es al menos `min` de la fila.
    fn gain(&self, eqs: &[Constraint], min: f64) -> usize {
        let mut basis = self.basis.clone();
        let mut gained = 0;
        for row in self.rows(eqs) {
            if project(&mut basis, &row, min) {
                gained += 1;
            }
        }
        gained
    }

    /// Suma las ecuaciones; devuelve cuántas filas nuevas aportaron.
    fn absorb(&mut self, eqs: &[Constraint]) -> usize {
        let mut gained = 0;
        for row in self.rows(eqs) {
            if project(&mut self.basis, &row, 1e-6) {
                gained += 1;
            }
        }
        gained
    }

    fn dof(&self) -> usize {
        let svd = self.basis.clone().svd(false, false);
        svd.singular_values.iter().filter(|&&v| v > 1e-6).count()
    }
}

/// Quita de la base la dirección que restringe `row`, si es nueva.
fn project(basis: &mut nalgebra::DMatrix<f64>, row: &nalgebra::DVector<f64>, min: f64) -> bool {
    let norm = row.norm();
    if norm < 1e-12 || basis.ncols() == 0 {
        return false;
    }
    let m = basis.transpose() * row;
    let mn = m.norm();
    if mn / norm < min {
        return false;
    }
    let q = m / mn;
    let nq = &*basis * &q;
    *basis -= nq * q.transpose();
    true
}

/// Ángulo entre dos direcciones, en grados (0..90 para rectas sin sentido).
fn line_angle(a: P2, b: P2) -> f64 {
    let cross = a[0] * b[1] - a[1] * b[0];
    let dot = a[0] * b[0] + a[1] * b[1];
    cross.abs().atan2(dot.abs()).to_degrees()
}

const ANGLE_TOL: f64 = 2.0;
/// Parte de la fila de una cota que tiene que ser nueva para tomarla de entrada.
const WELL: f64 = 0.2;

struct Line {
    id: u32,
    a: u32,
    b: u32,
    dir: P2,
    len: f64,
}

struct Round {
    id: u32,
    center: u32,
    radius: f64,
    arc: bool,
}

impl Sketch {
    /// Entidades que puede tocar la ayuda: ni textos ni aristas usadas.
    fn assist_entities(&self) -> Vec<&crate::sketch::SketchEntity> {
        let skip: HashSet<u32> = self.texts.iter().flat_map(|t| t.entities.iter().copied()).chain(self.uses.iter().map(|u| u.entity)).collect();
        self.entities.iter().filter(|e| !skip.contains(&e.id)).collect()
    }

    fn assist_lines(&self) -> Vec<Line> {
        self.assist_entities()
            .into_iter()
            .filter_map(|e| match e.geometry {
                Geometry::Line { start, end } => {
                    let (a, b) = (self.point(start).ok()?, self.point(end).ok()?);
                    let dir = [b[0] - a[0], b[1] - a[1]];
                    let len = dir[0].hypot(dir[1]);
                    (len > 1e-9).then_some(Line { id: e.id, a: start, b: end, dir, len })
                }
                _ => None,
            })
            .collect()
    }

    fn assist_rounds(&self) -> Vec<Round> {
        self.assist_entities()
            .into_iter()
            .filter_map(|e| match e.geometry {
                Geometry::Circle { center, .. } | Geometry::Arc { center, .. } => {
                    Some(Round { id: e.id, center, radius: self.radius(e.id).ok()?, arc: matches!(e.geometry, Geometry::Arc { .. }) })
                }
                _ => None,
            })
            .collect()
    }

    /// Relaciones casi cumplidas (las candidatas, sin filtrar), en orden de
    /// preferencia.
    fn relation_candidates(&self) -> Vec<Suggestion> {
        use SketchConstraint as S;
        let size = self.extent().max(1e-3);
        let near = (size * 5e-3).max(1e-4);
        let lines = self.assist_lines();
        let rounds = self.assist_rounds();
        let mut out: Vec<Suggestion> = Vec::new();
        let mut push = |c: SketchConstraint, why: String| out.push(Suggestion { constraint: c, why });

        // Extremos sueltos casi encima de otro
        let centers: HashSet<u32> = rounds.iter().map(|r| r.center).collect();
        let mut ends: Vec<u32> = Vec::new();
        for e in self.assist_entities() {
            match &e.geometry {
                Geometry::Point { point } => ends.push(*point),
                g => {
                    if let Some((a, b)) = g.ends() {
                        ends.extend([a, b]);
                    }
                }
            }
        }
        ends.sort_unstable();
        ends.dedup();
        // Los dos extremos de una misma curva no se juntan (la anularía)
        let same: HashSet<(u32, u32)> = self.entities.iter().filter_map(|e| e.geometry.ends()).map(|(a, b)| (a.min(b), a.max(b))).collect();
        for (i, &a) in ends.iter().enumerate() {
            for &b in &ends[i + 1..] {
                if same.contains(&(a.min(b), a.max(b))) {
                    continue;
                }
                let (Ok(pa), Ok(pb)) = (self.point(a), self.point(b)) else { continue };
                let d = dist2(pa, pb);
                if d < near {
                    push(S::Coincident { a, b }, format!("casi coincidentes ({d:.3} mm)"));
                }
            }
        }
        // Casi sobre el origen o alineados con él
        if let Some(o) = self.origin {
            for &p in ends.iter().chain(&centers).filter(|&&p| p != o) {
                let Ok(q) = self.point(p) else { continue };
                if q[0].hypot(q[1]) < near {
                    push(S::Coincident { a: o, b: p }, "casi en el origen".into());
                } else if q[0].abs() < near {
                    push(S::VerticalPoints { a: o, b: p }, "casi alineado con el origen".into());
                } else if q[1].abs() < near {
                    push(S::HorizontalPoints { a: o, b: p }, "casi alineado con el origen".into());
                }
            }
        }
        // Casi horizontales o verticales
        for l in &lines {
            let h = line_angle(l.dir, [1.0, 0.0]);
            if h < ANGLE_TOL {
                push(S::Horizontal { line: l.id }, format!("casi horizontal ({h:.1}°)"));
            } else if 90.0 - h < ANGLE_TOL {
                push(S::Vertical { line: l.id }, format!("casi vertical ({:.1}°)", 90.0 - h));
            }
        }
        // Tangencias donde una línea o un arco llega a un arco
        let arc_tangent = |r: &Round, p: u32| -> Option<P2> {
            let (c, q) = (self.point(r.center).ok()?, self.point(p).ok()?);
            Some([-(q[1] - c[1]), q[0] - c[0]])
        };
        let arc_ends = |r: &Round| match self.entity(r.id).map(|e| &e.geometry) {
            Ok(Geometry::Arc { start, end, .. }) => vec![*start, *end],
            _ => vec![],
        };
        for r in rounds.iter().filter(|r| r.arc) {
            for p in arc_ends(r) {
                let Some(t) = arc_tangent(r, p) else { continue };
                for l in lines.iter().filter(|l| l.a == p || l.b == p) {
                    let a = line_angle(l.dir, t);
                    if a < ANGLE_TOL {
                        push(S::Tangent { a: l.id, b: r.id }, format!("casi tangentes ({a:.1}°)"));
                    }
                }
                for r2 in rounds.iter().filter(|r2| r2.arc && r2.id > r.id) {
                    if arc_ends(r2).contains(&p)
                        && let Some(t2) = arc_tangent(r2, p)
                    {
                        let a = line_angle(t, t2);
                        if a < ANGLE_TOL {
                            push(S::Tangent { a: r.id, b: r2.id }, format!("casi tangentes ({a:.1}°)"));
                        }
                    }
                }
            }
        }
        // Entre líneas: perpendiculares si se tocan, paralelas, mismo largo
        for (i, l1) in lines.iter().enumerate() {
            for l2 in &lines[i + 1..] {
                let a = line_angle(l1.dir, l2.dir);
                let touch = [l1.a, l1.b].iter().any(|p| *p == l2.a || *p == l2.b);
                if touch && 90.0 - a < ANGLE_TOL {
                    push(S::Perpendicular { a: l1.id, b: l2.id }, format!("casi perpendiculares ({:.1}°)", 90.0 - a));
                }
                if a < ANGLE_TOL {
                    push(S::Parallel { a: l1.id, b: l2.id }, format!("casi paralelas ({a:.1}°)"));
                }
            }
        }
        for (i, l1) in lines.iter().enumerate() {
            for l2 in &lines[i + 1..] {
                if (l1.len - l2.len).abs() < 5e-3 * l1.len.max(l2.len) {
                    push(S::Equal { a: l1.id, b: l2.id }, "casi del mismo largo".into());
                }
            }
        }
        for (i, r1) in rounds.iter().enumerate() {
            for r2 in &rounds[i + 1..] {
                if let (Ok(c1), Ok(c2)) = (self.point(r1.center), self.point(r2.center))
                    && r1.center != r2.center
                    && dist2(c1, c2) < near
                {
                    push(S::Concentric { a: r1.id, b: r2.id }, "casi concéntricos".into());
                }
                if (r1.radius - r2.radius).abs() < 5e-3 * r1.radius.max(r2.radius) {
                    push(S::Equal { a: r1.id, b: r2.id }, "casi del mismo radio".into());
                }
            }
        }
        // Extremos casi sobre una línea o un círculo
        for &p in &ends {
            let Ok(q) = self.point(p) else { continue };
            for l in lines.iter().filter(|l| l.a != p && l.b != p) {
                let Ok(a) = self.point(l.a) else { continue };
                let t = ((q[0] - a[0]) * l.dir[0] + (q[1] - a[1]) * l.dir[1]) / (l.len * l.len);
                let d = (l.dir[0] * (q[1] - a[1]) - l.dir[1] * (q[0] - a[0])).abs() / l.len;
                if (0.02..=0.98).contains(&t) && d < near {
                    push(S::PointOnLine { point: p, line: l.id }, "casi sobre la línea".into());
                }
            }
            for r in &rounds {
                let Ok(c) = self.point(r.center) else { continue };
                if !arc_ends(r).contains(&p) && (dist2(c, q) - r.radius).abs() < near {
                    push(S::PointOnCircle { point: p, circle: r.id }, "casi sobre el círculo".into());
                }
            }
        }
        out
    }

    /// Cotas candidatas en orden: tamaños (radios, largos, semiejes) y después
    /// la posición de cada punto desde el origen (o desde el primero, fijo).
    fn dimension_candidates(&self, positions: bool) -> Vec<Suggestion> {
        use SketchConstraint as S;
        let mut out = Vec::new();
        let opts = Default::default;
        for r in self.assist_rounds() {
            let c = if r.arc {
                S::Radius { entity: r.id, value: 0.0, reference: false, opts: opts() }
            } else {
                S::Diameter { entity: r.id, value: 0.0, reference: false, opts: opts() }
            };
            out.push(Suggestion { constraint: c, why: if r.arc { "falta el radio".into() } else { "falta el diámetro".into() } });
        }
        for l in self.assist_lines() {
            if !self.entity(l.id).is_ok_and(|e| e.infinite) {
                out.push(Suggestion { constraint: S::Length { line: l.id, value: 0.0, reference: false, opts: opts() }, why: "falta el largo".into() });
            }
        }
        for e in self.assist_entities() {
            if let Geometry::Ellipse { center, major, minor } | Geometry::EllipseArc { center, major, minor, .. } = e.geometry {
                for (p, why) in [(major, "falta el semieje mayor"), (minor, "falta el semieje menor")] {
                    out.push(Suggestion { constraint: S::Distance { a: center, b: p, value: 0.0, reference: false, opts: opts() }, why: why.into() });
                }
            }
        }
        if positions {
            let mut seen = HashSet::new();
            let pts: Vec<u32> = self.assist_entities().iter().flat_map(|e| e.geometry.point_ids()).filter(|p| seen.insert(*p)).collect();
            let base = match self.origin {
                Some(o) => o,
                None => match pts.first() {
                    Some(&p) => {
                        let q = self.point(p).unwrap_or([0.0, 0.0]);
                        out.push(Suggestion { constraint: S::Fixed { point: p, x: q[0], y: q[1] }, why: "falta ubicarlo".into() });
                        p
                    }
                    None => return out,
                },
            };
            for p in pts.into_iter().filter(|&p| p != base) {
                for c in [
                    S::HorizontalDistance { a: base, b: p, value: 0.0, reference: false, opts: opts() },
                    S::VerticalDistance { a: base, b: p, value: 0.0, reference: false, opts: opts() },
                ] {
                    out.push(Suggestion { constraint: c, why: "falta ubicarlo".into() });
                }
            }
        }
        // Valen lo que mide la geometría (redondeado: así se relee igual)
        for s in &mut out {
            if let Some(v) = self.measure(&s.constraint) {
                s.constraint.set_value((v * 1e6).round() / 1e6);
            }
        }
        out
    }

    /// Restricciones que faltan: relaciones casi cumplidas y, después, las
    /// cotas de tamaño de lo que sigue libre. Solo las que restringen algo.
    pub fn suggest(&self) -> Result<Vec<Suggestion>, SketchError> {
        let mut work = Work::new(self)?;
        work.take(self.relation_candidates(), 40, 1e-6);
        let left = 40usize.saturating_sub(work.taken.len());
        work.take(self.dimension_candidates(false), left, WELL);
        Ok(work.taken)
    }

    /// Definir completamente: relaciones casi cumplidas (si `relations`),
    /// tamaños y la posición de los puntos desde el origen, hasta que no quede
    /// nada libre (o no haya más candidatas). Las cotas valen lo que mide la
    /// geometría: sin relaciones nuevas, nada se mueve.
    pub fn auto_define(&self, relations: bool) -> Result<Vec<SketchConstraint>, SketchError> {
        let mut work = Work::new(self)?;
        if relations {
            work.take(self.relation_candidates(), usize::MAX, 1e-6);
        }
        // Primero las cotas bien condicionadas; lo que quede, con cualquiera
        for min in [WELL, 1e-6] {
            if work.freedom.dof() > 0 {
                work.take(self.dimension_candidates(true), usize::MAX, min);
            }
        }
        Ok(work.taken.into_iter().map(|s| s.constraint).collect())
    }
}

/// Copia del sketch con lo aceptado hasta ahora. Las relaciones se resuelven
/// al aceptarlas: la dependencia entre ecuaciones se mide sobre la geometría
/// que las cumple (dos líneas casi horizontales no son paralelas «gratis»
/// hasta que son horizontales de verdad).
struct Work {
    sketch: Sketch,
    built: Built,
    freedom: Freedom,
    taken: Vec<Suggestion>,
}

impl Work {
    fn new(s: &Sketch) -> Result<Self, SketchError> {
        let built = s.build()?;
        let freedom = Freedom::new(&built.sys);
        Ok(Work { sketch: s.clone(), built, freedom, taken: Vec::new() })
    }

    /// Acepta las candidatas que restringen algo nuevo, hasta `limit`. Las
    /// cotas tienen que aportar su ecuación entera; a las relaciones les basta
    /// con una.
    ///
    /// Con `min` alto solo pasan las cotas bien condicionadas: un largo y la
    /// distancia horizontal de una línea casi horizontal definen su altura,
    /// pero un redondeo mínimo la mueve mucho (y puede quedar del otro lado).
    fn take(&mut self, candidates: Vec<Suggestion>, limit: usize, min: f64) {
        let mut added = 0;
        for mut s in candidates {
            if added >= limit {
                break;
            }
            if self.sketch.constraints.contains(&s.constraint) {
                continue;
            }
            let dim = s.constraint.value().is_some();
            if dim && let Some(v) = self.sketch.measure(&s.constraint) {
                s.constraint.set_value((v * 1e6).round() / 1e6);
            }
            let index = &self.built.index;
            let ix = |id: u32| index.get(&id).copied().ok_or(SketchError::NoPoint(id));
            let Ok(eqs) = self.sketch.lower(&s.constraint, &ix, &self.built.rims, &self.built.sys.points) else { continue };
            let rows: usize = eqs.iter().map(|c| c.num_equations()).sum();
            let gain = self.freedom.gain(&eqs, if dim { min } else { 1e-6 });
            if gain == 0 || (dim && gain < rows) {
                continue;
            }
            if dim {
                self.freedom.absorb(&eqs);
                self.sketch.constraints.push(s.constraint.clone());
            } else {
                // La relación mueve un poco la geometría: resolver y medir de nuevo
                let before = self.sketch.clone();
                self.sketch.constraints.push(s.constraint.clone());
                let rebuilt = if self.sketch.solve_quiet() { self.sketch.build().ok() } else { None };
                let Some(built) = rebuilt else {
                    self.sketch = before;
                    continue;
                };
                self.freedom = Freedom::new(&built.sys);
                self.built = built;
            }
            self.taken.push(s);
            added += 1;
        }
    }
}
