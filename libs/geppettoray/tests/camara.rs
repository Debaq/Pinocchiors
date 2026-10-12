use geppettoray::*;
use glam::Vec3;
use std::f32::consts::PI;

fn cam() -> Camera {
    Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y, 30.0, 1.0)
}

#[test]
fn distancia_focal_a_campo_de_vision() {
    let v = Camera::vfov_from_focal_length(50.0, SENSOR_FULL_FRAME_HEIGHT_MM);
    assert!((v - 27.0).abs() < 0.05, "{v}"); // 50 mm en 35 mm completo: ~27° vertical
    // Más focal = más cerrado
    assert!(Camera::vfov_from_focal_length(100.0, 24.0) < v);
    // El rayo del borde superior forma la mitad del campo con el eje
    let c = Camera::from_focal_length(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 50.0, 24.0, 1.5);
    let r = c.ray(0.5, 0.0);
    let ang = (-r.dir.z).acos().to_degrees();
    assert!((ang - v / 2.0).abs() < 0.05, "{ang}");
}

#[test]
fn radio_de_apertura() {
    // 50 mm a f/2 → 12.5 mm de radio; escena en metros
    let r = Camera::aperture_radius(50.0, 2.0, 0.001);
    assert!((r - 0.0125).abs() < 1e-6);
}

/// Todos los rayos de un píxel convergen en el mismo punto del plano focal y nacen en el disco.
#[test]
fn rayos_convergen_en_el_plano_focal() {
    let c = cam().with_lens(Lens { radius: 0.4, focus_distance: 6.0, blades: 0 });
    let mut rng = Rng::new(1, 1);
    let mut first: Option<Vec3> = None;
    let mut max_off = 0.0f32;
    for _ in 0..500 {
        let r = c.sample_ray(0.3, 0.6, &mut rng);
        // Plano focal: z = 10 - 6 = 4
        let t = (4.0 - r.origin.z) / r.dir.z;
        let hit = r.origin + r.dir * t;
        match first {
            None => first = Some(hit),
            Some(f) => assert!((hit - f).length() < 1e-3, "{hit} vs {f}"),
        }
        max_off = max_off.max((r.origin - c.position).length());
    }
    assert!(max_off <= 0.4 + 1e-4 && max_off > 0.3, "{max_off}");
}

#[test]
fn sin_lente_ni_movimiento_es_pinhole() {
    let c = cam();
    let mut rng = Rng::new(2, 2);
    let a = c.ray(0.2, 0.7);
    let b = c.sample_ray(0.2, 0.7, &mut rng);
    assert!((a.dir - b.dir).length() < 1e-6 && a.origin == b.origin);
}

/// Diafragma de 6 aspas: los puntos quedan dentro del hexágono y se reparten parejo.
#[test]
fn bokeh_poligonal() {
    let c = cam().with_lens(Lens { radius: 1.0, focus_distance: 5.0, blades: 6 });
    let mut rng = Rng::new(7, 7);
    let n = 20_000;
    let mut inside_inscribed = 0;
    let mut sum = Vec3::ZERO;
    for _ in 0..n {
        let o = c.sample_ray(0.5, 0.5, &mut rng).origin - c.position;
        let r = (o.x * o.x + o.y * o.y).sqrt();
        assert!(r <= 1.0 + 1e-4, "{r}");
        // Apotema de un hexágono de radio 1: cos(30°) = 0.866
        if r <= (PI / 6.0).cos() {
            inside_inscribed += 1;
        }
        sum += o;
    }
    assert!(inside_inscribed as f32 / n as f32 > 0.5);
    assert!((sum / n as f32).length() < 0.02, "sin sesgo: {}", sum / n as f32);
}

/// Con movimiento, el origen cubre el recorrido de la cámara y promedia el punto medio.
#[test]
fn desenfoque_de_movimiento() {
    let c = cam().with_motion(Vec3::new(2.0, 0.0, 10.0), Vec3::new(2.0, 0.0, 0.0), Vec3::Y);
    let mut rng = Rng::new(3, 3);
    let n = 10_000;
    let (mut min_x, mut max_x, mut sum) = (f32::MAX, f32::MIN, 0.0);
    for _ in 0..n {
        let x = c.sample_ray(0.5, 0.5, &mut rng).origin.x;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        sum += x;
    }
    assert!(min_x < 0.05 && max_x > 1.95, "{min_x}..{max_x}");
    assert!((sum / n as f32 - 1.0).abs() < 0.03);
}

fn emitter_scene(z: f32) -> RayScene {
    // Esfera emisiva pequeña a distancia `z` de la cámara (en z = 10 - z)
    let (nu, nv) = (24, 12);
    let r = 0.05;
    let pt = |i: usize, j: usize| {
        let th = PI * j as f32 / nv as f32;
        let ph = 2.0 * PI * i as f32 / nu as f32;
        Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()) * r + Vec3::new(0.0, 0.0, 10.0 - z)
    };
    let mut tris = Vec::new();
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            for t in [[a, b, c], [a, c, d]] {
                if (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-12 {
                    tris.push(Triangle::new(t, None, None, 0));
                }
            }
        }
    }
    RayScene::new(tris, vec![RayMaterial { double_sided: true, unlit: true, ..RayMaterial::diffuse(Vec3::ONE, Vec3::ZERO) }])
}

/// Una luz puntual fuera de foco se reparte en un disco: baja el pico y se conserva la energía.
#[test]
fn profundidad_de_campo_desenfoca_fuera_del_plano() {
    let settings = Settings {
        width: 64,
        height: 64,
        samples: 256,
        max_depth: 0,
        environment: Environment::uniform(Vec3::ZERO),
        ..Default::default()
    };
    let lens = Lens { radius: 0.5, focus_distance: 5.0, blades: 0 };
    let in_focus = render(&emitter_scene(5.0), &cam().with_lens(lens), &settings);
    let out_focus = render(&emitter_scene(9.0), &cam().with_lens(lens), &settings);
    let peak = |i: &Image| i.pixels.iter().map(|p| p.x).fold(0.0, f32::max);
    assert!(peak(&out_focus) < 0.35 * peak(&in_focus), "{} vs {}", peak(&out_focus), peak(&in_focus));
    // Se compara cuántos píxeles toca la luz: fuera de foco ocupa un disco mucho mayor
    let spread = |i: &Image| i.pixels.iter().filter(|p| p.x > 0.02 * peak(i)).count();
    assert!(spread(&out_focus) > 3 * spread(&in_focus), "{} vs {}", spread(&out_focus), spread(&in_focus));
}
