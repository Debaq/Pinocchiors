//! Benchmarks de rendimiento de geppettoray (path tracer en CPU). No son pruebas
//! funcionales: van con `#[ignore = "perf"]` y se ejecutan a mano.
//!
//! Comando (el perfil release tiene LTO y tarda muchísimo, por eso se desactiva):
//!
//!   CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!     cargo test --release -p geppettoray --test perf -- --ignored --nocapture --test-threads=1
//!
//! Escalado con hilos: repetir con `RAYON_NUM_THREADS=1` y sin la variable.
//! Tamaño: `PERF_SCALE=small` (humo, diminuto); por defecto ~270k triángulos y 512x512.
//! Cada benchmark hace 1 iteración de calentamiento + mediana de 3 repeticiones e
//! imprime: `PERF crate=geppettoray bench=<nombre> threads=<n> tris=<n> ms=<mediana>`.
//! Cada uno verifica un resultado sensato (no medir un camino vacío).

use geppettoray::*;
use glam::Vec3;
use std::time::Instant;

fn small() -> bool {
    std::env::var("PERF_SCALE").is_ok_and(|v| v == "small")
}

/// Mediana de 3 repeticiones tras un calentamiento; imprime la línea PERF.
fn bench<R>(name: &str, tris: usize, mut f: impl FnMut() -> R) -> R {
    let mut last = f();
    let mut ms = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        last = f();
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    println!("PERF crate=geppettoray bench={name} threads={} tris={tris} ms={:.2}", rayon::current_num_threads(), ms[1]);
    last
}

/// Esfera-cubo ondulada (determinista), `n`×`n` quads por cara = 12·n² triángulos.
fn bumpy_sphere(n: usize, material: u32) -> Vec<Triangle> {
    let pt = |axis: usize, sign: f32, i: usize, j: usize| {
        let (u, v) = (2.0 * i as f32 / n as f32 - 1.0, 2.0 * j as f32 / n as f32 - 1.0);
        let mut p = [0.0f32; 3];
        p[axis] = sign;
        p[(axis + 1) % 3] = u * sign;
        p[(axis + 2) % 3] = v;
        let d = Vec3::from(p).normalize();
        d * (1.0 + 0.04 * (d.x * 9.0).sin() * (d.y * 7.0).cos() * (d.z * 11.0).sin())
    };
    let mut out = Vec::with_capacity(12 * n * n);
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            for j in 0..n {
                for i in 0..n {
                    let (a, b, c, d) = (pt(axis, sign, i, j), pt(axis, sign, i + 1, j), pt(axis, sign, i + 1, j + 1), pt(axis, sign, i, j + 1));
                    for t in [[a, b, c], [a, c, d]] {
                        let nrm = (t[1] - t[0]).cross(t[2] - t[0]);
                        if nrm.length() < 1e-12 {
                            continue;
                        }
                        // Orientación hacia afuera
                        let t = if nrm.dot(t[0]) < 0.0 { [t[0], t[2], t[1]] } else { t };
                        out.push(Triangle::new(t, Some(t.map(|p| p.normalize())), None, material));
                    }
                }
            }
        }
    }
    out
}

/// Esfera ondulada + suelo difuso + lámpara de área emisiva.
fn triangles() -> Vec<Triangle> {
    let n = if small() { 8 } else { 150 }; // 270k triángulos
    let mut tris = bumpy_sphere(n, 0);
    tris.extend(quad(Vec3::new(-6.0, -1.2, -6.0), Vec3::new(12.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 12.0), 1));
    tris.extend(quad(Vec3::new(-1.0, 3.0, -1.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 2.0), 2));
    tris
}

fn materials() -> Vec<RayMaterial> {
    vec![
        RayMaterial::diffuse(Vec3::new(0.8, 0.5, 0.3), Vec3::ZERO),
        RayMaterial::diffuse(Vec3::splat(0.7), Vec3::ZERO),
        RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(8.0)),
    ]
}

fn scene() -> RayScene {
    RayScene::new(triangles(), materials())
}

fn camera(aspect: f32) -> Camera {
    Camera::look_at(Vec3::new(2.5, 1.5, 4.0), Vec3::ZERO, Vec3::Y, 45.0, aspect)
}

fn settings(samples: u32) -> Settings {
    let side = if small() { 48 } else { 512 };
    Settings {
        width: side,
        height: side,
        samples,
        max_depth: 4,
        environment: Environment::uniform(Vec3::splat(0.3)),
        lights: vec![Light::Sun(Sun { direction: Vec3::new(0.4, 1.0, 0.5).normalize(), color: Vec3::splat(2.0), angular_radius: 0.0 })],
        ..Default::default()
    }
}

fn assert_lit(img: &Image) {
    let mean = img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32;
    assert!(mean.is_finite() && mean.max_element() > 0.01, "imagen negra o NaN: media {mean}");
}

/// Construcción de la BVH sola (sobre los triángulos).
#[test]
#[ignore = "perf"]
fn bvh_build() {
    let tris = triangles();
    let b = bench("bvh_build", tris.len(), || Bvh::build(&tris));
    let ext = b.bounds();
    assert!(ext.max.x > ext.min.x && ext.max.y > ext.min.y);
    let ray = Ray { origin: Vec3::new(0.13, 0.21, 10.0), dir: -Vec3::Z };
    assert!(b.closest(&tris, &ray, f32::INFINITY).is_some(), "la BVH no golpea la esfera");
}

/// `RayScene::new`: BVH + luces de área + materiales (lo que hace la app al preparar).
#[test]
#[ignore = "perf"]
fn scene_build() {
    let n = triangles().len();
    let s = bench("scene_build", n, || RayScene::new(triangles(), materials()));
    assert!(!s.area_lights.is_empty(), "debería haber luz de área");
}

fn render_bench(name: &str, spp: u32) {
    let scene = scene();
    let s = settings(spp);
    let cam = camera(1.0);
    let img = bench(name, scene.triangles.len(), || render(&scene, &cam, &s));
    assert_eq!(img.pixels.len(), (s.width * s.height) as usize);
    assert_lit(&img);
}

#[test]
#[ignore = "perf"]
fn render_4spp() {
    render_bench("render_4spp", 4);
}

#[test]
#[ignore = "perf"]
fn render_32spp() {
    render_bench("render_32spp", 32);
}

/// Render de capas auxiliares (albedo/normal/profundidad) a 4 spp.
#[test]
#[ignore = "perf"]
fn render_aov_4spp() {
    let scene = scene();
    let s = settings(4);
    let cam = camera(1.0);
    let aov = bench("render_aov_4spp", scene.triangles.len(), || render_aov(&scene, &cam, &s));
    assert!(aov.normal.iter().any(|n| *n != Vec3::ZERO), "sin normales");
    assert!(aov.depth.iter().any(|d| d.is_finite()), "sin profundidad");
}

/// Denoise à-trous (solo el filtro; el AOV se renderiza una vez fuera de la medición).
#[test]
#[ignore = "perf"]
fn denoise_only() {
    let scene = scene();
    let s = settings(4);
    let aov = render_aov(&scene, &camera(1.0), &s);
    let img = bench("denoise", scene.triangles.len(), || denoise(&aov, &DenoiseSettings::default()));
    assert_eq!(img.pixels.len(), aov.color.len());
    assert_lit(&img);
}
