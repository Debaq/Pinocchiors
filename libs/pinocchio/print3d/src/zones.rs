//! Zonas de caras elegidas para texturizar: crecer por ángulo, invertir, área

use std::collections::VecDeque;

use pinocchio_math::Real;
use pinocchio_mesh::Mesh;

/// Caras vecinas por arista, uniendo los vértices que están en el mismo lugar (las costuras de UV y las
/// aristas duras parten los vértices en las mallas importadas, y por índice no se verían vecinas)
pub fn face_neighbors(mesh: &Mesh) -> Vec<Vec<usize>> {
    use std::collections::HashMap;
    let (pos, tris) = crate::geom::triangles(mesh);
    let weld = crate::geom::weld_ids(&pos, (crate::geom::diagonal(&pos) * 1e-7).max(1e-9));
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (f, t) in tris.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (weld[t[i]], weld[t[(i + 1) % 3]]);
            if a != b {
                edges.entry((a.min(b), a.max(b))).or_default().push(f);
            }
        }
    }
    let mut out = vec![Vec::new(); tris.len()];
    for faces in edges.values() {
        for &f in faces {
            for &g in faces {
                if f != g && !out[f].contains(&g) {
                    out[f].push(g);
                }
            }
        }
    }
    out
}

/// Amplía `selected` con las caras que se alcanzan cruzando solo aristas casi planas: el ángulo entre la
/// normal de una cara y la de su vecina no pasa de `max_angle_deg`. Con 0 queda igual; con 180 son todas
/// las caras conectadas. Devuelve las caras ordenadas y sin repetir
pub fn grow_by_angle(mesh: &Mesh, selected: &[usize], max_angle_deg: Real) -> Vec<usize> {
    let n = mesh.num_faces();
    let neighbors = face_neighbors(mesh);
    let normals: Vec<_> = (0..n).map(|f| mesh.get_face_normal(f)).collect();
    let min_dot = max_angle_deg.clamp(0.0, 180.0).to_radians().cos();
    let mut inside = vec![false; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for &f in selected.iter().filter(|&&f| f < n) {
        if !inside[f] {
            inside[f] = true;
            queue.push_back(f);
        }
    }
    while let Some(f) = queue.pop_front() {
        for &g in &neighbors[f] {
            if !inside[g] && normals[f].dot(&normals[g]) >= min_dot - 1e-12 {
                inside[g] = true;
                queue.push_back(g);
            }
        }
    }
    (0..n).filter(|&f| inside[f]).collect()
}

/// Todas las caras que no están en `selected`
pub fn invert_zone(face_count: usize, selected: &[usize]) -> Vec<usize> {
    let mut inside = vec![false; face_count];
    for &f in selected.iter().filter(|&&f| f < face_count) {
        inside[f] = true;
    }
    (0..face_count).filter(|&f| !inside[f]).collect()
}

/// Área de las caras elegidas (en las unidades de la malla al cuadrado)
pub fn zone_area(mesh: &Mesh, faces: &[usize]) -> Real {
    faces.iter().filter(|&&f| f < mesh.num_faces()).map(|&f| mesh.get_face_area(f)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::tests::make_box;

    /// Triángulos de la cara de arriba (z = max) de la caja
    fn top_faces(m: &Mesh) -> Vec<usize> {
        let zmax = m.vertices.iter().map(|v| v.position.z()).fold(Real::NEG_INFINITY, Real::max);
        (0..m.num_faces()).filter(|&f| m.get_face_vertices(f).iter().all(|&i| (m.vertices[i].position.z() - zmax).abs() < 1e-9)).collect()
    }

    #[test]
    fn crecer_por_angulo_se_queda_en_la_cara_plana() {
        let b = make_box(20.0, 10.0, 5.0);
        let top = top_faces(&b);
        assert_eq!(top.len(), 2);
        // Un triángulo de la tapa y 10°: llega al otro de la misma tapa y a nada más (las aristas de la caja son de 90°)
        assert_eq!(grow_by_angle(&b, &top[..1], 10.0), top);
        // Con 100° cruza las aristas vivas y toma toda la caja
        assert_eq!(grow_by_angle(&b, &top[..1], 100.0).len(), b.num_faces());
        // Con 0° no crece, pero el otro triángulo coplanar sigue entrando (ángulo 0 entre normales)
        assert_eq!(grow_by_angle(&b, &top[..1], 0.0), top);
        // Las caras fuera de rango se ignoran
        assert_eq!(grow_by_angle(&b, &[999], 90.0), Vec::<usize>::new());
    }

    #[test]
    fn crece_aunque_los_vertices_esten_partidos() {
        // Caja "sopa": cada triángulo con sus 3 vértices propios (como un STL sin unir)
        let b = make_box(20.0, 10.0, 5.0);
        let (pos, tris) = crate::geom::triangles(&b);
        let soup_pos: Vec<_> = tris.iter().flat_map(|t| t.iter().map(|&i| pos[i])).collect();
        let soup_tris: Vec<[usize; 3]> = (0..tris.len()).map(|f| [3 * f, 3 * f + 1, 3 * f + 2]).collect();
        let soup = Mesh::from_triangles(&soup_pos, &soup_tris);
        let top = top_faces(&soup);
        assert_eq!(top.len(), 2);
        assert_eq!(grow_by_angle(&soup, &top[..1], 10.0), top);
        assert_eq!(grow_by_angle(&soup, &top[..1], 100.0).len(), soup.num_faces());
    }

    #[test]
    fn invertir_y_area() {
        let b = make_box(20.0, 10.0, 5.0);
        let top = top_faces(&b);
        let rest = invert_zone(b.num_faces(), &top);
        assert_eq!(rest.len(), b.num_faces() - top.len());
        assert!(rest.iter().all(|f| !top.contains(f)));
        assert!((zone_area(&b, &top) - 200.0).abs() < 1e-9);
        assert!((zone_area(&b, &top) + zone_area(&b, &rest) - 2.0 * (20.0 * 10.0 + 20.0 * 5.0 + 10.0 * 5.0)).abs() < 1e-9);
        assert_eq!(invert_zone(4, &[]), vec![0, 1, 2, 3]);
        assert_eq!(invert_zone(3, &[0, 1, 2, 9]), Vec::<usize>::new());
    }
}
