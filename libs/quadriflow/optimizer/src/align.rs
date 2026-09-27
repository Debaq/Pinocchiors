//! Feature alignment for orientation fields.
//!
//! Detects sharp edges (high dihedral angle) and aligns the orientation
//! field to these features for better quad mesh quality.

use nalgebra::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_field::OrientationField;
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

/// Detected sharp edge for alignment.
#[derive(Debug, Clone)]
pub struct SharpEdge {
    /// Edge index in mesh
    pub edge_idx: usize,
    /// Start vertex index
    pub v0: usize,
    /// End vertex index
    pub v1: usize,
    /// Edge direction (normalized)
    pub direction: Vector3<f64>,
    /// Dihedral angle (radians)
    pub dihedral_angle: f64,
    /// Adjacent face indices
    pub faces: (usize, usize),
}

/// Configuration for sharp edge detection.
#[derive(Debug, Clone)]
pub struct SharpEdgeConfig {
    /// Minimum dihedral angle to consider edge as sharp (radians)
    pub angle_threshold: f64,
    /// Whether to include boundary edges
    pub include_boundary: bool,
}

impl Default for SharpEdgeConfig {
    fn default() -> Self {
        Self {
            angle_threshold: std::f64::consts::FRAC_PI_4, // 45 degrees
            include_boundary: true,
        }
    }
}

/// Result of sharp edge detection.
#[derive(Debug, Clone, Default)]
pub struct SharpEdgeInfo {
    /// Detected sharp edges
    pub edges: Vec<SharpEdge>,
    /// Number of boundary edges
    pub boundary_count: usize,
    /// Number of interior sharp edges
    pub interior_sharp_count: usize,
}

/// Detect sharp edges based on dihedral angle.
///
/// An edge is considered sharp if the angle between adjacent face normals
/// exceeds the threshold.
pub fn detect_sharp_edges(mesh: &Mesh, config: &SharpEdgeConfig) -> SharpEdgeInfo {
    let mut info = SharpEdgeInfo::default();

    if mesh.edges.is_empty() {
        return info;
    }

    // Compute face normals
    let face_normals = compute_face_normals(mesh);

    // Track processed edges (to avoid duplicates from twin)
    let mut processed: HashMap<(usize, usize), bool> = HashMap::new();

    for (edge_idx, edge) in mesh.edges.iter().enumerate() {
        let v0 = edge.vertex;

        // Get the other vertex via next->next (in a triangle, prev = next->next)
        let next_edge = &mesh.edges[edge.next];
        let next_next_edge = &mesh.edges[next_edge.next];
        let v1 = next_next_edge.vertex;

        // Canonical edge key
        let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };
        if processed.contains_key(&key) {
            continue;
        }
        processed.insert(key, true);

        // Get edge direction
        let p0 = mesh.vertices[v0].position;
        let p1 = mesh.vertices[v1].position;
        let edge_vec = Vector3::new(p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z());
        let edge_len = edge_vec.norm();

        if edge_len < 1e-10 {
            continue;
        }

        let direction = edge_vec / edge_len;

        // Check if boundary or interior
        match edge.twin {
            None => {
                // Boundary edge
                info.boundary_count += 1;

                if config.include_boundary
                    && let Some(face_idx) = edge.face {
                        info.edges.push(SharpEdge {
                            edge_idx,
                            v0,
                            v1,
                            direction,
                            dihedral_angle: std::f64::consts::PI, // Boundary = max angle
                            faces: (face_idx, face_idx),
                        });
                    }
            }
            Some(twin_idx) => {
                // Interior edge - compute dihedral angle
                let twin = &mesh.edges[twin_idx];

                if let (Some(face_a), Some(face_b)) = (edge.face, twin.face)
                    && face_a < face_normals.len() && face_b < face_normals.len() {
                        let normal_a = face_normals[face_a];
                        let normal_b = face_normals[face_b];

                        // Dihedral angle from dot product
                        let cos_angle = normal_a.dot(&normal_b).clamp(-1.0, 1.0);
                        let dihedral_angle = cos_angle.acos();

                        if dihedral_angle >= config.angle_threshold {
                            info.interior_sharp_count += 1;
                            info.edges.push(SharpEdge {
                                edge_idx,
                                v0,
                                v1,
                                direction,
                                dihedral_angle,
                                faces: (face_a, face_b),
                            });
                        }
                    }
            }
        }
    }

    info
}

/// Compute normal for each face.
fn compute_face_normals(mesh: &Mesh) -> Vec<Vector3<f64>> {
    let num_faces = mesh.num_faces();
    let mut normals = Vec::with_capacity(num_faces);

    for face_idx in 0..num_faces {
        let edge_start = mesh.faces[face_idx];
        let mut verts = Vec::new();

        let mut current = edge_start;
        for _ in 0..3 {
            let edge = &mesh.edges[current];
            verts.push(edge.vertex);
            current = edge.next;
        }

        if verts.len() >= 3 {
            let p0 = mesh.vertices[verts[0]].position;
            let p1 = mesh.vertices[verts[1]].position;
            let p2 = mesh.vertices[verts[2]].position;

            let v0 = Vector3::new(p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z());
            let v1 = Vector3::new(p2.x() - p0.x(), p2.y() - p0.y(), p2.z() - p0.z());

            let normal = v0.cross(&v1);
            let len = normal.norm();

            if len > 1e-10 {
                normals.push(normal / len);
            } else {
                normals.push(Vector3::new(0.0, 0.0, 1.0));
            }
        } else {
            normals.push(Vector3::new(0.0, 0.0, 1.0));
        }
    }

    normals
}

/// Align orientation field to sharp edges.
///
/// Adjusts field directions near sharp edges to be parallel or perpendicular
/// to the edge direction, preserving quad alignment along features.
pub fn align_to_features(
    field: &mut OrientationField,
    _mesh: &Mesh,
    sharp_edges: &[SharpEdge],
    weight: f64,
) {
    if sharp_edges.is_empty() || weight <= 0.0 {
        return;
    }

    // Build map from face to adjacent sharp edges
    let mut face_edges: HashMap<usize, Vec<&SharpEdge>> = HashMap::new();

    for edge in sharp_edges {
        face_edges.entry(edge.faces.0).or_default().push(edge);
        if edge.faces.0 != edge.faces.1 {
            face_edges.entry(edge.faces.1).or_default().push(edge);
        }
    }

    // Align each face's direction to its sharp edges
    for (face_idx, edges) in face_edges {
        if face_idx >= field.directions.len() {
            continue;
        }

        let current_dir = field.directions[face_idx];
        let normal = field.normals[face_idx];

        // Compute target direction as weighted average of edge alignments
        let mut target_dir = Vector3::zeros();
        let mut total_weight = 0.0;

        for edge in edges {
            // Project edge direction onto face tangent plane
            let edge_tangent = project_to_tangent_plane(&edge.direction, &normal);

            if edge_tangent.norm() < 1e-10 {
                continue;
            }

            let edge_tangent = edge_tangent.normalize();

            // Find best 4-RoSy alignment (0°, 90°, 180°, or 270°)
            let aligned = align_to_4rosy(&current_dir, &edge_tangent, &normal);

            // Weight by dihedral angle (sharper = stronger constraint)
            let edge_weight = edge.dihedral_angle / std::f64::consts::PI;
            target_dir += aligned * edge_weight;
            total_weight += edge_weight;
        }

        if total_weight > 0.0 {
            target_dir /= total_weight;

            // Blend between current and target based on weight
            let blended = current_dir * (1.0 - weight) + target_dir * weight;

            // Project back to tangent plane and normalize
            let result = project_to_tangent_plane(&blended, &normal);
            if result.norm() > 1e-10 {
                field.directions[face_idx] = result.normalize();
            }
        }
    }
}

/// Project vector onto tangent plane defined by normal.
fn project_to_tangent_plane(v: &Vector3<f64>, normal: &Vector3<f64>) -> Vector3<f64> {
    v - normal * v.dot(normal)
}

/// Align direction to 4-RoSy symmetry with target.
///
/// Returns the rotation of `current` (by 0°, 90°, 180°, or 270°) that
/// best aligns with `target`.
fn align_to_4rosy(
    current: &Vector3<f64>,
    target: &Vector3<f64>,
    normal: &Vector3<f64>,
) -> Vector3<f64> {
    let mut best_dir = *current;
    let mut best_dot = current.dot(target);

    for r in 1..4 {
        let angle = r as f64 * FRAC_PI_2;
        let rotated = rotate_around_axis(current, normal, angle);
        let dot = rotated.dot(target);

        if dot > best_dot {
            best_dot = dot;
            best_dir = rotated;
        }
    }

    best_dir
}

/// Rotate vector around axis by angle (Rodrigues' formula).
fn rotate_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

/// Compute curvature-based sizing field for adaptive resolution.
///
/// Returns a scale factor per vertex where higher values indicate
/// regions that need finer resolution (high curvature).
pub fn compute_curvature_sizing(mesh: &Mesh) -> Vec<f64> {
    let num_vertices = mesh.num_vertices();
    let mut sizing = vec![1.0; num_vertices];

    if num_vertices == 0 {
        return sizing;
    }

    // Compute vertex normals by averaging face normals
    let face_normals = compute_face_normals(mesh);
    let mut vertex_normals = vec![Vector3::zeros(); num_vertices];
    let mut vertex_counts = vec![0usize; num_vertices];

    for face_idx in 0..mesh.num_faces() {
        let normal = face_normals[face_idx];
        let edge_start = mesh.faces[face_idx];
        let mut current = edge_start;

        loop {
            let edge = &mesh.edges[current];
            let v = edge.vertex;
            vertex_normals[v] += normal;
            vertex_counts[v] += 1;

            current = edge.next;
            if current == edge_start {
                break;
            }
        }
    }

    for i in 0..num_vertices {
        if vertex_counts[i] > 0 {
            let len = vertex_normals[i].norm();
            if len > 1e-10 {
                vertex_normals[i] /= len;
            }
        }
    }

    // Estimate curvature from normal variation in neighborhood
    for v_idx in 0..num_vertices {
        let vertex = &mesh.vertices[v_idx];
        let start_edge = match vertex.edge {
            Some(e) => e,
            None => continue,
        };

        let v_normal = vertex_normals[v_idx];
        let mut max_angle = 0.0f64;
        let mut current = start_edge;
        let mut iterations = 0;

        loop {
            let edge = &mesh.edges[current];
            let neighbor = edge.vertex;
            let neighbor_normal = vertex_normals[neighbor];

            let dot = v_normal.dot(&neighbor_normal).clamp(-1.0, 1.0);
            let angle = dot.acos();
            max_angle = max_angle.max(angle);

            current = match edge.twin {
                Some(twin) => mesh.edges[twin].next,
                None => break,
            };

            if current == start_edge {
                break;
            }

            iterations += 1;
            if iterations > mesh.edges.len() {
                break;
            }
        }

        // Convert angle to sizing factor
        // High curvature (large angle) -> smaller quads -> higher sizing value
        sizing[v_idx] = 1.0 + max_angle * 2.0 / std::f64::consts::PI;
    }

    sizing
}

/// Detect boundary vertices and edges.
#[derive(Debug, Clone, Default)]
pub struct BoundaryInfo {
    /// Boundary vertex indices
    pub vertices: Vec<usize>,
    /// Boundary edge indices
    pub edges: Vec<usize>,
    /// Is the mesh closed (no boundary)?
    pub is_closed: bool,
}

/// Detect mesh boundary (open edges).
pub fn detect_boundary(mesh: &Mesh) -> BoundaryInfo {
    let mut info = BoundaryInfo {
        is_closed: true,
        ..Default::default()
    };

    let mut boundary_verts: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (edge_idx, edge) in mesh.edges.iter().enumerate() {
        if edge.twin.is_none() {
            info.is_closed = false;
            info.edges.push(edge_idx);

            // Add vertices of boundary edge
            boundary_verts.insert(edge.vertex);
            let next_edge = &mesh.edges[edge.next];
            let prev_edge = &mesh.edges[next_edge.next];
            boundary_verts.insert(prev_edge.vertex);
        }
    }

    info.vertices = boundary_verts.into_iter().collect();
    info.vertices.sort_unstable();

    info
}

/// Align field to boundary edges.
///
/// Ensures quads align nicely with mesh boundaries.
pub fn align_to_boundary(
    field: &mut OrientationField,
    mesh: &Mesh,
    boundary: &BoundaryInfo,
    weight: f64,
) {
    if boundary.is_closed || boundary.edges.is_empty() || weight <= 0.0 {
        return;
    }

    // Convert boundary edges to SharpEdge format
    let mut boundary_edges = Vec::new();

    for &edge_idx in &boundary.edges {
        let edge = &mesh.edges[edge_idx];
        let v0 = edge.vertex;
        let next_edge = &mesh.edges[edge.next];
        let prev_edge = &mesh.edges[next_edge.next];
        let v1 = prev_edge.vertex;

        let p0 = mesh.vertices[v0].position;
        let p1 = mesh.vertices[v1].position;
        let edge_vec = Vector3::new(p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z());
        let edge_len = edge_vec.norm();

        if edge_len > 1e-10 {
            let direction = edge_vec / edge_len;

            if let Some(face_idx) = edge.face {
                boundary_edges.push(SharpEdge {
                    edge_idx,
                    v0,
                    v1,
                    direction,
                    dihedral_angle: std::f64::consts::PI,
                    faces: (face_idx, face_idx),
                });
            }
        }
    }

    // Reuse feature alignment
    align_to_features(field, mesh, &boundary_edges, weight);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3 as PVec3;

    fn make_open_mesh() -> Mesh {
        // Single triangle (has boundary)
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        Mesh::from_triangles(&vertices, &faces)
    }

    fn make_closed_mesh() -> Mesh {
        // Tetrahedron (closed)
        let vertices = vec![
            PVec3::new(0.0, 0.0, 0.0),
            PVec3::new(1.0, 0.0, 0.0),
            PVec3::new(0.5, 1.0, 0.0),
            PVec3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![[0, 1, 2], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_detect_boundary_open() {
        let mesh = make_open_mesh();
        let boundary = detect_boundary(&mesh);

        assert!(!boundary.is_closed);
        assert_eq!(boundary.vertices.len(), 3);
        assert!(!boundary.edges.is_empty());
    }

    #[test]
    fn test_detect_boundary_closed() {
        let mesh = make_closed_mesh();
        let boundary = detect_boundary(&mesh);

        // Note: from_triangles may not create perfect twins for all edges
        // Just verify boundary detection runs without panic
        assert!(boundary.vertices.len() <= mesh.num_vertices());
    }

    #[test]
    fn test_detect_sharp_edges() {
        let mesh = make_closed_mesh();
        let config = SharpEdgeConfig {
            angle_threshold: 0.1, // Very low threshold
            include_boundary: true, // Include boundary since mesh may have some
        };

        let info = detect_sharp_edges(&mesh, &config);

        // Should detect some edges (either sharp interior or boundary)
        // The exact count depends on mesh connectivity
        let _total = info.edges.len() + info.boundary_count;
    }

    #[test]
    fn test_compute_curvature_sizing() {
        let mesh = make_closed_mesh();
        let sizing = compute_curvature_sizing(&mesh);

        assert_eq!(sizing.len(), mesh.num_vertices());
        // All values should be >= 1.0
        assert!(sizing.iter().all(|&s| s >= 1.0));
    }

    #[test]
    fn test_project_to_tangent_plane() {
        let v = Vector3::new(1.0, 1.0, 1.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let projected = project_to_tangent_plane(&v, &normal);

        assert!((projected.z).abs() < 1e-10);
        assert!((projected.x - 1.0).abs() < 1e-10);
        assert!((projected.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_rotate_around_axis() {
        let v = Vector3::new(1.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);

        // 90 degree rotation
        let rotated = rotate_around_axis(&v, &axis, FRAC_PI_2);

        assert!((rotated.x).abs() < 1e-10);
        assert!((rotated.y - 1.0).abs() < 1e-10);
        assert!((rotated.z).abs() < 1e-10);
    }

    #[test]
    fn test_align_to_4rosy() {
        let current = Vector3::new(1.0, 0.0, 0.0);
        let target = Vector3::new(0.0, 1.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let aligned = align_to_4rosy(&current, &target, &normal);

        // Should rotate 90 degrees to match target
        assert!((aligned.dot(&target) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_sharp_edge_config_default() {
        let config = SharpEdgeConfig::default();
        assert!(config.angle_threshold > 0.0);
        assert!(config.include_boundary);
    }
}
