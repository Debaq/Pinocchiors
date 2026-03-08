//! Seamless parametrization handling.
//!
//! Ensures parametrization is consistent across cut seams.
//! The key constraint is that when crossing from face A to face B,
//! the UV coordinates must transform according to the transition function:
//!
//! uv_B = R(rotation) * uv_A + translation
//!
//! where R is a 90° rotation matrix.

use nalgebra::Vector2;
use pinocchio_mesh::Mesh;
use quadriflow_field::OrientationField;
use std::f64::consts::FRAC_PI_2;

/// Transition function across a seam edge.
///
/// Represents the transformation needed when crossing from one face
/// to an adjacent face in the parametrization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeamTransition {
    /// Rotation (0, 1, 2, 3 for 0°, 90°, 180°, 270°)
    pub rotation: u8,
    /// Translation in U (integer offset)
    pub translate_u: i32,
    /// Translation in V (integer offset)
    pub translate_v: i32,
}

impl SeamTransition {
    /// Identity transition (no change).
    pub fn identity() -> Self {
        Self {
            rotation: 0,
            translate_u: 0,
            translate_v: 0,
        }
    }

    /// Create a transition with just rotation.
    pub fn with_rotation(rotation: u8) -> Self {
        Self {
            rotation: rotation % 4,
            translate_u: 0,
            translate_v: 0,
        }
    }

    /// Create a transition with rotation and translation.
    pub fn new(rotation: u8, translate_u: i32, translate_v: i32) -> Self {
        Self {
            rotation: rotation % 4,
            translate_u,
            translate_v,
        }
    }

    /// Compose two transitions: self followed by other.
    ///
    /// If crossing edge E1 requires transition T1, and then crossing E2
    /// requires T2, the combined transition is T2.compose(T1).
    pub fn compose(&self, other: &SeamTransition) -> SeamTransition {
        // Combined rotation
        let rotation = (self.rotation + other.rotation) % 4;

        // Rotate self's translation by other's rotation
        let (tu, tv) = rotate_translation(
            self.translate_u,
            self.translate_v,
            other.rotation,
        );

        SeamTransition {
            rotation,
            translate_u: tu + other.translate_u,
            translate_v: tv + other.translate_v,
        }
    }

    /// Inverse transition.
    pub fn inverse(&self) -> SeamTransition {
        let inv_rotation = (4 - self.rotation) % 4;

        // Rotate translation by inverse rotation, then negate
        let (tu, tv) = rotate_translation(
            self.translate_u,
            self.translate_v,
            inv_rotation,
        );

        SeamTransition {
            rotation: inv_rotation,
            translate_u: -tu,
            translate_v: -tv,
        }
    }

    /// Apply transition to UV coordinates.
    pub fn apply(&self, uv: Vector2<f64>) -> Vector2<f64> {
        let rotated = rotate_uv(uv, self.rotation);
        Vector2::new(
            rotated.x + self.translate_u as f64,
            rotated.y + self.translate_v as f64,
        )
    }

    /// Apply transition to integer UV coordinates.
    pub fn apply_int(&self, u: i32, v: i32) -> (i32, i32) {
        let (ru, rv) = rotate_int(u, v, self.rotation);
        (ru + self.translate_u, rv + self.translate_v)
    }
}

impl Default for SeamTransition {
    fn default() -> Self {
        Self::identity()
    }
}

/// Rotate integer translation by rotation index.
fn rotate_translation(u: i32, v: i32, rotation: u8) -> (i32, i32) {
    rotate_int(u, v, rotation)
}

/// Rotate integer coordinates by 90° * rotation.
fn rotate_int(u: i32, v: i32, rotation: u8) -> (i32, i32) {
    match rotation % 4 {
        0 => (u, v),
        1 => (-v, u),
        2 => (-u, -v),
        3 => (v, -u),
        _ => unreachable!(),
    }
}

/// Rotate UV coordinates by 90° * rotation.
fn rotate_uv(uv: Vector2<f64>, rotation: u8) -> Vector2<f64> {
    match rotation % 4 {
        0 => uv,
        1 => Vector2::new(-uv.y, uv.x),
        2 => Vector2::new(-uv.x, -uv.y),
        3 => Vector2::new(uv.y, -uv.x),
        _ => unreachable!(),
    }
}

/// Compute seamless transition data for the entire mesh.
///
/// Returns transition information for each edge in the mesh.
#[derive(Debug, Clone)]
pub struct SeamData {
    /// Transition for each half-edge (indexed by edge index)
    /// transition[e] is the transition when crossing edge e from its face
    pub transitions: Vec<SeamTransition>,
}

impl SeamData {
    /// Compute seam transitions from orientation field.
    pub fn from_orientation_field(
        mesh: &Mesh,
        orientation: &OrientationField,
    ) -> Self {
        let num_edges = mesh.edges.len();
        let mut transitions = vec![SeamTransition::identity(); num_edges];

        for edge_idx in 0..num_edges {
            let edge = &mesh.edges[edge_idx];

            // Only process edges with twins (interior edges)
            if let (Some(face_a), Some(twin_idx)) = (edge.face, edge.twin) {
                if let Some(face_b) = mesh.edges[twin_idx].face {
                    // Compute rotation between face_a and face_b
                    let rotation = compute_face_rotation(
                        orientation,
                        face_a,
                        face_b,
                    );

                    transitions[edge_idx] = SeamTransition::with_rotation(rotation);
                }
            }
        }

        SeamData { transitions }
    }

    /// Get transition for crossing a specific edge.
    pub fn transition(&self, edge_idx: usize) -> SeamTransition {
        self.transitions.get(edge_idx).copied().unwrap_or_default()
    }

    /// Verify that transitions are consistent around each vertex.
    ///
    /// For a closed surface, going around a vertex should give identity
    /// at regular vertices, or a specific rotation at singularities.
    pub fn verify_consistency(&self, mesh: &Mesh) -> Vec<u8> {
        let num_vertices = mesh.num_vertices();
        let mut vertex_holonomy = vec![0u8; num_vertices];

        for v_idx in 0..num_vertices {
            let holonomy = self.compute_vertex_holonomy(mesh, v_idx);
            vertex_holonomy[v_idx] = holonomy;
        }

        vertex_holonomy
    }

    /// Compute holonomy around a vertex (total rotation when going around).
    fn compute_vertex_holonomy(&self, mesh: &Mesh, vertex_idx: usize) -> u8 {
        let vertex = &mesh.vertices[vertex_idx];
        let start_edge = match vertex.edge {
            Some(e) => e,
            None => return 0,
        };

        let mut total_rotation = 0u8;
        let mut current_edge = start_edge;

        loop {
            let transition = self.transition(current_edge);
            total_rotation = (total_rotation + transition.rotation) % 4;

            // Move to next edge around vertex
            let edge = &mesh.edges[current_edge];
            current_edge = match edge.twin {
                Some(twin) => mesh.edges[twin].next,
                None => break, // Boundary
            };

            if current_edge == start_edge {
                break;
            }
        }

        total_rotation
    }
}

/// Compute the rotation index between two faces based on their orientation fields.
fn compute_face_rotation(
    orientation: &OrientationField,
    face_a: usize,
    face_b: usize,
) -> u8 {
    let dir_a = orientation.directions[face_a];
    let normal_a = orientation.normals[face_a];
    let dir_b = orientation.directions[face_b];
    let normal_b = orientation.normals[face_b];

    // Transport dir_a to face_b's tangent plane
    let transported = transport_direction(&dir_a, &normal_a, &normal_b);

    // Find rotation that aligns transported with dir_b
    find_best_rotation(&transported, &dir_b, &normal_b)
}

/// Transport a tangent vector between faces.
fn transport_direction(
    dir: &nalgebra::Vector3<f64>,
    from_normal: &nalgebra::Vector3<f64>,
    to_normal: &nalgebra::Vector3<f64>,
) -> nalgebra::Vector3<f64> {
    let cross = from_normal.cross(to_normal);
    let sin_angle = cross.norm();

    if sin_angle < 1e-10 {
        return *dir;
    }

    let axis = cross / sin_angle;
    let cos_angle = from_normal.dot(to_normal).clamp(-1.0, 1.0);
    let angle = cos_angle.acos();

    // Rodrigues' formula
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    dir * cos_a + axis.cross(dir) * sin_a + axis * axis.dot(dir) * (1.0 - cos_a)
}

/// Find the rotation (0-3) that best aligns source with target.
fn find_best_rotation(
    source: &nalgebra::Vector3<f64>,
    target: &nalgebra::Vector3<f64>,
    normal: &nalgebra::Vector3<f64>,
) -> u8 {
    let mut best_rotation = 0u8;
    let mut best_dot = source.dot(target);

    for r in 1..4 {
        let angle = r as f64 * FRAC_PI_2;
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let rotated = source * cos_a + normal.cross(source) * sin_a
            + normal * normal.dot(source) * (1.0 - cos_a);

        let dot = rotated.dot(target);
        if dot > best_dot {
            best_dot = dot;
            best_rotation = r as u8;
        }
    }

    best_rotation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_transition() {
        let t = SeamTransition::identity();
        assert_eq!(t.rotation, 0);
        assert_eq!(t.translate_u, 0);
        assert_eq!(t.translate_v, 0);
    }

    #[test]
    fn test_transition_compose_rotations() {
        let t1 = SeamTransition::with_rotation(1); // 90°
        let t2 = SeamTransition::with_rotation(1); // 90°

        let combined = t1.compose(&t2);
        assert_eq!(combined.rotation, 2); // 180°
    }

    #[test]
    fn test_transition_inverse() {
        let t = SeamTransition::new(1, 5, -3);
        let inv = t.inverse();
        let identity = t.compose(&inv);

        assert_eq!(identity.rotation, 0);
        assert_eq!(identity.translate_u, 0);
        assert_eq!(identity.translate_v, 0);
    }

    #[test]
    fn test_rotate_int() {
        // 90° rotation
        assert_eq!(rotate_int(1, 0, 1), (0, 1));
        assert_eq!(rotate_int(0, 1, 1), (-1, 0));

        // 180° rotation
        assert_eq!(rotate_int(1, 0, 2), (-1, 0));
        assert_eq!(rotate_int(0, 1, 2), (0, -1));

        // 270° rotation
        assert_eq!(rotate_int(1, 0, 3), (0, -1));

        // Full rotation (360° = identity)
        assert_eq!(rotate_int(5, 7, 4), (5, 7));
    }

    #[test]
    fn test_apply_transition() {
        let t = SeamTransition::new(1, 2, 3);
        let uv = Vector2::new(1.0, 0.0);

        let result = t.apply(uv);

        // 90° rotation of (1,0) is (0,1), plus translation (2,3)
        assert!((result.x - 2.0).abs() < 1e-10);
        assert!((result.y - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_apply_int_transition() {
        let t = SeamTransition::new(2, 1, 1); // 180° + (1,1)

        let (u, v) = t.apply_int(3, 4);

        // 180° rotation of (3,4) is (-3,-4), plus (1,1) = (-2,-3)
        assert_eq!(u, -2);
        assert_eq!(v, -3);
    }

    #[test]
    fn test_transition_chain() {
        // Going around a square: 4 rotations of 90° should give identity
        let t90 = SeamTransition::with_rotation(1);

        let t1 = t90;
        let t2 = t1.compose(&t90);
        let t3 = t2.compose(&t90);
        let t4 = t3.compose(&t90);

        assert_eq!(t4.rotation, 0);
    }
}
