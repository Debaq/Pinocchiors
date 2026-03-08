use crate::export::GlbExportError;
use crate::options::GlbExportOptions;
use converter_scene::{Texture, TextureFormat};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

/// Procesa una textura según las opciones de exportación.
///
/// Retorna `(bytes, mime_type)`. Si no hay nada que hacer, devuelve los datos originales.
pub(crate) fn process_texture(
    tex: &Texture,
    options: &GlbExportOptions,
) -> Result<(Vec<u8>, &'static str), GlbExportError> {
    let needs_resize = options
        .max_texture_size
        .is_some_and(|max| tex.width > max || tex.height > max);
    let needs_recompress = options.texture_quality.is_some();
    let is_webp = tex.format == TextureFormat::WebP;

    if !needs_resize && !needs_recompress && !is_webp {
        let mime = format_to_mime(tex.format);
        return Ok((tex.data.clone(), mime));
    }

    // Decodificar imagen
    let mut img = decode_texture(tex)?;

    // Resize si excede max_texture_size
    if let Some(max_size) = options.max_texture_size {
        if tex.width > max_size || tex.height > max_size {
            let (new_w, new_h) = fit_dimensions(img.width(), img.height(), max_size);
            img = img.resize_exact(new_w, new_h, FilterType::Lanczos3);
        }
    }

    // Decidir formato de salida
    let has_alpha = image_has_alpha(&img);

    if let Some(quality) = options.texture_quality {
        if !has_alpha {
            // Recomprimir a JPEG (sin alpha)
            let bytes = encode_jpeg(&img, quality)?;
            return Ok((bytes, "image/jpeg"));
        }
    }

    // Mantener PNG (tiene alpha o no se pidió recompresión)
    let bytes = encode_png(&img)?;
    Ok((bytes, "image/png"))
}

fn decode_texture(tex: &Texture) -> Result<DynamicImage, GlbExportError> {
    let format = match tex.format {
        TextureFormat::Png => ImageFormat::Png,
        TextureFormat::Jpeg => ImageFormat::Jpeg,
        TextureFormat::WebP => ImageFormat::WebP,
    };
    image::load(Cursor::new(&tex.data), format)
        .map_err(|e| GlbExportError::TextureProcess(e.to_string()))
}

fn image_has_alpha(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageRgba8(rgba) => rgba.pixels().any(|p| p.0[3] < 255),
        DynamicImage::ImageRgba16(rgba) => rgba.pixels().any(|p| p.0[3] < 65535),
        DynamicImage::ImageRgba32F(rgba) => rgba.pixels().any(|p| p.0[3] < 1.0),
        DynamicImage::ImageLumaA8(la) => la.pixels().any(|p| p.0[1] < 255),
        DynamicImage::ImageLumaA16(la) => la.pixels().any(|p| p.0[1] < 65535),
        _ => false, // RGB, Luma → sin alpha
    }
}

fn fit_dimensions(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w >= h {
        let new_w = max;
        let new_h = (h as f64 * max as f64 / w as f64).round() as u32;
        (new_w, new_h.max(1))
    } else {
        let new_h = max;
        let new_w = (w as f64 * max as f64 / h as f64).round() as u32;
        (new_w.max(1), new_h)
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, GlbExportError> {
    let mut buf = Cursor::new(Vec::new());
    let rgb = img.to_rgb8();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)
        .map_err(|e| GlbExportError::TextureProcess(e.to_string()))?;
    Ok(buf.into_inner())
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>, GlbExportError> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| GlbExportError::TextureProcess(e.to_string()))?;
    Ok(buf.into_inner())
}

fn format_to_mime(format: TextureFormat) -> &'static str {
    match format {
        TextureFormat::Png => "image/png",
        TextureFormat::Jpeg => "image/jpeg",
        TextureFormat::WebP => "image/webp",
    }
}
