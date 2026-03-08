//! # quadriflow-hierarchy
//!
//! Multi-scale mesh hierarchy for efficient field optimization.
//!
//! QuadriFlow uses a coarse-to-fine hierarchy to efficiently propagate
//! field information across the mesh surface. This allows:
//!
//! 1. Fast initial field computation on coarse levels
//! 2. Progressive refinement to finer levels
//! 3. Global coherence through multi-scale optimization

mod builder;
mod propagate;

pub use builder::{build_hierarchy, HierarchyConfig};
pub use propagate::{propagate_field_to_finer, propagate_field_to_coarser};

use pinocchio_mesh::Mesh;

/// A level in the mesh hierarchy.
#[derive(Debug, Clone)]
pub struct HierarchyLevel {
    /// The mesh at this level
    pub mesh: Mesh,
    /// Mapping from faces at this level to faces in finer level
    /// Each face maps to multiple finer faces that it covers
    pub face_to_finer: Vec<Vec<usize>>,
    /// Mapping from faces at this level to parent face in coarser level
    pub face_to_coarser: Vec<usize>,
    /// Mapping from vertices at this level to vertices in finer level
    pub vertex_to_finer: Vec<Vec<usize>>,
    /// Mapping from vertices at this level to parent vertex in coarser level
    pub vertex_to_coarser: Vec<usize>,
}

impl HierarchyLevel {
    /// Create a new hierarchy level from a mesh (base level, no mappings).
    pub fn new_base(mesh: Mesh) -> Self {
        let num_faces = mesh.num_faces();
        let num_vertices = mesh.num_vertices();

        Self {
            mesh,
            face_to_finer: vec![Vec::new(); num_faces],
            face_to_coarser: (0..num_faces).collect(), // Identity mapping
            vertex_to_finer: vec![Vec::new(); num_vertices],
            vertex_to_coarser: (0..num_vertices).collect(),
        }
    }

    /// Number of faces at this level.
    pub fn num_faces(&self) -> usize {
        self.mesh.num_faces()
    }

    /// Number of vertices at this level.
    pub fn num_vertices(&self) -> usize {
        self.mesh.num_vertices()
    }
}

/// Multi-scale mesh hierarchy.
///
/// Level 0 is the finest (original mesh), higher levels are coarser.
#[derive(Debug, Clone)]
pub struct MeshHierarchy {
    /// Hierarchy levels from finest (0) to coarsest
    pub levels: Vec<HierarchyLevel>,
}

impl MeshHierarchy {
    /// Create a new empty hierarchy.
    pub fn new() -> Self {
        Self { levels: Vec::new() }
    }

    /// Create a hierarchy with just the base mesh (no coarsening).
    pub fn from_mesh(mesh: Mesh) -> Self {
        Self {
            levels: vec![HierarchyLevel::new_base(mesh)],
        }
    }

    /// Number of levels in the hierarchy.
    pub fn depth(&self) -> usize {
        self.levels.len()
    }

    /// Check if hierarchy is empty.
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Get the finest level (original mesh).
    pub fn finest(&self) -> Option<&HierarchyLevel> {
        self.levels.first()
    }

    /// Get the coarsest level.
    pub fn coarsest(&self) -> Option<&HierarchyLevel> {
        self.levels.last()
    }

    /// Get a specific level.
    pub fn level(&self, idx: usize) -> Option<&HierarchyLevel> {
        self.levels.get(idx)
    }

    /// Get the mesh at a specific level.
    pub fn mesh_at(&self, idx: usize) -> Option<&Mesh> {
        self.levels.get(idx).map(|l| &l.mesh)
    }

    /// Iterate over levels from finest to coarsest.
    pub fn iter_fine_to_coarse(&self) -> impl Iterator<Item = &HierarchyLevel> {
        self.levels.iter()
    }

    /// Iterate over levels from coarsest to finest.
    pub fn iter_coarse_to_fine(&self) -> impl Iterator<Item = &HierarchyLevel> {
        self.levels.iter().rev()
    }
}

impl Default for MeshHierarchy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3 as PVec3;

    fn make_test_mesh() -> Mesh {
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.5, 1.0, 0.0),
            PVec3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_hierarchy_from_mesh() {
        let mesh = make_test_mesh();
        let hierarchy = MeshHierarchy::from_mesh(mesh);

        assert_eq!(hierarchy.depth(), 1);
        assert!(!hierarchy.is_empty());
        assert!(hierarchy.finest().is_some());
        assert!(hierarchy.coarsest().is_some());
    }

    #[test]
    fn test_base_level_identity_mapping() {
        let mesh = make_test_mesh();
        let level = HierarchyLevel::new_base(mesh);

        // Identity mapping for face_to_coarser
        for (i, &parent) in level.face_to_coarser.iter().enumerate() {
            assert_eq!(i, parent);
        }
    }
}
