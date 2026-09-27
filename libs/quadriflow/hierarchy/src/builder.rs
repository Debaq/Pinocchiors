//! Hierarchy construction via progressive decimation.
//!
//! Builds a multi-scale mesh hierarchy by repeatedly decimating the mesh
//! while tracking correspondences between levels.

use crate::{HierarchyLevel, MeshHierarchy};
use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3 as PVec3;
use std::collections::HashMap;

/// Configuration for hierarchy construction.
#[derive(Debug, Clone)]
pub struct HierarchyConfig {
    /// Target reduction ratio per level (0.5 = halve faces each level)
    pub reduction_ratio: f64,
    /// Minimum number of faces at coarsest level
    pub min_faces: usize,
    /// Maximum number of hierarchy levels
    pub max_levels: usize,
}

impl Default for HierarchyConfig {
    fn default() -> Self {
        Self {
            reduction_ratio: 0.25,
            min_faces: 100,
            max_levels: 6,
        }
    }
}

/// Build a multi-scale mesh hierarchy.
///
/// Creates progressively coarser versions of the mesh while maintaining
/// bidirectional mappings between levels.
///
/// # Arguments
/// * `mesh` - The original (finest) mesh
/// * `config` - Hierarchy configuration
///
/// # Returns
/// A mesh hierarchy with multiple levels
pub fn build_hierarchy(mesh: &Mesh, config: &HierarchyConfig) -> MeshHierarchy {
    let mut levels = Vec::new();

    // Level 0 is the original mesh
    levels.push(HierarchyLevel::new_base(mesh.clone()));

    let mut current_mesh = mesh.clone();
    let mut level_idx = 0;

    while level_idx < config.max_levels - 1 {
        let current_faces = current_mesh.num_faces();

        // Check if we've reached minimum size
        if current_faces <= config.min_faces {
            break;
        }

        // Decimate to next level
        let (coarse_mesh, vertex_map, face_map) =
            decimate_with_mapping(&current_mesh, config.reduction_ratio);

        // Check if decimation actually reduced the mesh
        if coarse_mesh.num_faces() >= current_faces || coarse_mesh.num_faces() < 4 {
            break;
        }

        // Build the coarse level with mappings
        let coarse_level = build_coarse_level(
            coarse_mesh.clone(),
            &vertex_map,
            &face_map,
            current_mesh.num_vertices(),
            current_mesh.num_faces(),
        );

        // Update the previous level's to_finer mappings
        update_finer_mappings(&mut levels[level_idx], &vertex_map, &face_map);

        levels.push(coarse_level);
        current_mesh = coarse_mesh;
        level_idx += 1;
    }

    MeshHierarchy { levels }
}

/// Decimate a mesh while tracking vertex and face correspondences.
///
/// Returns (decimated_mesh, vertex_map, face_map) where:
/// - vertex_map[fine_vertex] = coarse_vertex (or usize::MAX if collapsed)
/// - face_map[fine_face] = coarse_face (or usize::MAX if collapsed)
fn decimate_with_mapping(
    mesh: &Mesh,
    ratio: f64,
) -> (Mesh, Vec<usize>, Vec<usize>) {
    let bbox = mesh.bounding_box();
    let size = bbox.size();

    // Calculate grid resolution based on target ratio
    let target_verts = (mesh.num_vertices() as f64 * ratio).max(4.0);
    let resolution = (target_verts.cbrt() * 1.5).ceil() as usize;
    let resolution = resolution.max(2);

    // Cell size
    let cell_size = PVec3::new(
        if size.x() > 0.0 { size.x() / resolution as f64 } else { 1.0 },
        if size.y() > 0.0 { size.y() / resolution as f64 } else { 1.0 },
        if size.z() > 0.0 { size.z() / resolution as f64 } else { 1.0 },
    );

    // Map vertices to cells
    let mut cell_vertices: HashMap<(usize, usize, usize), (PVec3, Vec<usize>)> = HashMap::new();
    let mut vertex_to_cell: Vec<(usize, usize, usize)> = Vec::with_capacity(mesh.num_vertices());

    for (v_idx, v) in mesh.vertices.iter().enumerate() {
        let local = v.position - bbox.min;
        let cx = ((local.x() / cell_size.x()) as usize).min(resolution - 1);
        let cy = ((local.y() / cell_size.y()) as usize).min(resolution - 1);
        let cz = ((local.z() / cell_size.z()) as usize).min(resolution - 1);

        let cell = (cx, cy, cz);
        vertex_to_cell.push(cell);

        let entry = cell_vertices.entry(cell).or_insert((PVec3::zero(), Vec::new()));
        entry.0 += v.position;
        entry.1.push(v_idx);
    }

    // Create new vertices (cell centroids)
    let mut cell_to_new_idx: HashMap<(usize, usize, usize), usize> = HashMap::new();
    let mut new_positions: Vec<PVec3> = Vec::new();

    for (cell, (sum, vertices)) in &cell_vertices {
        let centroid = *sum * (1.0 / vertices.len() as f64);
        cell_to_new_idx.insert(*cell, new_positions.len());
        new_positions.push(centroid);
    }

    // Build vertex map: fine_vertex -> coarse_vertex
    let vertex_map: Vec<usize> = vertex_to_cell
        .iter()
        .map(|cell| cell_to_new_idx[cell])
        .collect();

    // Build faces and face map
    let mut new_indices: Vec<[usize; 3]> = Vec::new();
    let mut face_map: Vec<usize> = vec![usize::MAX; mesh.num_faces()];

    for face_idx in 0..mesh.num_faces() {
        let old_verts = mesh.get_face_vertices(face_idx);
        let new_verts = [
            vertex_map[old_verts[0]],
            vertex_map[old_verts[1]],
            vertex_map[old_verts[2]],
        ];

        // Only add non-degenerate faces
        if new_verts[0] != new_verts[1]
            && new_verts[1] != new_verts[2]
            && new_verts[0] != new_verts[2]
        {
            face_map[face_idx] = new_indices.len();
            new_indices.push(new_verts);
        }
    }

    let new_mesh = Mesh::from_triangles(&new_positions, &new_indices);

    (new_mesh, vertex_map, face_map)
}

/// Build a coarse hierarchy level with proper mappings.
fn build_coarse_level(
    mesh: Mesh,
    vertex_map: &[usize],
    face_map: &[usize],
    _fine_num_vertices: usize,
    _fine_num_faces: usize,
) -> HierarchyLevel {
    let num_coarse_faces = mesh.num_faces();
    let num_coarse_vertices = mesh.num_vertices();

    // Build vertex_to_finer: for each coarse vertex, which fine vertices map to it
    let mut vertex_to_finer: Vec<Vec<usize>> = vec![Vec::new(); num_coarse_vertices];
    for (fine_v, &coarse_v) in vertex_map.iter().enumerate() {
        if coarse_v < num_coarse_vertices {
            vertex_to_finer[coarse_v].push(fine_v);
        }
    }

    // Build face_to_finer: for each coarse face, which fine faces map to it
    let mut face_to_finer: Vec<Vec<usize>> = vec![Vec::new(); num_coarse_faces];
    for (fine_f, &coarse_f) in face_map.iter().enumerate() {
        if coarse_f < num_coarse_faces {
            face_to_finer[coarse_f].push(fine_f);
        }
    }

    // vertex_to_coarser and face_to_coarser are identity for coarse level
    // (they point to themselves since this is the coarse level looking up)
    let vertex_to_coarser: Vec<usize> = (0..num_coarse_vertices).collect();
    let face_to_coarser: Vec<usize> = (0..num_coarse_faces).collect();

    HierarchyLevel {
        mesh,
        face_to_finer,
        face_to_coarser,
        vertex_to_finer,
        vertex_to_coarser,
    }
}

/// Update the finer level's to_coarser mappings based on decimation.
fn update_finer_mappings(
    finer_level: &mut HierarchyLevel,
    vertex_map: &[usize],
    face_map: &[usize],
) {
    // Update vertex_to_coarser
    finer_level.vertex_to_coarser = vertex_map.to_vec();

    // Update face_to_coarser
    finer_level.face_to_coarser = face_map.to_vec();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid_mesh(size: usize) -> Mesh {
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        // Create a grid of vertices
        for i in 0..=size {
            for j in 0..=size {
                positions.push(PVec3::new(i as f64 * 0.1, j as f64 * 0.1, 0.0));
            }
        }

        // Create triangles
        for i in 0..size {
            for j in 0..size {
                let idx = i * (size + 1) + j;
                indices.push([idx, idx + 1, idx + size + 1]);
                indices.push([idx + 1, idx + size + 2, idx + size + 1]);
            }
        }

        Mesh::from_triangles(&positions, &indices)
    }

    fn make_tetrahedron() -> Mesh {
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.5, 0.866, 0.0),
            PVec3::new(0.5, 0.289, 0.816),
        ];
        let faces = vec![[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_build_hierarchy_small_mesh() {
        let mesh = make_tetrahedron();
        let config = HierarchyConfig {
            min_faces: 2,
            ..Default::default()
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        // Small mesh might not create many levels
        assert!(hierarchy.depth() >= 1);
        assert_eq!(hierarchy.finest().unwrap().num_faces(), 4);
    }

    #[test]
    fn test_build_hierarchy_creates_multiple_levels() {
        let mesh = make_grid_mesh(10); // 200 triangles
        let config = HierarchyConfig {
            reduction_ratio: 0.25,
            min_faces: 10,
            max_levels: 4,
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        // Should have multiple levels
        assert!(hierarchy.depth() >= 2);

        // Each level should be coarser
        for i in 1..hierarchy.depth() {
            let finer = hierarchy.level(i - 1).unwrap();
            let coarser = hierarchy.level(i).unwrap();
            assert!(coarser.num_faces() < finer.num_faces());
        }
    }

    #[test]
    fn test_hierarchy_mappings_valid() {
        let mesh = make_grid_mesh(8);
        let config = HierarchyConfig {
            reduction_ratio: 0.25,
            min_faces: 10,
            max_levels: 3,
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        if hierarchy.depth() >= 2 {
            let fine = hierarchy.level(0).unwrap();
            let coarse = hierarchy.level(1).unwrap();

            // Check face_to_coarser validity
            for &coarse_f in fine.face_to_coarser.iter() {
                // Either maps to a valid coarse face or is collapsed (usize::MAX)
                assert!(coarse_f == usize::MAX || coarse_f < coarse.num_faces());
            }

            // Check vertex_to_coarser validity
            for &coarse_v in &fine.vertex_to_coarser {
                assert!(coarse_v < coarse.num_vertices());
            }
        }
    }

    #[test]
    fn test_face_to_finer_inverse() {
        let mesh = make_grid_mesh(6);
        let config = HierarchyConfig {
            reduction_ratio: 0.25,
            min_faces: 5,
            max_levels: 3,
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        if hierarchy.depth() >= 2 {
            let fine = hierarchy.level(0).unwrap();
            let coarse = hierarchy.level(1).unwrap();

            // face_to_finer should be inverse of face_to_coarser
            for (coarse_f, fine_faces) in coarse.face_to_finer.iter().enumerate() {
                for &fine_f in fine_faces {
                    assert_eq!(fine.face_to_coarser[fine_f], coarse_f);
                }
            }
        }
    }
}
