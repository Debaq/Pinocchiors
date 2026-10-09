//! Superficies: extruidas, de revolución, relleno, coser y engrosar.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn area(s: &Shape) -> f64 {
    s.faces().unwrap().iter().map(|f| f.area).sum()
}

fn evaluate(doc: &Document) -> Evaluation {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev
}

/// Sketch en XY con un círculo de radio `r` en el origen
fn circle(doc: &mut Document, r: f64) -> FeatureId {
    let mut s = Sketch::default();
    let c = s.add_point(0.0, 0.0);
    s.add_entity(Geometry::Circle { center: c, radius: r });
    doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s })
}

#[test]
fn open_curve_extruded_is_a_surface() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let mut s = Sketch::default();
    let p = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    s.add_line(p[0], p[1]);
    s.add_line(p[1], p[2]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::SurfaceExtrude { sketch: sk, entities: vec![], extent: Extent::Symmetric { distance: 4.0 }, reverse: false });
    let ev = evaluate(&doc);
    assert_eq!(ev.parts.len(), 1);
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(area(&ev.parts[0].shape), 15.0 * 4.0, epsilon = 1e-9);
    let m = ev.parts[0].shape.mass().unwrap();
    assert_relative_eq!(m.bbox_min[2], -2.0, epsilon = 1e-9);
    assert_relative_eq!(m.bbox_max[2], 2.0, epsilon = 1e-9);
}

#[test]
fn can_from_a_tube_two_fills_and_sewing() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = circle(&mut doc, 5.0);
    let tube = doc.add(FeatureKind::SurfaceExtrude { sketch: sk, entities: vec![], extent: Extent::Blind { distance: 10.0 }, reverse: false });
    // Una caja aparte: las superficies no se le unen y la caja no se corta
    let ev = evaluate(&doc);
    assert_relative_eq!(area(ev.body.as_ref().unwrap()), 2.0 * PI * 5.0 * 10.0, max_relative = 1e-9);
    // Tapas: relleno de cada círculo del borde
    let lid = |doc: &mut Document, z: f64| {
        let ev = evaluate(doc);
        let (e, _) = ev.body.as_ref().unwrap().closest_edge([-5.0, 0.0, z], None, 0.0).unwrap();
        let e = ev.edge_ref(e).unwrap();
        doc.add(FeatureKind::Fill { edges: vec![e], tangent: false })
    };
    let bottom = lid(&mut doc, 0.0);
    let top = lid(&mut doc, 10.0);
    let ev = evaluate(&doc);
    assert_eq!(ev.parts.len(), 3);
    assert!(ev.parts.iter().all(|p| p.is_surface()));
    let ids: Vec<PartId> = ev.parts.iter().map(|p| p.id).collect();
    assert_eq!(ids.iter().map(|p| p.feature).collect::<Vec<_>>(), vec![tube, bottom, top]);
    doc.add(FeatureKind::Sew { parts: ids, solid: true, tolerance: 0.01 });
    let ev = evaluate(&doc);
    assert_eq!(ev.parts.len(), 1);
    assert!(!ev.parts[0].is_surface());
    assert!(ev.parts[0].shape.is_valid());
    assert_relative_eq!(ev.parts[0].shape.mass().unwrap().volume, PI * 25.0 * 10.0, max_relative = 1e-6);
}

#[test]
fn revolved_arc_and_open_sewing() {
    if !occt() {
        return;
    }
    // Medio círculo de radio 5 en XZ girado alrededor de Z: una esfera de superficie
    let mut doc = Document::new();
    let mut s = Sketch::default();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(0.0, -5.0);
    let b = s.add_point(0.0, 5.0);
    s.add_entity(Geometry::Arc { center: c, start: a, end: b });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    doc.add(FeatureKind::SurfaceRevolve { sketch: sk, entities: vec![], axis: AxisSpec::Z, angle: 360.0 });
    let ev = evaluate(&doc);
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(area(&ev.parts[0].shape), 4.0 * PI * 25.0, max_relative = 1e-6);
    // Coser una sola superficie cerrada como sólido
    let id = ev.parts[0].id;
    doc.add(FeatureKind::Sew { parts: vec![id], solid: true, tolerance: 0.01 });
    let ev = evaluate(&doc);
    assert_relative_eq!(ev.parts[0].shape.mass().unwrap().volume, 4.0 / 3.0 * PI * 125.0, max_relative = 1e-4);
}

#[test]
fn thicken_a_surface_keeps_it_and_does_not_join_it() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = circle(&mut doc, 5.0);
    doc.add(FeatureKind::SurfaceExtrude { sketch: sk, entities: vec![], extent: Extent::Blind { distance: 10.0 }, reverse: false });
    let ev = evaluate(&doc);
    let faces: Vec<FaceRef> = (0..ev.body.as_ref().unwrap().face_count()).map(|i| ev.face_ref(i).unwrap()).collect();
    doc.add(FeatureKind::Thicken { faces, thickness: 1.0, op: BodyOp::Join });
    let ev = evaluate(&doc);
    // La superficie y el tubo de 1 de espesor (hacia afuera o hacia adentro)
    assert_eq!(ev.parts.len(), 2);
    let solid = ev.parts.iter().find(|p| !p.is_surface()).unwrap();
    let v = solid.shape.mass().unwrap().volume;
    let out = PI * (36.0 - 25.0) * 10.0;
    let inn = PI * (25.0 - 16.0) * 10.0;
    assert!((v - out).abs() < 1e-6 || (v - inn).abs() < 1e-6, "{v}");
}

#[test]
fn tangent_fill_closes_a_sphere_cut_at_45_degrees() {
    if !occt() {
        return;
    }
    // Esfera de radio 5 sin el casquete de arriba: arco de −90° a 45° girado
    // alrededor de Z; el borde de arriba queda a z = r = 5·sen 45°
    let mut doc = Document::new();
    let mut s = Sketch::default();
    let k = 5.0 * std::f64::consts::FRAC_1_SQRT_2;
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(0.0, -5.0);
    let b = s.add_point(k, k);
    s.add_entity(Geometry::Arc { center: c, start: a, end: b });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    doc.add(FeatureKind::SurfaceRevolve { sketch: sk, entities: vec![], axis: AxisSpec::Z, angle: 360.0 });
    let ev = evaluate(&doc);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    let rim = m.bbox_max[2];
    assert_relative_eq!(rim.abs(), k, epsilon = 1e-6);
    let (e, _) = ev.body.as_ref().unwrap().closest_edge([-k, 0.0, rim], None, 0.0).unwrap();
    let e = ev.edge_ref(e).unwrap();
    // Plano: una tapa de π·r²; tangente: abomba siguiendo la esfera (hasta cerca de 5)
    let flat = {
        let mut d = doc.clone();
        d.add(FeatureKind::Fill { edges: vec![e.clone()], tangent: false });
        evaluate(&d).parts[1].shape.clone()
    };
    assert_relative_eq!(area(&flat), PI * k * k, max_relative = 1e-6);
    doc.add(FeatureKind::Fill { edges: vec![e], tangent: true });
    let ev = evaluate(&doc);
    let lid = &ev.parts[1].shape;
    let lm = lid.mass().unwrap();
    let height = if rim > 0.0 { lm.bbox_max[2] - rim } else { rim - lm.bbox_min[2] };
    assert!(height > 0.8, "abomba {height}");
    // Cosida con la esfera: un sólido cerca del de la esfera entera
    let ids: Vec<PartId> = ev.parts.iter().map(|p| p.id).collect();
    doc.add(FeatureKind::Sew { parts: ids, solid: true, tolerance: 0.01 });
    let ev = evaluate(&doc);
    let v = ev.parts[0].shape.mass().unwrap().volume;
    assert_relative_eq!(v, 4.0 / 3.0 * PI * 125.0, max_relative = 0.03);
}

#[test]
fn trim_a_surface_with_a_plane_and_with_a_solid() {
    if !occt() {
        return;
    }
    // Tubo de superficie de radio 5 y alto 10
    let mut doc = Document::new();
    let sk = circle(&mut doc, 5.0);
    let tube = doc.add(FeatureKind::SurfaceExtrude { sketch: sk, entities: vec![], extent: Extent::Blind { distance: 10.0 }, reverse: false });
    let surface = evaluate(&doc).parts[0].id;
    // Cortar por un plano a z = 4, eligiendo la superficie: queda la parte de arriba
    let mut by_plane = doc.clone();
    let split = by_plane.add(FeatureKind::Split { plane: PlaneSpec::Custom { plane: Plane::XY.offset(4.0) }, flip: false });
    by_plane.get_mut(split).unwrap().scope = vec![surface];
    let ev = evaluate(&by_plane);
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(area(&ev.parts[0].shape), 2.0 * PI * 5.0 * 6.0, max_relative = 1e-6);
    // Con una caja que la atraviesa: restar se queda con lo de afuera, intersecar con lo de adentro
    for (op, h) in [(PartBoolean::Subtract, 7.0), (PartBoolean::Intersect, 3.0)] {
        let mut d = doc.clone();
        d.add(FeatureKind::Primitive(Primitive {
            shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 3.0, centered: true, centered_z: false },
            origin: [0.0, 0.0, 7.0],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::New,
            link: None,
        }));
        let ev = evaluate(&d);
        let solid = ev.parts.iter().find(|p| !p.is_surface()).unwrap().id;
        d.add(FeatureKind::Boolean { op, targets: vec![surface], tools: vec![solid], keep_tools: false });
        let ev = evaluate(&d);
        let s = ev.parts.iter().find(|p| p.id.feature == tube).unwrap();
        assert!(s.is_surface());
        assert_relative_eq!(area(&s.shape), 2.0 * PI * 5.0 * h, max_relative = 1e-6);
    }
}

#[test]
fn split_a_box_with_a_plane_and_with_a_curved_surface() {
    if !occt() {
        return;
    }
    let caja = |doc: &mut Document| {
        doc.add(FeatureKind::Primitive(Primitive {
            shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 20.0, centered: true, centered_z: true },
            origin: [0.0, 0.0, 0.0],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::New,
            link: None,
        }))
    };
    let volumes = |ev: &Evaluation| {
        let mut v: Vec<f64> = ev.parts.iter().filter(|p| !p.is_surface()).map(|p| p.shape.mass().unwrap().volume).collect();
        v.sort_by(f64::total_cmp);
        v
    };
    // Con el plano z = 3: arriba 7 de alto, abajo 13
    let mut doc = Document::new();
    caja(&mut doc);
    doc.add(FeatureKind::SplitBy { parts: vec![], tool: SplitTool::Plane { plane: PlaneSpec::Custom { plane: Plane::XY.offset(3.0) } } });
    let v = volumes(&evaluate(&doc));
    assert_eq!(v.len(), 2);
    assert_relative_eq!(v[0], 2800.0, max_relative = 1e-6);
    assert_relative_eq!(v[1], 5200.0, max_relative = 1e-6);
    // Con un tubo de superficie de radio 5 que la atraviesa: el cilindro de adentro y el resto
    let mut doc = Document::new();
    caja(&mut doc);
    let mut s = Sketch::default();
    let c = s.add_point(0.0, 0.0);
    s.add_entity(Geometry::Circle { center: c, radius: 5.0 });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: -15.0, sketch: s });
    doc.add(FeatureKind::SurfaceExtrude { sketch: sk, entities: vec![], extent: Extent::Blind { distance: 30.0 }, reverse: false });
    let tube = evaluate(&doc).parts.iter().find(|p| p.is_surface()).unwrap().id;
    doc.add(FeatureKind::SplitBy { parts: vec![], tool: SplitTool::Part { part: tube } });
    let ev = evaluate(&doc);
    let v = volumes(&ev);
    assert_eq!(v.len(), 2);
    assert_relative_eq!(v[0], PI * 25.0 * 20.0, max_relative = 1e-6);
    assert_relative_eq!(v[1], 8000.0 - PI * 25.0 * 20.0, max_relative = 1e-6);
    // La superficie sigue
    assert!(ev.parts.iter().any(|p| p.is_surface()));
}
