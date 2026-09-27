use crate::{Animation, Material, Mesh, Skeleton, Texture, Transform, VertexAttribute};
use thiserror::Error;

/// Nodo del grafo de escena.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub transform: Transform,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Error)]
pub enum SceneError {
    #[error("nodo {node} referencia mesh {index} pero solo hay {count} meshes")]
    MeshOutOfBounds { node: usize, index: usize, count: usize },
    #[error("nodo {node} referencia skin {index} pero solo hay {count} skeletons")]
    SkinOutOfBounds { node: usize, index: usize, count: usize },
    #[error("nodo {node} referencia hijo {index} pero solo hay {count} nodos")]
    ChildOutOfBounds { node: usize, index: usize, count: usize },
    #[error("root_nodes referencia nodo {index} pero solo hay {count} nodos")]
    RootOutOfBounds { index: usize, count: usize },
    #[error("primitiva {prim} del mesh {mesh} referencia material {index} pero solo hay {count} materiales")]
    MaterialOutOfBounds { mesh: usize, prim: usize, index: usize, count: usize },
    #[error("mesh {0} no tiene primitivas")]
    EmptyMesh(usize),
    #[error("primitiva {prim} del mesh {mesh} no tiene posiciones")]
    MissingPositions { mesh: usize, prim: usize },
}

/// Escena 3D completa — formato pivote entre todos los conversores.
#[derive(Debug, Clone)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub root_nodes: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub skeletons: Vec<Skeleton>,
    pub animations: Vec<Animation>,
    /// Metros por unidad (glTF=1.0, USD default=0.01)
    pub meters_per_unit: f64,
    /// Eje "arriba" — true = Y-up, false = Z-up
    pub y_up: bool,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root_nodes: Vec::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            skeletons: Vec::new(),
            animations: Vec::new(),
            meters_per_unit: 1.0,
            y_up: true,
        }
    }

    /// Valida que todos los índices internos sean consistentes.
    pub fn validate(&self) -> Result<(), SceneError> {
        let n_nodes = self.nodes.len();
        let n_meshes = self.meshes.len();
        let n_materials = self.materials.len();
        let n_skeletons = self.skeletons.len();

        for &root in &self.root_nodes {
            if root >= n_nodes {
                return Err(SceneError::RootOutOfBounds { index: root, count: n_nodes });
            }
        }

        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(mesh_idx) = node.mesh {
                if mesh_idx >= n_meshes {
                    return Err(SceneError::MeshOutOfBounds { node: i, index: mesh_idx, count: n_meshes });
                }
            }
            if let Some(skin_idx) = node.skin {
                if skin_idx >= n_skeletons {
                    return Err(SceneError::SkinOutOfBounds { node: i, index: skin_idx, count: n_skeletons });
                }
            }
            for &child in &node.children {
                if child >= n_nodes {
                    return Err(SceneError::ChildOutOfBounds { node: i, index: child, count: n_nodes });
                }
            }
        }

        for (mi, mesh) in self.meshes.iter().enumerate() {
            if mesh.primitives.is_empty() {
                return Err(SceneError::EmptyMesh(mi));
            }
            for (pi, prim) in mesh.primitives.iter().enumerate() {
                if let Some(mat_idx) = prim.material {
                    if mat_idx >= n_materials {
                        return Err(SceneError::MaterialOutOfBounds {
                            mesh: mi, prim: pi, index: mat_idx, count: n_materials,
                        });
                    }
                }
                let has_positions = prim.attributes.iter().any(|a| matches!(a, VertexAttribute::Positions(_)));
                if !has_positions {
                    return Err(SceneError::MissingPositions { mesh: mi, prim: pi });
                }
            }
        }

        Ok(())
    }

    /// Combina otra escena dentro de esta, ajustando todos los índices.
    pub fn merge(&mut self, other: Scene) {
        let node_offset = self.nodes.len();
        let mesh_offset = self.meshes.len();
        let mat_offset = self.materials.len();
        let tex_offset = self.textures.len();
        let skel_offset = self.skeletons.len();

        // Reasignar índices de nodos
        for mut node in other.nodes {
            node.mesh = node.mesh.map(|i| i + mesh_offset);
            node.skin = node.skin.map(|i| i + skel_offset);
            node.children = node.children.into_iter().map(|i| i + node_offset).collect();
            self.nodes.push(node);
        }

        // Root nodes con offset
        for root in other.root_nodes {
            self.root_nodes.push(root + node_offset);
        }

        // Reasignar material indices en primitivas
        for mut mesh in other.meshes {
            for prim in &mut mesh.primitives {
                prim.material = prim.material.map(|i| i + mat_offset);
            }
            self.meshes.push(mesh);
        }

        // Reasignar texture indices en materiales
        for mut mat in other.materials {
            if let Some(ref mut t) = mat.base_color_texture { t.texture_index += tex_offset; }
            if let Some(ref mut t) = mat.metallic_roughness_texture { t.texture_index += tex_offset; }
            if let Some(ref mut t) = mat.normal_texture { t.texture_index += tex_offset; }
            if let Some(ref mut t) = mat.occlusion_texture { t.texture_index += tex_offset; }
            if let Some(ref mut t) = mat.emissive_texture { t.texture_index += tex_offset; }
            self.materials.push(mat);
        }

        self.textures.extend(other.textures);
        self.skeletons.extend(other.skeletons);

        // Reasignar node indices en canales de animación
        for mut anim in other.animations {
            for ch in &mut anim.channels {
                ch.node += node_offset;
            }
            self.animations.push(anim);
        }
    }

    /// Calcula el bounding box global (AABB) de toda la geometría, en espacio
    /// mundo (aplicando las transformaciones de los nodos).
    ///
    /// Retorna `None` si la escena no contiene vértices.
    /// Retorna `(min, max)` como arrays `[f32; 3]`.
    pub fn compute_bounding_box(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found = false;

        for prim in self.world_primitives() {
            for p in &prim.positions {
                found = true;
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
        }

        if found { Some((min, max)) } else { None }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IndexData, Primitive};

    fn simple_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "cube".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "root".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn validate_ok() {
        let scene = simple_scene();
        assert!(scene.validate().is_ok());
    }

    #[test]
    fn validate_mesh_out_of_bounds() {
        let mut scene = simple_scene();
        scene.nodes[0].mesh = Some(99);
        assert!(matches!(
            scene.validate(),
            Err(SceneError::MeshOutOfBounds { .. })
        ));
    }

    #[test]
    fn validate_empty_mesh() {
        let mut scene = simple_scene();
        scene.meshes[0].primitives.clear();
        assert!(matches!(
            scene.validate(),
            Err(SceneError::EmptyMesh(0))
        ));
    }

    #[test]
    fn validate_missing_positions() {
        let mut scene = simple_scene();
        scene.meshes[0].primitives[0].attributes.clear();
        assert!(matches!(
            scene.validate(),
            Err(SceneError::MissingPositions { .. })
        ));
    }

    #[test]
    fn bounding_box_simple() {
        let scene = simple_scene();
        let (min, max) = scene.compute_bounding_box().unwrap();
        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 0.0]);
    }

    #[test]
    fn bounding_box_empty() {
        let scene = Scene::new();
        assert!(scene.compute_bounding_box().is_none());
    }

    #[test]
    fn merge_scenes() {
        let mut a = simple_scene();
        let b = simple_scene();

        a.merge(b);

        assert_eq!(a.meshes.len(), 2);
        assert_eq!(a.nodes.len(), 2);
        assert_eq!(a.root_nodes, vec![0, 1]);

        // El segundo nodo debe apuntar al mesh 1
        assert_eq!(a.nodes[1].mesh, Some(1));

        // Ambas meshes deben contribuir al bounding box
        let (min, max) = a.compute_bounding_box().unwrap();
        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 0.0]);

        assert!(a.validate().is_ok());
    }
}
