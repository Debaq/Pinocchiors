//! Path tracer difuso: rebotes de coseno, emisión, entorno uniforme y un sol con
//! muestreo directo. Fase 3a.

use crate::bvh::Ray;
use crate::camera::Camera;
use crate::image_out::Image;
use crate::rng::Rng;
use crate::bsdf::Bsdf;
use crate::scene::{RayMaterial, RayScene, Triangle};
use glam::{Vec2, Vec3};
use rayon::prelude::*;

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

/// Normal de sombreado con el mapa de normales aplicado. `n` y `gn` ya miran hacia el rayo.
fn apply_normal_map(scene: &RayScene, mat: &RayMaterial, tri: &Triangle, uv: Vec2, n: Vec3) -> Vec3 {
    let Some(Some(tex)) = mat.normal_texture.map(|i| &scene.textures[i]) else { return n };
    // Tangente = dirección de +u; bitangente = dirección de -v (el verde del mapa apunta "arriba")
    let (du1, du2) = (tri.uv[1] - tri.uv[0], tri.uv[2] - tri.uv[0]);
    let det = du1.x * du2.y - du2.x * du1.y;
    if det.abs() < 1e-12 {
        return n;
    }
    let r = 1.0 / det;
    let dpdu = (tri.e1 * du2.y - tri.e2 * du1.y) * r;
    let dpdv = (tri.e2 * du1.x - tri.e1 * du2.x) * r;
    let t = (dpdu - n * n.dot(dpdu)).normalize_or_zero();
    let bt = (-dpdv - n * n.dot(-dpdv) - t * t.dot(-dpdv)).normalize_or_zero();
    if t == Vec3::ZERO || bt == Vec3::ZERO {
        return n;
    }
    let mut c = tex.sample(uv * mat.uv_scale, false).truncate() * 2.0 - Vec3::ONE;
    c.x *= mat.normal_scale;
    c.y *= mat.normal_scale;
    let out = (t * c.x + bt * c.y + n * c.z.max(1e-3)).normalize_or_zero();
    if out == Vec3::ZERO { n } else { out }
}

/// Radiancia que llega por un rayo de cámara
fn radiance(scene: &RayScene, s: &Settings, mut ray: Ray, rng: &mut Rng) -> Vec3 {
    let eps = 1e-4 * scene.extent;
    let mut throughput = Vec3::ONE;
    let mut sum = Vec3::ZERO;
    for depth in 0..s.max_depth {
        let Some(hit) = scene.closest_alpha(&ray, f32::INFINITY, rng) else {
            sum += throughput * s.environment.color;
            break;
        };
        let tri = &scene.triangles[hit.tri as usize];
        let mat = scene.material(tri);
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

        // Propiedades del material en el punto
        let uv = tri.interp_uv(hit.u, hit.v);
        let tuv = uv * mat.uv_scale;
        let vcol = tri.interp_color(hit.u, hit.v);
        let mut base = mat.base_color.truncate() * vcol.truncate();
        if let Some(Some(tex)) = mat.base_texture.map(|i| &scene.textures[i]) {
            base *= tex.sample(tuv, true).truncate();
        }
        let mut emission = mat.emission;
        if let Some(Some(tex)) = mat.emissive_texture.map(|i| &scene.textures[i]) {
            emission *= tex.sample(tuv, true).truncate();
        }
        if mat.unlit {
            sum += throughput * base;
            break;
        }
        sum += throughput * emission;

        let (mut metallic, mut roughness) = (mat.metallic, mat.roughness);
        if let Some(Some(tex)) = mat.metal_rough_texture.map(|i| &scene.textures[i]) {
            let mr = tex.sample(tuv, false);
            roughness *= mr.y;
            metallic *= mr.z;
        }
        let n = apply_normal_map(scene, mat, tri, uv, n);
        let bsdf = if mat.lambert {
            Bsdf::lambert(base)
        } else {
            Bsdf::new(base, metallic, roughness, mat.clearcoat, mat.clearcoat_roughness)
        };
        let (t, b) = onb(n);
        let wo_world = -ray.dir;
        let to_local = |v: Vec3| Vec3::new(v.dot(t), v.dot(b), v.dot(n));
        let wo = to_local(wo_world);

        // Luz directa del sol (luz delta: solo se muestrea aquí)
        if let Some(sun) = s.sun {
            let l = sun.direction.normalize_or_zero();
            if l.dot(gn) > 0.0 && l.dot(n) > 0.0 {
                let shadow = Ray { origin: p + gn * eps, dir: l };
                if !scene.occluded_alpha(&shadow, f32::INFINITY, rng) {
                    let (f, _) = bsdf.eval(wo, to_local(l));
                    sum += throughput * f * sun.color * l.dot(n);
                }
            }
        }

        // Rebote indirecto
        let Some((wi_local, f, pdf)) = bsdf.sample(wo, rng) else { break };
        let wi = t * wi_local.x + b * wi_local.y + n * wi_local.z;
        if wi.dot(gn) <= 0.0 {
            break;
        }
        throughput *= f * (wi_local.z / pdf);

        // Ruleta rusa (sin sesgo) desde el tercer rebote
        if depth >= 2 {
            let q = throughput.max_element().clamp(0.05, 0.95);
            if rng.next_f32() > q {
                break;
            }
            throughput /= q;
        }

        ray = Ray { origin: p + gn * eps, dir: wi };
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
