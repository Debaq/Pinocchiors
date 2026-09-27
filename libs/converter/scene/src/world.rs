//! Geometría en espacio mundo: aplica las transformaciones del grafo de nodos.
//!
//! Todos los consumidores que necesitan la geometría "aplanada" (exportadores
//! sin grafo de escena como STL/OBJ, visores, rigging) deben usar estas
//! funciones para recorrer la escena en el mismo orden.

use crate::{IndexData, Scene, VertexAttribute};
use glam::{Mat3, Mat4, Vec3};

/// Una malla colocada en la escena por un nodo
#[derive(Debug, Clone, Copy)]
pub struct MeshInstance {
    /// Índice en `Scene::meshes`
    pub mesh: usize,
    /// Nodo que la instancia (`None` si la escena no tiene nodos)
    pub node: Option<usize>,
    /// Transformación de mundo aplicada a los vértices
    pub world: Mat4,
}

/// Primitiva con sus vértices en espacio mundo, triangulada
#[derive(Debug, Clone)]
pub struct WorldPrimitive {
    /// Índice de la instancia en [`Scene::mesh_instances`]
    pub instance: usize,
    /// Índice en `Scene::meshes`
    pub mesh: usize,
    /// Índice de la primitiva dentro de la malla
    pub primitive: usize,
    /// Nodo que instancia la malla
    pub node: Option<usize>,
    pub positions: Vec<[f32; 3]>,
    /// Normales transformadas y normalizadas
    pub normals: Option<Vec<[f32; 3]>>,
    /// Coordenadas UV (set 0)
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Triángulos con índices válidos; el orden se invierte si la
    /// transformación refleja (determinante negativo)
    pub triangles: Vec<[u32; 3]>,
    pub material: Option<usize>,
}

impl Scene {
    /// Nodos raíz efectivos: `root_nodes`, o si está vacío, los nodos que no
    /// son hijos de ningún otro
    fn effective_roots(&self) -> Vec<usize> {
        if !self.root_nodes.is_empty() {
            return self.root_nodes.iter().copied().filter(|&r| r < self.nodes.len()).collect();
        }
        let mut is_child = vec![false; self.nodes.len()];
        for node in &self.nodes {
            for &c in &node.children {
                if c < is_child.len() {
                    is_child[c] = true;
                }
            }
        }
        (0..self.nodes.len()).filter(|&i| !is_child[i]).collect()
    }

    /// Recorre los nodos en preorden desde las raíces, llamando `visit(nodo, mundo)`.
    /// Ignora ciclos e índices inválidos.
    fn walk_nodes(&self, mut visit: impl FnMut(usize, Mat4)) {
        let mut visited = vec![false; self.nodes.len()];
        let mut stack: Vec<(usize, Mat4)> =
            self.effective_roots().into_iter().rev().map(|r| (r, Mat4::IDENTITY)).collect();
        while let Some((idx, parent)) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            let node = &self.nodes[idx];
            let world = parent * node.transform.to_matrix();
            visit(idx, world);
            for &child in node.children.iter().rev() {
                if child < self.nodes.len() {
                    stack.push((child, world));
                }
            }
        }
    }

    /// Transformación de mundo de cada nodo (identidad si no es alcanzable)
    pub fn world_transforms(&self) -> Vec<Mat4> {
        let mut worlds = vec![Mat4::IDENTITY; self.nodes.len()];
        self.walk_nodes(|idx, world| worlds[idx] = world);
        worlds
    }

    /// Mallas instanciadas por el grafo de nodos, en preorden.
    ///
    /// - Si la escena no tiene nodos, cada malla aparece una vez con identidad.
    /// - Una malla con skin usa identidad: como en glTF, sus vértices ya están
    ///   en el espacio de bind del esqueleto y se ignora la transformación del nodo.
    pub fn mesh_instances(&self) -> Vec<MeshInstance> {
        if self.nodes.is_empty() {
            return (0..self.meshes.len())
                .map(|mesh| MeshInstance { mesh, node: None, world: Mat4::IDENTITY })
                .collect();
        }
        let mut instances = Vec::new();
        self.walk_nodes(|idx, world| {
            let node = &self.nodes[idx];
            if let Some(mesh) = node.mesh.filter(|&m| m < self.meshes.len()) {
                let world = if node.skin.is_some() { Mat4::IDENTITY } else { world };
                instances.push(MeshInstance { mesh, node: Some(idx), world });
            }
        });
        instances
    }

    /// Geometría de todas las instancias en espacio mundo. Omite las primitivas
    /// sin posiciones y los triángulos con índices fuera de rango.
    pub fn world_primitives(&self) -> Vec<WorldPrimitive> {
        let mut out = Vec::new();
        for (instance_idx, instance) in self.mesh_instances().into_iter().enumerate() {
            let world = instance.world;
            let identity = world == Mat4::IDENTITY;
            let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
            let flip = world.determinant() < 0.0;

            for (prim_idx, prim) in self.meshes[instance.mesh].primitives.iter().enumerate() {
                let mut positions = None;
                let mut normals = None;
                let mut uvs = None;
                for attr in &prim.attributes {
                    match attr {
                        VertexAttribute::Positions(p) => positions = Some(p),
                        VertexAttribute::Normals(n) => normals = Some(n),
                        VertexAttribute::TexCoords(0, uv) => uvs = Some(uv),
                        _ => {}
                    }
                }
                let Some(positions) = positions else { continue };
                let count = positions.len();

                let positions: Vec<[f32; 3]> = if identity {
                    positions.clone()
                } else {
                    positions
                        .iter()
                        .map(|p| world.transform_point3(Vec3::from(*p)).to_array())
                        .collect()
                };
                let normals = normals.filter(|n| n.len() == count).map(|n| {
                    if identity {
                        n.clone()
                    } else {
                        n.iter()
                            .map(|v| (normal_matrix * Vec3::from(*v)).normalize_or_zero().to_array())
                            .collect()
                    }
                });
                let uvs = uvs.filter(|uv| uv.len() == count).cloned();

                let indices: Vec<u32> = match &prim.indices {
                    Some(IndexData::U16(idx)) => idx.iter().map(|&i| i as u32).collect(),
                    Some(IndexData::U32(idx)) => idx.clone(),
                    None => (0..count as u32).collect(),
                };
                let triangles = indices
                    .chunks_exact(3)
                    .filter(|t| t.iter().all(|&i| (i as usize) < count))
                    .map(|t| if flip { [t[0], t[2], t[1]] } else { [t[0], t[1], t[2]] })
                    .collect();

                out.push(WorldPrimitive {
                    instance: instance_idx,
                    mesh: instance.mesh,
                    primitive: prim_idx,
                    node: instance.node,
                    positions,
                    normals,
                    uvs,
                    triangles,
                    material: prim.material,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::{Mesh, Node, Primitive, Transform};
    use super::*;
    use glam::Quat;

    fn triangle_mesh() -> Mesh {
        Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
                    VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; 3]),
                ],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        }
    }

    fn node(transform: Transform, mesh: Option<usize>, children: Vec<usize>) -> Node {
        Node { name: String::new(), transform, mesh, skin: None, children }
    }

    fn trs(t: [f32; 3], s: f32) -> Transform {
        Transform::Trs { translation: Vec3::from(t), rotation: Quat::IDENTITY, scale: Vec3::splat(s) }
    }

    #[test]
    fn no_nodes_means_identity() {
        let scene = Scene { meshes: vec![triangle_mesh()], ..Scene::new() };
        let prims = scene.world_primitives();
        assert_eq!(prims.len(), 1);
        assert_eq!(prims[0].positions[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn nested_transforms_and_instancing() {
        let mut scene = Scene::new();
        scene.meshes.push(triangle_mesh());
        // raíz escala 2 → hijo trasladado (10,0,0) con la malla; otro nodo instancia la misma malla
        scene.nodes.push(node(trs([0.0, 0.0, 0.0], 2.0), None, vec![1]));
        scene.nodes.push(node(trs([10.0, 0.0, 0.0], 1.0), Some(0), vec![]));
        scene.nodes.push(node(Transform::identity(), Some(0), vec![]));
        scene.root_nodes = vec![0, 2];

        let prims = scene.world_primitives();
        assert_eq!(prims.len(), 2);
        assert_eq!(prims[0].positions[1], [22.0, 0.0, 0.0]);
        assert_eq!(prims[1].positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(scene.compute_bounding_box().unwrap().1, [22.0, 2.0, 0.0]);
    }

    #[test]
    fn mirror_flips_winding_and_normals() {
        let mut scene = Scene::new();
        scene.meshes.push(triangle_mesh());
        let mirror = Transform::Trs {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, -1.0),
        };
        scene.nodes.push(node(mirror, Some(0), vec![]));
        scene.root_nodes = vec![0];

        let p = &scene.world_primitives()[0];
        assert_eq!(p.triangles[0], [0, 2, 1]);
        assert_eq!(p.normals.as_ref().unwrap()[0], [0.0, 0.0, -1.0]);
    }

    #[test]
    fn skinned_mesh_ignores_node_transform() {
        let mut scene = Scene::new();
        scene.meshes.push(triangle_mesh());
        let mut n = node(trs([5.0, 0.0, 0.0], 0.01), Some(0), vec![]);
        n.skin = Some(0);
        scene.nodes.push(n);
        scene.root_nodes = vec![0];
        assert_eq!(scene.world_primitives()[0].positions[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn invalid_indices_and_cycles_are_ignored() {
        let mut scene = Scene::new();
        let mut mesh = triangle_mesh();
        mesh.primitives[0].indices = Some(IndexData::U32(vec![0, 1, 2, 0, 1, 99]));
        scene.meshes.push(mesh);
        // ciclo 0 → 1 → 0
        scene.nodes.push(node(Transform::identity(), Some(0), vec![1]));
        scene.nodes.push(node(Transform::identity(), None, vec![0]));
        scene.root_nodes = vec![0];

        let prims = scene.world_primitives();
        assert_eq!(prims.len(), 1);
        assert_eq!(prims[0].triangles.len(), 1);
    }
}
