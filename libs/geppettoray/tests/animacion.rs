use converter_scene::glam::Vec3 as SVec3;
use converter_scene::{Animation, Channel, IndexData, Interpolation, KeyframeValues, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};
use geppettoray::*;
use glam::Vec3;
use std::sync::atomic::{AtomicBool, Ordering};

/// Un cuadrado de lado 1 mirando a +Z que se desplaza en X de -2 a 2 durante un segundo
fn moving_square() -> Scene {
    let mut s = Scene::new();
    s.meshes.push(Mesh {
        name: "cuadrado".into(),
        primitives: vec![Primitive {
            attributes: vec![
                VertexAttribute::Positions(vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]]),
                VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; 4]),
            ],
            indices: Some(IndexData::U32(vec![0, 1, 2, 0, 2, 3])),
            material: None,
        }],
    });
    s.nodes.push(Node { name: "n".into(), transform: Transform::Trs { translation: SVec3::ZERO, rotation: Default::default(), scale: SVec3::ONE }, mesh: Some(0), skin: None, children: vec![] });
    s.root_nodes = vec![0];
    s.animations.push(Animation {
        name: "mueve".into(),
        channels: vec![Channel {
            node: 0,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 1.0],
            values: KeyframeValues::Translation(vec![[-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]]),
        }],
    });
    s
}

fn settings() -> Settings {
    Settings {
        width: 40,
        height: 10,
        samples: 8,
        max_depth: 1,
        environment: Environment::uniform(Vec3::ONE),
        transparent_background: true,
        ..Default::default()
    }
}

fn camera() -> Camera {
    // Mira de frente: a distancia 5 con fov vertical 2·atan(1/5): el alto visible es 2 y el ancho, 8
    Camera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y, 2.0 * (1.0f32 / 5.0).atan().to_degrees(), 4.0)
}

fn opts() -> ProgressiveOptions {
    ProgressiveOptions { passes: 2, tile: 16, denoise: None, denoise_previews: false }
}

/// Centro (en píxeles, columna) de lo cubierto por el cuadrado y cuántas columnas toca
fn coverage(f: &Frame) -> (f32, usize) {
    let a = f.alpha.as_ref().unwrap();
    let w = f.image.width as usize;
    let cols: Vec<usize> = (0..w).filter(|&x| (0..f.image.height as usize).any(|y| a[y * w + x] > 0.05)).collect();
    (cols.iter().sum::<usize>() as f32 / cols.len().max(1) as f32, cols.len())
}

fn run(a: AnimationSettings, skip: impl Fn(u32) -> bool, cancel: &AtomicBool) -> (AnimationOutcome, Vec<(u32, Frame)>) {
    let scene = moving_square();
    let mut frames = Vec::new();
    let out = render_animation(&scene, &camera(), &settings(), &opts(), &a, cancel, skip, |_, _| {}, |i, f| frames.push((i, f.clone())));
    (out, frames)
}

fn base() -> AnimationSettings {
    AnimationSettings { animation: Some(0), fps: 4.0, first_frame: 0, last_frame: 4, shutter: 0.0, motion_samples: 1, studio: None }
}

#[test]
fn el_objeto_se_mueve_de_un_cuadro_al_siguiente() {
    let (out, frames) = run(base(), |_| false, &AtomicBool::new(false));
    assert_eq!((out.rendered, out.skipped, out.cancelled), (5, 0, false));
    assert_eq!(frames.len(), 5);
    let centers: Vec<f32> = frames.iter().map(|(_, f)| coverage(f).0).collect();
    // 40 px = 8 unidades: de x=-2 (columna 10) a x=2 (columna 30)
    assert!((centers[0] - 10.0).abs() < 1.5 && (centers[4] - 30.0).abs() < 1.5, "{centers:?}");
    assert!(centers.windows(2).all(|w| w[1] > w[0] + 3.0), "avanza en cada cuadro: {centers:?}");
}

#[test]
fn reanudar_salta_los_cuadros_que_ya_existen() {
    let (out, frames) = run(base(), |i| i == 1 || i == 2, &AtomicBool::new(false));
    assert_eq!((out.rendered, out.skipped), (3, 2));
    assert_eq!(frames.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 3, 4]);
}

#[test]
fn cancelar_detiene_la_secuencia() {
    let cancel = AtomicBool::new(false);
    let scene = moving_square();
    let mut n = 0;
    let out = render_animation(&scene, &camera(), &settings(), &opts(), &base(), &cancel, |_| false, |_, _| {}, |_, _| {
        n += 1;
        cancel.store(true, Ordering::Relaxed);
    });
    assert!(out.cancelled);
    assert_eq!((n, out.rendered), (1, 1));
}

#[test]
fn el_avance_llega_a_uno_y_el_tiempo_baja() {
    let scene = moving_square();
    let mut fractions = Vec::new();
    render_animation(&scene, &camera(), &settings(), &opts(), &base(), &AtomicBool::new(false), |_| false, |p, _| fractions.push((p.fraction, p.eta_secs)), |_, _| {});
    assert!(fractions.windows(2).all(|w| w[1].0 >= w[0].0 - 1e-6), "{fractions:?}");
    assert!((fractions.last().unwrap().0 - 1.0).abs() < 1e-4);
    assert!(fractions.last().unwrap().1 < 1e-3);
}

#[test]
fn desenfoque_de_movimiento_estira_al_objeto() {
    let sharp = run(AnimationSettings { first_frame: 2, last_frame: 2, ..base() }, |_| false, &AtomicBool::new(false)).1;
    let blurred = run(AnimationSettings { first_frame: 2, last_frame: 2, shutter: 1.0, motion_samples: 8, ..base() }, |_| false, &AtomicBool::new(false)).1;
    let (c0, w0) = coverage(&sharp[0].1);
    let (c1, w1) = coverage(&blurred[0].1);
    assert!((c0 - c1).abs() < 1.5, "mismo centro: {c0} vs {c1}");
    assert!(w1 > w0 + 3, "más ancho con desenfoque: {w0} vs {w1}");
    // Los bordes quedan semitransparentes
    let a = blurred[0].1.alpha.as_ref().unwrap();
    assert!(a.iter().any(|&v| v > 0.1 && v < 0.9));
}

#[test]
fn sin_animacion_todos_los_cuadros_son_iguales() {
    let (_, frames) = run(AnimationSettings { animation: None, ..base() }, |_| false, &AtomicBool::new(false));
    let c: Vec<f32> = frames.iter().map(|(_, f)| coverage(f).0).collect();
    assert!(c.iter().all(|&x| (x - c[0]).abs() < 1e-3), "{c:?}");
}
