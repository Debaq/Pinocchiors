//! Agujeros: simple, con caja, avellanado y ciego con punta.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

/// Placa 40×40×10 (z 0..10) y un sketch en la cara de arriba con dos puntos.
fn plate() -> (Document, FeatureId) {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([0.0, 0.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let mut s = Sketch::default();
    for x in [-10.0, 10.0] {
        let p = s.add_point(x, 0.0);
        s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Point { point: p }, construction: false });
        s.next_id += 1;
    }
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: ev.face_ref(top).unwrap() }, offset: 0.0, sketch: s });
    (doc, sk)
}

fn hole(sketch: FeatureId, diameter: f64, depth: HoleDepth, style: HoleStyle) -> FeatureKind {
    FeatureKind::Hole(Hole { sketch, points: vec![], diameter, depth, style, tip_angle: 0.0, thread: None })
}

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

#[test]
fn through_counterbore_and_countersink() {
    if !occt() {
        return;
    }
    let (mut doc, sk) = plate();
    let h = doc.add(hole(sk, 6.6, HoleDepth::ThroughAll, HoleStyle::Simple));
    let plate = 40.0 * 40.0 * 10.0;
    let a = PI * 3.3 * 3.3;
    assert_relative_eq!(vol(&doc), plate - 2.0 * a * 10.0, max_relative = 1e-6);
    // Caja de 11 × 6
    if let FeatureKind::Hole(x) = &mut doc.get_mut(h).unwrap().kind {
        x.style = HoleStyle::Counterbore { diameter: 11.0, depth: 6.0 };
    }
    let cb = PI * 5.5 * 5.5 * 6.0 + a * 4.0;
    assert_relative_eq!(vol(&doc), plate - 2.0 * cb, max_relative = 1e-6);
    // Avellanado de 12 a 90°: cono de 12 a 6,6 con alto 2,7
    if let FeatureKind::Hole(x) = &mut doc.get_mut(h).unwrap().kind {
        x.style = HoleStyle::Countersink { diameter: 12.0, angle: 90.0 };
    }
    let hgt = (12.0 - 6.6) / 2.0;
    let cone = PI * hgt / 3.0 * (36.0 + 3.3 * 3.3 + 6.0 * 3.3);
    assert_relative_eq!(vol(&doc), plate - 2.0 * (cone + a * (10.0 - hgt)), max_relative = 1e-6);
}

#[test]
fn blind_with_tip_and_no_points() {
    if !occt() {
        return;
    }
    let (mut doc, sk) = plate();
    let h = doc.add(FeatureKind::Hole(Hole {
        sketch: sk,
        points: vec![],
        diameter: 5.0,
        depth: HoleDepth::Blind { depth: 6.0 },
        style: HoleStyle::Simple,
        tip_angle: 118.0,
        thread: Some("M6".into()),
    }));
    let tip = 2.5 / (59.0f64).to_radians().tan();
    let one = PI * 6.25 * 6.0 + PI * 6.25 * tip / 3.0;
    assert_relative_eq!(vol(&doc), 16000.0 - 2.0 * one, max_relative = 1e-6);
    // El fondo no llega abajo: la cara de abajo sigue entera
    let ev = doc.evaluate();
    let (bottom, _) = ev.body.as_ref().unwrap().closest_face([0.0, 0.0, 0.0], Some([0.0, 0.0, -1.0]), 0.99).unwrap();
    assert_relative_eq!(ev.body.as_ref().unwrap().face_info(bottom).unwrap().area, 1600.0, max_relative = 1e-9);
    // Un sketch sin puntos ni círculos: error claro
    let mut doc2 = Document::new();
    let empty = doc2.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: Sketch::default() });
    let h2 = doc2.add(hole(empty, 5.0, HoleDepth::ThroughAll, HoleStyle::Simple));
    assert!(matches!(doc2.evaluate().state(h2), Some(FeatureState::Error { .. })));
    let _ = h;
}
