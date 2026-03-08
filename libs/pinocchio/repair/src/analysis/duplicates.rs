//! Detección de vértices duplicados usando spatial hashing

use pinocchio_mesh::Mesh;
use pinocchio_math::Real;
use serde::Serialize;
use std::collections::HashMap;

/// Grupo de vértices duplicados
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// Índices de los vértices duplicados
    pub indices: Vec<usize>,
    /// Posición promedio del grupo (no serializable)
    #[serde(skip)]
    pub position: pinocchio_math::Vector3,
}

impl DuplicateGroup {
    /// El vértice representante (el de menor índice)
    pub fn representative(&self) -> usize {
        *self.indices.first().unwrap_or(&0)
    }

    /// Número de duplicados (excluyendo el representante)
    pub fn num_duplicates(&self) -> usize {
        self.indices.len().saturating_sub(1)
    }
}

/// Encuentra grupos de vértices duplicados usando spatial hashing
///
/// Dos vértices se consideran duplicados si su distancia es menor que `tolerance`.
///
/// # Algoritmo
///
/// Usa spatial hashing con celdas de tamaño `tolerance` para agrupar vértices
/// cercanos eficientemente. Solo compara vértices dentro de la misma celda y
/// celdas adyacentes.
///
/// Complejidad: O(V) promedio, O(V²) peor caso
///
/// # Argumentos
///
/// * `mesh` - La malla a analizar
/// * `tolerance` - Distancia máxima para considerar dos vértices como duplicados
pub fn find_duplicate_vertices(mesh: &Mesh, tolerance: Real) -> Vec<DuplicateGroup> {
    if mesh.num_vertices() == 0 {
        return Vec::new();
    }

    let tolerance_sq = tolerance * tolerance;

    // Spatial hash: celda -> lista de índices de vértices
    let cell_size = tolerance.max(1e-10);
    let mut spatial_hash: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();

    // Insertar vértices en el hash espacial
    for (i, vertex) in mesh.vertices.iter().enumerate() {
        let cell = position_to_cell(vertex.position, cell_size);
        spatial_hash.entry(cell).or_default().push(i);
    }

    // Union-Find para agrupar duplicados
    let mut parent: Vec<usize> = (0..mesh.num_vertices()).collect();

    // Función para encontrar el representante (con path compression)
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    // Función para unir dos conjuntos
    fn union(parent: &mut [usize], i: usize, j: usize) {
        let pi = find(parent, i);
        let pj = find(parent, j);
        if pi != pj {
            // Unir al de menor índice
            if pi < pj {
                parent[pj] = pi;
            } else {
                parent[pi] = pj;
            }
        }
    }

    // Comparar vértices en celdas adyacentes
    for (&cell, indices) in &spatial_hash {
        // Comparar dentro de la misma celda
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let vi = indices[i];
                let vj = indices[j];
                let dist_sq = mesh.vertices[vi].position
                    .distance_squared(&mesh.vertices[vj].position);
                if dist_sq <= tolerance_sq {
                    union(&mut parent, vi, vj);
                }
            }
        }

        // Comparar con celdas adyacentes (solo en una dirección para evitar duplicados)
        for dx in 0..=1_i64 {
            for dy in 0..=1_i64 {
                for dz in 0..=1_i64 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    if let Some(neighbor_indices) = spatial_hash.get(&neighbor_cell) {
                        for &vi in indices {
                            for &vj in neighbor_indices {
                                let dist_sq = mesh.vertices[vi].position
                                    .distance_squared(&mesh.vertices[vj].position);
                                if dist_sq <= tolerance_sq {
                                    union(&mut parent, vi, vj);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Agrupar por representante
    let mut groups_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..mesh.num_vertices() {
        let root = find(&mut parent, i);
        groups_map.entry(root).or_default().push(i);
    }

    // Filtrar grupos con más de un vértice y calcular posición promedio
    groups_map
        .into_values()
        .filter(|indices| indices.len() > 1)
        .map(|indices| {
            let sum: pinocchio_math::Vector3 = indices.iter()
                .map(|&i| mesh.vertices[i].position)
                .fold(pinocchio_math::Vector3::zero(), |acc, p| acc + p);
            let position = sum * (1.0 / indices.len() as f64);
            DuplicateGroup { indices, position }
        })
        .collect()
}

/// Convierte una posición 3D a una celda del hash espacial
fn position_to_cell(pos: pinocchio_math::Vector3, cell_size: Real) -> (i64, i64, i64) {
    (
        (pos.x() / cell_size).floor() as i64,
        (pos.y() / cell_size).floor() as i64,
        (pos.z() / cell_size).floor() as i64,
    )
}

/// Crea un mapa de vértice original a vértice representante
///
/// Útil para remapear índices después de fusionar duplicados.
pub fn create_remap_table(
    num_vertices: usize,
    groups: &[DuplicateGroup],
) -> Vec<usize> {
    let mut remap: Vec<usize> = (0..num_vertices).collect();

    for group in groups {
        let rep = group.representative();
        for &idx in &group.indices {
            remap[idx] = rep;
        }
    }

    remap
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    #[test]
    fn test_no_duplicates() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let groups = find_duplicate_vertices(&mesh, 1e-6);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_exact_duplicates() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 0.0),  // Duplicado exacto
            Vector3::new(1.0, 0.0, 0.0),
        ];
        let faces = vec![[0, 2, 2]];  // Cara dummy
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let groups = find_duplicate_vertices(&mesh, 1e-6);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].indices.len(), 2);
    }

    #[test]
    fn test_near_duplicates() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1e-7, 0.0, 0.0),  // Casi duplicado
            Vector3::new(1.0, 0.0, 0.0),
        ];
        let faces = vec![[0, 2, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let groups = find_duplicate_vertices(&mesh, 1e-6);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn test_multiple_groups() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 0.0),  // Duplicado de 0
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),  // Duplicado de 2
        ];
        let faces = vec![[0, 2, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let groups = find_duplicate_vertices(&mesh, 1e-6);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_remap_table() {
        let groups = vec![
            DuplicateGroup {
                indices: vec![0, 3, 5],
                position: Vector3::zero(),
            },
            DuplicateGroup {
                indices: vec![2, 4],
                position: Vector3::zero(),
            },
        ];

        let remap = create_remap_table(6, &groups);
        assert_eq!(remap[0], 0);
        assert_eq!(remap[1], 1);  // No en grupo
        assert_eq!(remap[2], 2);
        assert_eq!(remap[3], 0);  // Mapeado a 0
        assert_eq!(remap[4], 2);  // Mapeado a 2
        assert_eq!(remap[5], 0);  // Mapeado a 0
    }
}
