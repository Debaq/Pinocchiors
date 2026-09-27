//! Informe de calidad de una malla de quads.

use crate::quad::QuadMesh;
use crate::V3;
use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use std::collections::HashMap;

/// Métricas de calidad de una [`QuadMesh`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QualityReport {
    /// Vértices interiores (fuera del borde).
    pub interior_vertices: usize,
    /// Vértices interiores con valencia distinta de 4.
    pub irregular_vertices: usize,
    /// Quads con alguna esquina plegada (jacobiano escalado < 0).
    pub folded_quads: usize,
    /// Quads deformes: jacobiano escalado mínimo < 0.5 (esquina fuera de ~30°–150°).
    pub poor_quads: usize,
    /// Quads con el lado mayor más de 5 veces el menor.
    pub stretched_quads: usize,
    /// Desviación media de los ángulos internos respecto de 90°, en grados.
    pub mean_angle_deviation: f64,
    /// Distancia máxima de los quads (vértices y centros) a la malla original,
    /// en fracción de la diagonal de su caja envolvente; `None` sin original.
    pub max_distance: Option<f64>,
}

impl QualityReport {
    /// Porcentaje de vértices interiores irregulares.
    pub fn irregular_percent(&self) -> f64 {
        percent(self.irregular_vertices, self.interior_vertices)
    }
}

fn percent(part: usize, total: usize) -> f64 {
    if total == 0 { 0.0 } else { 100.0 * part as f64 / total as f64 }
}

/// Analiza la malla; si se da la original, mide también la distancia a ella.
pub fn analyze(q: &QuadMesh, original: Option<&Mesh>) -> QualityReport {
    let mut report = QualityReport::default();

    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for f in &q.faces {
        for k in 0..4 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 4]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut valence = vec![0usize; q.vertices.len()];
    let mut boundary = vec![false; q.vertices.len()];
    for (&(a, b), &count) in &edges {
        valence[a] += 1;
        valence[b] += 1;
        if count != 2 {
            boundary[a] = true;
            boundary[b] = true;
        }
    }
    for v in 0..q.vertices.len() {
        if valence[v] > 0 && !boundary[v] {
            report.interior_vertices += 1;
            if valence[v] != 4 {
                report.irregular_vertices += 1;
            }
        }
    }

    let mut deviation = 0.0;
    for f in &q.faces {
        let p = f.v.map(|i| q.vertices[i]);
        let normal = (p[2] - p[0]).cross(&(p[3] - p[1])).try_normalize(1e-300);
        let mut jacobian = f64::INFINITY;
        let mut lengths = [0.0; 4];
        for k in 0..4 {
            let (a, b) = (p[(k + 1) % 4] - p[k], p[(k + 3) % 4] - p[k]);
            lengths[k] = a.norm();
            deviation += (a.angle(&b).to_degrees() - 90.0).abs();
            let d = a.norm() * b.norm();
            let j = match normal {
                Some(n) if d > 0.0 => a.cross(&b).dot(&n) / d,
                _ => -1.0,
            };
            jacobian = jacobian.min(j);
        }
        report.folded_quads += usize::from(jacobian < 0.0);
        report.poor_quads += usize::from(jacobian < 0.5);
        let longest = lengths.iter().cloned().fold(0.0, f64::max);
        let shortest = lengths.iter().cloned().fold(f64::INFINITY, f64::min);
        report.stretched_quads += usize::from(longest > 5.0 * shortest);
    }
    report.mean_angle_deviation = deviation / (4 * q.faces.len()).max(1) as f64;

    report.max_distance = original.filter(|m| m.num_faces() > 0).map(|m| max_distance(q, m));
    report
}

fn max_distance(q: &QuadMesh, mesh: &Mesh) -> f64 {
    let positions: Vec<pinocchio_math::Vector3> = mesh.vertices.iter().map(|v| v.position).collect();
    let (lo, hi) = positions.iter().fold(
        (V3::repeat(f64::INFINITY), V3::repeat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.inf(&p.0), hi.sup(&p.0)),
    );
    let diagonal = (hi - lo).norm();
    let bvh = Bvh::build(
        (0..mesh.num_faces())
            .map(|f| {
                let [a, b, c] = mesh.get_face_vertices(f).map(|i| positions[i]);
                Triangle::new(a, b, c)
            })
            .collect(),
    );
    let centers = q.faces.iter().map(|f| f.v.iter().map(|&i| q.vertices[i]).sum::<V3>() / 4.0);
    let max = centers
        .chain(q.vertices.iter().copied())
        .map(|p| bvh.query_distance(&pinocchio_math::Vector3(p)))
        .fold(0.0, f64::max);
    if diagonal > 0.0 { max / diagonal } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quad::QuadFace;

    #[test]
    fn regular_grid_is_perfect() {
        // Grilla 3×3 de quads unitarios
        let vertices = (0..16).map(|i| V3::new((i % 4) as f64, (i / 4) as f64, 0.0)).collect();
        let faces = (0..9)
            .map(|c| {
                let v = c / 3 * 4 + c % 3;
                QuadFace { v: [v, v + 1, v + 5, v + 4] }
            })
            .collect();
        let r = analyze(&QuadMesh { vertices, faces }, None);
        assert_eq!(r.interior_vertices, 4);
        assert_eq!(r.irregular_vertices, 0);
        assert_eq!((r.folded_quads, r.poor_quads, r.stretched_quads), (0, 0, 0));
        assert!(r.mean_angle_deviation < 1e-9);
        assert_eq!(r.max_distance, None);
    }

    #[test]
    fn folded_and_stretched_quads_are_counted() {
        let vertices = vec![
            V3::new(0.0, 0.0, 0.0),
            V3::new(10.0, 0.0, 0.0),
            V3::new(10.0, 1.0, 0.0),
            V3::new(0.0, 1.0, 0.0),
            V3::new(0.0, 5.0, 0.0),
            V3::new(1.0, 5.0, 0.0),
            V3::new(0.3, 5.3, 0.0), // esquina reentrante
            V3::new(0.0, 6.0, 0.0),
        ];
        let faces = vec![QuadFace { v: [0, 1, 2, 3] }, QuadFace { v: [4, 5, 7, 6] }];
        let r = analyze(&QuadMesh { vertices, faces }, None);
        assert_eq!(r.stretched_quads, 1);
        assert_eq!(r.folded_quads, 1);
        assert_eq!(r.poor_quads, 1);
    }
}
