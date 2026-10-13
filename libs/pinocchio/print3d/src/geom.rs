//! Utilidades de geometría compartidas por los módulos de preparación

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

/// Posiciones y triángulos indexados de una malla
pub(crate) fn triangles(mesh: &Mesh) -> (Vec<Vector3>, Vec<[usize; 3]>) {
    let positions = mesh.vertices.iter().map(|v| v.position).collect();
    let tris = (0..mesh.num_faces()).map(|f| mesh.get_face_vertices(f)).collect();
    (positions, tris)
}

/// Normal unitaria y área de un triángulo; `None` si es degenerado
pub(crate) fn normal_area(a: Vector3, b: Vector3, c: Vector3) -> Option<(Vector3, Real)> {
    let n = (b - a).cross(&(c - a));
    let len = n.length();
    (len > 1e-14).then(|| (n * (1.0 / len), len * 0.5))
}

/// Soldadura por posición con rejilla de tolerancia `tol`: devuelve el índice
/// soldado de cada vértice. Une las costuras duplicadas de STL/OBJ.
pub(crate) fn weld_ids(positions: &[Vector3], tol: Real) -> Vec<usize> {
    use std::collections::HashMap;
    let inv = 1.0 / tol.max(1e-12);
    let key = |p: &Vector3| ((p.x() * inv).round() as i64, (p.y() * inv).round() as i64, (p.z() * inv).round() as i64);
    let mut map: HashMap<(i64, i64, i64), usize> = HashMap::new();
    positions
        .iter()
        .map(|p| {
            let next = map.len();
            *map.entry(key(p)).or_insert(next)
        })
        .collect()
}

/// Diagonal del cuadro delimitador (0 si está vacía)
pub(crate) fn diagonal(positions: &[Vector3]) -> Real {
    if positions.is_empty() {
        return 0.0;
    }
    let mut lo = positions[0];
    let mut hi = positions[0];
    for p in positions {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (hi - lo).length()
}
