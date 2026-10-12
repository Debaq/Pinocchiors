//! Path tracer difuso: rebotes de coseno, emisión, entorno uniforme y un sol con
//! muestreo directo. Fase 3a.

use crate::bvh::Ray;
use crate::camera::Camera;
use crate::image_out::Image;
use crate::rng::Rng;
use crate::scene::RayScene;
use glam::Vec3;
use rayon::prelude::*;
use std::f32::consts::PI;

/// Luz de entorno uniforme (el "cielo")
#[derive(Debug, Clone, Copy)]
pub struct Environment {
    pub color: Vec3,
}

/// Luz direccional. `direction` apunta desde la escena hacia el sol.
#[derive(Debug, Clone, Copy)]
pub struct Sun {
    pub direction: Vec3,
    /// Irradiancia perpendicular a la luz (radiancia × ángulo sólido)
    pub color: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_depth: u32,
    pub seed: u64,
    pub environment: Environment,
    pub sun: Option<Sun>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            width: 640,
            height: 480,
            samples: 64,
            max_depth: 6,
            seed: 1,
            environment: Environment { color: Vec3::splat(0.5) },
            sun: None,
        }
    }
}

fn onb(n: Vec3) -> (Vec3, Vec3) {
    let s = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    (Vec3::new(1.0 + s * n.x * n.x * a, s * b, -s * n.x), Vec3::new(b, s + n.y * n.y * a, -n.y))
}

fn cosine_hemisphere(n: Vec3, rng: &mut Rng) -> Vec3 {
    let (u1, u2) = (rng.next_f32(), rng.next_f32());
    let r = u1.sqrt();
    let phi = 2.0 * PI * u2;
    let (t, b) = onb(n);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - u1).max(0.0).sqrt()).normalize()
}

/// Radiancia que llega por un rayo de cámara
fn radiance(scene: &RayScene, s: &Settings, mut ray: Ray, rng: &mut Rng) -> Vec3 {
    let eps = 1e-4 * scene.extent;
    let mut throughput = Vec3::ONE;
    let mut sum = Vec3::ZERO;
    for depth in 0..s.max_depth {
        let Some(hit) = scene.closest(&ray, f32::INFINITY) else {
            sum += throughput * s.environment.color;
            break;
        };
        let tri = &scene.triangles[hit.tri as usize];
        let mat = &scene.materials[(tri.material as usize).min(scene.materials.len() - 1)];
        let w = 1.0 - hit.u - hit.v;
        let p = tri.v0 + tri.e1 * hit.u + tri.e2 * hit.v;
        let g = tri.geometric_normal();
        let back = g.dot(ray.dir) > 0.0;
        if back && !mat.double_sided {
            // Cara trasera de un material de una sola cara: el rayo la atraviesa
            ray = Ray { origin: p + ray.dir * eps, dir: ray.dir };
            continue;
        }
        let gn = if back { -g } else { g };
        let mut n = (tri.n[0] * w + tri.n[1] * hit.u + tri.n[2] * hit.v).normalize_or_zero();
        if n == Vec3::ZERO {
            n = gn;
        } else if back {
            n = -n;
        }
        // La normal de sombreado no puede quedar del otro lado de la geométrica
        if n.dot(gn) < 0.0 {
            n = gn;
        }

        sum += throughput * mat.emission;

        // Luz directa del sol (luz delta: solo se muestrea aquí)
        if let Some(sun) = s.sun {
            let l = sun.direction.normalize_or_zero();
            let cos = n.dot(l);
            if cos > 0.0 && l.dot(gn) > 0.0 {
                let shadow = Ray { origin: p + gn * eps, dir: l };
                if !scene.occluded(&shadow, f32::INFINITY) {
                    sum += throughput * mat.albedo * sun.color * (cos / PI);
                }
            }
        }

        throughput *= mat.albedo;

        // Ruleta rusa (sin sesgo) desde el tercer rebote
        if depth >= 2 {
            let q = throughput.max_element().clamp(0.05, 0.95);
            if rng.next_f32() > q {
                break;
            }
            throughput /= q;
        }

        let dir = cosine_hemisphere(n, rng);
        ray = Ray { origin: p + gn * eps, dir };
    }
    sum
}

/// Renderiza la escena en RGB lineal.
pub fn render(scene: &RayScene, camera: &Camera, s: &Settings) -> Image {
    let (w, h) = (s.width as usize, s.height as usize);
    let mut pixels = vec![Vec3::ZERO; w * h];
    pixels.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let mut rng = Rng::new(s.seed, (y * w + x) as u64);
            let mut acc = Vec3::ZERO;
            for _ in 0..s.samples {
                let fx = (x as f32 + rng.next_f32()) / w as f32;
                let fy = (y as f32 + rng.next_f32()) / h as f32;
                let c = radiance(scene, s, camera.ray(fx, fy), &mut rng);
                // Un NaN o infinito no debe contaminar el píxel
                if c.is_finite() {
                    acc += c;
                }
            }
            *px = acc / s.samples.max(1) as f32;
        }
    });
    Image { width: s.width, height: s.height, pixels }
}
