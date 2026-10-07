//! Mover caras, escalar y patrón a lo largo de una curva.

use approx::assert_relative_eq;
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn cube() -> Document {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    doc
}

fn eval_ok(doc: &Document) -> Evaluation {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev
}

#[test]
fn move_face_out_and_in() {
    if !occt() {
        return;
    }
    let mut doc = cube();
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([5.0, 5.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let mv = doc.add(FeatureKind::MoveFace { faces: vec![ev.face_ref(top).unwrap()], distance: 3.0 });
    let ev = eval_ok(&doc);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 1300.0, max_relative = 1e-9);
    assert_relative_eq!(m.bbox_max[2], 13.0, epsilon = 1e-9);
    assert_eq!(ev.body.as_ref().unwrap().face_count(), 6, "sigue siendo una caja");
    // Hacia adentro
    if let FeatureKind::MoveFace { distance, .. } = &mut doc.get_mut(mv).unwrap().kind {
        *distance = -4.0;
    }
    assert_relative_eq!(eval_ok(&doc).body.unwrap().mass().unwrap().volume, 600.0, max_relative = 1e-9);
}

#[test]
fn scale_about_a_point() {
    if !occt() {
        return;
    }
    let mut doc = cube();
    doc.add(FeatureKind::Scale { factor: [2.0, 1.0, 0.5], center: PointSpec::At { point: [10.0, 0.0, 0.0] } });
    let m = eval_ok(&doc).body.unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 1000.0, max_relative = 1e-9);
    // La caja envolvente trae la tolerancia del sólido
    assert_relative_eq!(m.bbox_min[0], -10.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[0], 10.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[2], 5.0, epsilon = 1e-6);
}

#[test]
fn pattern_along_a_sketch_path() {
    if !occt() {
        return;
    }
    // Cubo de 2 en el origen y un camino en L de 30 + 10 en la planta: 5 copias cada 10
    let mut doc = Document::new();
    let c = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 2.0, dy: 2.0, dz: 2.0, centered: true, centered_z: true },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let mut s = Sketch::default();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(30.0, 0.0);
    let d = s.add_point(30.0, 10.0);
    s.add_line(a, b);
    s.add_line(b, d);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let p = doc.add(FeatureKind::Pattern {
        features: vec![c],
        pattern: PatternKind::Curve { path: SweepPath::Sketch { sketch: sk, entities: vec![] }, count: 5 },
    });
    let ev = eval_ok(&doc);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 5.0 * 8.0, max_relative = 1e-9);
    assert_relative_eq!(m.bbox_max[0], 31.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[1], 11.0, epsilon = 1e-6);
    assert!(doc.get(p).unwrap().kind.dependencies().contains(&sk));
}
