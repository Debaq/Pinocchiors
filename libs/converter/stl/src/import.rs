use converter_scene::{
    z_up_to_y_up, IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute,
};
use std::io::{Read, Seek};
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
/// un único mesh sin materiales, texturas, esqueleto ni animaciones. Se lee
/// con Z arriba (como lo escriben Blender y los slicers) y se pasa a Y arriba.
pub fn import_stl(path: impl AsRef<Path>) -> Result<Scene, StlImportError> {
    let path = path.as_ref();
    let mut file = std::fs::File::open(path)?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("stl_mesh");
    read_scene(&mut file, name)
}

/// Importa un STL (ASCII o binario) desde bytes en memoria (p. ej. en WASM).
pub fn import_stl_bytes(data: &[u8]) -> Result<Scene, StlImportError> {
    read_scene(&mut std::io::Cursor::new(data), "stl_mesh")
}

fn read_scene<R: Read + Seek>(reader: &mut R, name: &str) -> Result<Scene, StlImportError> {
    let stl_mesh = stl_io::read_stl(reader)?;

    if stl_mesh.faces.is_empty() {
        return Err(StlImportError::Empty);
    }

    // STL viene con Z arriba (Blender, CAD, slicers); la escena es Y arriba
    let positions: Vec<[f32; 3]> = stl_mesh
        .vertices
        .iter()
        .map(|v| z_up_to_y_up([v[0], v[1], v[2]]))
        .collect();

    // Convertir índices: stl_io usa usize, nosotros u32
    let indices: Vec<u32> = stl_mesh
        .faces
        .iter()
        .flat_map(|face| {
            face.vertices.iter().map(|&i| i as u32)
        })
        .collect();

    // Normales por vértice: promedio de las caras vecinas, pesado por área. La
    // normal escrita en el archivo no se usa (muchos programas escriben 0 0 0
    // o una que no coincide con el orden de los vértices): como los
    // laminadores, manda el orden de los vértices.
    let mut vertex_normals = vec![[0.0f32; 3]; positions.len()];
    for face in &stl_mesh.faces {
        let [a, b, c] = face.vertices.map(|i| positions[i]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
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
        name: name.to_string(),
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
        let tris = [stl_io::Triangle {
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
            }];
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

    #[test]
    fn bytes_roundtrip_matches_file_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tri.stl");
        let mut scene = import_stl_bytes(ASCII_TRIANGLE.as_bytes()).unwrap();
        scene.meters_per_unit = 0.001;

        let bytes = crate::export_stl_bytes(&scene).unwrap();
        export_stl(&scene, &path).unwrap();
        assert_eq!(bytes, std::fs::read(&path).unwrap());

        let back = import_stl_bytes(&bytes).unwrap();
        assert_eq!(back.compute_bounding_box(), scene.compute_bounding_box());
        assert_eq!(back.meters_per_unit, 0.001);
    }

    #[test]
    fn imports_ascii_from_bytes() {
        let scene = import_stl_bytes(ASCII_TRIANGLE.as_bytes()).unwrap();
        assert!(scene.validate().is_ok());
        let (min, max) = scene.compute_bounding_box().unwrap();
        // El triángulo está en el plano XY del STL (el piso, con Z arriba):
        // en la escena (Y arriba) queda en XZ, con Y del STL hacia −Z
        assert_eq!(min, [0.0, 0.0, -3.0]);
        assert_eq!(max, [2.0, 0.0, 0.0]);
    }

    #[test]
    fn rejects_garbage_and_empty_bytes() {
        assert!(import_stl_bytes(b"").is_err());
        assert!(import_stl_bytes(b"not an stl file at all").is_err());
        // Binario válido sin triángulos: cabecera de 80 bytes + contador 0
        assert!(matches!(import_stl_bytes(&[0u8; 84]), Err(StlImportError::Empty)));
    }

    #[test]
    fn normals_follow_winding_not_the_file() {
        // Normal escrita en cero (permitido; muchos programas lo hacen)
        let scene = import_stl_bytes(ASCII_TRIANGLE.replace("facet normal 0 0 1", "facet normal 0 0 0").as_bytes()).unwrap();
        let normals = scene.meshes[0].primitives[0].attributes.iter().find_map(|a| match a {
            VertexAttribute::Normals(n) => Some(n.clone()),
            _ => None,
        });
        // Antihorario visto desde +Z del STL = +Y de la escena
        for n in normals.unwrap() {
            assert!((n[1] - 1.0).abs() < 1e-6, "{n:?}");
        }
    }

    const ASCII_TRIANGLE: &str = "solid t
facet normal 0 0 1
  outer loop
    vertex 0 0 0
    vertex 2 0 0
    vertex 0 3 0
  endloop
endfacet
endsolid t
";
}
