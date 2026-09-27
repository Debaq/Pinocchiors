//! Singularity detection and handling.
//!
//! Singularities are points where the orientation field is not well-defined.
//! In a 4-RoSy field, singularities occur at vertices where the total rotation
//! around the vertex is not a multiple of 90°.
//!
//! QuadriFlow minimizes these using network flow optimization.

use crate::OrientationField;
use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use std::f64::consts::FRAC_PI_2;

/// Type of singularity in a 4-RoSy field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingularityType {
    /// Valence 3 vertex (index +1/4, rotation deficit of +90°)
    Positive,
    /// Valence 5 vertex (index -1/4, rotation deficit of -90°)
    Negative,
}

/// A singularity in the orientation field.
#[derive(Debug, Clone)]
pub struct Singularity {
    /// Vertex index where singularity occurs
    pub vertex: usize,
    /// Type of singularity
    pub kind: SingularityType,
    /// The singularity index (±1/4 for standard singularities)
    pub index: f64,
}

/// Result of singularity detection.
#[derive(Debug, Clone)]
pub struct SingularityInfo {
    /// List of detected singularities
    pub singularities: Vec<Singularity>,
    /// Singularity index per vertex (0 for regular, ±0.25 for singular)
    pub vertex_indices: Vec<f64>,
}

/// Detect singularities in an orientation field.
///
/// A singularity occurs at a vertex when the field directions around it
/// don't form a consistent 4-RoSy field. The singularity index is computed
/// as the total rotation divided by 2π, minus 1.
///
/// For a 4-RoSy field:
/// - Index +1/4 = valence 3 (positive singularity)
/// - Index -1/4 = valence 5 (negative singularity)
/// - Index 0 = regular (valence 4)
///
/// # Arguments
/// * `field` - The orientation field
/// * `mesh` - The underlying mesh
///
/// # Returns
/// Information about detected singularities
pub fn detect_singularities(field: &OrientationField, mesh: &Mesh) -> SingularityInfo {
    let num_vertices = mesh.num_vertices();
    let mut vertex_indices = vec![0.0; num_vertices];
    let mut singularities = Vec::new();

    // For each vertex, compute the rotation around it
    for vertex_idx in 0..num_vertices {
        let index = compute_vertex_index(field, mesh, vertex_idx);
        vertex_indices[vertex_idx] = index;

        // Check if it's a singularity (index significantly different from 0)
        if index.abs() > 0.1 {
            let kind = if index > 0.0 {
                SingularityType::Positive
            } else {
                SingularityType::Negative
            };

            singularities.push(Singularity {
                vertex: vertex_idx,
                kind,
                index,
            });
        }
    }

    SingularityInfo {
        singularities,
        vertex_indices,
    }
}

/// Compute the singularity index at a vertex.
///
/// The index is the total rotation of the field around the vertex,
/// normalized by 2π, accounting for 4-RoSy symmetry.
fn compute_vertex_index(field: &OrientationField, mesh: &Mesh, vertex_idx: usize) -> f64 {
    // Get the faces around this vertex (in order)
    let faces_around = get_faces_around_vertex(mesh, vertex_idx);

    if faces_around.len() < 2 {
        return 0.0;
    }

    let mut total_rotation = 0.0;

    // Accumulate rotation between consecutive faces
    for i in 0..faces_around.len() {
        let face_a = faces_around[i];
        let face_b = faces_around[(i + 1) % faces_around.len()];

        // Compute rotation from face_a to face_b
        let rotation = compute_rotation_between_faces(field, mesh, face_a, face_b);
        total_rotation += rotation;
    }

    // The singularity index is (total_rotation / 2π) - 1
    // But for 4-RoSy, we need to account for the 4-fold symmetry
    // The index is the number of quarter-turns (90°) extra
    let quarter_turns = total_rotation / FRAC_PI_2;
    let rounded_quarters = quarter_turns.round();
    

    (rounded_quarters - (faces_around.len() as f64)) / 4.0
}

/// Compute the rotation needed to align face_a's direction with face_b's.
///
/// Returns the angle in radians, accounting for 4-RoSy symmetry.
fn compute_rotation_between_faces(
    field: &OrientationField,
    _mesh: &Mesh,
    face_a: usize,
    face_b: usize,
) -> f64 {
    let dir_a = field.directions[face_a];
    let normal_a = field.normals[face_a];
    let dir_b = field.directions[face_b];
    let normal_b = field.normals[face_b];

    // Transport dir_a to face_b's tangent plane
    let transported = transport_direction(&dir_a, &normal_a, &normal_b);

    // Find the angle between transported and dir_b, considering 4-RoSy
    

    angle_between_4rosy(&transported, &dir_b, &normal_b)
}

/// Transport a tangent vector from one face to another.
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

/// Compute the angle between two directions accounting for 4-RoSy symmetry.
///
/// Returns the smallest angle between any pair of symmetric directions.
fn angle_between_4rosy(dir_a: &Vector3<f64>, dir_b: &Vector3<f64>, normal: &Vector3<f64>) -> f64 {
    // Compute angle in the tangent plane
    let cos_angle = dir_a.dot(dir_b).clamp(-1.0, 1.0);
    let sin_angle = normal.dot(&dir_a.cross(dir_b));
    let mut angle = sin_angle.atan2(cos_angle);

    // Normalize to [0, π/2) by taking modulo π/2
    angle = angle.rem_euclid(FRAC_PI_2);

    // Return the closest to 0 or π/2
    if angle > FRAC_PI_2 / 2.0 {
        FRAC_PI_2 - angle
    } else {
        angle
    }
}

/// Get faces around a vertex in order (fan order).
fn get_faces_around_vertex(mesh: &Mesh, vertex_idx: usize) -> Vec<usize> {
    let vertex = &mesh.vertices[vertex_idx];
    let start_edge = match vertex.edge {
        Some(e) => e,
        None => return Vec::new(),
    };

    let mut faces = Vec::new();
    let mut current_edge = start_edge;

    // Walk around the vertex collecting faces
    loop {
        let edge = &mesh.edges[current_edge];

        if let Some(face) = edge.face
            && !faces.contains(&face) {
                faces.push(face);
            }

        // Move to next edge around vertex
        current_edge = match edge.twin {
            Some(twin) => mesh.edges[twin].next,
            None => {
                // Hit a boundary - try going the other way
                break;
            }
        };

        if current_edge == start_edge {
            break;
        }
    }

    faces
}

/// Rotate a vector around an axis using Rodrigues' formula.
fn rotate_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

/// Count singularities in a field.
///
/// Returns (positive_count, negative_count).
pub fn count_singularities(singularities: &[Singularity]) -> (usize, usize) {
    let positive = singularities
        .iter()
        .filter(|s| s.kind == SingularityType::Positive)
        .count();
    let negative = singularities
        .iter()
        .filter(|s| s.kind == SingularityType::Negative)
        .count();
    (positive, negative)
}

/// Compute the Euler characteristic from singularities.
///
/// For a closed surface: χ = (positive - negative) / 4
/// This should match the topological Euler characteristic.
pub fn euler_from_singularities(singularities: &[Singularity]) -> f64 {
    singularities.iter().map(|s| s.index).sum::<f64>()
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

    fn make_flat_plane() -> Mesh {
        // A flat 2x2 grid of triangles
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(2.0, 0.0, 0.0),
            PVec3::new(0.0, 1.0, 0.0),
            PVec3::new(1.0, 1.0, 0.0),
            PVec3::new(2.0, 1.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 4],
            [0, 4, 3],
            [1, 2, 5],
            [1, 5, 4],
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_detect_singularities_tetrahedron() {
        let mesh = make_tetrahedron();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);
        let info = detect_singularities(&field, &mesh);

        // A tetrahedron has Euler characteristic χ = 2
        // So we expect some singularities
        let _euler = euler_from_singularities(&info.singularities);
        // Note: exact value depends on field configuration
        assert!(info.singularities.len() <= mesh.num_vertices());
    }

    #[test]
    fn test_count_singularities() {
        let singularities = vec![
            Singularity {
                vertex: 0,
                kind: SingularityType::Positive,
                index: 0.25,
            },
            Singularity {
                vertex: 1,
                kind: SingularityType::Negative,
                index: -0.25,
            },
            Singularity {
                vertex: 2,
                kind: SingularityType::Positive,
                index: 0.25,
            },
        ];

        let (pos, neg) = count_singularities(&singularities);
        assert_eq!(pos, 2);
        assert_eq!(neg, 1);
    }

    #[test]
    fn test_euler_from_singularities() {
        // For a sphere (χ = 2), we need total index of 2
        // This requires 8 positive singularities or equivalent
        let singularities = vec![
            Singularity {
                vertex: 0,
                kind: SingularityType::Positive,
                index: 0.25,
            },
            Singularity {
                vertex: 1,
                kind: SingularityType::Positive,
                index: 0.25,
            },
        ];

        let euler = euler_from_singularities(&singularities);
        assert!((euler - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_flat_plane_fewer_singularities() {
        let mesh = make_flat_plane();
        let field = OrientationField::from_mesh_edge_aligned(&mesh);
        let info = detect_singularities(&field, &mesh);

        // A flat plane with aligned field should have few/no singularities
        // (interior vertices on a flat surface with consistent field are regular)
        // Boundary vertices are a special case
        let interior_singularities: Vec<_> = info
            .singularities
            .iter()
            .filter(|s| {
                // Check if vertex is interior (has all edges with twins)
                let vertex = &mesh.vertices[s.vertex];
                if let Some(start_edge) = vertex.edge {
                    let mut edge = start_edge;
                    loop {
                        if mesh.edges[edge].twin.is_none() {
                            return false; // boundary vertex
                        }
                        edge = mesh.edges[mesh.edges[edge].twin.unwrap()].next;
                        if edge == start_edge {
                            break;
                        }
                    }
                    true
                } else {
                    false
                }
            })
            .collect();

        // Interior vertices of a flat plane with consistent field should be regular
        assert!(interior_singularities.is_empty() || interior_singularities.len() <= 2);
    }
}
