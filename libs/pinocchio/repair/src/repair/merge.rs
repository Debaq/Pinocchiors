//! Fusión de vértices duplicados

use pinocchio_mesh::{Mesh, MeshVertex};
use pinocchio_math::Real;
use crate::analysis::duplicates::{find_duplicate_vertices, create_remap_table};

/// Fusiona vértices duplicados en la malla
///
/// Los vértices que están a una distancia menor que `tolerance` se fusionan
/// en uno solo, y todas las referencias se actualizan.
///
/// # Algoritmo
///
/// 1. Encontrar grupos de vértices duplicados usando spatial hashing
/// 2. Crear tabla de remapeo (vértice original → representante)
/// 3. Crear nuevos vértices (solo los representantes)
/// 4. Actualizar todas las caras con los nuevos índices
/// 5. Reconstruir la estructura half-edge
///
/// # Argumentos
///
/// * `mesh` - La malla a modificar
/// * `tolerance` - Distancia máxima para considerar dos vértices como duplicados
///
/// # Retorna
///
/// Número de vértices fusionados (eliminados)
pub fn merge_duplicate_vertices(mesh: &mut Mesh, tolerance: Real) -> usize {
    let groups = find_duplicate_vertices(mesh, tolerance);

    if groups.is_empty() {
        return 0;
    }

    let old_num_vertices = mesh.num_vertices();
    let remap = create_remap_table(old_num_vertices, &groups);

    // Extraer los triángulos originales con índices remapeados
    let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(mesh.num_faces());

    for face_idx in 0..mesh.num_faces() {
        let [v0, v1, v2] = mesh.get_face_vertices(face_idx);
        let new_v0 = remap[v0];
        let new_v1 = remap[v1];
        let new_v2 = remap[v2];

        // Filtrar triángulos degenerados (donde dos vértices se fusionaron al mismo)
        if new_v0 != new_v1 && new_v1 != new_v2 && new_v2 != new_v0 {
            triangles.push([new_v0, new_v1, new_v2]);
        }
    }

    // Crear mapa de índice viejo a índice nuevo
    let mut old_to_new: Vec<Option<usize>> = vec![None; old_num_vertices];
    let mut new_vertices: Vec<MeshVertex> = Vec::new();

    for (old_idx, _) in mesh.vertices.iter().enumerate() {
        let rep = remap[old_idx];
        if rep == old_idx {
            // Este es un representante, mantenerlo
            old_to_new[old_idx] = Some(new_vertices.len());
            new_vertices.push(mesh.vertices[old_idx].clone());
        }
    }

    // Para vértices no-representantes, mapear al índice del representante
    for old_idx in 0..old_num_vertices {
        if old_to_new[old_idx].is_none() {
            let rep = remap[old_idx];
            old_to_new[old_idx] = old_to_new[rep];
        }
    }

    // Actualizar índices en triángulos
    for tri in &mut triangles {
        for v in tri.iter_mut() {
            *v = old_to_new[*v].unwrap();
        }
    }

    // Reconstruir la malla
    *mesh = Mesh::from_triangles(
        &new_vertices.iter().map(|v| v.position).collect::<Vec<_>>(),
        &triangles,
    );

    old_num_vertices - mesh.num_vertices()
}

/// Fusiona dos vértices específicos
///
/// El vértice `v1` se fusiona con `v0`, y todas las referencias a `v1`
/// se actualizan para apuntar a `v0`.
///
/// # Nota
///
/// Esta función es más costosa que `merge_duplicate_vertices` para múltiples
/// fusiones, ya que reconstruye la malla cada vez.
pub fn merge_vertices(mesh: &mut Mesh, v0: usize, v1: usize) -> bool {
    if v0 >= mesh.num_vertices() || v1 >= mesh.num_vertices() || v0 == v1 {
        return false;
    }

    // Extraer triángulos con v1 remapeado a v0
    let mut triangles: Vec<[usize; 3]> = Vec::new();

    for face_idx in 0..mesh.num_faces() {
        let [a, b, c] = mesh.get_face_vertices(face_idx);
        let na = if a == v1 { v0 } else { a };
        let nb = if b == v1 { v0 } else { b };
        let nc = if c == v1 { v0 } else { c };

        // Filtrar degenerados
        if na != nb && nb != nc && nc != na {
            triangles.push([na, nb, nc]);
        }
    }

    // Crear mapa de índices (v1 se elimina, índices mayores se decrementan)
    let mut index_map: Vec<usize> = (0..mesh.num_vertices()).collect();
    for (i, slot) in index_map.iter_mut().enumerate().skip(v1 + 1) {
        *slot = i - 1;
    }
    index_map[v1] = v0;

    // Actualizar índices en triángulos
    for tri in &mut triangles {
        for v in tri.iter_mut() {
            *v = index_map[*v];
        }
    }

    // Posición promedio para el vértice fusionado
    let pos0 = mesh.vertices[v0].position;
    let pos1 = mesh.vertices[v1].position;
    let avg_pos = (pos0 + pos1) * 0.5;

    // Nuevos vértices (excluyendo v1)
    let mut new_positions: Vec<_> = mesh.vertices.iter()
        .enumerate()
        .filter(|(i, _)| *i != v1)
        .map(|(_, v)| v.position)
        .collect();

    // Actualizar posición del fusionado
    new_positions[if v0 > v1 { v0 - 1 } else { v0 }] = avg_pos;

    // Reconstruir
    *mesh = Mesh::from_triangles(&new_positions, &triangles);

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    #[test]
    fn test_no_merge_needed() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let merged = merge_duplicate_vertices(&mut mesh, 1e-6);
        assert_eq!(merged, 0);
        assert_eq!(mesh.num_vertices(), 3);
    }

    #[test]
    fn test_merge_exact_duplicates() {
        // Dos triángulos que comparten vértices duplicados
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0
            Vector3::new(1.0, 0.0, 0.0),  // 1
            Vector3::new(0.5, 1.0, 0.0),  // 2
            Vector3::new(0.0, 0.0, 0.0),  // 3 - duplicado de 0
            Vector3::new(1.0, 0.0, 0.0),  // 4 - duplicado de 1
            Vector3::new(0.5, -1.0, 0.0), // 5
        ];
        let faces = vec![
            [0, 1, 2],
            [3, 5, 4],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let merged = merge_duplicate_vertices(&mut mesh, 1e-6);
        assert_eq!(merged, 2);  // 3→0, 4→1
        assert_eq!(mesh.num_vertices(), 4);
        assert_eq!(mesh.num_faces(), 2);
    }

    #[test]
    fn test_merge_creates_valid_mesh() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 0.0),  // Duplicado
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![
            [0, 2, 3],
            [1, 3, 2],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        merge_duplicate_vertices(&mut mesh, 1e-6);
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_merge_two_vertices() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, -1.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        // Fusionar vértices 2 y 3
        let success = merge_vertices(&mut mesh, 2, 3);
        assert!(success);
        assert_eq!(mesh.num_vertices(), 3);
    }
}
