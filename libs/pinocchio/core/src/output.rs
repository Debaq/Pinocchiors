//! Resultado del proceso de auto-rigging

use pinocchio_attachment::Attachment;
use pinocchio_embedding::EmbeddingResult;
use pinocchio_math::{Real, Vector3};

/// Resultado del proceso de auto-rigging
pub struct PinocchioOutput {
    /// Attachment (pesos de skinning)
    pub attachment: Attachment,

    /// Resultado del embedding
    pub embedding: EmbeddingResult,

    /// Posiciones embebidas de los huesos
    pub bone_positions: Vec<Vector3>,

    /// Transformaciones de referencia de los huesos
    pub bone_rest_transforms: Vec<pinocchio_math::Transform>,

    /// Estadísticas del proceso
    pub stats: ProcessStats,
}

/// Estadísticas del proceso de auto-rigging
#[derive(Debug, Clone, Default)]
pub struct ProcessStats {
    /// Número de vértices procesados
    pub num_vertices: usize,

    /// Número de huesos
    pub num_bones: usize,

    /// Número de esferas mediales usadas
    pub num_medial_spheres: usize,

    /// Puntuación de calidad del embedding
    pub embedding_quality: Real,

    /// Promedio de huesos influyentes por vértice
    pub avg_influences_per_vertex: Real,
}

impl PinocchioOutput {
    /// Obtiene los pesos de skinning de un vértice
    pub fn get_weights(&self, vertex_idx: usize) -> &[Real] {
        self.attachment.get_weights(vertex_idx)
    }

    /// Obtiene los huesos dominantes para un vértice
    pub fn get_dominant_bones(&self, vertex_idx: usize, max: usize) -> Vec<(usize, Real)> {
        self.attachment.get_dominant_bones(vertex_idx, max)
    }

    /// Deforma la malla usando las transformaciones de huesos dadas
    pub fn deform(&self, bone_transforms: &[pinocchio_math::Transform]) -> Vec<Vector3> {
        self.attachment.deform(bone_transforms, &self.bone_rest_transforms)
    }

    /// Obtiene la posición embebida de un hueso
    pub fn get_bone_position(&self, bone_idx: usize) -> Option<Vector3> {
        self.bone_positions.get(bone_idx).copied()
    }

    /// Exporta los pesos en formato compatible con la mayoría de motores 3D
    ///
    /// Devuelve (indices, weights) donde cada vértice tiene hasta `max_influences`
    /// huesos influyentes
    pub fn export_weights(&self, max_influences: usize) -> (Vec<Vec<usize>>, Vec<Vec<Real>>) {
        let num_vertices = self.attachment.num_vertices();
        let mut indices = Vec::with_capacity(num_vertices);
        let mut weights = Vec::with_capacity(num_vertices);

        for vert_idx in 0..num_vertices {
            let dominant = self.attachment.get_dominant_bones(vert_idx, max_influences);

            let mut vert_indices = Vec::with_capacity(max_influences);
            let mut vert_weights = Vec::with_capacity(max_influences);

            for (bone_idx, weight) in dominant {
                vert_indices.push(bone_idx);
                vert_weights.push(weight);
            }

            // Rellenar con ceros si hay menos influencias
            while vert_indices.len() < max_influences {
                vert_indices.push(0);
                vert_weights.push(0.0);
            }

            indices.push(vert_indices);
            weights.push(vert_weights);
        }

        (indices, weights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Transform;

    fn make_dummy_output() -> PinocchioOutput {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
        ];
        let indices: Vec<[usize; 3]> = vec![];
        let mesh = pinocchio_mesh::Mesh::from_triangles(&positions, &indices);

        let weights = vec![
            vec![0.8, 0.2],
            vec![0.3, 0.7],
        ];

        let attachment = Attachment::new(&mesh, weights, 2);

        let embedding = EmbeddingResult {
            bone_positions: vec![Vector3::zero(), Vector3::unit_y()],
            sphere_bone_map: vec![],
            quality_score: 0.9,
        };

        PinocchioOutput {
            attachment,
            embedding,
            bone_positions: vec![Vector3::zero(), Vector3::unit_y()],
            bone_rest_transforms: vec![Transform::identity(), Transform::identity()],
            stats: ProcessStats {
                num_vertices: 2,
                num_bones: 2,
                num_medial_spheres: 10,
                embedding_quality: 0.9,
                avg_influences_per_vertex: 2.0,
            },
        }
    }

    #[test]
    fn test_get_weights() {
        let output = make_dummy_output();
        let weights = output.get_weights(0);
        assert!((weights[0] - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_export_weights() {
        let output = make_dummy_output();
        let (indices, weights) = output.export_weights(4);

        assert_eq!(indices.len(), 2);
        assert_eq!(weights.len(), 2);
        assert_eq!(indices[0].len(), 4);
        assert_eq!(weights[0].len(), 4);
    }
}
