use converter_scene::{IndexData, Scene, VertexAttribute};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StlExportError {
    #[error("error escribiendo STL: {0}")]
    Io(#[from] std::io::Error),
    #[error("la escena no contiene geometría")]
    NoGeometry,
}

/// Exporta una `Scene` a formato STL binario.
///
/// Fusiona todas las primitivas de todos los meshes en un solo conjunto
/// de triángulos. Solo exporta geometría — materiales, texturas,
/// esqueletos y animaciones se descartan.
pub fn export_stl(scene: &Scene, path: impl AsRef<Path>) -> Result<(), StlExportError> {
    let triangles = extract_triangles(scene)?;

    let mut file = std::fs::File::create(path)?;
    stl_io::write_stl(&mut file, triangles.iter())?;

    Ok(())
}

/// Extrae todos los triángulos de la escena como `stl_io::Triangle`.
fn extract_triangles(scene: &Scene) -> Result<Vec<stl_io::Triangle>, StlExportError> {
    let mut triangles = Vec::new();

    for mesh in &scene.meshes {
        for prim in &mesh.primitives {
            // Buscar posiciones y normales
            let positions = prim.attributes.iter().find_map(|a| {
                if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
            });
            let normals = prim.attributes.iter().find_map(|a| {
                if let VertexAttribute::Normals(n) = a { Some(n) } else { None }
            });

            let positions = match positions {
                Some(p) => p,
                None => continue,
            };

            // Obtener lista de índices de triángulos
            let tri_indices = match &prim.indices {
                Some(IndexData::U16(idx)) => {
                    idx.iter().map(|&i| i as usize).collect::<Vec<_>>()
                }
                Some(IndexData::U32(idx)) => {
                    idx.iter().map(|&i| i as usize).collect::<Vec<_>>()
                }
                // Sin índices: cada 3 vértices forman un triángulo
                None => (0..positions.len()).collect(),
            };

            // Iterar triángulos (cada 3 índices)
            for chunk in tri_indices.chunks_exact(3) {
                let (i0, i1, i2) = (chunk[0], chunk[1], chunk[2]);

                let v0 = positions[i0];
                let v1 = positions[i1];
                let v2 = positions[i2];

                // Usar normal del primer vértice si existe, sino calcular de la geometría
                let normal = if let Some(normals) = normals {
                    normals[i0]
                } else {
                    compute_face_normal(v0, v1, v2)
                };

                triangles.push(stl_io::Triangle {
                    normal: stl_io::Normal::new(normal),
                    vertices: [
                        stl_io::Vertex::new(v0),
                        stl_io::Vertex::new(v1),
                        stl_io::Vertex::new(v2),
                    ],
                });
            }
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
