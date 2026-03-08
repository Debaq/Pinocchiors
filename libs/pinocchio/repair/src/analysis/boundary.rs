//! Detección de boundary loops (agujeros) en mallas

use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3;
use serde::Serialize;

/// Un loop de borde que forma un agujero en la malla
#[derive(Debug, Clone, Serialize)]
pub struct BoundaryLoop {
    /// Índices de las aristas que forman el loop (en orden)
    pub edges: Vec<usize>,
    /// Índices de los vértices del loop (en orden)
    pub vertices: Vec<usize>,
}

impl BoundaryLoop {
    /// Calcula el perímetro del loop
    pub fn perimeter(&self, mesh: &Mesh) -> f64 {
        let mut length = 0.0;
        for i in 0..self.vertices.len() {
            let v0 = self.vertices[i];
            let v1 = self.vertices[(i + 1) % self.vertices.len()];
            let p0 = mesh.vertices[v0].position;
            let p1 = mesh.vertices[v1].position;
            length += (p1 - p0).length();
        }
        length
    }

    /// Calcula el centroide del loop
    pub fn centroid(&self, mesh: &Mesh) -> Vector3 {
        if self.vertices.is_empty() {
            return Vector3::zero();
        }
        let sum: Vector3 = self.vertices.iter()
            .map(|&v| mesh.vertices[v].position)
            .fold(Vector3::zero(), |acc, p| acc + p);
        sum * (1.0 / self.vertices.len() as f64)
    }

    /// Calcula la normal aproximada del plano del loop
    pub fn normal(&self, mesh: &Mesh) -> Vector3 {
        if self.vertices.len() < 3 {
            return Vector3::new(0.0, 0.0, 1.0);
        }

        let centroid = self.centroid(mesh);
        let mut normal = Vector3::zero();

        // Newell's method para calcular normal de polígono
        for i in 0..self.vertices.len() {
            let v0 = self.vertices[i];
            let v1 = self.vertices[(i + 1) % self.vertices.len()];
            let p0 = mesh.vertices[v0].position - centroid;
            let p1 = mesh.vertices[v1].position - centroid;
            normal += p0.cross(&p1);
        }

        normal.try_normalize().unwrap_or(Vector3::new(0.0, 0.0, 1.0))
    }

    /// Número de vértices/aristas en el loop
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Verifica si el loop está vacío
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

/// Encuentra todos los boundary loops en la malla
///
/// Un boundary loop es una secuencia cerrada de aristas de borde (sin twin).
/// Cada loop representa un agujero en la malla.
///
/// # Algoritmo
///
/// 1. Recopilar todas las aristas sin twin
/// 2. Para cada arista no visitada, seguir las aristas hasta cerrar el ciclo
/// 3. Reconstruir la secuencia de vértices
///
/// Complejidad: O(E) donde E es el número de aristas
pub fn find_boundary_loops(mesh: &Mesh) -> Vec<BoundaryLoop> {
    let mut loops = Vec::new();
    let mut visited = vec![false; mesh.edges.len()];

    // Encontrar todas las aristas de borde
    let boundary_edges: Vec<usize> = mesh.edges.iter()
        .enumerate()
        .filter(|(_, e)| e.twin.is_none())
        .map(|(i, _)| i)
        .collect();

    if boundary_edges.is_empty() {
        return loops;
    }

    // Construir mapa de siguiente arista de borde para cada vértice
    // Para aristas de borde, necesitamos encontrar la siguiente arista de borde
    // que sale del vértice de destino
    let mut next_boundary: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();

    for &edge_idx in &boundary_edges {
        let dest_vertex = mesh.edges[edge_idx].vertex;
        // Buscar la siguiente arista de borde que parte de dest_vertex
        for &other_edge in &boundary_edges {
            let origin = mesh.get_edge_origin(other_edge);
            if origin == dest_vertex {
                next_boundary.insert(edge_idx, other_edge);
                break;
            }
        }
    }

    // Recorrer y formar loops
    for &start_edge in &boundary_edges {
        if visited[start_edge] {
            continue;
        }

        let mut loop_edges = Vec::new();
        let mut loop_vertices = Vec::new();
        let mut current = start_edge;

        loop {
            if visited[current] {
                break;
            }
            visited[current] = true;
            loop_edges.push(current);
            loop_vertices.push(mesh.get_edge_origin(current));

            match next_boundary.get(&current) {
                Some(&next) => {
                    if next == start_edge {
                        break; // Loop cerrado
                    }
                    current = next;
                }
                None => break, // No hay siguiente (no debería pasar en mallas bien formadas)
            }
        }

        if !loop_edges.is_empty() {
            loops.push(BoundaryLoop {
                edges: loop_edges,
                vertices: loop_vertices,
            });
        }
    }

    loops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_triangle() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        Mesh::from_triangles(&vertices, &faces)
    }

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
    fn test_single_triangle_has_boundary() {
        let mesh = make_triangle();
        let loops = find_boundary_loops(&mesh);

        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].vertices.len(), 3);
    }

    #[test]
    fn test_closed_mesh_no_boundary() {
        let mesh = make_tetrahedron();
        let loops = find_boundary_loops(&mesh);

        assert!(loops.is_empty());
    }

    #[test]
    fn test_boundary_loop_perimeter() {
        let mesh = make_triangle();
        let loops = find_boundary_loops(&mesh);

        assert_eq!(loops.len(), 1);
        let perimeter = loops[0].perimeter(&mesh);
        // Triángulo con lados ~1, ~1, ~sqrt(2)
        assert!(perimeter > 2.0);
    }

    #[test]
    fn test_boundary_loop_centroid() {
        let mesh = make_triangle();
        let loops = find_boundary_loops(&mesh);

        let centroid = loops[0].centroid(&mesh);
        // Centroide del triángulo
        assert!((centroid.x() - 0.5).abs() < 0.1);
        assert!((centroid.y() - 0.333).abs() < 0.1);
    }

    #[test]
    fn test_boundary_loop_normal() {
        let mesh = make_triangle();
        let loops = find_boundary_loops(&mesh);

        let normal = loops[0].normal(&mesh);
        // Normal debería apuntar en Z (triángulo en plano XY)
        assert!(normal.z().abs() > 0.9);
    }
}
