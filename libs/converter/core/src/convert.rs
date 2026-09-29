use std::path::Path;
use thiserror::Error;

use converter_scene::Scene;

use crate::format::Format;
use crate::options::ConvertOptions;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("formato de entrada no reconocido: {0}")]
    UnknownInputFormat(String),
    #[error("formato de salida no reconocido: {0}")]
    UnknownOutputFormat(String),
    #[error("importación no soportada para {0}")]
    ImportNotSupported(String),
    #[error("exportación no soportada para {0}")]
    ExportNotSupported(String),
    #[error("error glTF: {0}")]
    Gltf(#[from] converter_gltf_io::GltfImportError),
    #[error("error GLB export: {0}")]
    GlbExport(#[from] converter_gltf_io::GlbExportError),
    #[error("error STL: {0}")]
    StlImport(#[from] converter_stl::StlImportError),
    #[error("error STL export: {0}")]
    StlExport(#[from] converter_stl::StlExportError),
    #[error("error OBJ: {0}")]
    ObjImport(#[from] converter_obj::ObjImportError),
    #[error("error OBJ export: {0}")]
    ObjExport(#[from] converter_obj::ObjExportError),
    #[error("error PLY: {0}")]
    PlyImport(#[from] converter_ply::PlyImportError),
    #[error("error PLY export: {0}")]
    PlyExport(#[from] converter_ply::PlyExportError),
    #[error("error 3MF export: {0}")]
    ThreeMfExport(#[from] converter_3mf::ThreeMfExportError),
    #[error("error USDA: {0}")]
    Usda(#[from] converter_usda::UsdaWriteError),
    #[error("error USDZ: {0}")]
    Usdz(#[from] converter_usda::UsdzPackageError),
    #[error("error I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Convierte un archivo 3D de un formato a otro.
///
/// Detecta formatos automáticamente por extensión.
pub fn convert(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: &ConvertOptions,
) -> Result<(), ConvertError> {
    let scene = import(input_path)?;
    export(&scene, output_path, options)?;
    Ok(())
}

/// Importa un archivo 3D y retorna una Scene.
pub fn import(path: impl AsRef<Path>) -> Result<Scene, ConvertError> {
    let path = path.as_ref();
    let format = Format::from_extension(path).ok_or_else(|| {
        ConvertError::UnknownInputFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string(),
        )
    })?;

    if !format.can_import() {
        return Err(ConvertError::ImportNotSupported(format.name().to_string()));
    }

    match format {
        Format::Gltf => Ok(converter_gltf_io::import_gltf(path)?),
        Format::Stl => Ok(converter_stl::import_stl(path)?),
        Format::Obj => Ok(converter_obj::import_obj(path)?),
        Format::Ply => Ok(converter_ply::import_ply(path)?),
        _ => Err(ConvertError::ImportNotSupported(format.name().to_string())),
    }
}

/// Exporta una Scene a un archivo 3D.
pub fn export(
    scene: &Scene,
    path: impl AsRef<Path>,
    options: &ConvertOptions,
) -> Result<(), ConvertError> {
    let path = path.as_ref();
    let format = Format::from_extension(path).ok_or_else(|| {
        ConvertError::UnknownOutputFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string(),
        )
    })?;

    if !format.can_export() {
        return Err(ConvertError::ExportNotSupported(format.name().to_string()));
    }

    let usda_opts = options.to_usda_options();
    let glb_opts = options.to_glb_options();

    match format {
        Format::Gltf => {
            let separate = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gltf"));
            if separate {
                converter_gltf_io::export_gltf(scene, path, &glb_opts)?;
            } else {
                converter_gltf_io::export_glb(scene, path, &glb_opts)?;
            }
        }
        Format::Usda => {
            let output = converter_usda::write_usda(scene, &usda_opts)?;
            std::fs::write(path, output.usda)?;
        }
        Format::Usdz => {
            converter_usda::write_usdz(scene, &usda_opts, path)?;
        }
        Format::Stl => {
            converter_stl::export_stl(scene, path)?;
        }
        Format::Obj => {
            converter_obj::export_obj(scene, path)?;
        }
        Format::Ply => converter_ply::export_ply(scene, path)?,
        Format::ThreeMf => converter_3mf::export_3mf(scene, path)?,
    }

    Ok(())
}

/// Importa una escena desde bytes en memoria.
///
/// Soporta: GLB (`Format::Gltf`), STL (`Format::Stl`) y PLY (`Format::Ply`).
pub fn import_bytes(data: &[u8], format: Format) -> Result<Scene, ConvertError> {
    if !format.can_import_bytes() {
        return Err(ConvertError::ImportNotSupported(format.name().to_string()));
    }

    match format {
        Format::Gltf => Ok(converter_gltf_io::import_gltf_bytes(data)?),
        Format::Stl => Ok(converter_stl::import_stl_bytes(data)?),
        Format::Ply => Ok(converter_ply::import_ply_bytes(data)?),
        _ => Err(ConvertError::ImportNotSupported(format.name().to_string())),
    }
}

/// Exporta una escena a bytes en memoria.
///
/// Soporta: GLB (`Format::Gltf`), USDA, USDZ, STL, PLY y 3MF. OBJ no, porque sus
/// materiales y texturas van en archivos aparte.
pub fn export_bytes(
    scene: &Scene,
    format: Format,
    options: &ConvertOptions,
) -> Result<Vec<u8>, ConvertError> {
    if !format.can_export_bytes() {
        return Err(ConvertError::ExportNotSupported(format.name().to_string()));
    }

    match format {
        Format::Gltf => Ok(converter_gltf_io::export_glb_bytes(scene, &options.to_glb_options())?),
        Format::Usda => {
            Ok(converter_usda::write_usda(scene, &options.to_usda_options())?.usda.into_bytes())
        }
        Format::Usdz => Ok(converter_usda::write_usdz_bytes(scene, &options.to_usda_options())?),
        Format::Stl => Ok(converter_stl::export_stl_bytes(scene)?),
        Format::Ply => Ok(converter_ply::export_ply_bytes(scene)?),
        Format::ThreeMf => Ok(converter_3mf::export_3mf_bytes(scene)?),
        _ => Err(ConvertError::ExportNotSupported(format.name().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::*;

    fn triangle_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "Triangle".into(),
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
            name: "Root".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn export_import_glb_roundtrip() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.glb");

        export(&scene, &path, &ConvertOptions::default()).unwrap();
        let imported = import(&path).unwrap();
        assert_eq!(imported.meshes.len(), 1);
    }

    #[test]
    fn export_usda() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.usda");

        export(&scene, &path, &ConvertOptions::default()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("#usda 1.0"));
    }

    #[test]
    fn export_usdz() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.usdz");

        export(&scene, &path, &ConvertOptions::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn export_stl() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stl");

        export(&scene, &path, &ConvertOptions::default()).unwrap();
        let imported = import(&path).unwrap();
        assert_eq!(imported.meshes.len(), 1);
    }

    #[test]
    fn convert_glb_to_usdz() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let glb_path = dir.path().join("input.glb");
        let usdz_path = dir.path().join("output.usdz");

        converter_gltf_io::export_glb(&scene, &glb_path, &converter_gltf_io::GlbExportOptions::default()).unwrap();
        convert(&glb_path, &usdz_path, &ConvertOptions::default()).unwrap();
        assert!(usdz_path.exists());
    }

    #[test]
    fn format_detection_error() {
        let result = import("unknown.xyz");
        assert!(result.is_err());
    }

    #[test]
    fn import_not_supported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.usda");
        std::fs::write(&path, "#usda 1.0").unwrap();

        let result = import(&path);
        assert!(matches!(result, Err(ConvertError::ImportNotSupported(_))));
    }

    #[test]
    fn export_import_glb_bytes_roundtrip() {
        let scene = triangle_scene();
        let opts = ConvertOptions::default();

        let bytes = export_bytes(&scene, Format::Gltf, &opts).unwrap();
        assert!(!bytes.is_empty());

        let imported = import_bytes(&bytes, Format::Gltf).unwrap();
        assert_eq!(imported.meshes.len(), 1);
    }

    #[test]
    fn export_usdz_bytes() {
        let scene = triangle_scene();
        let opts = ConvertOptions::default();

        let bytes = export_bytes(&scene, Format::Usdz, &opts).unwrap();
        assert!(!bytes.is_empty());
        // USDZ es ZIP: empieza con PK magic bytes
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn import_bytes_not_supported() {
        let result = import_bytes(b"data", Format::Obj);
        assert!(matches!(result, Err(ConvertError::ImportNotSupported(_))));
    }

    #[test]
    fn export_bytes_not_supported() {
        let scene = triangle_scene();
        let result = export_bytes(&scene, Format::Obj, &ConvertOptions::default());
        assert!(matches!(result, Err(ConvertError::ExportNotSupported(_))));
    }

    #[test]
    fn stl_bytes_roundtrip() {
        let mut scene = triangle_scene();
        scene.meters_per_unit = 0.001;
        let bytes = export_bytes(&scene, Format::Stl, &ConvertOptions::default()).unwrap();
        let imported = import_bytes(&bytes, Format::Stl).unwrap();
        assert_eq!(imported.compute_bounding_box(), scene.compute_bounding_box());
    }

    #[test]
    fn export_usda_bytes() {
        let bytes = export_bytes(&triangle_scene(), Format::Usda, &ConvertOptions::default()).unwrap();
        assert!(String::from_utf8(bytes).unwrap().starts_with("#usda 1.0"));
    }

    #[test]
    fn gltf_extension_writes_separate_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("modelo.gltf");
        export(&triangle_scene(), &path, &ConvertOptions::default()).unwrap();
        let json = std::fs::read(&path).unwrap();
        assert_eq!(json[0], b'{', "debe ser JSON, no GLB");
        assert!(dir.path().join("modelo.bin").exists());
        assert!(import(&path).is_ok());
    }

    #[test]
    fn draco_option_reaches_glb() {
        let opts = ConvertOptions { draco: Some(crate::DracoOptions::default()), ..Default::default() };
        let glb = export_bytes(&triangle_scene(), Format::Gltf, &opts).unwrap();
        let text = String::from_utf8_lossy(&glb);
        assert!(text.contains("KHR_draco_mesh_compression"));
        assert!(import_bytes(&glb, Format::Gltf).is_ok());
    }

    #[test]
    fn byte_capabilities_match_implementation() {
        let scene = triangle_scene();
        let opts = ConvertOptions::default();
        let glb = export_bytes(&scene, Format::Gltf, &opts).unwrap();
        for &format in Format::all() {
            assert_eq!(
                export_bytes(&scene, format, &opts).is_ok(),
                format.can_export_bytes(),
                "export_bytes {format:?}"
            );
            let input = match format {
                Format::Stl | Format::Ply => export_bytes(&scene, format, &opts).unwrap(),
                _ => glb.clone(),
            };
            let imported = import_bytes(&input, format);
            assert_eq!(imported.is_ok(), format.can_import_bytes(), "import_bytes {format:?}");
        }
    }
}
