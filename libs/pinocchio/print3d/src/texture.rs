//! Mapas de altura para pieles imprimibles: patrones de fábrica sin costuras y operaciones sobre imágenes
//!
//! Un mapa de altura es una imagen en grises con valores de 0 (más bajo) a 1 (más alto). Se muestrea con
//! repetición (`u`, `v` fuera de 0..1 vuelven a empezar), así que una baldosa cubre cualquier superficie.

use pinocchio_math::Real;
use serde::{Deserialize, Serialize};

use crate::error::{Print3dError, Result};

/// Imagen de alturas, de 0 a 1, fila a fila desde arriba
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightMap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl HeightMap {
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Result<Self> {
        if width == 0 || height == 0 || data.len() != width * height {
            return Err(Print3dError::InvalidConfig(format!("el mapa de altura de {width}×{height} lleva {} valores, no {}", width * height, data.len())));
        }
        Ok(Self { width, height, data })
    }

    /// Desde grises de 8 bits
    pub fn from_gray8(width: usize, height: usize, bytes: &[u8]) -> Result<Self> {
        Self::new(width, height, bytes.iter().map(|&b| f32::from(b) / 255.0).collect())
    }

    /// Desde RGBA de 8 bits: la luminosidad manda y lo transparente cuenta como negro
    pub fn from_rgba8(width: usize, height: usize, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != width * height * 4 {
            return Err(Print3dError::InvalidConfig("los bytes de la imagen no cuadran con su tamaño".into()));
        }
        let data = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let lum = 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
                lum / 255.0 * f32::from(p[3]) / 255.0
            })
            .collect();
        Self::new(width, height, data)
    }

    /// A grises de 8 bits (para vistas previas y para guardar en el proyecto)
    pub fn to_gray8(&self) -> Vec<u8> {
        self.data.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
    }

    fn at(&self, x: isize, y: isize) -> f32 {
        let (w, h) = (self.width as isize, self.height as isize);
        self.data[(y.rem_euclid(h) * w + x.rem_euclid(w)) as usize]
    }

    /// Altura en `(u, v)` con repetición y mezcla bilineal. `(0, 0)` es el centro del píxel de arriba a la izquierda
    pub fn sample(&self, u: Real, v: Real) -> Real {
        let x = u * self.width as Real - 0.5;
        let y = v * self.height as Real - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (ix, iy) = (x0 as isize, y0 as isize);
        let top = Real::from(self.at(ix, iy)) * (1.0 - fx) + Real::from(self.at(ix + 1, iy)) * fx;
        let bottom = Real::from(self.at(ix, iy + 1)) * (1.0 - fx) + Real::from(self.at(ix + 1, iy + 1)) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// Blanco por negro
    pub fn invert(&mut self) {
        for v in &mut self.data {
            *v = 1.0 - *v;
        }
    }

    /// Contraste alrededor del gris medio: `gain` 1 no cambia nada, mayor lo aprieta hacia 0 y 1
    pub fn contrast(&mut self, gain: Real) {
        let g = gain.max(0.0) as f32;
        for v in &mut self.data {
            *v = ((*v - 0.5) * g + 0.5).clamp(0.0, 1.0);
        }
    }

    /// Estira los valores para que usen todo el rango 0..1 (un mapa plano se queda en 0)
    pub fn normalize(&mut self) {
        let (lo, hi) = self.data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
        if hi - lo < 1e-6 {
            self.data.iter_mut().for_each(|v| *v = 0.0);
            return;
        }
        for v in &mut self.data {
            *v = (*v - lo) / (hi - lo);
        }
    }

    /// Desenfoque de caja de `radius` píxeles, con repetición (no abre costuras)
    pub fn blur(&mut self, radius: usize) {
        if radius == 0 {
            return;
        }
        let (w, h, r) = (self.width as isize, self.height as isize, radius as isize);
        let norm = 1.0 / (2 * r + 1) as f32;
        let mut tmp = vec![0.0; self.data.len()];
        for y in 0..h {
            for x in 0..w {
                tmp[(y * w + x) as usize] = (-r..=r).map(|k| self.at(x + k, y)).sum::<f32>() * norm;
            }
        }
        let horizontal = HeightMap { width: self.width, height: self.height, data: tmp };
        for y in 0..h {
            for x in 0..w {
                self.data[(y * w + x) as usize] = (-r..=r).map(|k| horizontal.at(x, y + k)).sum::<f32>() * norm;
            }
        }
    }

    /// Reduce a como mucho `max` píxeles de lado por promedio de cajas
    pub fn downscale(&self, max: usize) -> HeightMap {
        let longest = self.width.max(self.height);
        if longest <= max || max == 0 {
            return self.clone();
        }
        let f = longest as Real / max as Real;
        let (w, h) = (((self.width as Real / f).round() as usize).max(1), ((self.height as Real / f).round() as usize).max(1));
        let mut data = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (x0, x1) = ((x as Real * f) as usize, (((x + 1) as Real * f).ceil() as usize).min(self.width).max((x as Real * f) as usize + 1));
                let (y0, y1) = ((y as Real * f) as usize, (((y + 1) as Real * f).ceil() as usize).min(self.height).max((y as Real * f) as usize + 1));
                let mut sum = 0.0;
                let mut n = 0.0;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        sum += self.data[yy * self.width + xx];
                        n += 1.0;
                    }
                }
                data[y * w + x] = sum / n;
            }
        }
        HeightMap { width: w, height: h, data }
    }
}

/// Patrones de fábrica: todos sin costuras
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Grano de cuero (celdas irregulares con surcos entre ellas)
    Leather,
    /// Líneas paralelas
    Lines,
    /// Rejilla de surcos
    Grid,
    /// Puntos redondeados
    Dots,
    /// Rombos en relieve
    Diamonds,
    /// Panal
    Hex,
}

impl Pattern {
    pub const ALL: [Pattern; 6] = [Pattern::Leather, Pattern::Lines, Pattern::Grid, Pattern::Dots, Pattern::Diamonds, Pattern::Hex];

    pub fn id(self) -> &'static str {
        match self {
            Pattern::Leather => "leather",
            Pattern::Lines => "lines",
            Pattern::Grid => "grid",
            Pattern::Dots => "dots",
            Pattern::Diamonds => "diamonds",
            Pattern::Hex => "hex",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pattern::Leather => "Cuero",
            Pattern::Lines => "Líneas",
            Pattern::Grid => "Rejilla",
            Pattern::Dots => "Puntos",
            Pattern::Diamonds => "Rombos",
            Pattern::Hex => "Panal",
        }
    }

    pub fn from_id(id: &str) -> Option<Pattern> {
        Pattern::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Baldosa de `size`×`size` píxeles (entre 16 y 2048)
    pub fn generate(self, size: usize) -> HeightMap {
        let n = size.clamp(16, 2048);
        let mut data = vec![0.0f32; n * n];
        for y in 0..n {
            for x in 0..n {
                let (u, v) = ((x as Real + 0.5) / n as Real, (y as Real + 0.5) / n as Real);
                data[y * n + x] = self.height_at(u, v) as f32;
            }
        }
        let mut map = HeightMap { width: n, height: n, data };
        map.normalize();
        map
    }

    fn height_at(self, u: Real, v: Real) -> Real {
        match self {
            Pattern::Lines => {
                // Ocho surcos redondeados por baldosa
                let s = (std::f64::consts::TAU * 8.0 * u).cos();
                smooth(-0.55, 0.55, s)
            }
            Pattern::Grid => {
                let line = |t: Real| {
                    let d = ((t * 4.0).fract() - 0.5).abs() * 2.0; // 0 en el centro del hueco, 1 en la línea
                    smooth(0.78, 0.9, d)
                };
                line(u).max(line(v))
            }
            Pattern::Dots => {
                let (fx, fy) = ((u * 5.0).fract() - 0.5, (v * 5.0).fract() - 0.5);
                let r = (fx * fx + fy * fy).sqrt() / 0.38;
                1.0 - smooth(0.55, 1.0, r)
            }
            Pattern::Diamonds => {
                let (fx, fy) = ((u * 4.0).fract() - 0.5, (v * 4.0).fract() - 0.5);
                // Pirámide de base cuadrada girada 45°: distancia Manhattan al centro
                1.0 - smooth(0.0, 0.5, fx.abs() + fy.abs())
            }
            Pattern::Hex => {
                // Celdas en filas desplazadas medio paso; los bordes entre celdas son la cresta
                let (cols, rows) = (6.0, 8.0);
                let (a, b) = cell_edge_distance(u * cols, v * rows, cols as i64, rows as i64, false, 0);
                1.0 - smooth(0.0, 0.18, (b - a) / ((1.0 / cols).hypot(1.0 / rows) * rows.max(cols)))
            }
            Pattern::Leather => {
                let (n, a) = (9i64, 0u64);
                let (f1, f2) = cell_edge_distance(u * n as Real, v * n as Real, n, n, true, a);
                // Surcos entre las celdas y una arruga fina encima
                let groove = smooth(0.0, 0.22, f2 - f1);
                let wrinkle = 0.5 + 0.5 * (value_noise(u * 48.0, v * 48.0, 48, 7) * 2.0 - 1.0);
                0.8 * groove + 0.2 * wrinkle * groove
            }
        }
    }
}

fn smooth(lo: Real, hi: Real, x: Real) -> Real {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Hash de enteros a [0, 1)
fn hash01(x: i64, y: i64, seed: u64) -> Real {
    let mut z = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as Real / (1u64 << 53) as Real
}

/// Ruido de valor periódico con `period` celdas por lado
fn value_noise(x: Real, y: Real, period: i64, seed: u64) -> Real {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(0.0, 1.0, x - x0), smooth(0.0, 1.0, y - y0));
    let g = |i: i64, j: i64| hash01(i.rem_euclid(period), j.rem_euclid(period), seed);
    let (ix, iy) = (x0 as i64, y0 as i64);
    let top = g(ix, iy) * (1.0 - fx) + g(ix + 1, iy) * fx;
    let bottom = g(ix, iy + 1) * (1.0 - fx) + g(ix + 1, iy + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Distancias al centro de celda más cercano y al segundo más cercano, sobre una rejilla periódica de
/// `cols`×`rows` celdas. Con `jitter` los centros se mueven al azar (cuero); sin él, las filas pares
/// van corridas medio paso (panal). Las distancias salen en unidades de celda
fn cell_edge_distance(x: Real, y: Real, cols: i64, rows: i64, jitter: bool, seed: u64) -> (Real, Real) {
    let (cx, cy) = (x.floor() as i64, y.floor() as i64);
    let (mut d1, mut d2) = (Real::INFINITY, Real::INFINITY);
    for j in -1..=1 {
        for i in -1..=1 {
            let (gx, gy) = (cx + i, cy + j);
            let (wx, wy) = (gx.rem_euclid(cols), gy.rem_euclid(rows));
            let (px, py) = if jitter {
                (gx as Real + 0.15 + 0.7 * hash01(wx, wy, seed + 1), gy as Real + 0.15 + 0.7 * hash01(wx, wy, seed + 2))
            } else {
                (gx as Real + 0.5 + if wy % 2 == 0 { 0.0 } else { 0.5 }, gy as Real + 0.5)
            };
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1, d2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_patrones_usan_todo_el_rango() {
        for p in Pattern::ALL {
            let m = p.generate(128);
            assert_eq!((m.width, m.height), (128, 128));
            let (lo, hi) = m.data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(lo.abs() < 1e-6 && (hi - 1.0).abs() < 1e-6, "{}: {lo}..{hi}", p.id());
            // Ni plano ni todo blanco: varía de verdad
            let mean = m.data.iter().sum::<f32>() / m.data.len() as f32;
            let var = m.data.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / m.data.len() as f32;
            assert!(var.sqrt() > 0.1, "{}: desvío {}", p.id(), var.sqrt());
        }
    }

    #[test]
    fn los_patrones_no_tienen_costura() {
        for p in Pattern::ALL {
            let m = p.generate(128);
            // El salto entre el último píxel y el primero no es mayor que el mayor salto interior
            let interior = |horizontal: bool| {
                let mut worst = 0.0f32;
                for a in 0..128 {
                    for b in 0..127 {
                        let (p0, p1) = if horizontal { (m.data[a * 128 + b], m.data[a * 128 + b + 1]) } else { (m.data[b * 128 + a], m.data[(b + 1) * 128 + a]) };
                        worst = worst.max((p0 - p1).abs());
                    }
                }
                worst
            };
            let seam_h = (0..128).map(|y| (m.data[y * 128 + 127] - m.data[y * 128]).abs()).fold(0.0f32, f32::max);
            let seam_v = (0..128).map(|x| (m.data[127 * 128 + x] - m.data[x]).abs()).fold(0.0f32, f32::max);
            assert!(seam_h <= interior(true) + 0.05, "{}: costura horizontal {seam_h} vs {}", p.id(), interior(true));
            assert!(seam_v <= interior(false) + 0.05, "{}: costura vertical {seam_v} vs {}", p.id(), interior(false));
        }
    }

    #[test]
    fn el_muestreo_se_repite_y_mezcla() {
        let m = Pattern::Dots.generate(64);
        for &(u, v) in &[(0.13, 0.77), (0.5, 0.5), (0.01, 0.99)] {
            assert!((m.sample(u, v) - m.sample(u + 1.0, v - 2.0)).abs() < 1e-9);
        }
        // En el centro de un píxel da el valor del píxel
        assert!((m.sample(0.5 / 64.0, 0.5 / 64.0) - Real::from(m.data[0])).abs() < 1e-9);
        // Entre dos píxeles, el promedio
        let mid = m.sample(1.0 / 64.0, 0.5 / 64.0);
        assert!((mid - Real::from(m.data[0] + m.data[1]) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn los_patrones_son_deterministas_y_distintos() {
        assert_eq!(Pattern::Leather.generate(64), Pattern::Leather.generate(64));
        assert_ne!(Pattern::Leather.generate(64), Pattern::Hex.generate(64));
        assert_eq!(Pattern::from_id("grid"), Some(Pattern::Grid));
        assert_eq!(Pattern::from_id("nada"), None);
        let ids: std::collections::HashSet<_> = Pattern::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), Pattern::ALL.len());
    }

    #[test]
    fn invertir_contraste_y_desenfoque() {
        let mut m = HeightMap::from_gray8(2, 2, &[0, 255, 51, 204]).unwrap();
        let before = m.clone();
        m.invert();
        assert!((m.data[0] - 1.0).abs() < 1e-6 && m.data[1].abs() < 1e-6);
        m.invert();
        assert!(m.data.iter().zip(&before.data).all(|(a, b)| (a - b).abs() < 1e-6));
        m.contrast(2.0);
        assert!(m.data[2] < before.data[2] && m.data[3] > before.data[3]);
        let mut flat = HeightMap::new(4, 4, vec![0.4; 16]).unwrap();
        flat.normalize();
        assert!(flat.data.iter().all(|&v| v == 0.0));
        let mut spike = HeightMap::new(5, 5, (0..25).map(|i| if i == 12 { 1.0 } else { 0.0 }).collect()).unwrap();
        spike.blur(1);
        assert!(spike.data[12] < 0.2 && spike.data[11] > 0.0);
        // El desenfoque conserva el total
        assert!((spike.data.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn imagenes_de_color_y_tamanos() {
        // Rojo opaco, blanco opaco y blanco transparente
        let m = HeightMap::from_rgba8(3, 1, &[255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0]).unwrap();
        assert!((m.data[0] - 0.2126).abs() < 1e-3 && (m.data[1] - 1.0).abs() < 1e-6 && m.data[2].abs() < 1e-6);
        assert!(HeightMap::from_rgba8(2, 2, &[0; 4]).is_err());
        assert!(HeightMap::new(0, 4, vec![]).is_err());
        let big = Pattern::Lines.generate(512);
        let small = big.downscale(128);
        assert_eq!((small.width, small.height), (128, 128));
        assert_eq!(big.downscale(1024), big);
        assert_eq!(HeightMap::from_gray8(2, 1, &[0, 255]).unwrap().to_gray8(), vec![0, 255]);
    }
}
