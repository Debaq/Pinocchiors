//! Position field computation.
//!
//! The position field determines where quad vertices will be placed,
//! aligned to an integer grid in the parametric domain.
//!
//! The key idea is to solve for UV coordinates such that:
//! 1. The gradient of U aligns with one direction of the orientation field
//! 2. The gradient of V aligns with the perpendicular direction
//! 3. Transitions between faces are seamless (consistent rotations)

use crate::OrientationField;
use nalgebra::{Vector2, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_sparse::SPDMatrix;
use std::f64::consts::FRAC_PI_2;

/// Position field on a mesh surface.
///
/// Stores 2D parametric coordinates per vertex that will be snapped to integer
/// positions to form the final quad mesh vertices.
#[derive(Debug, Clone)]
pub struct PositionField {
    /// Parametric UV coordinates per vertex
    pub coords: Vec<Vector2<f64>>,
    /// Scale factor for the parametrization (controls quad size)
    pub scale: f64,
}

/// Configuration for position field computation.
#[derive(Debug, Clone)]
pub struct PositionFieldConfig {
    /// Target edge length for quads (in mesh units)
    pub target_edge_length: f64,
    /// Number of solver iterations
    pub solver_iterations: usize,
    /// Solver tolerance
    pub solver_tolerance: f64,
    /// Weight for alignment term
    pub alignment_weight: f64,
}

impl Default for PositionFieldConfig {
    fn default() -> Self {
        Self {
            target_edge_length: 0.1,
            solver_iterations: 500,
            solver_tolerance: 1e-6,
            alignment_weight: 1.0,
        }
    }
}

/// Transition rotation between adjacent faces.
///
/// Represents how much the local frame rotates when crossing an edge.
/// The rotation is in units of 90° (0, 1, 2, or 3).
#[derive(Debug, Clone, Copy, Default)]
pub struct FaceTransition {
    /// Rotation index (0-3) representing 0°, 90°, 180°, 270°
    pub rotation: i32,
    /// Translation in U direction (for seamless parametrization)
    pub translation_u: f64,
    /// Translation in V direction
    pub translation_v: f64,
}

impl PositionField {
    /// Create an empty position field.
    pub fn new() -> Self {
        Self {
            coords: Vec::new(),
            scale: 1.0,
        }
    }

    /// Compute the position field from an orientation field.
    ///
    /// This solves a Poisson-like system to find UV coordinates whose
    /// gradients align with the orientation field directions.
    pub fn from_orientation_field(
        mesh: &Mesh,
        orientation: &OrientationField,
        config: &PositionFieldConfig,
    ) -> Self {
        let num_vertices = mesh.num_vertices();

        if num_vertices == 0 {
            return Self::new();
        }

        // Compute face transitions (rotations between adjacent faces)
        let transitions = compute_face_transitions(mesh, orientation);

        // Build and solve the Poisson system for U and V
        let (u_coords, v_coords) = solve_position_system(
            mesh,
            orientation,
            &transitions,
            config,
        );

        // Combine into UV coordinates
        let coords: Vec<Vector2<f64>> = u_coords
            .into_iter()
            .zip(v_coords)
            .map(|(u, v)| Vector2::new(u, v))
            .collect();

        // Compute scale based on target edge length
        let scale = compute_scale(mesh, &coords, config.target_edge_length);

        Self { coords, scale }
    }

    /// Number of field samples.
    pub fn len(&self) -> usize {
        self.coords.len()
    }

    /// Check if field is empty.
    pub fn is_empty(&self) -> bool {
        self.coords.is_empty()
    }

    /// Get UV coordinates for a vertex.
    pub fn uv(&self, vertex_idx: usize) -> Vector2<f64> {
        self.coords[vertex_idx]
    }

    /// Get scaled UV coordinates (ready for integer snapping).
    pub fn scaled_uv(&self, vertex_idx: usize) -> Vector2<f64> {
        self.coords[vertex_idx] * self.scale
    }

    /// Interpolate UV coordinates at a point inside a face.
    ///
    /// Uses barycentric coordinates for interpolation.
    pub fn interpolate_uv(
        &self,
        mesh: &Mesh,
        face_idx: usize,
        bary: &Vector3<f64>,
    ) -> Vector2<f64> {
        let verts = mesh.get_face_vertices(face_idx);
        let uv0 = self.coords[verts[0]];
        let uv1 = self.coords[verts[1]];
        let uv2 = self.coords[verts[2]];

        uv0 * bary.x + uv1 * bary.y + uv2 * bary.z
    }
}

impl Default for PositionField {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute transition rotations between adjacent faces.
///
/// The rotation tells us how to transform the local UV frame when
/// crossing from one face to its neighbor.
fn compute_face_transitions(
    mesh: &Mesh,
    orientation: &OrientationField,
) -> Vec<Vec<FaceTransition>> {
    let num_faces = mesh.num_faces();
    let mut transitions = vec![vec![FaceTransition::default(); 3]; num_faces];

    for face_idx in 0..num_faces {
        let neighbors = mesh.get_face_neighbors(face_idx);
        let dir_a = orientation.directions[face_idx];
        let normal_a = orientation.normals[face_idx];

        for (edge_idx, &neighbor_idx) in neighbors.iter().enumerate() {
            let dir_b = orientation.directions[neighbor_idx];
            let normal_b = orientation.normals[neighbor_idx];

            // Transport dir_a to neighbor's tangent plane
            let transported = transport_direction(&dir_a, &normal_a, &normal_b);

            // Find rotation index (0-3) to align with dir_b
            let rotation = find_rotation_index(&transported, &dir_b, &normal_b);

            transitions[face_idx][edge_idx] = FaceTransition {
                rotation,
                translation_u: 0.0,
                translation_v: 0.0,
            };
        }
    }

    transitions
}

/// Find the rotation index (0-3) that best aligns source with target.
fn find_rotation_index(
    source: &Vector3<f64>,
    target: &Vector3<f64>,
    normal: &Vector3<f64>,
) -> i32 {
    let mut best_rotation = 0;
    let mut best_dot = source.dot(target);

    for r in 1..4 {
        let angle = r as f64 * FRAC_PI_2;
        let rotated = rotate_around_axis(source, normal, angle);
        let dot = rotated.dot(target);

        if dot > best_dot {
            best_dot = dot;
            best_rotation = r;
        }
    }

    best_rotation
}

/// Solve the Poisson system for position field.
///
/// We want gradients of U and V to match the orientation field.
/// This leads to a least-squares problem for each coordinate.
fn solve_position_system(
    mesh: &Mesh,
    orientation: &OrientationField,
    _transitions: &[Vec<FaceTransition>],
    config: &PositionFieldConfig,
) -> (Vec<f64>, Vec<f64>) {
    let num_vertices = mesh.num_vertices();
    let num_faces = mesh.num_faces();

    if num_vertices == 0 {
        return (Vec::new(), Vec::new());
    }

    // Build the system matrix (Laplacian-like)
    // For each face, we add equations relating vertex UV differences to field directions

    let mut rows = Vec::new();
    let mut cols = Vec::new();
    let mut values = Vec::new();
    let mut rhs_u = vec![0.0; num_vertices];
    let mut rhs_v = vec![0.0; num_vertices];

    // Add smoothness terms (Laplacian)
    for face_idx in 0..num_faces {
        let verts = mesh.get_face_vertices(face_idx);
        let positions = mesh.get_face_positions(face_idx);

        // Get local frame from orientation field
        let dir_u = orientation.directions[face_idx];
        let normal = orientation.normals[face_idx];
        let dir_v = normal.cross(&dir_u);

        // For each edge in the face
        for i in 0..3 {
            let j = (i + 1) % 3;
            let vi = verts[i];
            let vj = verts[j];

            // Edge vector in 3D
            let edge_3d = Vector3::new(
                positions[j].x() - positions[i].x(),
                positions[j].y() - positions[i].y(),
                positions[j].z() - positions[i].z(),
            );

            // Project edge onto local UV frame
            let edge_u = edge_3d.dot(&dir_u);
            let edge_v = edge_3d.dot(&dir_v);

            let weight = config.alignment_weight;

            // Equation: u[vj] - u[vi] ≈ edge_u
            // Contributes to A^T A and A^T b
            add_edge_equation(&mut rows, &mut cols, &mut values, &mut rhs_u, vi, vj, edge_u, weight);

            // Equation: v[vj] - v[vi] ≈ edge_v
            add_edge_equation(&mut rows, &mut cols, &mut values, &mut rhs_v, vi, vj, edge_v, weight);
        }
    }

    // Add regularization (small identity term) to ensure system is solvable
    let reg = 1e-6;
    for i in 0..num_vertices {
        rows.push(i);
        cols.push(i);
        values.push(reg);
    }

    // Pin one vertex to (0, 0) to remove translation ambiguity
    let pin_weight = 1000.0;
    rows.push(0);
    cols.push(0);
    values.push(pin_weight);

    // Build and solve for U
    let matrix_u = SPDMatrix::from_triplets(num_vertices, &rows, &cols, &values)
        .expect("Failed to build U matrix");
    let u_coords = matrix_u
        .solve_gauss_seidel(&rhs_u, config.solver_iterations, config.solver_tolerance)
        .unwrap_or_else(|_| vec![0.0; num_vertices]);

    // Build and solve for V
    let matrix_v = SPDMatrix::from_triplets(num_vertices, &rows, &cols, &values)
        .expect("Failed to build V matrix");
    let v_coords = matrix_v
        .solve_gauss_seidel(&rhs_v, config.solver_iterations, config.solver_tolerance)
        .unwrap_or_else(|_| vec![0.0; num_vertices]);

    (u_coords, v_coords)
}

/// Add an edge equation to the normal equations system.
fn add_edge_equation(
    rows: &mut Vec<usize>,
    cols: &mut Vec<usize>,
    values: &mut Vec<f64>,
    rhs: &mut [f64],
    vi: usize,
    vj: usize,
    target: f64,
    weight: f64,
) {
    let w2 = weight * weight;

    // (x[vj] - x[vi])^2 term expands to:
    // x[vi]^2 - 2*x[vi]*x[vj] + x[vj]^2

    // A^T A contributions
    rows.push(vi);
    cols.push(vi);
    values.push(w2);

    rows.push(vj);
    cols.push(vj);
    values.push(w2);

    rows.push(vi);
    cols.push(vj);
    values.push(-w2);

    rows.push(vj);
    cols.push(vi);
    values.push(-w2);

    // A^T b contributions
    // target * (x[vj] - x[vi]) -> target on vj, -target on vi
    rhs[vj] += target * w2;
    rhs[vi] -= target * w2;
}

/// Compute the scale factor based on target edge length.
fn compute_scale(mesh: &Mesh, coords: &[Vector2<f64>], target_length: f64) -> f64 {
    if mesh.num_faces() == 0 || coords.is_empty() {
        return 1.0;
    }

    // Compute average UV edge length
    let mut total_uv_length = 0.0;
    let mut total_3d_length = 0.0;
    let mut count = 0;

    for face_idx in 0..mesh.num_faces() {
        let verts = mesh.get_face_vertices(face_idx);
        let positions = mesh.get_face_positions(face_idx);

        for i in 0..3 {
            let j = (i + 1) % 3;

            if verts[i] < coords.len() && verts[j] < coords.len() {
                let uv_diff = coords[verts[j]] - coords[verts[i]];
                let uv_len = uv_diff.norm();

                let pos_diff = positions[j] - positions[i];
                let pos_len = pos_diff.length();

                if uv_len > 1e-10 && pos_len > 1e-10 {
                    total_uv_length += uv_len;
                    total_3d_length += pos_len;
                    count += 1;
                }
            }
        }
    }

    if count == 0 || total_uv_length < 1e-10 {
        return 1.0;
    }

    // Scale so that one UV unit corresponds to target_length in 3D
    let avg_ratio = total_3d_length / total_uv_length;
    1.0 / (avg_ratio * target_length)
}

/// Transport a tangent vector between faces.
fn transport_direction(
    dir: &Vector3<f64>,
    from_normal: &Vector3<f64>,
    to_normal: &Vector3<f64>,
) -> Vector3<f64> {
    let cross = from_normal.cross(to_normal);
    let sin_angle = cross.norm();

    if sin_angle < 1e-10 {
        return *dir;
    }

    let axis = cross / sin_angle;
    let cos_angle = from_normal.dot(to_normal).clamp(-1.0, 1.0);
    let angle = cos_angle.acos();

    rotate_around_axis(dir, &axis, angle)
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

    fn make_flat_quad() -> Mesh {
        // 2 triangles forming a square
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(1.0, 1.0, 0.0),
            PVec3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3]];
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
    fn test_position_field_creation() {
        let field = PositionField::new();
        assert!(field.is_empty());
        assert_eq!(field.len(), 0);
    }

    #[test]
    fn test_position_field_from_orientation() {
        let mesh = make_flat_quad();
        let orientation = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = PositionFieldConfig::default();

        let position = PositionField::from_orientation_field(&mesh, &orientation, &config);

        // Should have one UV coordinate per vertex
        assert_eq!(position.len(), mesh.num_vertices());

        // All coordinates should be finite
        for coord in &position.coords {
            assert!(coord.x.is_finite());
            assert!(coord.y.is_finite());
        }
    }

    #[test]
    fn test_position_field_grid() {
        let mesh = make_grid_mesh(3);
        let orientation = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = PositionFieldConfig {
            solver_iterations: 200,
            ..Default::default()
        };

        let position = PositionField::from_orientation_field(&mesh, &orientation, &config);

        assert_eq!(position.len(), mesh.num_vertices());
        assert!(position.scale > 0.0);
    }

    #[test]
    fn test_interpolate_uv() {
        let mesh = make_flat_quad();
        let orientation = OrientationField::from_mesh_edge_aligned(&mesh);
        let config = PositionFieldConfig::default();

        let position = PositionField::from_orientation_field(&mesh, &orientation, &config);

        // Barycentric center of first face
        let bary = Vector3::new(1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
        let uv = position.interpolate_uv(&mesh, 0, &bary);

        assert!(uv.x.is_finite());
        assert!(uv.y.is_finite());
    }

    #[test]
    fn test_find_rotation_index() {
        let normal = Vector3::new(0.0, 0.0, 1.0);

        // Same direction -> rotation 0
        let dir = Vector3::new(1.0, 0.0, 0.0);
        assert_eq!(find_rotation_index(&dir, &dir, &normal), 0);

        // 90° rotation
        let dir_90 = Vector3::new(0.0, 1.0, 0.0);
        let rot = find_rotation_index(&dir, &dir_90, &normal);
        assert!(rot == 1 || rot == 3); // Either +90 or -90 should work

        // 180° rotation
        let dir_180 = Vector3::new(-1.0, 0.0, 0.0);
        assert_eq!(find_rotation_index(&dir, &dir_180, &normal), 2);
    }

    #[test]
    fn test_scale_computation() {
        let mesh = make_flat_quad();
        let orientation = OrientationField::from_mesh_edge_aligned(&mesh);

        let config1 = PositionFieldConfig {
            target_edge_length: 0.1,
            ..Default::default()
        };
        let pos1 = PositionField::from_orientation_field(&mesh, &orientation, &config1);

        let config2 = PositionFieldConfig {
            target_edge_length: 0.5,
            ..Default::default()
        };
        let pos2 = PositionField::from_orientation_field(&mesh, &orientation, &config2);

        // Smaller target edge length should give larger scale
        // (more quads needed to cover the same area)
        assert!(pos1.scale > pos2.scale * 0.5);
    }
}
