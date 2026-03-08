//! Detección de geometría non-manifold

use pinocchio_mesh::Mesh;
use serde::Serialize;
use std::collections::HashMap;

/// Una arista non-manifold (más de 2 caras incidentes)
#[derive(Debug, Clone, Serialize)]
pub struct NonManifoldEdge {
    /// Vértice de origen
    pub v0: usize,
    /// Vértice de destino
    pub v1: usize,
    /// Índices de las caras que comparten esta arista
    pub faces: Vec<usize>,
}

impl NonManifoldEdge {
    /// Número de caras incidentes
    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }
}

/// Un vértice non-manifold (geometría no conectada localmente)
#[derive(Debug, Clone, Serialize)]
pub struct NonManifoldVertex {
    /// Índice del vértice
    pub vertex: usize,
    /// Número de "fans" de caras separados alrededor del vértice
    pub num_fans: usize,
}

/// Encuentra aristas non-manifold (con más de 2 caras incidentes)
///
/// En una malla manifold, cada arista puede tener como máximo 2 caras incidentes:
/// - 1 cara: arista de borde
/// - 2 caras: arista interior
/// - 3+ caras: non-manifold
///
/// # Algoritmo
///
/// Cuenta las caras incidentes a cada arista usando un HashMap.
///
/// Complejidad: O(F) donde F es el número de caras
pub fn find_non_manifold_edges(mesh: &Mesh) -> Vec<NonManifoldEdge> {
    // Contar caras por arista (usando par de vértices ordenados como clave)
    let mut edge_faces: HashMap<(usize, usize), Vec<usize>> = HashMap::new();

    for face_idx in 0..mesh.num_faces() {
        let verts = mesh.get_face_vertices(face_idx);

        for i in 0..3 {
            let v0 = verts[i];
            let v1 = verts[(i + 1) % 3];
            // Clave ordenada para que (a,b) y (b,a) sean la misma arista
            let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };
            edge_faces.entry(key).or_default().push(face_idx);
        }
    }

    // Filtrar aristas con más de 2 caras
    edge_faces
        .into_iter()
        .filter(|(_, faces)| faces.len() > 2)
        .map(|((v0, v1), faces)| NonManifoldEdge { v0, v1, faces })
        .collect()
}

/// Encuentra vértices non-manifold
///
/// Un vértice es non-manifold si las caras a su alrededor no forman un único
/// "fan" (abanico) conectado. Esto sucede cuando hay geometría separada que
/// comparte el mismo vértice.
///
/// # Algoritmo
///
/// Para cada vértice:
/// 1. Recopilar todas las caras incidentes
/// 2. Verificar si forman un único componente conectado
/// 3. Si hay múltiples componentes, es non-manifold
///
/// Complejidad: O(V * F_avg) donde F_avg es el grado promedio de un vértice
pub fn find_non_manifold_vertices(mesh: &Mesh) -> Vec<NonManifoldVertex> {
    // Primero, construir la lista de caras incidentes a cada vértice
    let mut vertex_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.num_vertices()];

    for face_idx in 0..mesh.num_faces() {
        let verts = mesh.get_face_vertices(face_idx);
        for &v in &verts {
            vertex_faces[v].push(face_idx);
        }
    }

    let mut non_manifold = Vec::new();

    for (vertex_idx, faces) in vertex_faces.iter().enumerate() {
        if faces.len() < 2 {
            continue;
        }

        let num_fans = count_connected_fans(mesh, vertex_idx, faces);
        if num_fans > 1 {
            non_manifold.push(NonManifoldVertex {
                vertex: vertex_idx,
                num_fans,
            });
        }
    }

    non_manifold
}

/// Cuenta el número de "fans" conectados alrededor de un vértice
fn count_connected_fans(mesh: &Mesh, vertex: usize, faces: &[usize]) -> usize {
    if faces.is_empty() {
        return 0;
    }

    let mut visited = vec![false; faces.len()];
    let mut num_fans = 0;

    // Construir adyacencia entre caras (comparten arista en el vértice)
    let face_neighbors = build_face_adjacency_at_vertex(mesh, vertex, faces);

    // BFS para contar componentes conectados
    for start in 0..faces.len() {
        if visited[start] {
            continue;
        }

        num_fans += 1;
        let mut stack = vec![start];

        while let Some(idx) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;

            for &neighbor in &face_neighbors[idx] {
                if !visited[neighbor] {
                    stack.push(neighbor);
                }
            }
        }
    }

    num_fans
}

/// Construye la adyacencia entre caras en un vértice
///
/// Dos caras son adyacentes si comparten una arista que incluye el vértice dado.
fn build_face_adjacency_at_vertex(
    mesh: &Mesh,
    vertex: usize,
    faces: &[usize],
) -> Vec<Vec<usize>> {
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); faces.len()];

    // Para cada cara, encontrar las aristas que incluyen el vértice
    let mut face_edges: Vec<[(usize, usize); 2]> = Vec::with_capacity(faces.len());

    for &face_idx in faces {
        let verts = mesh.get_face_vertices(face_idx);
        let mut edges = [(0, 0), (0, 0)];
        let mut edge_count = 0;

        for i in 0..3 {
            let v0 = verts[i];
            let v1 = verts[(i + 1) % 3];
            if v0 == vertex || v1 == vertex {
                // Normalizar la arista
                let edge = if v0 < v1 { (v0, v1) } else { (v1, v0) };
                if edge_count < 2 {
                    edges[edge_count] = edge;
                    edge_count += 1;
                }
            }
        }

        face_edges.push(edges);
    }

    // Comparar pares de caras
    for i in 0..faces.len() {
        for j in (i + 1)..faces.len() {
            // Verificar si comparten una arista
            let edges_i = &face_edges[i];
            let edges_j = &face_edges[j];

            let shared = edges_i.iter().any(|e| edges_j.contains(e));
            if shared {
                adjacency[i].push(j);
                adjacency[j].push(i);
            }
        }
    }

    adjacency
}

/// Verifica si una malla es 2-manifold
///
/// Una malla es 2-manifold si:
/// - Cada arista tiene como máximo 2 caras incidentes
/// - Las caras alrededor de cada vértice forman un único fan conectado
pub fn is_manifold(mesh: &Mesh) -> bool {
    find_non_manifold_edges(mesh).is_empty() && find_non_manifold_vertices(mesh).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_tetrahedron() -> Mesh {
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
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_manifold_mesh() {
        let mesh = make_tetrahedron();

        let nm_edges = find_non_manifold_edges(&mesh);
        let nm_verts = find_non_manifold_vertices(&mesh);

        assert!(nm_edges.is_empty());
        assert!(nm_verts.is_empty());
        assert!(is_manifold(&mesh));
    }

    #[test]
    fn test_single_triangle_manifold() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        assert!(is_manifold(&mesh));
    }

    #[test]
    fn test_non_manifold_edge() {
        // Crear una configuración con 3 caras compartiendo una arista
        // (triángulo central con dos triángulos extra en la misma arista)
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0
            Vector3::new(1.0, 0.0, 0.0),  // 1
            Vector3::new(0.5, 1.0, 0.0),  // 2
            Vector3::new(0.5, -1.0, 0.0), // 3
            Vector3::new(0.5, 0.0, 1.0),  // 4 (tercera cara en arista 0-1)
        ];
        let faces = vec![
            [0, 1, 2],  // Arriba
            [0, 3, 1],  // Abajo
            [0, 1, 4],  // Extra (comparte arista 0-1 con las otras dos)
        ];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let nm_edges = find_non_manifold_edges(&mesh);
        assert_eq!(nm_edges.len(), 1);
        assert_eq!(nm_edges[0].num_faces(), 3);
    }
}
