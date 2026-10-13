//! Texturas decodificadas con muestreo bilineal y repetición.

use glam::{Vec2, Vec4};
use std::sync::OnceLock;

fn srgb_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut l = [0.0; 256];
        for (i, v) in l.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        l
    })
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<[u8; 4]>,
}

impl Texture {
    /// Decodifica PNG/JPEG/WebP
    pub fn decode(bytes: &[u8]) -> Option<Texture> {
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return None;
        }
        let rgba = img.pixels().map(|p| p.0).collect();
        Some(Texture { width, height, rgba })
    }

    /// Una sola celda de color sólido (para pruebas)
    pub fn solid(rgba: [u8; 4]) -> Texture {
        Texture { width: 1, height: 1, rgba: vec![rgba] }
    }

    /// Luminancia lineal media (la textura se toma como sRGB)
    pub fn mean_luminance(&self) -> f32 {
        let l = srgb_lut();
        let sum: f64 = self
            .rgba
            .iter()
            .map(|p| (0.2126 * l[p[0] as usize] + 0.7152 * l[p[1] as usize] + 0.0722 * l[p[2] as usize]) as f64)
            .sum();
        (sum / self.rgba.len().max(1) as f64) as f32
    }

    #[inline]
    fn texel(&self, x: i64, y: i64, srgb: bool) -> Vec4 {
        let x = x.rem_euclid(self.width as i64) as usize;
        let y = y.rem_euclid(self.height as i64) as usize;
        let p = self.rgba[y * self.width as usize + x];
        if srgb {
            let l = srgb_lut();
            Vec4::new(l[p[0] as usize], l[p[1] as usize], l[p[2] as usize], p[3] as f32 / 255.0)
        } else {
            Vec4::new(p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32) / 255.0
        }
    }

    /// Muestreo bilineal con repetición. `uv` en convención glTF (v hacia abajo).
    /// Con `srgb` los canales de color se pasan a lineal antes de mezclar.
    pub fn sample(&self, uv: Vec2, srgb: bool) -> Vec4 {
        let x = uv.x * self.width as f32 - 0.5;
        let y = uv.y * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.texel(x0, y0, srgb).lerp(self.texel(x0 + 1, y0, srgb), fx);
        let b = self.texel(x0, y0 + 1, srgb).lerp(self.texel(x0 + 1, y0 + 1, srgb), fx);
        a.lerp(b, fy)
    }
}
