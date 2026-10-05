//! Contornos 2D → sketch: simplificación, círculos reconocidos y cortes del
//! escaneo por un plano.

use std::collections::HashMap;

use cad_model::geom::{P2, Plane, dist2, dot, sub};
use cad_model::{Sketch, SketchConstraint};
use serde::{Deserialize, Serialize};

use crate::mesh::ScanMesh;
use crate::regions_area;

/// Douglas–Peucker para polilíneas abiertas.
pub fn simplify_open(p: &[P2], tol: f64) -> Vec<P2> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let (a, b) = (p[0], p[p.len() - 1]);
    let (mut worst, mut idx) = (0.0, 0);
    for (i, &q) in p.iter().enumerate().take(p.len() - 1).skip(1) {
        let d = seg_dist(q, a, b);
        if d > worst {
            worst = d;
            idx = i;
        }
    }
    if worst <= tol {
        return vec![a, b];
    }
    let mut left = simplify_open(&p[..=idx], tol);
    let right = simplify_open(&p[idx..], tol);
    left.pop();
    left.extend(right);
    left
}

/// Douglas–Peucker para lazos cerrados (sin repetir el primer punto al final).
pub fn simplify_closed(p: &[P2], tol: f64) -> Vec<P2> {
    if p.len() < 4 {
        return p.to_vec();
    }
    // Partir en los dos puntos más lejanos entre sí
    let far = (1..p.len()).max_by(|&i, &j| dist2(p[0], p[i]).total_cmp(&dist2(p[0], p[j]))).unwrap();
    let mut a: Vec<P2> = p[..=far].to_vec();
    let mut b: Vec<P2> = p[far..].to_vec();
    b.push(p[0]);
    a = simplify_open(&a, tol);
    b = simplify_open(&b, tol);
    a.pop();
    b.pop();
    a.extend(b);
    // El punto de partida queda aunque esté a mitad de un lado: quitar
    // vértices que no se apartan de la recta de sus vecinos.
    loop {
        let n = a.len();
        if n <= 3 {
            break;
        }
        let Some(i) = (0..n).find(|&i| seg_dist(a[i], a[(i + n - 1) % n], a[(i + 1) % n]) <= tol) else {
            break;
        };
        a.remove(i);
    }
    a
}

fn seg_dist(p: P2, a: P2, b: P2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return dist2(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    dist2(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Círculo por mínimos cuadrados (Kåsa): centro, radio y error rms radial.
pub fn fit_circle(p: &[P2]) -> Option<(P2, f64, f64)> {
    if p.len() < 5 {
        return None;
    }
    // x² + y² + D x + E y + F = 0  →  sistema normal 3×3
    let mut m = [[0.0f64; 3]; 3];
    let mut r = [0.0f64; 3];
    for q in p {
        let row = [q[0], q[1], 1.0];
        let rhs = -(q[0] * q[0] + q[1] * q[1]);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            r[i] += row[i] * rhs;
        }
    }
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-300 {
        return None;
    }
    let mut sol = [0.0; 3];
    for k in 0..3 {
        let mut mk = m;
        for i in 0..3 {
            mk[i][k] = r[i];
        }
        sol[k] = det(&mk) / d;
    }
    let c = [-sol[0] / 2.0, -sol[1] / 2.0];
    let rad2 = c[0] * c[0] + c[1] * c[1] - sol[2];
    if rad2 <= 0.0 {
        return None;
    }
    let rad = rad2.sqrt();
    let rms = (p.iter().map(|q| (dist2(*q, c) - rad).powi(2)).sum::<f64>() / p.len() as f64).sqrt();
    Some((c, rad, rms))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutlineOptions {
    /// Tolerancia de simplificación y de reconocimiento de círculos (mm).
    pub tolerance: f64,
    /// Lazos que encajan en un círculo se convierten en círculos.
    pub detect_circles: bool,
    /// Líneas casi horizontales/verticales (±`snap_angle`°) se restringen como tales.
    pub snap_axes: bool,
    pub snap_angle: f64,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        Self { tolerance: 0.1, detect_circles: true, snap_axes: true, snap_angle: 2.0 }
    }
}

/// Arma un sketch con lazos cerrados (coordenadas del plano).
pub fn outline_sketch(loops: &[Vec<P2>], opts: &OutlineOptions) -> Sketch {
    let mut s = Sketch::new();
    for l in loops {
        if l.len() < 3 {
            continue;
        }
        if opts.detect_circles
            && l.len() >= 8
            && let Some((c, r, rms)) = fit_circle(l)
            && rms <= opts.tolerance.max(r * 0.01)
        {
            s.circle(c, r);
            continue;
        }
        let pts = simplify_closed(l, opts.tolerance);
        if pts.len() < 3 {
            continue;
        }
        let lines = s.polyline(&pts);
        if opts.snap_axes {
            let lim = opts.snap_angle.to_radians();
            for (i, &id) in lines.iter().enumerate() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let ang = (b[1] - a[1]).atan2(b[0] - a[0]).abs();
                let h = ang.min(std::f64::consts::PI - ang);
                let v = (ang - std::f64::consts::FRAC_PI_2).abs();
                if h <= lim {
                    s.constrain(SketchConstraint::Horizontal { line: id });
                } else if v <= lim {
                    s.constrain(SketchConstraint::Vertical { line: id });
                }
            }
        }
    }
    s
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub points: Vec<P2>,
    pub closed: bool,
}

/// Corte del escaneo por un plano: polilíneas en coordenadas del plano.
pub fn slice(mesh: &ScanMesh, plane: &Plane) -> Vec<Section> {
    let eps = mesh.diagonal() * 1e-9;
    let d: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|v| {
            let x = dot(sub(*v, plane.origin), plane.normal);
            // Vértices sobre el plano: empujarlos a un lado evita casos degenerados
            if x.abs() < eps { eps } else { x }
        })
        .collect();
    // Cada segmento une dos aristas cortadas; los nodos son aristas de la malla
    let mut segs: Vec<((u32, u32), (u32, u32))> = Vec::new();
    for t in &mesh.triangles {
        let mut cut = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if (d[a as usize] > 0.0) != (d[b as usize] > 0.0) {
                cut.push((a.min(b), a.max(b)));
            }
        }
        if cut.len() == 2 {
            segs.push((cut[0], cut[1]));
        }
    }
    let point = |e: (u32, u32)| {
        let (a, b) = (mesh.vertices[e.0 as usize], mesh.vertices[e.1 as usize]);
        let (da, db) = (d[e.0 as usize], d[e.1 as usize]);
        let t = da / (da - db);
        plane.to_local([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t])
    };
    let mut adj: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        adj.entry(s.0).or_default().push(i);
        adj.entry(s.1).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        // Extender hacia ambos lados
        let mut chain = vec![segs[start].0, segs[start].1];
        for dir in 0..2 {
            loop {
                let end = if dir == 0 { *chain.last().unwrap() } else { chain[0] };
                let Some(&n) = adj[&end].iter().find(|&&i| !used[i]) else { break };
                used[n] = true;
                let other = if segs[n].0 == end { segs[n].1 } else { segs[n].0 };
                if dir == 0 {
                    chain.push(other);
                } else {
                    chain.insert(0, other);
                }
            }
        }
        let closed = chain.len() > 3 && chain[0] == *chain.last().unwrap();
        if closed {
            chain.pop();
        }
        out.push(Section { points: chain.into_iter().map(point).collect(), closed });
    }
    out.sort_by(|a, b| regions_area(&b.points).total_cmp(&regions_area(&a.points)));
    out
}
