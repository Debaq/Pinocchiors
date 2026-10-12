use geppettoray::*;
use glam::{Vec2, Vec3, Vec4};

fn rng_state() -> impl FnMut() -> f32 {
    let mut s = 98765u32;
    move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (s >> 8) as f32 / 16_777_216.0
    }
}

fn random_dir(r: &mut impl FnMut() -> f32) -> Vec3 {
    let z = r();
    let phi = 2.0 * std::f32::consts::PI * r();
    let s = (1.0 - z * z).sqrt();
    Vec3::new(s * phi.cos(), s * phi.sin(), z)
}

const CASOS: [(f32, f32, f32); 6] = [(0.0, 1.0, 0.0), (0.0, 0.3, 0.0), (1.0, 0.5, 0.0), (1.0, 0.1, 0.0), (0.0, 0.6, 1.0), (0.5, 0.8, 0.5)];

/// Reciprocidad: f(wo, wi) = f(wi, wo)
#[test]
fn bsdf_reciprocidad() {
    let mut r = rng_state();
    for (metal, rough, coat) in CASOS {
        let b = Bsdf::new(Vec3::new(0.9, 0.5, 0.2), metal, rough, coat, 0.2);
        for _ in 0..200 {
            let (wo, wi) = (random_dir(&mut r), random_dir(&mut r));
            let (a, _) = b.eval(wo, wi);
            let (c, _) = b.eval(wi, wo);
            assert!((a - c).abs().max_element() <= 1e-3 * (1.0 + a.max_element()), "{a} vs {c} ({metal},{rough},{coat})");
        }
    }
}

/// Conservación: la reflectancia direccional ∫ f cosθ dω no pasa de 1 con base blanca.
#[test]
fn bsdf_energia_menor_que_uno() {
    let mut r = rng_state();
    for (metal, rough, coat) in CASOS {
        let b = Bsdf::new(Vec3::ONE, metal, rough, coat, 0.2);
        for &cos_o in &[0.9f32, 0.5, 0.15] {
            let wo = Vec3::new((1.0 - cos_o * cos_o).sqrt(), 0.0, cos_o);
            let n = 40_000;
            let mut sum = Vec3::ZERO;
            for _ in 0..n {
                let mut rr = || r();
                if let Some((wi, f, pdf)) = b.sample(wo, &mut Rng2(&mut rr).into()) {
                    sum += f * wi.z / pdf;
                }
            }
            let albedo = sum / n as f32;
            // Con el difuso de Ashikhmin–Shirley la suma no crea energía
            assert!(albedo.max_element() < 1.02, "reflectancia {albedo} ({metal},{rough},{coat}) cos {cos_o}");
        }
    }
}

/// La pdf de la mezcla integra ≤ 1 sobre el hemisferio y es coherente con el muestreo.
#[test]
fn bsdf_pdf_integra_uno() {
    let mut r = rng_state();
    for (metal, rough, coat) in CASOS {
        let b = Bsdf::new(Vec3::new(0.7, 0.7, 0.7), metal, rough.max(0.3), coat, 0.4);
        let wo = Vec3::new(0.5, 0.2, 0.84).normalize();
        let n = 200_000;
        let mut acc = 0.0;
        for _ in 0..n {
            let wi = random_dir(&mut r);
            acc += b.eval(wo, wi).1 * 2.0 * std::f32::consts::PI; // pdf uniforme del hemisferio = 1/2π
        }
        let integral = acc / n as f32;
        assert!(integral < 1.05 && integral > 0.5, "∫pdf = {integral} ({metal},{rough},{coat})");
    }
}

// Pequeño adaptador: la API pública pide `Rng`, así que se crea uno por prueba.
struct Rng2<'a>(&'a mut dyn FnMut() -> f32);
impl<'a> From<Rng2<'a>> for geppettoray::Rng {
    fn from(v: Rng2<'a>) -> Self {
        geppettoray::Rng::new((v.0)().to_bits() as u64, 1)
    }
}

fn quad(y: f32, half: f32, mat: u32, uv: bool) -> Vec<Triangle> {
    let (a, b, c, d) = (Vec3::new(-half, y, -half), Vec3::new(half, y, -half), Vec3::new(half, y, half), Vec3::new(-half, y, half));
    let uvs = |q: [Vec2; 3]| if uv { Some(q) } else { None };
    let (ua, ub, uc, ud) = (Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0));
    vec![
        Triangle::new([a, c, b], None, uvs([ua, uc, ub]), mat),
        Triangle::new([a, d, c], None, uvs([ua, ud, uc]), mat),
    ]
}

fn look_down() -> Camera {
    Camera::look_at(Vec3::new(0.0, 5.0, 0.01), Vec3::ZERO, Vec3::Y, 20.0, 1.0)
}

fn settings(env: Vec3) -> Settings {
    Settings {
        width: 8,
        height: 8,
        samples: 32,
        max_depth: 3,
        environment: Environment { color: env },
        ..Default::default()
    }
}

fn mean(img: &Image) -> Vec3 {
    img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32
}

/// Emisión con textura sRGB: un texel rojo (255,0,0) conserva el rojo en lineal.
#[test]
fn textura_base_color() {
    let mat = RayMaterial {
        base_texture: Some(0),
        base_color: Vec4::ONE,
        unlit: true,
        ..Default::default()
    };
    let scene = RayScene::with_textures(quad(0.0, 50.0, 0, true), vec![mat], vec![Some(Texture::solid([255, 128, 0, 255]))]);
    let m = mean(&render(&scene, &look_down(), &settings(Vec3::ZERO)));
    let g = ((128.0f32 / 255.0 + 0.055) / 1.055).powf(2.4); // sRGB → lineal
    assert!((m - Vec3::new(1.0, g, 0.0)).abs().max_element() < 1e-3, "{m}");
}

/// Máscara: con alfa 0 bajo el umbral el plano desaparece y se ve el fondo.
#[test]
fn opacidad_mascara() {
    let mk = |a: f32| {
        let mat = RayMaterial { base_color: Vec4::new(1.0, 0.0, 0.0, a), alpha: RayAlpha::Mask(0.5), unlit: true, ..Default::default() };
        RayScene::new(quad(0.0, 50.0, 0, false), vec![mat])
    };
    let env = Vec3::new(0.0, 0.0, 1.0);
    let hidden = mean(&render(&mk(0.2), &look_down(), &settings(env)));
    let shown = mean(&render(&mk(0.9), &look_down(), &settings(env)));
    assert!((hidden - env).abs().max_element() < 1e-4, "{hidden}");
    assert!((shown - Vec3::X).abs().max_element() < 1e-4, "{shown}");
}

/// Mezcla: alfa 0.25 deja pasar ~75 % de la luz del fondo.
#[test]
fn opacidad_mezcla() {
    let mat = RayMaterial { base_color: Vec4::new(1.0, 0.0, 0.0, 0.25), alpha: RayAlpha::Blend, unlit: true, ..Default::default() };
    let scene = RayScene::new(quad(0.0, 50.0, 0, false), vec![mat]);
    let mut s = settings(Vec3::new(0.0, 0.0, 1.0));
    s.samples = 512;
    let m = mean(&render(&scene, &look_down(), &s));
    assert!((m.x - 0.25).abs() < 0.04 && (m.z - 0.75).abs() < 0.04, "{m}");
}

/// Espejo metálico: refleja el entorno uniforme con su color base (F0 = base).
#[test]
fn metal_espejo_refleja_entorno() {
    let mat = RayMaterial { base_color: Vec4::new(1.0, 0.8, 0.4, 1.0), metallic: 1.0, roughness: 0.0, ..Default::default() };
    let scene = RayScene::new(quad(0.0, 50.0, 0, false), vec![mat]);
    let mut s = settings(Vec3::ONE);
    s.samples = 64;
    let m = mean(&render(&scene, &look_down(), &s));
    assert!((m - Vec3::new(1.0, 0.8, 0.4)).abs().max_element() < 0.03, "{m}");
}

/// Mapa de normales plano (128,128,255) no cambia la imagen.
#[test]
fn mapa_normales_plano_no_cambia() {
    let base = RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO);
    let with_map = RayMaterial { normal_texture: Some(0), ..base };
    let env = Vec3::ONE;
    let s = settings(env);
    let a = mean(&render(&RayScene::new(quad(0.0, 50.0, 0, true), vec![base]), &look_down(), &s));
    let b = mean(&render(
        &RayScene::with_textures(quad(0.0, 50.0, 0, true), vec![with_map], vec![Some(Texture::solid([128, 128, 255, 255]))]),
        &look_down(),
        &s,
    ));
    assert!((a - b).abs().max_element() < 0.02, "{a} vs {b}");
}
