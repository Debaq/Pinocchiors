//! Path tracer difuso: rebotes de coseno, emisión, entorno uniforme y un sol con
//! muestreo directo. Fase 3a.

use crate::bvh::Ray;
use crate::camera::Camera;
use crate::image_out::Image;
use crate::rng::Rng;
use crate::bsdf::Bsdf;
use crate::env::Environment;
use crate::glass::{absorption_sigma, fresnel_dielectric, reflect, refract};
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
    /// Los píxeles donde la cámara ve directo el entorno salen transparentes (alfa 0)
    pub transparent_background: bool,
    /// Color con que se ve el fondo directo (la cámara); no ilumina. `None` = el del entorno
    pub background: Option<Vec3>,
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
            transparent_background: false,
            background: None,
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
pub(crate) fn radiance(scene: &RayScene, s: &Settings, mut ray: Ray, rng: &mut Rng) -> (Vec3, f32) {
    let eps = 1e-4 * scene.extent;
    let mut throughput = Vec3::ONE;
    let mut sum = Vec3::ZERO;
    // pdf del último rebote del BSDF y su punto; 0 = viene de la cámara (sin MIS)
    let mut prev_pdf = 0.0f32;
    let mut prev_p = Vec3::ZERO;
    let env_map = s.environment.map.as_deref();
    let mut depth = 0u32;
    // Extinción del volumen en el que está el rayo (dentro de un vidrio o líquido cerrado)
    let mut medium: Option<Vec3> = None;
    // Mientras el rayo no haya rebotado ni cruzado vidrio, el fondo que ve es el de la imagen
    let mut camera_visible = true;
    // Cada iteración es un vértice; los cruces descartados (cara trasera) y los eventos
    // de vidrio no cuentan como rebote, pero el total se acota
    let mut guard = 0;
    while guard < 64 + 4 * s.max_depth {
        guard += 1;
        let Some(hit) = scene.closest_alpha(&ray, f32::INFINITY, rng) else {
            if camera_visible && s.transparent_background {
                return (Vec3::ZERO, 0.0);
            }
            if let (true, Some(bg)) = (camera_visible, s.background) {
                sum += throughput * bg;
                break;
            }
            let mut le = s.environment.eval(ray.dir);
            if let (Some(map), true) = (env_map, prev_pdf > 0.0) {
                le *= power(prev_pdf, map.pdf(ray.dir));
            }
            sum += throughput * le;
            break;
        };
        if let Some(sigma) = medium {
            throughput *= Vec3::new((-sigma.x * hit.t).exp(), (-sigma.y * hit.t).exp(), (-sigma.z * hit.t).exp());
        }
        let tri = &scene.triangles[hit.tri as usize];
        let mat = scene.material(tri);
        let w = 1.0 - hit.u - hit.v;
        let p = tri.v0 + tri.e1 * hit.u + tri.e2 * hit.v;
        let g = tri.geometric_normal();
        let back = g.dot(ray.dir) > 0.0;
        if back && !mat.double_sided && mat.transmission <= 0.0 {
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

        // Vidrio y líquidos: interfaz lisa, se elige reflexión o refracción con la
        // probabilidad de Fresnel (el peso queda en 1). Es un evento delta: sin luz
        // directa en este vértice y sin MIS en el siguiente.
        let t_prob = if medium.is_some() { 1.0 } else { (mat.transmission * (1.0 - metallic.clamp(0.0, 1.0))).clamp(0.0, 1.0) };
        if t_prob > 0.0 && rng.next_f32() < t_prob {
            let volume = mat.thickness > 0.0;
            let cos_i = n.dot(-ray.dir).clamp(1e-4, 1.0);
            // Aire por fuera; el volumen se recorre hacia adentro por la cara frontal
            let (n1, n2) = if volume && back { (mat.ior, 1.0) } else { (1.0, mat.ior) };
            let f = fresnel_dielectric(cos_i, n1, n2);
            let (dir, side) = if rng.next_f32() < f {
                (reflect(ray.dir, n), 1.0)
            } else if volume {
                match refract(ray.dir, n, n1 / n2) {
                    Some(d) => {
                        medium = if back { None } else { Some(absorption_sigma(base, mat.thickness)) };
                        (d, -1.0)
                    }
                    None => (reflect(ray.dir, n), 1.0),
                }
            } else {
                // Lámina delgada: la luz sigue derecha y se tiñe con el color base
                throughput *= base;
                (ray.dir, -1.0)
            };
            if dir.dot(gn) * side <= 0.0 {
                break;
            }
            ray = Ray { origin: p + gn * (eps * side), dir };
            prev_pdf = 0.0;
            camera_visible = false;
            continue;
        }

        let bsdf = if mat.lambert {
            Bsdf::lambert(base)
        } else {
            Bsdf::with_ior(base, metallic, roughness, mat.clearcoat, mat.clearcoat_roughness, mat.ior)
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
        camera_visible = false;

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
    (sum, 1.0)
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
                let ray = camera.sample_ray(fx, fy, &mut rng);
                let (c, _) = radiance(scene, s, ray, &mut rng);
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

/// Imagen con las capas auxiliares que guían el denoise
#[derive(Debug, Clone)]
pub struct Aov {
    pub width: u32,
    pub height: u32,
    /// RGB lineal con ruido
    pub color: Vec<Vec3>,
    /// Color de la superficie sin iluminar (textura × base); el fondo, el del entorno
    pub albedo: Vec<Vec3>,
    /// Normal de sombreado hacia el rayo; (0,0,0) donde no hay geometría
    pub normal: Vec<Vec3>,
    /// Posición en el mundo del primer cruce
    pub position: Vec<Vec3>,
    /// Distancia de la cámara al primer cruce (infinito sin geometría)
    pub depth: Vec<f32>,
    /// Varianza de la luminancia del promedio del píxel (ruido que queda)
    pub variance: Vec<f32>,
    /// Ángulo (rad) que abarca un píxel: sirve para medir el tamaño de píxel a cada profundidad
    pub pixel_angle: f32,
}

pub(crate) fn lum(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// Primer cruce visible: (albedo, normal, posición, distancia)
pub(crate) fn first_hit(scene: &RayScene, env: &Environment, ray: Ray, rng: &mut Rng) -> (Vec3, Vec3, Vec3, f32) {
    let Some(hit) = scene.closest_alpha(&ray, f32::INFINITY, rng) else {
        return (env.eval(ray.dir).clamp(Vec3::ZERO, Vec3::ONE), Vec3::ZERO, Vec3::ZERO, f32::INFINITY);
    };
    let tri = &scene.triangles[hit.tri as usize];
    let mat = scene.material(tri);
    let w = 1.0 - hit.u - hit.v;
    let g = tri.geometric_normal();
    let back = g.dot(ray.dir) > 0.0;
    let gn = if back { -g } else { g };
    let mut n = (tri.n[0] * w + tri.n[1] * hit.u + tri.n[2] * hit.v).normalize_or_zero();
    if n == Vec3::ZERO {
        n = gn;
    } else if back {
        n = -n;
    }
    if n.dot(gn) < 0.0 {
        n = gn;
    }
    let tuv = tri.interp_uv(hit.u, hit.v) * mat.uv_scale;
    let mut base = mat.base_color.truncate() * tri.interp_color(hit.u, hit.v).truncate();
    if let Some(Some(tex)) = mat.base_texture.map(|i| &scene.textures[i]) {
        base *= tex.sample(tuv, true).truncate();
    }
    let albedo = if mat.transmission > 0.5 {
        Vec3::ONE
    } else {
        // Un emisor casi negro se modula por su emisión para que el denoise no divida por ~0
        base.max(mat.emission.clamp(Vec3::ZERO, Vec3::ONE)).clamp(Vec3::ZERO, Vec3::ONE)
    };
    (albedo, n, tri.v0 + tri.e1 * hit.u + tri.e2 * hit.v, hit.t)
}

/// Renderiza el color y las capas que necesita [`crate::denoise`]. El color es idéntico
/// al de [`render`]; las capas se promedian con hasta 8 muestras por píxel (cámara sin lente).
pub fn render_aov(scene: &RayScene, camera: &Camera, s: &Settings) -> Aov {
    let (w, h) = (s.width as usize, s.height as usize);
    let n = w * h;
    let mut color = vec![Vec3::ZERO; n];
    let mut albedo = vec![Vec3::ZERO; n];
    let mut normal = vec![Vec3::ZERO; n];
    let mut position = vec![Vec3::ZERO; n];
    let mut depth = vec![f32::INFINITY; n];
    let mut variance = vec![0.0f32; n];
    let aov_n = s.samples.clamp(1, 8);
    let a = camera.ray(0.5, 0.5).dir;
    let b = camera.ray(0.5, 0.5 + 1.0 / h as f32).dir;
    let pixel_angle = a.dot(b).clamp(-1.0, 1.0).acos();

    type Row<'a> = (&'a mut [Vec3], &'a mut [Vec3], &'a mut [Vec3], &'a mut [Vec3], &'a mut [f32], &'a mut [f32]);
    let rows: Vec<Row> = color
        .chunks_mut(w)
        .zip(albedo.chunks_mut(w))
        .zip(normal.chunks_mut(w))
        .zip(position.chunks_mut(w))
        .zip(depth.chunks_mut(w))
        .zip(variance.chunks_mut(w))
        .map(|(((((c, a), nr), p), d), v)| (c, a, nr, p, d, v))
        .collect();
    rows.into_par_iter().enumerate().for_each(|(y, (crow, arow, nrow, prow, drow, vrow))| {
        for x in 0..w {
            let mut rng = Rng::new(s.seed, (y * w + x) as u64);
            let mut aov_rng = Rng::new(s.seed ^ 0x5DEE_CE66, (y * w + x) as u64);
            let (mut acc, mut sum_l, mut sum_l2, mut valid) = (Vec3::ZERO, 0.0f32, 0.0f32, 0u32);
            let (mut a_acc, mut n_acc, mut p_acc, mut d_acc, mut hits) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0f32, 0u32);
            let mut a_bg = Vec3::ZERO;
            for i in 0..s.samples {
                let fx = (x as f32 + rng.next_f32()) / w as f32;
                let fy = (y as f32 + rng.next_f32()) / h as f32;
                let ray = camera.sample_ray(fx, fy, &mut rng);
                let (c, _) = radiance(scene, s, ray, &mut rng);
                if c.is_finite() {
                    acc += c;
                    let l = lum(c);
                    sum_l += l;
                    sum_l2 += l * l;
                    valid += 1;
                }
                if i < aov_n {
                    let (al, nr, ps, t) = first_hit(scene, &s.environment, camera.ray(fx, fy), &mut aov_rng);
                    if t.is_finite() {
                        a_acc += al;
                        n_acc += nr;
                        p_acc += ps;
                        d_acc += t;
                        hits += 1;
                    } else {
                        a_bg += al;
                    }
                }
            }
            let ns = s.samples.max(1) as f32;
            crow[x] = acc / ns;
            if hits > 0 {
                let hf = hits as f32;
                // Borde del objeto: mezcla de geometría y fondo; el albedo se promedia con los dos
                arow[x] = (a_acc + a_bg) / aov_n.min(s.samples.max(1)) as f32;
                nrow[x] = n_acc.normalize_or_zero();
                prow[x] = p_acc / hf;
                drow[x] = d_acc / hf;
            } else {
                arow[x] = a_bg / aov_n.min(s.samples.max(1)) as f32;
            }
            if valid > 1 {
                let v = valid as f32;
                let mean = sum_l / v;
                vrow[x] = ((sum_l2 / v - mean * mean).max(0.0) * v / (v - 1.0)) / v;
            }
        }
    });
    Aov { width: s.width, height: s.height, color, albedo, normal, position, depth, variance, pixel_angle }
}
