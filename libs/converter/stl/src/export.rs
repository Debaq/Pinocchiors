use converter_scene::Scene;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StlExportError {
    #[error("error escribiendo STL: {0}")]
    Io(#[from] std::io::Error),
    #[error("la escena no contiene geometría")]
    NoGeometry,
}

/// Exporta una `Scene` a formato STL binario, en milímetros.
///
/// Fusiona todas las primitivas de todos los meshes en un solo conjunto
/// de triángulos. Solo exporta geometría — materiales, texturas,
/// esqueletos y animaciones se descartan. Como STL no guarda unidades y los
/// slicers asumen milímetros, las coordenadas se convierten según
/// `scene.meters_per_unit` (una escena glTF en metros sale ×1000).
pub fn export_stl(scene: &Scene, path: impl AsRef<Path>) -> Result<(), StlExportError> {
    let triangles = extract_triangles(scene)?;

    let mut file = std::fs::File::create(path)?;
    stl_io::write_stl(&mut file, triangles.iter())?;

    Ok(())
}

/// Extrae todos los triángulos de la escena como `stl_io::Triangle`, en espacio
/// mundo (aplicando las transformaciones de los nodos).
///
/// La normal de cada faceta es la de la cara según el orden de sus vértices,
/// como exige el formato STL.
fn extract_triangles(scene: &Scene) -> Result<Vec<stl_io::Triangle>, StlExportError> {
    let mut triangles = Vec::new();

    let to_mm = (scene.meters_per_unit * 1000.0) as f32;
    for prim in scene.world_primitives() {
        for tri in &prim.triangles {
            let [v0, v1, v2] = tri.map(|i| prim.positions[i as usize].map(|c| c * to_mm));
            triangles.push(stl_io::Triangle {
                normal: stl_io::Normal::new(compute_face_normal(v0, v1, v2)),
                vertices: [
                    stl_io::Vertex::new(v0),
                    stl_io::Vertex::new(v1),
                    stl_io::Vertex::new(v2),
                ],
            });
        }
    }

    if triangles.is_empty() {
        return Err(StlExportError::NoGeometry);
    }

    Ok(triangles)
}

/// Calcula la normal de una cara a partir de 3 vértices (producto cruzado).
fn compute_face_normal(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> [f32; 3] {
    let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 1e-10 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::glam::{Quat, Vec3};
    use converter_scene::{IndexData, Mesh, Node, Primitive, Transform, VertexAttribute};

    #[test]
    fn applies_node_transforms() {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "n".into(),
            transform: Transform::Trs {
                translation: Vec3::new(10.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(0.5),
            },
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        scene.meters_per_unit = 0.001; // milímetros: sin conversión

        let tris = extract_triangles(&scene).unwrap();
        assert_eq!(tris.len(), 1);
        let v1 = tris[0].vertices[1];
        assert_eq!([v1[0], v1[1], v1[2]], [10.5, 0.0, 0.0]);
        let n = tris[0].normal;
        assert_eq!([n[0], n[1], n[2]], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn converts_meters_to_millimeters() {
        let mut scene = Scene::new(); // glTF: metros
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1.7, 0.0, 0.0], [0.0, 1.0, 0.0]])],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        let tris = extract_triangles(&scene).unwrap();
        assert!((tris[0].vertices[1][0] - 1700.0).abs() < 1e-3);
    }
}
