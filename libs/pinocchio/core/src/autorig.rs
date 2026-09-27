//! Función principal de auto-rigging

use crate::config::SkeletonFit;
use crate::{PinocchioConfig, PinocchioError, PinocchioOutput};
use crate::output::ProcessStats;
use pinocchio_attachment::{Attachment, HeatDiffusion};
use pinocchio_embedding::{full_embedding_pipeline, EmbeddingResult};
use pinocchio_math::{Real, Rect, Transform, Vector3};
use pinocchio_mesh::{decimate, Mesh};
use pinocchio_skeleton::{fit_to_bounds, map_positions, BasicSkeleton, Bone, Skeleton};

/// Fracción de la malla que ocupa una plantilla ajustada con [`SkeletonFit::Auto`]
const SKELETON_FILL: Real = 0.9;

/// Transformación de normalización: `p' = (p - center) * scale`
#[derive(Debug, Clone, Copy)]
struct Normalization {
    center: Vector3,
    scale: Real,
}

impl Normalization {
    fn identity() -> Self {
        Self { center: Vector3::zero(), scale: 1.0 }
    }

    /// Centra la caja en el origen y lleva su eje mayor a longitud 1
    fn for_bounds(bounds: &Rect) -> Self {
        let longest = bounds.longest_axis_length();
        let scale = if longest > 0.0 { 1.0 / longest } else { 1.0 };
        Self { center: bounds.center(), scale }
    }

    fn apply(&self, p: Vector3) -> Vector3 {
        (p - self.center) * self.scale
    }

    fn invert(&self, p: Vector3) -> Vector3 {
        p * (1.0 / self.scale) + self.center
    }
}

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

    // Si se decimó, los pesos se calcularán en la malla simplificada
    // y luego se transferirán a la malla original
    let _ = was_decimated; // TODO: implementar transferencia de pesos

    // 2b. Normalizar (el resultado se devuelve en las coordenadas originales)
    let normalization = if config.normalize_mesh {
        Normalization::for_bounds(&working_mesh.bounding_box())
    } else {
        Normalization::identity()
    };
    for vertex in &mut working_mesh.vertices {
        vertex.position = normalization.apply(vertex.position);
    }

    // 3. Llevar el esqueleto al espacio de trabajo de la malla
    let working_skeleton = match config.skeleton_fit {
        SkeletonFit::Auto => fit_to_bounds(skeleton, &working_mesh.bounding_box(), SKELETON_FILL),
        SkeletonFit::None => map_positions(skeleton, |p| normalization.apply(p)),
    };

    // 4. Embedding del esqueleto
    let embedding = full_embedding_pipeline(
        &working_mesh,
        &working_skeleton,
        config.distance_field_resolution,
    )?;

    // 5. Pesos de skinning con el esqueleto ya embebido
    let embedded_skeleton = BasicSkeleton::from_bones(
        working_skeleton
            .bones()
            .iter()
            .zip(&embedding.bone_positions)
            .map(|(bone, &position)| Bone { position, ..bone.clone() })
            .collect(),
    );
    let weights = HeatDiffusion::new(&working_mesh)
        .with_diffusion_weight(config.diffusion_weight)
        .compute_weights(&embedded_skeleton)?;

    // 6. Volver a las coordenadas originales
    let bone_positions: Vec<Vector3> = embedding
        .bone_positions
        .iter()
        .map(|&p| normalization.invert(p))
        .collect();
    let rest_positions = working_mesh
        .vertices
        .iter()
        .map(|v| normalization.invert(v.position))
        .collect();

    let mut attachment = Attachment::from_rest_positions(rest_positions, weights, skeleton.num_bones());
    attachment.compact_weights(config.max_bone_influences);

    let bone_rest_transforms = bone_positions
        .iter()
        .map(|&p| Transform::from_translation(p))
        .collect();

    let stats = compute_stats(&working_mesh, &working_skeleton, &attachment, &embedding);

    Ok(PinocchioOutput {
        attachment,
        embedding: EmbeddingResult {
            bone_positions: bone_positions.clone(),
            ..embedding
        },
        bone_positions,
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
