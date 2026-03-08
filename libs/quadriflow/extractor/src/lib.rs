//! # quadriflow-extractor
//!
//! Extract quad mesh from integer parametrization.
//!
//! Traces the integer isolines in the parametric domain to construct
//! the final quad mesh vertices and faces.
//!
//! ## Components
//!
//! - **Trace**: Isoline tracing to find quad vertices (`trace_isolines`)
//! - **Topology**: Validation and repair (`is_manifold`, `merge_close_vertices`)

pub mod topology;
pub mod trace;

pub use topology::{
    is_manifold, merge_close_vertices, post_process, remove_degenerate_faces,
    repair_topology, validate_topology, TopologyConfig, TopologyInfo,
};
pub use trace::{trace_isolines, trace_isolines_with_config, TraceConfig};

use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_parametrizer::IntegerParametrization;
use thiserror::Error;

/// Errors during mesh extraction.
#[derive(Error, Debug)]
pub enum ExtractionError {
    #[error("Invalid parametrization")]
    InvalidParametrization,
    #[error("Failed to trace isolines")]
    TraceError,
    #[error("Resulting mesh is non-manifold")]
    NonManifold,
    #[error("Empty input mesh")]
    EmptyInput,
}

/// A quad face (4 vertex indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuadFace {
    pub v: [usize; 4],
}

/// Extracted quad mesh.
#[derive(Debug, Clone)]
pub struct QuadMesh {
    /// Vertex positions
    pub vertices: Vec<Vector3<f64>>,
    /// Quad faces (counter-clockwise winding)
    pub faces: Vec<QuadFace>,
}

impl QuadMesh {
    /// Create an empty quad mesh.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// Number of vertices.
    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    /// Number of faces.
    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }

    /// Check if mesh is empty.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Get Euler characteristic (V - E + F).
    pub fn euler_characteristic(&self) -> i32 {
        let v = self.vertices.len() as i32;
        let f = self.faces.len() as i32;
        // For quads: each face has 4 edges, each edge shared by 2 faces
        // Plus boundary edges (counted once)
        let info = validate_topology(self);
        let e = (self.faces.len() * 4 - info.boundary_edges) / 2 + info.boundary_edges;
        v - e as i32 + f
    }
}

impl Default for QuadMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for quad extraction.
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Trace configuration
    pub trace: TraceConfig,
    /// Topology configuration
    pub topology: TopologyConfig,
    /// Whether to post-process the mesh
    pub post_process: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            trace: TraceConfig::default(),
            topology: TopologyConfig::default(),
            post_process: true,
        }
    }
}

/// Extract quad mesh from input triangle mesh and integer parametrization.
///
/// This is the main entry point for quad extraction.
pub fn extract_quads(
    mesh: &Mesh,
    param: &IntegerParametrization,
    config: &ExtractionConfig,
) -> Result<QuadMesh, ExtractionError> {
    if mesh.num_vertices() == 0 {
        return Err(ExtractionError::EmptyInput);
    }

    if param.u.len() != mesh.num_vertices() || param.v.len() != mesh.num_vertices() {
        return Err(ExtractionError::InvalidParametrization);
    }

    // Convert mesh to positions and triangles
    let positions: Vec<Vector3<f64>> = mesh
        .vertices
        .iter()
        .map(|v| Vector3::new(v.position.x(), v.position.y(), v.position.z()))
        .collect();

    let triangles: Vec<[usize; 3]> = get_triangles(mesh);

    // Trace isolines
    let mut quad_mesh = trace_isolines_with_config(
        &positions,
        &triangles,
        &param.u,
        &param.v,
        &config.trace,
    );

    // Post-process if requested
    if config.post_process {
        let info = post_process(&mut quad_mesh, &config.topology);

        if !info.is_manifold && info.non_manifold_edges > 0 {
            // Try to continue anyway, just warn
        }
    }

    Ok(quad_mesh)
}

/// Extract triangles from mesh.
fn get_triangles(mesh: &Mesh) -> Vec<[usize; 3]> {
    let mut triangles = Vec::new();

    for face_idx in 0..mesh.num_faces() {
        let edge_start = mesh.faces[face_idx];
        let mut current = edge_start;
        let mut verts = Vec::new();

        loop {
            let edge = &mesh.edges[current];
            verts.push(edge.vertex);
            current = edge.next;

            if current == edge_start {
                break;
            }

            if verts.len() > 10 {
                break; // Safety limit
            }
        }

        if verts.len() >= 3 {
            triangles.push([verts[0], verts[1], verts[2]]);
        }
    }

    triangles
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3 as PVec3;

    fn make_test_mesh() -> Mesh {
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
    fn test_quad_mesh_creation() {
        let mesh = QuadMesh::new();
        assert!(mesh.is_empty());
        assert_eq!(mesh.num_vertices(), 0);
        assert_eq!(mesh.num_faces(), 0);
    }

    #[test]
    fn test_extract_quads_empty() {
        let mesh = Mesh::new();
        let param = IntegerParametrization::new();
        let config = ExtractionConfig::default();

        let result = extract_quads(&mesh, &param, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_quads_simple() {
        let mesh = make_test_mesh();
        let param = IntegerParametrization {
            u: vec![0, 1, 1, 0],
            v: vec![0, 0, 1, 1],
            rotations: vec![0, 0],
        };
        let config = ExtractionConfig::default();

        let result = extract_quads(&mesh, &param, &config);
        assert!(result.is_ok());

        let quad_mesh = result.unwrap();
        assert_eq!(quad_mesh.num_faces(), 1);
    }

    #[test]
    fn test_quad_face_equality() {
        let f1 = QuadFace { v: [0, 1, 2, 3] };
        let f2 = QuadFace { v: [0, 1, 2, 3] };
        let f3 = QuadFace { v: [0, 1, 2, 4] };

        assert_eq!(f1, f2);
        assert_ne!(f1, f3);
    }
}
