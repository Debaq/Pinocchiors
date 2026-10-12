//! Denoise à-trous guiado por albedo, normales y profundidad, con ponderación por
//! varianza (al estilo SVGF, solo espacial). Filtra la iluminación (color / albedo) para
//! no desdibujar texturas y la vuelve a multiplicar por el albedo.

use crate::image_out::Image;
use crate::render::Aov;
use glam::Vec3;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct DenoiseSettings {
    /// Pasadas del filtro; cada una dobla el paso (4 cubre ~ 30 píxeles)
    pub iterations: u32,
    /// Tolerancia de luminancia relativa a la desviación típica del ruido
    pub sigma_luminance: f32,
    /// Exponente del peso de normales (más alto = más estricto)
    pub sigma_normal: f32,
    /// Tolerancia de profundidad en píxeles de superficie
    pub sigma_depth: f32,
    /// Un píxel más de `k` veces la media de sus vecinos se recorta a ese valor (0 = no)
    pub firefly_k: f32,
}

impl Default for DenoiseSettings {
    fn default() -> Self {
        DenoiseSettings { iterations: 4, sigma_luminance: 4.0, sigma_normal: 128.0, sigma_depth: 2.0, firefly_k: 6.0 }
    }
}

const KERNEL: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];

fn lum(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

pub fn denoise(aov: &Aov, st: &DenoiseSettings) -> Image {
    let (w, h) = (aov.width as usize, aov.height as usize);
    let n = w * h;

    // Iluminación = color / albedo
    let safe = |a: Vec3| a.max(Vec3::splat(0.01));
    let mut illum: Vec<Vec3> = (0..n).map(|i| aov.color[i] / safe(aov.albedo[i])).collect();
    let mut var: Vec<f32> = (0..n)
        .map(|i| {
            let a = lum(safe(aov.albedo[i]));
            aov.variance[i] / (a * a)
        })
        .collect();

    // Luciérnagas: contra la media de los 8 vecinos
    if st.firefly_k > 0.0 {
        let src = illum.clone();
        illum.par_iter_mut().enumerate().for_each(|(i, c)| {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            let (mut sum, mut cnt) = (0.0f32, 0.0f32);
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    let (qx, qy) = (x + dx, y + dy);
                    if (dx, dy) != (0, 0) && qx >= 0 && qy >= 0 && (qx as usize) < w && (qy as usize) < h {
                        sum += lum(src[qy as usize * w + qx as usize]);
                        cnt += 1.0;
                    }
                }
            }
            let (mean, l) = (sum / cnt.max(1.0), lum(*c));
            let limit = (mean * st.firefly_k).max(0.1);
            if l > limit {
                *c *= limit / l;
            }
        });
    }

    // Varianza: desenfoque 5×5 previo para que no sea ella misma ruidosa
    {
        let src = var.clone();
        var.par_iter_mut().enumerate().for_each(|(i, v)| {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            let (mut sum, mut wsum) = (0.0f32, 0.0f32);
            for dy in -2..=2i64 {
                for dx in -2..=2i64 {
                    let (qx, qy) = (x + dx, y + dy);
                    if qx >= 0 && qy >= 0 && (qx as usize) < w && (qy as usize) < h {
                        let k = [1.0, 2.0, 3.0, 2.0, 1.0][(dx + 2) as usize] * [1.0, 2.0, 3.0, 2.0, 1.0][(dy + 2) as usize];
                        sum += k * src[qy as usize * w + qx as usize];
                        wsum += k;
                    }
                }
            }
            *v = sum / wsum;
        });
    }

    for it in 0..st.iterations {
        let step = 1i64 << it;
        let (src_c, src_v) = (illum.clone(), var.clone());
        let out: Vec<(Vec3, f32)> = (0..n)
            .into_par_iter()
            .map(|i| {
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let (np, ap, lp, tp) = (aov.normal[i], aov.albedo[i], lum(src_c[i]), aov.depth[i]);
                let pp = aov.position[i];
                let sigma_l = st.sigma_luminance * src_v[i].max(0.0).sqrt() + 1e-4;
                let footprint = if tp.is_finite() { tp * aov.pixel_angle } else { 0.0 };
                let (mut sum_c, mut sum_w, mut sum_v) = (Vec3::ZERO, 0.0f32, 0.0f32);
                for ky in -2..=2i64 {
                    for kx in -2..=2i64 {
                        let (qx, qy) = (x + kx * step, y + ky * step);
                        if qx < 0 || qy < 0 || qx as usize >= w || qy as usize >= h {
                            continue;
                        }
                        let j = qy as usize * w + qx as usize;
                        let mut wgt = KERNEL[(kx + 2) as usize] * KERNEL[(ky + 2) as usize];
                        if j != i {
                            let nq = aov.normal[j];
                            // Normales: ambos sin geometría cuentan como iguales
                            let w_n = match (np == Vec3::ZERO, nq == Vec3::ZERO) {
                                (true, true) => 1.0,
                                (false, false) => np.dot(nq).max(0.0).powf(st.sigma_normal),
                                _ => 0.0,
                            };
                            let tq = aov.depth[j];
                            let w_z = if tp.is_finite() && tq.is_finite() {
                                // Distancia al plano tangente de p: coplanar ⇒ 0
                                let plane = np.dot(aov.position[j] - pp).abs();
                                let tol = st.sigma_depth * footprint * (step as f32) * ((kx * kx + ky * ky) as f32).sqrt().max(1.0);
                                (-plane / tol.max(1e-9)).exp()
                            } else if tp.is_finite() == tq.is_finite() {
                                1.0
                            } else {
                                0.0
                            };
                            let da = ap - aov.albedo[j];
                            let w_a = (-da.length_squared() / (2.0 * 0.15 * 0.15)).exp();
                            let w_l = (-(lp - lum(src_c[j])).abs() / sigma_l).exp();
                            wgt *= w_n * w_z * w_a * w_l;
                        }
                        sum_c += src_c[j] * wgt;
                        sum_v += src_v[j] * wgt * wgt;
                        sum_w += wgt;
                    }
                }
                if sum_w > 1e-9 { (sum_c / sum_w, sum_v / (sum_w * sum_w)) } else { (src_c[i], src_v[i]) }
            })
            .collect();
        for (i, (c, v)) in out.into_iter().enumerate() {
            illum[i] = c;
            var[i] = v;
        }
    }

    Image { width: aov.width, height: aov.height, pixels: (0..n).map(|i| illum[i] * safe(aov.albedo[i])).collect() }
}
