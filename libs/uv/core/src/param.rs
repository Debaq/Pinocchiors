//! Parametrización de una carta: LSCM (Lévy 2002) y refinado ARAP (Liu 2008).

use crate::geometry::PolyMesh;
use pinocchio_math::Vector3;
use pinocchio_sparse::SPDMatrix;
use std::collections::HashMap;

/// UV de una carta y su calidad.
pub(crate) struct ChartUv {
    /// UV por vértice de la malla.
    pub vertex_uv: HashMap<usize, [f64; 2]>,
    /// Triángulos invertidos en el plano UV.
    pub flipped: usize,
    /// Estiramiento L2 (Sander 2001) con la carta a escala de área: 1 es
    /// isométrico.
    pub stretch: f64,
    pub area_3d: f64,
    pub area_uv: f64,
}

/// Coordenadas 2D de un triángulo en su propio plano, en sentido antihorario
/// visto desde su normal. `None` si es degenerado.
fn local_coords(a: Vector3, b: Vector3, c: Vector3) -> Option<[[f64; 2]; 3]> {
    let e1 = b - a;
    let len = e1.length();
    let normal = e1.cross(&(c - a)).try_normalize()?;
    if len <= 0.0 {
        return None;
    }
    let x = e1 * (1.0 / len);
    let y = normal.cross(&x);
    let q = [(c - a).dot(&x), (c - a).dot(&y)];
    (q[1] > len * 1e-12).then_some([[0.0, 0.0], [len, 0.0], q])
}

fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

struct Chart {
    vertices: Vec<usize>,
    positions: Vec<Vector3>,
    triangles: Vec<[usize; 3]>,
    /// Coordenadas locales por triángulo (`None` si es degenerado).
    coords: Vec<Option<[[f64; 2]; 3]>>,
}

impl Chart {
    fn new(mesh: &PolyMesh, faces: &[usize]) -> Self {
        let mut index: HashMap<usize, usize> = HashMap::new();
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for &f in faces {
            for tri in mesh.fan(f) {
                triangles.push(tri.map(|v| {
                    *index.entry(v).or_insert_with(|| {
                        vertices.push(v);
                        vertices.len() - 1
                    })
                }));
            }
        }
        let positions: Vec<Vector3> = vertices.iter().map(|&v| mesh.points[v]).collect();
        let coords = triangles
            .iter()
            .map(|t| local_coords(positions[t[0]], positions[t[1]], positions[t[2]]))
            .collect();
        Self { vertices, positions, triangles, coords }
    }

    /// Dos vértices lejanos (dos barridos de "el más lejano").
    fn far_pair(&self) -> (usize, usize) {
        let farthest = |from: usize| {
            (0..self.positions.len())
                .max_by(|&a, &b| {
                    self.positions[from]
                        .distance_squared(&self.positions[a])
                        .total_cmp(&self.positions[from].distance_squared(&self.positions[b]))
                })
                .unwrap_or(from)
        };
        let a = farthest(0);
        (a, farthest(a))
    }
}

/// Proyección al plano de la normal media: último recurso si falla el solver.
fn planar(chart: &Chart) -> Vec<[f64; 2]> {
    let mut normal = Vector3::zero();
    for t in &chart.triangles {
        let p = t.map(|i| chart.positions[i]);
        normal += (p[1] - p[0]).cross(&(p[2] - p[0]));
    }
    let n = normal.try_normalize().unwrap_or(Vector3::unit_z());
    let helper = if n.x().abs() < 0.9 { Vector3::unit_x() } else { Vector3::unit_y() };
    let x = helper.cross(&n).normalize();
    let y = n.cross(&x);
    chart.positions.iter().map(|p| [p.dot(&x), p.dot(&y)]).collect()
}

/// LSCM con dos vértices fijos.
fn lscm(chart: &Chart) -> Option<Vec<[f64; 2]>> {
    let n = chart.positions.len();
    let (pin_a, pin_b) = chart.far_pair();
    if pin_a == pin_b {
        return None;
    }
    let mut pinned: HashMap<usize, [f64; 2]> = HashMap::new();
    pinned.insert(pin_a, [0.0, 0.0]);
    pinned.insert(pin_b, [chart.positions[pin_a].distance(&chart.positions[pin_b]), 0.0]);

    let mut var = vec![usize::MAX; n];
    let mut num_free = 0;
    for (i, slot) in var.iter_mut().enumerate() {
        if !pinned.contains_key(&i) {
            *slot = num_free;
            num_free += 1;
        }
    }
    let size = 2 * num_free;
    let (mut rows, mut cols, mut vals) = (Vec::new(), Vec::new(), Vec::new());
    let mut rhs = vec![0.0; size];

    for (t, coords) in chart.triangles.iter().zip(&chart.coords) {
        let Some(x) = coords else { continue };
        let double_area = cross2(sub2(x[1], x[0]), sub2(x[2], x[0]));
        let weight = 0.5 * double_area;
        // Gradiente de cada función base: perp(arista opuesta) / 2A
        let g: [[f64; 2]; 3] = std::array::from_fn(|j| {
            let e = sub2(x[(j + 2) % 3], x[(j + 1) % 3]);
            [-e[1] / double_area, e[0] / double_area]
        });
        // Cauchy-Riemann: ∇v = perp(∇u), dos filas por triángulo
        for row in [
            [(g[0][1], g[0][0]), (g[1][1], g[1][0]), (g[2][1], g[2][0])],
            [(-g[0][0], g[0][1]), (-g[1][0], g[1][1]), (-g[2][0], g[2][1])],
        ] {
            let mut entries: Vec<(usize, f64)> = Vec::with_capacity(6);
            let mut b = 0.0;
            for (j, &(cu, cv)) in row.iter().enumerate() {
                let vertex = t[j];
                match pinned.get(&vertex) {
                    Some(uv) => b -= cu * uv[0] + cv * uv[1],
                    None => {
                        entries.push((2 * var[vertex], cu));
                        entries.push((2 * var[vertex] + 1, cv));
                    }
                }
            }
            for &(i, ci) in &entries {
                rhs[i] += weight * ci * b;
                for &(k, ck) in &entries {
                    rows.push(i);
                    cols.push(k);
                    vals.push(weight * ci * ck);
                }
            }
        }
    }
    if size == 0 {
        let mut uv = vec![[0.0; 2]; n];
        for (&i, &p) in &pinned {
            uv[i] = p;
        }
        return Some(uv);
    }
    // Regularización mínima: vértices sin triángulos válidos quedan definidos
    let scale = vals.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    for i in 0..size {
        rows.push(i);
        cols.push(i);
        vals.push(scale * 1e-10);
    }
    let matrix = SPDMatrix::from_triplets(size, &rows, &cols, &vals).ok()?;
    let solution = matrix.solve_cg(&rhs, 20 * size + 200, 1e-10).ok()?;

    let uv = (0..n)
        .map(|i| match pinned.get(&i) {
            Some(&p) => p,
            None => [solution[2 * var[i]], solution[2 * var[i] + 1]],
        })
        .collect::<Vec<_>>();
    uv.iter().all(|p| p[0].is_finite() && p[1].is_finite()).then_some(uv)
}

/// Refinado ARAP local/global partiendo de `uv`.
#[allow(clippy::needless_range_loop)] // `k` indexa pesos y vértices del triángulo a la vez
fn arap(chart: &Chart, uv: &mut [[f64; 2]], iterations: usize) {
    let n = uv.len();
    if iterations == 0 || n < 3 {
        return;
    }
    // Pesos cotangentes por triángulo y arista (i, j) opuesta al vértice k
    let weights: Vec<Option<[f64; 3]>> = chart
        .coords
        .iter()
        .map(|c| {
            c.map(|x| {
                std::array::from_fn(|k| {
                    let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                    let (a, b) = (sub2(x[i], x[k]), sub2(x[j], x[k]));
                    let cot = (a[0] * b[0] + a[1] * b[1]) / cross2(a, b).abs().max(1e-300);
                    cot.clamp(1e-3, 1e3)
                })
            })
        })
        .collect();

    let pin = 0;
    let var = |i: usize| if i < pin { i } else { i - 1 };
    let size = n - 1;
    let (mut rows, mut cols, mut vals) = (Vec::new(), Vec::new(), Vec::new());
    for (t, w) in chart.triangles.iter().zip(&weights) {
        let Some(w) = w else { continue };
        for k in 0..3 {
            let (i, j) = (t[(k + 1) % 3], t[(k + 2) % 3]);
            for (a, b) in [(i, j), (j, i)] {
                if a == pin {
                    continue;
                }
                rows.push(var(a));
                cols.push(var(a));
                vals.push(w[k]);
                if b != pin {
                    rows.push(var(a));
                    cols.push(var(b));
                    vals.push(-w[k]);
                }
            }
        }
    }
    for i in 0..size {
        rows.push(i);
        cols.push(i);
        vals.push(1e-12);
    }
    let Ok(matrix) = SPDMatrix::from_triplets(size, &rows, &cols, &vals) else { return };

    for _ in 0..iterations {
        // Paso local: rotación más cercana al jacobiano de cada triángulo
        let rotations: Vec<Option<[f64; 2]>> = chart
            .triangles
            .iter()
            .zip(&chart.coords)
            .zip(&weights)
            .map(|((t, x), w)| {
                let (x, w) = (x.as_ref()?, w.as_ref()?);
                let mut s = [[0.0; 2]; 2];
                for k in 0..3 {
                    let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                    let du = sub2(uv[t[i]], uv[t[j]]);
                    let dx = sub2(x[i], x[j]);
                    for r in 0..2 {
                        for c in 0..2 {
                            s[r][c] += w[k] * du[r] * dx[c];
                        }
                    }
                }
                let angle = (s[1][0] - s[0][1]).atan2(s[0][0] + s[1][1]);
                Some([angle.cos(), angle.sin()])
            })
            .collect();

        // Paso global: L u = Σ w R (x_i − x_j)
        let pinned_uv = uv[pin];
        let mut rhs = [vec![0.0; size], vec![0.0; size]];
        for ((t, x), (w, r)) in chart.triangles.iter().zip(&chart.coords).zip(weights.iter().zip(&rotations)) {
            let (Some(x), Some(w), Some([cos, sin])) = (x, w, r) else { continue };
            for k in 0..3 {
                let (li, lj) = ((k + 1) % 3, (k + 2) % 3);
                let d = sub2(x[li], x[lj]);
                let rd = [cos * d[0] - sin * d[1], sin * d[0] + cos * d[1]];
                for (a, b, sign) in [(t[li], t[lj], 1.0), (t[lj], t[li], -1.0)] {
                    if a == pin {
                        continue;
                    }
                    for c in 0..2 {
                        rhs[c][var(a)] += w[k] * sign * rd[c];
                        if b == pin {
                            rhs[c][var(a)] += w[k] * pinned_uv[c];
                        }
                    }
                }
            }
        }
        let solve = |b: &[f64]| matrix.solve_cg(b, 20 * size + 200, 1e-9).ok();
        let (Some(us), Some(vs)) = (solve(&rhs[0]), solve(&rhs[1])) else { return };
        if us.iter().chain(&vs).any(|x| !x.is_finite()) {
            return;
        }
        for i in 0..n {
            if i != pin {
                uv[i] = [us[var(i)], vs[var(i)]];
            }
        }
    }
}

/// Valores singulares de una matriz 2×2 `[[a, b], [c, d]]`.
fn singular_values(a: f64, b: f64, c: f64, d: f64) -> (f64, f64) {
    let (e, f, g, h) = ((a + d) / 2.0, (a - d) / 2.0, (c + b) / 2.0, (c - b) / 2.0);
    let (q, r) = ((e * e + h * h).sqrt(), (f * f + g * g).sqrt());
    (q + r, (q - r).abs())
}

fn measure(chart: &Chart, uv: &[[f64; 2]]) -> (usize, f64, f64, f64) {
    let mut area_3d = 0.0;
    let mut area_uv = 0.0;
    let mut flipped = 0;
    for (t, x) in chart.triangles.iter().zip(&chart.coords) {
        let Some(x) = x else { continue };
        area_3d += 0.5 * cross2(sub2(x[1], x[0]), sub2(x[2], x[0]));
        let signed = 0.5 * cross2(sub2(uv[t[1]], uv[t[0]]), sub2(uv[t[2]], uv[t[0]]));
        if signed <= 0.0 {
            flipped += 1;
        }
        area_uv += signed.abs();
    }
    let scale = if area_uv > 0.0 { (area_3d / area_uv).sqrt() } else { 0.0 };

    // Σ A (1/σ1² + 1/σ2²) / 2 del mapa textura → superficie
    let mut sum = 0.0;
    for (t, x) in chart.triangles.iter().zip(&chart.coords) {
        let Some(x) = x else { continue };
        let (x1, x2) = (sub2(x[1], x[0]), sub2(x[2], x[0]));
        let (u1, u2) = (sub2(uv[t[1]], uv[t[0]]), sub2(uv[t[2]], uv[t[0]]));
        let det = cross2(x1, x2);
        // J = U X⁻¹
        let inv = [[x2[1] / det, -x2[0] / det], [-x1[1] / det, x1[0] / det]];
        let j = |r: usize, c: usize| scale * (u1[r] * inv[0][c] + u2[r] * inv[1][c]);
        let (s1, s2) = singular_values(j(0, 0), j(0, 1), j(1, 0), j(1, 1));
        let area = 0.5 * det;
        let inv_sq = |s: f64| if s > 1e-6 { 1.0 / (s * s) } else { 1e12 };
        sum += area * 0.5 * (inv_sq(s1) + inv_sq(s2));
    }
    let stretch = if area_3d > 0.0 { (sum / area_3d).sqrt() } else { 1.0 };
    (flipped, stretch, area_3d, area_uv)
}

/// Despliega las caras `faces` (una carta con forma de disco).
pub(crate) fn parametrize(mesh: &PolyMesh, faces: &[usize], arap_iterations: usize) -> ChartUv {
    let chart = Chart::new(mesh, faces);
    let mut uv = lscm(&chart).unwrap_or_else(|| planar(&chart));
    let (flipped_lscm, ..) = measure(&chart, &uv);
    let before = uv.clone();
    arap(&chart, &mut uv, arap_iterations);
    let (mut flipped, mut stretch, area_3d, mut area_uv) = measure(&chart, &uv);
    // ARAP no debe introducir inversiones que LSCM no tenía
    if flipped > flipped_lscm {
        uv = before;
        (flipped, stretch, _, area_uv) = measure(&chart, &uv);
    }
    let vertex_uv = chart.vertices.iter().copied().zip(uv).collect();
    ChartUv { vertex_uv, flipped, stretch, area_3d, area_uv }
}
