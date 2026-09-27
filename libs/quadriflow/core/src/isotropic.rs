//! Remallado isótropo con preservación de rasgos (Botsch y Kobbelt 2004).
//!
//! Deja triángulos casi equiláteros de lado `target`: parte las aristas
//! largas, colapsa las cortas, voltea aristas hacia valencia 6 y relaja los
//! vértices sobre el plano tangente, reproyectándolos sobre la superficie
//! original. Las aristas vivas y los bordes siguen siendo aristas: sus
//! vértices solo se deslizan a lo largo de la línea y las esquinas no se
//! mueven. Así el campo no hereda el muestreo de la entrada (astillas de CAD,
//! abanicos alrededor de agujeros, zonas mucho más densas que otras).

use crate::features::FeatureLines;
use crate::surface::Surface;
use crate::V3;
use pinocchio_spatial::{Bvh, Triangle};
use rayon::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Free,
    /// Sobre una arista viva o un borde
    Line,
    /// Extremo o quiebre de una línea: no se mueve
    Corner,
}

/// Coseno mínimo entre la normal de una cara antes y después de una
/// operación: más abajo, la cara se pliega.
const MIN_NORMAL_COS: f64 = 0.2;

pub(crate) fn remesh(surface: &Surface, target: f64, sharp_angle: Option<f64>, iterations: usize) -> Surface {
    let mut mesh = Mesh::new(surface, sharp_angle);
    let bvh = triangle_bvh(surface);
    let segments: Vec<(V3, V3)> = mesh
        .features
        .iter()
        .map(|&(a, b)| (mesh.pos[a as usize], mesh.pos[b as usize]))
        .collect();
    let lines = FeatureLines::from_segments(segments, target);
    let (low, high) = (0.8 * target, 4.0 / 3.0 * target);
    for _ in 0..iterations {
        mesh.split_long_edges(high);
        mesh.collapse_short_edges(low, high);
        mesh.flip_edges();
        mesh.smooth(&bvh, &lines);
    }
    mesh.into_surface()
}

struct Mesh {
    pos: Vec<V3>,
    kind: Vec<Kind>,
    alive_vertex: Vec<bool>,
    tris: Vec<[u32; 3]>,
    alive: Vec<bool>,
    /// Caras de cada vértice
    vf: Vec<Vec<u32>>,
    /// Aristas vivas y de borde, `(menor, mayor)`
    features: HashSet<(u32, u32)>,
}

fn key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

impl Mesh {
    fn new(surface: &Surface, sharp_angle: Option<f64>) -> Self {
        let n = surface.positions.len();
        let mut vf = vec![Vec::new(); n];
        for (f, t) in surface.triangles.iter().enumerate() {
            for &v in t {
                vf[v as usize].push(f as u32);
            }
        }
        let mut mesh = Self {
            pos: surface.positions.clone(),
            kind: vec![Kind::Free; n],
            alive_vertex: vec![true; n],
            tris: surface.triangles.clone(),
            alive: vec![true; surface.triangles.len()],
            vf,
            features: HashSet::new(),
        };

        let cos_sharp = sharp_angle.map(f64::cos);
        let mut edges: Vec<(u32, u32)> =
            surface.triangles.iter().flat_map(|t| (0..3).map(move |k| key(t[k], t[(k + 1) % 3]))).collect();
        edges.sort_unstable();
        edges.dedup();
        for (a, b) in edges {
            let faces = mesh.edge_faces(a, b);
            let feature = match faces.len() {
                1 => true,
                2 => cos_sharp.is_some_and(|c| {
                    let (n0, n1) = (mesh.normal(faces[0]), mesh.normal(faces[1]));
                    n0.normalize().dot(&n1.normalize()) < c
                }),
                _ => false,
            };
            if feature {
                mesh.features.insert((a, b));
            }
        }

        // Vértices sobre rasgos: línea si tienen dos aristas casi alineadas
        let mut feature_neighbors: Vec<Vec<u32>> = vec![Vec::new(); n];
        for &(a, b) in &mesh.features {
            feature_neighbors[a as usize].push(b);
            feature_neighbors[b as usize].push(a);
        }
        let cos_corner = sharp_angle.unwrap_or(std::f64::consts::FRAC_PI_4).cos();
        for (v, around) in feature_neighbors.iter().enumerate() {
            let p = mesh.pos[v];
            let dirs: Vec<V3> = around
                .iter()
                .filter_map(|&u| (mesh.pos[u as usize] - p).try_normalize(1e-30))
                .collect();
            mesh.kind[v] = match dirs.as_slice() {
                [] => Kind::Free,
                [d0, d1] if (-d0).dot(d1) > cos_corner => Kind::Line,
                _ => Kind::Corner,
            };
        }
        mesh
    }

    fn normal(&self, f: u32) -> V3 {
        let [a, b, c] = self.tris[f as usize].map(|i| self.pos[i as usize]);
        (b - a).cross(&(c - a))
    }

    fn edge_faces(&self, a: u32, b: u32) -> Vec<u32> {
        self.vf[a as usize].iter().copied().filter(|&f| self.tris[f as usize].contains(&b)).collect()
    }

    fn neighbors(&self, v: u32) -> Vec<u32> {
        let mut out: Vec<u32> =
            self.vf[v as usize].iter().flat_map(|&f| self.tris[f as usize]).filter(|&u| u != v).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn valence(&self, v: u32) -> usize {
        self.neighbors(v).len()
    }

    fn is_boundary_vertex(&self, v: u32) -> bool {
        self.neighbors(v).iter().any(|&u| self.edge_faces(v, u).len() == 1)
    }

    fn edges(&self) -> Vec<(u32, u32)> {
        let mut edges: Vec<(u32, u32)> = self
            .tris
            .iter()
            .zip(&self.alive)
            .filter(|(_, a)| **a)
            .flat_map(|(t, _)| (0..3).map(move |k| key(t[k], t[(k + 1) % 3])))
            .collect();
        edges.sort_unstable();
        edges.dedup();
        edges
    }

    fn len(&self, (a, b): (u32, u32)) -> f64 {
        (self.pos[a as usize] - self.pos[b as usize]).norm()
    }

    fn add_face(&mut self, t: [u32; 3]) -> u32 {
        let f = self.tris.len() as u32;
        self.tris.push(t);
        self.alive.push(true);
        for v in t {
            self.vf[v as usize].push(f);
        }
        f
    }

    fn set_face(&mut self, f: u32, t: [u32; 3]) {
        for v in self.tris[f as usize] {
            self.vf[v as usize].retain(|&g| g != f);
        }
        self.tris[f as usize] = t;
        for v in t {
            self.vf[v as usize].push(f);
        }
    }

    fn kill_face(&mut self, f: u32) {
        for v in self.tris[f as usize] {
            self.vf[v as usize].retain(|&g| g != f);
        }
        self.alive[f as usize] = false;
    }

    fn split_long_edges(&mut self, high: f64) {
        use std::cmp::Reverse;
        // La más larga primero; largos no negativos: el orden de sus bits es el numérico
        let mut heap: std::collections::BinaryHeap<(u64, Reverse<(u32, u32)>)> = self
            .edges()
            .into_iter()
            .map(|e| (self.len(e), e))
            .filter(|&(l, _)| l > high)
            .map(|(l, e)| (l.to_bits(), Reverse(e)))
            .collect();
        while let Some((_, Reverse((a, b)))) = heap.pop() {
            let faces = self.edge_faces(a, b);
            if faces.is_empty() {
                continue;
            }
            let m = self.pos.len() as u32;
            self.pos.push((self.pos[a as usize] + self.pos[b as usize]) * 0.5);
            let feature = self.features.remove(&(a, b));
            self.kind.push(if feature { Kind::Line } else { Kind::Free });
            self.alive_vertex.push(true);
            self.vf.push(Vec::new());
            if feature {
                self.features.insert(key(a, m));
                self.features.insert(key(m, b));
            }
            let mut new_edges = vec![key(a, m), key(m, b)];
            for f in faces {
                let t = self.tris[f as usize];
                let k = (0..3).find(|&k| key(t[k], t[(k + 1) % 3]) == (a, b)).expect("arista de la cara");
                let (x, y, z) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                self.set_face(f, [x, m, z]);
                self.add_face([m, y, z]);
                new_edges.push(key(m, z));
            }
            for e in new_edges {
                let l = self.len(e);
                if l > high {
                    heap.push((l.to_bits(), Reverse(e)));
                }
            }
        }
    }

    /// Intenta fundir `a` en `b`. Devuelve si lo hizo.
    fn try_collapse(&mut self, a: u32, b: u32, high: f64) -> bool {
        let feature_edge = self.features.contains(&key(a, b));
        match self.kind[a as usize] {
            Kind::Corner => return false,
            Kind::Line if !feature_edge => return false,
            _ => {}
        }
        // Una arista entre dos líneas distintas no se colapsa (las uniría)
        if self.kind[a as usize] != Kind::Free && self.kind[b as usize] != Kind::Free && !feature_edge {
            return false;
        }
        let shared = self.edge_faces(a, b);
        if shared.is_empty() {
            return false;
        }
        let (na, nb) = (self.neighbors(a), self.neighbors(b));
        if na.len() <= 3 || nb.len() <= 3 {
            return false;
        }
        let mut apex: Vec<u32> = shared
            .iter()
            .map(|&f| *self.tris[f as usize].iter().find(|&&x| x != a && x != b).expect("tercer vértice"))
            .collect();
        apex.sort_unstable();
        let common: Vec<u32> = na.iter().copied().filter(|x| nb.binary_search(x).is_ok()).collect();
        if common != apex {
            return false;
        }

        let target = if self.kind[a as usize] == Kind::Free && self.kind[b as usize] == Kind::Free {
            (self.pos[a as usize] + self.pos[b as usize]) * 0.5
        } else {
            self.pos[b as usize]
        };
        if na.iter().any(|&x| x != b && (self.pos[x as usize] - target).norm() > high) {
            return false;
        }
        // Ninguna cara que queda se pliega ni se degenera
        let affected: Vec<u32> = self.vf[a as usize]
            .iter()
            .chain(&self.vf[b as usize])
            .copied()
            .filter(|f| !shared.contains(f))
            .collect();
        for &f in &affected {
            let before = self.normal(f);
            let [p, q, r] = self.tris[f as usize].map(|i| if i == a || i == b { target } else { self.pos[i as usize] });
            let after = (q - p).cross(&(r - p));
            if after.dot(&before) <= MIN_NORMAL_COS * after.norm() * before.norm() {
                return false;
            }
        }

        for f in shared {
            self.kill_face(f);
        }
        for f in self.vf[a as usize].clone() {
            let t = self.tris[f as usize].map(|i| if i == a { b } else { i });
            self.set_face(f, t);
        }
        let moved: Vec<(u32, u32)> = self.features.iter().copied().filter(|&(x, y)| x == a || y == a).collect();
        for (x, y) in moved {
            self.features.remove(&(x, y));
            let other = if x == a { y } else { x };
            if other != b {
                self.features.insert(key(other, b));
            }
        }
        self.pos[b as usize] = target;
        self.kind[b as usize] = self.kind[b as usize].max(self.kind[a as usize]);
        self.alive_vertex[a as usize] = false;
        true
    }

    fn collapse_short_edges(&mut self, low: f64, high: f64) {
        let mut short: Vec<((u32, u32), f64)> =
            self.edges().into_iter().map(|e| (e, self.len(e))).filter(|&(_, l)| l < low).collect();
        short.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
        for ((a, b), _) in short {
            if !self.alive_vertex[a as usize] || !self.alive_vertex[b as usize] || self.len((a, b)) >= low {
                continue;
            }
            // Se mueve el vértice menos restringido
            let (from, to) = if self.kind[a as usize] <= self.kind[b as usize] { (a, b) } else { (b, a) };
            if !self.try_collapse(from, to, high) {
                self.try_collapse(to, from, high);
            }
        }
    }

    fn flip_edges(&mut self) {
        let boundary: Vec<bool> = (0..self.pos.len() as u32)
            .map(|v| self.alive_vertex[v as usize] && self.is_boundary_vertex(v))
            .collect();
        let target = |v: u32| if boundary[v as usize] { 4 } else { 6 };
        for (a, b) in self.edges() {
            if self.features.contains(&(a, b)) {
                continue;
            }
            let faces = self.edge_faces(a, b);
            let [f, g] = faces[..] else { continue };
            // f recorre a → b; g recorre b → a
            let (f, g) = if (0..3).any(|k| {
                let t = self.tris[f as usize];
                (t[k], t[(k + 1) % 3]) == (a, b)
            }) {
                (f, g)
            } else {
                (g, f)
            };
            let c = *self.tris[f as usize].iter().find(|&&x| x != a && x != b).expect("tercer vértice");
            let d = *self.tris[g as usize].iter().find(|&&x| x != a && x != b).expect("tercer vértice");
            if c == d || self.neighbors(c).binary_search(&d).is_ok() {
                continue;
            }
            let (va, vb, vc, vd) = (self.valence(a), self.valence(b), self.valence(c), self.valence(d));
            if va <= 3 || vb <= 3 {
                continue;
            }
            let dev = |v: usize, t: usize| (v as i64 - t as i64).abs();
            let before = dev(va, target(a)) + dev(vb, target(b)) + dev(vc, target(c)) + dev(vd, target(d));
            let after =
                dev(va - 1, target(a)) + dev(vb - 1, target(b)) + dev(vc + 1, target(c)) + dev(vd + 1, target(d));
            if after >= before {
                continue;
            }
            let reference = self.normal(f) + self.normal(g);
            let (n1, n2) = (self.normal_of([a, d, c]), self.normal_of([d, b, c]));
            let ok = |n: V3| n.dot(&reference) > MIN_NORMAL_COS * n.norm() * reference.norm();
            if !ok(n1) || !ok(n2) {
                continue;
            }
            self.set_face(f, [a, d, c]);
            self.set_face(g, [d, b, c]);
        }
    }

    fn normal_of(&self, t: [u32; 3]) -> V3 {
        let [a, b, c] = t.map(|i| self.pos[i as usize]);
        (b - a).cross(&(c - a))
    }

    /// Relajación tangencial: cada vértice libre va hacia el promedio de sus
    /// vecinos sobre su plano tangente y se proyecta a la superficie; los de
    /// línea, hacia el punto medio de sus dos vecinos de línea, sobre la línea.
    fn smooth(&mut self, bvh: &Bvh, lines: &FeatureLines) {
        let n = self.pos.len();
        let mut normals = vec![V3::zeros(); n];
        for (t, _) in self.tris.iter().zip(&self.alive).filter(|(_, a)| **a) {
            let nf = self.normal_of(*t);
            for &v in t {
                normals[v as usize] += nf;
            }
        }
        let mut feature_neighbors: Vec<Vec<u32>> = vec![Vec::new(); n];
        for &(a, b) in &self.features {
            feature_neighbors[a as usize].push(b);
            feature_neighbors[b as usize].push(a);
        }
        let neighbors: Vec<Vec<u32>> = (0..n as u32)
            .map(|v| if self.alive_vertex[v as usize] { self.neighbors(v) } else { Vec::new() })
            .collect();

        let moved: Vec<V3> = (0..n)
            .into_par_iter()
            .map(|v| {
                let p = self.pos[v];
                if !self.alive_vertex[v] || neighbors[v].is_empty() {
                    return p;
                }
                match self.kind[v] {
                    Kind::Corner => p,
                    Kind::Line => {
                        let [x, y] = feature_neighbors[v][..] else { return p };
                        let (px, py) = (self.pos[x as usize], self.pos[y as usize]);
                        let Some(dir) = (py - px).try_normalize(1e-30) else { return p };
                        let c = (px + py) * 0.5;
                        let q = p + dir * dir.dot(&(c - p));
                        lines.closest(&q).unwrap_or(q)
                    }
                    Kind::Free => {
                        let c = neighbors[v].iter().map(|&u| self.pos[u as usize]).sum::<V3>()
                            / neighbors[v].len() as f64;
                        let nrm = normals[v].try_normalize(1e-30).unwrap_or_else(V3::zeros);
                        let d = c - p;
                        let q = p + d - nrm * nrm.dot(&d);
                        bvh.query_closest(&pinocchio_math::Vector3(q)).map_or(q, |hit| hit.point.0)
                    }
                }
            })
            .collect();

        // Aceptar solo los movimientos que no pliegan caras
        for (v, target) in moved.into_iter().enumerate() {
            if target == self.pos[v] {
                continue;
            }
            let old = self.pos[v];
            let before: Vec<V3> = self.vf[v].iter().map(|&f| self.normal(f)).collect();
            self.pos[v] = target;
            let ok = self.vf[v].iter().zip(&before).all(|(&f, b)| {
                let a = self.normal(f);
                a.dot(b) > MIN_NORMAL_COS * a.norm() * b.norm()
            });
            if !ok {
                self.pos[v] = old;
            }
        }
    }

    fn into_surface(self) -> Surface {
        let mut index = vec![u32::MAX; self.pos.len()];
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for (t, _) in self.tris.iter().zip(&self.alive).filter(|(_, a)| **a) {
            triangles.push(t.map(|v| {
                if index[v as usize] == u32::MAX {
                    index[v as usize] = positions.len() as u32;
                    positions.push(self.pos[v as usize]);
                }
                index[v as usize]
            }));
        }
        Surface { positions, triangles }
    }
}

fn triangle_bvh(surface: &Surface) -> Bvh {
    Bvh::build(
        surface
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| pinocchio_math::Vector3(surface.positions[i as usize]));
                Triangle::new(a, b, c)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cubo [0,1]³ triangulado con astillas: cada cara es un abanico desde una esquina.
    fn cube() -> Surface {
        let positions = (0..8)
            .map(|k| V3::new((k & 1) as f64, (k >> 1 & 1) as f64, (k >> 2 & 1) as f64))
            .collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let triangles = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        Surface { positions, triangles }
    }

    fn edge_lengths(s: &Surface) -> Vec<f64> {
        s.triangles
            .iter()
            .flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3])))
            .map(|(a, b)| (s.positions[a as usize] - s.positions[b as usize]).norm())
            .collect()
    }

    #[test]
    fn cube_becomes_uniform_and_keeps_its_edges() {
        let out = remesh(&cube(), 0.1, Some(std::f64::consts::FRAC_PI_4), 5);
        let lengths = edge_lengths(&out);
        let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
        assert!((0.08..0.13).contains(&mean), "media {mean}");
        let short = lengths.iter().filter(|&&l| l < 0.05).count();
        assert!(short * 100 < lengths.len(), "{short} aristas cortas de {}", lengths.len());
        // Sigue siendo el cubo: todo vértice sobre una cara y las 8 esquinas presentes
        for p in &out.positions {
            let on_face = (0..3).any(|i| p[i].abs() < 1e-9 || (p[i] - 1.0).abs() < 1e-9);
            assert!(on_face, "{p:?}");
            assert!(p.iter().all(|&x| (-1e-9..=1.0 + 1e-9).contains(&x)));
        }
        for k in 0..8 {
            let corner = V3::new((k & 1) as f64, (k >> 1 & 1) as f64, (k >> 2 & 1) as f64);
            assert!(out.positions.iter().any(|p| (p - corner).norm() < 1e-9), "falta la esquina {corner:?}");
        }
        assert!((out.area() - 6.0).abs() < 1e-6, "área {}", out.area());
        assert!(crate::rebuild::is_closed_manifold(&out));
    }
}
