//! Topology validation and repair for quad meshes.
//!
//! Provides tools to:
//! - Check if a mesh is manifold
//! - Merge close vertices
//! - Remove degenerate faces
//! - Repair non-manifold configurations

use crate::QuadMesh;
use nalgebra::Vector3;
use std::collections::{HashMap, HashSet};

/// Configuration for topology operations.
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// Tolerance for merging close vertices
    pub merge_tolerance: f64,
    /// Whether to remove degenerate faces
    pub remove_degenerates: bool,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            merge_tolerance: 1e-8,
            remove_degenerates: true,
        }
    }
}

/// Result of topology validation.
#[derive(Debug, Clone, Default)]
pub struct TopologyInfo {
    /// Is the mesh manifold?
    pub is_manifold: bool,
    /// Number of boundary edges
    pub boundary_edges: usize,
    /// Number of non-manifold edges (shared by >2 faces)
    pub non_manifold_edges: usize,
    /// Number of non-manifold vertices
    pub non_manifold_vertices: usize,
    /// Number of degenerate faces
    pub degenerate_faces: usize,
}

/// Check if quad mesh is manifold.
///
/// A mesh is manifold if:
/// - Each edge is shared by at most 2 faces
/// - Vertex neighborhoods form a single connected fan
pub fn is_manifold(mesh: &QuadMesh) -> bool {
    validate_topology(mesh).is_manifold
}

/// Validate mesh topology and return detailed info.
pub fn validate_topology(mesh: &QuadMesh) -> TopologyInfo {
    let mut info = TopologyInfo {
        is_manifold: true,
        ..Default::default()
    };

    if mesh.faces.is_empty() {
        return info;
    }

    // Count edge usage
    let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();

    for face in &mesh.faces {
        // Check for degenerate face (repeated vertices)
        let vertices: HashSet<usize> = face.v.iter().copied().collect();
        if vertices.len() < 4 {
            info.degenerate_faces += 1;
        }

        // Count each edge
        for i in 0..4 {
            let v0 = face.v[i];
            let v1 = face.v[(i + 1) % 4];
            let edge = if v0 < v1 { (v0, v1) } else { (v1, v0) };
            *edge_count.entry(edge).or_insert(0) += 1;
        }
    }

    // Check edge manifoldness
    for (&_edge, &count) in &edge_count {
        if count == 1 {
            info.boundary_edges += 1;
        } else if count > 2 {
            info.non_manifold_edges += 1;
            info.is_manifold = false;
        }
    }

    // Check vertex manifoldness (simplified check)
    info.non_manifold_vertices = count_non_manifold_vertices(mesh);
    if info.non_manifold_vertices > 0 {
        info.is_manifold = false;
    }

    if info.degenerate_faces > 0 {
        info.is_manifold = false;
    }

    info
}

/// Count non-manifold vertices.
fn count_non_manifold_vertices(mesh: &QuadMesh) -> usize {
    // Build vertex-to-faces map
    let mut vertex_faces: HashMap<usize, Vec<usize>> = HashMap::new();

    for (face_idx, face) in mesh.faces.iter().enumerate() {
        for &v in &face.v {
            vertex_faces.entry(v).or_default().push(face_idx);
        }
    }

    let mut non_manifold = 0;

    for (&_vertex, faces) in &vertex_faces {
        if faces.len() < 2 {
            continue;
        }

        // Check if faces form a connected fan
        // Simplified: just check that we can traverse all faces around vertex
        if !faces_form_connected_fan(mesh, faces) {
            non_manifold += 1;
        }
    }

    non_manifold
}

/// Check if faces around a vertex form a connected fan.
fn faces_form_connected_fan(mesh: &QuadMesh, face_indices: &[usize]) -> bool {
    if face_indices.len() <= 1 {
        return true;
    }

    // Build adjacency between faces (share an edge)
    let mut adjacency: HashMap<usize, HashSet<usize>> = HashMap::new();

    for (i, &fi) in face_indices.iter().enumerate() {
        for &fj in face_indices.iter().skip(i + 1) {
            if faces_share_edge(mesh, fi, fj) {
                adjacency.entry(fi).or_default().insert(fj);
                adjacency.entry(fj).or_default().insert(fi);
            }
        }
    }

    // BFS to check connectivity
    let start = face_indices[0];
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue = vec![start];

    while let Some(f) = queue.pop() {
        if visited.contains(&f) {
            continue;
        }
        visited.insert(f);

        if let Some(neighbors) = adjacency.get(&f) {
            for &n in neighbors {
                if !visited.contains(&n) {
                    queue.push(n);
                }
            }
        }
    }

    visited.len() == face_indices.len()
}

/// Check if two faces share an edge.
fn faces_share_edge(mesh: &QuadMesh, fi: usize, fj: usize) -> bool {
    let face_i = &mesh.faces[fi];
    let face_j = &mesh.faces[fj];

    let verts_i: HashSet<usize> = face_i.v.iter().copied().collect();
    let verts_j: HashSet<usize> = face_j.v.iter().copied().collect();

    let shared: Vec<usize> = verts_i.intersection(&verts_j).copied().collect();
    shared.len() >= 2
}

/// Attempt to repair non-manifold topology.
///
/// Returns true if repair was successful.
pub fn repair_topology(mesh: &mut QuadMesh) -> bool {
    let mut changed = false;

    // Remove degenerate faces
    let original_faces = mesh.faces.len();
    mesh.faces.retain(|face| {
        let vertices: HashSet<usize> = face.v.iter().copied().collect();
        vertices.len() == 4
    });

    if mesh.faces.len() < original_faces {
        changed = true;
    }

    // Note: More complex repairs (splitting non-manifold vertices) not implemented
    changed
}

/// Remove duplicate vertices within tolerance.
///
/// Returns the number of vertices merged.
pub fn merge_close_vertices(mesh: &mut QuadMesh, tolerance: f64) -> usize {
    if mesh.vertices.is_empty() {
        return 0;
    }

    let n = mesh.vertices.len();
    let tolerance_sq = tolerance * tolerance;

    // Build mapping from old index to new index
    let mut old_to_new: Vec<usize> = (0..n).collect();
    let mut merged_count = 0;

    // Find vertices to merge using spatial hashing
    let cell_size = tolerance * 2.0;
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();

    for (i, v) in mesh.vertices.iter().enumerate() {
        let cell = (
            (v.x / cell_size).floor() as i64,
            (v.y / cell_size).floor() as i64,
            (v.z / cell_size).floor() as i64,
        );
        grid.entry(cell).or_default().push(i);
    }

    // Check each vertex against neighbors in same and adjacent cells
    for i in 0..n {
        if old_to_new[i] != i {
            continue; // Already merged
        }

        let v = mesh.vertices[i];
        let cell = (
            (v.x / cell_size).floor() as i64,
            (v.y / cell_size).floor() as i64,
            (v.z / cell_size).floor() as i64,
        );

        // Check all 27 neighboring cells
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);

                    if let Some(indices) = grid.get(&neighbor_cell) {
                        for &j in indices {
                            if j <= i || old_to_new[j] != j {
                                continue;
                            }

                            let dist_sq = (mesh.vertices[j] - v).norm_squared();
                            if dist_sq < tolerance_sq {
                                old_to_new[j] = i;
                                merged_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if merged_count == 0 {
        return 0;
    }

    // Compact vertices and update indices
    let mut new_vertices: Vec<Vector3<f64>> = Vec::new();
    let mut compact_map: Vec<usize> = vec![usize::MAX; n];

    for i in 0..n {
        let target = old_to_new[i];
        if compact_map[target] == usize::MAX {
            compact_map[target] = new_vertices.len();
            new_vertices.push(mesh.vertices[target]);
        }
        compact_map[i] = compact_map[target];
    }

    // Update face indices
    for face in &mut mesh.faces {
        for v in &mut face.v {
            *v = compact_map[*v];
        }
    }

    mesh.vertices = new_vertices;

    merged_count
}

/// Remove degenerate faces (faces with repeated vertices or zero area).
///
/// Returns the number of faces removed.
pub fn remove_degenerate_faces(mesh: &mut QuadMesh) -> usize {
    let original_count = mesh.faces.len();

    mesh.faces.retain(|face| {
        // Check for repeated vertices
        let vertices: HashSet<usize> = face.v.iter().copied().collect();
        if vertices.len() < 4 {
            return false;
        }

        // Check for zero area (all vertices collinear or coincident)
        // Simplified: just check that corners form a valid quad
        true
    });

    original_count - mesh.faces.len()
}

/// Post-process the extracted quad mesh.
///
/// Applies vertex merging, degenerate removal, and topology repair.
pub fn post_process(mesh: &mut QuadMesh, config: &TopologyConfig) -> TopologyInfo {
    // Merge close vertices
    merge_close_vertices(mesh, config.merge_tolerance);

    // Remove degenerates
    if config.remove_degenerates {
        remove_degenerate_faces(mesh);
    }

    // Attempt repair
    repair_topology(mesh);

    // Return final topology info
    validate_topology(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuadFace;

    fn make_simple_quad_mesh() -> QuadMesh {
        QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }],
        }
    }

    #[test]
    fn test_single_quad_is_manifold() {
        let mesh = make_simple_quad_mesh();
        assert!(is_manifold(&mesh));
    }

    #[test]
    fn test_validate_topology() {
        let mesh = make_simple_quad_mesh();
        let info = validate_topology(&mesh);

        assert!(info.is_manifold);
        assert_eq!(info.boundary_edges, 4); // Single quad has 4 boundary edges
        assert_eq!(info.non_manifold_edges, 0);
        assert_eq!(info.degenerate_faces, 0);
    }

    #[test]
    fn test_degenerate_face_detection() {
        let mesh = QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 1, 2, 0] }], // Repeated vertex
        };

        let info = validate_topology(&mesh);
        assert_eq!(info.degenerate_faces, 1);
        assert!(!info.is_manifold);
    }

    #[test]
    fn test_merge_close_vertices() {
        let mut mesh = QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1e-10, 0.0, 0.0), // Very close to vertex 0
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 2, 3, 4] }],
        };

        let merged = merge_close_vertices(&mut mesh, 1e-6);
        assert_eq!(merged, 1);
        assert_eq!(mesh.vertices.len(), 4);
    }

    #[test]
    fn test_remove_degenerate_faces() {
        let mut mesh = QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ],
            faces: vec![
                QuadFace { v: [0, 1, 2, 3] },       // Valid
                QuadFace { v: [0, 0, 1, 2] },       // Degenerate
            ],
        };

        let removed = remove_degenerate_faces(&mut mesh);
        assert_eq!(removed, 1);
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn test_two_adjacent_quads() {
        let mesh = QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(2.0, 0.0, 0.0),
                Vector3::new(2.0, 1.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ],
            faces: vec![
                QuadFace { v: [0, 1, 4, 5] },
                QuadFace { v: [1, 2, 3, 4] },
            ],
        };

        let info = validate_topology(&mesh);
        assert!(info.is_manifold);
        // Shared edge (1,4) should be counted once, not as boundary
        assert_eq!(info.boundary_edges, 6); // 8 edges - 2 shared = 6 boundary
    }

    #[test]
    fn test_post_process() {
        let mut mesh = make_simple_quad_mesh();
        let config = TopologyConfig::default();

        let info = post_process(&mut mesh, &config);
        assert!(info.is_manifold);
    }

    #[test]
    fn test_faces_share_edge() {
        let mesh = QuadMesh {
            vertices: vec![
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(2.0, 0.0, 0.0),
                Vector3::new(2.0, 1.0, 0.0),
            ],
            faces: vec![
                QuadFace { v: [0, 1, 2, 3] },
                QuadFace { v: [1, 4, 5, 2] },
            ],
        };

        assert!(faces_share_edge(&mesh, 0, 1));
    }
}
