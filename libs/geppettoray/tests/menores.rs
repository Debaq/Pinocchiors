//! Sol y focos con tamaño, suelo con sombra, emisión texturizada como luz de área,
//! vidrio esmerilado y volúmenes anidados.

use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;
use std::sync::atomic::AtomicBool;

fn mean(img: &Image) -> Vec3 {
    img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32
}

fn floor(half: f32, albedo: f32) -> Vec<Triangle> {
    let _ = albedo;
    quad(Vec3::new(-half, 0.0, half), Vec3::X * 2.0 * half, -Vec3::Z * 2.0 * half, 0).to_vec()
}

fn sphere(r: f32, mat: u32) -> Vec<Triangle> {
    let (nu, nv) = (64, 32);
    let pt = |i: usize, j: usize| {
        let th = PI * j as f32 / nv as f32;
        let ph = 2.0 * PI * i as f32 / nu as f32;
        Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin())
    };
    let mut out = Vec::new();
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            for t in [[a, b, c], [a, c, d]] {
                if (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-9 {
                    out.push(Triangle::new(t.map(|p| p * r), Some(t), None, mat));
                }
            }
        }
    }
    out
}

fn cfg(w: u32, samples: u32, depth: u32) -> Settings {
    Settings { width: w, height: w, samples, max_depth: depth, environment: Environment::uniform(Vec3::ZERO), ..Default::default() }
}

fn column_mean(img: &Image, x: usize) -> f32 {
    let w = img.width as usize;
    (0..img.height as usize).map(|y| img.pixels[y * w + x].x).sum::<f32>() / img.height as f32
}

/// Un borde de sombra: con un sol puntual es un escalón; con disco, una penumbra.
#[test]
fn sol_con_disco_hace_penumbra() {
    let mut tris = floor(50.0, 0.5);
    // Techo que cubre x < 0 a 1 de altura (el borde cae justo sobre el origen)
    tris.extend(quad(Vec3::new(-50.0, 1.0, -50.0), Vec3::Z * 100.0, Vec3::X * 50.0, 1));
    let mats = vec![
        RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO),
        RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO) },
    ];
    let scene = RayScene::new(tris, mats);
    // Cámara a un costado y baja: ve el suelo a través del borde del techo
    let cam = Camera::look_at(Vec3::new(0.0, 0.6, 3.0), Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 8.0, 1.0);
    let run = |radius: f32| {
        let mut s = cfg(16, 256, 1);
        s.lights = vec![Light::Sun(Sun { direction: Vec3::Y, color: Vec3::splat(3.0), angular_radius: radius })];
        render(&scene, &cam, &s)
    };
    let hard = run(0.0);
    let soft = run(0.25);
    let lit = hard.pixels.iter().map(|p| p.x).fold(0.0f32, f32::max);
    assert!(lit > 0.1, "debe haber zona iluminada: {lit}");
    // Cuántos píxeles quedan a medias: con sol puntual casi ninguno
    let half = |img: &Image| img.pixels.iter().filter(|p| p.x > 0.15 * lit && p.x < 0.85 * lit).count();
    assert!(half(&soft) > half(&hard) + 8, "penumbra: suave {} vs dura {}", half(&soft), half(&hard));
    let _ = column_mean(&soft, 0);
}

#[test]
fn luz_puntual_esfera_suaviza_la_sombra() {
    let mut tris = floor(50.0, 0.5);
    tris.extend(quad(Vec3::new(-50.0, 1.0, -50.0), Vec3::Z * 100.0, Vec3::X * 50.0, 1));
    let mats = vec![
        RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO),
        RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO) },
    ];
    let scene = RayScene::new(tris, mats);
    let cam = Camera::look_at(Vec3::new(0.0, 0.6, 3.0), Vec3::ZERO, Vec3::Y, 8.0, 1.0);
    let run = |radius: f32| {
        let mut s = cfg(16, 256, 1);
        s.lights = vec![Light::Point { position: Vec3::new(0.0, 3.0, 0.0), intensity: Vec3::splat(40.0), radius }];
        render(&scene, &cam, &s)
    };
    let (hard, soft) = (run(0.0), run(0.5));
    let lit = hard.pixels.iter().map(|p| p.x).fold(0.0f32, f32::max);
    let half = |img: &Image| img.pixels.iter().filter(|p| p.x > 0.15 * lit && p.x < 0.85 * lit).count();
    assert!(half(&soft) > half(&hard) + 8, "penumbra: suave {} vs dura {}", half(&soft), half(&hard));
}

fn catcher_scene() -> (RayScene, Camera, Settings) {
    // Una lámina de 1×1 a 1 de altura da sombra sobre el suelo (que no existe en la escena)
    let tris = quad(Vec3::new(-0.5, 1.0, -0.5), Vec3::X, Vec3::Z, 0).to_vec();
    let scene = RayScene::new(tris, vec![RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO) }]);
    let cam = Camera::look_at(Vec3::new(1.0, 5.0, 3.0), Vec3::new(0.5, 0.0, 0.0), Vec3::Y, 35.0, 1.0);
    let mut s = cfg(32, 64, 2);
    s.transparent_background = true;
    s.lights = vec![Light::Sun(Sun { direction: Vec3::new(-0.5, 1.0, 0.0).normalize(), color: Vec3::splat(3.0), angular_radius: 0.0 })];
    s.shadow_catcher = Some(ShadowCatcher { center: Vec3::ZERO, normal: Vec3::Y, radius: 100.0, strength: 0.7 });
    (scene, cam, s)
}

fn render_frame(scene: &RayScene, cam: &Camera, s: &Settings) -> Frame {
    let opts = ProgressiveOptions { passes: 1, tile: 16, denoise: None, denoise_previews: false };
    render_progressive(scene, cam, s, &opts, &AtomicBool::new(false), |_, _| {}).frame
}

/// El suelo es invisible salvo donde cae la sombra, con la opacidad pedida y sin color.
#[test]
fn suelo_con_sombra_solo_muestra_la_sombra() {
    let (scene, cam, s) = catcher_scene();
    let f = render_frame(&scene, &cam, &s);
    let alpha = f.alpha.as_ref().expect("fondo transparente");
    let shadow = alpha.iter().filter(|&&a| (a - 0.7).abs() < 0.03).count();
    let clear = alpha.iter().filter(|&&a| a < 0.02).count();
    assert!(shadow > 6, "debe haber sombra de opacidad 0,7: {shadow}");
    assert!(clear > 300, "el resto del suelo es invisible: {clear}");
    // La sombra pura no tiene color (los bordes del objeto mezclan su color, no cuentan)
    let black = f.image.pixels.iter().zip(alpha).filter(|(p, a)| (**a - 0.7).abs() < 0.03 && p.max_element() < 1e-3).count();
    assert!(black > 6, "sombra sin color: {black}");
}

/// Sin fondo transparente la sombra se mezcla con el fondo visible.
#[test]
fn suelo_con_sombra_sobre_color_de_fondo() {
    let (scene, cam, mut s) = catcher_scene();
    s.transparent_background = false;
    s.background = Some(Vec3::ONE);
    let f = render_frame(&scene, &cam, &s);
    assert!(f.alpha.is_none());
    let dark = f.image.pixels.iter().filter(|p| (p.x - 0.3).abs() < 0.03).count();
    let white = f.image.pixels.iter().filter(|p| p.x > 0.99).count();
    assert!(dark > 6 && white > 300, "sombra {dark}, fondo {white}");
}

/// Con luz de cielo, la sombra del sol es más suave que sin ella.
#[test]
fn suelo_con_sombra_se_aclara_con_ambiente() {
    let (scene, cam, mut s) = catcher_scene();
    s.environment = Environment::uniform(Vec3::splat(1.0));
    let f = render_frame(&scene, &cam, &s);
    let alpha = f.alpha.unwrap();
    let darkest = alpha
        .iter()
        .zip(&f.image.pixels)
        .filter(|(a, p)| **a < 0.75 && p.max_element() < 1e-3)
        .map(|(a, _)| *a)
        .fold(0.0f32, f32::max);
    assert!(darkest < 0.69 && darkest > 0.1, "sombra con cielo: {darkest}");
}

/// La mitad blanca / mitad negra de una textura de emisión da la mitad de luz.
#[test]
fn emision_texturizada_ilumina_por_muestreo() {
    let big = 3000.0;
    let build = |textured: bool| {
        let mut tris = floor(50.0, 0.5);
        tris.extend(quad(Vec3::new(-big, 1.0, -big), Vec3::X * 2.0 * big, Vec3::Z * 2.0 * big, 1));
        let mut emitter = RayMaterial { double_sided: false, ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(2.0)) };
        let mut textures = Vec::new();
        if textured {
            textures.push(Some(Texture { width: 2, height: 1, rgba: vec![[255, 255, 255, 255], [0, 0, 0, 255]] }));
            emitter.emissive_texture = Some(0);
        }
        RayScene::with_textures(tris, vec![RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO), emitter], textures)
    };
    let cam = Camera::look_at(Vec3::new(0.0, 0.5, 0.001), Vec3::ZERO, Vec3::Y, 2.0, 1.0);
    let s = cfg(8, 1024, 1);
    let full = mean(&render(&build(false), &cam, &s)).x;
    let half = mean(&render(&build(true), &cam, &s)).x;
    assert!((half / full - 0.5).abs() < 0.04, "{half} / {full}");
}

/// Con la emisión texturizada sampleada hay menos ruido que dejando solo el rebote del BSDF.
#[test]
fn emision_texturizada_es_luz_de_area() {
    let mut tris = floor(50.0, 0.5);
    tris.extend(quad(Vec3::new(-0.5, 2.0, -0.5), Vec3::X, Vec3::Z, 1));
    let emitter = RayMaterial { double_sided: true, emissive_texture: Some(0), ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(50.0)) };
    let scene = RayScene::with_textures(
        tris,
        vec![RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO), emitter],
        vec![Some(Texture::solid([255, 255, 255, 255]))],
    );
    assert!(!scene.area_lights.is_empty(), "el emisor texturizado entra en las luces de área");
}

fn glass(base: Vec3, ior: f32, thickness: f32, roughness: f32) -> RayMaterial {
    RayMaterial { base_color: base.extend(1.0), metallic: 0.0, roughness, transmission: 1.0, ior, thickness, ..Default::default() }
}

fn furnace(env: f32, samples: u32) -> Settings {
    Settings { width: 16, height: 16, samples, max_depth: 16, environment: Environment::uniform(Vec3::splat(env)), ..Default::default() }
}

/// Un líquido dentro de un vaso (dos volúmenes) sin absorción no pierde ni gana energía.
#[test]
fn horno_blanco_con_volumenes_anidados() {
    let mut tris = sphere(1.0, 0);
    tris.extend(sphere(0.6, 1));
    let scene = RayScene::new(tris, vec![glass(Vec3::ONE, 1.5, 2.0, 0.0), glass(Vec3::ONE, 1.33, 1.0, 0.0)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0);
    let m = mean(&render(&scene, &cam, &furnace(1.0, 128)));
    assert!((m - Vec3::ONE).abs().max_element() < 0.03, "{m}");
}

/// El líquido absorbente dentro de un vaso transparente oscurece solo lo que lo atraviesa.
#[test]
fn liquido_absorbente_dentro_del_vaso() {
    let make = |liquid_tint: f32| {
        let mut tris = sphere(1.0, 0);
        tris.extend(sphere(0.6, 1));
        RayScene::new(tris, vec![glass(Vec3::ONE, 1.5, 2.0, 0.0), glass(Vec3::splat(liquid_tint), 1.33, 1.2, 0.0)])
    };
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 3.0, 1.0);
    let clear = mean(&render(&make(1.0), &cam, &furnace(1.0, 64))).x;
    let dyed = mean(&render(&make(0.3), &cam, &furnace(1.0, 64))).x;
    assert!(clear > 0.95, "{clear}");
    assert!(dyed < 0.7 * clear, "teñido {dyed} vs claro {clear}");
}

/// El vidrio esmerilado casi conserva la energía y difumina lo que hay detrás.
#[test]
fn vidrio_esmerilado() {
    let scene = RayScene::new(sphere(1.0, 0), vec![glass(Vec3::ONE, 1.5, 2.0, 0.4)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0);
    let m = mean(&render(&scene, &cam, &furnace(1.0, 256))).x;
    assert!(m > 0.85 && m < 1.02, "horno blanco esmerilado: {m}");

    // Detrás, una pared emisiva con un solo punto brillante muy chico: liso = nítido, esmerilado = difuso
    let wall = |tris: &mut Vec<Triangle>| {
        tris.extend(quad(Vec3::new(-20.0, -20.0, -10.0), Vec3::X * 40.0, Vec3::Y * 40.0, 1));
        tris.extend(quad(Vec3::new(-0.1, -0.1, -9.9), Vec3::X * 0.2, Vec3::Y * 0.2, 2));
    };
    let mats = |r: f32| {
        vec![
            glass(Vec3::ONE, 1.5, 2.0, r),
            RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(0.05)) },
            RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(500.0)) },
        ]
    };
    let spread = |r: f32| {
        let mut tris = sphere(1.0, 0);
        wall(&mut tris);
        let scene = RayScene::new(tris, mats(r));
        let cam = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y, 30.0, 1.0);
        let s = Settings { width: 32, height: 32, samples: 128, max_depth: 8, environment: Environment::uniform(Vec3::ZERO), ..Default::default() };
        let img = render(&scene, &cam, &s);
        // Cuántos píxeles de la esfera reciben algo del punto brillante
        img.pixels.iter().filter(|p| p.x > 0.5).count()
    };
    assert!(spread(0.5) > spread(0.0), "esmerilado {} vs liso {}", spread(0.5), spread(0.0));
}

/// ACES: el negro sigue negro, sube de forma monótona y comprime los brillos sin pasar de 1.
#[test]
fn tono_aces() {
    let aces = ToneMap::Aces { exposure: 1.0 };
    assert!(aces.apply(Vec3::ZERO).max_element() < 1e-3);
    let mut prev = 0.0;
    for k in 0..40 {
        let v = aces.apply(Vec3::splat(k as f32 * 0.25)).x;
        assert!(v >= prev - 1e-6 && v <= 1.0, "{k}: {v}");
        prev = v;
    }
    // Gris medio de three.js (0,18 → ~0,26 lineal tras la curva)
    let mid = aces.apply(Vec3::splat(0.18)).x;
    assert!(mid > 0.2 && mid < 0.35, "{mid}");
    assert_eq!(ToneMap::Linear.apply(Vec3::splat(0.5)), Vec3::splat(0.5));
}
