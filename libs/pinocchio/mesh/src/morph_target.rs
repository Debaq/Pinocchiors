//! Soporte para morph targets (blend shapes)
//!
//! Los morph targets permiten deformar una malla base usando deltas
//! de posición y normales opcionales.

use crate::Mesh;
use pinocchio_math::{Real, Vector3};
use std::collections::HashMap;

/// Delta de un vértice en un morph target
#[derive(Debug, Clone)]
pub struct MorphDelta {
    /// Desplazamiento de posición
    pub position_delta: Vector3,
    /// Desplazamiento de normal (opcional)
    pub normal_delta: Option<Vector3>,
}

impl MorphDelta {
    /// Crea un nuevo delta con solo posición
    pub fn new(position_delta: Vector3) -> Self {
        Self {
            position_delta,
            normal_delta: None,
        }
    }

    /// Crea un nuevo delta con posición y normal
    pub fn with_normal(position_delta: Vector3, normal_delta: Vector3) -> Self {
        Self {
            position_delta,
            normal_delta: Some(normal_delta),
        }
    }

    /// Interpola entre delta cero y este delta
    pub fn scaled(&self, weight: Real) -> Self {
        Self {
            position_delta: self.position_delta * weight,
            normal_delta: self.normal_delta.map(|n| n * weight),
        }
    }
}

/// Un morph target (blend shape)
#[derive(Debug, Clone)]
pub struct MorphTarget {
    /// Nombre del morph target
    pub name: String,
    /// Deltas por índice de vértice (sparse: solo vértices afectados)
    pub deltas: HashMap<usize, MorphDelta>,
}

impl MorphTarget {
    /// Crea un nuevo morph target vacío
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            deltas: HashMap::new(),
        }
    }

    /// Agrega un delta para un vértice
    pub fn add_delta(&mut self, vertex_idx: usize, delta: MorphDelta) {
        self.deltas.insert(vertex_idx, delta);
    }

    /// Agrega un delta de posición para un vértice
    pub fn add_position_delta(&mut self, vertex_idx: usize, delta: Vector3) {
        self.deltas.insert(vertex_idx, MorphDelta::new(delta));
    }

    /// Número de vértices afectados
    pub fn num_affected_vertices(&self) -> usize {
        self.deltas.len()
    }

    /// Verifica si afecta a un vértice específico
    pub fn affects_vertex(&self, vertex_idx: usize) -> bool {
        self.deltas.contains_key(&vertex_idx)
    }

    /// Obtiene el delta para un vértice (None si no está afectado)
    pub fn get_delta(&self, vertex_idx: usize) -> Option<&MorphDelta> {
        self.deltas.get(&vertex_idx)
    }
}

/// Malla con morph targets
#[derive(Debug, Clone)]
pub struct MeshWithMorphs {
    /// Malla base
    pub base_mesh: Mesh,
    /// Lista de morph targets
    pub morph_targets: Vec<MorphTarget>,
}

impl MeshWithMorphs {
    /// Crea una nueva malla con morphs desde una malla base
    pub fn new(base_mesh: Mesh) -> Self {
        Self {
            base_mesh,
            morph_targets: Vec::new(),
        }
    }

    /// Agrega un morph target y devuelve su índice
    pub fn add_morph_target(&mut self, target: MorphTarget) -> usize {
        let idx = self.morph_targets.len();
        self.morph_targets.push(target);
        idx
    }

    /// Obtiene un morph target por índice
    pub fn get_morph_target(&self, idx: usize) -> Option<&MorphTarget> {
        self.morph_targets.get(idx)
    }

    /// Obtiene un morph target por nombre
    pub fn get_morph_target_by_name(&self, name: &str) -> Option<&MorphTarget> {
        self.morph_targets.iter().find(|mt| mt.name == name)
    }

    /// Número de morph targets
    pub fn num_morph_targets(&self) -> usize {
        self.morph_targets.len()
    }

    /// Aplica los morph targets con los pesos dados y devuelve una nueva malla
    ///
    /// # Arguments
    /// * `weights` - Peso para cada morph target (debe tener mismo tamaño que morph_targets)
    ///
    /// # Panics
    /// Si `weights.len() != self.morph_targets.len()`
    pub fn apply_morphs(&self, weights: &[Real]) -> Mesh {
        assert_eq!(
            weights.len(),
            self.morph_targets.len(),
            "Número de pesos debe coincidir con número de morph targets"
        );

        let mut result = self.base_mesh.clone();

        for (morph_idx, weight) in weights.iter().enumerate() {
            if weight.abs() < 1e-10 {
                continue;
            }

            let morph = &self.morph_targets[morph_idx];

            for (&vert_idx, delta) in &morph.deltas {
                if vert_idx >= result.vertices.len() {
                    continue;
                }

                result.vertices[vert_idx].position += delta.position_delta * *weight;

                if let Some(normal_delta) = &delta.normal_delta {
                    result.vertices[vert_idx].normal += *normal_delta * *weight;
                }
            }
        }

        // Renormalizar normales si hubo cambios
        if weights.iter().any(|w| w.abs() > 1e-10) {
            for v in &mut result.vertices {
                if let Some(n) = v.normal.try_normalize() {
                    v.normal = n;
                }
            }
        }

        result
    }

    /// Aplica un solo morph target con un peso específico
    pub fn apply_single_morph(&self, morph_idx: usize, weight: Real) -> Mesh {
        let mut weights = vec![0.0; self.morph_targets.len()];
        if morph_idx < weights.len() {
            weights[morph_idx] = weight;
        }
        self.apply_morphs(&weights)
    }

    /// Interpola entre la malla base (weight=0) y un morph target completamente aplicado (weight=1)
    pub fn lerp_to_morph(&self, morph_idx: usize, t: Real) -> Mesh {
        self.apply_single_morph(morph_idx, t)
    }
}

impl From<Mesh> for MeshWithMorphs {
    fn from(mesh: Mesh) -> Self {
        Self::new(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_mesh() -> Mesh {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let indices = vec![[0, 1, 2]];
        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_morph_delta() {
        let delta = MorphDelta::new(Vector3::new(0.1, 0.2, 0.3));
        assert!(delta.normal_delta.is_none());

        let delta_with_normal =
            MorphDelta::with_normal(Vector3::new(0.1, 0.0, 0.0), Vector3::new(0.0, 0.1, 0.0));
        assert!(delta_with_normal.normal_delta.is_some());
    }

    #[test]
    fn test_morph_target() {
        let mut morph = MorphTarget::new("smile");
        morph.add_position_delta(0, Vector3::new(0.0, 0.1, 0.0));
        morph.add_position_delta(2, Vector3::new(0.0, 0.2, 0.0));

        assert_eq!(morph.num_affected_vertices(), 2);
        assert!(morph.affects_vertex(0));
        assert!(!morph.affects_vertex(1));
        assert!(morph.affects_vertex(2));
    }

    #[test]
    fn test_apply_morphs() {
        let mesh = make_simple_mesh();
        let mut mesh_with_morphs = MeshWithMorphs::new(mesh);

        // Crear morph que mueve el vértice superior
        let mut morph = MorphTarget::new("raise_top");
        morph.add_position_delta(2, Vector3::new(0.0, 0.5, 0.0));
        mesh_with_morphs.add_morph_target(morph);

        // Aplicar con peso 0 (sin cambio)
        let result0 = mesh_with_morphs.apply_morphs(&[0.0]);
        assert!((result0.vertices[2].position.y() - 1.0).abs() < 1e-10);

        // Aplicar con peso 1 (cambio completo)
        let result1 = mesh_with_morphs.apply_morphs(&[1.0]);
        assert!((result1.vertices[2].position.y() - 1.5).abs() < 1e-10);

        // Aplicar con peso 0.5 (mitad)
        let result05 = mesh_with_morphs.apply_morphs(&[0.5]);
        assert!((result05.vertices[2].position.y() - 1.25).abs() < 1e-10);
    }

    #[test]
    fn test_multiple_morphs() {
        let mesh = make_simple_mesh();
        let mut mesh_with_morphs = MeshWithMorphs::new(mesh);

        // Morph 1: mover vértice 0 hacia arriba
        let mut morph1 = MorphTarget::new("raise_v0");
        morph1.add_position_delta(0, Vector3::new(0.0, 1.0, 0.0));
        mesh_with_morphs.add_morph_target(morph1);

        // Morph 2: mover vértice 1 hacia arriba
        let mut morph2 = MorphTarget::new("raise_v1");
        morph2.add_position_delta(1, Vector3::new(0.0, 1.0, 0.0));
        mesh_with_morphs.add_morph_target(morph2);

        assert_eq!(mesh_with_morphs.num_morph_targets(), 2);

        // Aplicar ambos con peso 0.5
        let result = mesh_with_morphs.apply_morphs(&[0.5, 0.5]);
        assert!((result.vertices[0].position.y() - 0.5).abs() < 1e-10);
        assert!((result.vertices[1].position.y() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_get_by_name() {
        let mesh = make_simple_mesh();
        let mut mesh_with_morphs = MeshWithMorphs::new(mesh);

        mesh_with_morphs.add_morph_target(MorphTarget::new("smile"));
        mesh_with_morphs.add_morph_target(MorphTarget::new("frown"));

        assert!(mesh_with_morphs.get_morph_target_by_name("smile").is_some());
        assert!(mesh_with_morphs.get_morph_target_by_name("frown").is_some());
        assert!(mesh_with_morphs.get_morph_target_by_name("blink").is_none());
    }
}
