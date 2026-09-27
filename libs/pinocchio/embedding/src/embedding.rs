//! Embedding del esqueleto en la malla

use crate::medial_surface::MedialSphere;
use crate::sphere_graph::SphereGraph;
use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::Skeleton;
use pinocchio_spatial::DistanceField;
use thiserror::Error;

/// Error en el proceso de embedding
#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("Malla vacía")]
    EmptyMesh,
    #[error("Esqueleto vacío")]
    EmptySkeleton,
    #[error("No se pudo encontrar un embedding válido")]
    NoValidEmbedding,
    #[error("Hueso {0} no tiene posición válida")]
    InvalidBonePosition(String),
}

/// Resultado del embedding
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    /// Posiciones embebidas de los huesos
    pub bone_positions: Vec<Vector3>,
    /// Correspondencia esfera -> hueso más cercano
    pub sphere_bone_map: Vec<Option<usize>>,
    /// Puntuación de calidad del embedding
    pub quality_score: Real,
}

/// Realiza el embedding discreto inicial
pub fn discrete_embed<S: Skeleton>(
    mesh: &Mesh,
    skeleton: &S,
    spheres: &[MedialSphere],
) -> Result<EmbeddingResult, EmbeddingError> {
    if mesh.num_vertices() == 0 {
        return Err(EmbeddingError::EmptyMesh);
    }
    if skeleton.num_bones() == 0 {
        return Err(EmbeddingError::EmptySkeleton);
    }

    let num_bones = skeleton.num_bones();

    // Asignar cada hueso a la esfera más cercana
    let mut bone_positions = Vec::with_capacity(num_bones);
    let mut sphere_bone_map = vec![None; spheres.len()];

    for bone_idx in 0..num_bones {
        if let Some(bone) = skeleton.get_bone(bone_idx) {
            // Encontrar la esfera más cercana a la posición del hueso
            let (best_sphere_idx, _) = spheres
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let da = a.1.center.distance(&bone.position);
                    let db = b.1.center.distance(&bone.position);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or((0, &spheres[0]));

            bone_positions.push(spheres[best_sphere_idx].center);
            sphere_bone_map[best_sphere_idx] = Some(bone_idx);
        } else {
            return Err(EmbeddingError::InvalidBonePosition(format!("Bone {}", bone_idx)));
        }
    }

    // Calcular puntuación de calidad
    let quality = compute_embedding_quality(skeleton, &bone_positions);

    Ok(EmbeddingResult {
        bone_positions,
        sphere_bone_map,
        quality_score: quality,
    })
}

/// Refina el embedding usando optimización
pub fn refine_embedding<S: Skeleton>(
    _mesh: &Mesh,
    skeleton: &S,
    initial: &EmbeddingResult,
    distance_field: &DistanceField,
    num_iterations: usize,
) -> EmbeddingResult {
    let mut positions = initial.bone_positions.clone();
    let step_size = 0.01;

    for _ in 0..num_iterations {
        for bone_idx in 0..skeleton.num_bones() {
            if let Some(bone) = skeleton.get_bone(bone_idx) {
                // Gradiente hacia la posición objetivo
                let target = bone.position;
                let current = positions[bone_idx];
                let direction = (target - current).normalize();

                // Verificar que el nuevo punto esté dentro de la malla
                let new_pos = current + direction * step_size;

                if is_inside(distance_field, &new_pos) {
                    positions[bone_idx] = new_pos;
                }

                // Mantener conexiones con padre/hijos
                if let Some(parent_idx) = bone.parent {
                    let parent_pos = positions[parent_idx];
                    let bone_length = target.distance(&skeleton.get_bone(parent_idx).unwrap().position);
                    let current_length = positions[bone_idx].distance(&parent_pos);

                    // Ajustar para mantener longitud aproximada
                    if current_length > bone_length * 1.2 {
                        let direction = (positions[bone_idx] - parent_pos).normalize();
                        positions[bone_idx] = parent_pos + direction * bone_length;
                    }
                }
            }
        }
    }

    let quality = compute_embedding_quality(skeleton, &positions);

    EmbeddingResult {
        bone_positions: positions,
        sphere_bone_map: initial.sphere_bone_map.clone(),
        quality_score: quality,
    }
}

/// Configuración para refinamiento global del embedding
#[derive(Debug, Clone)]
pub struct RefinementConfig {
    /// Número máximo de iteraciones
    pub max_iterations: usize,
    /// Tasa de aprendizaje inicial
    pub learning_rate: Real,
    /// Factor de momentum (0.0-0.99)
    pub momentum: Real,
    /// Peso para el término de longitud de huesos
    pub length_weight: Real,
    /// Peso para el término de superficie medial
    pub medial_weight: Real,
    /// Peso para el término de suavidad
    pub smoothness_weight: Real,
    /// Tolerancia de convergencia
    pub convergence_tolerance: Real,
}

impl Default for RefinementConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            learning_rate: 0.01,
            momentum: 0.9,
            length_weight: 1.0,
            medial_weight: 0.5,
            smoothness_weight: 0.3,
            convergence_tolerance: 1e-6,
        }
    }
}

/// Refina el embedding usando optimización global
///
/// A diferencia de `refine_embedding`, esta función optimiza todos los huesos
/// simultáneamente considerando:
/// 1. Preservación de longitudes de huesos
/// 2. Mantenerse cerca de la superficie medial
/// 3. Suavidad de cadenas de huesos
pub fn refine_embedding_global<S: Skeleton>(
    skeleton: &S,
    initial: &EmbeddingResult,
    distance_field: &DistanceField,
    sphere_graph: &SphereGraph,
    config: &RefinementConfig,
) -> EmbeddingResult {
    let num_bones = skeleton.num_bones();
    let mut positions = initial.bone_positions.clone();
    let mut velocities = vec![Vector3::zero(); num_bones];

    // Precalcular longitudes objetivo
    let target_lengths: Vec<Option<Real>> = (0..num_bones)
        .map(|bone_idx| {
            skeleton.get_bone(bone_idx).and_then(|bone| {
                bone.parent.and_then(|parent_idx| {
                    skeleton.get_bone(parent_idx).map(|parent| {
                        bone.position.distance(&parent.position)
                    })
                })
            })
        })
        .collect();

    let mut prev_energy = compute_total_energy(
        skeleton,
        &positions,
        &target_lengths,
        distance_field,
        sphere_graph,
        config,
    );

    for _iter in 0..config.max_iterations {
        // Calcular gradientes para todos los huesos
        let gradients: Vec<Vector3> = (0..num_bones)
            .map(|bone_idx| {
                compute_gradient(
                    bone_idx,
                    skeleton,
                    &positions,
                    &target_lengths,
                    distance_field,
                    sphere_graph,
                    config,
                )
            })
            .collect();

        // Actualizar posiciones con momentum
        for bone_idx in 0..num_bones {
            velocities[bone_idx] = velocities[bone_idx] * config.momentum
                - gradients[bone_idx] * config.learning_rate;

            let new_pos = positions[bone_idx] + velocities[bone_idx];

            // Verificar que el nuevo punto esté dentro de la malla
            let dist = distance_field.sample(&new_pos);
            if is_inside(distance_field, &new_pos) {
                positions[bone_idx] = new_pos;
            } else if dist.is_finite() {
                // Proyectar al interior
                let gradient = distance_field.gradient(&positions[bone_idx]);
                let projected = positions[bone_idx] + gradient * dist.abs() * 1.1;
                if is_inside(distance_field, &projected) {
                    positions[bone_idx] = projected;
                }
                velocities[bone_idx] = Vector3::zero();
            }
        }

        // Verificar convergencia
        let energy = compute_total_energy(
            skeleton,
            &positions,
            &target_lengths,
            distance_field,
            sphere_graph,
            config,
        );

        if (prev_energy - energy).abs() < config.convergence_tolerance {
            break;
        }
        prev_energy = energy;
    }

    // Fase final: ajustar longitudes de huesos
    enforce_bone_lengths(skeleton, &mut positions, &target_lengths, 0.2);

    let quality = compute_embedding_quality(skeleton, &positions);

    EmbeddingResult {
        bone_positions: positions,
        sphere_bone_map: initial.sphere_bone_map.clone(),
        quality_score: quality,
    }
}

/// Calcula la energía total del embedding
fn compute_total_energy<S: Skeleton>(
    skeleton: &S,
    positions: &[Vector3],
    target_lengths: &[Option<Real>],
    _distance_field: &DistanceField,
    sphere_graph: &SphereGraph,
    config: &RefinementConfig,
) -> Real {
    let mut energy = 0.0;

    for bone_idx in 0..skeleton.num_bones() {
        // Término de longitud
        if let Some(target_len) = target_lengths[bone_idx]
            && let Some(bone) = skeleton.get_bone(bone_idx)
                && let Some(parent_idx) = bone.parent {
                    let actual_len = positions[bone_idx].distance(&positions[parent_idx]);
                    let len_error = (actual_len - target_len) / target_len.max(0.001);
                    energy += config.length_weight * len_error * len_error;
                }

        // Término de superficie medial (distancia a la esfera más cercana)
        if let Some(nearest) = sphere_graph.find_nearest_sphere(&positions[bone_idx])
            && let Some(sphere) = sphere_graph.get_sphere(nearest) {
                let dist_to_sphere = positions[bone_idx].distance(&sphere.center);
                energy += config.medial_weight * dist_to_sphere * dist_to_sphere;
            }

        // Término de suavidad (ángulos entre huesos consecutivos)
        if let Some(bone) = skeleton.get_bone(bone_idx)
            && let Some(parent_idx) = bone.parent {
                let children = skeleton.get_children(bone_idx);
                for child_idx in children {
                    let v1 = positions[bone_idx] - positions[parent_idx];
                    let v2 = positions[child_idx] - positions[bone_idx];

                    let dot = v1.normalize().dot(&v2.normalize());
                    // Penalizar ángulos muy agudos
                    if dot < 0.0 {
                        energy += config.smoothness_weight * (1.0 - dot);
                    }
                }
            }
    }

    energy
}

/// Calcula el gradiente de la energía respecto a la posición de un hueso
fn compute_gradient<S: Skeleton>(
    bone_idx: usize,
    skeleton: &S,
    positions: &[Vector3],
    target_lengths: &[Option<Real>],
    distance_field: &DistanceField,
    sphere_graph: &SphereGraph,
    config: &RefinementConfig,
) -> Vector3 {
    let h = 0.001;
    let mut gradient = Vector3::zero();

    // Gradiente numérico
    for axis in 0..3 {
        let mut pos_plus = positions.to_vec();
        let mut pos_minus = positions.to_vec();

        match axis {
            0 => {
                pos_plus[bone_idx] += Vector3::new(h, 0.0, 0.0);
                pos_minus[bone_idx] -= Vector3::new(h, 0.0, 0.0);
            }
            1 => {
                pos_plus[bone_idx] += Vector3::new(0.0, h, 0.0);
                pos_minus[bone_idx] -= Vector3::new(0.0, h, 0.0);
            }
            _ => {
                pos_plus[bone_idx] += Vector3::new(0.0, 0.0, h);
                pos_minus[bone_idx] -= Vector3::new(0.0, 0.0, h);
            }
        }

        let e_plus = compute_total_energy(skeleton, &pos_plus, target_lengths, distance_field, sphere_graph, config);
        let e_minus = compute_total_energy(skeleton, &pos_minus, target_lengths, distance_field, sphere_graph, config);

        let deriv = (e_plus - e_minus) / (2.0 * h);

        match axis {
            0 => gradient += Vector3::new(deriv, 0.0, 0.0),
            1 => gradient += Vector3::new(0.0, deriv, 0.0),
            _ => gradient += Vector3::new(0.0, 0.0, deriv),
        }
    }

    gradient
}

/// Ajusta las posiciones para mantener las longitudes de huesos dentro de tolerancia
fn enforce_bone_lengths<S: Skeleton>(
    skeleton: &S,
    positions: &mut [Vector3],
    target_lengths: &[Option<Real>],
    tolerance: Real,
) {
    for bone_idx in 0..skeleton.num_bones() {
        if let Some(target_len) = target_lengths[bone_idx]
            && let Some(bone) = skeleton.get_bone(bone_idx)
                && let Some(parent_idx) = bone.parent {
                    let actual_len = positions[bone_idx].distance(&positions[parent_idx]);
                    let error_ratio = (actual_len - target_len).abs() / target_len;

                    if error_ratio > tolerance {
                        // Ajustar la posición del hueso para corregir la longitud
                        let direction = (positions[bone_idx] - positions[parent_idx]).normalize();
                        positions[bone_idx] = positions[parent_idx] + direction * target_len;
                    }
                }
    }
}

/// Calcula la calidad del embedding
fn compute_embedding_quality<S: Skeleton>(skeleton: &S, positions: &[Vector3]) -> Real {
    let mut total_error = 0.0;
    let mut count = 0;

    for bone_idx in 0..skeleton.num_bones() {
        if let Some(bone) = skeleton.get_bone(bone_idx)
            && let Some(parent_idx) = bone.parent
                && let Some(parent) = skeleton.get_bone(parent_idx) {
                    // Error en la longitud del hueso
                    let target_length = bone.position.distance(&parent.position);
                    let actual_length = positions[bone_idx].distance(&positions[parent_idx]);

                    let length_error = (target_length - actual_length).abs() / target_length.max(0.001);
                    total_error += length_error;
                    count += 1;
                }
    }

    if count > 0 {
        1.0 - (total_error / count as Real).min(1.0)
    } else {
        1.0
    }
}

/// Indica si `pos` está dentro de la malla según un campo con signo
fn is_inside(field: &DistanceField, pos: &Vector3) -> bool {
    let d = field.sample(pos);
    d.is_finite() && d > 0.0
}

/// Pipeline completo de embedding.
///
/// El esqueleto debe estar en el mismo espacio que la malla y aproximadamente
/// alineado con ella (ver `pinocchio_skeleton::fit_to_bounds`). Usa el
/// embedding por cadenas sobre el eje medial ([`crate::chain_embed`]).
pub fn full_embedding_pipeline<S: Skeleton>(
    mesh: &Mesh,
    skeleton: &S,
    field_resolution: [usize; 3],
) -> Result<EmbeddingResult, EmbeddingError> {
    use crate::medial_surface::medial_spheres_from_field;

    if mesh.num_vertices() == 0 {
        return Err(EmbeddingError::EmptyMesh);
    }

    // 1. Campo de distancias con signo (positivo dentro)
    let padding = mesh.bounding_box().longest_axis_length() * 0.1;
    let distance_field = DistanceField::from_mesh_signed(mesh, field_resolution, padding);

    // 2. Esferas del eje medial
    let spheres = medial_spheres_from_field(&distance_field);
    if spheres.is_empty() {
        return Err(EmbeddingError::NoValidEmbedding);
    }

    // 3. Embedding por cadenas
    crate::chain::chain_embed(skeleton, &distance_field, &spheres)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_skeleton::{BasicSkeleton, Bone};

    fn make_simple_mesh() -> Mesh {
        let positions = vec![
            Vector3::new(-1.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, -1.0),
            Vector3::new(1.0, 2.0, -1.0),
            Vector3::new(-1.0, 2.0, -1.0),
            Vector3::new(-1.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(1.0, 2.0, 1.0),
            Vector3::new(-1.0, 2.0, 1.0),
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

    fn make_simple_skeleton() -> BasicSkeleton {
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::new(0.0, 0.5, 0.0)));
        skel.add_bone(Bone::with_parent("tip", Vector3::new(0.0, 1.5, 0.0), 0));
        skel
    }

    #[test]
    fn test_discrete_embed() {
        let mesh = make_simple_mesh();
        let skeleton = make_simple_skeleton();

        let spheres = vec![
            MedialSphere::new(Vector3::new(0.0, 0.5, 0.0), 0.5),
            MedialSphere::new(Vector3::new(0.0, 1.0, 0.0), 0.5),
            MedialSphere::new(Vector3::new(0.0, 1.5, 0.0), 0.5),
        ];

        let result = discrete_embed(&mesh, &skeleton, &spheres).unwrap();

        assert_eq!(result.bone_positions.len(), 2);
        assert!(result.quality_score > 0.0);
    }

    #[test]
    fn test_embedding_quality() {
        let skeleton = make_simple_skeleton();

        // Posiciones exactas
        let exact = vec![
            Vector3::new(0.0, 0.5, 0.0),
            Vector3::new(0.0, 1.5, 0.0),
        ];
        let quality_exact = compute_embedding_quality(&skeleton, &exact);
        assert!(quality_exact > 0.99);

        // Posiciones con error
        let with_error = vec![
            Vector3::new(0.0, 0.5, 0.0),
            Vector3::new(0.0, 2.0, 0.0), // Demasiado lejos
        ];
        let quality_error = compute_embedding_quality(&skeleton, &with_error);
        assert!(quality_error < quality_exact);
    }
}
