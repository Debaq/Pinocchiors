//! Path tracer difuso: rebotes de coseno, emisión, entorno uniforme y un sol con
//! muestreo directo. Fase 3a.

use crate::bvh::Ray;
use crate::camera::Camera;
use crate::image_out::Image;
use crate::rng::Rng;
use crate::bsdf::Bsdf;
use crate::env::Environment;
use crate::scene::{RayMaterial, RayScene, Triangle};
use glam::{Vec2, Vec3};
use rayon::prelude::*;

/// Luz direccional. `direction` apunta desde la escena hacia el sol.
#[derive(Debug, Clone, Copy)]
pub struct Sun {
    pub direction: Vec3,
    /// Irradiancia perpendicular a la luz (radiancia × ángulo sólido)
    pub color: Vec3,
}

/// Luces delta (sin tamaño). Las luces de área son triángulos emisivos de la escena.
#[derive(Debug, Clone, Copy)]
pub enum Light {
    Sun(Sun),
    /// `intensity` en W/sr: la irradiancia cae con 1/d²
    Point { position: Vec3, intensity: Vec3 },
    /// Foco: `direction` hacia donde apunta; transición suave entre `inner` y `outer` (radianes)
    Spot { position: Vec3, direction: Vec3, intensity: Vec3, inner: f32, outer: f32 },
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    /// Rebotes máximos (0 = solo emisión visible; 1 = luz directa + un rebote)
    pub max_depth: u32,
    pub seed: u64,
    pub environment: Environment,
    pub lights: Vec<Light>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            width: 640,
            height: 480,
            samples: 64,
            max_depth: 6,
            seed: 1,
            environment: Environment::uniform(Vec3::splat(0.5)),
            lights: Vec::new(),
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

#[inline]
fn power(a: f32, b: f32) -> f32 {
    let (a2, b2) = (a * a, b * b);
    if a2 + b2 > 0.0 { a2 / (a2 + b2) } else { 0.0 }
}

/// Radiancia que llega por un rayo de cámara.
///
/// Luz directa por muestreo de luces (sol/puntual/foco exactos; área y HDRI por
/// importancia) combinada con el muestreo del BSDF mediante MIS (heurística de
/// potencia). `max_depth` cuenta rebotes: el último vértice solo recoge emisión.
fn radiance(scene: &RayScene, s: &Settings, mut ray: Ray, rng: &mut Rng) -> Vec3 {
    let eps = 1e-4 * scene.extent;
    let mut throughput = Vec3::ONE;
    let mut sum = Vec3::ZERO;
    // pdf del último rebote del BSDF y su punto; 0 = viene de la cámara (sin MIS)
    let mut prev_pdf = 0.0f32;
    let mut prev_p = Vec3::ZERO;
    let env_map = s.environment.map.as_deref();
    let mut depth = 0u32;
    // Cada iteración es un vértice; los cruces descartados (cara trasera) no cuentan
    let mut guard = 0;
    while guard < 4 * (s.max_depth + 2) {
        guard += 1;
        let Some(hit) = scene.closest_alpha(&ray, f32::INFINITY, rng) else {
            let mut le = s.environment.eval(ray.dir);
            if let (Some(map), true) = (env_map, prev_pdf > 0.0) {
                le *= power(prev_pdf, map.pdf(ray.dir));
            }
            sum += throughput * le;
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
        // Emisión; si el triángulo también se muestrea como luz, pesa contra ese muestreo
        let mut e_weight = 1.0;
        if prev_pdf > 0.0 {
            let p_sel = scene.area_lights.select_prob(hit.tri);
            if p_sel > 0.0 {
                let area = 0.5 * tri.e1.cross(tri.e2).length();
                let d2 = (p - prev_p).length_squared();
                let cos_l = gn.dot(-ray.dir).abs().max(1e-6);
                e_weight = power(prev_pdf, p_sel * d2 / (cos_l * area));
            }
        }
        sum += throughput * emission * e_weight;
        if depth >= s.max_depth {
            break;
        }

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
        let to_local = |v: Vec3| Vec3::new(v.dot(t), v.dot(b), v.dot(n));
        let wo = to_local(-ray.dir);
        let origin = p + gn * eps;

        // Luces delta: se evalúan exactas, sin MIS
        for light in &s.lights {
            let (l, dist, li) = match *light {
                Light::Sun(sun) => (sun.direction.normalize_or_zero(), f32::INFINITY, sun.color),
                Light::Point { position, intensity } => {
                    let d = position - origin;
                    let d2 = d.length_squared().max(1e-12);
                    (d / d2.sqrt(), d2.sqrt(), intensity / d2)
                }
                Light::Spot { position, direction, intensity, inner, outer } => {
                    let d = position - origin;
                    let d2 = d.length_squared().max(1e-12);
                    let l = d / d2.sqrt();
                    let cos_a = (-l).dot(direction.normalize_or_zero());
                    let (c_in, c_out) = (inner.cos(), outer.cos());
                    let t = ((cos_a - c_out) / (c_in - c_out).max(1e-6)).clamp(0.0, 1.0);
                    (l, d2.sqrt(), intensity * (t * t * (3.0 - 2.0 * t)) / d2)
                }
            };
            if li == Vec3::ZERO || l.dot(gn) <= 0.0 || l.dot(n) <= 0.0 {
                continue;
            }
            let t_max = if dist.is_finite() { dist - eps } else { f32::INFINITY };
            if !scene.occluded_alpha(&Ray { origin, dir: l }, t_max, rng) {
                let (f, _) = bsdf.eval(wo, to_local(l));
                sum += throughput * f * li * l.dot(n);
            }
        }

        // Luces de área: un triángulo emisivo por vértice, elegido por potencia
        if !scene.area_lights.is_empty() {
            let (lt, p_sel) = scene.area_lights.pick(rng.next_f32());
            let ltri = &scene.triangles[lt as usize];
            let lmat = scene.material(ltri);
            let su = rng.next_f32().sqrt();
            let (bu, bv) = (1.0 - su, rng.next_f32() * su);
            let q = ltri.v0 + ltri.e1 * bu + ltri.e2 * bv;
            let ln = ltri.geometric_normal();
            let d = q - origin;
            let d2 = d.length_squared();
            if d2 > 1e-12 {
                let dist = d2.sqrt();
                let wi = d / dist;
                let cos_l_signed = ln.dot(-wi);
                if (lmat.double_sided || cos_l_signed > 0.0) && wi.dot(gn) > 0.0 && wi.dot(n) > 0.0 {
                    let cos_l = cos_l_signed.abs().max(1e-6);
                    let area = 0.5 * ltri.e1.cross(ltri.e2).length();
                    let pdf_l = p_sel * d2 / (cos_l * area);
                    let shadow = Ray { origin, dir: wi };
                    if !scene.occluded_alpha(&shadow, dist * (1.0 - 1e-3) - eps, rng) {
                        let (f, pdf_b) = bsdf.eval(wo, to_local(wi));
                        sum += throughput * f * lmat.emission * (wi.dot(n) * power(pdf_l, pdf_b) / pdf_l);
                    }
                }
            }
        }

        // Entorno con mapa: muestreo por importancia
        if let Some(map) = env_map {
            if let Some((wi, le, pdf_e)) = map.sample(rng) {
                if wi.dot(gn) > 0.0 && wi.dot(n) > 0.0 && !scene.occluded_alpha(&Ray { origin, dir: wi }, f32::INFINITY, rng) {
                    let (f, pdf_b) = bsdf.eval(wo, to_local(wi));
                    sum += throughput * f * le * (wi.dot(n) * power(pdf_e, pdf_b) / pdf_e);
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
        prev_pdf = pdf;
        prev_p = p;
        depth += 1;

        // Ruleta rusa (sin sesgo) desde el tercer rebote
        if depth >= 3 {
            let q = throughput.max_element().clamp(0.05, 0.95);
            if rng.next_f32() > q {
                break;
            }
            throughput /= q;
        }

        ray = Ray { origin, dir: wi };
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
