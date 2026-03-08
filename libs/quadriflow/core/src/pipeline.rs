//! Full remeshing pipeline with intermediate state access.

use crate::{
    align_field_to_features, estimate_target_edge_length, smooth_field_multiscale, QuadMesh,
    RemeshConfig, RemeshError, RemeshStage,
};
use pinocchio_mesh::Mesh;
use quadriflow_extractor::{extract_quads, ExtractionConfig};
use quadriflow_field::{OrientationField, PositionField, PositionFieldConfig};
use quadriflow_flow::{
    apply_singularity_optimization, optimize_singularities, SingularityOptConfig,
    SingularityOptResult,
};
use quadriflow_hierarchy::{build_hierarchy, HierarchyConfig, MeshHierarchy};
use quadriflow_optimizer::{smooth_orientation_field_simd, BoundaryInfo, SharpEdgeInfo};
use quadriflow_parametrizer::{
    compute_integer_parametrization, IntegerConfig, IntegerParametrization, SeamData,
};

/// Pipeline state for debugging/visualization.
#[derive(Debug)]
pub struct PipelineState {
    /// Mesh hierarchy for multi-scale optimization
    pub hierarchy: Option<MeshHierarchy>,
    /// Computed orientation field
    pub orientation: Option<OrientationField>,
    /// Computed position field
    pub position: Option<PositionField>,
    /// Seam transition data
    pub seam_data: Option<SeamData>,
    /// Integer parametrization
    pub parametrization: Option<IntegerParametrization>,
    /// Current stage
    pub current_stage: RemeshStage,
    /// Detected sharp edges (if preserve_sharp enabled)
    pub sharp_edges: Option<SharpEdgeInfo>,
    /// Detected boundary info (if preserve_sharp enabled)
    pub boundary_info: Option<BoundaryInfo>,
    /// Singularity optimization result
    pub singularity_result: Option<SingularityOptResult>,
}

impl PipelineState {
    pub fn new() -> Self {
        Self {
            hierarchy: None,
            orientation: None,
            position: None,
            seam_data: None,
            parametrization: None,
            current_stage: RemeshStage::BuildHierarchy,
            sharp_edges: None,
            boundary_info: None,
            singularity_result: None,
        }
    }
}

impl Default for PipelineState {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the full remeshing pipeline with intermediate state access.
///
/// This version stores all intermediate results for debugging/visualization.
pub fn run_pipeline(
    mesh: &Mesh,
    config: &RemeshConfig,
    state: &mut PipelineState,
) -> Result<QuadMesh, RemeshError> {
    run_pipeline_with_callback(mesh, config, state, |_, _| {})
}

/// Run pipeline with progress callback.
///
/// Identical logic to `remesh_with_callback` but stores all intermediate
/// results in `PipelineState` for debugging and visualization.
pub fn run_pipeline_with_callback<F>(
    mesh: &Mesh,
    config: &RemeshConfig,
    state: &mut PipelineState,
    mut on_progress: F,
) -> Result<QuadMesh, RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
    if mesh.vertices.is_empty() {
        return Err(RemeshError::EmptyMesh);
    }

    // Step 1: Build hierarchy
    state.current_stage = RemeshStage::BuildHierarchy;
    on_progress(RemeshStage::BuildHierarchy, "Building mesh hierarchy...");
    let hierarchy = if config.adaptive {
        build_hierarchy(mesh, &HierarchyConfig::default())
    } else {
        MeshHierarchy::from_mesh(mesh.clone())
    };
    state.hierarchy = Some(hierarchy.clone());

    // Step 2: Compute orientation field
    state.current_stage = RemeshStage::OrientationField;
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
    state.current_stage = RemeshStage::FieldSmoothing;
    on_progress(RemeshStage::FieldSmoothing, "Smoothing orientation field...");
    smooth_field_multiscale(&mut orientation, mesh, &hierarchy, config);

    // Step 4: Feature alignment
    state.current_stage = RemeshStage::FeatureAlignment;
    if config.preserve_sharp {
        on_progress(RemeshStage::FeatureAlignment, "Aligning field to features...");

        let sharp_config = quadriflow_optimizer::SharpEdgeConfig {
            angle_threshold: config.sharp_angle,
            include_boundary: true,
        };
        let sharp_info = quadriflow_optimizer::detect_sharp_edges(mesh, &sharp_config);
        let boundary = quadriflow_optimizer::detect_boundary(mesh);

        // Store for inspection
        state.sharp_edges = Some(sharp_info.clone());
        state.boundary_info = Some(boundary.clone());

        // Apply alignment using the shared helper
        align_field_to_features(&mut orientation, mesh, config);
    } else {
        on_progress(RemeshStage::FeatureAlignment, "Skipping feature alignment (preserve_sharp=false)");
    }

    // Step 5: Singularity optimization
    state.current_stage = RemeshStage::SingularityOptimization;
    on_progress(RemeshStage::SingularityOptimization, "Optimizing singularities...");
    let sing_result =
        optimize_singularities(mesh, &orientation, &SingularityOptConfig::default())?;
    apply_singularity_optimization(&mut orientation, mesh, &sing_result);
    smooth_orientation_field_simd(&mut orientation, mesh, config.smooth_iterations / 2);
    state.singularity_result = Some(sing_result);

    state.orientation = Some(orientation.clone());

    // Step 6: Compute position field
    state.current_stage = RemeshStage::PositionField;
    on_progress(RemeshStage::PositionField, "Computing position field...");
    let position_config = PositionFieldConfig {
        target_edge_length: estimate_target_edge_length(mesh, config.target_faces),
        solver_iterations: config.smooth_iterations * 50,
        ..Default::default()
    };
    let position = PositionField::from_orientation_field(mesh, &orientation, &position_config);
    state.position = Some(position.clone());

    // Step 7: Compute seam data
    state.current_stage = RemeshStage::SeamData;
    on_progress(RemeshStage::SeamData, "Computing seam transitions...");
    let seam_data = SeamData::from_orientation_field(mesh, &orientation);
    state.seam_data = Some(seam_data.clone());

    // Step 8: Integer parametrization
    state.current_stage = RemeshStage::Parametrization;
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
    state.parametrization = Some(param.clone());

    // Step 9: Extract quad mesh
    state.current_stage = RemeshStage::Extraction;
    on_progress(RemeshStage::Extraction, "Extracting quad mesh...");
    let extraction_config = ExtractionConfig::default();
    let quad_mesh = extract_quads(mesh, &param, &extraction_config)?;

    state.current_stage = RemeshStage::Done;
    on_progress(RemeshStage::Done, "Pipeline complete");

    Ok(quad_mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_simple_mesh() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3]];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_pipeline_runs() {
        let mesh = make_simple_mesh();
        let config = RemeshConfig::default();
        let mut state = PipelineState::new();

        let result = run_pipeline(&mesh, &config, &mut state);

        assert!(result.is_ok());
        assert!(state.orientation.is_some());
        assert!(state.position.is_some());
        assert!(state.seam_data.is_some());
        assert!(state.parametrization.is_some());
    }

    #[test]
    fn test_pipeline_state_stages() {
        let mesh = make_simple_mesh();
        let config = RemeshConfig::default();
        let mut state = PipelineState::new();

        let mut stages_seen = Vec::new();
        let _ = run_pipeline_with_callback(&mesh, &config, &mut state, |stage, _| {
            stages_seen.push(stage);
        });

        assert!(stages_seen.contains(&RemeshStage::BuildHierarchy));
        assert!(stages_seen.contains(&RemeshStage::OrientationField));
        assert!(stages_seen.contains(&RemeshStage::FieldSmoothing));
        assert!(stages_seen.contains(&RemeshStage::FeatureAlignment));
        assert!(stages_seen.contains(&RemeshStage::SingularityOptimization));
        assert!(stages_seen.contains(&RemeshStage::PositionField));
        assert!(stages_seen.contains(&RemeshStage::Parametrization));
        assert!(stages_seen.contains(&RemeshStage::Extraction));
        assert!(stages_seen.contains(&RemeshStage::Done));
    }
}
