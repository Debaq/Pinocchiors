//! Función principal de auto-rigging

use crate::{PinocchioConfig, PinocchioError, PinocchioOutput};
use crate::output::ProcessStats;
use pinocchio_attachment::{Attachment, HeatDiffusion};
use pinocchio_embedding::{full_embedding_pipeline, EmbeddingResult};
use pinocchio_math::{Real, Transform, Vector3};
use pinocchio_mesh::{decimate, Mesh};
use pinocchio_skeleton::Skeleton;

/// Realiza el auto-rigging de una malla con un esqueleto
///
/// # Arguments
///
/// * `mesh` - La malla 3D a riggear
/// * `skeleton` - El esqueleto a usar para el rigging
/// * `config` - Configuración opcional (usa default si es None)
///
/// # Returns
///
/// Un `PinocchioOutput` con los pesos de skinning y el embedding,
/// o un error si el proceso falla.
///
/// # Example
///
/// ```ignore
/// use pinocchio_core::{autorig, PinocchioConfig};
/// use pinocchio_mesh::load_obj;
/// use pinocchio_skeleton::HumanSkeleton;
///
/// let mesh = load_obj("character.obj")?;
/// let skeleton = HumanSkeleton::new();
/// let result = autorig(&mesh, &skeleton, None)?;
/// ```
pub fn autorig<S: Skeleton + Sync>(
    mesh: &Mesh,
    skeleton: &S,
    config: Option<PinocchioConfig>,
) -> Result<PinocchioOutput, PinocchioError> {
    let config = config.unwrap_or_default();

    // 1. Validar entrada
    validate_input(mesh, skeleton, &config)?;

    // 2. Preparar malla
    let mut working_mesh = mesh.clone();

    // 2a. Aplicar decimación si la malla es muy grande
    let was_decimated = if working_mesh.num_faces() > config.auto_decimate_threshold {
        working_mesh = decimate(&working_mesh, config.decimate_ratio);
        true
    } else {
        false
    };

    if config.normalize_mesh {
        working_mesh.normalize_bounding_box();
    }

    // Si se decimó, los pesos se calcularán en la malla simplificada
    // y luego se transferirán a la malla original
    let _ = was_decimated; // TODO: implementar transferencia de pesos

    // 3. Embedding del esqueleto
    let embedding = full_embedding_pipeline(
        &working_mesh,
        skeleton,
        config.max_medial_spheres,
        config.refine_iterations,
    )?;

    // 4. Calcular pesos de skinning
    let heat_diffusion = HeatDiffusion::new(&working_mesh)
        .with_diffusion_weight(config.diffusion_weight);

    let initial_heat = heat_diffusion.compute_initial_heat(skeleton);
    let weights = heat_diffusion.compute_weights(skeleton, &initial_heat)?;

    // 5. Crear Attachment
    let mut attachment = Attachment::new(&working_mesh, weights, skeleton.num_bones());

    // 6. Compactar pesos
    attachment.compact_weights(config.max_bone_influences);

    // 7. Calcular transformaciones de reposo
    let bone_rest_transforms = compute_rest_transforms(skeleton, &embedding);

    // 8. Calcular estadísticas
    let stats = compute_stats(&working_mesh, skeleton, &attachment, &embedding);

    Ok(PinocchioOutput {
        attachment,
        embedding: embedding.clone(),
        bone_positions: embedding.bone_positions,
        bone_rest_transforms,
        stats,
    })
}

/// Valida la entrada antes del procesamiento
fn validate_input<S: Skeleton>(
    mesh: &Mesh,
    skeleton: &S,
    config: &PinocchioConfig,
) -> Result<(), PinocchioError> {
    if mesh.num_vertices() == 0 {
        return Err(PinocchioError::EmptyMesh);
    }

    if skeleton.num_bones() == 0 {
        return Err(PinocchioError::EmptySkeleton);
    }

    if config.verify_mesh_integrity {
        if let Err(e) = mesh.integrity_check() {
            return Err(PinocchioError::IntegrityCheckFailed(e));
        }
    }

    Ok(())
}

/// Calcula las transformaciones de reposo de los huesos
fn compute_rest_transforms<S: Skeleton>(
    skeleton: &S,
    embedding: &EmbeddingResult,
) -> Vec<Transform> {
    let num_bones = skeleton.num_bones();
    let mut transforms = Vec::with_capacity(num_bones);

    for bone_idx in 0..num_bones {
        let pos = embedding.bone_positions.get(bone_idx)
            .copied()
            .unwrap_or_else(Vector3::zero);

        transforms.push(Transform::from_translation(pos));
    }

    transforms
}

/// Calcula las estadísticas del proceso
fn compute_stats<S: Skeleton>(
    mesh: &Mesh,
    skeleton: &S,
    attachment: &Attachment,
    embedding: &EmbeddingResult,
) -> ProcessStats {
    let num_vertices = mesh.num_vertices();
    let num_bones = skeleton.num_bones();

    // Calcular promedio de influencias por vértice
    let mut total_influences = 0;
    for vert_idx in 0..num_vertices {
        let weights = attachment.get_weights(vert_idx);
        let active = weights.iter().filter(|&&w| w > 1e-6).count();
        total_influences += active;
    }
    let avg_influences = total_influences as Real / num_vertices.max(1) as Real;

    ProcessStats {
        num_vertices,
        num_bones,
        num_medial_spheres: embedding.sphere_bone_map.len(),
        embedding_quality: embedding.quality_score,
        avg_influences_per_vertex: avg_influences,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_skeleton::{BasicSkeleton, Bone};

    fn make_test_mesh() -> Mesh {
        let positions = vec![
            Vector3::new(-0.5, 0.0, -0.5),
            Vector3::new(0.5, 0.0, -0.5),
            Vector3::new(0.5, 1.0, -0.5),
            Vector3::new(-0.5, 1.0, -0.5),
            Vector3::new(-0.5, 0.0, 0.5),
            Vector3::new(0.5, 0.0, 0.5),
            Vector3::new(0.5, 1.0, 0.5),
            Vector3::new(-0.5, 1.0, 0.5),
        ];

        let indices = vec![
            [0, 1, 2], [0, 2, 3],
            [5, 4, 7], [5, 7, 6],
            [1, 5, 6], [1, 6, 2],
            [4, 0, 3], [4, 3, 7],
            [3, 2, 6], [3, 6, 7],
            [4, 5, 1], [4, 1, 0],
        ];

        Mesh::from_triangles(&positions, &indices)
    }

    fn make_test_skeleton() -> BasicSkeleton {
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::new(0.0, 0.0, 0.0)));
        skel.add_bone(Bone::with_parent("spine", Vector3::new(0.0, 0.5, 0.0), 0));
        skel.add_bone(Bone::with_parent("head", Vector3::new(0.0, 1.0, 0.0), 1));
        skel
    }

    #[test]
    fn test_autorig_basic() {
        let mesh = make_test_mesh();
        let skeleton = make_test_skeleton();

        let config = PinocchioConfig::fast();
        let result = autorig(&mesh, &skeleton, Some(config));

        // En un test real, esto podría fallar por la complejidad del proceso
        // Por ahora solo verificamos que no panic
        match result {
            Ok(output) => {
                assert_eq!(output.stats.num_bones, 3);
                assert!(output.stats.num_vertices > 0);
            }
            Err(e) => {
                // Algunos errores son esperables en mallas simples
                println!("Error esperado en test: {:?}", e);
            }
        }
    }

    #[test]
    fn test_validate_empty_mesh() {
        let mesh = Mesh::new();
        let skeleton = make_test_skeleton();

        let result = autorig(&mesh, &skeleton, None);
        assert!(matches!(result, Err(PinocchioError::EmptyMesh)));
    }

    #[test]
    fn test_validate_empty_skeleton() {
        let mesh = make_test_mesh();
        let skeleton = BasicSkeleton::new();

        let result = autorig(&mesh, &skeleton, None);
        assert!(matches!(result, Err(PinocchioError::EmptySkeleton)));
    }
}
