//! Reparación de geometría non-manifold

use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3;
use crate::analysis::manifold::{find_non_manifold_edges, find_non_manifold_vertices};

/// Repara aristas non-manifold duplicando geometría
///
/// Cuando una arista tiene más de 2 caras incidentes, duplica los vértices
/// para separar las caras en grupos de 2.
///
/// # Retorna
///
/// Número de aristas reparadas
pub fn repair_non_manifold_edges(mesh: &mut Mesh) -> usize {
    let nm_edges = find_non_manifold_edges(mesh);

    if nm_edges.is_empty() {
        return 0;
    }

    let mut repaired = 0;

    // Procesar cada arista non-manifold
    for edge in &nm_edges {
        if edge.faces.len() <= 2 {
            continue;
        }

        // Estrategia: mantener las primeras 2 caras, duplicar vértices para las demás
        let faces_to_split: Vec<usize> = edge.faces[2..].to_vec();

        if !faces_to_split.is_empty() {
            split_faces_at_edge(mesh, edge.v0, edge.v1, &faces_to_split);
            repaired += 1;
        }
    }

    repaired
}

/// Separa caras que comparten una arista duplicando los vértices de esa arista
fn split_faces_at_edge(
    mesh: &mut Mesh,
    v0: usize,
    v1: usize,
    faces: &[usize],
) {
    if faces.is_empty() {
        return;
    }

    // Extraer triángulos actuales
    let mut triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    // Crear nuevos vértices (duplicados de v0 y v1)
    let mut positions: Vec<Vector3> = mesh.vertices.iter()
        .map(|v| v.position)
        .collect();

    let new_v0 = positions.len();
    positions.push(mesh.vertices[v0].position);

    let new_v1 = positions.len();
    positions.push(mesh.vertices[v1].position);

    // Actualizar las caras que deben usar los nuevos vértices
    for &face_idx in faces {
        let tri = &mut triangles[face_idx];
        for v in tri.iter_mut() {
            if *v == v0 {
                *v = new_v0;
            } else if *v == v1 {
                *v = new_v1;
            }
        }
    }

    // Reconstruir la malla
    *mesh = Mesh::from_triangles(&positions, &triangles);
}

/// Repara vértices non-manifold duplicando el vértice para cada fan separado
///
/// # Retorna
///
/// Número de vértices reparados
pub fn repair_non_manifold_vertices(mesh: &mut Mesh) -> usize {
    let nm_verts = find_non_manifold_vertices(mesh);

    if nm_verts.is_empty() {
        return 0;
    }

    let mut repaired = 0;

    for nv in &nm_verts {
        if nv.num_fans <= 1 {
            continue;
        }

        split_vertex_fans(mesh, nv.vertex);
        repaired += 1;
    }

    repaired
}

/// Separa los fans de caras alrededor de un vértice non-manifold
fn split_vertex_fans(mesh: &mut Mesh, vertex: usize) {
    // Encontrar todas las caras que usan este vértice
    let mut vertex_faces: Vec<usize> = Vec::new();
    for face_idx in 0..mesh.num_faces() {
        let verts = mesh.get_face_vertices(face_idx);
        if verts.contains(&vertex) {
            vertex_faces.push(face_idx);
        }
    }

    if vertex_faces.len() < 2 {
        return;
    }

    // Agrupar caras en fans conectados
    let fans = group_faces_into_fans(mesh, vertex, &vertex_faces);

    if fans.len() <= 1 {
        return;  // Solo hay un fan, no es non-manifold
    }

    // Extraer datos actuales
    let mut triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    let mut positions: Vec<Vector3> = mesh.vertices.iter()
        .map(|v| v.position)
        .collect();

    // Para cada fan después del primero, crear un nuevo vértice
    for fan in fans.iter().skip(1) {
        let new_vertex = positions.len();
        positions.push(mesh.vertices[vertex].position);

        // Actualizar las caras de este fan para usar el nuevo vértice
        for &face_idx in fan {
            let tri = &mut triangles[face_idx];
            for v in tri.iter_mut() {
                if *v == vertex {
                    *v = new_vertex;
                }
            }
        }
    }

    // Reconstruir la malla
    *mesh = Mesh::from_triangles(&positions, &triangles);
}

/// Agrupa caras en fans conectados alrededor de un vértice
fn group_faces_into_fans(
    mesh: &Mesh,
    vertex: usize,
    faces: &[usize],
) -> Vec<Vec<usize>> {
    if faces.is_empty() {
        return Vec::new();
    }

    let mut fans: Vec<Vec<usize>> = Vec::new();
    let mut visited = vec![false; faces.len()];

    // Construir adyacencia entre caras (comparten arista que incluye vertex)
    let adjacency = build_face_adjacency_for_vertex(mesh, vertex, faces);

    // BFS para encontrar componentes conectados
    for start in 0..faces.len() {
        if visited[start] {
            continue;
        }

        let mut fan = Vec::new();
        let mut stack = vec![start];

        while let Some(idx) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            fan.push(faces[idx]);

            for &neighbor in &adjacency[idx] {
                if !visited[neighbor] {
                    stack.push(neighbor);
                }
            }
        }

        fans.push(fan);
    }

    fans
}

/// Construye adyacencia entre caras que comparten una arista con el vértice dado
fn build_face_adjacency_for_vertex(
    mesh: &Mesh,
    vertex: usize,
    faces: &[usize],
) -> Vec<Vec<usize>> {
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); faces.len()];

    // Para cada cara, encontrar los otros vértices conectados a vertex
    let mut face_edges: Vec<Vec<usize>> = Vec::new();
    for &face_idx in faces {
        let verts = mesh.get_face_vertices(face_idx);
        let other_verts: Vec<usize> = verts.iter()
            .filter(|&&v| v != vertex)
            .cloned()
            .collect();
        face_edges.push(other_verts);
    }

    // Dos caras son adyacentes si comparten un vértice (además del central)
    for i in 0..faces.len() {
        for j in (i + 1)..faces.len() {
            let shared = face_edges[i].iter()
                .any(|v| face_edges[j].contains(v));
            if shared {
                adjacency[i].push(j);
                adjacency[j].push(i);
            }
        }
    }

    adjacency
}

/// Elimina caras que causan problemas non-manifold
///
/// Esta es una estrategia alternativa más agresiva: en lugar de duplicar
/// vértices, simplemente elimina las caras problemáticas.
///
/// # Retorna
///
/// Número de caras eliminadas
pub fn remove_non_manifold_faces(mesh: &mut Mesh) -> usize {
    let nm_edges = find_non_manifold_edges(mesh);

    if nm_edges.is_empty() {
        return 0;
    }

    // Marcar caras a eliminar (las extras de cada arista nm)
    let mut faces_to_remove: Vec<bool> = vec![false; mesh.num_faces()];

    for edge in &nm_edges {
        // Mantener las primeras 2 caras, eliminar el resto
        for &face_idx in &edge.faces[2..] {
            faces_to_remove[face_idx] = true;
        }
    }

    let removed_count = faces_to_remove.iter().filter(|&&x| x).count();

    if removed_count == 0 {
        return 0;
    }

    // Extraer triángulos que no se eliminan
    let triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
        .filter(|&i| !faces_to_remove[i])
        .map(|i| mesh.get_face_vertices(i))
        .collect();

    // Identificar vértices usados
    let mut vertex_used = vec![false; mesh.num_vertices()];
    for tri in &triangles {
        for &v in tri {
            vertex_used[v] = true;
        }
    }

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

    removed_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifold_mesh_no_repair() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
            [1, 3, 2],
            [2, 3, 0],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let repaired = repair_non_manifold_edges(&mut mesh);
        assert_eq!(repaired, 0);
    }

    #[test]
    fn test_non_manifold_edge_repair() {
        // Crear una arista con 3 caras
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0
            Vector3::new(1.0, 0.0, 0.0),  // 1
            Vector3::new(0.5, 1.0, 0.0),  // 2
            Vector3::new(0.5, -1.0, 0.0), // 3
            Vector3::new(0.5, 0.0, 1.0),  // 4
        ];
        let faces = vec![
            [0, 1, 2],  // Cara superior
            [0, 3, 1],  // Cara inferior
            [0, 1, 4],  // Tercera cara en arista 0-1
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let repaired = repair_non_manifold_edges(&mut mesh);
        assert_eq!(repaired, 1);

        // Verificar que ahora es manifold
        let nm_edges = find_non_manifold_edges(&mesh);
        assert!(nm_edges.is_empty());
    }

    #[test]
    fn test_remove_non_manifold_faces() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, -1.0, 0.0),
            Vector3::new(0.5, 0.0, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
            [0, 1, 4],
        ];
        let mut mesh = Mesh::from_triangles(&vertices, &faces);

        let removed = remove_non_manifold_faces(&mut mesh);
        assert_eq!(removed, 1);
        assert_eq!(mesh.num_faces(), 2);
    }
}
