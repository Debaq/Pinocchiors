//! SIMD-optimized operations for field processing.
//!
//! Provides batch operations that process multiple vectors simultaneously
//! for improved cache utilization and SIMD autovectorization.
//!
//! The operations use SoA (Structure of Arrays) layout internally
//! for better memory access patterns.

use nalgebra::Vector3;
use std::f64::consts::FRAC_PI_2;

/// Batch size for SIMD operations.
/// 4 is optimal for most SIMD implementations (256-bit vectors with f64).
pub const BATCH_SIZE: usize = 4;

/// Structure-of-Arrays representation for batch of 3D vectors.
/// This layout enables better SIMD autovectorization.
#[derive(Debug, Clone)]
pub struct VectorBatch {
    pub x: [f64; BATCH_SIZE],
    pub y: [f64; BATCH_SIZE],
    pub z: [f64; BATCH_SIZE],
    pub count: usize, // Actual number of valid vectors (may be < BATCH_SIZE)
}

impl VectorBatch {
    /// Create a new empty batch.
    #[inline]
    pub fn new() -> Self {
        Self {
            x: [0.0; BATCH_SIZE],
            y: [0.0; BATCH_SIZE],
            z: [0.0; BATCH_SIZE],
            count: 0,
        }
    }

    /// Create batch from slice of vectors.
    #[inline]
    pub fn from_vectors(vectors: &[Vector3<f64>]) -> Self {
        let count = vectors.len().min(BATCH_SIZE);
        let mut batch = Self::new();
        batch.count = count;

        for (i, v) in vectors.iter().take(count).enumerate() {
            batch.x[i] = v.x;
            batch.y[i] = v.y;
            batch.z[i] = v.z;
        }

        batch
    }

    /// Extract vectors back to AoS format.
    #[inline]
    pub fn to_vectors(&self) -> Vec<Vector3<f64>> {
        (0..self.count)
            .map(|i| Vector3::new(self.x[i], self.y[i], self.z[i]))
            .collect()
    }

    /// Get single vector at index.
    #[inline]
    pub fn get(&self, index: usize) -> Vector3<f64> {
        debug_assert!(index < self.count);
        Vector3::new(self.x[index], self.y[index], self.z[index])
    }

    /// Set single vector at index.
    #[inline]
    pub fn set(&mut self, index: usize, v: &Vector3<f64>) {
        debug_assert!(index < BATCH_SIZE);
        self.x[index] = v.x;
        self.y[index] = v.y;
        self.z[index] = v.z;
    }
}

impl Default for VectorBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch dot product: computes dot products of corresponding vectors.
#[inline]
pub fn batch_dot(a: &VectorBatch, b: &VectorBatch) -> [f64; BATCH_SIZE] {
    let mut result = [0.0; BATCH_SIZE];

    // This loop structure enables SIMD autovectorization
    for i in 0..BATCH_SIZE {
        result[i] = a.x[i] * b.x[i] + a.y[i] * b.y[i] + a.z[i] * b.z[i];
    }

    result
}

/// Batch cross product: computes cross products of corresponding vectors.
#[inline]
pub fn batch_cross(a: &VectorBatch, b: &VectorBatch) -> VectorBatch {
    let mut result = VectorBatch::new();
    result.count = a.count.min(b.count);

    for i in 0..BATCH_SIZE {
        result.x[i] = a.y[i] * b.z[i] - a.z[i] * b.y[i];
        result.y[i] = a.z[i] * b.x[i] - a.x[i] * b.z[i];
        result.z[i] = a.x[i] * b.y[i] - a.y[i] * b.x[i];
    }

    result
}

/// Batch normalize: normalizes all vectors in batch.
#[inline]
pub fn batch_normalize(batch: &mut VectorBatch) {
    for i in 0..BATCH_SIZE {
        let len_sq = batch.x[i] * batch.x[i] + batch.y[i] * batch.y[i] + batch.z[i] * batch.z[i];
        let inv_len = if len_sq > 1e-20 {
            1.0 / len_sq.sqrt()
        } else {
            0.0
        };
        batch.x[i] *= inv_len;
        batch.y[i] *= inv_len;
        batch.z[i] *= inv_len;
    }
}

/// Batch scale: multiplies all components by scalars.
#[inline]
pub fn batch_scale(batch: &mut VectorBatch, scalars: &[f64; BATCH_SIZE]) {
    for i in 0..BATCH_SIZE {
        batch.x[i] *= scalars[i];
        batch.y[i] *= scalars[i];
        batch.z[i] *= scalars[i];
    }
}

/// Batch add: adds corresponding vectors.
#[inline]
pub fn batch_add(a: &VectorBatch, b: &VectorBatch) -> VectorBatch {
    let mut result = VectorBatch::new();
    result.count = a.count.min(b.count);

    for i in 0..BATCH_SIZE {
        result.x[i] = a.x[i] + b.x[i];
        result.y[i] = a.y[i] + b.y[i];
        result.z[i] = a.z[i] + b.z[i];
    }

    result
}

/// Batch subtract: subtracts corresponding vectors.
#[inline]
pub fn batch_sub(a: &VectorBatch, b: &VectorBatch) -> VectorBatch {
    let mut result = VectorBatch::new();
    result.count = a.count.min(b.count);

    for i in 0..BATCH_SIZE {
        result.x[i] = a.x[i] - b.x[i];
        result.y[i] = a.y[i] - b.y[i];
        result.z[i] = a.z[i] - b.z[i];
    }

    result
}

/// Batch Rodrigues rotation around axes.
///
/// Rotates vectors around corresponding axes by angles.
/// Uses the Rodrigues rotation formula:
/// v' = v*cos(θ) + (axis × v)*sin(θ) + axis*(axis·v)*(1-cos(θ))
#[inline]
pub fn batch_rotate_around_axis(
    vectors: &VectorBatch,
    axes: &VectorBatch,
    angles: &[f64; BATCH_SIZE],
) -> VectorBatch {
    let mut cos_a = [0.0; BATCH_SIZE];
    let mut sin_a = [0.0; BATCH_SIZE];

    // Precompute trig functions
    for i in 0..BATCH_SIZE {
        cos_a[i] = angles[i].cos();
        sin_a[i] = angles[i].sin();
    }

    // axis × v
    let cross = batch_cross(axes, vectors);

    // axis · v
    let dots = batch_dot(axes, vectors);

    // v * cos(θ)
    let mut term1 = vectors.clone();
    batch_scale(&mut term1, &cos_a);

    // (axis × v) * sin(θ)
    let mut term2 = cross;
    batch_scale(&mut term2, &sin_a);

    // axis * (axis · v) * (1 - cos(θ))
    let mut term3 = axes.clone();
    let mut scale3 = [0.0; BATCH_SIZE];
    for i in 0..BATCH_SIZE {
        scale3[i] = dots[i] * (1.0 - cos_a[i]);
    }
    batch_scale(&mut term3, &scale3);

    // Sum all terms
    let sum12 = batch_add(&term1, &term2);
    batch_add(&sum12, &term3)
}

/// Batch 4-RoSy alignment.
///
/// For each vector, finds which of the 4 symmetric rotations (0°, 90°, 180°, 270°)
/// around the normal is closest to the reference direction.
pub fn batch_align_4rosy(
    targets: &VectorBatch,
    references: &VectorBatch,
    normals: &VectorBatch,
) -> VectorBatch {
    let count = targets.count.min(references.count).min(normals.count);
    let mut result = VectorBatch::new();
    result.count = count;

    for i in 0..count {
        let target = Vector3::new(targets.x[i], targets.y[i], targets.z[i]);
        let reference = Vector3::new(references.x[i], references.y[i], references.z[i]);
        let normal = Vector3::new(normals.x[i], normals.y[i], normals.z[i]);

        let aligned = align_single_4rosy(&target, &reference, &normal);
        result.x[i] = aligned.x;
        result.y[i] = aligned.y;
        result.z[i] = aligned.z;
    }

    result
}

/// Single vector 4-RoSy alignment (helper for batch operation).
#[inline]
fn align_single_4rosy(
    target: &Vector3<f64>,
    reference: &Vector3<f64>,
    normal: &Vector3<f64>,
) -> Vector3<f64> {
    let mut best_dir = *target;
    let mut best_dot = target.dot(reference);

    // 90° rotation
    let rot90 = rotate_single_around_axis(target, normal, FRAC_PI_2);
    let dot90 = rot90.dot(reference);
    if dot90 > best_dot {
        best_dir = rot90;
        best_dot = dot90;
    }

    // 180° rotation (just negate)
    let rot180 = -target;
    let dot180 = rot180.dot(reference);
    if dot180 > best_dot {
        best_dir = rot180;
        best_dot = dot180;
    }

    // 270° rotation
    let rot270 = rotate_single_around_axis(target, normal, 3.0 * FRAC_PI_2);
    let dot270 = rot270.dot(reference);
    if dot270 > best_dot {
        best_dir = rot270;
    }

    best_dir
}

/// Single vector Rodrigues rotation (helper).
#[inline]
fn rotate_single_around_axis(v: &Vector3<f64>, axis: &Vector3<f64>, angle: f64) -> Vector3<f64> {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

/// Batch parallel transport of directions between tangent planes.
///
/// Transports vectors from source tangent planes (defined by from_normals)
/// to target tangent planes (defined by to_normals).
pub fn batch_transport_direction(
    dirs: &VectorBatch,
    from_normals: &VectorBatch,
    to_normals: &VectorBatch,
) -> VectorBatch {
    let count = dirs.count.min(from_normals.count).min(to_normals.count);
    let mut result = VectorBatch::new();
    result.count = count;

    for i in 0..count {
        let dir = Vector3::new(dirs.x[i], dirs.y[i], dirs.z[i]);
        let from_n = Vector3::new(from_normals.x[i], from_normals.y[i], from_normals.z[i]);
        let to_n = Vector3::new(to_normals.x[i], to_normals.y[i], to_normals.z[i]);

        let transported = transport_single_direction(&dir, &from_n, &to_n);
        result.x[i] = transported.x;
        result.y[i] = transported.y;
        result.z[i] = transported.z;
    }

    result
}

/// Single direction transport (helper).
#[inline]
fn transport_single_direction(
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

    rotate_single_around_axis(dir, &axis, angle)
}

/// Process multiple faces in batch for smoothing.
///
/// This is the main entry point for SIMD-accelerated face processing.
/// Returns smoothed directions for the batch of faces.
pub fn smooth_faces_batch(
    directions: &[Vector3<f64>],
    normals: &[Vector3<f64>],
    face_indices: &[usize],
    neighbor_indices: &[Vec<usize>],
) -> Vec<Vector3<f64>> {
    let num_faces = face_indices.len();
    let mut results = Vec::with_capacity(num_faces);

    // Process in batches of BATCH_SIZE
    for chunk_start in (0..num_faces).step_by(BATCH_SIZE) {
        let chunk_end = (chunk_start + BATCH_SIZE).min(num_faces);
        let chunk_size = chunk_end - chunk_start;

        // Build batches for this chunk
        let mut current_dirs = VectorBatch::new();
        let mut current_normals = VectorBatch::new();
        current_dirs.count = chunk_size;
        current_normals.count = chunk_size;

        for (i, &face_idx) in face_indices[chunk_start..chunk_end].iter().enumerate() {
            current_dirs.set(i, &directions[face_idx]);
            current_normals.set(i, &normals[face_idx]);
        }

        // Accumulate neighbor contributions
        let mut sums = current_dirs.clone();

        for (i, &face_idx) in face_indices[chunk_start..chunk_end].iter().enumerate() {
            let neighbors = &neighbor_indices[face_idx];

            for &neighbor_idx in neighbors {
                let neighbor_dir = directions[neighbor_idx];
                let neighbor_normal = normals[neighbor_idx];
                let current_normal = normals[face_idx];
                let current_dir = directions[face_idx];

                // Transport and align
                let transported = transport_single_direction(&neighbor_dir, &neighbor_normal, &current_normal);
                let aligned = align_single_4rosy(&transported, &current_dir, &current_normal);

                sums.x[i] += aligned.x;
                sums.y[i] += aligned.y;
                sums.z[i] += aligned.z;
            }
        }

        // Project to tangent planes and normalize
        for i in 0..chunk_size {
            let sum = Vector3::new(sums.x[i], sums.y[i], sums.z[i]);
            let normal = Vector3::new(current_normals.x[i], current_normals.y[i], current_normals.z[i]);

            // Project to tangent plane
            let projected = sum - normal * sum.dot(&normal);
            let len = projected.norm();

            let result = if len > 1e-10 {
                projected / len
            } else {
                // Fallback: keep original direction
                current_dirs.get(i)
            };

            results.push(result);
        }
    }

    results
}

/// Compute smoothness energy in batch.
///
/// More efficient than computing per-face sequentially.
pub fn compute_energy_batch(
    directions: &[Vector3<f64>],
    normals: &[Vector3<f64>],
    adjacency: &[Vec<usize>],
) -> f64 {
    let mut energy = 0.0;

    for (face_idx, neighbors) in adjacency.iter().enumerate() {
        let dir = directions[face_idx];
        let normal = normals[face_idx];

        for &neighbor_idx in neighbors {
            let neighbor_dir = directions[neighbor_idx];
            let neighbor_normal = normals[neighbor_idx];

            let transported = transport_single_direction(&neighbor_dir, &neighbor_normal, &normal);
            let aligned = align_single_4rosy(&transported, &dir, &normal);

            let dot = dir.dot(&aligned).clamp(-1.0, 1.0);
            energy += 1.0 - dot;
        }
    }

    energy / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_vector_batch_roundtrip() {
        let vectors = vec![
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        ];

        let batch = VectorBatch::from_vectors(&vectors);
        let result = batch.to_vectors();

        assert_eq!(result.len(), vectors.len());
        for (a, b) in result.iter().zip(vectors.iter()) {
            assert_relative_eq!(a.x, b.x, epsilon = 1e-10);
            assert_relative_eq!(a.y, b.y, epsilon = 1e-10);
            assert_relative_eq!(a.z, b.z, epsilon = 1e-10);
        }
    }

    #[test]
    fn test_batch_dot() {
        let a = VectorBatch::from_vectors(&[
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ]);
        let b = VectorBatch::from_vectors(&[
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ]);

        let dots = batch_dot(&a, &b);
        assert_relative_eq!(dots[0], 1.0, epsilon = 1e-10);
        assert_relative_eq!(dots[1], 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_batch_cross() {
        let a = VectorBatch::from_vectors(&[
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ]);
        let b = VectorBatch::from_vectors(&[
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        ]);

        let cross = batch_cross(&a, &b);
        assert_relative_eq!(cross.z[0], 1.0, epsilon = 1e-10);
        assert_relative_eq!(cross.x[1], 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_batch_normalize() {
        let mut batch = VectorBatch::from_vectors(&[
            Vector3::new(3.0, 0.0, 0.0),
            Vector3::new(0.0, 4.0, 0.0),
        ]);
        batch.count = 2;

        batch_normalize(&mut batch);

        assert_relative_eq!(batch.x[0], 1.0, epsilon = 1e-10);
        assert_relative_eq!(batch.y[1], 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_batch_rotate() {
        let vectors = VectorBatch::from_vectors(&[
            Vector3::new(1.0, 0.0, 0.0),
        ]);
        let axes = VectorBatch::from_vectors(&[
            Vector3::new(0.0, 0.0, 1.0),
        ]);
        let angles = [FRAC_PI_2, 0.0, 0.0, 0.0];

        let rotated = batch_rotate_around_axis(&vectors, &axes, &angles);

        // 90° rotation around Z should turn (1,0,0) into (0,1,0)
        assert_relative_eq!(rotated.x[0], 0.0, epsilon = 1e-10);
        assert_relative_eq!(rotated.y[0], 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_align_single_4rosy() {
        let target = Vector3::new(1.0, 0.0, 0.0);
        let reference = Vector3::new(-1.0, 0.0, 0.0);
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let aligned = align_single_4rosy(&target, &reference, &normal);

        // Should align to 180° rotation
        assert_relative_eq!(aligned.dot(&reference), 1.0, epsilon = 1e-10);
    }
}
