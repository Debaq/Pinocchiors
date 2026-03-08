//! Heat diffusion para cálculo de skinning weights

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::Skeleton;
use pinocchio_sparse::SPDMatrix;
use rayon::prelude::*;
use thiserror::Error;

/// Error en heat diffusion
#[derive(Debug, Error)]
pub enum HeatDiffusionError {
    #[error("Error en el solver de mínimos cuadrados")]
    SolverError,
    #[error("Malla vacía")]
    EmptyMesh,
    #[error("Esqueleto vacío")]
    EmptySkeleton,
}

/// Calculador de heat diffusion para skinning weights
pub struct HeatDiffusion<'a> {
    mesh: &'a Mesh,
    /// Peso de difusión (controla suavidad)
    diffusion_weight: Real,
}

impl<'a> HeatDiffusion<'a> {
    /// Crea un nuevo calculador de heat diffusion
    pub fn new(mesh: &'a Mesh) -> Self {
        Self {
            mesh,
            diffusion_weight: 1.0,
        }
    }

    /// Establece el peso de difusión
    pub fn with_diffusion_weight(mut self, weight: Real) -> Self {
        self.diffusion_weight = weight;
        self
    }

    /// Calcula los skinning weights para todos los vértices
    pub fn compute_weights<S: Skeleton>(
        &self,
        skeleton: &S,
        bone_heat: &[Vec<Real>],  // Heat inicial para cada vértice por cada hueso
    ) -> Result<Vec<Vec<Real>>, HeatDiffusionError> {
        let num_vertices = self.mesh.num_vertices();
        let num_bones = skeleton.num_bones();

        if num_vertices == 0 {
            return Err(HeatDiffusionError::EmptyMesh);
        }
        if num_bones == 0 {
            return Err(HeatDiffusionError::EmptySkeleton);
        }

        // Construir matriz Laplaciana
        let laplacian = self.build_laplacian();

        // Resolver (H - wL) * weights = H * initial_heat para cada hueso (en paralelo)
        let all_weights: Result<Vec<_>, _> = (0..num_bones)
            .into_par_iter()
            .map(|bone_idx| {
                let initial_heat = &bone_heat[bone_idx];
                self.solve_diffusion(&laplacian, initial_heat)
            })
            .collect();
        let all_weights = all_weights?;

        // Transponer: de [hueso][vértice] a [vértice][hueso]
        let mut vertex_weights = vec![vec![0.0; num_bones]; num_vertices];
        for bone_idx in 0..num_bones {
            for vert_idx in 0..num_vertices {
                vertex_weights[vert_idx][bone_idx] = all_weights[bone_idx][vert_idx];
            }
        }

        // Normalizar para que sumen 1
        for weights in &mut vertex_weights {
            let sum: Real = weights.iter().sum();
            if sum > 1e-10 {
                for w in weights.iter_mut() {
                    *w /= sum;
                }
            }
        }

        Ok(vertex_weights)
    }

    /// Construye la matriz Laplaciana cotangente (paralelizado)
    fn build_laplacian(&self) -> SPDMatrix {
        let n = self.mesh.num_vertices();
        let num_faces = self.mesh.num_faces();

        // Calcular contribuciones por cara en paralelo
        // Cada cara contribuye 12 entradas (4 por cada una de las 3 aristas)
        let contributions: Vec<_> = (0..num_faces)
            .into_par_iter()
            .flat_map(|face_idx| {
                let verts = self.mesh.get_face_vertices(face_idx);
                let positions = self.mesh.get_face_positions(face_idx);
                let mut triplets = Vec::with_capacity(12);

                for i in 0..3 {
                    let j = (i + 1) % 3;
                    let k = (i + 2) % 3;

                    let vi = verts[i];
                    let vj = verts[j];

                    // Peso cotangente
                    let cot_weight =
                        Self::cotangent_weight_static(&positions[i], &positions[j], &positions[k]);
                    let w = cot_weight * self.diffusion_weight * self.diffusion_weight;

                    // Añadir entradas para L(vi, vi), L(vj, vj), L(vi, vj), L(vj, vi)
                    triplets.push((vi, vi, w));
                    triplets.push((vj, vj, w));
                    triplets.push((vi, vj, -w));
                    triplets.push((vj, vi, -w));
                }

                triplets
            })
            .collect();

        // Desempaquetar triplets en vectores separados
        let mut rows = Vec::with_capacity(contributions.len());
        let mut cols = Vec::with_capacity(contributions.len());
        let mut values = Vec::with_capacity(contributions.len());

        for (r, c, v) in contributions {
            rows.push(r);
            cols.push(c);
            values.push(v);
        }

        SPDMatrix::from_triplets(n, &rows, &cols, &values)
            .unwrap_or_else(|_| SPDMatrix::new(n))
    }

    /// Calcula el peso cotangente para una arista (versión estática para paralelización)
    fn cotangent_weight_static(pi: &Vector3, pj: &Vector3, pk: &Vector3) -> Real {
        let v1 = *pi - *pk;
        let v2 = *pj - *pk;

        let len1 = v1.length();
        let len2 = v2.length();

        if len1 < 1e-10 || len2 < 1e-10 {
            return 0.0;
        }

        let cos_angle = v1.dot(&v2) / (len1 * len2);
        let sin_angle = v1.cross(&v2).length() / (len1 * len2);

        if sin_angle.abs() < 1e-10 {
            0.0
        } else {
            (cos_angle / sin_angle).max(0.0)
        }
    }


    /// Resuelve el sistema de difusión: (I + λL) * w = h
    ///
    /// Donde:
    /// - L es la matriz Laplaciana
    /// - λ es el peso de difusión (self.diffusion_weight^2)
    /// - h es el heat inicial
    /// - w son los pesos resultantes
    fn solve_diffusion(
        &self,
        laplacian: &SPDMatrix,
        initial_heat: &[Real],
    ) -> Result<Vec<Real>, HeatDiffusionError> {
        let scale = self.diffusion_weight * self.diffusion_weight;

        laplacian
            .solve_with_identity(initial_heat, scale, 500, 1e-6)
            .map_err(|_| HeatDiffusionError::SolverError)
    }

    /// Calcula el heat inicial basado en la distancia a los huesos (en paralelo)
    pub fn compute_initial_heat<S: Skeleton + Sync>(&self, skeleton: &S) -> Vec<Vec<Real>> {
        let num_vertices = self.mesh.num_vertices();
        let num_bones = skeleton.num_bones();

        // Paralelizar por hueso
        let heat: Vec<Vec<Real>> = (0..num_bones)
            .into_par_iter()
            .map(|bone_idx| {
                let mut bone_heat = vec![0.0; num_vertices];
                if let Some(bone) = skeleton.get_bone(bone_idx) {
                    if let Some(parent_idx) = bone.parent {
                        if let Some(parent) = skeleton.get_bone(parent_idx) {
                            for vert_idx in 0..num_vertices {
                                let pos = self.mesh.vertices[vert_idx].position;
                                let dist = Self::point_to_segment_distance(
                                    &pos,
                                    &parent.position,
                                    &bone.position,
                                );
                                // Heat inversamente proporcional a la distancia
                                bone_heat[vert_idx] = 1.0 / (1.0 + dist * dist);
                            }
                        }
                    }
                }
                bone_heat
            })
            .collect();

        heat
    }

    /// Distancia de un punto a un segmento
    fn point_to_segment_distance(point: &Vector3, seg_start: &Vector3, seg_end: &Vector3) -> Real {
        let v = *seg_end - *seg_start;
        let w = *point - *seg_start;

        let c1 = w.dot(&v);
        if c1 <= 0.0 {
            return point.distance(seg_start);
        }

        let c2 = v.dot(&v);
        if c2 <= c1 {
            return point.distance(seg_end);
        }

        let b = c1 / c2;
        let closest = *seg_start + v * b;
        point.distance(&closest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_skeleton::{BasicSkeleton, Bone};

    fn make_simple_mesh() -> Mesh {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let indices = vec![
            [0, 1, 2],
            [0, 3, 1],
            [1, 3, 2],
            [2, 3, 0],
        ];
        Mesh::from_triangles(&positions, &indices)
    }

    fn make_simple_skeleton() -> BasicSkeleton {
        let mut skel = BasicSkeleton::new();
        skel.add_bone(Bone::new("root", Vector3::new(0.5, 0.0, 0.0)));
        skel.add_bone(Bone::with_parent("tip", Vector3::new(0.5, 1.0, 0.0), 0));
        skel
    }

    #[test]
    fn test_heat_diffusion_creation() {
        let mesh = make_simple_mesh();
        let hd = HeatDiffusion::new(&mesh);
        assert!((hd.diffusion_weight - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_initial_heat() {
        let mesh = make_simple_mesh();
        let skeleton = make_simple_skeleton();
        let hd = HeatDiffusion::new(&mesh);

        let heat = hd.compute_initial_heat(&skeleton);
        assert_eq!(heat.len(), 2); // 2 huesos
        assert_eq!(heat[0].len(), 4); // 4 vértices
    }

    #[test]
    fn test_point_to_segment() {
        let p = Vector3::new(1.0, 0.5, 0.0);
        let s1 = Vector3::new(0.0, 0.0, 0.0);
        let s2 = Vector3::new(0.0, 1.0, 0.0);

        let dist = HeatDiffusion::point_to_segment_distance(&p, &s1, &s2);
        assert!((dist - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_weights_normalized() {
        let mesh = make_simple_mesh();
        let skeleton = make_simple_skeleton();
        let hd = HeatDiffusion::new(&mesh).with_diffusion_weight(0.5);

        let bone_heat = hd.compute_initial_heat(&skeleton);
        let weights = hd.compute_weights(&skeleton, &bone_heat).unwrap();

        // Verificar que los pesos están normalizados (suman ~1 por vértice)
        for vert_weights in &weights {
            let sum: Real = vert_weights.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01 || sum < 0.01,
                "Suma de pesos = {}, esperado ~1.0", sum
            );
        }
    }
}
