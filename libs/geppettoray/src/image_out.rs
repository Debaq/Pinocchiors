use glam::Vec3;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("no se pudo guardar la imagen: {0}")]
    Image(#[from] image::ImageError),
}

/// Imagen en RGB lineal de punto flotante
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Vec3>,
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Image {
    pub fn to_srgb8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for p in &self.pixels {
            for c in [p.x, p.y, p.z] {
                out.push((linear_to_srgb(c) * 255.0 + 0.5) as u8);
            }
        }
        out
    }

    pub fn save_png(&self, path: &Path) -> Result<(), SaveError> {
        image::save_buffer(path, &self.to_srgb8(), self.width, self.height, image::ExtendedColorType::Rgb8)?;
        Ok(())
    }
}
