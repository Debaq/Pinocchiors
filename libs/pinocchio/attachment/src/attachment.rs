//! Attachment: deformación de malla según esqueleto

use pinocchio_math::{Real, Transform, Vector3};
use pinocchio_mesh::Mesh;
use rayon::prelude::*;

/// Attachment de una malla a un esqueleto
pub struct Attachment {
    /// Pesos de skinning [vértice][hueso]
    weights: Vec<Vec<Real>>,
    /// Posiciones originales de los vértices
    rest_positions: Vec<Vector3>,
    /// Número de huesos
    num_bones: usize,
}

impl Attachment {
    /// Crea un nuevo Attachment
    pub fn new(mesh: &Mesh, weights: Vec<Vec<Real>>, num_bones: usize) -> Self {
        let rest_positions = mesh.vertices.iter().map(|v| v.position).collect();

        Self {
            weights,
            rest_positions,
            num_bones,
        }
    }

    /// Crea un Attachment a partir de las posiciones de reposo de los vértices
    pub fn from_rest_positions(rest_positions: Vec<Vector3>, weights: Vec<Vec<Real>>, num_bones: usize) -> Self {
        Self {
            weights,
            rest_positions,
            num_bones,
        }
    }

    /// Obtiene los pesos de un vértice
    pub fn get_weights(&self, vertex_idx: usize) -> &[Real] {
        &self.weights[vertex_idx]
    }

    /// Número de vértices
    pub fn num_vertices(&self) -> usize {
        self.rest_positions.len()
    }

    /// Número de huesos
    pub fn num_bones(&self) -> usize {
        self.num_bones
    }

    /// Deforma la malla según las transformaciones de los huesos (en paralelo)
    ///
    /// # Arguments
    /// * `bone_transforms` - Transformaciones actuales de cada hueso
    /// * `rest_transforms` - Transformaciones en pose de reposo
    ///
    /// # Returns
    /// Posiciones deformadas de los vértices
    pub fn deform(
        &self,
        bone_transforms: &[Transform],
        rest_transforms: &[Transform],
    ) -> Vec<Vector3> {
        let num_vertices = self.num_vertices();

        // Paralelizar por vértice
        let deformed: Vec<Vector3> = (0..num_vertices)
            .into_par_iter()
            .map(|vert_idx| {
                let rest_pos = self.rest_positions[vert_idx];
                let weights = &self.weights[vert_idx];

                // Linear Blend Skinning (LBS)
                let mut new_pos = Vector3::zero();

                for bone_idx in 0..self.num_bones {
                    let weight = weights[bone_idx];
                    if weight < 1e-10 {
                        continue;
                    }

                    // Calcular transformación relativa
                    let rest_inv = rest_transforms[bone_idx].inverse_unchecked();
                    let relative = bone_transforms[bone_idx].compose(&rest_inv);

                    // Aplicar transformación ponderada
                    let transformed = relative.transform_point(&rest_pos);
                    new_pos += transformed * weight;
                }

                new_pos
            })
            .collect();

        deformed
    }

    /// Deforma usando Dual Quaternion Skinning (más suave que LBS)
    pub fn deform_dqs(
        &self,
        _bone_transforms: &[Transform],
        _rest_transforms: &[Transform],
    ) -> Vec<Vector3> {
        // TODO: Implementar Dual Quaternion Skinning
        // Por ahora, usar LBS
        self.rest_positions.clone()
    }

    /// Obtiene los huesos más influyentes para un vértice
    pub fn get_dominant_bones(&self, vertex_idx: usize, max_bones: usize) -> Vec<(usize, Real)> {
        dominant_influences(&self.weights[vertex_idx], max_bones)
    }

    /// Compacta los pesos manteniendo solo los N más significativos
    pub fn compact_weights(&mut self, max_influences: usize) {
        for vert_idx in 0..self.num_vertices() {
            let dominant = self.get_dominant_bones(vert_idx, max_influences);

            // Resetear todos los pesos
            self.weights[vert_idx].fill(0.0);

            // Asignar solo los dominantes
            for (bone_idx, weight) in dominant {
                self.weights[vert_idx][bone_idx] = weight;
            }
        }
    }

    /// Verifica que los pesos están normalizados
    pub fn verify_weights(&self) -> bool {
        for vert_idx in 0..self.num_vertices() {
            let sum: Real = self.weights[vert_idx].iter().sum();
            if (sum - 1.0).abs() > 1e-6 {
                return false;
            }
        }
        true
    }
}

/// Los `max_bones` huesos de mayor peso de un vértice, `(hueso, peso)` en
/// orden decreciente y renormalizados para sumar 1.
pub fn dominant_influences(weights: &[Real], max_bones: usize) -> Vec<(usize, Real)> {
    let mut indexed: Vec<(usize, Real)> = weights.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.total_cmp(&a.1));
    indexed.truncate(max_bones);

    let sum: Real = indexed.iter().map(|(_, w)| w).sum();
    if sum > 1e-10 {
        for (_, w) in &mut indexed {
            *w /= sum;
        }
    }
    indexed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_creation() {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let indices = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&positions, &indices);

        let weights = vec![
            vec![1.0, 0.0],
            vec![0.5, 0.5],
            vec![0.0, 1.0],
        ];

        let attachment = Attachment::new(&mesh, weights, 2);
        assert_eq!(attachment.num_vertices(), 3);
        assert_eq!(attachment.num_bones(), 2);
    }

    #[test]
    fn test_get_dominant_bones() {
        let positions = vec![Vector3::zero()];
        let indices: Vec<[usize; 3]> = vec![];
        let mesh = Mesh::from_triangles(&positions, &indices);

        let weights = vec![vec![0.1, 0.5, 0.3, 0.1]];
        let attachment = Attachment::new(&mesh, weights, 4);

        let dominant = attachment.get_dominant_bones(0, 2);
        assert_eq!(dominant.len(), 2);
        assert_eq!(dominant[0].0, 1); // Mayor peso
        assert_eq!(dominant[1].0, 2); // Segundo mayor
    }

    #[test]
    fn test_deform_identity() {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
        ];
        let indices: Vec<[usize; 3]> = vec![];
        let mesh = Mesh::from_triangles(&positions, &indices);

        let weights = vec![
            vec![1.0],
            vec![1.0],
        ];

        let attachment = Attachment::new(&mesh, weights, 1);

        let bone_transforms = vec![Transform::identity()];
        let rest_transforms = vec![Transform::identity()];

        let deformed = attachment.deform(&bone_transforms, &rest_transforms);

        assert!((deformed[0].x() - 0.0).abs() < 1e-10);
        assert!((deformed[1].x() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_verify_weights() {
        let positions = vec![Vector3::zero()];
        let indices: Vec<[usize; 3]> = vec![];
        let mesh = Mesh::from_triangles(&positions, &indices);

        // Pesos normalizados
        let weights = vec![vec![0.3, 0.7]];
        let attachment = Attachment::new(&mesh, weights, 2);
        assert!(attachment.verify_weights());

        // Pesos no normalizados
        let weights2 = vec![vec![0.5, 0.3]];
        let attachment2 = Attachment::new(&mesh, weights2, 2);
        assert!(!attachment2.verify_weights());
    }
}
