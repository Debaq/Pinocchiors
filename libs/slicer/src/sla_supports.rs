//! Soportes para resina (SLA4 de `PLAN_SLA.md`): puntos automáticos y su geometría.
//!
//! **Puntos**: en las caras que miran hacia abajo más planas que `overhang_angle` (se
//! reparten a una distancia `spacing` entre sí, de abajo hacia arriba), y uno en cada
//! isla que haya quedado sin punto. Los puntos se pueden editar a mano y volver a armar.
//!
//! **Geometría** de cada soporte, de arriba hacia abajo: punta fina que se clava un poco
//! en la pieza, columna vertical y un pie ancho sobre la balsa (o, si abajo hay pieza,
//! otra punta apoyada en ella). La balsa es la envolvente convexa de los pies.
//! Todo sale como triángulos que se suman a la pieza al laminar (las cáscaras se suman).

use serde::{Deserialize, Serialize};

use crate::sla_check::Island;

pub type Tri = [[f32; 3]; 3];

/// Cuánto soporte poner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportDensity {
    Light,
    Medium,
    Heavy,
}

/// Medidas de los soportes (mm y grados).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SupportSettings {
    /// Distancia entre puntos en las caras en voladizo.
    pub spacing: f32,
    /// Las caras que miran hacia abajo con menos de este ángulo sobre la horizontal llevan soporte.
    pub overhang_angle: f32,
    pub tip_diameter: f32,
    pub tip_length: f32,
    /// Cuánto se clava la punta en la pieza.
    pub tip_penetration: f32,
    pub pillar_diameter: f32,
    pub foot_diameter: f32,
    pub foot_height: f32,
    /// Altura de la pieza sobre la balsa.
    pub lift: f32,
    pub raft_thickness: f32,
    /// Cuánto sobresale la balsa de los pies.
    pub raft_margin: f32,
    /// Permitir soportes que se apoyan en la propia pieza.
    pub on_model: bool,
    /// Lados de cada columna.
    pub segments: u32,
}

impl Default for SupportSettings {
    fn default() -> Self {
        Self::preset(SupportDensity::Medium)
    }
}

impl SupportSettings {
    pub fn preset(d: SupportDensity) -> Self {
        let (spacing, tip, pillar) = match d {
            SupportDensity::Light => (3.5, 0.3, 0.8),
            SupportDensity::Medium => (2.5, 0.4, 1.0),
            SupportDensity::Heavy => (1.8, 0.5, 1.4),
        };
        Self {
            spacing,
            overhang_angle: 45.0,
            tip_diameter: tip,
            tip_length: 1.5,
            tip_penetration: 0.2,
            pillar_diameter: pillar,
            foot_diameter: pillar * 2.2,
            foot_height: 0.6,
            lift: 5.0,
            raft_thickness: 1.0,
            raft_margin: 1.5,
            on_model: true,
            segments: 8,
        }
    }

    /// Altura de la cara de abajo de la pieza una vez elevada.
    pub fn model_bottom(&self) -> f32 {
        self.raft_thickness + self.lift
    }
}

/// De dónde salió un punto de soporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointKind {
    Overhang,
    Island,
    Manual,
}

/// Un punto donde la punta toca la pieza (mm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SupportPoint {
    pub pos: [f32; 3],
    pub kind: PointKind,
}

/// Puntos automáticos: caras en voladizo e islas. `adjust` corre o descarta un punto
/// (venteos): `None` lo descarta; a los de voladizo solo se los deja o descarta.
pub fn auto_points(
    model: &[Tri],
    islands: &[Island],
    s: &SupportSettings,
    adjust: impl Fn(&[f32; 3]) -> Option<[f32; 3]>,
) -> Vec<SupportPoint> {
    let cos = s.overhang_angle.to_radians().cos();
    let step = (s.spacing * 0.5).max(0.05);
    let mut cand: Vec<[f32; 3]> = Vec::new();
    for t in model {
        let n = unit_normal(t);
        if -n[2] < cos {
            continue;
        }
        let longest = (0..3).map(|e| dist(&t[e], &t[(e + 1) % 3])).fold(0f32, f32::max);
        let k = (longest / step).ceil().max(1.0) as usize;
        // Grilla por dentro del triángulo (en las aristas el rayo hacia abajo es ambiguo).
        for i in 0..k {
            for j in 0..k - i {
                let (u, v) = ((i as f32 + 1.0 / 3.0) / k as f32, (j as f32 + 1.0 / 3.0) / k as f32);
                cand.push(std::array::from_fn(|c| t[0][c] + u * (t[1][c] - t[0][c]) + v * (t[2][c] - t[0][c])));
            }
        }
    }
    // Lo más bajo primero: es lo que primero se forma en el aire.
    cand.sort_by(|a, b| a[2].total_cmp(&b[2]));

    let mut grid = PointGrid::new(s.spacing);
    let mut points = Vec::new();
    for p in cand {
        if adjust(&p) == Some(p) && !grid.near(&p, s.spacing, s.spacing) {
            grid.insert(p);
            points.push(SupportPoint { pos: p, kind: PointKind::Overhang });
        }
    }
    // Toda isla lleva su punto salvo que ya haya uno casi encima (las chicas no se tocan
    // con un punto a más de una punta de distancia).
    let reach = (s.tip_diameter * 1.5).max(0.5);
    for i in islands {
        let Some(p) = adjust(&i.pos) else { continue };
        if !grid.near(&p, reach, 0.5) {
            grid.insert(p);
            points.push(SupportPoint { pos: p, kind: PointKind::Island });
        }
    }
    points
}

/// Resultado de armar los soportes.
#[derive(Debug, Clone, Default)]
pub struct Supports {
    pub triangles: Vec<Tri>,
    /// Soportes hasta la balsa y apoyados en la pieza.
    pub to_raft: usize,
    pub on_model: usize,
    /// Puntos que no se pudieron poner y por qué.
    pub skipped: Vec<(usize, SkipReason)>,
}

/// Por qué no se puso un soporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Lo de abajo es la misma pieza a menos de 0,5 mm.
    TooClose,
    /// El punto quedó dentro de la pieza.
    Inside,
    /// Abajo hay pieza y no se permiten soportes sobre la pieza.
    OnModelOff,
    /// Abajo hay un agujero de venteo.
    Vent,
    /// El punto está casi sobre la balsa.
    TooLow,
}

/// Arma los soportes de `points` bajo la pieza `model` (ya elevada). `blocked(x, y, z)`
/// dice dónde no se puede apoyar un pie sobre la pieza (agujeros de venteo).
pub fn build(model: &[Tri], points: &[SupportPoint], s: &SupportSettings, blocked: impl Fn(f32, f32, f32) -> bool) -> Supports {
    let seg = s.segments.max(3) as usize;
    let (tip_r, pillar_r, foot_r) = (s.tip_diameter / 2.0, s.pillar_diameter / 2.0, s.foot_diameter / 2.0);
    let raft_top = s.raft_thickness;
    let base_top = raft_top + s.foot_height;
    let grid = ZGrid::new(model, s.spacing.max(1.0));
    let mut out = Supports::default();
    let mut feet: Vec<[f32; 2]> = Vec::new();

    for (idx, sp) in points.iter().enumerate() {
        let [x, y, z] = sp.pos;
        let top = z + s.tip_penetration;
        // Lo primero que hay abajo (sin contar la cara del punto).
        match grid.below(x, y, z - 0.05) {
            // Lo primero de abajo mira hacia abajo: el punto quedó dentro de la pieza.
            Some((_, false)) => {
                out.skipped.push((idx, SkipReason::Inside));
                continue;
            }
            Some((hit, true)) if hit > raft_top => {
                let gap = z - hit;
                let why = if !s.on_model {
                    Some(SkipReason::OnModelOff)
                } else if gap < 0.5 {
                    Some(SkipReason::TooClose)
                } else if blocked(x, y, hit) {
                    Some(SkipReason::Vent)
                } else {
                    None
                };
                if let Some(why) = why {
                    out.skipped.push((idx, why));
                    continue;
                }
                let bottom = hit - s.tip_penetration;
                if gap >= 2.0 * s.tip_length + 0.2 {
                    let (a, b) = (z - s.tip_length, hit + s.tip_length);
                    frustum(&mut out.triangles, [x, y, a], pillar_r, [x, y, top], tip_r, seg);
                    frustum(&mut out.triangles, [x, y, b - 0.05], pillar_r, [x, y, a + 0.05], pillar_r, seg);
                    frustum(&mut out.triangles, [x, y, bottom], tip_r, [x, y, b], pillar_r, seg);
                } else {
                    let mid = (z + hit) / 2.0;
                    let r = pillar_r.min(gap / 4.0).max(tip_r);
                    frustum(&mut out.triangles, [x, y, mid], r, [x, y, top], tip_r, seg);
                    frustum(&mut out.triangles, [x, y, bottom], tip_r, [x, y, mid + 0.02], r, seg);
                }
                out.on_model += 1;
            }
            _ => {
                if z <= base_top + 0.3 {
                    out.skipped.push((idx, SkipReason::TooLow));
                    continue;
                }
                let a = (z - s.tip_length).max(base_top + 0.1);
                frustum(&mut out.triangles, [x, y, a], pillar_r, [x, y, top], tip_r, seg);
                frustum(&mut out.triangles, [x, y, base_top - 0.05], pillar_r, [x, y, a + 0.05], pillar_r, seg);
                frustum(&mut out.triangles, [x, y, raft_top - 0.05], foot_r, [x, y, base_top], pillar_r, seg);
                feet.push([x, y]);
                out.to_raft += 1;
            }
        }
    }

    if !feet.is_empty() && s.raft_thickness > 0.0 {
        let r = foot_r + s.raft_margin;
        let ring: Vec<[f32; 2]> = feet
            .iter()
            .flat_map(|f| {
                (0..16).map(move |i| {
                    let a = i as f32 / 16.0 * std::f32::consts::TAU;
                    [f[0] + r * a.cos(), f[1] + r * a.sin()]
                })
            })
            .collect();
        prism(&mut out.triangles, &convex_hull(ring), 0.0, s.raft_thickness);
    }
    out
}

/// Tronco de cono cerrado entre `c0` (abajo, radio `r0`) y `c1` (arriba, radio `r1`), normales afuera.
pub fn frustum(out: &mut Vec<Tri>, c0: [f32; 3], r0: f32, c1: [f32; 3], r1: f32, seg: usize) {
    let ring = |c: [f32; 3], r: f32, i: usize| {
        let a = (i % seg) as f32 / seg as f32 * std::f32::consts::TAU;
        [c[0] + r * a.cos(), c[1] + r * a.sin(), c[2]]
    };
    for i in 0..seg {
        let (b0, b1, t0, t1) = (ring(c0, r0, i), ring(c0, r0, i + 1), ring(c1, r1, i), ring(c1, r1, i + 1));
        out.push([b0, b1, t1]);
        out.push([b0, t1, t0]);
        out.push([c0, b1, b0]);
        out.push([c1, t0, t1]);
    }
}

/// Prisma vertical sobre un polígono convexo en sentido antihorario.
fn prism(out: &mut Vec<Tri>, poly: &[[f32; 2]], z0: f32, z1: f32) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let p = |i: usize, z: f32| [poly[i % n][0], poly[i % n][1], z];
    for i in 1..n - 1 {
        out.push([p(0, z0), p(i + 1, z0), p(i, z0)]);
        out.push([p(0, z1), p(i, z1), p(i + 1, z1)]);
    }
    for i in 0..n {
        out.push([p(i, z0), p(i + 1, z0), p(i + 1, z1)]);
        out.push([p(i, z0), p(i + 1, z1), p(i, z1)]);
    }
}

/// Envolvente convexa (cadena monótona), en sentido antihorario.
fn convex_hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<[f32; 2]> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

fn unit_normal(t: &Tri) -> [f32; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 { n.map(|c| c / l) } else { [0.0; 3] }
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Puntos en una grilla XY para preguntar «¿hay uno cerca?».
struct PointGrid {
    cell: f32,
    map: std::collections::HashMap<(i32, i32), Vec<[f32; 3]>>,
}

impl PointGrid {
    fn new(cell: f32) -> Self {
        Self { cell: cell.max(0.1), map: Default::default() }
    }
    fn key(&self, x: f32, y: f32) -> (i32, i32) {
        ((x / self.cell).floor() as i32, (y / self.cell).floor() as i32)
    }
    fn insert(&mut self, p: [f32; 3]) {
        let k = self.key(p[0], p[1]);
        self.map.entry(k).or_default().push(p);
    }
    /// Hay un punto a menos de `r` en XY y de `dz` en altura.
    fn near(&self, p: &[f32; 3], r: f32, dz: f32) -> bool {
        let (kx, ky) = self.key(p[0], p[1]);
        let reach = (r / self.cell).ceil() as i32;
        for i in -reach..=reach {
            for j in -reach..=reach {
                if let Some(v) = self.map.get(&(kx + i, ky + j))
                    && v.iter().any(|q| (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < r * r && (q[2] - p[2]).abs() < dz)
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Triángulos repartidos en una grilla XY para tirar rayos verticales.
pub(crate) struct ZGrid<'a> {
    tris: &'a [Tri],
    cell: f32,
    min: [f32; 2],
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl<'a> ZGrid<'a> {
    pub(crate) fn new(tris: &'a [Tri], cell: f32) -> Self {
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for v in tris.iter().flatten() {
            for c in 0..2 {
                min[c] = min[c].min(v[c]);
                max[c] = max[c].max(v[c]);
            }
        }
        if tris.is_empty() {
            return Self { tris, cell, min: [0.0; 2], nx: 0, ny: 0, cells: Vec::new() };
        }
        let nx = (((max[0] - min[0]) / cell).floor() as usize + 1).min(4096);
        let ny = (((max[1] - min[1]) / cell).floor() as usize + 1).min(4096);
        let cell = cell.max((max[0] - min[0]) / nx as f32).max((max[1] - min[1]) / ny as f32);
        let mut cells = vec![Vec::new(); nx * ny];
        for (i, t) in tris.iter().enumerate() {
            let lo = |c: usize| t.iter().map(|v| v[c]).fold(f32::MAX, f32::min);
            let hi = |c: usize| t.iter().map(|v| v[c]).fold(f32::MIN, f32::max);
            let ix = |x: f32| (((x - min[0]) / cell) as usize).min(nx - 1);
            let iy = |y: f32| (((y - min[1]) / cell) as usize).min(ny - 1);
            for gy in iy(lo(1))..=iy(hi(1)) {
                for gx in ix(lo(0))..=ix(hi(0)) {
                    cells[gy * nx + gx].push(i as u32);
                }
            }
        }
        Self { tris, cell, min, nx, ny, cells }
    }

    /// La z más alta de la malla en la vertical `(x, y)` que esté por debajo de `z`, y si
    /// esa cara mira hacia arriba.
    pub(crate) fn below(&self, x: f32, y: f32, z: f32) -> Option<(f32, bool)> {
        let (fx, fy) = ((x - self.min[0]) / self.cell, (y - self.min[1]) / self.cell);
        if self.nx == 0 || fx < 0.0 || fy < 0.0 || fx as usize >= self.nx || fy as usize >= self.ny {
            return None;
        }
        let mut best: Option<(f32, bool)> = None;
        for &i in &self.cells[fy as usize * self.nx + fx as usize] {
            let t = &self.tris[i as usize];
            if let Some(h) = vertical_hit(t, x, y)
                && h < z
                && best.is_none_or(|b| h > b.0)
            {
                best = Some((h, unit_normal(t)[2] > 0.0));
            }
        }
        best
    }
}

/// Altura del triángulo en `(x, y)` si la vertical lo atraviesa.
fn vertical_hit(t: &Tri, x: f32, y: f32) -> Option<f32> {
    let (a, b, c) = (t[0], t[1], t[2]);
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < 1e-12 {
        return None;
    }
    let l0 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
    let l1 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
    let l2 = 1.0 - l0 - l1;
    let e = -1e-6;
    (l0 >= e && l1 >= e && l2 >= e).then(|| l0 * a[2] + l1 * b[2] + l2 * c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;

    #[test]
    fn cara_de_abajo_lleva_grilla_de_puntos() {
        let s = SupportSettings::default();
        let model = cube([10.0, 10.0, 6.0], [10.0, 10.0, 5.0]);
        let pts = auto_points(&model, &[], &s, |p| Some(*p));
        // 10 × 10 mm a 2,5 mm: entre 12 y 36 puntos, todos en la cara de abajo, y ningún
        // lugar de la cara a más de 1,5 × la distancia de un punto.
        assert!((12..=36).contains(&pts.len()), "{}", pts.len());
        for i in 0..=20 {
            for j in 0..=20 {
                let (x, y) = (10.0 + i as f32 * 0.5, 10.0 + j as f32 * 0.5);
                let d = pts.iter().map(|p| ((p.pos[0] - x).powi(2) + (p.pos[1] - y).powi(2)).sqrt()).fold(f32::MAX, f32::min);
                assert!(d <= s.spacing * 1.5, "({x}, {y}) a {d} mm");
            }
        }
        assert!(pts.iter().all(|p| (p.pos[2] - 6.0).abs() < 1e-4));
        for (i, a) in pts.iter().enumerate() {
            for b in &pts[i + 1..] {
                assert!(((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt() >= s.spacing - 1e-4);
            }
        }
    }

    #[test]
    fn paredes_verticales_no_llevan_puntos() {
        let s = SupportSettings::default();
        let model = cube([10.0, 10.0, 0.0], [10.0, 10.0, 5.0]);
        let pts = auto_points(&model, &[], &s, |p| (p[2] > 0.5).then_some(*p));
        assert!(pts.is_empty());
    }

    #[test]
    fn soporte_sobre_la_pieza_y_hasta_la_balsa() {
        let s = SupportSettings::default();
        // Mesa: tablero arriba (z 10–11) y un bloque abajo (z 6–8) bajo la mitad izquierda.
        let mut model = cube([10.0, 10.0, 10.0], [10.0, 4.0, 1.0]);
        model.extend(cube([10.0, 10.0, 6.0], [4.0, 4.0, 2.0]));
        let points = [
            SupportPoint { pos: [12.0, 12.0, 10.0], kind: PointKind::Manual },
            SupportPoint { pos: [18.0, 12.0, 10.0], kind: PointKind::Manual },
            SupportPoint { pos: [12.0, 12.0, 10.3], kind: PointKind::Manual },
        ];
        let sup = build(&model, &points, &s, |_, _, _| false);
        assert_eq!(sup.on_model, 1);
        assert_eq!(sup.to_raft, 1);
        assert_eq!(sup.skipped, vec![(2, SkipReason::Inside)]);
        let zmin = sup.triangles.iter().flatten().map(|v| v[2]).fold(f32::MAX, f32::min);
        assert!(zmin.abs() < 1e-6);
    }

    #[test]
    fn envolvente() {
        let h = convex_hull(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]]);
        assert_eq!(h.len(), 4);
    }
}
