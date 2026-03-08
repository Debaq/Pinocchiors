//! Field propagation between hierarchy levels.
//!
//! Implements coarse-to-fine and fine-to-coarse field propagation
//! for multi-scale optimization.

use crate::HierarchyLevel;
use nalgebra::Vector3;
use std::f64::consts::FRAC_PI_2;

/// Propagate an orientation field from a coarser level to a finer level.
///
/// Each fine face inherits the direction from its parent coarse face,
/// transported to its own tangent plane.
///
/// # Arguments
/// * `coarse_directions` - Directions at the coarse level (one per coarse face)
/// * `coarse_normals` - Normals at the coarse level
/// * `fine_level` - The finer hierarchy level (contains mapping and mesh)
///
/// # Returns
/// Directions for the fine level (one per fine face)
pub fn propagate_field_to_finer(
    coarse_directions: &[Vector3<f64>],
    coarse_normals: &[Vector3<f64>],
    fine_level: &HierarchyLevel,
) -> (Vec<Vector3<f64>>, Vec<Vector3<f64>>) {
    let num_fine_faces = fine_level.num_faces();
    let mut fine_directions = Vec::with_capacity(num_fine_faces);
    let mut fine_normals = Vec::with_capacity(num_fine_faces);

    for fine_face in 0..num_fine_faces {
        // Get this face's normal
        let fine_normal = fine_level.mesh.get_face_normal(fine_face);
        let fine_normal = Vector3::new(fine_normal.x(), fine_normal.y(), fine_normal.z());
        fine_normals.push(fine_normal);

        // Get the coarse parent face
        let coarse_face = fine_level.face_to_coarser[fine_face];

        if coarse_face < coarse_directions.len() {
            // Transport the coarse direction to this face's tangent plane
            let coarse_dir = coarse_directions[coarse_face];
            let coarse_normal = coarse_normals[coarse_face];

            let transported = transport_direction(&coarse_dir, &coarse_normal, &fine_normal);
            fine_directions.push(transported);
        } else {
            // No parent (collapsed face) - use arbitrary direction
            fine_directions.push(arbitrary_perpendicular(&fine_normal));
        }
    }

    (fine_directions, fine_normals)
}

/// Propagate an orientation field from a finer level to a coarser level.
///
/// Each coarse face gets the average direction of its child faces,
/// properly aligned using 4-RoSy symmetry.
///
/// # Arguments
/// * `fine_directions` - Directions at the fine level
/// * `fine_normals` - Normals at the fine level
/// * `coarse_level` - The coarser hierarchy level
///
/// # Returns
/// Directions and normals for the coarse level
pub fn propagate_field_to_coarser(
    fine_directions: &[Vector3<f64>],
    fine_normals: &[Vector3<f64>],
    coarse_level: &HierarchyLevel,
) -> (Vec<Vector3<f64>>, Vec<Vector3<f64>>) {
    let num_coarse_faces = coarse_level.num_faces();
    let mut coarse_directions = Vec::with_capacity(num_coarse_faces);
    let mut coarse_normals = Vec::with_capacity(num_coarse_faces);

    for coarse_face in 0..num_coarse_faces {
        // Get this face's normal
        let coarse_normal = coarse_level.mesh.get_face_normal(coarse_face);
        let coarse_normal = Vector3::new(coarse_normal.x(), coarse_normal.y(), coarse_normal.z());
        coarse_normals.push(coarse_normal);

        // Get all child faces
        let children = &coarse_level.face_to_finer[coarse_face];

        if children.is_empty() {
            // No children - use arbitrary direction
            coarse_directions.push(arbitrary_perpendicular(&coarse_normal));
            continue;
        }

        // Average the child directions (with 4-RoSy alignment)
        let dir = average_directions_4rosy(
            children,
            fine_directions,
            fine_normals,
            &coarse_normal,
        );
        coarse_directions.push(dir);
    }

    (coarse_directions, coarse_normals)
}

/// Average multiple directions while respecting 4-RoSy symmetry.
///
/// Uses the first direction as reference and aligns others to it
/// before averaging.
fn average_directions_4rosy(
    face_indices: &[usize],
    directions: &[Vector3<f64>],
    normals: &[Vector3<f64>],
    target_normal: &Vector3<f64>,
) -> Vector3<f64> {
    if face_indices.is_empty() {
        return arbitrary_perpendicular(target_normal);
    }

    // Transport first direction to target tangent plane as reference
    let first_idx = face_indices[0];
    let reference = transport_direction(
        &directions[first_idx],
        &normals[first_idx],
        target_normal,
    );

    let mut sum = reference;

    // Add aligned versions of other directions
    for &idx in face_indices.iter().skip(1) {
        let transported = transport_direction(&directions[idx], &normals[idx], target_normal);
        let aligned = align_to_4rosy(&transported, &reference, target_normal);
        sum += aligned;
    }

    // Normalize and project to tangent plane
    project_to_tangent_plane(&sum, target_normal)
}

/// Transport a tangent vector from one normal to another.
fn transport_direction(
    dir: &Vector3<f64>,
    from_normal: &Vector3<f64>,
    to_normal: &Vector3<f64>,
) -> Vector3<f64> {
    let cross = from_normal.cross(to_normal);
    let sin_angle = cross.norm();

    if sin_angle < 1e-10 {
        // Normals are nearly parallel
        return project_to_tangent_plane(dir, to_normal);
    }

    let axis = cross / sin_angle;
    let cos_angle = from_normal.dot(to_normal).clamp(-1.0, 1.0);
    let angle = cos_angle.acos();

    let rotated = rotate_around_axis(dir, &axis, angle);
    project_to_tangent_plane(&rotated, to_normal)
}

/// Align a direction to the closest 4-RoSy symmetric variant.
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

/// Project a vector onto tangent plane and normalize.
fn project_to_tangent_plane(v: &Vector3<f64>, normal: &Vector3<f64>) -> Vector3<f64> {
    let projected = v - normal * v.dot(normal);
    let len = projected.norm();
    if len > 1e-10 {
        projected / len
    } else {
        arbitrary_perpendicular(normal)
    }
}

/// Construct an arbitrary perpendicular unit vector.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_hierarchy, HierarchyConfig};
    use approx::assert_relative_eq;
    use pinocchio_math::Vector3 as PVec3;
    use pinocchio_mesh::Mesh;

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
    fn test_propagate_to_finer_preserves_tangent_plane() {
        let mesh = make_grid_mesh(6);
        let config = HierarchyConfig {
            reduction_ratio: 0.25,
            min_faces: 5,
            max_levels: 2,
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        if hierarchy.depth() < 2 {
            return; // Skip if not enough levels
        }

        let fine = hierarchy.level(0).unwrap();
        let coarse = hierarchy.level(1).unwrap();

        // Create test field on coarse level
        let coarse_directions: Vec<_> = (0..coarse.num_faces())
            .map(|f| {
                let n = coarse.mesh.get_face_normal(f);
                arbitrary_perpendicular(&Vector3::new(n.x(), n.y(), n.z()))
            })
            .collect();
        let coarse_normals: Vec<_> = (0..coarse.num_faces())
            .map(|f| {
                let n = coarse.mesh.get_face_normal(f);
                Vector3::new(n.x(), n.y(), n.z())
            })
            .collect();

        let (fine_dirs, fine_norms) =
            propagate_field_to_finer(&coarse_directions, &coarse_normals, fine);

        // All fine directions should be perpendicular to their normals
        for i in 0..fine_dirs.len() {
            assert_relative_eq!(fine_dirs[i].dot(&fine_norms[i]), 0.0, epsilon = 1e-9);
            assert_relative_eq!(fine_dirs[i].norm(), 1.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn test_propagate_to_coarser_preserves_tangent_plane() {
        let mesh = make_grid_mesh(6);
        let config = HierarchyConfig {
            reduction_ratio: 0.25,
            min_faces: 5,
            max_levels: 2,
        };
        let hierarchy = build_hierarchy(&mesh, &config);

        if hierarchy.depth() < 2 {
            return;
        }

        let fine = hierarchy.level(0).unwrap();
        let coarse = hierarchy.level(1).unwrap();

        // Create test field on fine level
        let fine_directions: Vec<_> = (0..fine.num_faces())
            .map(|f| {
                let n = fine.mesh.get_face_normal(f);
                arbitrary_perpendicular(&Vector3::new(n.x(), n.y(), n.z()))
            })
            .collect();
        let fine_normals: Vec<_> = (0..fine.num_faces())
            .map(|f| {
                let n = fine.mesh.get_face_normal(f);
                Vector3::new(n.x(), n.y(), n.z())
            })
            .collect();

        let (coarse_dirs, coarse_norms) =
            propagate_field_to_coarser(&fine_directions, &fine_normals, coarse);

        // All coarse directions should be perpendicular to their normals
        for i in 0..coarse_dirs.len() {
            assert_relative_eq!(coarse_dirs[i].dot(&coarse_norms[i]), 0.0, epsilon = 1e-9);
            assert_relative_eq!(coarse_dirs[i].norm(), 1.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn test_transport_same_normal() {
        let dir = Vector3::new(1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let transported = transport_direction(&dir, &normal, &normal);
        assert_relative_eq!(transported.x, 1.0, epsilon = 1e-10);
        assert_relative_eq!(transported.y, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_align_to_4rosy_identity() {
        let target = Vector3::new(1.0, 0.0, 0.0);
        let reference = Vector3::new(1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let aligned = align_to_4rosy(&target, &reference, &normal);
        assert_relative_eq!(aligned.dot(&reference), 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_align_to_4rosy_90_degree() {
        let target = Vector3::new(0.0, 1.0, 0.0);
        let reference = Vector3::new(1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let aligned = align_to_4rosy(&target, &reference, &normal);
        // Should pick the 90° rotation that aligns with reference
        assert_relative_eq!(aligned.dot(&reference), 1.0, epsilon = 1e-10);
    }
}
