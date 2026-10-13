//! Render progresivo: pasadas de muestras que se acumulan sobre toda la imagen, con
//! mosaicos en paralelo, cancelación entre mosaicos, tiempo estimado y vista previa
//! que se afina en cada pasada.

use crate::camera::Camera;
use crate::denoise::{DenoiseSettings, denoise};
use crate::image_out::{Image, ToneMap};
use crate::render::{Aov, Settings, first_hit, lum, radiance};
use crate::rng::Rng;
use crate::scene::RayScene;
use glam::Vec3;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct ProgressiveOptions {
    /// Cuántas pasadas se hacen; las muestras totales (`Settings::samples`) se reparten entre ellas
    pub passes: u32,
    /// Lado del mosaico en píxeles (unidad de reparto de trabajo y de cancelación)
    pub tile: u32,
    /// Denoise del resultado final
    pub denoise: Option<DenoiseSettings>,
    /// Aplicarlo también a las vistas previas (más lento por pasada)
    pub denoise_previews: bool,
}

impl Default for ProgressiveOptions {
    fn default() -> Self {
        ProgressiveOptions { passes: 8, tile: 32, denoise: Some(DenoiseSettings::default()), denoise_previews: false }
    }
}

/// Imagen con alfa opcional (solo si el fondo es transparente)
#[derive(Debug, Clone)]
pub struct Frame {
    /// Color lineal; con alfa ya sin premultiplicar
    pub image: Image,
    pub alpha: Option<Vec<f32>>,
}

impl Frame {
    /// RGBA de 8 bits en sRGB
    pub fn to_srgb8_rgba(&self) -> Vec<u8> {
        self.to_srgb8_rgba_with(ToneMap::Linear)
    }

    /// RGBA de 8 bits en sRGB pasando antes por la curva de tono
    pub fn to_srgb8_rgba_with(&self, tone: ToneMap) -> Vec<u8> {
        let rgb = self.image.to_srgb8_with(tone);
        let mut out = Vec::with_capacity(rgb.len() / 3 * 4);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            out.extend_from_slice(px);
            out.push(self.alpha.as_ref().map_or(255, |a| (a[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
        }
        out
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), crate::SaveError> {
        image::save_buffer(path, &self.to_srgb8_rgba(), self.image.width, self.image.height, image::ExtendedColorType::Rgba8)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PassInfo {
    /// Pasada terminada (1 = la primera)
    pub pass: u32,
    pub passes: u32,
    pub samples: u32,
    pub total_samples: u32,
    pub elapsed_secs: f64,
    /// Tiempo que falta, estimado con el ritmo medido hasta ahora
    pub eta_secs: f64,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub frame: Frame,
    pub cancelled: bool,
    /// Muestras por píxel que llegaron a completarse en toda la imagen
    pub samples: u32,
}

#[derive(Default, Clone, Copy)]
struct Acc {
    color: Vec3,
    alpha: f32,
    lum: f32,
    lum2: f32,
    n: u32,
}

/// Capas auxiliares (albedo, normal, posición, profundidad): se calculan una vez
fn aov_layers(scene: &RayScene, camera: &Camera, s: &Settings) -> Aov {
    let (w, h) = (s.width as usize, s.height as usize);
    let n = w * h;
    let aov_n = s.samples.clamp(1, 8);
    let a = camera.ray(0.5, 0.5).dir;
    let b = camera.ray(0.5, 0.5 + 1.0 / h as f32).dir;
    let pixel_angle = a.dot(b).clamp(-1.0, 1.0).acos();
    let layers: Vec<(Vec3, Vec3, Vec3, f32)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let mut rng = Rng::new(s.seed ^ 0x5DEE_CE66, i as u64);
            let (mut al, mut nr, mut ps, mut d, mut hits, mut bg) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0f32, 0u32, Vec3::ZERO);
            for _ in 0..aov_n {
                let fx = (x as f32 + rng.next_f32()) / w as f32;
                let fy = (y as f32 + rng.next_f32()) / h as f32;
                let (a, n, p, t) = first_hit(scene, &s.environment, camera.ray(fx, fy), &mut rng);
                if t.is_finite() {
                    al += a;
                    nr += n;
                    ps += p;
                    d += t;
                    hits += 1;
                } else {
                    bg += a;
                }
            }
            let div = aov_n as f32;
            if hits > 0 {
                ((al + bg) / div, nr.normalize_or_zero(), ps / hits as f32, d / hits as f32)
            } else {
                (bg / div, Vec3::ZERO, Vec3::ZERO, f32::INFINITY)
            }
        })
        .collect();
    Aov {
        width: s.width,
        height: s.height,
        color: vec![Vec3::ZERO; n],
        albedo: layers.iter().map(|l| l.0).collect(),
        normal: layers.iter().map(|l| l.1).collect(),
        position: layers.iter().map(|l| l.2).collect(),
        depth: layers.iter().map(|l| l.3).collect(),
        variance: vec![0.0; n],
        pixel_angle,
    }
}

fn frame_from(acc: &[Acc], aov: &mut Aov, s: &Settings, denoise_with: Option<&DenoiseSettings>) -> Frame {
    for (i, a) in acc.iter().enumerate() {
        let n = a.n.max(1) as f32;
        aov.color[i] = a.color / n;
        aov.variance[i] = if a.n > 1 {
            let mean = a.lum / n;
            ((a.lum2 / n - mean * mean).max(0.0) * n / (n - 1.0)) / n
        } else {
            0.0
        };
    }
    let mut image = match denoise_with {
        Some(d) => denoise(aov, d),
        None => Image { width: aov.width, height: aov.height, pixels: aov.color.clone() },
    };
    let alpha = s.transparent_background.then(|| {
        let alpha: Vec<f32> = acc.iter().map(|a| a.alpha / a.n.max(1) as f32).collect();
        // El color acumulado está premultiplicado por el alfa
        for (p, a) in image.pixels.iter_mut().zip(&alpha) {
            if *a > 1e-4 {
                *p /= *a;
            }
        }
        alpha
    });
    Frame { image, alpha }
}

/// Renderiza por pasadas. Después de cada una llama a `on_pass` con la imagen acumulada;
/// si `cancel` se activa, termina el mosaico en curso y devuelve lo que haya.
pub fn render_progressive(
    scene: &RayScene,
    camera: &Camera,
    s: &Settings,
    o: &ProgressiveOptions,
    cancel: &AtomicBool,
    mut on_pass: impl FnMut(&PassInfo, &Frame),
) -> Outcome {
    let (w, h) = (s.width as usize, s.height as usize);
    let total = s.samples.max(1);
    let passes = o.passes.clamp(1, total);
    let tile = o.tile.max(8) as usize;
    let start = Instant::now();
    let mut aov = aov_layers(scene, camera, s);
    let mut acc = vec![Acc::default(); w * h];

    // Mosaicos como bloques contiguos de filas × columnas
    let tiles: Vec<(usize, usize)> =
        (0..h.div_ceil(tile)).flat_map(|ty| (0..w.div_ceil(tile)).map(move |tx| (tx * tile, ty * tile))).collect();

    let mut done = 0u32;
    let mut cancelled = false;
    for pass in 0..passes {
        let share = total / passes + u32::from(pass < total % passes);
        let seed = s.seed ^ (pass as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        // Cada mosaico devuelve sus píxeles; así no se comparte memoria mutable entre hilos
        let results: Vec<Option<(usize, usize, Vec<Acc>)>> = tiles
            .par_iter()
            .map(|&(x0, y0)| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let (x1, y1) = ((x0 + tile).min(w), (y0 + tile).min(h));
                let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0));
                for y in y0..y1 {
                    for x in x0..x1 {
                        let mut rng = Rng::new(seed, (y * w + x) as u64);
                        let mut a = Acc::default();
                        for _ in 0..share {
                            let fx = (x as f32 + rng.next_f32()) / w as f32;
                            let fy = (y as f32 + rng.next_f32()) / h as f32;
                            let ray = camera.sample_ray(fx, fy, &mut rng);
                            let (c, al) = radiance(scene, s, ray, &mut rng);
                            if c.is_finite() {
                                let l = lum(c);
                                a.color += c;
                                a.alpha += al;
                                a.lum += l;
                                a.lum2 += l * l;
                                a.n += 1;
                            }
                        }
                        out.push(a);
                    }
                }
                Some((x0, y0, out))
            })
            .collect();
        let mut complete = true;
        for r in results {
            match r {
                Some((x0, y0, px)) => {
                    let tw = (x0 + tile).min(w) - x0;
                    for (k, a) in px.into_iter().enumerate() {
                        let i = (y0 + k / tw) * w + x0 + k % tw;
                        let t = &mut acc[i];
                        t.color += a.color;
                        t.alpha += a.alpha;
                        t.lum += a.lum;
                        t.lum2 += a.lum2;
                        t.n += a.n;
                    }
                }
                None => complete = false,
            }
        }
        if !complete {
            cancelled = true;
            break;
        }
        done += share;
        let elapsed = start.elapsed().as_secs_f64();
        let last = pass + 1 == passes;
        let preview_denoise = if last { o.denoise.as_ref() } else { o.denoise.as_ref().filter(|_| o.denoise_previews) };
        let frame = frame_from(&acc, &mut aov, s, preview_denoise);
        let info = PassInfo {
            pass: pass + 1,
            passes,
            samples: done,
            total_samples: total,
            elapsed_secs: elapsed,
            eta_secs: elapsed / done as f64 * (total - done) as f64,
        };
        on_pass(&info, &frame);
        if cancel.load(Ordering::Relaxed) && !last {
            cancelled = true;
            break;
        }
    }
    // Lo acumulado hasta ahora; si se canceló no se aplica denoise (las muestras por píxel difieren)
    let frame = frame_from(&acc, &mut aov, s, if cancelled { None } else { o.denoise.as_ref() });
    Outcome { frame, cancelled, samples: done }
}
