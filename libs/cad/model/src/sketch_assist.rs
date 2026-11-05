//! Ayudas del solver: restricciones que faltan y definir el sketch entero.
//!
//! Las candidatas salen de la geometría (lo que está casi horizontal, casi
//! coincidente, del mismo largo…) y solo quedan las que restringen algo nuevo:
//! cada una se prueba contra los movimientos que el sketch todavía permite (el
//! espacio nulo del jacobiano), que se achica con cada una que se acepta.

use std::collections::{HashMap, HashSet};

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

/// Movimientos que el sketch todavía permite, por bloques: cada grupo de
/// puntos unido por ecuaciones tiene su base (columnas de `basis`, sobre sus
/// variables, ortonormales: una por movimiento que queda), que pierde una
/// columna con cada fila nueva (`basis·basisᵀ` es el proyector). Una ecuación
/// que une dos grupos los junta. Una base densa de todo el sketch eran gigas
/// y minutos con mil entidades; con las columnas de m×m (sin achicar) juntar
/// mil círculos iguales tardaba 35 s.
struct Freedom {
    points: Vec<Point2>,
    /// Bloque de cada variable
    of: Vec<usize>,
    /// Lugar de cada variable dentro de su bloque
    local: Vec<usize>,
    /// Bloques (None: se juntó con otro)
    blocks: Vec<Option<Block>>,
}

#[derive(Clone)]
struct Block {
    /// Variables del bloque, en orden
    vars: Vec<usize>,
    /// Variables × movimientos libres (columnas ortonormales)
    basis: nalgebra::DMatrix<f64>,
    /// Ecuaciones (del solver) que tiene sumadas
    eqs: usize,
    /// Movimientos que le quedan (cada fila nueva quita uno)
    free: usize,
}

/// Fila del jacobiano de una ecuación: (variable, valor), sin repetidas.
type Row = Vec<(usize, f64)>;

impl Freedom {
    fn new(sys: &ConstraintSystem) -> Self {
        Self::reusing(sys, None)
    }

    /// Como `new`, reusando los bloques de `old` que no cambiaron: mismas
    /// variables, mismas ecuaciones y ningún punto movido
    fn reusing(sys: &ConstraintSystem, old: Option<&Freedom>) -> Self {
        let n = sys.num_variables();
        let n_pts = n / 2;
        // Grupos de puntos unidos por las ecuaciones
        let mut parent: Vec<usize> = (0..n_pts).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for c in &sys.constraints {
            let pts: Vec<usize> = c.point_indices().into_iter().filter(|&p| p < n_pts).collect();
            for w in pts.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
        let mut slot: HashMap<usize, usize> = HashMap::new();
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for p in 0..n_pts {
            let r = find(&mut parent, p);
            let k = *slot.entry(r).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[k].extend([p * 2, p * 2 + 1]);
        }
        let mut f = Freedom { points: sys.points.clone(), of: vec![0; n], local: vec![0; n], blocks: Vec::with_capacity(groups.len()) };
        let mut eqs_of: Vec<Vec<&Constraint>> = vec![Vec::new(); groups.len()];
        for c in &sys.constraints {
            if let Some(&p) = c.point_indices().iter().find(|&&p| p < n_pts) {
                let r = find(&mut parent, p);
                eqs_of[slot[&r]].push(c);
            }
        }
        for (k, vars) in groups.into_iter().enumerate() {
            for (i, &v) in vars.iter().enumerate() {
                f.of[v] = k;
                f.local[v] = i;
            }
            let eqs = eqs_of[k].len();
            // El bloque de antes sirve si es el mismo y nada se movió
            let same = old.and_then(|o| {
                let b = o.blocks.get(*o.of.get(vars[0])?)?.as_ref()?;
                let still = vars.iter().step_by(2).all(|&v| {
                    let p = v / 2;
                    o.points.get(p).is_some_and(|q| (q.x() - sys.points[p].x()).abs() <= 1e-12 && (q.y() - sys.points[p].y()).abs() <= 1e-12)
                });
                (b.vars == vars && b.eqs == eqs && still).then(|| b.clone())
            });
            match same {
                Some(b) => f.blocks.push(Some(b)),
                None => {
                    let m = vars.len();
                    f.blocks.push(Some(Block { vars, basis: nalgebra::DMatrix::identity(m, m), eqs: 0, free: m }));
                    let list: Vec<Constraint> = eqs_of[k].iter().map(|c| (*c).clone()).collect();
                    f.absorb(&list);
                }
            }
        }
        f
    }

    /// Filas del jacobiano de unas ecuaciones en la geometría actual.
    fn rows(&self, eqs: &[Constraint]) -> Vec<Row> {
        let n = self.of.len();
        let mut out = Vec::new();
        for c in eqs {
            let mut rows: Vec<Row> = vec![Vec::new(); c.num_equations()];
            for (r, p, dx, dy) in c.jacobian_entries(&self.points) {
                if r < rows.len() && p * 2 + 1 < n {
                    for (v, d) in [(p * 2, dx), (p * 2 + 1, dy)] {
                        match rows[r].iter_mut().find(|e| e.0 == v) {
                            Some(e) => e.1 += d,
                            None => rows[r].push((v, d)),
                        }
                    }
                }
            }
            out.extend(rows);
        }
        out
    }

    /// Los bloques que tocan unas filas, sacados de su lugar y juntos en uno
    /// (sin copiar el bloque si es uno solo: con miles de variables, copiarlo
    /// en cada ecuación era lo que más tardaba).
    fn take_merged(&mut self, rows: &[Row]) -> (Vec<usize>, Block) {
        let mut ids: Vec<usize> = rows.iter().flatten().map(|&(v, _)| self.of[v]).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut parts: Vec<Block> = ids.iter().filter_map(|&k| self.blocks[k].take()).collect();
        if parts.len() == 1 {
            return (ids, parts.pop().unwrap());
        }
        let m: usize = parts.iter().map(|b| b.vars.len()).sum();
        let cols: usize = parts.iter().map(|b| b.basis.ncols()).sum();
        let mut vars = Vec::with_capacity(m);
        let mut basis = nalgebra::DMatrix::zeros(m, cols);
        let (mut at, mut col) = (0, 0);
        for b in &parts {
            let (k, c) = (b.vars.len(), b.basis.ncols());
            basis.view_mut((at, col), (k, c)).copy_from(&b.basis);
            vars.extend_from_slice(&b.vars);
            at += k;
            col += c;
        }
        let eqs = parts.iter().map(|b| b.eqs).sum();
        let free = parts.iter().map(|b| b.free).sum();
        (ids, Block { vars, basis, eqs, free })
    }

    /// Proyecta las filas sobre un bloque; cuántas aportaron algo nuevo.
    fn project_rows(block: &mut Block, rows: &[Row], min: f64) -> usize {
        let local: HashMap<usize, usize> = block.vars.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let mut gained = 0;
        for row in rows {
            let mut dense = nalgebra::DVector::zeros(block.vars.len());
            for &(v, d) in row {
                if let Some(&i) = local.get(&v) {
                    dense[i] += d;
                }
            }
            if project(&mut block.basis, &dense, min) {
                gained += 1;
            }
        }
        gained
    }

    /// Cuántas filas de estas ecuaciones son nuevas (no salen de las que ya
    /// hay): lo que restringen de nuevo es al menos `min` de la fila.
    ///
    /// Sin tocar ni copiar las bases: proyectar una fila tras otra es
    /// Gram-Schmidt sobre `Bᵀ·fila` (con B' = B − B·q·qᵀ, B'ᵀr = Bᵀr − q·qᵀBᵀr).
    fn gain(&self, eqs: &[Constraint], min: f64) -> usize {
        let rows = self.rows(eqs);
        // Bloques tocados y dónde empieza cada uno en el vector junto
        let mut ids: Vec<usize> = rows.iter().flatten().map(|&(v, _)| self.of[v]).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut start: HashMap<usize, usize> = HashMap::new();
        let mut total = 0;
        for &k in &ids {
            start.insert(k, total);
            total += self.blocks[k].as_ref().map_or(0, |b| b.basis.ncols());
        }
        let mut qs: Vec<nalgebra::DVector<f64>> = Vec::new();
        let mut gained = 0;
        for row in &rows {
            let norm = row.iter().map(|e| e.1 * e.1).sum::<f64>().sqrt();
            if norm < 1e-12 {
                continue;
            }
            // Bᵀ·fila: suma de las filas de B de cada variable, pesadas
            let mut m = nalgebra::DVector::zeros(total);
            for &(v, d) in row {
                let k = self.of[v];
                let Some(b) = self.blocks[k].as_ref() else { continue };
                let (off, i) = (start[&k], self.local[v]);
                for j in 0..b.basis.ncols() {
                    m[off + j] += d * b.basis[(i, j)];
                }
            }
            for q in &qs {
                let c = q.dot(&m);
                m.axpy(-c, q, 1.0);
            }
            let mn = m.norm();
            if mn / norm >= min {
                qs.push(m / mn);
                gained += 1;
            }
        }
        gained
    }

    /// Suma las ecuaciones; devuelve cuántas filas nuevas aportaron.
    fn absorb(&mut self, eqs: &[Constraint]) -> usize {
        let rows = self.rows(eqs);
        if rows.iter().all(|r| r.is_empty()) {
            return 0;
        }
        let (ids, mut block) = self.take_merged(&rows);
        let gained = Self::project_rows(&mut block, &rows, 1e-6);
        block.eqs += eqs.len();
        block.free = block.free.saturating_sub(gained);
        // El bloque junto queda en el lugar del primero
        let keep = ids[0];
        for &k in &ids[1..] {
            self.blocks[k] = None;
        }
        for (i, &v) in block.vars.iter().enumerate() {
            self.of[v] = keep;
            self.local[v] = i;
        }
        self.blocks[keep] = Some(block);
        gained
    }

    fn dof(&self) -> usize {
        self.blocks.iter().flatten().map(|b| b.free).sum()
    }
}

/// Quita de la base la dirección que restringe `row`, si es nueva.
fn project(basis: &mut nalgebra::DMatrix<f64>, row: &nalgebra::DVector<f64>, min: f64) -> bool {
    let norm = row.norm();
    if norm < 1e-12 || basis.ncols() == 0 {
        return false;
    }
    let m = basis.tr_mul(row);
    let mn = m.norm();
    if mn / norm < min {
        return false;
    }
    // Reflexión de Householder H (H·q = s·e_último): B·H tiene las mismas
    // columnas ortonormales y la última es la dirección que se quita (s·B·q)
    let mut u = m / mn;
    let f = u.len();
    let s = if u[f - 1] >= 0.0 { -1.0 } else { 1.0 };
    u[f - 1] -= s;
    let un = u.norm();
    if un > 1e-15 {
        u /= un;
        let bu = &*basis * &u;
        basis.ger(-2.0, &bu, &u, 1.0);
    }
    let cols = basis.ncols();
    let taken = std::mem::replace(basis, nalgebra::DMatrix::zeros(0, 0));
    *basis = taken.remove_column(cols - 1);
    true
}

/// Ángulo entre dos direcciones, en grados (0..90 para rectas sin sentido).
fn line_angle(a: P2, b: P2) -> f64 {
    let cross = a[0] * b[1] - a[1] * b[0];
    let dot = a[0] * b[0] + a[1] * b[1];
    cross.abs().atan2(dot.abs()).to_degrees()
}

/// Grupos de elementos casi iguales: `sorted` ya ordenados; cada grupo
/// junta los que `close` acepta contra el primero del grupo. La última y la
/// primera también se prueban (los ángulos dan la vuelta en 180°).
fn clusters(sorted: &[usize], close: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    for &i in sorted {
        match out.last_mut() {
            Some(g) if close(g[0], i) => g.push(i),
            _ => out.push(vec![i]),
        }
    }
    if out.len() > 1 && close(out[out.len() - 1][0], out[0][0]) {
        let last = out.pop().unwrap();
        out[0].extend(last);
    }
    out.into_iter().filter(|g| g.len() > 1).collect()
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
    /// preferencia. Iguales, paralelas y concéntricas van contra la primera de
    /// su grupo (no todos los pares: con cientos de líneas iguales eran cientos
    /// de miles de candidatas, y al aceptarlas encadenan lo mismo).
    fn relation_candidates(&self) -> Vec<Suggestion> {
        use SketchConstraint as S;
        let size = self.extent().max(1e-3);
        let near = (size * 5e-3).max(1e-4);
        let lines = self.assist_lines();
        let rounds = self.assist_rounds();
        let at: HashMap<u32, P2> = self.points.iter().map(|p| (p.id, [p.x, p.y])).collect();
        let pt = |id: u32| at.get(&id).copied();
        let arcs: HashMap<u32, (u32, u32)> = self
            .entities
            .iter()
            .filter_map(|e| match e.geometry {
                Geometry::Arc { start, end, .. } => Some((e.id, (start, end))),
                _ => None,
            })
            .collect();
        let arc_ends = |r: &Round| arcs.get(&r.id).map_or(vec![], |&(a, b)| vec![a, b]);
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
        // Vecinos por rejilla de celdas de `near` (todos contra todos era cuadrático)
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let cell = |q: P2| ((q[0] / near).floor() as i64, (q[1] / near).floor() as i64);
        for (i, &e) in ends.iter().enumerate() {
            if let Some(q) = pt(e) {
                cells.entry(cell(q)).or_default().push(i);
            }
        }
        for (i, &a) in ends.iter().enumerate() {
            let Some(pa) = pt(a) else { continue };
            let (cx, cy) = cell(pa);
            let mut close: Vec<usize> = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy)))
                .flat_map(|c| cells.get(&c).cloned().unwrap_or_default())
                .filter(|&j| j > i)
                .collect();
            close.sort_unstable();
            for j in close {
                let b = ends[j];
                if same.contains(&(a.min(b), a.max(b))) {
                    continue;
                }
                let Some(pb) = pt(b) else { continue };
                let d = dist2(pa, pb);
                if d < near {
                    push(S::Coincident { a, b }, format!("casi coincidentes ({d:.3} mm)"));
                }
            }
        }
        // Casi sobre el origen o alineados con él
        if let Some(o) = self.origin {
            for &p in ends.iter().chain(&centers).filter(|&&p| p != o) {
                let Some(q) = pt(p) else { continue };
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
        let mut axis_aligned: HashSet<u32> = HashSet::new();
        for l in &lines {
            let h = line_angle(l.dir, [1.0, 0.0]);
            if h < ANGLE_TOL {
                push(S::Horizontal { line: l.id }, format!("casi horizontal ({h:.1}°)"));
                axis_aligned.insert(l.id);
            } else if 90.0 - h < ANGLE_TOL {
                push(S::Vertical { line: l.id }, format!("casi vertical ({:.1}°)", 90.0 - h));
                axis_aligned.insert(l.id);
            }
        }
        // Tangencias donde una línea o un arco llega a un arco
        let arc_tangent = |r: &Round, p: u32| -> Option<P2> {
            let (c, q) = (pt(r.center)?, pt(p)?);
            Some([-(q[1] - c[1]), q[0] - c[0]])
        };
        let mut lines_at: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, l) in lines.iter().enumerate() {
            lines_at.entry(l.a).or_default().push(i);
            if l.b != l.a {
                lines_at.entry(l.b).or_default().push(i);
            }
        }
        let mut arcs_at: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, r) in rounds.iter().enumerate().filter(|(_, r)| r.arc) {
            for p in arc_ends(r) {
                arcs_at.entry(p).or_default().push(i);
            }
        }
        for r in rounds.iter().filter(|r| r.arc) {
            for p in arc_ends(r) {
                let Some(t) = arc_tangent(r, p) else { continue };
                for &li in lines_at.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
                    let l = &lines[li];
                    let a = line_angle(l.dir, t);
                    if a < ANGLE_TOL {
                        push(S::Tangent { a: l.id, b: r.id }, format!("casi tangentes ({a:.1}°)"));
                    }
                }
                for &ri in arcs_at.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
                    let r2 = &rounds[ri];
                    if r2.id > r.id
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
        // Perpendiculares: las que se tocan
        for (i, l1) in lines.iter().enumerate() {
            let mut touching: Vec<usize> = [l1.a, l1.b].iter().flat_map(|p| lines_at.get(p).cloned().unwrap_or_default()).filter(|&j| j > i).collect();
            touching.sort_unstable();
            touching.dedup();
            for j in touching {
                let l2 = &lines[j];
                let a = line_angle(l1.dir, l2.dir);
                if 90.0 - a < ANGLE_TOL {
                    push(S::Perpendicular { a: l1.id, b: l2.id }, format!("casi perpendiculares ({:.1}°)", 90.0 - a));
                }
            }
        }
        // Paralelas: contra la primera de su dirección (las casi horizontales o
        // verticales ya tienen la suya, que las deja paralelas)
        let angle = |l: &Line| l.dir[1].atan2(l.dir[0]).to_degrees().rem_euclid(180.0);
        let mut by_angle: Vec<usize> = (0..lines.len()).filter(|&i| !axis_aligned.contains(&lines[i].id)).collect();
        by_angle.sort_by(|&a, &b| angle(&lines[a]).total_cmp(&angle(&lines[b])));
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for group in clusters(&by_angle, |a, b| line_angle(lines[a].dir, lines[b].dir) < ANGLE_TOL) {
            let first = *group.iter().min().unwrap();
            pairs.extend(group.iter().filter(|&&j| j != first).map(|&j| (first, j)));
        }
        pairs.sort_unstable();
        for (i, j) in pairs {
            let a = line_angle(lines[i].dir, lines[j].dir);
            push(S::Parallel { a: lines[i].id, b: lines[j].id }, format!("casi paralelas ({a:.1}°)"));
        }
        // Mismo largo: contra la primera de su grupo
        let mut by_len: Vec<usize> = (0..lines.len()).collect();
        by_len.sort_by(|&a, &b| lines[a].len.total_cmp(&lines[b].len));
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for group in clusters(&by_len, |a, b| (lines[a].len - lines[b].len).abs() < 5e-3 * lines[a].len.max(lines[b].len)) {
            let first = *group.iter().min().unwrap();
            pairs.extend(group.iter().filter(|&&j| j != first).map(|&j| (first, j)));
        }
        pairs.sort_unstable();
        for (i, j) in pairs {
            push(S::Equal { a: lines[i].id, b: lines[j].id }, "casi del mismo largo".into());
        }
        // Círculos y arcos: concéntricos (centros casi juntos) y mismo radio
        let mut pairs: Vec<(usize, usize, bool)> = Vec::new();
        let mut by_x: Vec<usize> = (0..rounds.len()).filter(|&i| pt(rounds[i].center).is_some()).collect();
        let cx = |i: usize| pt(rounds[i].center).map_or(0.0, |c| c[0]);
        by_x.sort_by(|&a, &b| cx(a).total_cmp(&cx(b)));
        for (k, &i) in by_x.iter().enumerate() {
            for &j in &by_x[k + 1..] {
                if cx(j) - cx(i) >= near {
                    break;
                }
                let (r1, r2) = (&rounds[i], &rounds[j]);
                if let (Some(c1), Some(c2)) = (pt(r1.center), pt(r2.center))
                    && r1.center != r2.center
                    && dist2(c1, c2) < near
                {
                    pairs.push((i.min(j), i.max(j), true));
                }
            }
        }
        let mut by_r: Vec<usize> = (0..rounds.len()).collect();
        by_r.sort_by(|&a, &b| rounds[a].radius.total_cmp(&rounds[b].radius));
        for group in clusters(&by_r, |a, b| (rounds[a].radius - rounds[b].radius).abs() < 5e-3 * rounds[a].radius.max(rounds[b].radius)) {
            let first = *group.iter().min().unwrap();
            pairs.extend(group.iter().filter(|&&j| j != first).map(|&j| (first, j, false)));
        }
        pairs.sort_unstable_by_key(|&(i, j, conc)| (i, j, !conc));
        for (i, j, conc) in pairs {
            if conc {
                push(S::Concentric { a: rounds[i].id, b: rounds[j].id }, "casi concéntricos".into());
            } else {
                push(S::Equal { a: rounds[i].id, b: rounds[j].id }, "casi del mismo radio".into());
            }
        }
        // Extremos casi sobre una línea o un círculo (cajas: solo las cercanas)
        let mut by_box: Vec<(usize, [f64; 4])> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let (a, b) = (pt(l.a)?, pt(l.b)?);
                Some((i, [a[0].min(b[0]) - near, a[1].min(b[1]) - near, a[0].max(b[0]) + near, a[1].max(b[1]) + near]))
            })
            .collect();
        by_box.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
        let max_w = by_box.iter().map(|(_, b)| b[2] - b[0]).fold(0.0f64, f64::max);
        for &p in &ends {
            let Some(q) = pt(p) else { continue };
            // Cajas que pueden contener q en x: empiezan entre q.x − ancho máximo y q.x
            let from = by_box.partition_point(|(_, b)| b[0] < q[0] - max_w);
            let mut hits: Vec<usize> = by_box[from..]
                .iter()
                .take_while(|(_, b)| b[0] <= q[0])
                .filter(|(_, b)| q[0] <= b[2] && q[1] >= b[1] && q[1] <= b[3])
                .map(|(i, _)| *i)
                .collect();
            hits.sort_unstable();
            for i in hits {
                let l = &lines[i];
                if l.a == p || l.b == p {
                    continue;
                }
                let Some(a) = pt(l.a) else { continue };
                let t = ((q[0] - a[0]) * l.dir[0] + (q[1] - a[1]) * l.dir[1]) / (l.len * l.len);
                let d = (l.dir[0] * (q[1] - a[1]) - l.dir[1] * (q[0] - a[0])).abs() / l.len;
                if (0.02..=0.98).contains(&t) && d < near {
                    push(S::PointOnLine { point: p, line: l.id }, "casi sobre la línea".into());
                }
            }
            for r in &rounds {
                let Some(c) = pt(r.center) else { continue };
                if (dist2(c, q) - r.radius).abs() < near && !arc_ends(r).contains(&p) {
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

    /// Solo las relaciones casi cumplidas (horizontales, verticales,
    /// coincidencias, tangencias…) que restringen algo, sin cotas: para un
    /// dibujo importado, que sigue con sus medidas libres.
    pub fn detect_relations(&self) -> Result<Vec<SketchConstraint>, SketchError> {
        let mut work = Work::new(self)?;
        work.take(self.relation_candidates(), usize::MAX, 1e-6);
        Ok(work.taken.into_iter().map(|s| s.constraint).collect())
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
            // Una relación que ya se cumple no mueve nada: se suma sin resolver
            let holds = !dim && eqs.iter().all(|c| c.residuals(&self.built.sys.points).iter().all(|r| r.abs() <= 1e-10));
            if dim || holds {
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
                // Si nada se movió (ya se cumplía) alcanza con sumarla; si no, se
                // rearman los bloques que cambiaron
                let still = built.sys.points.len() == self.built.sys.points.len()
                    && built.sys.points.iter().zip(&self.built.sys.points).all(|(a, b)| (a.x() - b.x()).abs() <= 1e-12 && (a.y() - b.y()).abs() <= 1e-12);
                if still {
                    self.freedom.absorb(&eqs);
                } else {
                    self.freedom = Freedom::reusing(&built.sys, Some(&self.freedom));
                }
                self.built = built;
            }
            self.taken.push(s);
            added += 1;
        }
    }
}
