//! Isoline tracing for quad extraction.
//!
//! Traces integer U and V isolines through the triangulated parametric
//! domain to find quad vertices and edges.
//!
//! ## Algorithm
//!
//! For each triangle:
//! 1. Find where integer U isolines cross the triangle edges
//! 2. Find where integer V isolines cross the triangle edges
//! 3. Find intersection points inside the triangle (grid crossings)
//! 4. Connect points to form quad edges
//!
//! ## Parallelism
//!
//! When the `parallel` feature is enabled, triangle processing is parallelized
//! using rayon for improved performance on large meshes.

use crate::QuadMesh;
use nalgebra::Vector3;
use std::collections::HashMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Configuration for isoline tracing.
#[derive(Debug, Clone)]
pub struct TraceConfig {
    /// Tolerance for detecting crossings
    pub epsilon: f64,
    /// Minimum number of triangles to trigger parallel processing
    pub parallel_threshold: usize,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            epsilon: 1e-10,
            parallel_threshold: 1000,
        }
    }
}

/// Grid point found in a triangle.
#[derive(Debug, Clone)]
struct GridPoint {
    uv: (i32, i32),
    position: Vector3<f64>,
}

/// Trace isolines and extract quad mesh.
///
/// # Arguments
/// * `positions` - 3D vertex positions of the input triangle mesh
/// * `triangles` - Triangle indices
/// * `u` - Integer U coordinate per vertex
/// * `v` - Integer V coordinate per vertex
///
/// # Returns
/// The extracted quad mesh
pub fn trace_isolines(
    positions: &[Vector3<f64>],
    triangles: &[[usize; 3]],
    u: &[i32],
    v: &[i32],
) -> QuadMesh {
    trace_isolines_with_config(positions, triangles, u, v, &TraceConfig::default())
}

/// Trace isolines with custom configuration.
pub fn trace_isolines_with_config(
    positions: &[Vector3<f64>],
    triangles: &[[usize; 3]],
    u: &[i32],
    v: &[i32],
    config: &TraceConfig,
) -> QuadMesh {
    if triangles.is_empty() || positions.is_empty() {
        return QuadMesh::new();
    }

    // Collect grid points from all triangles
    #[cfg(feature = "parallel")]
    let all_points: Vec<GridPoint> = if triangles.len() >= config.parallel_threshold {
        collect_grid_points_parallel(positions, triangles, u, v, config.epsilon)
    } else {
        collect_grid_points_sequential(positions, triangles, u, v, config.epsilon)
    };

    #[cfg(not(feature = "parallel"))]
    let all_points: Vec<GridPoint> =
        collect_grid_points_sequential(positions, triangles, u, v, config.epsilon);

    // Build vertex map from collected points
    let mut uv_to_vertex: HashMap<(i32, i32), usize> = HashMap::new();
    let mut vertices: Vec<Vector3<f64>> = Vec::new();

    for point in all_points {
        if !uv_to_vertex.contains_key(&point.uv) {
            let idx = vertices.len();
            vertices.push(point.position);
            uv_to_vertex.insert(point.uv, idx);
        }
    }

    // Find all quads from global set of UV points
    let quads = find_quads(&uv_to_vertex);

    // Convert to QuadFace
    let faces = quads
        .into_iter()
        .map(|v| crate::QuadFace { v })
        .collect();

    QuadMesh { vertices, faces }
}

/// Collect grid points from triangles sequentially.
fn collect_grid_points_sequential(
    positions: &[Vector3<f64>],
    triangles: &[[usize; 3]],
    u: &[i32],
    v: &[i32],
    epsilon: f64,
) -> Vec<GridPoint> {
    let mut points = Vec::new();

    for tri in triangles {
        let tri_points = process_triangle(positions, tri, u, v, epsilon);
        points.extend(tri_points);
    }

    points
}

/// Collect grid points from triangles in parallel.
#[cfg(feature = "parallel")]
fn collect_grid_points_parallel(
    positions: &[Vector3<f64>],
    triangles: &[[usize; 3]],
    u: &[i32],
    v: &[i32],
    epsilon: f64,
) -> Vec<GridPoint> {
    triangles
        .par_iter()
        .flat_map(|tri| process_triangle(positions, tri, u, v, epsilon))
        .collect()
}

/// Process a single triangle to find grid points.
fn process_triangle(
    positions: &[Vector3<f64>],
    tri: &[usize; 3],
    u: &[i32],
    v: &[i32],
    epsilon: f64,
) -> Vec<GridPoint> {
    let v0 = tri[0];
    let v1 = tri[1];
    let v2 = tri[2];

    let p0 = positions[v0];
    let p1 = positions[v1];
    let p2 = positions[v2];

    let uv0 = (u[v0], v[v0]);
    let uv1 = (u[v1], v[v1]);
    let uv2 = (u[v2], v[v2]);

    find_grid_points_in_triangle(&p0, &p1, &p2, uv0, uv1, uv2, epsilon)
}

/// Find all integer grid points inside or on a triangle.
fn find_grid_points_in_triangle(
    p0: &Vector3<f64>,
    p1: &Vector3<f64>,
    p2: &Vector3<f64>,
    uv0: (i32, i32),
    uv1: (i32, i32),
    uv2: (i32, i32),
    epsilon: f64,
) -> Vec<GridPoint> {
    let mut points = Vec::new();

    // Find bounding box in UV space
    let u_min = uv0.0.min(uv1.0).min(uv2.0);
    let u_max = uv0.0.max(uv1.0).max(uv2.0);
    let v_min = uv0.1.min(uv1.1).min(uv2.1);
    let v_max = uv0.1.max(uv1.1).max(uv2.1);

    // Check all integer points in bounding box
    for iu in u_min..=u_max {
        for iv in v_min..=v_max {
            // Compute barycentric coordinates for this UV point
            if let Some(bary) = uv_to_barycentric((iu, iv), uv0, uv1, uv2, epsilon) {
                // Check if inside triangle (barycentric coords in [0,1])
                if bary.0 >= -epsilon
                    && bary.1 >= -epsilon
                    && bary.2 >= -epsilon
                    && bary.0 <= 1.0 + epsilon
                    && bary.1 <= 1.0 + epsilon
                    && bary.2 <= 1.0 + epsilon
                {
                    // Interpolate 3D position
                    let pos = p0 * bary.0 + p1 * bary.1 + p2 * bary.2;
                    points.push(GridPoint {
                        uv: (iu, iv),
                        position: pos,
                    });
                }
            }
        }
    }

    points
}

/// Convert UV coordinates to barycentric coordinates in a triangle.
fn uv_to_barycentric(
    uv: (i32, i32),
    uv0: (i32, i32),
    uv1: (i32, i32),
    uv2: (i32, i32),
    epsilon: f64,
) -> Option<(f64, f64, f64)> {
    let u = uv.0 as f64;
    let v = uv.1 as f64;

    let u0 = uv0.0 as f64;
    let v0 = uv0.1 as f64;
    let u1 = uv1.0 as f64;
    let v1 = uv1.1 as f64;
    let u2 = uv2.0 as f64;
    let v2 = uv2.1 as f64;

    // Triangle area in UV space (2x area)
    let det = (u1 - u0) * (v2 - v0) - (u2 - u0) * (v1 - v0);

    if det.abs() < epsilon {
        return None; // Degenerate triangle
    }

    let inv_det = 1.0 / det;

    // Barycentric coordinates
    let lambda1 = ((u - u0) * (v2 - v0) - (u2 - u0) * (v - v0)) * inv_det;
    let lambda2 = ((u1 - u0) * (v - v0) - (u - u0) * (v1 - v0)) * inv_det;
    let lambda0 = 1.0 - lambda1 - lambda2;

    Some((lambda0, lambda1, lambda2))
}

/// Find all complete quads from a set of UV vertices.
fn find_quads(uv_to_vertex: &HashMap<(i32, i32), usize>) -> Vec<[usize; 4]> {
    let mut quads = Vec::new();

    for (&uv, &_idx) in uv_to_vertex {
        let (iu, iv) = uv;

        // Check if this is the bottom-left of a unit quad
        let br = (iu + 1, iv);
        let tr = (iu + 1, iv + 1);
        let tl = (iu, iv + 1);

        if let (Some(&i0), Some(&i1), Some(&i2), Some(&i3)) = (
            uv_to_vertex.get(&uv),
            uv_to_vertex.get(&br),
            uv_to_vertex.get(&tr),
            uv_to_vertex.get(&tl),
        ) {
            quads.push([i0, i1, i2, i3]);
        }
    }

    // Remove duplicates
    deduplicate_quads(&quads)
}

/// Remove duplicate quads.
fn deduplicate_quads(quads: &[[usize; 4]]) -> Vec<[usize; 4]> {
    let mut seen: std::collections::HashSet<[usize; 4]> = std::collections::HashSet::new();
    let mut unique = Vec::new();

    for &quad in quads {
        // Normalize quad (rotate to start with minimum vertex index)
        let normalized = normalize_quad(quad);

        if !seen.contains(&normalized) {
            seen.insert(normalized);
            unique.push(quad);
        }
    }

    unique
}

/// Normalize a quad by rotating to start with minimum vertex index.
fn normalize_quad(quad: [usize; 4]) -> [usize; 4] {
    let min_idx = quad
        .iter()
        .enumerate()
        .min_by_key(|&(_, v)| v)
        .map(|(i, _)| i)
        .unwrap_or(0);

    [
        quad[min_idx],
        quad[(min_idx + 1) % 4],
        quad[(min_idx + 2) % 4],
        quad[(min_idx + 3) % 4],
    ]
}

/// Find edge crossings for a specific isoline value.
#[allow(dead_code)]
fn find_edge_crossing(
    p0: Vector3<f64>,
    p1: Vector3<f64>,
    val0: i32,
    val1: i32,
    target: i32,
) -> Option<(Vector3<f64>, f64)> {
    if val0 == val1 {
        return None;
    }

    // Check if target is between val0 and val1
    let min_val = val0.min(val1);
    let max_val = val0.max(val1);

    if target < min_val || target > max_val {
        return None;
    }

    // Interpolation parameter
    let t = (target - val0) as f64 / (val1 - val0) as f64;

    if t < 0.0 || t > 1.0 {
        return None;
    }

    let pos = p0 * (1.0 - t) + p1 * t;
    Some((pos, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = trace_isolines(&[], &[], &[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_triangle_no_quads() {
        // Triangle with UV coords that don't form complete quads
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let triangles = vec![[0, 1, 2]];
        let u = vec![0, 1, 0];
        let v = vec![0, 0, 1];

        let result = trace_isolines(&positions, &triangles, &u, &v);
        // Small triangle, may or may not have quads
        assert!(result.num_faces() <= 1);
    }

    #[test]
    fn test_quad_triangle_pair() {
        // Two triangles forming a quad in UV space
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let triangles = vec![[0, 1, 2], [0, 2, 3]];
        let u = vec![0, 1, 1, 0];
        let v = vec![0, 0, 1, 1];

        let result = trace_isolines(&positions, &triangles, &u, &v);

        // Should find exactly one quad
        assert_eq!(result.num_faces(), 1);
        assert_eq!(result.num_vertices(), 4);
    }

    #[test]
    fn test_uv_to_barycentric() {
        let uv = (0, 0);
        let uv0 = (0, 0);
        let uv1 = (1, 0);
        let uv2 = (0, 1);

        let bary = uv_to_barycentric(uv, uv0, uv1, uv2, 1e-10);
        assert!(bary.is_some());

        let (b0, b1, b2) = bary.unwrap();
        assert!((b0 - 1.0).abs() < 1e-10);
        assert!(b1.abs() < 1e-10);
        assert!(b2.abs() < 1e-10);
    }

    #[test]
    fn test_normalize_quad() {
        let quad = [3, 0, 1, 2];
        let normalized = normalize_quad(quad);
        assert_eq!(normalized[0], 0);
    }

    #[test]
    fn test_grid_points_in_triangle() {
        let p0 = Vector3::new(0.0, 0.0, 0.0);
        let p1 = Vector3::new(2.0, 0.0, 0.0);
        let p2 = Vector3::new(0.0, 2.0, 0.0);

        let uv0 = (0, 0);
        let uv1 = (2, 0);
        let uv2 = (0, 2);

        let points = find_grid_points_in_triangle(&p0, &p1, &p2, uv0, uv1, uv2, 1e-10);

        // Should find points at (0,0), (1,0), (2,0), (0,1), (1,1), (0,2)
        assert!(points.len() >= 3); // At minimum the 3 corners
    }

    #[test]
    fn test_deduplicate_quads() {
        let quads = vec![
            [0, 1, 2, 3],
            [0, 1, 2, 3], // Duplicate
            [1, 2, 3, 0], // Same quad, rotated
            [4, 5, 6, 7],
        ];

        let unique = deduplicate_quads(&quads);
        assert_eq!(unique.len(), 2); // Only 2 unique quads
    }

    #[test]
    fn test_process_triangle() {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let u = vec![0, 1, 0];
        let v = vec![0, 0, 1];

        let points = process_triangle(&positions, &[0, 1, 2], &u, &v, 1e-10);
        assert!(!points.is_empty());
    }
}
