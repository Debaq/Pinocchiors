//! I/O para mallas (OBJ, GLB/GLTF, PLY, STL)

use crate::Mesh;
use pinocchio_math::Vector3;
use std::path::Path;
use thiserror::Error;

/// Error al cargar una malla
#[derive(Debug, Error)]
pub enum MeshLoadError {
    #[error("Error de I/O: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Error al parsear OBJ: {0}")]
    ObjParseError(String),
    #[error("Error al parsear GLTF: {0}")]
    GltfParseError(String),
    #[error("Formato no soportado: {0}")]
    UnsupportedFormat(String),
    #[error("Malla vacía o inválida")]
    EmptyMesh,
}

/// Carga una malla desde un archivo OBJ
pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<Mesh, MeshLoadError> {
    let path = path.as_ref();

    let (models, _materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )
    .map_err(|e| MeshLoadError::ObjParseError(e.to_string()))?;

    if models.is_empty() {
        return Err(MeshLoadError::EmptyMesh);
    }

    // Combinar todos los modelos en una malla
    let mut all_positions = Vec::new();
    let mut all_indices = Vec::new();
    let mut vertex_offset = 0;

    for model in &models {
        let mesh = &model.mesh;

        // Extraer posiciones
        for chunk in mesh.positions.chunks(3) {
            if chunk.len() == 3 {
                all_positions.push(Vector3::new(
                    chunk[0] as f64,
                    chunk[1] as f64,
                    chunk[2] as f64,
                ));
            }
        }

        // Extraer índices (ya triangulados)
        for chunk in mesh.indices.chunks(3) {
            if chunk.len() == 3 {
                all_indices.push([
                    chunk[0] as usize + vertex_offset,
                    chunk[1] as usize + vertex_offset,
                    chunk[2] as usize + vertex_offset,
                ]);
            }
        }

        vertex_offset = all_positions.len();
    }

    if all_positions.is_empty() || all_indices.is_empty() {
        return Err(MeshLoadError::EmptyMesh);
    }

    Ok(Mesh::from_triangles(&all_positions, &all_indices))
}

/// Carga una malla desde un archivo GLB/GLTF
///
/// Cuando el feature `converter` está activo, usa `converter-gltf-io` internamente.
/// De lo contrario, usa una implementación directa con el crate `gltf`.
#[cfg(feature = "converter")]
pub fn load_glb<P: AsRef<Path>>(path: P) -> Result<Mesh, MeshLoadError> {
    crate::adapter::load_glb_via_converter(path)
}

/// Carga una malla desde un archivo GLB/GLTF
#[cfg(not(feature = "converter"))]
pub fn load_glb<P: AsRef<Path>>(path: P) -> Result<Mesh, MeshLoadError> {
    let (document, buffers, _images) = gltf::import(path)
        .map_err(|e| MeshLoadError::GltfParseError(e.to_string()))?;

    let mut all_positions = Vec::new();
    let mut all_indices = Vec::new();
    let mut vertex_offset = 0;

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            // Leer posiciones
            if let Some(positions) = reader.read_positions() {
                for pos in positions {
                    all_positions.push(Vector3::new(
                        pos[0] as f64,
                        pos[1] as f64,
                        pos[2] as f64,
                    ));
                }
            }

            // Leer índices
            if let Some(indices) = reader.read_indices() {
                let indices: Vec<u32> = indices.into_u32().collect();
                for chunk in indices.chunks(3) {
                    if chunk.len() == 3 {
                        all_indices.push([
                            chunk[0] as usize + vertex_offset,
                            chunk[1] as usize + vertex_offset,
                            chunk[2] as usize + vertex_offset,
                        ]);
                    }
                }
            }

            vertex_offset = all_positions.len();
        }
    }

    if all_positions.is_empty() || all_indices.is_empty() {
        return Err(MeshLoadError::EmptyMesh);
    }

    Ok(Mesh::from_triangles(&all_positions, &all_indices))
}

/// Guarda una malla en formato OBJ
#[allow(dead_code)]
pub fn save_obj<P: AsRef<Path>>(mesh: &Mesh, path: P) -> Result<(), MeshLoadError> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(path)?;

    // Escribir vértices
    for v in &mesh.vertices {
        writeln!(file, "v {} {} {}", v.position.x(), v.position.y(), v.position.z())?;
    }

    // Escribir normales
    for v in &mesh.vertices {
        writeln!(file, "vn {} {} {}", v.normal.x(), v.normal.y(), v.normal.z())?;
    }

    // Escribir caras (OBJ usa índices base-1)
    for i in 0..mesh.num_faces() {
        let verts = mesh.get_face_vertices(i);
        writeln!(
            file,
            "f {}//{} {}//{} {}//{}",
            verts[0] + 1, verts[0] + 1,
            verts[1] + 1, verts[1] + 1,
            verts[2] + 1, verts[2] + 1,
        )?;
    }

    Ok(())
}

/// Detecta el formato de archivo por extensión
pub fn detect_format<P: AsRef<Path>>(path: P) -> Option<&'static str> {
    let ext = path.as_ref().extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "obj" => Some("obj"),
        "glb" => Some("glb"),
        "gltf" => Some("gltf"),
        "ply" => Some("ply"),
        "stl" => Some("stl"),
        "off" => Some("off"),
        _ => None,
    }
}

/// Carga una malla desde un archivo STL.
///
/// Solo disponible cuando el feature `converter` está activo.
#[cfg(feature = "converter")]
pub fn load_stl<P: AsRef<Path>>(path: P) -> Result<Mesh, MeshLoadError> {
    crate::adapter::load_stl_via_converter(path)
}

/// Carga una malla detectando el formato automáticamente.
///
/// Formatos soportados:
/// - OBJ (siempre)
/// - GLB/glTF (siempre)
/// - STL (solo con feature `converter`)
pub fn load_mesh<P: AsRef<Path>>(path: P) -> Result<Mesh, MeshLoadError> {
    let path = path.as_ref();
    let format = detect_format(path)
        .ok_or_else(|| MeshLoadError::UnsupportedFormat("Extensión desconocida".into()))?;

    match format {
        "obj" => load_obj(path),
        "glb" | "gltf" => load_glb(path),
        #[cfg(feature = "converter")]
        "stl" => load_stl(path),
        _ => Err(MeshLoadError::UnsupportedFormat(format.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_simple_obj() {
        let obj_content = r#"
v 0 0 0
v 1 0 0
v 0 1 0
v 0 0 1
f 1 2 3
f 1 4 2
f 2 4 3
f 3 4 1
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(obj_content.as_bytes()).unwrap();

        let mesh = load_obj(file.path()).unwrap();
        assert_eq!(mesh.num_vertices(), 4);
        assert_eq!(mesh.num_faces(), 4);
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(detect_format("model.obj"), Some("obj"));
        assert_eq!(detect_format("model.OBJ"), Some("obj"));
        assert_eq!(detect_format("model.ply"), Some("ply"));
        assert_eq!(detect_format("model.xyz"), None);
    }
}
