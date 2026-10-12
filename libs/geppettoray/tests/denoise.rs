use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;

fn sphere(center: Vec3, r: f32, mat: u32) -> Vec<Triangle> {
    let (nu, nv) = (48, 24);
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
                    out.push(Triangle::new(t.map(|p| p * r + center), Some(t), None, mat));
                }
            }
        }
    }
    out
}

fn mse(a: &Image, b: &Image) -> f32 {
    a.pixels.iter().zip(&b.pixels).map(|(p, q)| (*p - *q).length_squared()).sum::<f32>() / a.pixels.len() as f32
}

fn settings(samples: u32, w: u32) -> Settings {
    Settings {
        width: w,
        height: w,
        samples,
        max_depth: 3,
        environment: Environment::uniform(Vec3::splat(0.4)),
        lights: vec![Light::Sun(Sun { direction: Vec3::new(0.4, 1.0, 0.5).normalize(), color: Vec3::splat(3.0) })],
        ..Default::default()
    }
}

fn scene() -> (RayScene, Camera) {
    let mut tris = quad(Vec3::new(-6.0, -1.0, 6.0), Vec3::X * 12.0, -Vec3::Z * 12.0, 0).to_vec();
    tris.extend(sphere(Vec3::ZERO, 1.0, 1));
    let mats = vec![
        RayMaterial::diffuse(Vec3::splat(0.7), Vec3::ZERO),
        RayMaterial::diffuse(Vec3::new(0.8, 0.3, 0.2), Vec3::ZERO),
    ];
    (RayScene::new(tris, mats), Camera::look_at(Vec3::new(0.0, 1.5, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0))
}

/// Con pocas muestras el denoise acerca la imagen a la de referencia de muchas.
#[test]
fn reduce_el_error_contra_la_referencia() {
    let (sc, cam) = scene();
    let w = 48;
    let reference = render(&sc, &cam, &Settings { seed: 99, ..settings(512, w) });
    let aov = render_aov(&sc, &cam, &settings(4, w));
    let noisy = Image { width: aov.width, height: aov.height, pixels: aov.color.clone() };
    let clean = denoise(&aov, &DenoiseSettings::default());
    let (e0, e1) = (mse(&noisy, &reference), mse(&clean, &reference));
    assert!(e1 < 0.6 * e0, "error {e0} → {e1}");
}

/// El color de render_aov es el de render con la misma semilla.
#[test]
fn aov_no_cambia_el_color() {
    let (sc, cam) = scene();
    let s = settings(4, 16);
    let a = render(&sc, &cam, &s);
    let b = render_aov(&sc, &cam, &s);
    assert!(a.pixels.iter().zip(&b.color).all(|(p, q)| (*p - *q).length() < 1e-6));
    assert!(b.normal.iter().any(|n| *n != Vec3::ZERO) && b.depth.iter().any(|d| d.is_finite()));
}

/// Una imagen sin ruido (sin luz indirecta ni muestreo aleatorio) queda igual.
#[test]
fn imagen_limpia_no_cambia() {
    let (sc, cam) = scene();
    let mut s = settings(1, 32);
    s.max_depth = 0;
    s.environment = Environment::uniform(Vec3::ZERO);
    let aov = render_aov(&sc, &cam, &s);
    let clean = denoise(&aov, &DenoiseSettings::default());
    let noisy = Image { width: 32, height: 32, pixels: aov.color.clone() };
    assert!(mse(&noisy, &clean) < 1e-3, "{}", mse(&noisy, &clean));
}

/// No mezcla a través de un borde de geometría: el rincón de una caja conserva el contraste.
#[test]
fn conserva_los_bordes_de_geometria() {
    // Piso (+Y) y pared (+Z) con el sol de lado: dos tonos distintos
    let mut tris = quad(Vec3::new(-5.0, 0.0, 5.0), Vec3::X * 10.0, -Vec3::Z * 10.0, 0).to_vec();
    tris.extend(quad(Vec3::new(-5.0, 0.0, -5.0), Vec3::X * 10.0, Vec3::Y * 10.0, 0));
    let sc = RayScene::new(tris, vec![RayMaterial::diffuse(Vec3::splat(0.7), Vec3::ZERO)]);
    let cam = Camera::look_at(Vec3::new(0.0, 4.0, 8.0), Vec3::new(0.0, 0.0, -5.0), Vec3::Y, 50.0, 1.0);
    let w = 48;
    let mut s = settings(4, w);
    s.lights = vec![Light::Sun(Sun { direction: Vec3::new(0.0, 1.0, 0.1).normalize(), color: Vec3::splat(3.0) })];
    let reference = render(&sc, &cam, &Settings { seed: 5, samples: 256, ..s.clone() });
    let aov = render_aov(&sc, &cam, &s);
    let clean = denoise(&aov, &DenoiseSettings::default());
    // Franja de ±2 píxeles a cada lado del borde: el denoise no debe correrse hacia el otro tono
    let (mut err_clean, mut err_noisy, mut bias_c, mut bias_r) = (0.0f32, 0.0f32, Vec3::ZERO, Vec3::ZERO);
    let mut band = 0;
    for y in 0..w as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            let here = aov.normal[i];
            let near_edge = (-2i64..=2).any(|d| {
                let yy = y as i64 + d;
                yy >= 0 && (yy as usize) < w as usize && (aov.normal[yy as usize * w as usize + x] - here).length() > 0.5
            });
            if near_edge {
                band += 1;
                err_clean += (clean.pixels[i] - reference.pixels[i]).length();
                err_noisy += (aov.color[i] - reference.pixels[i]).length();
                bias_c += clean.pixels[i];
                bias_r += reference.pixels[i];
            }
        }
    }
    assert!(band > 20, "no se halló el borde");
    println!("borde: error medio {} → {}, brillo medio {:?} vs {:?}", err_noisy / band as f32, err_clean / band as f32, bias_c / band as f32, bias_r / band as f32);
    assert!(err_clean < err_noisy, "no mejora en el borde");
    // Sin sangrado: el brillo medio de la franja no se corre hacia el otro tono
    assert!(((bias_c - bias_r) / band as f32).length() < 0.05, "la franja se corrió {:?}", (bias_c - bias_r) / band as f32);
}

/// Las texturas no se desdibujan: el contraste de rayas se mantiene.
#[test]
fn conserva_las_texturas() {
    let tex = Texture { width: 8, height: 1, rgba: (0..8).map(|i| if i % 2 == 0 { [255, 255, 255, 255] } else { [40, 40, 40, 255] }).collect() };
    let mat = RayMaterial { base_texture: Some(0), base_color: glam::Vec4::ONE, lambert: true, ..Default::default() };
    // Plano con rayas bajo una luz de área, con una esfera que da penumbra ruidosa
    let mut tris = quad(Vec3::new(-2.0, 0.0, 2.0), Vec3::X * 4.0, -Vec3::Z * 4.0, 0).to_vec();
    tris.extend(quad(Vec3::new(-1.0, 3.0, -1.0), Vec3::X * 2.0, Vec3::Z * 2.0, 1)); // emisor mirando abajo
    tris.extend(sphere(Vec3::new(0.5, 1.2, 0.0), 0.5, 2));
    let emitter = RayMaterial { double_sided: false, ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(8.0)) };
    let sc = RayScene::with_textures(
        tris,
        vec![mat, emitter, RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO)],
        vec![Some(tex)],
    );
    let cam = Camera::look_at(Vec3::new(0.0, 6.0, 0.01), Vec3::ZERO, Vec3::Y, 30.0, 1.0);
    let w = 64;
    let mut s = settings(4, w);
    s.lights.clear();
    s.environment = Environment::uniform(Vec3::ZERO);
    let reference = render(&sc, &cam, &Settings { seed: 8, samples: 256, ..s.clone() });
    let aov = render_aov(&sc, &cam, &s);
    let clean = denoise(&aov, &DenoiseSettings::default());
    let noisy = Image { width: w, height: w, pixels: aov.color.clone() };
    println!("texturas: mse {} → {}", mse(&noisy, &reference), mse(&clean, &reference));
    assert!(mse(&clean, &reference) < 0.8 * mse(&noisy, &reference));
    // Contraste horizontal (rayas): desviación típica de una fila central
    let row = |im: &Image| {
        let r: Vec<f32> = (0..w as usize).map(|x| im.pixels[(w as usize / 8) * w as usize + x].x).collect();
        let m = r.iter().sum::<f32>() / r.len() as f32;
        (r.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / r.len() as f32).sqrt()
    };
    println!("contraste {} vs {}", row(&clean), row(&reference));
    assert!(row(&clean) > 0.8 * row(&reference), "contraste {} vs {}", row(&clean), row(&reference));
}

/// Una luciérnaga aislada se recorta.
#[test]
fn recorta_luciernagas() {
    let (w, h) = (16usize, 16usize);
    let n = w * h;
    let mut color = vec![Vec3::splat(0.5); n];
    let mut variance = vec![0.0004f32; n];
    color[8 * w + 8] = Vec3::splat(200.0);
    variance[8 * w + 8] = 40_000.0 / 4.0;
    let aov = Aov {
        width: w as u32,
        height: h as u32,
        color,
        albedo: vec![Vec3::ONE; n],
        normal: vec![Vec3::Y; n],
        position: (0..n).map(|i| Vec3::new((i % w) as f32 * 0.01, 0.0, (i / w) as f32 * 0.01)).collect(),
        depth: vec![5.0; n],
        variance,
        pixel_angle: 0.002,
    };
    let out = denoise(&aov, &DenoiseSettings::default());
    assert!(out.pixels[8 * w + 8].x < 1.0, "{}", out.pixels[8 * w + 8].x);
}

