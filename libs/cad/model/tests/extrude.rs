//! Variantes de la extrusión: dos lados, hasta la siguiente, desmolde y delgada.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn square(size: f64) -> Sketch {
    let mut s = Sketch::default();
    let h = size / 2.0;
    let p = [[-h, -h], [h, -h], [h, h], [-h, h]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

fn extrude(sketch: FeatureId, extent: Extent) -> Extrude {
    Extrude { sketch, regions: RegionSelection::All, extent, reverse: false, op: BodyOp::Join, draft: 0.0, thin: None }
}

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

fn bbox(doc: &Document) -> ([f64; 3], [f64; 3]) {
    let m = doc.evaluate().body.unwrap().mass().unwrap();
    (m.bbox_min, m.bbox_max)
}

#[test]
fn two_sides_and_up_to_next() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::TwoSides { distance: 7.0, second: 3.0 })));
    assert_relative_eq!(vol(&doc), 1000.0, max_relative = 1e-9);
    let (lo, hi) = bbox(&doc);
    assert_relative_eq!(lo[2], -3.0, epsilon = 1e-9);
    assert_relative_eq!(hi[2], 7.0, epsilon = 1e-9);

    // Una placa a z 12..15 y el cuadrado hasta la siguiente: llega a 12
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 30.0, dy: 30.0, dz: 3.0, centered: true, centered_z: false },
        origin: [0.0, 0.0, 12.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::UpToNext)));
    assert_relative_eq!(vol(&doc), 30.0 * 30.0 * 3.0 + 100.0 * 12.0, max_relative = 1e-9);
    assert_eq!(doc.evaluate().parts.len(), 1, "llega y se une a la placa");
}

#[test]
fn draft_and_thin() {
    if !occt() {
        return;
    }
    // Cuadrado de 10, alto 2, paredes a 45°: tronco de pirámide 10 → 6
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    let e = doc.add(FeatureKind::Extrude(Extrude { draft: 45.0, ..extrude(sk, Extent::Blind { distance: 2.0 }) }));
    let frustum = 2.0 / 3.0 * (100.0 + 36.0 + 60.0);
    assert_relative_eq!(vol(&doc), frustum, max_relative = 1e-6);
    // Hacia el otro lado, igual
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.reverse = true;
    }
    assert_relative_eq!(vol(&doc), frustum, max_relative = 1e-6);
    assert!(bbox(&doc).1[2] < 1e-6);
    // Simétrica con desmolde: dos troncos que se angostan hacia afuera
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.reverse = false;
        x.extent = Extent::Symmetric { distance: 4.0 };
    }
    assert_relative_eq!(vol(&doc), 2.0 * frustum, max_relative = 1e-6);

    // Delgada: pared de 1 alrededor del cuadrado de 10, alto 5
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    doc.add(FeatureKind::Extrude(Extrude { thin: Some(1.0), ..extrude(sk, Extent::Blind { distance: 5.0 }) }));
    let ring = (100.0 + 4.0 * 10.0 * 0.5 + PI * 0.25) - 81.0;
    assert_relative_eq!(vol(&doc), ring * 5.0, max_relative = 1e-6);
}

#[test]
fn old_extrudes_still_read() {
    let json = r#"{"type":"extrude","sketch":0,"extent":{"type":"blind","distance":5},"op":"join"}"#;
    let k: FeatureKind = serde_json::from_str(json).unwrap();
    let FeatureKind::Extrude(e) = k else { panic!() };
    assert_eq!(e.draft, 0.0);
    assert_eq!(e.thin, None);
    assert!(!serde_json::to_string(&FeatureKind::Extrude(e)).unwrap().contains("draft"));
}

/// Sketch con la horizontal según un eje y la normal invertida: el plano gira
/// y la extrusión sale hacia el otro lado.
#[test]
fn sketch_x_axis_and_flipped_normal() {
    if !occt() {
        return;
    }
    // Rectángulo 20 × 4 desde el origen: con x a lo largo de Y del mundo queda parado
    let mut s = Sketch::default();
    let p = [[0.0, 0.0], [20.0, 0.0], [20.0, 4.0], [0.0, 4.0]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s.x_axis = Some(AxisSpec::Y);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s.clone() });
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 5.0 })));
    let ev = doc.evaluate();
    let plane = ev.sketches[&sk].plane;
    assert_relative_eq!(plane.x_dir[1], 1.0, epsilon = 1e-12);
    // Lo que daría cada horizontal (para girar el dibujo al cambiarla): la del plano es X
    assert_relative_eq!(ev.sketch_x(sk, None).unwrap()[0], 1.0, epsilon = 1e-12);
    assert_relative_eq!(ev.sketch_x(sk, Some(&AxisSpec::Y)).unwrap()[1], 1.0, epsilon = 1e-12);
    // Z es perpendicular a la planta: queda la del plano
    assert_relative_eq!(ev.sketch_x(sk, Some(&AxisSpec::Z)).unwrap()[0], 1.0, epsilon = 1e-12);
    let (lo, hi) = bbox(&doc);
    assert_relative_eq!(hi[1] - lo[1], 20.0, epsilon = 1e-9);
    assert_relative_eq!(hi[0] - lo[0], 4.0, epsilon = 1e-9);
    assert_relative_eq!(lo[2], 0.0, epsilon = 1e-9);

    // Normal invertida: hacia −Z
    s.flip_normal = true;
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 5.0 })));
    assert_relative_eq!(vol(&doc), 400.0, max_relative = 1e-9);
    let (lo, hi) = bbox(&doc);
    assert_relative_eq!(lo[2], -5.0, epsilon = 1e-9);
    assert_relative_eq!(hi[2], 0.0, epsilon = 1e-9);
    // La dependencia del eje sale en las del sketch (acá ninguna: es un eje del mundo)
    assert!(doc.features[0].kind.dependencies().is_empty());
}
