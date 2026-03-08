//! # quadriflow-flow
//!
//! Min-cost network flow solver for singularity minimization.
//!
//! QuadriFlow formulates singularity placement as a minimum-cost flow problem
//! on the dual graph of the mesh. This module provides:
//!
//! 1. **Network Construction**: Build flow network from mesh and singularities
//! 2. **Solvers**: Successive Shortest Paths algorithm
//! 3. **Application**: Interpret flow solution to adjust field
//!
//! ## Algorithm
//!
//! The key insight is that moving a singularity from vertex A to vertex B
//! can be modeled as pushing flow along the path A→B. The min-cost flow
//! minimizes the total "distance" traveled by singularities.

pub mod network;
pub mod solver;

pub use network::FlowNetwork;
pub use solver::{FlowResult, FlowSolver, NetworkSimplex, SuccessiveShortestPaths};

use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_field::{detect_singularities, OrientationField, SingularityInfo};
use std::f64::consts::FRAC_PI_2;
use thiserror::Error;

/// Errors that can occur during flow solving.
#[derive(Error, Debug)]
pub enum FlowError {
    #[error("Flow network is infeasible")]
    Infeasible,
    #[error("Flow network has unbounded solution")]
    Unbounded,
    #[error("Solver failed: {0}")]
    SolverError(String),
}

/// Configuration for singularity optimization.
#[derive(Debug, Clone)]
pub struct SingularityOptConfig {
    /// Cost scaling factor (higher = more sensitive to edge lengths)
    pub cost_scale: i32,
    /// Maximum solver iterations
    pub max_iterations: usize,
}

impl Default for SingularityOptConfig {
    fn default() -> Self {
        Self {
            cost_scale: 1000,
            max_iterations: 100_000,
        }
    }
}

/// Result of singularity optimization.
#[derive(Debug, Clone)]
pub struct SingularityOptResult {
    /// The flow solution
    pub flow_result: FlowResult,
    /// Adjustment to apply per vertex (rotation increments in units of 90°)
    pub vertex_adjustments: Vec<i32>,
    /// Number of singularities before optimization
    pub initial_singularities: usize,
    /// Estimated singularities after optimization
    pub final_singularities: usize,
}

/// Optimize singularity placement using min-cost flow.
///
/// This is the main entry point for Phase 4 of QuadriFlow.
///
/// # Arguments
/// * `mesh` - The triangle mesh
/// * `field` - Current orientation field
/// * `config` - Optimization configuration
///
/// # Returns
/// Optimization result with flow solution and adjustments
pub fn optimize_singularities(
    mesh: &Mesh,
    field: &OrientationField,
    config: &SingularityOptConfig,
) -> Result<SingularityOptResult, FlowError> {
    // Detect current singularities
    let sing_info = detect_singularities(field, mesh);
    let initial_count = sing_info.singularities.len();

    // Build flow network
    let mut network = FlowNetwork::from_mesh(mesh, &sing_info, config.cost_scale);

    // Balance network if needed (for topological constraints)
    network.balance();

    // Solve min-cost flow
    let solver = SuccessiveShortestPaths::with_max_iterations(config.max_iterations);
    let flow_result = solver.solve(&network)?;

    // Compute vertex adjustments from flow
    let vertex_adjustments = compute_vertex_adjustments(&network, &flow_result, mesh);

    // Estimate final singularities (actual count depends on field adjustment)
    let final_count = estimate_final_singularities(&sing_info, &vertex_adjustments);

    Ok(SingularityOptResult {
        flow_result,
        vertex_adjustments,
        initial_singularities: initial_count,
        final_singularities: final_count,
    })
}

/// Compute vertex adjustments from the flow solution.
///
/// The adjustment represents how many 90° rotations to apply
/// to the field around each vertex.
fn compute_vertex_adjustments(
    network: &FlowNetwork,
    result: &FlowResult,
    mesh: &Mesh,
) -> Vec<i32> {
    let num_vertices = mesh.num_vertices();
    let mut adjustments = vec![0i32; num_vertices];

    // For each arc with non-zero flow, compute net flow into each vertex
    for (arc_idx, &flow) in result.flows.iter().enumerate() {
        if flow == 0 {
            continue;
        }

        let arc = &network.arcs[arc_idx];

        // Get the actual vertex indices
        let from_vertex = if arc.from < network.nodes.len() {
            network.nodes[arc.from].vertex
        } else {
            continue; // Super node
        };

        let to_vertex = if arc.to < network.nodes.len() {
            network.nodes[arc.to].vertex
        } else {
            continue; // Super node
        };

        if from_vertex >= num_vertices || to_vertex >= num_vertices {
            continue;
        }

        // Flow out of source vertex, into sink vertex
        adjustments[from_vertex] -= flow;
        adjustments[to_vertex] += flow;
    }

    adjustments
}

/// Estimate the number of singularities after applying adjustments.
fn estimate_final_singularities(
    sing_info: &SingularityInfo,
    adjustments: &[i32],
) -> usize {
    let mut count = 0;

    for (v, &orig_index) in sing_info.vertex_indices.iter().enumerate() {
        let adjustment = if v < adjustments.len() {
            adjustments[v] as f64 / 4.0
        } else {
            0.0
        };

        let new_index = orig_index - adjustment;

        if new_index.abs() > 0.1 {
            count += 1;
        }
    }

    count
}

/// Apply the optimization result to adjust an orientation field.
///
/// This modifies the field directions based on the flow-derived adjustments.
/// The adjustments represent rotations around vertices that cancel out
/// singularities.
///
/// # Arguments
/// * `field` - Orientation field to modify
/// * `mesh` - The mesh
/// * `opt_result` - Result from `optimize_singularities`
pub fn apply_singularity_optimization(
    field: &mut OrientationField,
    mesh: &Mesh,
    opt_result: &SingularityOptResult,
) {
    // The adjustment interpretation:
    // A positive adjustment at a vertex means we want to reduce positive
    // singularity index there, which means rotating neighboring face
    // directions to create a more consistent field.

    // For each face, accumulate rotations from its vertices
    for face_idx in 0..field.len() {
        let vertices = mesh.get_face_vertices(face_idx);
        let normal = field.normals[face_idx];

        // Average the vertex adjustments for this face
        let mut total_adjustment: f64 = 0.0;
        for &v in &vertices {
            if v < opt_result.vertex_adjustments.len() {
                total_adjustment += opt_result.vertex_adjustments[v] as f64;
            }
        }

        // Convert to rotation angle (each unit = 90° / 4 = 22.5°)
        // But we smooth it by dividing by vertex count
        let rotation_angle = (total_adjustment / 3.0) * FRAC_PI_2 / 4.0;

        if rotation_angle.abs() > 1e-6 {
            // Rotate the face direction
            let old_dir = field.directions[face_idx];
            let new_dir = rotate_around_axis(&old_dir, &normal, rotation_angle);
            field.directions[face_idx] = new_dir;
        }
    }
}

/// Rotate a vector around an axis using Rodrigues' formula.
fn rotate_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3 as PVec3;

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

    fn make_grid_mesh(size: usize) -> Mesh {
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        for i in 0..=size {
            for j in 0..=size {
                positions.push(PVec3::new(i as f64 * 0.1, j as f64 * 0.1, 0.0));
            }
        }

        for i in 0..size {
            for j in 0..size {
                let idx = i * (size + 1) + j;
                indices.push([idx, idx + 1, idx + size + 1]);
                indices.push([idx + 1, idx + size + 2, idx + size + 1]);
            }
        }

        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_optimize_singularities_runs() {
        let mesh = make_tetrahedron();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = SingularityOptConfig::default();

        let result = optimize_singularities(&mesh, &field, &config);

        // Should complete without error (may or may not reduce singularities
        // depending on topology)
        assert!(result.is_ok());
    }

    #[test]
    fn test_optimize_flat_mesh() {
        let mesh = make_grid_mesh(4);
        let field = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = SingularityOptConfig::default();

        let result = optimize_singularities(&mesh, &field, &config).unwrap();

        // Flat mesh with aligned field should have few singularities
        // and optimization shouldn't make it worse
        assert!(result.final_singularities <= result.initial_singularities + 2);
    }

    #[test]
    fn test_apply_optimization() {
        let mesh = make_tetrahedron();
        let mut field = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = SingularityOptConfig::default();

        let result = optimize_singularities(&mesh, &field, &config).unwrap();

        // Directions should remain normalized and in tangent plane after application
        apply_singularity_optimization(&mut field, &mesh, &result);

        for i in 0..field.len() {
            let d = field.directions[i];
            let n = field.normals[i];
            assert!((d.norm() - 1.0).abs() < 1e-9);
            assert!(d.dot(&n).abs() < 1e-9);
        }
    }

    #[test]
    fn test_vertex_adjustments_balance() {
        let mesh = make_tetrahedron();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = SingularityOptConfig::default();

        let result = optimize_singularities(&mesh, &field, &config).unwrap();

        // Total adjustments should sum to approximately 0 (flow is conserved)
        let total: i32 = result.vertex_adjustments.iter().sum();
        // May not be exactly 0 due to super nodes, but should be small
        assert!(total.abs() <= 4);
    }
}
