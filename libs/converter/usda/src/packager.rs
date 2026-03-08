use converter_scene::Scene;
use std::io::{Cursor, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use crate::writer::UsdaExportOptions;

#[derive(Debug, Error)]
pub enum UsdzPackageError {
    #[error("error generando USDA: {0}")]
    Usda(#[from] crate::writer::UsdaWriteError),
    #[error("error creando ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("error de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("tipo de archivo no permitido en USDZ: {0}")]
    InvalidFileType(String),
}

/// Alineamiento requerido por la especificación USDZ: 64 bytes.
const USDZ_ALIGNMENT: u16 = 64;

/// Extensiones permitidas dentro de un paquete USDZ.
const ALLOWED_EXTENSIONS: &[&str] = &[
    "usda", "usdc", "usdz", "png", "jpeg", "jpg", "m4a", "mp3", "wav",
];

/// Empaqueta una `Scene` como archivo USDZ en disco.
///
/// USDZ = ZIP sin compresión, alineado a 64 bytes, conteniendo:
/// - `scene.usda` — la escena principal
/// - texturas PNG/JPEG referenciadas
pub fn write_usdz(
    scene: &Scene,
    options: &UsdaExportOptions,
    path: impl AsRef<Path>,
) -> Result<(), UsdzPackageError> {
    let bytes = write_usdz_bytes(scene, options)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Empaqueta una `Scene` como USDZ en memoria.
///
/// Útil para WASM y pipelines que no necesitan escribir a disco.
pub fn write_usdz_bytes(
    scene: &Scene,
    options: &UsdaExportOptions,
) -> Result<Vec<u8>, UsdzPackageError> {
    // Ajustar opciones para ARKit si es necesario
    let effective_options = if options.arkit_compatible {
        let max_tex = match options.max_texture_size {
            Some(size) => Some(size.min(2048)),
            None => Some(2048),
        };
        UsdaExportOptions {
            scale_factor: options.scale_factor,
            max_texture_size: max_tex,
            split_orm_channels: options.split_orm_channels,
            arkit_compatible: true,
            fps: options.fps,
            export_animations: options.export_animations,
            keyframe_tolerance: options.keyframe_tolerance,
        }
    } else {
        UsdaExportOptions {
            scale_factor: options.scale_factor,
            max_texture_size: options.max_texture_size,
            split_orm_channels: options.split_orm_channels,
            arkit_compatible: options.arkit_compatible,
            fps: options.fps,
            export_animations: options.export_animations,
            keyframe_tolerance: options.keyframe_tolerance,
        }
    };

    let output = crate::write_usda(scene, &effective_options)?;

    let buf = Cursor::new(Vec::with_capacity(64 * 1024));
    let mut zip = ZipWriter::new(buf);

    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .with_alignment(USDZ_ALIGNMENT);

    // Primera entrada: scene.usda (requerido como primer archivo)
    zip.start_file("scene.usda", file_options)?;
    zip.write_all(output.usda.as_bytes())?;

    // Texturas procesadas
    for tex in &output.textures {
        validate_extension(&tex.asset_path)?;
        zip.start_file(&tex.asset_path, file_options)?;
        zip.write_all(&tex.data)?;
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Valida que la extensión del archivo es permitida en USDZ.
fn validate_extension(path: &str) -> Result<(), UsdzPackageError> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if ALLOWED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
        Ok(())
    } else {
        Err(UsdzPackageError::InvalidFileType(path.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::*;
    use zip::ZipArchive;

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
            name: "Root_Node".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    fn scene_with_texture() -> Scene {
        let mut scene = triangle_scene();
        // PNG mínimo válido (1x1 pixel rojo)
        let png_data = minimal_png();
        scene.textures.push(Texture {
            name: "baseColor".into(),
            data: png_data,
            format: TextureFormat::Png,
            width: 1,
            height: 1,
        });
        scene.materials.push(Material {
            name: "Mat".into(),
            base_color_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);
        scene
    }

    /// Genera un PNG mínimo válido de 1×1 pixel.
    fn minimal_png() -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        // Signature
        buf.write_all(&[137, 80, 78, 71, 13, 10, 26, 10]).unwrap();
        // IHDR
        let ihdr_data = [
            0, 0, 0, 1, // width
            0, 0, 0, 1, // height
            8,  // bit depth
            2,  // color type (RGB)
            0,  // compression
            0,  // filter
            0,  // interlace
        ];
        let ihdr_crc = crc32(&[b'I', b'H', b'D', b'R'], &ihdr_data);
        buf.write_all(&(ihdr_data.len() as u32).to_be_bytes()).unwrap();
        buf.write_all(b"IHDR").unwrap();
        buf.write_all(&ihdr_data).unwrap();
        buf.write_all(&ihdr_crc.to_be_bytes()).unwrap();
        // IDAT (scanline filter 0 + 3 bytes RGB, deflate-stored)
        let raw_scanline: &[u8] = &[0, 255, 0, 0]; // filter=None, R=255 G=0 B=0
        let idat_payload = deflate_stored(raw_scanline);
        let idat_crc = crc32(b"IDAT", &idat_payload);
        buf.write_all(&(idat_payload.len() as u32).to_be_bytes()).unwrap();
        buf.write_all(b"IDAT").unwrap();
        buf.write_all(&idat_payload).unwrap();
        buf.write_all(&idat_crc.to_be_bytes()).unwrap();
        // IEND
        let iend_crc = crc32(b"IEND", &[]);
        buf.write_all(&0u32.to_be_bytes()).unwrap();
        buf.write_all(b"IEND").unwrap();
        buf.write_all(&iend_crc.to_be_bytes()).unwrap();
        buf
    }

    /// Deflate stored block (sin compresión).
    fn deflate_stored(data: &[u8]) -> Vec<u8> {
        let len = data.len() as u16;
        let nlen = !len;
        let mut out = Vec::new();
        // zlib header
        out.push(0x78);
        out.push(0x01);
        // stored block: BFINAL=1, BTYPE=00
        out.push(0x01);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(data);
        // Adler32
        let adler = adler32(data);
        out.extend_from_slice(&adler.to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in chunk_type.iter().chain(data.iter()) {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xFFFF_FFFF
    }

    #[test]
    fn usdz_basic_package() {
        let scene = triangle_scene();
        let opts = UsdaExportOptions::default();
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();

        let cursor = Cursor::new(bytes);
        let archive = ZipArchive::new(cursor).unwrap();

        // Sin texturas: solo scene.usda
        assert_eq!(archive.len(), 1);
        assert_eq!(archive.name_for_index(0).unwrap(), "scene.usda");
    }

    #[test]
    fn usdz_no_compression() {
        let scene = triangle_scene();
        let opts = UsdaExportOptions::default();
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();

        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();

        for i in 0..archive.len() {
            let file = archive.by_index(i).unwrap();
            assert_eq!(
                file.compression(),
                CompressionMethod::Stored,
                "archivo '{}' usa compresión",
                file.name()
            );
        }
    }

    #[test]
    fn usdz_alignment_64_bytes() {
        let scene = scene_with_texture();
        let opts = UsdaExportOptions::default();
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();

        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();

        for i in 0..archive.len() {
            let file = archive.by_index(i).unwrap();
            let offset = file.data_start();
            assert_eq!(
                offset % 64,
                0,
                "archivo '{}' no alineado a 64 bytes (offset: {})",
                file.name(),
                offset
            );
        }
    }

    #[test]
    fn usdz_with_textures() {
        let scene = scene_with_texture();
        let opts = UsdaExportOptions::default();
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();

        let cursor = Cursor::new(bytes);
        let archive = ZipArchive::new(cursor).unwrap();

        // scene.usda + al menos 1 textura
        assert!(archive.len() >= 2, "esperaba al menos 2 entradas, got {}", archive.len());

        // Primera entrada siempre es scene.usda
        assert_eq!(archive.name_for_index(0).unwrap(), "scene.usda");

        // Segunda entrada debería ser una textura
        let tex_name = archive.name_for_index(1).unwrap();
        assert!(
            tex_name.ends_with(".png") || tex_name.ends_with(".jpeg") || tex_name.ends_with(".jpg"),
            "textura inesperada: {}",
            tex_name
        );
    }

    #[test]
    fn usdz_scene_usda_content() {
        let scene = triangle_scene();
        let opts = UsdaExportOptions::default();
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();

        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();
        let mut file = archive.by_name("scene.usda").unwrap();

        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content).unwrap();
        assert!(content.starts_with("#usda 1.0"));
        assert!(content.contains("def Mesh \"Triangle\""));
    }

    #[test]
    fn usdz_write_to_file() {
        let scene = triangle_scene();
        let opts = UsdaExportOptions::default();
        let dir = std::env::temp_dir().join("converter_usdz_test");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test_basic.usdz");

        write_usdz(&scene, &opts, &path).unwrap();
        assert!(path.exists());

        let data = std::fs::read(&path).unwrap();
        assert!(!data.is_empty());

        // Verificar que es un ZIP válido
        let cursor = Cursor::new(data);
        let archive = ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn usdz_arkit_limits_texture_size() {
        let scene = scene_with_texture();

        // Sin ARKit: sin límite explícito
        let opts_normal = UsdaExportOptions::default();
        let bytes_normal = write_usdz_bytes(&scene, &opts_normal).unwrap();

        // Con ARKit: max 2048
        let opts_arkit = UsdaExportOptions {
            arkit_compatible: true,
            ..UsdaExportOptions::default()
        };
        let bytes_arkit = write_usdz_bytes(&scene, &opts_arkit).unwrap();

        // Ambos deben producir paquetes válidos
        let archive_normal = ZipArchive::new(Cursor::new(bytes_normal)).unwrap();
        let archive_arkit = ZipArchive::new(Cursor::new(bytes_arkit)).unwrap();
        assert!(archive_normal.len() >= 1);
        assert!(archive_arkit.len() >= 1);
    }

    #[test]
    fn usdz_arkit_respects_smaller_max() {
        // Si el usuario pone max_texture_size=1024, ARKit no lo sube a 2048
        let scene = scene_with_texture();
        let opts = UsdaExportOptions {
            max_texture_size: Some(1024),
            arkit_compatible: true,
            ..UsdaExportOptions::default()
        };
        // No debe fallar — 1024 < 2048, se respeta el más pequeño
        let bytes = write_usdz_bytes(&scene, &opts).unwrap();
        let archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(archive.len() >= 1);
    }

    #[test]
    fn usdz_validate_extension_allowed() {
        assert!(validate_extension("textures/color.png").is_ok());
        assert!(validate_extension("textures/normal.jpeg").is_ok());
        assert!(validate_extension("textures/bump.jpg").is_ok());
        assert!(validate_extension("scene.usda").is_ok());
        assert!(validate_extension("audio.m4a").is_ok());
        assert!(validate_extension("audio.mp3").is_ok());
        assert!(validate_extension("audio.wav").is_ok());
    }

    #[test]
    fn usdz_validate_extension_rejected() {
        assert!(validate_extension("model.fbx").is_err());
        assert!(validate_extension("texture.webp").is_err());
        assert!(validate_extension("data.bin").is_err());
        assert!(validate_extension("script.py").is_err());
    }
}
