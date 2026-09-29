//! PLY (Stanford Polygon File Format): importación ([`import_ply`], ver el
//! módulo `import`) y exportación binaria little-endian.
//!
//! Al exportar, todas las primitivas se fusionan en espacio mundo. Guarda posiciones y,
//! cuando todas las primitivas los tienen, normales, UV (`s`, `t`) y colores
//! de vértice RGBA de 8 bits. Las coordenadas quedan en las unidades de la
//! escena: PLY no declara unidades. Sale con Z arriba, como lo leen Blender
//! y los programas de escaneo.

mod import;

pub use import::{import_ply, import_ply_bytes, PlyImportError};

use converter_scene::{Scene, VertexAttribute};
use std::io::Write;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlyExportError {
    #[error("error escribiendo PLY: {0}")]
    Io(#[from] std::io::Error),
    #[error("la escena no contiene geometría")]
    NoGeometry,
}

/// Exporta una `Scene` a un archivo PLY binario.
pub fn export_ply(scene: &Scene, path: impl AsRef<Path>) -> Result<(), PlyExportError> {
    let bytes = export_ply_bytes(scene)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Exporta una `Scene` a PLY binario en memoria.
pub fn export_ply_bytes(scene: &Scene) -> Result<Vec<u8>, PlyExportError> {
    let prims = scene.world_primitives_z_up();
    if prims.iter().all(|p| p.triangles.is_empty()) {
        return Err(PlyExportError::NoGeometry);
    }
    // Colores de vértice de la primitiva original (world_primitives no los trae)
    let colors: Vec<Option<&Vec<[f32; 4]>>> = prims
        .iter()
        .map(|p| {
            scene.meshes[p.mesh].primitives[p.primitive].attributes.iter().find_map(|a| match a {
                VertexAttribute::Colors(c) if c.len() == p.positions.len() => Some(c),
                _ => None,
            })
        })
        .collect();

    let has_normals = prims.iter().all(|p| p.normals.is_some());
    let has_uvs = prims.iter().all(|p| p.uvs.is_some());
    let has_colors = colors.iter().all(Option::is_some);
    let vertex_count: usize = prims.iter().map(|p| p.positions.len()).sum();
    let face_count: usize = prims.iter().map(|p| p.triangles.len()).sum();

    let mut out = Vec::new();
    writeln!(out, "ply")?;
    writeln!(out, "format binary_little_endian 1.0")?;
    writeln!(out, "comment Pinocchiors")?;
    writeln!(out, "element vertex {vertex_count}")?;
    for axis in ["x", "y", "z"] {
        writeln!(out, "property float {axis}")?;
    }
    if has_normals {
        for axis in ["nx", "ny", "nz"] {
            writeln!(out, "property float {axis}")?;
        }
    }
    if has_uvs {
        writeln!(out, "property float s")?;
        writeln!(out, "property float t")?;
    }
    if has_colors {
        for channel in ["red", "green", "blue", "alpha"] {
            writeln!(out, "property uchar {channel}")?;
        }
    }
    writeln!(out, "element face {face_count}")?;
    writeln!(out, "property list uchar uint vertex_indices")?;
    writeln!(out, "end_header")?;

    for (prim, colors) in prims.iter().zip(&colors) {
        for i in 0..prim.positions.len() {
            put_f32s(&mut out, &prim.positions[i]);
            if has_normals && let Some(n) = &prim.normals {
                put_f32s(&mut out, &n[i]);
            }
            if has_uvs && let Some(uv) = &prim.uvs {
                // PLY usa el origen de textura abajo a la izquierda, glTF arriba
                put_f32s(&mut out, &[uv[i][0], 1.0 - uv[i][1]]);
            }
            if has_colors && let Some(c) = colors {
                out.extend(c[i].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
        }
    }
    let mut offset = 0u32;
    for prim in &prims {
        for tri in &prim.triangles {
            out.push(3);
            for &i in tri {
                out.extend_from_slice(&(offset + i).to_le_bytes());
            }
        }
        offset += prim.positions.len() as u32;
    }
    Ok(out)
}

fn put_f32s(out: &mut Vec<u8>, values: &[f32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{IndexData, Mesh, Node, Primitive, Transform};

    fn scene(attributes: Vec<VertexAttribute>) -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive { attributes, indices: Some(IndexData::U16(vec![0, 1, 2])), material: None }],
        });
        scene.nodes.push(Node {
            name: "tri".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        scene
    }

    fn header(bytes: &[u8]) -> String {
        let end = bytes.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
        String::from_utf8(bytes[..end].to_vec()).unwrap()
    }

    #[test]
    fn writes_positions_and_faces() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let bytes = export_ply_bytes(&scene(vec![VertexAttribute::Positions(positions)])).unwrap();
        let head = header(&bytes);
        assert!(head.contains("element vertex 3\n"));
        assert!(head.contains("element face 1\n"));
        assert!(!head.contains("nx"));
        // 3 vértices × 12 bytes + 1 cara × (1 + 3×4)
        assert_eq!(bytes.len() - head.len(), 36 + 13);
        let face = &bytes[bytes.len() - 13..];
        assert_eq!(face[0], 3);
        assert_eq!(u32::from_le_bytes(face[9..13].try_into().unwrap()), 2);
    }

    #[test]
    fn writes_normals_uvs_and_colors() {
        let bytes = export_ply_bytes(&scene(vec![
            VertexAttribute::Positions(vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; 3]),
            VertexAttribute::TexCoords(0, vec![[0.0, 0.0]; 3]),
            VertexAttribute::Colors(vec![[1.0, 0.5, 0.0, 1.0]; 3]),
        ]))
        .unwrap();
        let head = header(&bytes);
        assert!(head.contains("property float nx"));
        assert!(head.contains("property float s"));
        assert!(head.contains("property uchar alpha"));
        // Primer vértice: 3 + 3 + 2 floats y luego RGBA
        let color = &bytes[head.len() + 32..head.len() + 36];
        assert_eq!(color, [255, 128, 0, 255]);
    }

    #[test]
    fn empty_scene_is_an_error() {
        assert!(matches!(export_ply_bytes(&Scene::new()), Err(PlyExportError::NoGeometry)));
    }
}
