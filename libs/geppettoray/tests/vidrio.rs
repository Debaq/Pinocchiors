use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;

fn mean(img: &Image) -> Vec3 {
    img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32
}

fn glass(base: Vec3, ior: f32, thickness: f32) -> RayMaterial {
    RayMaterial { base_color: base.extend(1.0), metallic: 0.0, roughness: 0.0, transmission: 1.0, ior, thickness, ..Default::default() }
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

fn settings(env: Vec3, samples: u32) -> Settings {
    Settings { width: 16, height: 16, samples, max_depth: 8, environment: Environment::uniform(env), ..Default::default() }
}

/// Lámina de vidrio absorbente de grosor 1: solución analítica con reflexiones múltiples.
#[test]
fn lamina_con_absorcion_analitica() {
    let a: f32 = 0.5; // transmitancia por unidad de recorrido (color base 0.5, distancia 1)
    let f: f32 = 0.04;
    let big = 100.0;
    let mut tris = quad(Vec3::new(-big, -big, 0.0), Vec3::X * 2.0 * big, Vec3::Y * 2.0 * big, 0).to_vec(); // mira a +Z
    tris.extend(quad(Vec3::new(-big, -big, -1.0), Vec3::Y * 2.0 * big, Vec3::X * 2.0 * big, 0)); // mira a -Z
    let scene = RayScene::new(tris, vec![glass(Vec3::splat(a), 1.5, 1.0)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 0.5, 1.0);
    let img = render(&scene, &cam, &settings(Vec3::ONE, 2000));
    let denom = 1.0 - f * f * a * a;
    let reflected = f + (1.0 - f).powi(2) * f * a * a / denom;
    let transmitted = (1.0 - f).powi(2) * a / denom;
    let expected = reflected + transmitted;
    let m = mean(&img);
    assert!((m.x / expected - 1.0).abs() < 0.02, "{} vs {expected}", m.x);
}

/// Esfera de vidrio sin absorción bajo luz uniforme: refleja o refracta todo → brillo 1.
#[test]
fn horno_blanco_vidrio() {
    for ior in [1.33, 1.5, 2.4] {
        let scene = RayScene::new(sphere(1.0, 0), vec![glass(Vec3::ONE, ior, 2.0)]);
        let cam = Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0);
        let m = mean(&render(&scene, &cam, &settings(Vec3::ONE, 64)));
        assert!((m - Vec3::ONE).abs().max_element() < 0.02, "ior {ior}: {m}");
    }
}

/// Una esfera de vidrio es una lente: invierte la imagen de lo que hay detrás.
#[test]
fn esfera_de_vidrio_invierte_la_imagen() {
    // Detrás, una pared emisiva: mitad izquierda roja, mitad derecha azul
    let mut tris = sphere(1.0, 0);
    let wall_l = quad(Vec3::new(-20.0, -20.0, -10.0), Vec3::X * 20.0, Vec3::Y * 40.0, 1); // x<0 rojo, mira a +Z
    let wall_r = quad(Vec3::new(0.0, -20.0, -10.0), Vec3::X * 20.0, Vec3::Y * 40.0, 2);
    tris.extend(wall_l);
    tris.extend(wall_r);
    let emit = |c: Vec3| RayMaterial { double_sided: true, ..RayMaterial::diffuse(Vec3::ZERO, c) };
    let scene = RayScene::new(tris, vec![glass(Vec3::ONE, 1.5, 2.0), emit(Vec3::X), emit(Vec3::Z)]);
    let cam = Camera::look_at(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y, 30.0, 1.0);
    let s = Settings { width: 32, height: 32, samples: 32, max_depth: 8, environment: Environment::uniform(Vec3::ZERO), ..Default::default() };
    let img = render(&scene, &cam, &s);
    // Columna central de la esfera: fila de píxeles a la altura del centro
    let row = 16usize;
    let px = |x: usize| img.pixels[row * 32 + x];
    // A los costados de la esfera la pared se ve normal (izquierda roja, derecha azul);
    // por dentro de la esfera lo que se ve en el lado izquierdo viene de la derecha (azul)
    assert!(px(1).x > 0.9 && px(1).z < 0.1, "borde izquierdo {:?}", px(1));
    assert!(px(30).z > 0.9 && px(30).x < 0.1, "borde derecho {:?}", px(30));
    let inside_left = (px(12) + px(13)) * 0.5;
    assert!(inside_left.z > inside_left.x, "dentro de la esfera, izquierda debería verse azul (invertido): {inside_left:?}");
}

/// Sombra de un vidrio: deja pasar parte de la luz, no queda negra.
#[test]
fn sombra_de_vidrio_no_es_negra() {
    let floor = quad(Vec3::new(-50.0, 0.0, 50.0), Vec3::X * 100.0, -Vec3::Z * 100.0, 0);
    let mut tris = floor.to_vec();
    tris.extend(quad(Vec3::new(-5.0, 1.0, 5.0), Vec3::X * 10.0, -Vec3::Z * 10.0, 1)); // lámina de vidrio encima
    let mats = vec![RayMaterial::diffuse(Vec3::splat(0.5), Vec3::ZERO), RayMaterial { double_sided: true, ..glass(Vec3::ONE, 1.5, 0.0) }];
    let scene = RayScene::new(tris, mats);
    let sun = Light::Sun(Sun { direction: Vec3::Y, color: Vec3::splat(3.0), angular_radius: 0.0 });
    let cam = Camera::look_at(Vec3::new(0.0, 5.0, 0.001), Vec3::ZERO, Vec3::Y, 2.0, 1.0);
    let mut s = settings(Vec3::ZERO, 256);
    s.lights = vec![sun];
    let m = mean(&render(&scene, &cam, &s)).x;
    // Abajo de la lámina: albedo/π · E · (1 - F) ≈ 0.5/π·3·0.96 (la lámina fina transmite todo salvo el Fresnel)
    // además la cámara mira a través de la lámina (tiñe con blanco: sin pérdida salvo reflexión)
    assert!(m > 0.3, "la sombra no debe ser negra: {m}");
}
