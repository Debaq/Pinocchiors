//! Eliminación de caras degeneradas

use pinocchio_mesh::Mesh;
use crate::config::DegenerateConfig;
use crate::analysis::degenerate::find_degenerate_faces;

/// Elimina caras degeneradas de la malla
///
/// Las caras degeneradas incluyen:
/// - Caras con área cero o casi cero
/// - "Needles": triángulos con ángulos muy pequeños
/// - "Caps": triángulos con ángulos muy grandes
///
/// # Argumentos
///
/// * `mesh` - La malla a modificar
/// * `config` - Configuración que define qué considerar degenerado
///
/// # Retorna
///
/// Número de caras eliminadas
pub fn remove_degenerate_faces(mesh: &mut Mesh, config: &DegenerateConfig) -> usize {
    let needle_angle = if config.remove_needles { config.needle_angle } else { 0.0 };
    let cap_angle = if config.remove_caps { config.cap_angle } else { std::f64::consts::PI };

    let degenerates = find_degenerate_faces(mesh, config.min_area, needle_angle, cap_angle);

    if degenerates.is_empty() {
        return 0;
    }

    // Crear conjunto de caras a eliminar
    let mut to_remove: Vec<bool> = vec![false; mesh.num_faces()];
    for d in &degenerates {
        to_remove[d.face_index()] = true;
    }

    // Extraer triángulos que no se eliminarán
    let mut triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .filter(|&face_idx| !to_remove[face_idx])
        .map(|face_idx| mesh.get_face_vertices(face_idx))
        .collect();

    let removed_count = degenerates.len();

    // Identificar vértices usados
    let mut vertex_used = vec![false; mesh.num_vertices()];
    for tri in &triangles {
        for &v in tri {
            vertex_used[v] = true;
        }
    }

    // Crear mapa de índices viejos a nuevos
    let mut old_to_new: Vec<usize> = vec![0; mesh.num_vertices()];
    let mut new_positions = Vec::new();
    for (old_idx, used) in vertex_used.iter().enumerate() {
        if *used {
            old_to_new[old_idx] = new_positions.len();
            new_positions.push(mesh.vertices[old_idx].position);
        }
    }

    // Actualizar índices en triángulos
    for tri in &mut triangles {
        for v in tri.iter_mut() {
            *v = old_to_new[*v];
        }
    }

    // Reconstruir la malla
    if triangles.is_empty() {
        *mesh = Mesh::new();
    } else {
        *mesh = Mesh::from_triangles(&new_positions, &triangles);
    }

    removed_count
}

/// Elimina vértices aislados (sin caras incidentes)
///
/// # Nota
///
/// Esta función generalmente no es necesaria ya que `Mesh::from_triangles`
/// solo incluye vértices referenciados. Se incluye para casos donde se
/// manipula la malla directamente.
pub fn remove_isolated_vertices(mesh: &mut Mesh) -> usize {
    // Identificar vértices usados
    let mut vertex_used = vec![false; mesh.num_vertices()];
    for face_idx in 0..mesh.num_faces() {
        let [v0, v1, v2] = mesh.get_face_vertices(face_idx);
        vertex_used[v0] = true;
        vertex_used[v1] = true;
        vertex_used[v2] = true;
    }

    let isolated_count = vertex_used.iter().filter(|&&u| !u).count();

    if isolated_count == 0 {
        return 0;
    }

    // Extraer triángulos y reconstruir
    let triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    // Crear mapa de índices
    let mut old_to_new: Vec<usize> = vec![0; mesh.num_vertices()];
    let mut new_positions = Vec::new();
    for (old_idx, used) in vertex_used.iter().enumerate() {
        if *used {
            old_to_new[old_idx] = new_positions.len();
            new_positions.push(mesh.vertices[old_idx].position);
        }
    }

    // Actualizar índices
    let triangles: Vec<[usize; 3]> = triangles.iter()
        .map(|[a, b, c]| [old_to_new[*a], old_to_new[*b], old_to_new[*c]])
        .collect();

    *mesh = Mesh::from_triangles(&new_positions, &triangles);

    isolated_count
}

/// Elimina caras duplicadas (mismos vértices)
pub fn remove_duplicate_faces(mesh: &mut Mesh) -> usize {
    use std::collections::HashSet;

    let mut seen: HashSet<[usize; 3]> = HashSet::new();
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    let mut removed = 0;

    for face_idx in 0..mesh.num_faces() {
        let mut verts = mesh.get_face_vertices(face_idx);

        // Normalizar: rotar para que el menor índice esté primero
        let min_pos = verts.iter().enumerate()
            .min_by_key(|&(_, v)| *v)
            .map(|(i, _)| i)
            .unwrap();
        verts.rotate_left(min_pos);

        if seen.insert(verts) {
            triangles.push(mesh.get_face_vertices(face_idx));
        } else {
            removed += 1;
        }
    }

    if removed == 0 {
        return 0;
    }

    // Reconstruir
    let positions: Vec<_> = mesh.vertices.iter().map(|v| v.position).collect();
    *mesh = Mesh::from_triangles(&positions, &triangles);

    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    #[test]
    fn test_remove_zero_area() {
        // Un triángulo normal y uno degenerado (colineal)
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),  // Colineal con 0 y 1
        ];
        let faces = vec![
            [0, 1, 2],  // Normal
            [0, 1, 3],  // Degenerado (área ~0)
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let removed = remove_degenerate_faces(&mut mesh, &DegenerateConfig::default());
        assert_eq!(removed, 1);
        assert_eq!(mesh.num_faces(), 1);
    }

    #[test]
    fn test_no_degenerates() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let removed = remove_degenerate_faces(&mut mesh, &DegenerateConfig::default());
        assert_eq!(removed, 0);
        assert_eq!(mesh.num_faces(), 1);
    }

    #[test]
    fn test_remove_duplicate_faces() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 1, 2],  // Duplicado exacto
            [1, 2, 0],  // Duplicado rotado
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let removed = remove_duplicate_faces(&mut mesh);
        assert_eq!(removed, 2);
        assert_eq!(mesh.num_faces(), 1);
    }

    #[test]
    fn test_cleanup_creates_valid_mesh() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 1, 3],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        remove_degenerate_faces(&mut mesh, &DegenerateConfig::default());
        assert!(mesh.integrity_check().is_ok());
    }
}
