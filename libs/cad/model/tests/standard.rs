//! Piezas estándar: volúmenes contra las medidas ISO.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn part(shape: PrimitiveShape) -> Shape {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive { shape, origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::Join }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let body = ev.body.unwrap();
    assert!(body.is_valid());
    assert_eq!(body.solids().unwrap().len(), 1);
    body
}

fn volume(s: &Shape) -> f64 {
    s.mass().unwrap().volume
}

fn bolt(size: &str, length: f64, head: BoltHead, modeled: bool) -> Shape {
    part(PrimitiveShape::Bolt { size: size.into(), length, head, modeled })
}

/// Volumen por mm de un macho M`d` × `p` (perfil ISO básico): núcleo + filete por Pappus.
fn rod_per_mm(d: f64, p: f64) -> f64 {
    let d1 = d - 1.082_532 * p;
    let (r1, h) = (d1 / 2.0, (d - d1) / 2.0);
    let (top, bottom) = (p / 8.0, p / 8.0 + 2.0 * h * (PI / 6.0).tan());
    let rc = r1 + h * (bottom + 2.0 * top) / (3.0 * (bottom + top));
    PI * r1 * r1 + (top + bottom) / 2.0 * h / p * 2.0 * PI * rc
}

/// Área de un hexágono de entre caras `s`
fn hex(s: f64) -> f64 {
    3f64.sqrt() / 2.0 * s * s
}

/// Lo que quita el chaflán de la punta: el anillo entre el cilindro y el cono a 45°
fn tip(d: f64, d1: f64) -> f64 {
    let (r, r1) = (d / 2.0, d1 / 2.0);
    let c = r - r1;
    PI * r * r * c - PI * c / 3.0 * (r1 * r1 + r1 * r + r * r)
}

#[test]
fn washer_m6() {
    if !occt() {
        return;
    }
    let w = part(PrimitiveShape::Washer { size: "M6".into() });
    assert_relative_eq!(volume(&w), PI / 4.0 * (12.0f64.powi(2) - 6.4f64.powi(2)) * 1.6, max_relative = 1e-9);
    let bb = w.mass().unwrap();
    assert_relative_eq!(bb.bbox_max[2], 1.6, epsilon = 1e-6);
}

#[test]
fn socket_bolt_m6x20() {
    if !occt() {
        return;
    }
    let b = bolt("M6", 20.0, BoltHead::Socket, false);
    let d1 = 6.0 - 1.082_532;
    let expected = PI * 25.0 * 6.0 - hex(5.0) * 3.0 + PI * 9.0 * 20.0 - tip(6.0, d1);
    assert_relative_eq!(volume(&b), expected, max_relative = 2e-3);
    // Cabeza arriba del origen y caña hacia abajo
    let m = b.mass().unwrap();
    assert_relative_eq!(m.bbox_max[2], 6.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_min[2], -20.0, epsilon = 1e-6);
}

#[test]
fn hex_and_countersunk_bolts() {
    if !occt() {
        return;
    }
    let d1 = 8.0 - 1.082_532 * 1.25;
    // Hexagonal M8×30: la cabeza achaflanada queda entre el círculo inscrito y el hexágono
    let v = volume(&bolt("M8", 30.0, BoltHead::Hex, false));
    let shank = PI * 16.0 * 30.0 - tip(8.0, d1);
    assert!(v < shank + hex(13.0) * 5.3 && v > shank + PI * 6.5 * 6.5 * 5.3, "{v}");
    // Avellanado M5×16: el largo incluye la cabeza (cono de 11,2 a 5 a 90°)
    let b = bolt("M5", 16.0, BoltHead::Countersunk, false);
    let k = (11.2 - 5.0) / 2.0;
    let d1 = 5.0 - 1.082_532 * 0.8;
    let cone = PI * k / 3.0 * (5.6f64.powi(2) + 2.5f64.powi(2) + 5.6 * 2.5);
    let expected = cone - hex(3.0) * 1.9 + PI * 6.25 * (16.0 - k) - tip(5.0, d1);
    assert_relative_eq!(volume(&b), expected, max_relative = 2e-3);
    assert_relative_eq!(b.mass().unwrap().bbox_max[2], 0.0, epsilon = 1e-6);
}

#[test]
fn nut_m6_plain_and_threaded() {
    if !occt() {
        return;
    }
    let plain = volume(&part(PrimitiveShape::Nut { size: "M6".into(), modeled: false }));
    let m = 5.2;
    assert!(plain < (hex(10.0) - PI * 9.0) * m && plain > (PI * 25.0 - PI * 9.0) * m, "{plain}");
    // Con rosca: el agujero es el macho en vez del cilindro nominal
    let threaded = volume(&part(PrimitiveShape::Nut { size: "M6".into(), modeled: true }));
    assert_relative_eq!(threaded - plain, m * (PI * 9.0 - rod_per_mm(6.0, 1.0)), max_relative = 2e-2);
}

#[test]
fn threaded_bolt_m4x12() {
    if !occt() {
        return;
    }
    let plain = volume(&bolt("M4", 12.0, BoltHead::Socket, false));
    let threaded = volume(&bolt("M4", 12.0, BoltHead::Socket, true));
    // Rosca en todo el largo (2·d + 6 = 14 > 12): se pierde lo que va del cilindro al macho
    let lost = 12.0 * (PI * 4.0 - rod_per_mm(4.0, 0.7));
    assert_relative_eq!(plain - threaded, lost, max_relative = 0.1);
}

#[test]
fn unknown_size_is_an_error() {
    let mut doc = Document::new();
    let id = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Nut { size: "M7".into(), modeled: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    if occt() {
        assert!(matches!(doc.evaluate().state(id), Some(FeatureState::Error { .. })));
    }
    assert_eq!(standard::size_for_hole(6.6).name, "M6");
    assert_eq!(standard::size_for_hole(5.0).name, "M6");
    assert_eq!(standard::size_for_hole(3.4).name, "M3");
}

#[test]
#[ignore]
fn timing() {
    use std::time::Instant;
    let f = cad_occt::Frame::WORLD;
    let runs: Vec<(&str, Box<dyn Fn() -> Result<Shape, String>>)> = vec![
        ("rod M8 L22", Box::new(|| Shape::thread(cad_occt::Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] }, 3.3, 4.0, 1.25, 22.0, false).map_err(|e| e.to_string()))),
        ("bolt M8x25 hex mod", Box::new(move || standard::bolt(f, "M8", 25.0, BoltHead::Hex, true))),
        ("bolt M6x20 socket mod", Box::new(move || standard::bolt(f, "M6", 20.0, BoltHead::Socket, true))),
        ("nut M8 mod", Box::new(move || standard::nut(f, "M8", true))),
        ("nut M6 mod", Box::new(move || standard::nut(f, "M6", true))),
    ];
    for (name, run) in runs {
        let t = Instant::now();
        let s = run().unwrap();
        println!("{name}: {:?} faces {} valid {}", t.elapsed(), s.face_count(), s.is_valid());
    }
    let mut doc = Document::new();
    for (shape, x) in [
        (PrimitiveShape::Bolt { size: "M8".into(), length: 25.0, head: BoltHead::Hex, modeled: true }, 0.0),
        (PrimitiveShape::Nut { size: "M8".into(), modeled: true }, 20.0),
    ] {
        doc.add(FeatureKind::Primitive(Primitive { shape, origin: [x, 0.0, 0.0], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New }));
    }
    let t = Instant::now();
    let ev = doc.evaluate();
    println!("evaluate: {:?}", t.elapsed());
    let t = Instant::now();
    let body = ev.body.unwrap();
    let _ = body.mass();
    println!("mass: {:?}", t.elapsed());
    let t = Instant::now();
    let _ = body.tessellate(0.05, 0.3);
    println!("tessellate: {:?}", t.elapsed());
    let t = Instant::now();
    let _ = body.is_valid();
    println!("valid: {:?}", t.elapsed());
}
