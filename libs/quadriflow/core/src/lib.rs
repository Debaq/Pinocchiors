//! # quadriflow-core
//!
//! QuadriFlow: A Scalable and Robust Method for Quadrangulation
//!
//! This is a Rust port of the QuadriFlow algorithm by Huang et al. (SGP 2018).
//!
//! ## Usage
//!
//! ```ignore
//! use quadriflow_core::{remesh, RemeshConfig};
//! use pinocchio_mesh::Mesh;
//!
//! let input_mesh = Mesh::from_obj("model.obj")?;
//! let config = RemeshConfig {
//!     target_faces: 5000,
//!     ..Default::default()
//! };
//! let quad_mesh = remesh(&input_mesh, &config)?;
//! ```
//!
//! ## Algorithm Overview
//!
//! 1. **Field Computation**: Compute orientation and position fields
//! 2. **Optimization**: Smooth fields and align to features
//! 3. **Flow Solve**: Minimize singularities via min-cost flow
//! 4. **Parametrization**: Convert to integer grid coordinates
//! 5. **Extraction**: Trace isolines to build quad mesh

pub mod config;
pub mod pipeline;

pub use config::RemeshConfig;
pub use quadriflow_extractor::QuadMesh;

use pinocchio_mesh::Mesh;
use quadriflow_extractor::{extract_quads, ExtractionConfig};
use quadriflow_field::{OrientationField, PositionField, PositionFieldConfig};
use quadriflow_flow::{
    apply_singularity_optimization, optimize_singularities, SingularityOptConfig,
};
use quadriflow_hierarchy::{
    build_hierarchy, propagate_field_to_coarser, propagate_field_to_finer, HierarchyConfig,
    MeshHierarchy,
};
use quadriflow_optimizer::{
    align_to_boundary, align_to_features, detect_boundary, detect_sharp_edges,
    smooth_orientation_field_simd, SharpEdgeConfig,
};
use quadriflow_parametrizer::{
    compute_integer_parametrization, IntegerConfig, SeamData,
};
use thiserror::Error;

/// Errors during quad remeshing.
#[derive(Error, Debug)]
pub enum RemeshError {
    #[error("Input mesh is empty")]
    EmptyMesh,
    #[error("Input mesh is non-manifold")]
    NonManifold,
    #[error("Field computation failed: {0}")]
    FieldError(String),
    #[error("Flow solver failed: {0}")]
    FlowError(#[from] quadriflow_flow::FlowError),
    #[error("Parametrization failed: {0}")]
    ParametrizationError(#[from] quadriflow_parametrizer::ParametrizationError),
    #[error("Mesh extraction failed: {0}")]
    ExtractionError(#[from] quadriflow_extractor::ExtractionError),
}

/// Stage of the remeshing pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemeshStage {
    /// Building mesh hierarchy for multi-scale optimization
    BuildHierarchy,
    /// Computing orientation field
    OrientationField,
    /// Smoothing orientation field (multi-scale + SIMD)
    FieldSmoothing,
    /// Aligning field to sharp edges and boundaries
    FeatureAlignment,
    /// Optimizing singularities via min-cost flow
    SingularityOptimization,
    /// Computing position field
    PositionField,
    /// Computing seam transitions
    SeamData,
    /// Integer parametrization
    Parametrization,
    /// Extracting quad mesh
    Extraction,
    /// Done
    Done,
}

impl RemeshStage {
    /// Get stage name for display.
    pub fn name(&self) -> &'static str {
        match self {
            RemeshStage::BuildHierarchy => "build_hierarchy",
            RemeshStage::OrientationField => "orientation_field",
            RemeshStage::FieldSmoothing => "field_smoothing",
            RemeshStage::FeatureAlignment => "feature_alignment",
            RemeshStage::SingularityOptimization => "singularity_optimization",
            RemeshStage::PositionField => "position_field",
            RemeshStage::SeamData => "seam_data",
            RemeshStage::Parametrization => "parametrization",
            RemeshStage::Extraction => "extraction",
            RemeshStage::Done => "done",
        }
    }

    /// Get approximate progress percentage for this stage.
    pub fn progress(&self) -> u32 {
        match self {
            RemeshStage::BuildHierarchy => 2,
            RemeshStage::OrientationField => 8,
            RemeshStage::FieldSmoothing => 18,
            RemeshStage::FeatureAlignment => 25,
            RemeshStage::SingularityOptimization => 35,
            RemeshStage::PositionField => 45,
            RemeshStage::SeamData => 55,
            RemeshStage::Parametrization => 70,
            RemeshStage::Extraction => 85,
            RemeshStage::Done => 100,
        }
    }
}

/// Remesh a triangle mesh into a quad-dominant mesh.
///
/// This is the main entry point for QuadriFlow.
///
/// # Arguments
///
/// * `mesh` - Input triangle mesh
/// * `config` - Remeshing configuration
///
/// # Returns
///
/// A quad mesh with approximately `config.target_faces` faces.
pub fn remesh(mesh: &Mesh, config: &RemeshConfig) -> Result<QuadMesh, RemeshError> {
    remesh_with_callback(mesh, config, |_, _| {})
}

/// Remesh with progress callback.
///
/// The callback receives (stage, message) for each pipeline step.
///
/// The pipeline follows the QuadriFlow paper:
/// 1. Build hierarchy (if adaptive)
/// 2. Compute orientation field
/// 3. Smooth field (multi-scale if adaptive, then SIMD)
/// 4. Align to features (if preserve_sharp)
/// 5. Optimize singularities via min-cost flow
/// 6. Compute position field
/// 7. Compute seam transitions
/// 8. Integer parametrization
/// 9. Extract quad mesh
pub fn remesh_with_callback<F>(
    mesh: &Mesh,
    config: &RemeshConfig,
    mut on_progress: F,
) -> Result<QuadMesh, RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
    if mesh.vertices.is_empty() {
        return Err(RemeshError::EmptyMesh);
    }

    // Step 1: Build hierarchy
    on_progress(RemeshStage::BuildHierarchy, "Building mesh hierarchy...");
    let hierarchy = if config.adaptive {
        build_hierarchy(mesh, &HierarchyConfig::default())
    } else {
        MeshHierarchy::from_mesh(mesh.clone())
    };

    // Step 2: Compute orientation field
    on_progress(RemeshStage::OrientationField, "Computing orientation field...");
    let mut orientation = if config.preserve_sharp {
        OrientationField::from_mesh_edge_aligned(mesh)
    } else {
        OrientationField::from_mesh(mesh)
    };

    if orientation.is_empty() {
        return Err(RemeshError::FieldError(
            "Failed to compute orientation field".to_string(),
        ));
    }

    // Step 3: Field smoothing (multi-scale + SIMD)
    on_progress(RemeshStage::FieldSmoothing, "Smoothing orientation field...");
    smooth_field_multiscale(&mut orientation, mesh, &hierarchy, config);

    // Step 4: Feature alignment
    if config.preserve_sharp {
        on_progress(RemeshStage::FeatureAlignment, "Aligning field to features...");
        align_field_to_features(&mut orientation, mesh, config);
    } else {
        on_progress(RemeshStage::FeatureAlignment, "Skipping feature alignment (preserve_sharp=false)");
    }

    // Step 5: Singularity optimization
    on_progress(RemeshStage::SingularityOptimization, "Optimizing singularities...");
    let sing_result = optimize_singularities(mesh, &orientation, &SingularityOptConfig::default())?;
    apply_singularity_optimization(&mut orientation, mesh, &sing_result);
    // Light re-smooth to clean flow artifacts
    smooth_orientation_field_simd(&mut orientation, mesh, config.smooth_iterations / 2);

    // Step 6: Compute position field
    on_progress(RemeshStage::PositionField, "Computing position field...");
    let position_config = PositionFieldConfig {
        target_edge_length: estimate_target_edge_length(mesh, config.target_faces),
        solver_iterations: config.smooth_iterations * 50,
        solver_tolerance: 1e-6,
        alignment_weight: 1.0,
    };
    let position = PositionField::from_orientation_field(mesh, &orientation, &position_config);

    if position.is_empty() {
        return Err(RemeshError::FieldError(
            "Failed to compute position field".to_string(),
        ));
    }

    // Step 7: Compute seam transitions
    on_progress(RemeshStage::SeamData, "Computing seam transitions...");
    let seam_data = SeamData::from_orientation_field(mesh, &orientation);

    // Step 8: Integer parametrization
    on_progress(RemeshStage::Parametrization, "Computing integer parametrization...");
    let integer_config = IntegerConfig {
        optimization_iterations: config.smooth_iterations,
        use_greedy: true,
        distortion_weight: 1.0,
        remove_tjunctions: config.remove_flips,
        ..Default::default()
    };

    let (param, _stats) =
        compute_integer_parametrization(mesh, &position, &seam_data, &integer_config)?;

    // Step 9: Extract quad mesh
    on_progress(RemeshStage::Extraction, "Extracting quad mesh...");
    let extraction_config = ExtractionConfig::default();
    let quad_mesh = extract_quads(mesh, &param, &extraction_config)?;

    on_progress(
        RemeshStage::Done,
        &format!(
            "Done: {} vertices, {} quads (singularities: {} -> {})",
            quad_mesh.num_vertices(),
            quad_mesh.num_faces(),
            sing_result.initial_singularities,
            sing_result.final_singularities,
        ),
    );

    Ok(quad_mesh)
}

/// Multi-scale field smoothing: propagate to coarser levels, smooth there, propagate back.
pub(crate) fn smooth_field_multiscale(
    orientation: &mut OrientationField,
    mesh: &Mesh,
    hierarchy: &MeshHierarchy,
    config: &RemeshConfig,
) {
    if config.adaptive && hierarchy.depth() > 1 {
        // Propagate to coarsest level
        let mut current_dirs = orientation.directions.clone();
        let mut current_normals = orientation.normals.clone();

        for level in hierarchy.iter_fine_to_coarse().skip(1) {
            let (dirs, normals) =
                propagate_field_to_coarser(&current_dirs, &current_normals, level);
            current_dirs = dirs;
            current_normals = normals;
        }

        // Smooth at coarsest level (more iterations since fewer faces)
        if let Some(coarsest_mesh) = hierarchy.mesh_at(hierarchy.depth() - 1) {
            let mut coarse_field = OrientationField {
                directions: current_dirs,
                normals: current_normals,
            };
            smooth_orientation_field_simd(
                &mut coarse_field,
                coarsest_mesh,
                config.smooth_iterations * 2,
            );
            current_dirs = coarse_field.directions;
            current_normals = coarse_field.normals;
        }

        // Propagate back to finest level
        for level in hierarchy.iter_coarse_to_fine().skip(1) {
            let (dirs, normals) =
                propagate_field_to_finer(&current_dirs, &current_normals, level);
            current_dirs = dirs;
            current_normals = normals;
        }

        orientation.directions = current_dirs;
        orientation.normals = current_normals;
    }

    // Always do SIMD smoothing at the finest level
    smooth_orientation_field_simd(orientation, mesh, config.smooth_iterations);
}

/// Align orientation field to sharp edges and mesh boundaries.
pub(crate) fn align_field_to_features(
    orientation: &mut OrientationField,
    mesh: &Mesh,
    config: &RemeshConfig,
) {
    let sharp_config = SharpEdgeConfig {
        angle_threshold: config.sharp_angle,
        include_boundary: true,
    };
    let sharp_info = detect_sharp_edges(mesh, &sharp_config);

    if !sharp_info.edges.is_empty() {
        align_to_features(orientation, mesh, &sharp_info.edges, 0.7);
    }

    let boundary = detect_boundary(mesh);
    if !boundary.is_closed {
        align_to_boundary(orientation, mesh, &boundary, 0.8);
    }

    // Light re-smooth to blend alignment
    let blend_iterations = config.smooth_iterations / 4;
    if blend_iterations > 0 {
        smooth_orientation_field_simd(orientation, mesh, blend_iterations);
    }
}

/// Estimate target edge length based on mesh size and desired face count.
pub(crate) fn estimate_target_edge_length(mesh: &Mesh, target_faces: usize) -> f64 {
    // Compute mesh bounding box diagonal
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];

    for vertex in &mesh.vertices {
        let pos = vertex.position;
        min[0] = min[0].min(pos.x());
        min[1] = min[1].min(pos.y());
        min[2] = min[2].min(pos.z());
        max[0] = max[0].max(pos.x());
        max[1] = max[1].max(pos.y());
        max[2] = max[2].max(pos.z());
    }

    let diagonal = ((max[0] - min[0]).powi(2)
        + (max[1] - min[1]).powi(2)
        + (max[2] - min[2]).powi(2))
    .sqrt();

    // Approximate: surface area ≈ diagonal² / 6, each quad has area ≈ edge_length²
    // So edge_length ≈ diagonal / sqrt(target_faces * 6)
    let estimated = diagonal / (target_faces as f64 * 6.0).sqrt();

    // Clamp to reasonable range
    estimated.clamp(diagonal * 0.001, diagonal * 0.5)
}
