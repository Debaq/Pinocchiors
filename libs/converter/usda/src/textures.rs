use std::collections::HashMap;

use converter_scene::{Scene, TextureFormat};

use crate::materials::texture_asset_path;
use crate::writer::{ProcessedTexture, UsdaExportOptions, UsdaWriteError};

/// Resultado del procesamiento de texturas.
pub(crate) struct TextureProcessingResult {
    /// Texturas procesadas listas para empaquetar.
    pub outputs: Vec<ProcessedTexture>,
    /// Mapa texture_index → asset_path directo.
    pub asset_paths: HashMap<usize, String>,
    /// Mapa texture_index → paths de canales ORM separados.
    pub orm_splits: HashMap<usize, OrmSplit>,
}

/// Paths de las texturas ORM separadas por canal.
pub(crate) struct OrmSplit {
    pub metallic_path: String,
    pub roughness_path: String,
    pub occlusion_path: String,
}

/// Procesa todas las texturas de la escena según las opciones configuradas.
pub(crate) fn process_textures(
    scene: &Scene,
    options: &UsdaExportOptions,
) -> Result<TextureProcessingResult, UsdaWriteError> {
    let mut outputs = Vec::new();
    let mut asset_paths = HashMap::new();
    let mut orm_splits = HashMap::new();

    // Recopilar qué texturas se usan como metallic_roughness
    let mr_texture_indices: std::collections::HashSet<usize> = if options.split_orm_channels {
        scene
            .materials
            .iter()
            .filter_map(|mat| mat.metallic_roughness_texture.as_ref())
            .map(|tr| tr.texture_index)
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    for (idx, tex) in scene.textures.iter().enumerate() {
        let needs_convert = tex.format == TextureFormat::WebP;
        let needs_resize = options
            .max_texture_size
            .is_some_and(|max| tex.width > max || tex.height > max);
        let needs_split = options.split_orm_channels && mr_texture_indices.contains(&idx);

        if needs_split {
            // Decodificar la textura
            let img = image::load_from_memory(&tex.data).map_err(|e| {
                UsdaWriteError::TextureDecode(format!("textura {}: {}", idx, e))
            })?;

            let img = if needs_resize {
                resize_image(&img, options.max_texture_size.unwrap())
            } else {
                img
            };

            let base_name = if tex.name.is_empty() {
                format!("texture_{}", idx)
            } else {
                crate::writer::sanitize_name(&tex.name, "texture", idx)
            };

            // Extraer canales: R=occlusion, G=roughness, B=metallic
            let rgba = img.to_rgba8();

            let (w, h) = (rgba.width(), rgba.height());

            let occlusion_img = image::GrayImage::from_fn(w, h, |x, y| {
                image::Luma([rgba.get_pixel(x, y).0[0]])
            });
            let roughness_img = image::GrayImage::from_fn(w, h, |x, y| {
                image::Luma([rgba.get_pixel(x, y).0[1]])
            });
            let metallic_img = image::GrayImage::from_fn(w, h, |x, y| {
                image::Luma([rgba.get_pixel(x, y).0[2]])
            });

            let occlusion_path = format!("textures/{}_occlusion.png", base_name);
            let roughness_path = format!("textures/{}_roughness.png", base_name);
            let metallic_path = format!("textures/{}_metallic.png", base_name);

            outputs.push(ProcessedTexture {
                asset_path: occlusion_path.clone(),
                data: encode_png_gray(&occlusion_img, idx)?,
            });
            outputs.push(ProcessedTexture {
                asset_path: roughness_path.clone(),
                data: encode_png_gray(&roughness_img, idx)?,
            });
            outputs.push(ProcessedTexture {
                asset_path: metallic_path.clone(),
                data: encode_png_gray(&metallic_img, idx)?,
            });

            orm_splits.insert(
                idx,
                OrmSplit {
                    metallic_path,
                    roughness_path,
                    occlusion_path,
                },
            );
        } else if needs_convert || needs_resize {
            // Decodificar y re-codificar
            let img = image::load_from_memory(&tex.data).map_err(|e| {
                UsdaWriteError::TextureDecode(format!("textura {}: {}", idx, e))
            })?;

            let img = if needs_resize {
                resize_image(&img, options.max_texture_size.unwrap())
            } else {
                img
            };

            let data = encode_png_rgba(&img, idx)?;
            let path = if needs_convert {
                // WebP → PNG: forzar extensión .png
                let base_name = if tex.name.is_empty() {
                    format!("texture_{}", idx)
                } else {
                    crate::writer::sanitize_name(&tex.name, "texture", idx)
                };
                format!("textures/{}.png", base_name)
            } else {
                texture_asset_path(tex, idx)
            };

            asset_paths.insert(idx, path.clone());
            outputs.push(ProcessedTexture {
                asset_path: path,
                data,
            });
        } else {
            // Copiar bytes originales sin procesar
            let path = texture_asset_path(tex, idx);
            asset_paths.insert(idx, path.clone());
            outputs.push(ProcessedTexture {
                asset_path: path,
                data: tex.data.clone(),
            });
        }
    }

    Ok(TextureProcessingResult {
        outputs,
        asset_paths,
        orm_splits,
    })
}

fn resize_image(img: &image::DynamicImage, max_size: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_size && h <= max_size {
        return img.clone();
    }
    let scale = max_size as f64 / w.max(h) as f64;
    let new_w = ((w as f64 * scale).round() as u32).max(1);
    let new_h = ((h as f64 * scale).round() as u32).max(1);
    img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
}

fn encode_png_rgba(img: &image::DynamicImage, idx: usize) -> Result<Vec<u8>, UsdaWriteError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| UsdaWriteError::TextureEncode(format!("textura {}: {}", idx, e)))?;
    Ok(buf.into_inner())
}

fn encode_png_gray(img: &image::GrayImage, idx: usize) -> Result<Vec<u8>, UsdaWriteError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(img.clone())
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| UsdaWriteError::TextureEncode(format!("textura {}: {}", idx, e)))?;
    Ok(buf.into_inner())
}
