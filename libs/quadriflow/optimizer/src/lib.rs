//! # quadriflow-optimizer
//!
//! Field optimization using smoothness and alignment objectives.
//!
//! The optimizer refines orientation and position fields to:
//! - Maximize smoothness (minimize field variation)
//! - Align to sharp features
//! - Minimize singularities via integer constraints
//!
//! ## Modules
//!
//! - **smooth**: Field smoothing with parallel processing
//! - **align**: Feature alignment
//! - **pool**: Buffer pooling for reduced allocations
//! - **simd**: SIMD-optimized batch operations

pub mod align;
pub mod pool;
pub mod simd;
pub mod smooth;

pub use align::{
    align_to_boundary, align_to_features, compute_curvature_sizing, detect_boundary,
    detect_sharp_edges, BoundaryInfo, SharpEdge, SharpEdgeConfig, SharpEdgeInfo,
};
pub use pool::{with_f64_buffer, with_usize_buffer, ScratchSpace, VecPool};
pub use simd::{
    batch_align_4rosy, batch_cross, batch_dot, batch_normalize, batch_rotate_around_axis,
    batch_transport_direction, smooth_faces_batch, VectorBatch, BATCH_SIZE,
};
pub use smooth::{
    smooth_orientation_field, smooth_orientation_field_simd, smoothness_energy,
    smoothness_energy_simd,
};

/// Configuration for field optimization.
#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    /// Number of smoothing iterations
    pub iterations: usize,
    /// Smoothness weight (higher = smoother field)
    pub smoothness: f64,
    /// Feature alignment weight
    pub alignment: f64,
    /// Whether to detect and preserve sharp edges
    pub preserve_sharp: bool,
    /// Angle threshold for sharp edge detection (radians)
    pub sharp_angle: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            iterations: 10,
            smoothness: 1.0,
            alignment: 0.5,
            preserve_sharp: false,
            sharp_angle: std::f64::consts::FRAC_PI_4, // 45 degrees
        }
    }
}
