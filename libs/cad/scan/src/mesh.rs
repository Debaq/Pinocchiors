//! Malla de escaneo con lo que necesitan los algoritmos: normales y áreas por
//! cara, adyacencia por aristas compartidas y vértices soldados.

use std::collections::HashMap;

use cad_model::geom::{P3, add, cross, dot, norm, normalize, scale, sub};

#[derive(Debug, Clone)]
pub struct ScanMesh {
    pub vertices: Vec<P3>,
    /// Mismo orden que la malla de entrada: el índice de triángulo que devuelve
    /// el visor al hacer clic sirve directo como semilla.
    pub triangles: Vec<[u32; 3]>,
    pub face_normals: Vec<P3>,
    pub face_areas: Vec<f64>,
    pub face_centroids: Vec<P3>,
    /// Caras vecinas por arista.
    pub adjacency: Vec<Vec<u32>>,
    pub bbox_min: P3,
    pub bbox_max: P3,
}

impl ScanMesh {
    /// Suelda vértices repetidos (STL trae cada triángulo suelto) y precalcula.
    pub fn new(vertices: &[P3], triangles: &[[u32; 3]]) -> Self {
        let mut bbox_min = [f64::MAX; 3];
        let mut bbox_max = [f64::MIN; 3];
        for v in vertices {
            for k in 0..3 {
                bbox_min[k] = bbox_min[k].min(v[k]);
                bbox_max[k] = bbox_max[k].max(v[k]);
            }
        }
        let diag = norm(sub(bbox_max, bbox_min)).max(1e-9);
        let q = diag * 1e-7;
        let key = |p: P3| ((p[0] / q).round() as i64, (p[1] / q).round() as i64, (p[2] / q).round() as i64);
        let mut welded: Vec<P3> = Vec::new();
        let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let remap: Vec<u32> = vertices
            .iter()
            .map(|&v| {
                *map.entry(key(v)).or_insert_with(|| {
                    welded.push(v);
                    (welded.len() - 1) as u32
                })
            })
            .collect();
        let triangles: Vec<[u32; 3]> = triangles.iter().map(|t| t.map(|i| remap[i as usize])).collect();

        let mut face_normals = Vec::with_capacity(triangles.len());
        let mut face_areas = Vec::with_capacity(triangles.len());
        let mut face_centroids = Vec::with_capacity(triangles.len());
        for t in &triangles {
            let [a, b, c] = t.map(|i| welded[i as usize]);
            let n = cross(sub(b, a), sub(c, a));
            let len = norm(n);
            face_areas.push(len / 2.0);
            face_normals.push(if len > 0.0 { scale(n, 1.0 / len) } else { [0.0, 0.0, 0.0] });
            face_centroids.push(scale(add(add(a, b), c), 1.0 / 3.0));
        }

        let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (f, t) in triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a != b {
                    edge_faces.entry((a.min(b), a.max(b))).or_default().push(f as u32);
                }
            }
        }
        let mut adjacency = vec![Vec::new(); triangles.len()];
        for faces in edge_faces.values() {
            for &f in faces {
                for &g in faces {
                    if f != g && !adjacency[f as usize].contains(&g) {
                        adjacency[f as usize].push(g);
                    }
                }
            }
        }
        ScanMesh { vertices: welded, triangles, face_normals, face_areas, face_centroids, adjacency, bbox_min, bbox_max }
    }

    pub fn diagonal(&self) -> f64 {
        norm(sub(self.bbox_max, self.bbox_min))
    }

    pub fn face_count(&self) -> usize {
        self.triangles.len()
    }

    /// Puntos de muestra de un conjunto de caras (vértices + centroides) con la
    /// normal de su cara, a lo sumo `max` (submuestreo uniforme).
    pub fn sample_faces(&self, faces: &[u32], max: usize) -> (Vec<P3>, Vec<P3>) {
        let mut pts = Vec::new();
        let mut nrm = Vec::new();
        for &f in faces {
            let n = self.face_normals[f as usize];
            pts.push(self.face_centroids[f as usize]);
            nrm.push(n);
            for &v in &self.triangles[f as usize] {
                pts.push(self.vertices[v as usize]);
                nrm.push(n);
            }
        }
        if pts.len() > max {
            let step = pts.len() as f64 / max as f64;
            let idx: Vec<usize> = (0..max).map(|i| (i as f64 * step) as usize).collect();
            pts = idx.iter().map(|&i| pts[i]).collect();
            nrm = idx.iter().map(|&i| nrm[i]).collect();
        }
        (pts, nrm)
    }

    /// Primer impacto de un rayo (Möller–Trumbore), distancia > `min_t`.
    pub fn raycast(&self, origin: P3, dir: P3, min_t: f64) -> Option<(f64, u32)> {
        self.raycast_filtered(origin, dir, min_t, |_| true)
    }

    /// Primer punto donde un rayo que va por dentro del material sale de él
    /// (cara con normal hacia donde va el rayo).
    pub fn raycast_exit(&self, origin: P3, dir: P3, min_t: f64) -> Option<(f64, u32)> {
        let d = normalize(dir);
        self.raycast_filtered(origin, dir, min_t, |f| dot(self.face_normals[f], d) > 0.0)
    }

    fn raycast_filtered(&self, origin: P3, dir: P3, min_t: f64, accept: impl Fn(usize) -> bool) -> Option<(f64, u32)> {
        let d = normalize(dir);
        let mut best: Option<(f64, u32)> = None;
        for (f, t) in self.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|i| self.vertices[i as usize]);
            let e1 = sub(b, a);
            let e2 = sub(c, a);
            let p = cross(d, e2);
            let det = dot(e1, p);
            if det.abs() < 1e-14 {
                continue;
            }
            let inv = 1.0 / det;
            let s = sub(origin, a);
            let u = dot(s, p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = cross(s, e1);
            let v = dot(d, q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = dot(e2, q) * inv;
            if t > min_t && best.is_none_or(|(bt, _)| t < bt) && accept(f) {
                best = Some((t, f as u32));
            }
        }
        best
    }
}
