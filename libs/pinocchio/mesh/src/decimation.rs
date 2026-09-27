//! Decimación de mallas por vertex clustering
//!
//! Implementa un algoritmo simple pero eficiente para reducir el número
//! de triángulos en una malla. Ideal para preprocesar mallas muy grandes
//! antes de operaciones costosas como cálculo de campo de distancias.

use crate::Mesh;
use pinocchio_math::{Real, Vector3};
use std::collections::HashMap;

/// Decima una malla reduciendo el número de vértices
///
/// # Argumentos
/// * `mesh` - Malla original
/// * `target_ratio` - Ratio de reducción (0.0-1.0), ej: 0.1 = 10% de los vértices originales
///
/// # Retorna
/// Nueva malla decimada
pub fn decimate(mesh: &Mesh, target_ratio: Real) -> Mesh {
    let target_ratio = target_ratio.clamp(0.01, 1.0);

    // Calcular resolución de grilla basada en el ratio objetivo
    let num_verts = mesh.num_vertices() as Real;
    let target_verts = (num_verts * target_ratio).max(4.0);

    // Resolución de grilla: apuntar a ~target_verts celdas ocupadas
    // Asumiendo distribución uniforme, usamos cbrt para 3D
    let resolution = (target_verts.cbrt() * 1.5).ceil() as usize;
    let resolution = resolution.max(2);

    decimate_with_resolution(mesh, resolution)
}

/// Decima una malla usando una grilla de resolución específica
pub fn decimate_with_resolution(mesh: &Mesh, resolution: usize) -> Mesh {
    let bbox = mesh.bounding_box();
    let size = bbox.size();

    // Tamaño de celda
    let cell_size = Vector3::new(
        size.x() / resolution as Real,
        size.y() / resolution as Real,
        size.z() / resolution as Real,
    );

    // Evitar división por cero
    let cell_size = Vector3::new(
        if cell_size.x() > 0.0 { cell_size.x() } else { 1.0 },
        if cell_size.y() > 0.0 { cell_size.y() } else { 1.0 },
        if cell_size.z() > 0.0 { cell_size.z() } else { 1.0 },
    );

    // Mapear vértices a celdas y acumular posiciones
    let mut cell_vertices: HashMap<(usize, usize, usize), (Vector3, usize)> = HashMap::new();
    let mut vertex_to_cell: Vec<(usize, usize, usize)> = Vec::with_capacity(mesh.num_vertices());

    for v in &mesh.vertices {
        let local = v.position - bbox.min;
        let cx = ((local.x() / cell_size.x()) as usize).min(resolution - 1);
        let cy = ((local.y() / cell_size.y()) as usize).min(resolution - 1);
        let cz = ((local.z() / cell_size.z()) as usize).min(resolution - 1);

        let cell = (cx, cy, cz);
        vertex_to_cell.push(cell);

        let entry = cell_vertices.entry(cell).or_insert((Vector3::zero(), 0));
        entry.0 += v.position;
        entry.1 += 1;
    }

    // Crear nuevos vértices (centroides de cada celda)
    let mut cell_to_new_idx: HashMap<(usize, usize, usize), usize> = HashMap::new();
    let mut new_positions: Vec<Vector3> = Vec::new();

    for (cell, (sum, count)) in &cell_vertices {
        let centroid = *sum * (1.0 / *count as Real);
        cell_to_new_idx.insert(*cell, new_positions.len());
        new_positions.push(centroid);
    }

    // Mapear índices originales a nuevos índices
    let vertex_map: Vec<usize> = vertex_to_cell.iter()
        .map(|cell| cell_to_new_idx[cell])
        .collect();

    // Reconstruir triángulos (eliminar degenerados)
    let mut new_indices: Vec<[usize; 3]> = Vec::new();

    for face_idx in 0..mesh.num_faces() {
        let old_verts = mesh.get_face_vertices(face_idx);
        let new_verts = [
            vertex_map[old_verts[0]],
            vertex_map[old_verts[1]],
            vertex_map[old_verts[2]],
        ];

        // Solo añadir si los 3 vértices son distintos (no degenerado)
        if new_verts[0] != new_verts[1] && new_verts[1] != new_verts[2] && new_verts[0] != new_verts[2] {
            new_indices.push(new_verts);
        }
    }

    Mesh::from_triangles(&new_positions, &new_indices)
}

/// Estima la resolución de grilla necesaria para un ratio de reducción dado
#[allow(dead_code)]
pub fn estimate_resolution(mesh: &Mesh, target_ratio: Real) -> usize {
    let target_ratio = target_ratio.clamp(0.01, 1.0);
    let num_verts = mesh.num_vertices() as Real;
    let target_verts = (num_verts * target_ratio).max(4.0);
    let resolution = (target_verts.cbrt() * 1.5).ceil() as usize;
    resolution.max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cube() -> Mesh {
        let positions = vec![
            // Frente
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(0.0, 1.0, 1.0),
            // Atrás
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let indices = vec![
            // Frente
            [0, 1, 2], [0, 2, 3],
            // Atrás
            [5, 4, 7], [5, 7, 6],
            // Derecha
            [1, 5, 6], [1, 6, 2],
            // Izquierda
            [4, 0, 3], [4, 3, 7],
            // Arriba
            [3, 2, 6], [3, 6, 7],
            // Abajo
            [4, 5, 1], [4, 1, 0],
        ];
        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_decimate_preserves_basic_structure() {
        let mesh = create_test_cube();
        let decimated = decimate(&mesh, 1.0); // Sin reducción real

        // Debe tener al menos algunos vértices y caras
        assert!(decimated.num_vertices() > 0);
        assert!(decimated.num_faces() > 0);
    }

    #[test]
    fn test_decimate_reduces_vertices() {
        // Crear una malla más grande para probar la reducción
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        // Crear una grilla de puntos
        for i in 0..10 {
            for j in 0..10 {
                positions.push(Vector3::new(i as Real * 0.1, j as Real * 0.1, 0.0));
            }
        }

        // Crear triángulos
        for i in 0..9 {
            for j in 0..9 {
                let idx = i * 10 + j;
                indices.push([idx, idx + 1, idx + 10]);
                indices.push([idx + 1, idx + 11, idx + 10]);
            }
        }

        let mesh = Mesh::from_triangles(&positions, &indices);
        let decimated = decimate(&mesh, 0.25);

        assert!(decimated.num_vertices() < mesh.num_vertices());
    }

    #[test]
    fn test_decimate_with_resolution() {
        let mesh = create_test_cube();
        let decimated = decimate_with_resolution(&mesh, 2);

        // Con resolución 2, máximo 8 vértices (2^3)
        assert!(decimated.num_vertices() <= 8);
    }

    #[test]
    fn test_estimate_resolution() {
        let mesh = create_test_cube();
        let res = estimate_resolution(&mesh, 0.5);
        assert!(res >= 2);
    }
}
