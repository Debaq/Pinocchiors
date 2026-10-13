//! Fondo de estudio (ciclorama).

use converter_scene::{IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};
use geppettoray::*;
use glam::Vec3;
use std::sync::atomic::AtomicBool;

/// Una pirámide sobre y = 0 (el modelo del estudio)
fn model() -> Scene {
    let mut s = Scene::new();
    s.meshes.push(Mesh {
        name: "m".into(),
        primitives: vec![Primitive {
            attributes: vec![VertexAttribute::Positions(vec![
                [-0.5, 0.0, -0.5],
                [0.5, 0.0, -0.5],
                [0.5, 0.0, 0.5],
                [-0.5, 0.0, 0.5],
                [0.0, 1.0, 0.0],
            ])],
            indices: Some(IndexData::U32(vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4])),
            material: None,
        }],
    });
    s.nodes.push(Node { name: "n".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
    s.root_nodes = vec![0];
    s
}

fn studio() -> Studio {
    Studio { color: Vec3::splat(0.9), floor_center: Vec3::ZERO, toward_wall: -Vec3::Z, size: 1.8, gloss: 0.0 }
}

fn render_frame(scene: &Scene, cam: &Camera, s: &Settings) -> Frame {
    let animated = AnimatedScene::new(scene).with_studio(studio());
    let rs = animated.at(None, 0.0);
    let o = ProgressiveOptions { passes: 1, tile: 16, denoise: None, denoise_previews: false };
    render_progressive(&rs, cam, s, &o, &AtomicBool::new(false), |_, _| {}).frame
}

fn settings() -> Settings {
    Settings {
        width: 48,
        height: 32,
        samples: 32,
        max_depth: 2,
        environment: Environment::uniform(Vec3::splat(0.3)),
        transparent_background: true,
        lights: vec![Light::Sun(Sun { direction: Vec3::new(0.3, 1.0, 0.5).normalize(), color: Vec3::splat(3.0), angular_radius: 0.03 })],
        ..Default::default()
    }
}

/// Mirando de frente, toda la imagen es piso o muro: no se ve el entorno ni "el vacío".
#[test]
fn el_estudio_llena_el_cuadro() {
    let cam = Camera::look_at(Vec3::new(0.0, 0.9, 4.0), Vec3::new(0.0, 0.5, 0.0), Vec3::Y, 50.0, 1.5);
    let f = render_frame(&model(), &cam, &settings());
    let alpha = f.alpha.unwrap();
    assert!(alpha.iter().all(|&a| a > 0.99), "min alpha {}", alpha.iter().cloned().fold(1.0, f32::min));
    // El piso y el muro de un ciclorama blanco salen claros
    let mean = f.image.pixels.iter().map(|p| p.x).sum::<f32>() / f.image.pixels.len() as f32;
    assert!(mean > 0.15, "{mean}");
}

/// El piso recibe la sombra del modelo, y el muro no tapa la luz que viene de detrás de él.
#[test]
fn sombra_en_el_piso_y_luz_desde_detras_del_muro() {
    // Luz hacia el muro (-Z) y arriba: sin `casts_shadow: false` el muro la taparía
    let mut s = settings();
    s.environment = Environment::uniform(Vec3::ZERO);
    s.lights = vec![Light::Sun(Sun { direction: Vec3::new(0.0, 0.6, -0.8).normalize(), color: Vec3::splat(3.0), angular_radius: 0.0 })];
    let cam = Camera::look_at(Vec3::new(0.0, 0.8, 4.0), Vec3::new(0.0, 0.4, 0.0), Vec3::Y, 50.0, 1.5);
    let f = render_frame(&model(), &cam, &s);
    let lit = f.image.pixels.iter().filter(|p| p.x > 0.05).count();
    assert!(lit > f.image.pixels.len() / 10, "con la luz detrás del muro, el piso y el modelo siguen iluminados: {lit}");
    // Piso con sombra: hay píxeles claros y oscuros de piso (no todo igual)
    let max = f.image.pixels.iter().map(|p| p.x).fold(0.0f32, f32::max);
    let dark_floor = f.image.pixels.iter().filter(|p| p.x < 0.3 * max).count();
    assert!(dark_floor > 4, "debe haber sombra: {dark_floor}");
}

/// Desde abajo no se ve el piso (una cara): el modelo se ve sin él.
#[test]
fn desde_abajo_el_piso_no_tapa() {
    let cam = Camera::look_at(Vec3::new(0.0, -3.0, 0.5), Vec3::new(0.0, 0.3, 0.0), Vec3::Y, 30.0, 1.5);
    let f = render_frame(&model(), &cam, &settings());
    let alpha = f.alpha.unwrap();
    assert!(alpha.iter().any(|&a| a > 0.99), "la pirámide se ve");
    assert!(alpha.iter().any(|&a| a < 0.5), "y alrededor no hay piso: se ve el fondo");
}

#[test]
fn geometria_del_ciclorama() {
    let st = studio();
    let tris = st.triangles(0);
    assert!(tris.len() > 30);
    for t in &tris {
        let g = t.geometric_normal();
        // Cada cara mira hacia el interior: el promedio de sus normales de sombreado coincide
        assert!(g.is_finite() && g.dot(t.n[0] + t.n[1] + t.n[2]) > 0.0);
        // Nada queda por debajo del piso
        for p in [t.v0, t.v0 + t.e1, t.v0 + t.e2] {
            assert!(p.y >= -1e-4);
        }
    }
    // El muro queda del lado de `toward_wall` (-Z) y el piso se extiende hacia la cámara (+Z)
    let (min_z, max_z) = tris.iter().fold((f32::MAX, f32::MIN), |(a, b), t| (a.min(t.v0.z), b.max(t.v0.z)));
    assert!(min_z < -4.0 && max_z > 8.0, "{min_z} {max_z}");
}
