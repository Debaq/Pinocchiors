//! Operaciones del plan de operaciones (F1–F3, las fáciles): simetría como
//! pieza aparte, mover/girar/copiar cuerpos, patrones con copias omitidas y en
//! dos direcciones, revolución simétrica o a dos ángulos y extrusión hasta un punto.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn cube(x: f64, size: f64, op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: size, dy: size, dz: size, centered: false, centered_z: false },
        origin: [x, 0.0, 0.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op,
        link: None,
    })
}

fn eval_ok(doc: &Document) -> Evaluation {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev
}

fn bbox(ev: &Evaluation) -> ([f64; 3], [f64; 3], f64) {
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    (m.bbox_min, m.bbox_max, m.volume)
}

fn extrude(sketch: FeatureId, extent: Extent, op: BodyOp) -> FeatureKind {
    FeatureKind::Extrude(Extrude { sketch, regions: RegionSelection::All, extent, reverse: false, op, draft: 0.0, thin: None })
}

/// Placa de 100 × 60 × 2 con un agujero de Ø4 en (10, 10)
fn plate_with_hole() -> (Document, FeatureId) {
    let mut doc = Document::new();
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [100.0, 60.0]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(extrude(sk, Extent::Blind { distance: 2.0 }, BodyOp::Join));
    let mut c = Sketch::default();
    let o = c.add_point(10.0, 10.0);
    c.add_entity(Geometry::Circle { center: o, radius: 2.0 });
    let hs = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: c });
    let hole = doc.add(extrude(hs, Extent::ThroughAll, BodyOp::Cut));
    (doc, hole)
}

const HOLE: f64 = PI * 4.0 * 2.0;

#[test]
fn mirror_body_as_new_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(10.0, 10.0, BodyOp::Join));
    let m = doc.add(FeatureKind::Mirror { features: vec![], plane: PlaneSpec::Yz, op: BodyOp::New });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    assert_eq!(ev.parts[1].id.feature, m);
    for p in &ev.parts {
        assert_relative_eq!(p.shape.mass().unwrap().volume, 1000.0, max_relative = 1e-9);
    }
    let (lo, hi, _) = bbox(&ev);
    assert_relative_eq!(lo[0], -20.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 20.0, epsilon = 1e-6);
    // Unir (como antes): una sola pieza
    if let FeatureKind::Mirror { op, .. } = &mut doc.get_mut(m).unwrap().kind {
        *op = BodyOp::Join;
    }
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
}

#[test]
fn transform_moves_rotates_and_copies_parts() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let t = doc.add(FeatureKind::Transform { parts: vec![], motion: Motion::Translate { vector: [5.0, 0.0, 0.0] }, copy: false });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
    let (lo, hi, v) = bbox(&ev);
    assert_relative_eq!(lo[0], 5.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 15.0, epsilon = 1e-6);
    assert_relative_eq!(v, 1000.0, max_relative = 1e-9);

    // Girar 90° alrededor de Z: X pasa a Y
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform { parts: vec![], motion: Motion::Rotate { axis: AxisSpec::Z, angle: 90.0 }, copy: false };
    let (lo, hi, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[0], -10.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[1], 10.0, epsilon = 1e-6);

    // De un punto a otro
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform {
        parts: vec![],
        motion: Motion::Points { from: PointSpec::At { point: [0.0, 0.0, 0.0] }, to: PointSpec::At { point: [0.0, 0.0, 30.0] } },
        copy: false,
    };
    let (lo, _, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[2], 30.0, epsilon = 1e-6);

    // Copiar: la original queda y la copia es una pieza nueva
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform { parts: vec![], motion: Motion::Translate { vector: [20.0, 0.0, 0.0] }, copy: true };
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    assert_eq!(ev.parts[1].id.feature, t);
    let (lo, hi, v) = bbox(&ev);
    assert_relative_eq!(lo[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 30.0, epsilon = 1e-6);
    assert_relative_eq!(v, 2000.0, max_relative = 1e-9);
}

#[test]
fn transform_only_the_chosen_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let b = doc.add(cube(30.0, 10.0, BodyOp::New));
    doc.add(FeatureKind::Transform { parts: vec![PartId { feature: b, index: 0 }], motion: Motion::Translate { vector: [0.0, 0.0, 50.0] }, copy: false });
    let ev = eval_ok(&doc);
    let lo = |i: usize| ev.parts[i].shape.mass().unwrap().bbox_min;
    assert_relative_eq!(lo(0)[2], 0.0, epsilon = 1e-6);
    assert_relative_eq!(lo(1)[2], 50.0, epsilon = 1e-6);
}

#[test]
fn pattern_skips_copies() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 4, spacing: 20.0, second: None },
        skip: vec![2],
    });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 3.0 * HOLE, max_relative = 1e-6);
    // Todas las copias omitidas: queda el original
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![1, 2, 3];
    }
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - HOLE, max_relative = 1e-6);
}

#[test]
fn linear_pattern_in_two_directions() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear {
            direction: [1.0, 0.0, 0.0],
            count: 3,
            spacing: 20.0,
            second: Some(LinearSecond { direction: [0.0, 1.0, 0.0], count: 2, spacing: 30.0 }),
        },
        skip: vec![],
    });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 6.0 * HOLE, max_relative = 1e-6);
    // Copia 4 = segunda fila, segunda columna (k = j·3 + i)
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![4];
    }
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 5.0 * HOLE, max_relative = 1e-6);
}

/// Anillo de [5, 8] × [0, 20] en el plano frontal, girado alrededor de Z
fn ring(extent: RevolveExtent, angle: f64) -> Document {
    let mut s = Sketch::new();
    s.rectangle([5.0, 0.0], [8.0, 20.0]);
    let axis = s.line([0.0, 0.0], [0.0, 10.0]);
    s.entities.iter_mut().find(|e| e.id == axis).unwrap().construction = true;
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Revolve(Revolve {
        sketch: sk,
        regions: RegionSelection::All,
        axis: AxisSpec::SketchLine { sketch: sk, line: axis },
        angle,
        op: BodyOp::Join,
        extent,
    }));
    doc
}

#[test]
fn revolve_symmetric_and_two_angles() {
    if !occt() {
        return;
    }
    let full = PI * (64.0 - 25.0) * 20.0;
    // Simétrica: la mitad hacia cada lado del plano del sketch
    let (lo, hi, v) = bbox(&eval_ok(&ring(RevolveExtent::Symmetric, 90.0)));
    assert_relative_eq!(v, full / 4.0, max_relative = 1e-6);
    assert_relative_eq!(lo[1], -hi[1], epsilon = 1e-6);
    assert!(hi[1] > 1.0);
    // Dos ángulos: 90 + 30 = un tercio de vuelta
    let (_, _, v) = bbox(&eval_ok(&ring(RevolveExtent::TwoAngles { second: 30.0 }, 90.0)));
    assert_relative_eq!(v, full / 3.0, max_relative = 1e-6);
    // Más de una vuelta entre los dos: error
    let ev = ring(RevolveExtent::TwoAngles { second: 200.0 }, 200.0).evaluate();
    assert!(!ev.errors().is_empty());
    // Los documentos de antes (sin `extent`) siguen abriendo: un solo lado
    let json = r#"{"sketch":1,"axis":{"type":"z"},"angle":90.0}"#;
    let r: Revolve = serde_json::from_str(json).unwrap();
    assert_eq!(r.extent, RevolveExtent::One);
}

#[test]
fn extrude_up_to_vertex() {
    if !occt() {
        return;
    }
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let e = doc.add(extrude(sk, Extent::UpToVertex { point: PointSpec::At { point: [50.0, 50.0, 7.0] } }, BodyOp::Join));
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 700.0, max_relative = 1e-9);
    assert_relative_eq!(lo[2], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 7.0, epsilon = 1e-6);
    // Un punto del otro lado: va hacia allá
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.extent = Extent::UpToVertex { point: PointSpec::At { point: [0.0, 0.0, -4.0] } };
    }
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 400.0, max_relative = 1e-9);
    assert_relative_eq!(lo[2], -4.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 0.0, epsilon = 1e-6);
}
