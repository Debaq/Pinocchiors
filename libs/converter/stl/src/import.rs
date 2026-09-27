use converter_scene::{
    IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute,
};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StlImportError {
    #[error("error leyendo STL: {0}")]
    Io(#[from] std::io::Error),
    #[error("STL vacío: no contiene triángulos")]
    Empty,
}

/// Importa un archivo STL (ASCII o binario) y lo convierte a `Scene`.
///
/// STL solo contiene triángulos y normales — la escena resultante tendrá
/// un único mesh sin materiales, texturas, esqueleto ni animaciones.
pub fn import_stl(path: impl AsRef<Path>) -> Result<Scene, StlImportError> {
    let mut file = std::fs::OpenOptions::new().read(true).open(path.as_ref())?;
    let stl_mesh = stl_io::read_stl(&mut file)?;

    if stl_mesh.faces.is_empty() {
        return Err(StlImportError::Empty);
    }

    // Convertir vértices: stl_io::Vertex → [f32; 3]
    let positions: Vec<[f32; 3]> = stl_mesh
        .vertices
        .iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();

    // Convertir índices: stl_io usa usize, nosotros u32
    let indices: Vec<u32> = stl_mesh
        .faces
        .iter()
        .flat_map(|face| {
            face.vertices.iter().map(|&i| i as u32)
        })
        .collect();

    // Calcular normales por vértice promediando las normales de las caras adyacentes.
    // STL provee normales por cara, no por vértice.
    let mut vertex_normals = vec![[0.0f32; 3]; positions.len()];
    for face in &stl_mesh.faces {
        let n = [face.normal[0], face.normal[1], face.normal[2]];
        for &vi in &face.vertices {
            vertex_normals[vi][0] += n[0];
            vertex_normals[vi][1] += n[1];
            vertex_normals[vi][2] += n[2];
        }
    }
    // Normalizar
    for n in &mut vertex_normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-10 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        }
    }

    let primitive = Primitive {
        attributes: vec![
            VertexAttribute::Positions(positions),
            VertexAttribute::Normals(vertex_normals),
        ],
        indices: Some(IndexData::U32(indices)),
        material: None,
    };

    let mesh = Mesh {
        name: path
            .as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("stl_mesh")
            .to_string(),
        primitives: vec![primitive],
    };

    let node = Node {
        name: mesh.name.clone(),
        transform: Transform::identity(),
        mesh: Some(0),
        skin: None,
        children: Vec::new(),
    };

    let mut scene = Scene::new();
    scene.meshes.push(mesh);
    scene.nodes.push(node);
    scene.root_nodes.push(0);
    // STL no guarda unidades; la convención (slicers, CAD) es milímetros
    scene.meters_per_unit = 0.001;

    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_stl;

    /// Crea un STL binario mínimo con un triángulo y lo importa.
    #[test]
    fn import_single_triangle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tri.stl");

        // Escribir STL binario mínimo con stl_io
        let tri = stl_io::Triangle {
            normal: stl_io::Normal::new([0.0, 0.0, 1.0]),
            vertices: [
                stl_io::Vertex::new([0.0, 0.0, 0.0]),
                stl_io::Vertex::new([1.0, 0.0, 0.0]),
                stl_io::Vertex::new([0.0, 1.0, 0.0]),
            ],
        };
        let mut file = std::fs::File::create(&path).unwrap();
        stl_io::write_stl(&mut file, [tri].iter()).unwrap();

        let scene = import_stl(&path).unwrap();
        assert_eq!(scene.meshes.len(), 1);
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.root_nodes, vec![0]);

        let prim = &scene.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();

        assert_eq!(positions.len(), 3);
        assert!(scene.validate().is_ok());
    }

    /// Roundtrip: STL → Scene → STL → Scene, comparar geometría.
    #[test]
    fn roundtrip_stl() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.stl");
        let exported = dir.path().join("exported.stl");

        // Crear STL con 2 triángulos (un cuadrado)
        let tris = vec![
            stl_io::Triangle {
                normal: stl_io::Normal::new([0.0, 0.0, 1.0]),
                vertices: [
                    stl_io::Vertex::new([0.0, 0.0, 0.0]),
                    stl_io::Vertex::new([1.0, 0.0, 0.0]),
                    stl_io::Vertex::new([1.0, 1.0, 0.0]),
                ],
            },
            stl_io::Triangle {
                normal: stl_io::Normal::new([0.0, 0.0, 1.0]),
                vertices: [
                    stl_io::Vertex::new([0.0, 0.0, 0.0]),
                    stl_io::Vertex::new([1.0, 1.0, 0.0]),
                    stl_io::Vertex::new([0.0, 1.0, 0.0]),
                ],
            },
        ];
        let mut file = std::fs::File::create(&original).unwrap();
        stl_io::write_stl(&mut file, tris.iter()).unwrap();

        // Import → Scene
        let scene = import_stl(&original).unwrap();
        assert!(scene.validate().is_ok());

        // Scene → Export
        export_stl(&scene, &exported).unwrap();

        // Re-import y comparar
        let scene2 = import_stl(&exported).unwrap();
        assert!(scene2.validate().is_ok());

        // Comparar bounding boxes
        let bb1 = scene.compute_bounding_box().unwrap();
        let bb2 = scene2.compute_bounding_box().unwrap();

        for i in 0..3 {
            assert!((bb1.0[i] - bb2.0[i]).abs() < 1e-5, "min[{i}] difiere");
            assert!((bb1.1[i] - bb2.1[i]).abs() < 1e-5, "max[{i}] difiere");
        }

        // Comparar cantidad de triángulos (indices / 3)
        let count = |s: &Scene| -> usize {
            s.meshes.iter()
                .flat_map(|m| &m.primitives)
                .filter_map(|p| p.indices.as_ref())
                .map(|idx| match idx {
                    IndexData::U16(v) => v.len() / 3,
                    IndexData::U32(v) => v.len() / 3,
                })
                .sum()
        };
        assert_eq!(count(&scene), count(&scene2));
    }
}
