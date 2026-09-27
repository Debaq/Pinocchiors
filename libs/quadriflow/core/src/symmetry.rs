//! Simetría espejo: recorte de la mitad positiva y reflexión del resultado.
//!
//! El corte queda como borde abierto, que la retopología respeta (vértices
//! sobre el borde y quads alineados a él). Al reflejar, los vértices de ese
//! borde se sueldan con sus imágenes, que son ellos mismos.

use crate::quad::{QuadFace, QuadMesh};
use crate::surface::Surface;
use crate::V3;
use std::collections::HashMap;

/// Coordenada del plano: el centro de la caja envolvente en el eje.
pub(crate) fn center(surface: &Surface, axis: usize) -> f64 {
    let (lo, hi) = surface
        .positions
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])));
    0.5 * (lo + hi)
}

/// Parte de la superficie con `p[axis] ≥ center`. Los triángulos que cruzan el
/// plano se cortan; los vértices a menos de una millonésima de la diagonal
/// del plano se llevan exactamente a él.
pub(crate) fn clip(surface: &Surface, axis: usize, center: f64) -> Surface {
    let epsilon = surface.bbox_diagonal() * 1e-6;
    let mut positions = surface.positions.clone();
    let side: Vec<f64> = positions
        .iter_mut()
        .map(|p| {
            let d = p[axis] - center;
            if d.abs() < epsilon {
                p[axis] = center;
                0.0
            } else {
                d
            }
        })
        .collect();

    let mut cut: HashMap<(u32, u32), u32> = HashMap::new();
    let mut triangles = Vec::new();
    for t in &surface.triangles {
        let d = t.map(|i| side[i as usize]);
        if d.iter().all(|&x| x <= 0.0) {
            continue; // del otro lado, o contenido en el plano
        }
        // Sutherland–Hodgman contra el semiespacio d ≥ 0
        let mut polygon = Vec::with_capacity(4);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (da, db) = (d[k], d[(k + 1) % 3]);
            if da >= 0.0 {
                polygon.push(a);
            }
            if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
                let key = (a.min(b), a.max(b));
                let m = *cut.entry(key).or_insert_with(|| {
                    let (pa, pb) = (positions[a as usize], positions[b as usize]);
                    let mut p = pa + (pb - pa) * (da / (da - db));
                    p[axis] = center;
                    positions.push(p);
                    (positions.len() - 1) as u32
                });
                polygon.push(m);
            }
        }
        for k in 1..polygon.len().saturating_sub(1) {
            triangles.push([polygon[0], polygon[k], polygon[k + 1]]);
        }
    }
    let mut out = Surface { positions, triangles };
    out.weld(epsilon * 1e-3);
    out
}

/// Refleja la mitad y la suelda con su imagen por los vértices de borde que
/// están sobre el plano.
pub(crate) fn mirror(half: &QuadMesh, axis: usize, center: f64, tolerance: f64) -> QuadMesh {
    let n = half.vertices.len();
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for f in &half.faces {
        for k in 0..4 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 4]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut on_seam = vec![false; n];
    for (&(a, b), &count) in &edges {
        if count == 1 {
            for v in [a, b] {
                on_seam[v] |= (half.vertices[v][axis] - center).abs() <= tolerance;
            }
        }
    }

    let mut vertices = half.vertices.clone();
    for (v, seam) in vertices.iter_mut().zip(&on_seam) {
        if *seam {
            v[axis] = center;
        }
    }
    let mut image = vec![0; n];
    for v in 0..n {
        image[v] = if on_seam[v] {
            v
        } else {
            let mut p: V3 = vertices[v];
            p[axis] = 2.0 * center - p[axis];
            vertices.push(p);
            vertices.len() - 1
        };
    }
    let mut faces = half.faces.clone();
    // La reflexión invierte la orientación: se recorre al revés
    faces.extend(half.faces.iter().map(|f| {
        let [a, b, c, d] = f.v.map(|i| image[i]);
        QuadFace { v: [d, c, b, a] }
    }));
    let mut out = QuadMesh { vertices, faces };
    out.split_nonmanifold_vertices();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_keeps_the_positive_half_with_a_straight_cut() {
        // Cuadrado [-1, 1]² en z = 0, dos triángulos
        let s = Surface {
            positions: vec![
                V3::new(-1.0, -1.0, 0.0),
                V3::new(1.0, -1.0, 0.0),
                V3::new(1.0, 1.0, 0.0),
                V3::new(-1.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let half = clip(&s, 0, 0.0);
        assert!((half.area() - 2.0).abs() < 1e-12, "{}", half.area());
        assert!(half.positions.iter().all(|p| p.x >= 0.0));
        // Todas las aristas de borde sobre x = 0 o sobre el contorno original
        let on_cut = half.positions.iter().filter(|p| p.x == 0.0).count();
        assert!(on_cut >= 2);
    }

    #[test]
    fn mirror_welds_the_seam() {
        // Dos quads en x ≥ 0 con el borde izquierdo sobre x = 0
        let half = QuadMesh {
            vertices: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(1.0, 0.0, 0.0),
                V3::new(1.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
                V3::new(2.0, 0.0, 0.0),
                V3::new(2.0, 1.0, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }, QuadFace { v: [1, 4, 5, 2] }],
        };
        let full = mirror(&half, 0, 0.0, 1e-9);
        assert_eq!(full.num_faces(), 4);
        assert_eq!(full.num_vertices(), 10); // 5 columnas × 2 filas
        let topo = full.topology();
        assert_eq!(topo.non_manifold_edges, 0);
        assert_eq!(topo.flipped_edges, 0);
        assert_eq!(topo.euler_characteristic, 1);
    }
}
