//! Configuration for quad remeshing.

/// Configuration for the QuadriFlow remeshing pipeline.
#[derive(Debug, Clone)]
pub struct RemeshConfig {
    /// Target number of quad faces in output mesh.
    /// The actual count may vary slightly.
    pub target_faces: usize,

    /// Whether to detect and preserve sharp edges.
    pub preserve_sharp: bool,

    /// Angle threshold (radians) for sharp edge detection.
    /// Edges with dihedral angle greater than this are considered sharp.
    pub sharp_angle: f64,

    /// Use adaptive resolution based on local curvature.
    pub adaptive: bool,

    /// Number of smoothing iterations for field optimization.
    pub smooth_iterations: usize,

    /// Whether to use SAT solver to remove T-junctions (slower but cleaner).
    pub remove_flips: bool,
}

impl Default for RemeshConfig {
    fn default() -> Self {
        Self {
            target_faces: 1000,
            preserve_sharp: false,
            sharp_angle: std::f64::consts::FRAC_PI_4, // 45 degrees
            adaptive: false,
            smooth_iterations: 10,
            remove_flips: false,
        }
    }
}

impl RemeshConfig {
    /// Create config for fast but lower quality remeshing.
    pub fn fast(target_faces: usize) -> Self {
        Self {
            target_faces,
            smooth_iterations: 5,
            ..Default::default()
        }
    }

    /// Create config for high quality remeshing.
    pub fn quality(target_faces: usize) -> Self {
        Self {
            target_faces,
            preserve_sharp: true,
            smooth_iterations: 20,
            remove_flips: true,
            ..Default::default()
        }
    }
}
