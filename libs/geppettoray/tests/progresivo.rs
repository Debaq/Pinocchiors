use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};

fn sphere(r: f32) -> Vec<Triangle> {
    let (nu, nv) = (32, 16);
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
                    out.push(Triangle::new(t.map(|p| p * r), Some(t), None, 0));
                }
            }
        }
    }
    out
}

fn setup(samples: u32) -> (RayScene, Camera, Settings) {
    let sc = RayScene::new(sphere(1.0), vec![RayMaterial::diffuse(Vec3::new(0.8, 0.4, 0.3), Vec3::ZERO)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 30.0, 1.0);
    let s = Settings {
        width: 40,
        height: 40,
        samples,
        max_depth: 3,
        environment: Environment::uniform(Vec3::splat(0.5)),
        lights: vec![Light::Sun(Sun { direction: Vec3::new(0.5, 1.0, 0.7).normalize(), color: Vec3::splat(3.0) })],
        ..Default::default()
    };
    (sc, cam, s)
}

fn mse(a: &Image, b: &Image) -> f32 {
    a.pixels.iter().zip(&b.pixels).map(|(p, q)| (*p - *q).length_squared()).sum::<f32>() / a.pixels.len() as f32
}

fn no_denoise(passes: u32) -> ProgressiveOptions {
    ProgressiveOptions { passes, denoise: None, ..Default::default() }
}

#[test]
fn pasadas_acumulan_y_refinan() {
    let (sc, cam, s) = setup(64);
    let reference = render(&sc, &cam, &Settings { samples: 1024, seed: 77, ..s.clone() });
    let cancel = AtomicBool::new(false);
    let mut infos = Vec::new();
    let mut errors = Vec::new();
    let out = render_progressive(&sc, &cam, &s, &no_denoise(4), &cancel, |i, f| {
        infos.push(*i);
        errors.push(mse(&f.image, &reference));
    });
    assert!(!out.cancelled && out.samples == 64);
    assert_eq!(infos.len(), 4);
    assert!(infos.windows(2).all(|w| w[1].samples > w[0].samples && w[1].elapsed_secs >= w[0].elapsed_secs));
    assert_eq!(infos.last().unwrap().eta_secs, 0.0);
    assert!(infos[0].eta_secs > 0.0 && infos[0].eta_secs.is_finite());
    assert!(errors[3] < errors[0], "el error debe bajar: {errors:?}");
}

#[test]
fn mismas_muestras_mismo_resultado_que_render() {
    let (sc, cam, s) = setup(32);
    let cancel = AtomicBool::new(false);
    let a = render_progressive(&sc, &cam, &s, &no_denoise(1), &cancel, |_, _| {}).frame;
    let b = render_progressive(&sc, &cam, &s, &no_denoise(1), &cancel, |_, _| {}).frame;
    assert!(a.image.pixels == b.image.pixels, "reproducible");
    // Estadísticamente igual a `render` con otras semillas
    let r = render(&sc, &cam, &Settings { seed: 5, ..s.clone() });
    let (ma, mr) = (
        a.image.pixels.iter().sum::<Vec3>() / 1600.0,
        r.pixels.iter().sum::<Vec3>() / 1600.0,
    );
    assert!((ma - mr).abs().max_element() < 0.01, "{ma} vs {mr}");
}

#[test]
fn cancelar_devuelve_lo_acumulado() {
    let (sc, cam, s) = setup(64);
    let cancel = AtomicBool::new(false);
    let out = render_progressive(&sc, &cam, &s, &no_denoise(8), &cancel, |i, _| {
        if i.pass == 2 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(out.cancelled);
    assert_eq!(out.samples, 16, "2 pasadas de 8 muestras");
    assert!(out.frame.image.pixels.iter().any(|p| p.max_element() > 0.1), "devuelve lo acumulado");
}

#[test]
fn cancelar_antes_de_empezar() {
    let (sc, cam, s) = setup(16);
    let cancel = AtomicBool::new(true);
    let mut calls = 0;
    let out = render_progressive(&sc, &cam, &s, &no_denoise(4), &cancel, |_, _| calls += 1);
    assert!(out.cancelled && out.samples == 0 && calls == 0);
}

#[test]
fn fondo_transparente() {
    let (sc, cam, mut s) = setup(32);
    let cancel = AtomicBool::new(false);
    let opaque = render_progressive(&sc, &cam, &s, &no_denoise(2), &cancel, |_, _| {}).frame;
    assert!(opaque.alpha.is_none());
    s.transparent_background = true;
    let f = render_progressive(&sc, &cam, &s, &no_denoise(2), &cancel, |_, _| {}).frame;
    let alpha = f.alpha.as_ref().unwrap();
    // Esquina: fondo; centro: esfera
    assert!(alpha[0] < 1e-6 && alpha[20 * 40 + 20] > 0.999, "{} {}", alpha[0], alpha[20 * 40 + 20]);
    // Borde de la esfera: hay píxeles parcialmente cubiertos
    assert!(alpha.iter().any(|a| *a > 0.05 && *a < 0.95));
    // El color de la esfera sin premultiplicar coincide con el render opaco
    let c = f.image.pixels[20 * 40 + 20];
    assert!((c - opaque.image.pixels[20 * 40 + 20]).abs().max_element() < 0.05);
    // PNG RGBA con el alfa
    let rgba = f.to_srgb8_rgba();
    assert_eq!(rgba.len(), 40 * 40 * 4);
    assert_eq!(rgba[3], 0);
    assert_eq!(rgba[(20 * 40 + 20) * 4 + 3], 255);
}

#[test]
fn con_denoise_final() {
    let (sc, cam, s) = setup(8);
    let reference = render(&sc, &cam, &Settings { samples: 1024, seed: 77, ..s.clone() });
    let cancel = AtomicBool::new(false);
    let raw = render_progressive(&sc, &cam, &s, &no_denoise(2), &cancel, |_, _| {}).frame;
    let den = render_progressive(&sc, &cam, &s, &ProgressiveOptions { passes: 2, ..Default::default() }, &cancel, |_, _| {}).frame;
    assert!(mse(&den.image, &reference) < mse(&raw.image, &reference));
}
