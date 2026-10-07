//! Planos, ejes y puntos de referencia.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn rect_sketch(x0: f64, y0: f64, x1: f64, y1: f64) -> Sketch {
    let mut s = Sketch::default();
    let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

fn plane_of(ev: &Evaluation, id: FeatureId) -> Plane {
    match ev.references.get(&id) {
        Some(RefGeom::Plane { plane }) => *plane,
        other => panic!("sin plano: {other:?}"),
    }
}

fn axis_of(ev: &Evaluation, id: FeatureId) -> (P3, P3) {
    match ev.references.get(&id) {
        Some(RefGeom::Axis { origin, dir }) => (*origin, *dir),
        other => panic!("sin eje: {other:?}"),
    }
}

fn assert_parallel(a: P3, b: P3) {
    let c = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).abs();
    assert_relative_eq!(c, 1.0, epsilon = 1e-9);
}

#[test]
fn reference_planes() {
    let mut doc = Document::new();
    let off = doc.add(FeatureKind::Plane { def: PlaneDef::Offset { base: PlaneSpec::Xy, distance: 10.0 } });
    let ang = doc.add(FeatureKind::Plane { def: PlaneDef::Angle { base: PlaneSpec::Xy, axis: AxisSpec::X, angle: 90.0 } });
    let mid = doc.add(FeatureKind::Plane {
        def: PlaneDef::Midplane { a: PlaneSpec::Xy, b: PlaneSpec::Reference { feature: off } },
    });
    let three = doc.add(FeatureKind::Plane {
        def: PlaneDef::ThreePoints {
            points: [
                PointSpec::At { point: [0.0, 0.0, 5.0] },
                PointSpec::At { point: [1.0, 0.0, 5.0] },
                PointSpec::At { point: [0.0, 1.0, 5.0] },
            ],
        },
    });
    let bad = doc.add(FeatureKind::Plane {
        def: PlaneDef::ThreePoints {
            points: [
                PointSpec::At { point: [0.0; 3] },
                PointSpec::At { point: [1.0, 0.0, 0.0] },
                PointSpec::At { point: [2.0, 0.0, 0.0] },
            ],
        },
    });
    let ev = doc.evaluate();
    assert_relative_eq!(plane_of(&ev, off).origin[2], 10.0);
    assert_parallel(plane_of(&ev, ang).normal, [0.0, 1.0, 0.0]);
    assert_relative_eq!(plane_of(&ev, mid).origin[2], 5.0);
    let t = plane_of(&ev, three);
    assert_parallel(t.normal, [0.0, 0.0, 1.0]);
    assert_relative_eq!(t.origin[2], 5.0);
    assert!(matches!(ev.state(bad), Some(FeatureState::Error { .. })));
    assert_eq!(doc.get(mid).unwrap().kind.dependencies(), vec![off]);
}

#[test]
fn sketch_on_reference_plane_and_revolve_on_reference_axis() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let pl = doc.add(FeatureKind::Plane { def: PlaneDef::Offset { base: PlaneSpec::Xy, distance: 10.0 } });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Reference { feature: pl }, offset: 0.0, sketch: rect_sketch(0.0, 0.0, 10.0, 10.0) });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 5.0 },
        reverse: false,
        op: BodyOp::Join,
        draft: 0.0,
        thin: None,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    assert_relative_eq!(m.bbox_min[2], 10.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[2], 15.0, epsilon = 1e-6);
    // Mover el plano mueve la pieza
    if let FeatureKind::Plane { def: PlaneDef::Offset { distance, .. } } = &mut doc.get_mut(pl).unwrap().kind {
        *distance = 20.0;
    }
    let m = doc.evaluate().body.unwrap().mass().unwrap();
    assert_relative_eq!(m.bbox_min[2], 20.0, epsilon = 1e-6);

    // Anillo: rectángulo en el plano XZ (x 10..12, z 0..5) girado sobre un eje de
    // referencia por dos puntos (el Z)
    let mut doc = Document::new();
    let ax = doc.add(FeatureKind::Axis {
        def: AxisDef::TwoPoints { a: PointSpec::At { point: [0.0; 3] }, b: PointSpec::At { point: [0.0, 0.0, 1.0] } },
    });
    // En XZ las coordenadas del sketch son (x, z)
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: rect_sketch(10.0, 0.0, 12.0, 5.0) });
    doc.add(FeatureKind::Revolve(Revolve {
        sketch: sk,
        regions: RegionSelection::All,
        axis: AxisSpec::Reference { feature: ax },
        angle: 360.0,
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_relative_eq!(ev.body.unwrap().mass().unwrap().volume, PI * (144.0 - 100.0) * 5.0, max_relative = 1e-6);
}

#[test]
fn reference_axes_and_points() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 5.0, height: 10.0 },
        origin: [20.0, 0.0, 0.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let (side, _) = body.closest_face([25.0, 0.0, 5.0], None, 0.0).unwrap();
    let (rim, _) = body.closest_edge([25.0, 0.0, 10.0], None, 0.0).unwrap();
    let (side, rim) = (ev.face_ref(side).unwrap(), ev.edge_ref(rim).unwrap());
    let cyl = doc.add(FeatureKind::Axis { def: AxisDef::Face { face: side } });
    let planes = doc.add(FeatureKind::Axis { def: AxisDef::Planes { a: PlaneSpec::Xz, b: PlaneSpec::Yz } });
    let center = doc.add(FeatureKind::Point { def: PointSpec::Center { edge: rim } });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let (o, d) = axis_of(&ev, cyl);
    assert_parallel(d, [0.0, 0.0, 1.0]);
    assert_relative_eq!(o[0], 20.0, epsilon = 1e-9);
    let (o, d) = axis_of(&ev, planes);
    assert_parallel(d, [0.0, 0.0, 1.0]);
    assert_relative_eq!(o[0].abs() + o[1].abs(), 0.0, epsilon = 1e-9);
    match ev.references.get(&center) {
        Some(RefGeom::Point { point }) => {
            assert_relative_eq!(point[0], 20.0, epsilon = 1e-9);
            assert_relative_eq!(point[2], 10.0, epsilon = 1e-9);
        }
        other => panic!("{other:?}"),
    }
    // La caché guarda las referencias
    let mut cache = EvalCache::default();
    doc.evaluate_with(&mut cache);
    let again = doc.evaluate_with(&mut cache);
    assert_eq!(again.recomputed, 0);
    assert_eq!(again.references.len(), 3);
}
