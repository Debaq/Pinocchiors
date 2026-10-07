//! Desviación entre un escaneo y el sólido diseñado: para cada triángulo del
//! escaneo, la distancia con signo de su centro a la superficie del sólido
//! (positiva afuera, negativa adentro), con un árbol de cajas sobre la
//! teselación del sólido. Sirve para ver en color dónde el diseño se aparta.

use cad_model::P3;
use serde::Serialize;

use crate::mesh::ScanMesh;

/// Resumen de la desviación (mm), pesado por área del escaneo.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DeviationStats {
    /// Media con signo (sesgo: + el escaneo queda afuera del sólido)
    pub mean: f64,
    pub mean_abs: f64,
    pub rms: f64,
    /// Percentil 95 de la distancia absoluta
    pub p95: f64,
    pub max_abs: f64,
    /// Fracción del área dentro de ±`tolerance`
    pub within: f64,
    pub tolerance: f64,
}

#[derive(Debug, Clone)]
pub struct Deviation {
    /// Por triángulo del escaneo (mismo orden que la malla de entrada)
    pub per_face: Vec<f64>,
    pub stats: DeviationStats,
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn add_scaled(a: P3, b: P3, s: f64) -> P3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

/// Punto del triángulo abc más cercano a p (Ericson, "Real-Time Collision Detection").
fn closest_on_triangle(p: P3, a: P3, b: P3, c: P3) -> P3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add_scaled(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add_scaled(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add_scaled(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    add_scaled(add_scaled(a, ab, vb * denom), ac, vc * denom)
}

struct Node {
    min: P3,
    max: P3,
    /// Hoja: rango en `order`; interno: hijos
    kind: NodeKind,
}

enum NodeKind {
    Leaf(usize, usize),
    Inner(usize, usize),
}

/// Árbol de cajas sobre triángulos, para el más cercano a un punto.
struct Bvh<'a> {
    pos: &'a [P3],
    tris: &'a [[u32; 3]],
    order: Vec<usize>,
    nodes: Vec<Node>,
}

impl<'a> Bvh<'a> {
    fn new(pos: &'a [P3], tris: &'a [[u32; 3]]) -> Self {
        let centroids: Vec<P3> = tris
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| pos[i as usize]);
                [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]
            })
            .collect();
        let mut bvh = Bvh { pos, tris, order: (0..tris.len()).collect(), nodes: Vec::new() };
        if !tris.is_empty() {
            bvh.build(0, tris.len(), &centroids);
        }
        bvh
    }

    fn bounds(&self, from: usize, to: usize) -> (P3, P3) {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for &t in &self.order[from..to] {
            for &i in &self.tris[t] {
                let p = self.pos[i as usize];
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
        }
        (min, max)
    }

    fn build(&mut self, from: usize, to: usize, centroids: &[P3]) -> usize {
        let (min, max) = self.bounds(from, to);
        let id = self.nodes.len();
        self.nodes.push(Node { min, max, kind: NodeKind::Leaf(from, to) });
        if to - from <= 8 {
            return id;
        }
        // Partir por la mediana del eje más largo
        let ext = sub(max, min);
        let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] { 0 } else if ext[1] >= ext[2] { 1 } else { 2 };
        let mid = (from + to) / 2;
        self.order[from..to].select_nth_unstable_by(mid - from, |&a, &b| centroids[a][axis].total_cmp(&centroids[b][axis]));
        let l = self.build(from, mid, centroids);
        let r = self.build(mid, to, centroids);
        self.nodes[id].kind = NodeKind::Inner(l, r);
        id
    }

    fn box_dist2(p: P3, min: P3, max: P3) -> f64 {
        (0..3).map(|k| (min[k] - p[k]).max(0.0).max(p[k] - max[k])).map(|d| d * d).sum()
    }

    /// Triángulo más cercano y su punto más cercano.
    fn nearest(&self, p: P3) -> Option<(usize, P3)> {
        let mut best: Option<(f64, usize, P3)> = None;
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if best.is_some_and(|b| Self::box_dist2(p, node.min, node.max) > b.0) {
                continue;
            }
            match node.kind {
                NodeKind::Leaf(a, b) => {
                    for &t in &self.order[a..b] {
                        let [x, y, z] = self.tris[t].map(|i| self.pos[i as usize]);
                        let q = closest_on_triangle(p, x, y, z);
                        let d = dot(sub(p, q), sub(p, q));
                        if best.is_none_or(|b| d < b.0) {
                            best = Some((d, t, q));
                        }
                    }
                }
                NodeKind::Inner(l, r) => {
                    // Primero el hijo más cercano
                    let (dl, dr) = (
                        Self::box_dist2(p, self.nodes[l].min, self.nodes[l].max),
                        Self::box_dist2(p, self.nodes[r].min, self.nodes[r].max),
                    );
                    if dl < dr {
                        stack.push(r);
                        stack.push(l);
                    } else {
                        stack.push(l);
                        stack.push(r);
                    }
                }
            }
        }
        best.map(|(_, t, q)| (t, q))
    }
}

/// Desviación del escaneo respecto del sólido teselado (`positions`, `normals`
/// por vértice y `triangles`), en las mismas unidades (mm).
pub fn deviation(scan: &ScanMesh, positions: &[P3], normals: &[P3], triangles: &[[u32; 3]], tolerance: f64) -> Deviation {
    let bvh = Bvh::new(positions, triangles);
    let per_face: Vec<f64> = scan
        .face_centroids
        .iter()
        .map(|&c| {
            let Some((t, q)) = bvh.nearest(c) else { return 0.0 };
            let v = sub(c, q);
            let d = dot(v, v).sqrt();
            // Signo por la normal del sólido ahí (promedio de las de sus vértices)
            let n = triangles[t].iter().fold([0.0; 3], |acc, &i| add_scaled(acc, normals[i as usize], 1.0));
            if dot(v, n) < 0.0 { -d } else { d }
        })
        .collect();
    let stats = stats(&per_face, &scan.face_areas, tolerance);
    Deviation { per_face, stats }
}

fn stats(values: &[f64], weights: &[f64], tolerance: f64) -> DeviationStats {
    let total: f64 = weights.iter().sum();
    if values.is_empty() || total <= 0.0 {
        return DeviationStats { tolerance, ..Default::default() };
    }
    let w = |i: usize| weights[i] / total;
    let mean = (0..values.len()).map(|i| values[i] * w(i)).sum();
    let mean_abs = (0..values.len()).map(|i| values[i].abs() * w(i)).sum();
    let rms = (0..values.len()).map(|i| values[i] * values[i] * w(i)).sum::<f64>().sqrt();
    let max_abs = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let within = (0..values.len()).filter(|&i| values[i].abs() <= tolerance).map(w).sum();
    // Percentil 95 pesado por área
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| values[a].abs().total_cmp(&values[b].abs()));
    let mut acc = 0.0;
    let mut p95 = max_abs;
    for i in idx {
        acc += w(i);
        if acc >= 0.95 {
            p95 = values[i].abs();
            break;
        }
    }
    DeviationStats { mean, mean_abs, rms, p95, max_abs, within, tolerance }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cubo de lado `s` desde el origen, con normales hacia afuera por vértice de cara.
    fn cube(s: f64, offset: f64) -> (Vec<P3>, Vec<P3>, Vec<[u32; 3]>) {
        let mut pos = Vec::new();
        let mut nor = Vec::new();
        let mut tris = Vec::new();
        let o = offset;
        // Cada cara: 4 vértices propios (normales planas)
        let faces: [([f64; 3], [P3; 4]); 6] = [
            ([0.0, 0.0, -1.0], [[0.0, 0.0, 0.0], [0.0, s, 0.0], [s, s, 0.0], [s, 0.0, 0.0]]),
            ([0.0, 0.0, 1.0], [[0.0, 0.0, s], [s, 0.0, s], [s, s, s], [0.0, s, s]]),
            ([0.0, -1.0, 0.0], [[0.0, 0.0, 0.0], [s, 0.0, 0.0], [s, 0.0, s], [0.0, 0.0, s]]),
            ([0.0, 1.0, 0.0], [[0.0, s, 0.0], [0.0, s, s], [s, s, s], [s, s, 0.0]]),
            ([-1.0, 0.0, 0.0], [[0.0, 0.0, 0.0], [0.0, 0.0, s], [0.0, s, s], [0.0, s, 0.0]]),
            ([1.0, 0.0, 0.0], [[s, 0.0, 0.0], [s, s, 0.0], [s, s, s], [s, 0.0, s]]),
        ];
        for (n, quad) in faces {
            let base = pos.len() as u32;
            for q in quad {
                pos.push([q[0] + o * n[0] - o * 0.0, q[1] + o * n[1], q[2] + o * n[2]]);
                nor.push(n);
            }
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);
        }
        (pos, nor, tris)
    }

    #[test]
    fn same_shape_is_zero_and_offset_is_measured() {
        let (pos, nor, tris) = cube(10.0, 0.0);
        let scan = ScanMesh::new(&pos, &tris);
        let d = deviation(&scan, &pos, &nor, &tris, 0.1);
        assert!(d.stats.max_abs < 1e-9, "{:?}", d.stats);
        assert!((d.stats.within - 1.0).abs() < 1e-12);
        // Escaneo inflado 0,5 mm hacia afuera en cada cara: +0,5 en todas
        let (spos, _, stris) = cube(10.0, 0.5);
        let scan = ScanMesh::new(&spos, &stris);
        let d = deviation(&scan, &pos, &nor, &tris, 0.1);
        assert!((d.stats.mean - 0.5).abs() < 1e-9, "{:?}", d.stats);
        assert!(d.per_face.iter().all(|v| (v - 0.5).abs() < 1e-9));
        assert_eq!(d.stats.within, 0.0);
        // Hacia adentro: negativa
        let (spos, _, stris) = cube(10.0, -0.25);
        let d = deviation(&ScanMesh::new(&spos, &stris), &pos, &nor, &tris, 0.1);
        assert!((d.stats.mean + 0.25).abs() < 1e-9, "{:?}", d.stats);
    }

    #[test]
    fn bvh_matches_brute_force() {
        let (pos, _, tris) = cube(10.0, 0.0);
        let bvh = Bvh::new(&pos, &tris);
        for p in [[3.0, 4.0, 20.0], [-5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [12.0, -3.0, 7.0]] {
            let (_, q) = bvh.nearest(p).unwrap();
            let brute = tris
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|i| pos[i as usize]);
                    let q = closest_on_triangle(p, a, b, c);
                    dot(sub(p, q), sub(p, q))
                })
                .fold(f64::MAX, f64::min);
            assert!((dot(sub(p, q), sub(p, q)) - brute).abs() < 1e-9);
        }
    }
}
