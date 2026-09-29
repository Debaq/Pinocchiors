//! Malla de polígonos con la geometría y vecindad por caras que usan el
//! desplegado y el horneado.

use pinocchio_math::Vector3;
use std::collections::HashMap;

/// Malla de polígonos (triángulos, quads o mezcla) con datos por cara.
pub(crate) struct PolyMesh {
    pub points: Vec<Vector3>,
    pub faces: Vec<Vec<usize>>,
    /// Normal unitaria (cero si la cara es degenerada).
    pub normals: Vec<Vector3>,
    pub areas: Vec<f64>,
    pub centroids: Vec<Vector3>,
    /// Por cara y arista local `k` (de `v[k]` a `v[k+1]`): la cara al otro
    /// lado si la arista es de exactamente dos caras.
    pub adjacent: Vec<Vec<Option<usize>>>,
    /// Longitud de cada arista local.
    pub edge_lengths: Vec<Vec<f64>>,
    /// Arista local sobre una costura vieja: las dos caras caen en islas
    /// distintas del mapa original (ver [`PolyMesh::with_regions`]).
    pub old_seams: Vec<Vec<bool>>,
}

impl PolyMesh {
    pub fn new<F: AsRef<[usize]>>(positions: &[[f64; 3]], faces: &[F]) -> Self {
        let points: Vec<Vector3> = positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
        let faces: Vec<Vec<usize>> = faces.iter().map(|f| f.as_ref().to_vec()).collect();

        let mut normals = Vec::with_capacity(faces.len());
        let mut areas = Vec::with_capacity(faces.len());
        let mut centroids = Vec::with_capacity(faces.len());
        let mut edge_lengths = Vec::with_capacity(faces.len());
        for face in &faces {
            // Normal de Newell: robusta para polígonos no planos
            let mut newell = Vector3::zero();
            let mut centroid = Vector3::zero();
            let mut lengths = Vec::with_capacity(face.len());
            for k in 0..face.len() {
                let (a, b) = (points[face[k]], points[face[(k + 1) % face.len()]]);
                newell += a.cross(&b);
                centroid += a;
                lengths.push(a.distance(&b));
            }
            let area = 0.5 * newell.length();
            normals.push(newell.try_normalize().unwrap_or(Vector3::zero()));
            areas.push(area);
            centroids.push(centroid * (1.0 / face.len().max(1) as f64));
            edge_lengths.push(lengths);
        }

        let mut edges: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
        for (f, face) in faces.iter().enumerate() {
            for k in 0..face.len() {
                let (a, b) = (face[k], face[(k + 1) % face.len()]);
                edges.entry((a.min(b), a.max(b))).or_default().push((f, k));
            }
        }
        let mut adjacent: Vec<Vec<Option<usize>>> = faces.iter().map(|f| vec![None; f.len()]).collect();
        for users in edges.values() {
            if let [(f, k), (g, l)] = users[..]
                && f != g
            {
                adjacent[f][k] = Some(g);
                adjacent[g][l] = Some(f);
            }
        }

        let old_seams = faces.iter().map(|f| vec![false; f.len()]).collect();
        Self { points, faces, normals, areas, centroids, adjacent, edge_lengths, old_seams }
    }

    /// Marca como costura vieja cada arista entre caras de regiones
    /// distintas (`regions[f]`: la isla del original bajo la cara `f`).
    pub fn with_regions(mut self, regions: &[usize]) -> Self {
        if regions.len() == self.num_faces() {
            for (f, seams) in self.old_seams.iter_mut().enumerate() {
                for (k, seam) in seams.iter_mut().enumerate() {
                    *seam = self.adjacent[f][k].is_some_and(|g| regions[g] != regions[f]);
                }
            }
        }
        self
    }

    /// Largo de la arista local `k` de `f` como costura: las que caen sobre
    /// una costura vieja pesan `1 / (1 + weight)`.
    pub fn seam_length(&self, f: usize, k: usize, weight: f64) -> f64 {
        let len = self.edge_lengths[f][k];
        if self.old_seams[f][k] { len / (1.0 + weight) } else { len }
    }

    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }

    /// Caras vecinas de `f` con la longitud de la arista compartida.
    pub fn neighbors(&self, f: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        self.adjacent[f]
            .iter()
            .zip(&self.edge_lengths[f])
            .filter_map(|(g, &len)| g.map(|g| (g, len)))
    }

    /// Triángulos en abanico de la cara `f` (los mismos que al exportar).
    pub fn fan(&self, f: usize) -> impl Iterator<Item = [usize; 3]> + '_ {
        let face = &self.faces[f];
        (1..face.len().saturating_sub(1)).map(move |k| [face[0], face[k], face[k + 1]])
    }
}
