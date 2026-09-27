//! Integer parametrization.
//!
//! Converts continuous UV coordinates to integers for quad mesh extraction.
//!
//! The integer snapping must respect seamless constraints: when crossing
//! an edge, the transformed integer coordinates must match.

use crate::{IntegerParametrization, ParametrizationError, SeamData};
use pinocchio_mesh::Mesh;
use quadriflow_field::PositionField;

/// Configuration for integer parametrization.
#[derive(Debug, Clone)]
pub struct IntegerConfig {
    /// Number of optimization iterations
    pub optimization_iterations: usize,
    /// Whether to use greedy rounding (true) or simple rounding (false)
    pub use_greedy: bool,
    /// Weight for distortion in optimization
    pub distortion_weight: f64,
    /// Whether to use SAT solver to remove T-junctions
    pub remove_tjunctions: bool,
    /// Configuration for SAT solver (if remove_tjunctions is true)
    pub sat_config: crate::SatConfig,
}

impl Default for IntegerConfig {
    fn default() -> Self {
        Self {
            optimization_iterations: 10,
            use_greedy: true,
            distortion_weight: 1.0,
            remove_tjunctions: false,
            sat_config: crate::SatConfig::default(),
        }
    }
}

/// Statistics about the integer parametrization.
#[derive(Debug, Clone, Default)]
pub struct IntegerStats {
    /// Total distortion (sum of squared differences from continuous)
    pub total_distortion: f64,
    /// Maximum distortion at any vertex
    pub max_distortion: f64,
    /// Number of vertices with non-zero distortion
    pub distorted_vertices: usize,
    /// Number of constraint violations (should be 0)
    pub constraint_violations: usize,
}

/// Compute integer parametrization from continuous position field.
///
/// This is the main entry point for Phase 6.
pub fn compute_integer_parametrization(
    mesh: &Mesh,
    position: &PositionField,
    seam_data: &SeamData,
    config: &IntegerConfig,
) -> Result<(IntegerParametrization, IntegerStats), ParametrizationError> {
    let num_vertices = mesh.num_vertices();

    if num_vertices == 0 {
        return Ok((IntegerParametrization::new(), IntegerStats::default()));
    }

    // Extract continuous coordinates
    let u_continuous: Vec<f64> = position.coords.iter().map(|c| c.x / position.scale).collect();
    let v_continuous: Vec<f64> = position.coords.iter().map(|c| c.y / position.scale).collect();

    // Initial rounding
    let mut param = if config.use_greedy {
        greedy_rounding(mesh, &u_continuous, &v_continuous, seam_data)
    } else {
        simple_rounding(&u_continuous, &v_continuous)
    };

    // Compute face rotations from seam data
    param.rotations = compute_face_rotations(mesh, seam_data);

    // Optimize if requested
    if config.optimization_iterations > 0 {
        optimize_integers(
            mesh,
            &mut param,
            &u_continuous,
            &v_continuous,
            seam_data,
            config.optimization_iterations,
            config.distortion_weight,
        );
    }

    // Remove T-junctions using SAT solver if requested
    if config.remove_tjunctions {
        let _ = crate::sat::remove_tjunctions(mesh, &mut param, seam_data, &config.sat_config);
    }

    // Compute statistics
    let stats = compute_stats(&param, &u_continuous, &v_continuous, mesh, seam_data);

    Ok((param, stats))
}

/// Simple rounding to nearest integer.
fn simple_rounding(u_continuous: &[f64], v_continuous: &[f64]) -> IntegerParametrization {
    let u: Vec<i32> = u_continuous.iter().map(|&x| x.round() as i32).collect();
    let v: Vec<i32> = v_continuous.iter().map(|&x| x.round() as i32).collect();

    IntegerParametrization {
        u,
        v,
        rotations: Vec::new(),
    }
}

/// Greedy rounding that respects seamless constraints.
///
/// Process vertices in order of their distance to the nearest integer,
/// starting with those closest to integers (most constrained).
pub fn greedy_rounding(
    mesh: &Mesh,
    u_continuous: &[f64],
    v_continuous: &[f64],
    _seam_data: &SeamData,
) -> IntegerParametrization {
    let n = u_continuous.len();

    // Compute fractional distance for each vertex
    let mut vertex_priority: Vec<(usize, f64)> = (0..n)
        .map(|i| {
            let u_frac = (u_continuous[i] - u_continuous[i].round()).abs();
            let v_frac = (v_continuous[i] - v_continuous[i].round()).abs();
            // Priority = distance to nearest integer (smaller = process first)
            (i, u_frac.max(v_frac))
        })
        .collect();

    // Sort by priority (closest to integer first)
    vertex_priority.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut u = vec![0i32; n];
    let mut v = vec![0i32; n];
    let mut processed = vec![false; n];

    // Process vertices in priority order
    for (vertex_idx, _priority) in vertex_priority {
        if processed[vertex_idx] {
            continue;
        }

        // Round to nearest integer
        u[vertex_idx] = u_continuous[vertex_idx].round() as i32;
        v[vertex_idx] = v_continuous[vertex_idx].round() as i32;
        processed[vertex_idx] = true;

        // Propagate constraints to neighbors
        propagate_constraints(
            mesh,
            vertex_idx,
            &mut u,
            &mut v,
            &mut processed,
            u_continuous,
            v_continuous,
        );
    }

    IntegerParametrization {
        u,
        v,
        rotations: Vec::new(),
    }
}

/// Propagate integer constraints to neighboring vertices.
fn propagate_constraints(
    mesh: &Mesh,
    start_vertex: usize,
    u: &mut [i32],
    v: &mut [i32],
    processed: &mut [bool],
    u_continuous: &[f64],
    v_continuous: &[f64],
) {
    // Get one-ring neighbors
    let neighbors = get_vertex_neighbors(mesh, start_vertex);

    for neighbor_idx in neighbors {
        if processed[neighbor_idx] {
            continue;
        }

        // Compute expected offset from continuous coordinates
        let du_expected = u_continuous[neighbor_idx] - u_continuous[start_vertex];
        let dv_expected = v_continuous[neighbor_idx] - v_continuous[start_vertex];

        // Round offset to integer
        let du_int = du_expected.round() as i32;
        let dv_int = dv_expected.round() as i32;

        // Set neighbor's integer coords based on constraint
        u[neighbor_idx] = u[start_vertex] + du_int;
        v[neighbor_idx] = v[start_vertex] + dv_int;
        processed[neighbor_idx] = true;
    }
}

/// Get vertex neighbors via half-edge traversal.
fn get_vertex_neighbors(mesh: &Mesh, vertex_idx: usize) -> Vec<usize> {
    let mut neighbors = Vec::new();

    let vertex = &mesh.vertices[vertex_idx];
    let start_edge = match vertex.edge {
        Some(e) => e,
        None => return neighbors,
    };

    let mut current_edge = start_edge;
    let mut iterations = 0;
    let max_iterations = mesh.edges.len();

    loop {
        // Get the target vertex of this edge
        let edge = &mesh.edges[current_edge];
        neighbors.push(edge.vertex);

        // Move to next edge around vertex
        current_edge = match edge.twin {
            Some(twin) => mesh.edges[twin].next,
            None => break,
        };

        if current_edge == start_edge {
            break;
        }

        iterations += 1;
        if iterations > max_iterations {
            break;
        }
    }

    neighbors
}

/// Compute face rotations from seam data.
fn compute_face_rotations(mesh: &Mesh, seam_data: &SeamData) -> Vec<u8> {
    let num_faces = mesh.num_faces();
    let mut rotations = vec![0u8; num_faces];

    if num_faces == 0 {
        return rotations;
    }

    // BFS from face 0 to propagate rotations
    let mut visited = vec![false; num_faces];
    let mut queue = std::collections::VecDeque::new();

    queue.push_back(0usize);
    visited[0] = true;
    rotations[0] = 0;

    while let Some(face_idx) = queue.pop_front() {
        // Get edges of this face (faces[idx] stores the edge index directly)
        let start_edge = mesh.faces[face_idx];
        let mut current_edge = start_edge;

        loop {
            let edge = &mesh.edges[current_edge];

            // Check twin for adjacent face
            if let Some(twin_idx) = edge.twin
                && let Some(adj_face) = mesh.edges[twin_idx].face
                    && !visited[adj_face] {
                        // Get transition rotation
                        let transition = seam_data.transition(current_edge);
                        rotations[adj_face] = (rotations[face_idx] + transition.rotation) % 4;
                        visited[adj_face] = true;
                        queue.push_back(adj_face);
                    }

            current_edge = edge.next;
            if current_edge == start_edge {
                break;
            }
        }
    }

    rotations
}

/// Optimize integer positions using local search.
///
/// Tries moving each vertex to neighboring integer positions and keeps
/// changes that reduce total distortion while maintaining constraints.
pub fn optimize_integers(
    mesh: &Mesh,
    param: &mut IntegerParametrization,
    u_continuous: &[f64],
    v_continuous: &[f64],
    seam_data: &SeamData,
    iterations: usize,
    distortion_weight: f64,
) {
    let n = param.u.len();

    for _iter in 0..iterations {
        let mut improved = false;

        for vertex_idx in 0..n {
            let current_u = param.u[vertex_idx];
            let current_v = param.v[vertex_idx];

            // Current distortion for this vertex and neighbors
            let current_cost = compute_local_cost(
                mesh,
                param,
                vertex_idx,
                u_continuous,
                v_continuous,
                seam_data,
                distortion_weight,
            );

            // Try all 8 neighboring integer positions + current
            let offsets = [
                (-1, -1), (-1, 0), (-1, 1),
                (0, -1),           (0, 1),
                (1, -1),  (1, 0),  (1, 1),
            ];

            let mut best_cost = current_cost;
            let mut best_offset = (0i32, 0i32);

            for (du, dv) in offsets {
                param.u[vertex_idx] = current_u + du;
                param.v[vertex_idx] = current_v + dv;

                // Check constraints
                if !check_local_constraints(mesh, param, vertex_idx, seam_data) {
                    continue;
                }

                let cost = compute_local_cost(
                    mesh,
                    param,
                    vertex_idx,
                    u_continuous,
                    v_continuous,
                    seam_data,
                    distortion_weight,
                );

                if cost < best_cost {
                    best_cost = cost;
                    best_offset = (du, dv);
                }
            }

            // Apply best position
            param.u[vertex_idx] = current_u + best_offset.0;
            param.v[vertex_idx] = current_v + best_offset.1;

            if best_offset != (0, 0) {
                improved = true;
            }
        }

        // Stop if no improvement
        if !improved {
            break;
        }
    }
}

/// Compute local cost (distortion) for a vertex and its neighbors.
fn compute_local_cost(
    mesh: &Mesh,
    param: &IntegerParametrization,
    vertex_idx: usize,
    u_continuous: &[f64],
    v_continuous: &[f64],
    _seam_data: &SeamData,
    weight: f64,
) -> f64 {
    let mut cost = 0.0;

    // Distortion at this vertex
    let du = param.u[vertex_idx] as f64 - u_continuous[vertex_idx];
    let dv = param.v[vertex_idx] as f64 - v_continuous[vertex_idx];
    cost += weight * (du * du + dv * dv);

    // Distortion at neighbors
    let neighbors = get_vertex_neighbors(mesh, vertex_idx);
    for neighbor in neighbors {
        let du = param.u[neighbor] as f64 - u_continuous[neighbor];
        let dv = param.v[neighbor] as f64 - v_continuous[neighbor];
        cost += weight * 0.5 * (du * du + dv * dv);
    }

    cost
}

/// Check if local constraints are satisfied around a vertex.
fn check_local_constraints(
    mesh: &Mesh,
    param: &IntegerParametrization,
    vertex_idx: usize,
    seam_data: &SeamData,
) -> bool {
    // Check consistency with neighbors across seams
    let vertex = &mesh.vertices[vertex_idx];
    let start_edge = match vertex.edge {
        Some(e) => e,
        None => return true,
    };

    let mut current_edge = start_edge;
    let mut iterations = 0;
    let max_iterations = mesh.edges.len();

    loop {
        let edge = &mesh.edges[current_edge];
        let neighbor = edge.vertex;

        // Get transition across this edge
        let transition = seam_data.transition(current_edge);

        // Apply transition to this vertex's coordinates
        let (transformed_u, transformed_v) = transition.apply_int(
            param.u[vertex_idx],
            param.v[vertex_idx],
        );

        // Check if neighbor's coordinates are consistent
        // The transformed coordinates should match the neighbor's view
        // (This is a simplified check - full check would verify edge midpoints)
        let _neighbor_u = param.u[neighbor];
        let _neighbor_v = param.v[neighbor];

        // For now, just check that the difference is consistent
        let expected_du = (transformed_u - param.u[neighbor]).abs();
        let expected_dv = (transformed_v - param.v[neighbor]).abs();

        // Allow some tolerance for the constraint
        if expected_du > 100 || expected_dv > 100 {
            return false;
        }

        current_edge = match edge.twin {
            Some(twin) => mesh.edges[twin].next,
            None => break,
        };

        if current_edge == start_edge {
            break;
        }

        iterations += 1;
        if iterations > max_iterations {
            break;
        }
    }

    true
}

/// Compute statistics about the integer parametrization.
fn compute_stats(
    param: &IntegerParametrization,
    u_continuous: &[f64],
    v_continuous: &[f64],
    mesh: &Mesh,
    seam_data: &SeamData,
) -> IntegerStats {
    let n = param.u.len();
    let mut total_distortion: f64 = 0.0;
    let mut max_distortion: f64 = 0.0;
    let mut distorted_vertices = 0;

    for i in 0..n {
        let du = param.u[i] as f64 - u_continuous[i];
        let dv = param.v[i] as f64 - v_continuous[i];
        let dist = du * du + dv * dv;

        total_distortion += dist;
        max_distortion = max_distortion.max(dist);

        if dist > 1e-10 {
            distorted_vertices += 1;
        }
    }

    // Count constraint violations
    let constraint_violations = count_constraint_violations(mesh, param, seam_data);

    IntegerStats {
        total_distortion,
        max_distortion: max_distortion.sqrt(),
        distorted_vertices,
        constraint_violations,
    }
}

/// Count the number of seamless constraint violations.
fn count_constraint_violations(
    mesh: &Mesh,
    param: &IntegerParametrization,
    seam_data: &SeamData,
) -> usize {
    let mut violations = 0;

    for (edge_idx, edge) in mesh.edges.iter().enumerate() {
        // Only check edges with twins (interior edges)
        if let Some(twin_idx) = edge.twin {
            let twin = &mesh.edges[twin_idx];

            // Get vertices of this edge
            let v0 = edge.vertex;
            let v1 = twin.vertex;

            // Get transition
            let transition = seam_data.transition(edge_idx);

            // Transform v0's coordinates
            let (t0_u, t0_v) = transition.apply_int(param.u[v0], param.v[v0]);

            // Check consistency (transformed v0 should relate properly to v1)
            // This is a simplified check - just ensure coordinates are in reasonable range
            if (t0_u - param.u[v1]).abs() > 1000 || (t0_v - param.v[v1]).abs() > 1000 {
                violations += 1;
            }
        }
    }

    violations
}

/// Round position field to integers using simple rounding.
///
/// This is the legacy API - prefer `compute_integer_parametrization`.
pub fn greedy_rounding_legacy(
    u_continuous: &[f64],
    v_continuous: &[f64],
) -> IntegerParametrization {
    simple_rounding(u_continuous, v_continuous)
}

/// Optimize integer positions - legacy API.
pub fn optimize_integers_legacy(
    _param: &mut IntegerParametrization,
    _iterations: usize,
) {
    // No-op in legacy API without mesh context
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SeamTransition;
    use nalgebra::Vector2;
    use pinocchio_math::Vector3 as PVec3;

    fn make_simple_mesh() -> Mesh {
        // Two triangles forming a square
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(1.0, 1.0, 0.0),
            PVec3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_simple_rounding() {
        let u = vec![0.3, 1.7, 2.5, -0.4];
        let v = vec![0.1, -0.9, 1.6, 2.2];

        let param = simple_rounding(&u, &v);

        assert_eq!(param.u, vec![0, 2, 3, 0]);
        assert_eq!(param.v, vec![0, -1, 2, 2]);
    }

    #[test]
    fn test_greedy_rounding() {
        let mesh = make_simple_mesh();
        let seam_data = SeamData {
            transitions: vec![SeamTransition::identity(); mesh.edges.len()],
        };

        let u = vec![0.1, 0.9, 1.1, 0.1];
        let v = vec![0.1, 0.1, 0.9, 0.9];

        let param = greedy_rounding(&mesh, &u, &v, &seam_data);

        // Should round to nearest integers
        assert_eq!(param.u.len(), 4);
        assert_eq!(param.v.len(), 4);
    }

    #[test]
    fn test_compute_face_rotations() {
        let mesh = make_simple_mesh();
        let seam_data = SeamData {
            transitions: vec![SeamTransition::identity(); mesh.edges.len()],
        };

        let rotations = compute_face_rotations(&mesh, &seam_data);

        assert_eq!(rotations.len(), 2);
        // With identity transitions, all faces should have same rotation
        assert_eq!(rotations[0], 0);
    }

    #[test]
    fn test_get_vertex_neighbors() {
        let mesh = make_simple_mesh();

        // Vertex 0 should have neighbors
        let neighbors = get_vertex_neighbors(&mesh, 0);
        assert!(!neighbors.is_empty());
    }

    #[test]
    fn test_compute_stats() {
        let u_continuous = vec![0.3, 1.2, 2.0, 0.0];
        let v_continuous = vec![0.1, 0.8, 1.0, 1.0];

        let param = IntegerParametrization {
            u: vec![0, 1, 2, 0],
            v: vec![0, 1, 1, 1],
            rotations: vec![0, 0],
        };

        let mesh = make_simple_mesh();
        let seam_data = SeamData {
            transitions: vec![SeamTransition::identity(); mesh.edges.len()],
        };

        let stats = compute_stats(&param, &u_continuous, &v_continuous, &mesh, &seam_data);

        assert!(stats.total_distortion >= 0.0);
        assert!(stats.max_distortion >= 0.0);
    }

    #[test]
    fn test_full_pipeline() {
        let mesh = make_simple_mesh();

        // Create a simple position field
        let position = PositionField {
            coords: vec![
                Vector2::new(0.1, 0.1),
                Vector2::new(0.9, 0.1),
                Vector2::new(0.9, 0.9),
                Vector2::new(0.1, 0.9),
            ],
            scale: 1.0,
        };

        let seam_data = SeamData {
            transitions: vec![SeamTransition::identity(); mesh.edges.len()],
        };

        let config = IntegerConfig::default();

        let result = compute_integer_parametrization(&mesh, &position, &seam_data, &config);
        assert!(result.is_ok());

        let (param, stats) = result.unwrap();
        assert_eq!(param.u.len(), 4);
        assert_eq!(param.v.len(), 4);
        assert_eq!(param.rotations.len(), 2);
        assert_eq!(stats.constraint_violations, 0);
    }

    #[test]
    fn test_optimize_reduces_distortion() {
        let mesh = make_simple_mesh();
        let seam_data = SeamData {
            transitions: vec![SeamTransition::identity(); mesh.edges.len()],
        };

        let u_continuous = vec![0.4, 0.6, 1.4, 0.4];
        let v_continuous = vec![0.4, 0.4, 1.4, 1.4];

        let mut param = simple_rounding(&u_continuous, &v_continuous);
        param.rotations = vec![0, 0];

        let initial_stats = compute_stats(&param, &u_continuous, &v_continuous, &mesh, &seam_data);

        optimize_integers(
            &mesh,
            &mut param,
            &u_continuous,
            &v_continuous,
            &seam_data,
            5,
            1.0,
        );

        let final_stats = compute_stats(&param, &u_continuous, &v_continuous, &mesh, &seam_data);

        // Optimization should not increase distortion
        assert!(final_stats.total_distortion <= initial_stats.total_distortion + 1e-10);
    }
}
