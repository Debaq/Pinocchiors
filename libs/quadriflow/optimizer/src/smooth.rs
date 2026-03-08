//! Smoothness optimization for fields.
//!
//! Implements local smoothing operators based on Instant Meshes:
//! "Instant Field-Aligned Meshes" (Wenzel et al., SIGGRAPH Asia 2015)

use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_field::OrientationField;
use rayon::prelude::*;
use std::f64::consts::FRAC_PI_2;

/// Smooth an orientation field using local averaging.
///
/// Each iteration updates each face's direction to be the weighted average
/// of its neighbors, while respecting 4-RoSy symmetry.
///
/// # Arguments
/// * `field` - Orientation field to smooth (modified in place)
/// * `mesh` - The underlying mesh (for adjacency)
/// * `iterations` - Number of smoothing iterations
pub fn smooth_orientation_field(field: &mut OrientationField, mesh: &Mesh, iterations: usize) {
    let num_faces = field.len();
    if num_faces == 0 {
        return;
    }

    // Precompute adjacency list
    let adjacency: Vec<Vec<usize>> = (0..num_faces)
        .map(|f| mesh.get_face_neighbors(f))
        .collect();

    for _ in 0..iterations {
        // Compute new directions in parallel
        let new_directions: Vec<Vector3<f64>> = (0..num_faces)
            .into_par_iter()
            .map(|face_idx| {
                smooth_single_face(field, &adjacency[face_idx], face_idx)
            })
            .collect();

        // Update all directions
        for (i, dir) in new_directions.into_iter().enumerate() {
            field.directions[i] = dir;
        }
    }
}

/// Smooth a single face's direction based on its neighbors.
fn smooth_single_face(
    field: &OrientationField,
    neighbors: &[usize],
    face_idx: usize,
) -> Vector3<f64> {
    if neighbors.is_empty() {
        return field.directions[face_idx];
    }

    let current_dir = field.directions[face_idx];
    let normal = field.normals[face_idx];

    // Accumulate aligned neighbor directions
    let mut sum = current_dir;

    for &neighbor_idx in neighbors {
        let neighbor_dir = field.directions[neighbor_idx];
        let neighbor_normal = field.normals[neighbor_idx];

        // Transport neighbor direction to this face's tangent plane
        let transported = transport_direction(&neighbor_dir, &neighbor_normal, &normal);

        // Find the closest of the 4 symmetric directions
        let aligned = align_to_4rosy(&transported, &current_dir, &normal);

        sum += aligned;
    }

    // Normalize the result and project back to tangent plane
    project_to_tangent_plane(&sum, &normal)
}

/// Transport a tangent vector from one face to another.
///
/// Uses parallel transport: rotate the vector so it remains tangent
/// to the new face while preserving the angle with shared structures.
fn transport_direction(
    dir: &Vector3<f64>,
    from_normal: &Vector3<f64>,
    to_normal: &Vector3<f64>,
) -> Vector3<f64> {
    // If normals are nearly parallel, no rotation needed
    let cross = from_normal.cross(to_normal);
    let sin_angle = cross.norm();

    if sin_angle < 1e-10 {
        // Normals are parallel - just return the direction
        return *dir;
    }

    // Rotation axis is the cross product of normals
    let axis = cross / sin_angle;
    let cos_angle = from_normal.dot(to_normal).clamp(-1.0, 1.0);
    let angle = cos_angle.acos();

    // Rodrigues' rotation formula
    rotate_around_axis(dir, &axis, angle)
}

/// Align a direction to the closest of the 4 symmetric directions.
///
/// Given a target direction and a reference, finds which of the 4
/// rotations of target (by 0°, 90°, 180°, 270°) is closest to reference.
fn align_to_4rosy(
    target: &Vector3<f64>,
    reference: &Vector3<f64>,
    normal: &Vector3<f64>,
) -> Vector3<f64> {
    let mut best_dir = *target;
    let mut best_dot = target.dot(reference);

    // Check 90° rotation
    let rot90 = rotate_around_axis(target, normal, FRAC_PI_2);
    let dot90 = rot90.dot(reference);
    if dot90 > best_dot {
        best_dir = rot90;
        best_dot = dot90;
    }

    // Check 180° rotation
    let rot180 = -target;
    let dot180 = rot180.dot(reference);
    if dot180 > best_dot {
        best_dir = rot180;
        best_dot = dot180;
    }

    // Check 270° rotation
    let rot270 = rotate_around_axis(target, normal, 3.0 * FRAC_PI_2);
    let dot270 = rot270.dot(reference);
    if dot270 > best_dot {
        best_dir = rot270;
    }

    best_dir
}

/// Project a vector onto the tangent plane and normalize.
fn project_to_tangent_plane(v: &Vector3<f64>, normal: &Vector3<f64>) -> Vector3<f64> {
    let projected = v - normal * v.dot(normal);
    let len = projected.norm();
    if len > 1e-10 {
        projected / len
    } else {
        // Fallback: construct arbitrary perpendicular
        arbitrary_perpendicular(normal)
    }
}

/// Construct an arbitrary unit vector perpendicular to the given vector.
fn arbitrary_perpendicular(v: &Vector3<f64>) -> Vector3<f64> {
    let abs_x = v.x.abs();
    let abs_y = v.y.abs();
    let abs_z = v.z.abs();

    let helper = if abs_x <= abs_y && abs_x <= abs_z {
        Vector3::new(1.0, 0.0, 0.0)
    } else if abs_y <= abs_z {
        Vector3::new(0.0, 1.0, 0.0)
    } else {
        Vector3::new(0.0, 0.0, 1.0)
    };

    v.cross(&helper).normalize()
}

/// Rotate a vector around an axis using Rodrigues' formula.
fn rotate_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

/// Compute a smoothness energy for the field.
///
/// Lower values indicate smoother fields. The energy is the sum of
/// squared angular differences between adjacent faces.
pub fn smoothness_energy(field: &OrientationField, mesh: &Mesh) -> f64 {
    let num_faces = field.len();
    if num_faces == 0 {
        return 0.0;
    }

    let mut energy = 0.0;

    for face_idx in 0..num_faces {
        let dir = field.directions[face_idx];
        let normal = field.normals[face_idx];

        for neighbor_idx in mesh.get_face_neighbors(face_idx) {
            let neighbor_dir = field.directions[neighbor_idx];
            let neighbor_normal = field.normals[neighbor_idx];

            // Transport and align
            let transported = transport_direction(&neighbor_dir, &neighbor_normal, &normal);
            let aligned = align_to_4rosy(&transported, &dir, &normal);

            // Angular difference (1 - cos(theta))
            let dot = dir.dot(&aligned).clamp(-1.0, 1.0);
            energy += 1.0 - dot;
        }
    }

    // Divide by 2 since each edge is counted twice
    energy / 2.0
}

/// Apply smoothing to position field coordinates.
pub fn smooth_position_field(_coords: &mut [nalgebra::Vector2<f64>], _iterations: usize) {
    // TODO: Implement position field smoothing
    // Must maintain integer grid alignment constraints
}

/// Smooth an orientation field using SIMD-optimized batch operations.
///
/// This version processes faces in batches for better cache utilization
/// and SIMD autovectorization.
///
/// # Arguments
/// * `field` - Orientation field to smooth (modified in place)
/// * `mesh` - The underlying mesh (for adjacency)
/// * `iterations` - Number of smoothing iterations
pub fn smooth_orientation_field_simd(
    field: &mut OrientationField,
    mesh: &Mesh,
    iterations: usize,
) {
    use crate::simd::smooth_faces_batch;

    let num_faces = field.len();
    if num_faces == 0 {
        return;
    }

    // Precompute adjacency list
    let adjacency: Vec<Vec<usize>> = (0..num_faces)
        .map(|f| mesh.get_face_neighbors(f))
        .collect();

    // Create face index list
    let face_indices: Vec<usize> = (0..num_faces).collect();

    for _ in 0..iterations {
        // Use SIMD batch processing
        let new_directions = smooth_faces_batch(
            &field.directions,
            &field.normals,
            &face_indices,
            &adjacency,
        );

        // Update all directions
        for (i, dir) in new_directions.into_iter().enumerate() {
            field.directions[i] = dir;
        }
    }
}

/// Compute smoothness energy using SIMD-optimized batch operations.
pub fn smoothness_energy_simd(field: &OrientationField, mesh: &Mesh) -> f64 {
    use crate::simd::compute_energy_batch;

    let num_faces = field.len();
    if num_faces == 0 {
        return 0.0;
    }

    let adjacency: Vec<Vec<usize>> = (0..num_faces)
        .map(|f| mesh.get_face_neighbors(f))
        .collect();

    compute_energy_batch(&field.directions, &field.normals, &adjacency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use pinocchio_math::Vector3 as PVec3;

    fn make_two_triangles() -> Mesh {
        // Two triangles sharing an edge
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.5, 1.0, 0.0),
            PVec3::new(0.5, -1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2], [1, 0, 3]];
        Mesh::from_triangles(&vertices, &faces)
    }

    fn make_flat_quad() -> Mesh {
        // 4 triangles forming a flat quad, all sharing center vertex
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0), // center
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.0, 1.0, 0.0),
            PVec3::new(-1.0, 0.0, 0.0),
            PVec3::new(0.0, -1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3], [0, 3, 4], [0, 4, 1]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_smooth_reduces_energy() {
        let mesh = make_flat_quad();
        let mut field = OrientationField::from_mesh(&mesh);

        let initial_energy = smoothness_energy(&field, &mesh);
        smooth_orientation_field(&mut field, &mesh, 10);
        let final_energy = smoothness_energy(&field, &mesh);

        // Energy should decrease or stay the same
        assert!(final_energy <= initial_energy + 1e-10);
    }

    #[test]
    fn test_smooth_preserves_tangent_plane() {
        let mesh = make_flat_quad();
        let mut field = OrientationField::from_mesh(&mesh);

        smooth_orientation_field(&mut field, &mesh, 5);

        // All directions should remain perpendicular to their normals
        for i in 0..field.len() {
            let d = field.directions[i];
            let n = field.normals[i];
            assert_relative_eq!(d.dot(&n), 0.0, epsilon = 1e-10);
            assert_relative_eq!(d.norm(), 1.0, epsilon = 1e-10);
        }
    }

    #[test]
    fn test_align_to_4rosy() {
        let target = Vector3::new(1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        // Reference aligned with target - should return target
        let reference = Vector3::new(1.0, 0.0, 0.0);
        let aligned = align_to_4rosy(&target, &reference, &normal);
        assert_relative_eq!(aligned.dot(&target), 1.0, epsilon = 1e-10);

        // Reference perpendicular - should return 90° rotation
        let reference = Vector3::new(0.0, 1.0, 0.0);
        let aligned = align_to_4rosy(&target, &reference, &normal);
        assert_relative_eq!(aligned.dot(&reference), 1.0, epsilon = 1e-10);

        // Reference opposite - should return 180° rotation
        let reference = Vector3::new(-1.0, 0.0, 0.0);
        let aligned = align_to_4rosy(&target, &reference, &normal);
        assert_relative_eq!(aligned.dot(&reference), 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_transport_same_normal() {
        let dir = Vector3::new(1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let transported = transport_direction(&dir, &normal, &normal);
        assert_relative_eq!(transported.x, dir.x, epsilon = 1e-10);
        assert_relative_eq!(transported.y, dir.y, epsilon = 1e-10);
        assert_relative_eq!(transported.z, dir.z, epsilon = 1e-10);
    }

    #[test]
    fn test_smoothness_energy_aligned_field() {
        let mesh = make_two_triangles();
        let mut field = OrientationField::from_mesh_edge_aligned(&mesh);

        // Both faces are coplanar with same edge direction - energy should be low
        let energy = smoothness_energy(&field, &mesh);

        // Perturb one direction
        field.directions[0] = rotate_around_axis(
            &field.directions[0],
            &field.normals[0],
            0.5,
        );

        let perturbed_energy = smoothness_energy(&field, &mesh);
        assert!(perturbed_energy > energy);
    }
}
