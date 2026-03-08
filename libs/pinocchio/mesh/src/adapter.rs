//! Adaptador para convertir `converter_scene::Scene` a `Mesh`.
//!
//! Este módulo solo está disponible cuando el feature `converter` está activo.

use crate::Mesh;
use converter_scene::{Scene, VertexAttribute};
use pinocchio_math::Vector3;

/// Convierte una `Scene` a `Mesh`, extrayendo toda la geometría.
///
/// Combina todos los meshes y primitivas de la escena en una sola malla.
/// Solo extrae posiciones e índices (la estructura half-edge no soporta
/// materiales, texturas, esqueletos ni animaciones).
///
/// # Errores
///
/// Retorna `None` si la escena no contiene geometría válida.
pub fn scene_to_mesh(scene: &Scene) -> Option<Mesh> {
    let mut all_positions: Vec<Vector3> = Vec::new();
    let mut all_indices: Vec<[usize; 3]> = Vec::new();
    let mut vertex_offset = 0;

    for mesh in &scene.meshes {
        for prim in &mesh.primitives {
            // Extraer posiciones
            let positions = prim.attributes.iter().find_map(|attr| {
                if let VertexAttribute::Positions(p) = attr {
                    Some(p)
                } else {
                    None
                }
            })?;

            for p in positions {
                all_positions.push(Vector3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            }

            // Extraer índices
            if let Some(ref idx) = prim.indices {
                let indices: Vec<u32> = match idx {
                    converter_scene::IndexData::U16(v) => v.iter().map(|&i| i as u32).collect(),
                    converter_scene::IndexData::U32(v) => v.clone(),
                };

                for chunk in indices.chunks(3) {
                    if chunk.len() == 3 {
                        all_indices.push([
                            chunk[0] as usize + vertex_offset,
                            chunk[1] as usize + vertex_offset,
                            chunk[2] as usize + vertex_offset,
                        ]);
                    }
                }
            } else {
                // Sin índices: triángulos implícitos
                for i in (0..positions.len()).step_by(3) {
                    if i + 2 < positions.len() {
                        all_indices.push([
                            i + vertex_offset,
                            i + 1 + vertex_offset,
                            i + 2 + vertex_offset,
                        ]);
                    }
                }
            }

            vertex_offset = all_positions.len();
        }
    }

    if all_positions.is_empty() || all_indices.is_empty() {
        return None;
    }

    Some(Mesh::from_triangles(&all_positions, &all_indices))
}

/// Carga un archivo GLB/glTF usando converter y lo convierte a `Mesh`.
///
/// Esta función reemplaza la implementación directa de `load_glb` cuando
/// el feature `converter` está activo.
pub fn load_glb_via_converter<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<Mesh, crate::io::MeshLoadError> {
    let scene = converter_gltf_io::import_gltf(path)
        .map_err(|e| crate::io::MeshLoadError::GltfParseError(e.to_string()))?;

    scene_to_mesh(&scene).ok_or(crate::io::MeshLoadError::EmptyMesh)
}

/// Carga un archivo STL usando converter y lo convierte a `Mesh`.
pub fn load_stl_via_converter<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<Mesh, crate::io::MeshLoadError> {
    let scene = converter_stl::import_stl(path)
        .map_err(|e| crate::io::MeshLoadError::ObjParseError(e.to_string()))?;

    scene_to_mesh(&scene).ok_or(crate::io::MeshLoadError::EmptyMesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Transform};

    fn triangle_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(SceneMesh {
            name: "triangle".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
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
    fn convert_simple_triangle() {
        let scene = triangle_scene();
        let mesh = scene_to_mesh(&scene).unwrap();

        assert_eq!(mesh.num_vertices(), 3);
        assert_eq!(mesh.num_faces(), 1);
    }

    #[test]
    fn convert_empty_scene() {
        let scene = Scene::new();
        assert!(scene_to_mesh(&scene).is_none());
    }

    #[test]
    fn convert_multiple_primitives() {
        let mut scene = Scene::new();

        // Primer mesh con 1 triángulo
        scene.meshes.push(SceneMesh {
            name: "mesh1".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });

        // Segundo mesh con 1 triángulo
        scene.meshes.push(SceneMesh {
            name: "mesh2".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [2.0, 0.0, 0.0],
                    [3.0, 0.0, 0.0],
                    [2.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });

        let mesh = scene_to_mesh(&scene).unwrap();
        assert_eq!(mesh.num_vertices(), 6);
        assert_eq!(mesh.num_faces(), 2);
    }
}
