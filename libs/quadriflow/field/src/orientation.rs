//! Orientation field (4-RoSy) computation.
//!
//! A 4-RoSy field assigns a cross (4 directions) to each face/vertex,
//! representing the local quad orientation.

use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use rand::Rng;
use std::f64::consts::FRAC_PI_2;

/// Orientation field on a mesh surface.
///
/// Stores a 4-RoSy (4-fold rotational symmetry) field that guides
/// the quad edge directions during remeshing.
///
/// Each face stores one representative direction in its tangent plane.
/// The other 3 directions are implicit via 90° rotations around the normal.
#[derive(Debug, Clone)]
pub struct OrientationField {
    /// One representative direction per face (the other 3 are implicit via 90° rotations)
    pub directions: Vec<Vector3<f64>>,
    /// Face normals (cached for efficiency)
    pub normals: Vec<Vector3<f64>>,
}

impl OrientationField {
    /// Create an empty orientation field.
    pub fn new() -> Self {
        Self {
            directions: Vec::new(),
            normals: Vec::new(),
        }
    }

    /// Initialize the orientation field from a mesh with random tangent directions.
    ///
    /// For each face:
    /// 1. Compute the face normal
    /// 2. Generate a random direction in the tangent plane
    ///
    /// # Arguments
    /// * `mesh` - Input triangle mesh
    ///
    /// # Returns
    /// An orientation field with one direction per face
    pub fn from_mesh(mesh: &Mesh) -> Self {
        let num_faces = mesh.num_faces();
        let mut directions = Vec::with_capacity(num_faces);
        let mut normals = Vec::with_capacity(num_faces);
        let mut rng = rand::thread_rng();

        for face_idx in 0..num_faces {
            // Compute face normal
            let normal = mesh.get_face_normal(face_idx);
            let na_normal = Vector3::new(normal.x(), normal.y(), normal.z());
            normals.push(na_normal);

            // Generate random direction in tangent plane
            let tangent = random_tangent_direction(&na_normal, &mut rng);
            directions.push(tangent);
        }

        Self { directions, normals }
    }

    /// Initialize the orientation field from a mesh with directions aligned to the first edge.
    ///
    /// This provides a deterministic initialization useful for testing.
    pub fn from_mesh_edge_aligned(mesh: &Mesh) -> Self {
        let num_faces = mesh.num_faces();
        let mut directions = Vec::with_capacity(num_faces);
        let mut normals = Vec::with_capacity(num_faces);

        for face_idx in 0..num_faces {
            let normal = mesh.get_face_normal(face_idx);
            let na_normal = Vector3::new(normal.x(), normal.y(), normal.z());
            normals.push(na_normal);

            // Use first edge as initial direction
            let [p0, p1, _] = mesh.get_face_positions(face_idx);
            let edge = Vector3::new(p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z());

            // Project onto tangent plane and normalize
            let tangent = project_to_tangent_plane(&edge, &na_normal);
            directions.push(tangent);
        }

        Self { directions, normals }
    }

    /// Number of field samples (one per face).
    pub fn len(&self) -> usize {
        self.directions.len()
    }

    /// Check if field is empty.
    pub fn is_empty(&self) -> bool {
        self.directions.is_empty()
    }

    /// Get the representative direction for a face.
    pub fn direction(&self, face_idx: usize) -> Vector3<f64> {
        self.directions[face_idx]
    }

    /// Get the normal for a face.
    pub fn normal(&self, face_idx: usize) -> Vector3<f64> {
        self.normals[face_idx]
    }

    /// Get all 4 directions for a face (the cross).
    ///
    /// Returns [d, d rotated 90°, d rotated 180°, d rotated 270°]
    /// where rotations are around the face normal.
    pub fn cross_directions(&self, face_idx: usize) -> [Vector3<f64>; 4] {
        let d = self.directions[face_idx];
        let n = self.normals[face_idx];

        // Rotate d around n by 90° increments
        let d90 = rotate_around_axis(&d, &n, FRAC_PI_2);
        let d180 = rotate_around_axis(&d, &n, std::f64::consts::PI);
        let d270 = rotate_around_axis(&d, &n, 3.0 * FRAC_PI_2);

        [d, d90, d180, d270]
    }

    /// Set the direction for a face.
    ///
    /// The direction is automatically projected onto the tangent plane
    /// and normalized.
    pub fn set_direction(&mut self, face_idx: usize, dir: Vector3<f64>) {
        let normal = self.normals[face_idx];
        self.directions[face_idx] = project_to_tangent_plane(&dir, &normal);
    }

    /// Find the closest of the 4 symmetric directions to a target.
    ///
    /// This is essential for comparing directions across faces, since
    /// two 4-RoSy fields are "equivalent" if one can be rotated by
    /// multiples of 90° to match the other.
    ///
    /// Returns (best_direction, rotation_index) where rotation_index is 0-3.
    pub fn closest_symmetric_direction(
        &self,
        face_idx: usize,
        target: &Vector3<f64>,
    ) -> (Vector3<f64>, usize) {
        let cross = self.cross_directions(face_idx);
        let mut best_idx = 0;
        let mut best_dot = cross[0].dot(target);

        for (i, dir) in cross.iter().enumerate().skip(1) {
            let dot = dir.dot(target);
            if dot > best_dot {
                best_dot = dot;
                best_idx = i;
            }
        }

        (cross[best_idx], best_idx)
    }
}

impl Default for OrientationField {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a random unit vector in the tangent plane (perpendicular to normal).
fn random_tangent_direction<R: Rng>(normal: &Vector3<f64>, rng: &mut R) -> Vector3<f64> {
    // Generate a random vector
    let random_vec = Vector3::new(
        rng.r#gen::<f64>() - 0.5,
        rng.r#gen::<f64>() - 0.5,
        rng.r#gen::<f64>() - 0.5,
    );

    // Project onto tangent plane
    project_to_tangent_plane(&random_vec, normal)
}

/// Project a vector onto the tangent plane defined by a normal.
///
/// Returns a normalized vector in the tangent plane.
fn project_to_tangent_plane(v: &Vector3<f64>, normal: &Vector3<f64>) -> Vector3<f64> {
    // Remove component parallel to normal: v - (v·n)n
    let projected = v - normal * v.dot(normal);

    // Normalize, with fallback for degenerate cases
    let len = projected.norm();
    if len > 1e-10 {
        projected / len
    } else {
        // v was parallel to normal, construct an arbitrary tangent
        arbitrary_perpendicular(normal)
    }
}

/// Construct an arbitrary unit vector perpendicular to the given vector.
fn arbitrary_perpendicular(v: &Vector3<f64>) -> Vector3<f64> {
    // Choose the axis most different from v
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

/// Rotate a vector around an axis by a given angle (in radians).
///
/// Uses Rodrigues' rotation formula.
fn rotate_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Rodrigues' formula: v_rot = v*cos(θ) + (k×v)*sin(θ) + k*(k·v)*(1-cos(θ))
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use pinocchio_math::Vector3 as PVec3;

    fn make_single_triangle_mesh() -> Mesh {
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        Mesh::from_triangles(&vertices, &faces)
    }

    fn make_tetrahedron() -> Mesh {
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
    fn test_from_mesh() {
        let mesh = make_tetrahedron();
        let field = OrientationField::from_mesh(&mesh);

        assert_eq!(field.len(), 4);
        assert!(!field.is_empty());

        // Each direction should be normalized
        for i in 0..field.len() {
            assert_relative_eq!(field.direction(i).norm(), 1.0, epsilon = 1e-10);
        }
    }

    #[test]
    fn test_directions_perpendicular_to_normal() {
        let mesh = make_tetrahedron();
        let field = OrientationField::from_mesh(&mesh);

        for i in 0..field.len() {
            let d = field.direction(i);
            let n = field.normal(i);

            // Direction should be perpendicular to normal
            assert_relative_eq!(d.dot(&n), 0.0, epsilon = 1e-10);
        }
    }

    #[test]
    fn test_cross_directions() {
        let mesh = make_single_triangle_mesh();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);

        let cross = field.cross_directions(0);
        let n = field.normal(0);

        // All 4 directions should be unit vectors
        for d in &cross {
            assert_relative_eq!(d.norm(), 1.0, epsilon = 1e-10);
        }

        // All 4 should be perpendicular to normal
        for d in &cross {
            assert_relative_eq!(d.dot(&n), 0.0, epsilon = 1e-10);
        }

        // Consecutive directions should be 90° apart (dot product = 0)
        assert_relative_eq!(cross[0].dot(&cross[1]), 0.0, epsilon = 1e-10);
        assert_relative_eq!(cross[1].dot(&cross[2]), 0.0, epsilon = 1e-10);
        assert_relative_eq!(cross[2].dot(&cross[3]), 0.0, epsilon = 1e-10);

        // Opposite directions should be anti-parallel
        assert_relative_eq!(cross[0].dot(&cross[2]), -1.0, epsilon = 1e-10);
        assert_relative_eq!(cross[1].dot(&cross[3]), -1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_closest_symmetric_direction() {
        let mesh = make_single_triangle_mesh();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);

        // The closest direction to the original should be itself
        let d = field.direction(0);
        let (closest, idx) = field.closest_symmetric_direction(0, &d);
        assert_eq!(idx, 0);
        assert_relative_eq!(closest.dot(&d), 1.0, epsilon = 1e-10);

        // Rotate target by 90° - should match direction 1
        let n = field.normal(0);
        let rotated = rotate_around_axis(&d, &n, FRAC_PI_2);
        let (_, idx) = field.closest_symmetric_direction(0, &rotated);
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_set_direction() {
        let mesh = make_single_triangle_mesh();
        let mut field = OrientationField::from_mesh_edge_aligned(&mesh);

        // Set a new direction (not in tangent plane)
        let new_dir = Vector3::new(1.0, 1.0, 1.0);
        field.set_direction(0, new_dir);

        let d = field.direction(0);
        let n = field.normal(0);

        // Should still be normalized and perpendicular to normal
        assert_relative_eq!(d.norm(), 1.0, epsilon = 1e-10);
        assert_relative_eq!(d.dot(&n), 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_arbitrary_perpendicular() {
        let test_vectors = [
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 1.0, 1.0).normalize(),
        ];

        for v in &test_vectors {
            let perp = arbitrary_perpendicular(v);
            assert_relative_eq!(perp.norm(), 1.0, epsilon = 1e-10);
            assert_relative_eq!(perp.dot(v), 0.0, epsilon = 1e-10);
        }
    }

    #[test]
    fn test_rotate_around_axis() {
        let v = Vector3::new(1.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);

        // Rotate 90° around Z
        let rotated = rotate_around_axis(&v, &axis, FRAC_PI_2);
        assert_relative_eq!(rotated.x, 0.0, epsilon = 1e-10);
        assert_relative_eq!(rotated.y, 1.0, epsilon = 1e-10);
        assert_relative_eq!(rotated.z, 0.0, epsilon = 1e-10);

        // Rotate 180° around Z
        let rotated = rotate_around_axis(&v, &axis, std::f64::consts::PI);
        assert_relative_eq!(rotated.x, -1.0, epsilon = 1e-10);
        assert_relative_eq!(rotated.y, 0.0, epsilon = 1e-10);
    }
}
