use geppettoray::*;
use glam::Vec3;

/// Esfera UV de radio r centrada en el origen
fn sphere(r: f32, mat: u32, inward: bool) -> Vec<Triangle> {
    let (nu, nv) = (48, 24);
    let pt = |i: usize, j: usize| {
        let th = std::f32::consts::PI * j as f32 / nv as f32;
        let ph = 2.0 * std::f32::consts::PI * i as f32 / nu as f32;
        Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()) * r
    };
    let mut out = Vec::new();
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            let quad = if inward { [[a, c, b], [a, d, c]] } else { [[a, b, c], [a, c, d]] };
            for t in quad {
                if (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-9 {
                    let n = t.map(|p| if inward { -p.normalize() } else { p.normalize() });
                    out.push(Triangle::new(t, Some(n), None, mat));
                }
            }
        }
    }
    out
}

fn cam() -> Camera {
    Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 40.0, 1.0)
}

/// Horno blanco: esfera difusa de albedo 1 bajo luz uniforme 1 sale con brillo 1.
#[test]
fn horno_blanco() {
    let scene = RayScene::new(
        sphere(1.0, 0, false),
        vec![RayMaterial::diffuse(Vec3::ONE, Vec3::ZERO)],
    );
    let s = Settings {
        width: 24,
        height: 24,
        samples: 64,
        max_depth: 16,
        environment: Environment::uniform(Vec3::ONE),
        ..Default::default()
    };
    let img = render(&scene, &cam(), &s);
    let mean = img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32;
    assert!((mean - Vec3::ONE).abs().max_element() < 0.02, "media {mean}");
}

/// Horno blanco cóncavo: esfera hueca con albedo 1 y emisión 0, cámara dentro,
/// ilumina solo el rebote: con albedo<1 la energía decae como serie geométrica.
#[test]
fn caja_cerrada_serie_geometrica() {
    // Interior emisivo L=1, albedo a: radiancia = L / (1 - a)
    let a = 0.5;
    let scene = RayScene::new(
        sphere(2.0, 0, true),
        vec![RayMaterial::diffuse(Vec3::splat(a), Vec3::ONE)],
    );
    let s = Settings {
        width: 16,
        height: 16,
        samples: 128,
        max_depth: 64,
        environment: Environment::uniform(Vec3::ZERO),
        ..Default::default()
    };
    let camera = Camera::look_at(Vec3::ZERO, Vec3::Z, Vec3::Y, 60.0, 1.0);
    let img = render(&scene, &camera, &s);
    let mean = img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32;
    assert!((mean.x - 1.0 / (1.0 - a)).abs() < 0.05, "media {mean}");
}

/// La BVH da lo mismo que la fuerza bruta.
#[test]
fn bvh_igual_a_fuerza_bruta() {
    let mut tris = sphere(1.0, 0, false);
    tris.extend(sphere(0.4, 0, false).into_iter().map(|mut t| {
        t.v0 += Vec3::new(1.5, 0.3, -0.2);
        t
    }));
    let scene = RayScene::new(tris, vec![]);
    let mut rng_state = 12345u32;
    let mut rnd = || {
        rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        (rng_state >> 8) as f32 / 16_777_216.0
    };
    for _ in 0..500 {
        let o = Vec3::new(rnd() * 8.0 - 4.0, rnd() * 8.0 - 4.0, rnd() * 8.0 - 4.0);
        let d = Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5).normalize();
        let ray = Ray { origin: o, dir: d };
        let fast = scene.closest(&ray, f32::INFINITY).map(|h| h.t);
        let brute = scene
            .triangles
            .iter()
            .filter_map(|t| t.intersect(&ray, f32::INFINITY).map(|r| r.0))
            .fold(None, |m: Option<f32>, t| Some(m.map_or(t, |x| x.min(t))));
        match (fast, brute) {
            (None, None) => {}
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-4, "{a} vs {b}"),
            other => panic!("difieren: {other:?}"),
        }
    }
}

/// Sol sobre un plano difuso: radiancia = albedo/π · E · cosθ (+ entorno 0).
#[test]
fn sol_sobre_plano() {
    let q = 50.0;
    let (a, b, c, d) = (Vec3::new(-q, 0.0, -q), Vec3::new(q, 0.0, -q), Vec3::new(q, 0.0, q), Vec3::new(-q, 0.0, q));
    let tris = vec![
        Triangle::new([a, c, b], None, None, 0), // normal +Y
        Triangle::new([a, d, c], None, None, 0),
    ];
    let scene = RayScene::new(tris, vec![RayMaterial { double_sided: false, ..RayMaterial::diffuse(Vec3::splat(0.6), Vec3::ZERO) }]);
    let sun_dir = Vec3::new(0.0, 1.0, 1.0).normalize();
    let s = Settings {
        width: 8,
        height: 8,
        samples: 16,
        max_depth: 1,
        environment: Environment::uniform(Vec3::ZERO),
        lights: vec![Light::Sun(Sun { direction: sun_dir, color: Vec3::splat(3.0) })],
        ..Default::default()
    };
    let camera = Camera::look_at(Vec3::new(0.0, 5.0, 0.1), Vec3::ZERO, Vec3::Y, 20.0, 1.0);
    let img = render(&scene, &camera, &s);
    let expected = 0.6 / std::f32::consts::PI * 3.0 * sun_dir.y;
    let mean = img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32;
    // El difuso pierde ~4 % por el Fresnel de la capa dieléctrica y el especular suma un poco
    assert!((mean.x / expected - 1.0).abs() < 0.04, "{} vs {expected}", mean.x);
}

/// Horno blanco con el material completo (dieléctrico blanco): conserva casi toda la energía y nunca crea.
#[test]
fn horno_blanco_material_completo() {
    let mat = RayMaterial { base_color: glam::Vec4::ONE, roughness: 0.5, ..Default::default() };
    let scene = RayScene::new(sphere(1.0, 0, false), vec![mat]);
    let s = Settings {
        width: 24,
        height: 24,
        samples: 64,
        max_depth: 16,
        environment: Environment::uniform(Vec3::ONE),
        ..Default::default()
    };
    let img = render(&scene, &cam(), &s);
    let mean = (img.pixels.iter().sum::<Vec3>() / img.pixels.len() as f32).x;
    assert!(mean < 1.02 && mean > 0.85, "media {mean}");
}
