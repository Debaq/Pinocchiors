use glam::{Mat3, Vec3};
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

/// Cómo pasar la radiancia lineal a la imagen final
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ToneMap {
    /// Recorta a [0, 1] (lo que se ve es lo que se calculó)
    #[default]
    Linear,
    /// ACES filmic como el de three.js (`ACESFilmicToneMapping`), para que el motor y el
    /// visor den el mismo tono; `exposure` multiplica antes de la curva
    Aces { exposure: f32 },
}

impl ToneMap {
    pub fn apply(self, c: Vec3) -> Vec3 {
        match self {
            ToneMap::Linear => c,
            ToneMap::Aces { exposure } => {
                // Mismas matrices y ajuste (Narkowicz/Hill) que three.js
                let input = Mat3::from_cols(
                    Vec3::new(0.59719, 0.07600, 0.02840),
                    Vec3::new(0.35458, 0.90834, 0.13383),
                    Vec3::new(0.04823, 0.01566, 0.83777),
                );
                let output = Mat3::from_cols(
                    Vec3::new(1.60475, -0.10208, -0.00327),
                    Vec3::new(-0.53108, 1.10813, -0.07276),
                    Vec3::new(-0.07367, -0.00605, 1.07602),
                );
                let v = input * (c.max(Vec3::ZERO) * (exposure / 0.6));
                let a = v * (v + Vec3::splat(0.024_578_6)) - Vec3::splat(0.000_090_537);
                let b = v * (v * 0.983_729 + Vec3::splat(0.432_951)) + Vec3::splat(0.238_081);
                (output * (a / b)).clamp(Vec3::ZERO, Vec3::ONE)
            }
        }
    }
}

impl Image {
    pub fn to_srgb8(&self) -> Vec<u8> {
        self.to_srgb8_with(ToneMap::Linear)
    }

    pub fn to_srgb8_with(&self, tone: ToneMap) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for p in &self.pixels {
            let p = tone.apply(*p);
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
