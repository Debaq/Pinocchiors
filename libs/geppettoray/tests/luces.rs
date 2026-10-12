use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;

fn mean(img: &Image) -> Vec3 {
    img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32
}

/// Plano receptor difuso (normal +Y) de lado 2·half
fn floor(half: f32, albedo: f32) -> (Vec<Triangle>, RayMaterial) {
    let m = RayMaterial::diffuse(Vec3::splat(albedo), Vec3::ZERO);
    (quad(Vec3::new(-half, 0.0, half), Vec3::X * 2.0 * half, -Vec3::Z * 2.0 * half, 0).to_vec(), m)
}

fn small_view(y: f32) -> Camera {
    Camera::look_at(Vec3::new(0.0, y, 0.001), Vec3::ZERO, Vec3::Y, 2.0, 1.0)
}

fn st(env: Environment, lights: Vec<Light>, samples: u32, depth: u32) -> Settings {
    Settings { width: 8, height: 8, samples, max_depth: depth, environment: env, lights, ..Default::default() }
}

#[test]
fn luz_puntual_ley_cuadrado_inverso() {
    let (tris, m) = floor(50.0, 0.5);
    let scene = RayScene::new(tris, vec![m]);
    let lights = vec![Light::Point { position: Vec3::new(0.0, 2.0, 0.0), intensity: Vec3::splat(10.0) }];
    let img = render(&scene, &small_view(5.0), &st(Environment::uniform(Vec3::ZERO), lights, 4, 1));
    let expected = 0.5 / PI * 10.0 / 4.0;
    assert!((mean(&img).x / expected - 1.0).abs() < 0.06, "{} vs {expected}", mean(&img).x);
}

#[test]
fn foco_respeta_el_cono() {
    let (tris, m) = floor(50.0, 0.5);
    let scene = RayScene::new(tris, vec![m]);
    let spot = |dir: Vec3| {
        vec![Light::Spot {
            position: Vec3::new(0.0, 2.0, 0.0),
            direction: dir,
            intensity: Vec3::splat(10.0),
            inner: 0.3,
            outer: 0.5,
        }]
    };
    let s = |d| mean(&render(&scene, &small_view(5.0), &st(Environment::uniform(Vec3::ZERO), spot(d), 4, 1))).x;
    assert!(s(-Vec3::Y) > 0.3, "dentro del cono");
    assert!(s(Vec3::new(1.0, -0.2, 0.0)) < 1e-4, "fuera del cono");
}

/// Un emisor enorme sobre el piso se comporta como cielo uniforme: Lo = a·L.
#[test]
fn luz_de_area_plano_infinito() {
    let (mut tris, floor_m) = floor(50.0, 0.5);
    let big = 3000.0;
    // Emisor a y=1, cara frontal hacia abajo (u × v = -Y con u=X, v=Z)
    tris.extend(quad(Vec3::new(-big, 1.0, -big), Vec3::X * 2.0 * big, Vec3::Z * 2.0 * big, 1));
    let emitter = RayMaterial { double_sided: false, ..RayMaterial::diffuse(Vec3::ZERO, Vec3::splat(2.0)) };
    let scene = RayScene::new(tris, vec![floor_m, emitter]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.5, 0.001), Vec3::ZERO, Vec3::Y, 2.0, 1.0);
    let img = render(&scene, &cam, &st(Environment::uniform(Vec3::ZERO), vec![], 512, 1));
    assert!((mean(&img).x / 1.0 - 1.0).abs() < 0.04, "media {}", mean(&img).x);
}

fn varied_map() -> EnvMap {
    let (w, h) = (32, 16);
    let px = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            // Un "sol" muy brillante y un cielo suave
            if (x, y) == (20, 4) { Vec3::new(500.0, 450.0, 400.0) } else { Vec3::new(0.2, 0.3, 0.5) * (1.0 + y as f32 / h as f32) }
        })
        .collect();
    EnvMap::from_rgb(w, h, px, 1.0, 0.7).unwrap()
}

/// pdf y muestreo coinciden y la pdf integra 1 sobre la esfera.
#[test]
fn hdri_pdf_normalizada_y_coherente() {
    let map = varied_map();
    let mut rng = Rng::new(3, 4);
    let n = 400_000;
    let mut integral = 0.0;
    for _ in 0..n {
        let z = rng.next_f32() * 2.0 - 1.0;
        let phi = 2.0 * PI * rng.next_f32();
        let r = (1.0 - z * z).sqrt();
        integral += map.pdf(Vec3::new(r * phi.cos(), z, r * phi.sin())) * 4.0 * PI;
    }
    let integral = integral / n as f32;
    assert!((integral - 1.0).abs() < 0.08, "∫pdf = {integral}");
    for _ in 0..200 {
        let (d, _, pdf) = map.sample(&mut rng).unwrap();
        assert!((map.pdf(d) / pdf - 1.0).abs() < 1e-4);
    }
}

/// E[L / pdf] con muestreo por importancia = ∫L dω, y coincide con el muestreo uniforme.
#[test]
fn hdri_muestreo_importancia_insesgado() {
    let map = varied_map();
    let mut rng = Rng::new(9, 9);
    let n = 200_000;
    let mut imp = Vec3::ZERO;
    let mut uni = Vec3::ZERO;
    for _ in 0..n {
        let (_, l, pdf) = map.sample(&mut rng).unwrap();
        imp += l / pdf;
        let z = rng.next_f32() * 2.0 - 1.0;
        let phi = 2.0 * PI * rng.next_f32();
        let r = (1.0 - z * z).sqrt();
        uni += map.eval(Vec3::new(r * phi.cos(), z, r * phi.sin())) * 4.0 * PI;
    }
    let (imp, uni) = (imp / n as f32, uni / n as f32);
    assert!((imp / uni - Vec3::ONE).abs().max_element() < 0.15, "{imp} vs {uni}");
}

/// Horno blanco con HDRI constante: pasa por muestreo del entorno + MIS y debe dar 1.
#[test]
fn horno_blanco_con_hdri_y_mis() {
    let map = EnvMap::from_rgb(16, 8, vec![Vec3::ONE; 128], 1.0, 0.0).unwrap();
    let sphere = {
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
                        out.push(Triangle::new(t, Some(t), None, 0));
                    }
                }
            }
        }
        out
    };
    let scene = RayScene::new(sphere, vec![RayMaterial::diffuse(Vec3::ONE, Vec3::ZERO)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0);
    let s = Settings { width: 24, height: 24, samples: 64, max_depth: 16, environment: Environment::hdri(map), ..Default::default() };
    let m = mean(&render(&scene, &cam, &s));
    assert!((m - Vec3::ONE).abs().max_element() < 0.03, "media {m}");
}

/// Con HDRI variado el resultado debe coincidir con muestrear solo por BSDF (sin sesgo del MIS).
#[test]
fn mis_hdri_coincide_con_solo_bsdf() {
    let (tris, m) = floor(50.0, 0.6);
    let scene = RayScene::new(tris, vec![m]);
    let cam = Camera::look_at(Vec3::new(0.0, 3.0, 0.01), Vec3::ZERO, Vec3::Y, 30.0, 1.0);
    let with_mis = Settings { width: 8, height: 8, samples: 2048, max_depth: 1, environment: Environment::hdri(varied_map()), ..Default::default() };
    let a = mean(&render(&scene, &cam, &with_mis));
    // Referencia: el mismo entorno promediado a fuerza bruta por irradiancia
    let map = varied_map();
    let mut rng = Rng::new(5, 5);
    let n = 400_000;
    let mut e = Vec3::ZERO;
    for _ in 0..n {
        let z = rng.next_f32();
        let phi = 2.0 * PI * rng.next_f32();
        let r = (1.0 - z * z).sqrt();
        let d = Vec3::new(r * phi.cos(), z, r * phi.sin());
        e += map.eval(d) * d.y * 2.0 * PI;
    }
    let expected = 0.6 / PI * (e / n as f32);
    assert!((a / expected - Vec3::ONE).abs().max_element() < 0.1, "{a} vs {expected}");
}

