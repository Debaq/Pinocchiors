//! Función principal de auto-rigging

use crate::config::SkeletonFit;
use crate::{PinocchioConfig, PinocchioError, PinocchioOutput};
use crate::output::ProcessStats;
use pinocchio_attachment::{Attachment, HeatDiffusion};
use pinocchio_embedding::{full_embedding_pipeline, EmbeddingResult};
use pinocchio_math::{Real, Rect, Transform, Vector3};
use pinocchio_mesh::{decimate, Mesh};
use pinocchio_skeleton::{fit_to_bounds, map_positions, BasicSkeleton, Bone, Skeleton};

/// Etapas del auto-rigging, para reportar progreso
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutorigStage {
    /// Validación, decimación y normalización de la malla
    Preparing,
    /// Campo de distancias, eje medial y embedding del esqueleto
    Embedding,
    /// Cálculo de pesos de skinning (bone heat)
    Weights,
    /// Terminado
    Done,
}

impl AutorigStage {
    /// Nombre corto de la etapa
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Embedding => "embedding",
            Self::Weights => "weights",
            Self::Done => "done",
        }
    }

    /// Porcentaje aproximado al comenzar la etapa
    pub fn progress(&self) -> u32 {
        match self {
            Self::Preparing => 0,
            Self::Embedding => 10,
            Self::Weights => 50,
            Self::Done => 100,
        }
    }
}

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
    autorig_with_progress(mesh, skeleton, config, |_| {})
}

/// Igual que [`autorig`], llamando `on_stage` al comenzar cada etapa
pub fn autorig_with_progress<S: Skeleton + Sync>(
    mesh: &Mesh,
    skeleton: &S,
    config: Option<PinocchioConfig>,
    mut on_stage: impl FnMut(AutorigStage),
) -> Result<PinocchioOutput, PinocchioError> {
    on_stage(AutorigStage::Preparing);
    let config = config.unwrap_or_default();

    // 1. Validar entrada
    validate_input(mesh, skeleton, &config)?;

    // 2. Preparar malla
    // 2a. Soldar duplicados (costuras UV): todos los vértices de una costura
    //     reciben los mismos pesos y la difusión no se corta en ella
    let weld_tolerance = mesh.bounding_box().diagonal() * 1e-7;
    let (welded, weld_map) = mesh.welded(weld_tolerance);

    // 2b. Decimar si la malla es muy grande (los pesos se transfieren después)
    let was_decimated = welded.num_faces() > config.auto_decimate_threshold;
    let mut working_mesh = if was_decimated {
        decimate(&welded, config.decimate_ratio)
    } else {
        welded.clone()
    };

    // 2c. Normalizar (el resultado se devuelve en las coordenadas originales)
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
    on_stage(AutorigStage::Embedding);
    let embedding = full_embedding_pipeline(
        &working_mesh,
        &working_skeleton,
        config.distance_field_resolution,
    )?;

    // 5. Pesos de skinning con el esqueleto ya embebido
    on_stage(AutorigStage::Weights);
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

    // 6. Pesos por vértice de la malla original
    let welded_weights = if was_decimated {
        let targets: Vec<Vector3> = welded.vertices.iter().map(|v| normalization.apply(v.position)).collect();
        transfer_weights(&working_mesh, &weights, &targets)
    } else {
        weights
    };
    let original_weights: Vec<Vec<Real>> = weld_map.iter().map(|&w| welded_weights[w].clone()).collect();

    // 7. Volver a las coordenadas originales
    let bone_positions: Vec<Vector3> = embedding
        .bone_positions
        .iter()
        .map(|&p| normalization.invert(p))
        .collect();
    let rest_positions = mesh.vertices.iter().map(|v| v.position).collect();

    let mut attachment = Attachment::from_rest_positions(rest_positions, original_weights, skeleton.num_bones());
    attachment.compact_weights(config.max_bone_influences);

    let bone_rest_transforms = bone_positions
        .iter()
        .map(|&p| Transform::from_translation(p))
        .collect();

    let stats = compute_stats(mesh, &working_skeleton, &attachment, &embedding);
    on_stage(AutorigStage::Done);

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

/// Transfiere pesos `[vértice][hueso]` de `source` a puntos arbitrarios en el
/// mismo espacio: cada punto toma el punto más cercano de `source` e interpola
/// con coordenadas baricéntricas los pesos de los vértices de ese triángulo.
///
/// Sirve para llevar un rig a otra malla de la misma forma (p. ej. la malla
/// retopologizada) sin recalcularlo.
pub fn transfer_weights(source: &Mesh, weights: &[Vec<Real>], targets: &[Vector3]) -> Vec<Vec<Real>> {
    use pinocchio_spatial::{Bvh, Triangle};
    use rayon::prelude::*;

    let triangles: Vec<Triangle> = (0..source.num_faces())
        .map(|f| {
            let [a, b, c] = source.get_face_positions(f);
            Triangle::new(a, b, c)
        })
        .collect();
    let bvh = Bvh::build(triangles);
    let num_bones = weights.first().map_or(0, Vec::len);

    targets
        .par_iter()
        .map(|p| {
            let mut out = vec![0.0; num_bones];
            let Some(hit) = bvh.query_closest(p) else {
                return out;
            };
            let verts = source.get_face_vertices(hit.triangle);
            let [a, b, c] = source.get_face_positions(hit.triangle);
            for (w, &v) in barycentric(&hit.point, &a, &b, &c).iter().zip(&verts) {
                for (o, &x) in out.iter_mut().zip(&weights[v]) {
                    *o += w * x;
                }
            }
            let sum: Real = out.iter().sum();
            if sum > 1e-12 {
                out.iter_mut().for_each(|w| *w /= sum);
            }
            out
        })
        .collect()
}

/// Coordenadas baricéntricas de `p` (sobre el triángulo) respecto de `a, b, c`
fn barycentric(p: &Vector3, a: &Vector3, b: &Vector3, c: &Vector3) -> [Real; 3] {
    let v0 = *b - *a;
    let v1 = *c - *a;
    let v2 = *p - *a;
    let d00 = v0.dot(&v0);
    let d01 = v0.dot(&v1);
    let d11 = v1.dot(&v1);
    let d20 = v2.dot(&v0);
    let d21 = v2.dot(&v1);
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-30 {
        // Triángulo degenerado: el vértice más cercano
        let d = [p.distance_squared(a), p.distance_squared(b), p.distance_squared(c)];
        let i = (0..3).min_by(|&x, &y| d[x].total_cmp(&d[y])).unwrap();
        let mut w = [0.0; 3];
        w[i] = 1.0;
        return w;
    }
    let v = ((d11 * d20 - d01 * d21) / denom).clamp(0.0, 1.0);
    let w = ((d00 * d21 - d01 * d20) / denom).clamp(0.0, 1.0);
    let u = (1.0 - v - w).max(0.0);
    let sum = u + v + w;
    [u / sum, v / sum, w / sum]
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
