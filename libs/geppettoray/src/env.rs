//! Entorno: color uniforme o mapa equirectangular (HDRI) con muestreo por importancia.
//!
//! Convención (Y arriba): u = atan2(x, -z)/2π + ½ + yaw/2π, v = acos(y)/π, con v = 0 arriba.

use crate::rng::Rng;
use glam::Vec3;
use std::f32::consts::PI;
use std::sync::Arc;

fn lum(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// Mapa equirectangular con distribución de importancia (luminancia × sen θ)
#[derive(Debug)]
pub struct EnvMap {
    width: usize,
    height: usize,
    /// RGB lineal, ya multiplicado por la intensidad
    pixels: Vec<Vec3>,
    yaw: f32,
    /// CDF marginal por filas (height + 1 entradas) y condicional por columnas (height × (width + 1))
    marginal: Vec<f32>,
    conditional: Vec<f32>,
    /// Probabilidad de cada fila y de cada texel dentro de su fila
    row_prob: Vec<f32>,
    cond_prob: Vec<f32>,
}

impl EnvMap {
    pub fn from_rgb(width: usize, height: usize, mut pixels: Vec<Vec3>, intensity: f32, yaw: f32) -> Option<EnvMap> {
        if width == 0 || height == 0 || pixels.len() != width * height {
            return None;
        }
        for p in &mut pixels {
            *p *= intensity;
        }
        let mut conditional = vec![0.0; height * (width + 1)];
        let mut cond_prob = vec![0.0; width * height];
        let mut row_sum = vec![0.0f32; height];
        for y in 0..height {
            let sin_t = (PI * (y as f32 + 0.5) / height as f32).sin();
            let base = y * (width + 1);
            for x in 0..width {
                let w = lum(pixels[y * width + x]).max(0.0) * sin_t;
                conditional[base + x + 1] = conditional[base + x] + w;
            }
            row_sum[y] = conditional[base + width];
            for x in 0..width {
                if row_sum[y] > 0.0 {
                    cond_prob[y * width + x] = (conditional[base + x + 1] - conditional[base + x]) / row_sum[y];
                    conditional[base + x] /= row_sum[y];
                } else {
                    cond_prob[y * width + x] = 1.0 / width as f32;
                    conditional[base + x] = x as f32 / width as f32;
                }
            }
            conditional[base + width] = 1.0;
        }
        let total: f32 = row_sum.iter().sum();
        let mut marginal = vec![0.0; height + 1];
        let mut row_prob = vec![0.0; height];
        for y in 0..height {
            row_prob[y] = if total > 0.0 { row_sum[y] / total } else { 1.0 / height as f32 };
            marginal[y + 1] = marginal[y] + row_prob[y];
        }
        marginal[height] = 1.0;
        Some(EnvMap { width, height, pixels, yaw, marginal, conditional, row_prob, cond_prob })
    }

    /// Lee un HDR de Radiance (.hdr) u otro formato que `image` entienda
    pub fn from_bytes(bytes: &[u8], intensity: f32, yaw: f32) -> Option<EnvMap> {
        let img = image::load_from_memory(bytes).ok()?.to_rgb32f();
        let (w, h) = img.dimensions();
        let px = img.pixels().map(|p| Vec3::new(p.0[0], p.0[1], p.0[2])).collect();
        EnvMap::from_rgb(w as usize, h as usize, px, intensity, yaw)
    }

    fn dir_to_uv(&self, d: Vec3) -> (f32, f32) {
        let phi = d.x.atan2(-d.z) + self.yaw;
        let u = (phi / (2.0 * PI) + 0.5).rem_euclid(1.0);
        (u, d.y.clamp(-1.0, 1.0).acos() / PI)
    }

    fn uv_to_dir(&self, u: f32, v: f32) -> Vec3 {
        let phi = (u - 0.5) * 2.0 * PI - self.yaw;
        let theta = v * PI;
        let s = theta.sin();
        Vec3::new(s * phi.sin(), theta.cos(), -s * phi.cos())
    }

    fn texel(&self, x: i64, y: i64) -> Vec3 {
        let x = x.rem_euclid(self.width as i64) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        self.pixels[y * self.width + x]
    }

    /// Radiancia hacia la dirección (bilineal)
    pub fn eval(&self, d: Vec3) -> Vec3 {
        let (u, v) = self.dir_to_uv(d);
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.texel(x0, y0).lerp(self.texel(x0 + 1, y0), fx);
        let b = self.texel(x0, y0 + 1).lerp(self.texel(x0 + 1, y0 + 1), fx);
        a.lerp(b, fy)
    }

    /// Densidad (por ángulo sólido) de muestrear la dirección
    pub fn pdf(&self, d: Vec3) -> f32 {
        let (u, v) = self.dir_to_uv(d);
        let x = ((u * self.width as f32) as usize).min(self.width - 1);
        let y = ((v * self.height as f32) as usize).min(self.height - 1);
        // En los polos el ángulo sólido de un texel tiende a 0: se acota para no dividir por cero
        let sin_t = (PI * v).sin().max(1e-6);
        let p_uv = self.row_prob[y] * self.cond_prob[y * self.width + x] * (self.width * self.height) as f32;
        p_uv / (2.0 * PI * PI * sin_t)
    }

    /// (dirección, radiancia, pdf por ángulo sólido)
    pub fn sample(&self, rng: &mut Rng) -> Option<(Vec3, Vec3, f32)> {
        let (u1, u2) = (rng.next_f32(), rng.next_f32());
        let y = (self.marginal.partition_point(|&c| c <= u1).saturating_sub(1)).min(self.height - 1);
        let row = &self.conditional[y * (self.width + 1)..(y + 1) * (self.width + 1)];
        let x = (row.partition_point(|&c| c <= u2).saturating_sub(1)).min(self.width - 1);
        // Posición dentro del texel (uniforme)
        let u = (x as f32 + rng.next_f32()) / self.width as f32;
        let v = (y as f32 + rng.next_f32()) / self.height as f32;
        let d = self.uv_to_dir(u, v);
        let pdf = self.pdf(d);
        (pdf > 0.0).then(|| (d, self.eval(d), pdf))
    }
}

/// Luz de entorno: color uniforme y/o mapa
#[derive(Debug, Clone)]
pub struct Environment {
    pub color: Vec3,
    pub map: Option<Arc<EnvMap>>,
}

impl Environment {
    pub fn uniform(color: Vec3) -> Self {
        Environment { color, map: None }
    }

    pub fn hdri(map: EnvMap) -> Self {
        Environment { color: Vec3::ZERO, map: Some(Arc::new(map)) }
    }

    pub fn eval(&self, d: Vec3) -> Vec3 {
        match &self.map {
            Some(m) => m.eval(d),
            None => self.color,
        }
    }
}
