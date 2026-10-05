use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    if !occt::available() {
        eprintln!("OpenCASCADE no disponible: test omitido");
        return false;
    }
    true
}

fn volume(ev: &Evaluation) -> f64 {
    ev.body.as_ref().expect("sin cuerpo").mass().unwrap().volume
}

fn assert_all_ok(ev: &Evaluation) {
    assert!(ev.errors().is_empty(), "errores: {:?}", ev.errors());
}

/// Rectángulo 100×50 anclado en el origen, con cotas de largo.
fn plate_sketch(w: f64, h: f64) -> (Sketch, [u32; 4], [usize; 2]) {
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [w * 0.9, h * 1.1]); // fuera de medida a propósito
    let Geometry::Line { start: corner, .. } = s.entity(l[0]).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: corner, x: 0.0, y: 0.0 });
    let cw = s.constrain(SketchConstraint::Length { line: l[0], value: w });
    let ch = s.constrain(SketchConstraint::Length { line: l[1], value: h });
    (s, l, [cw, ch])
}

#[test]
fn sketch_solves_rectangle() {
    let (mut s, l, _) = plate_sketch(100.0, 50.0);
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_eq!(r.dof, 0);
    let Geometry::Line { end, .. } = s.entity(l[1]).unwrap().geometry else { unreachable!() };
    let p = s.point(end).unwrap();
    assert_relative_eq!(p[0], 100.0, epsilon = 1e-6);
    assert_relative_eq!(p[1], 50.0, epsilon = 1e-6);
}

#[test]
fn sketch_reports_conflicts_and_dof() {
    let mut s = Sketch::new();
    let l = s.line([0.0, 0.0], [10.0, 1.0]);
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::UnderConstrained);
    assert_eq!(r.dof, 4);

    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::Vertical { line: l });
    s.constrain(SketchConstraint::Length { line: l, value: 10.0 });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::OverConstrained, "{r:?}");
    assert!(!r.conflicting.is_empty());
}

#[test]
fn sketch_drag_respects_constraints() {
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b);
    s.constrain(SketchConstraint::Fixed { point: a, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Length { line: l, value: 10.0 });
    s.solve_drag(b, [0.0, 30.0]).unwrap();
    let p = s.point(b).unwrap();
    assert_relative_eq!(p[0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(p[1], 10.0, epsilon = 1e-4);
}

#[test]
fn regions_with_holes_splits_and_arcs() {
    // Rectángulo con círculo adentro: anillo (prof. 0) y disco (prof. 1)
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [100.0, 50.0]);
    s.circle([50.0, 25.0], 10.0);
    let r = find_regions(&s).unwrap();
    assert_eq!(r.len(), 2);
    let ring = r.iter().find(|r| r.depth == 0).unwrap();
    assert_eq!(ring.holes.len(), 1);
    assert_relative_eq!(ring.area(), 5000.0 - PI * 100.0, max_relative = 1e-3);
    assert!(!ring.contains([50.0, 25.0]));
    assert!(ring.contains(ring.sample));

    // Línea que parte el rectángulo en dos + rama suelta ignorada
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [100.0, 50.0]);
    let pts: Vec<u32> = l.iter().map(|id| match s.entity(*id).unwrap().geometry {
        Geometry::Line { start, .. } => start,
        _ => unreachable!(),
    }).collect();
    let _ = pts;
    let top = s.add_point(40.0, 50.0);
    let bottom = s.add_point(40.0, 0.0);
    // Partir las líneas inferior y superior en el punto de corte
    s.remove_entity(l[0]).unwrap();
    s.remove_entity(l[2]).unwrap();
    let p00 = s.add_point(0.0, 0.0);
    let p10 = s.add_point(100.0, 0.0);
    let p11 = s.add_point(100.0, 50.0);
    let p01 = s.add_point(0.0, 50.0);
    s.add_line(p00, bottom);
    s.add_line(bottom, p10);
    s.add_line(p11, top);
    s.add_line(top, p01);
    s.add_line(bottom, top);
    s.line([200.0, 0.0], [210.0, 0.0]); // suelta
    let r = find_regions(&s).unwrap();
    let mut areas: Vec<f64> = r.iter().map(|r| r.area()).collect();
    areas.sort_by(f64::total_cmp);
    assert_eq!(areas.len(), 2, "{areas:?}");
    assert_relative_eq!(areas[0], 2000.0, max_relative = 1e-9);
    assert_relative_eq!(areas[1], 3000.0, max_relative = 1e-9);

    // Ranura con arcos
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(20.0, 0.0);
    let c = s.add_point(20.0, 10.0);
    let d = s.add_point(0.0, 10.0);
    let c1 = s.add_point(20.0, 5.0);
    let c2 = s.add_point(0.0, 5.0);
    s.add_line(a, b);
    s.add_entity(Geometry::Arc { center: c1, start: b, end: c });
    s.add_line(c, d);
    s.add_entity(Geometry::Arc { center: c2, start: d, end: a });
    let r = find_regions(&s).unwrap();
    assert_eq!(r.len(), 1);
    assert_relative_eq!(r[0].area(), 200.0 + PI * 25.0, max_relative = 2e-3);
}

/// Placa con agujero y redondeo en una arista superior.
fn plate_doc() -> (Document, FeatureId, [usize; 2]) {
    let mut doc = Document::new();
    let (mut s, _, cons) = plate_sketch(100.0, 50.0);
    s.circle([50.0, 25.0], 10.0);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 10.0 },
        reverse: false,
        op: BodyOp::Join,
    }));
    (doc, sk, cons)
}

#[test]
fn extrude_plate_with_hole_and_change_parameters() {
    if !occt() {
        return;
    }
    let (mut doc, sk, cons) = plate_doc();
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), (5000.0 - PI * 100.0) * 10.0, max_relative = 1e-6);

    // Redondeo de la arista superior larga del frente (y = 0, z = 10)
    let body = ev.body.as_ref().unwrap();
    let (edge, _) = body.closest_edge([50.0, 0.0, 10.0], Some([1.0, 0.0, 0.0]), 0.9).unwrap();
    let r = ev.edge_ref(edge).unwrap();
    let fillet = doc.add(FeatureKind::Fillet { edges: vec![r], radius: 2.0 });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    let with_fillet = volume(&ev);
    assert_relative_eq!(with_fillet, (5000.0 - PI * 100.0) * 10.0 - (4.0 - PI) * 100.0, max_relative = 1e-6);

    // Cambiar el ancho a 120: el redondeo sigue encontrando su arista
    let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(sk).unwrap().kind else { unreachable!() };
    sketch.constraints[cons[0]].set_value(120.0);
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), (6000.0 - PI * 100.0) * 10.0 - (4.0 - PI) * 120.0, max_relative = 1e-6);
    assert_eq!(ev.state(fillet), Some(&FeatureState::Ok));
}

#[test]
fn pocket_on_face_through_all_and_up_to_face() {
    if !occt() {
        return;
    }
    let (mut doc, _, _) = plate_doc();
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let (top, _) = body.closest_face([10.0, 10.0, 10.0], Some([0.0, 0.0, 1.0]), 0.9).unwrap();
    let top_ref = ev.face_ref(top).unwrap();

    // Sketch sobre la tapa: cuadrado 10×10 en (10..20, 10..20)
    let mut s = Sketch::new();
    s.rectangle([10.0, 10.0], [20.0, 20.0]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: top_ref.clone() }, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::ThroughAll,
        reverse: true,
        op: BodyOp::Cut,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    let res = ev.sketches.get(&sk).unwrap();
    assert_relative_eq!(res.plane.origin[2], 10.0, epsilon = 1e-9);
    assert_relative_eq!(volume(&ev), (5000.0 - PI * 100.0 - 100.0) * 10.0, max_relative = 1e-6);

    // Tetón desde z=0 hasta la cara superior, fuera de la placa
    let mut s = Sketch::new();
    s.circle([150.0, 25.0], 5.0);
    let sk2 = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk2,
        regions: RegionSelection::All,
        extent: Extent::UpToFace { face: top_ref },
        reverse: false,
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), (5000.0 - PI * 100.0 - 100.0) * 10.0 + PI * 25.0 * 10.0, max_relative = 1e-6);
}

#[test]
fn revolve_around_sketch_line() {
    if !occt() {
        return;
    }
    // Perfil en el plano frontal (x → X, y → Z): rectángulo [5,8]×[0,20] y eje en x=0
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
        angle: 360.0,
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), PI * (64.0 - 25.0) * 20.0, max_relative = 1e-6);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    assert_relative_eq!(m.bbox_max[2], 20.0, epsilon = 1e-6); // creció en Z: plano frontal correcto
}

#[test]
fn circular_pattern_of_holes_and_mirror() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 50.0, height: 5.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let hole = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 3.0, height: 20.0 },
        origin: [35.0, 0.0, -5.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    }));
    doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Circular { axis: AxisSpec::Z, count: 6, angle: 360.0 },
    });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), PI * 2500.0 * 5.0 - 6.0 * PI * 9.0 * 5.0, max_relative = 1e-6);

    // Simetría de todo el cuerpo respecto de z = 0: el disco se duplica hacia abajo
    doc.add(FeatureKind::Mirror { features: vec![], plane: PlaneSpec::Xy });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), 2.0 * (PI * 2500.0 * 5.0 - 6.0 * PI * 9.0 * 5.0), max_relative = 1e-6);
}

#[test]
fn shell_split_and_linear_pattern() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 20.0, dz: 10.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let top = FaceRef { point: [20.0, 10.0, 10.0], normal: [0.0, 0.0, 1.0], ..Default::default() };
    doc.add(FeatureKind::Shell { faces: vec![top], thickness: 1.0 });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), 8000.0 - 38.0 * 18.0 * 9.0, max_relative = 1e-6);

    doc.add(FeatureKind::Split {
        plane: PlaneSpec::Custom { plane: Plane::YZ.offset(20.0) },
        flip: true,
    });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    assert_relative_eq!(m.bbox_max[0], 20.0, epsilon = 1e-6);

    let mut doc = Document::new();
    let pin = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 1.0, dy: 1.0, dz: 1.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    doc.add(FeatureKind::Pattern {
        features: vec![pin],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 5, spacing: 3.0 },
    });
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), 5.0, epsilon = 1e-9);
}

#[test]
fn errors_stay_local_rollback_and_suppress() {
    if !occt() {
        return;
    }
    let (mut doc, _, _) = plate_doc();
    // Redondeo imposible
    let bad = doc.add(FeatureKind::Fillet {
        edges: vec![EdgeRef { point: [50.0, 0.0, 10.0], direction: [1.0, 0.0, 0.0], ..Default::default() }],
        radius: 50.0,
    });
    let cut = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 30.0 },
        origin: [0.0, 0.0, -10.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    }));
    let ev = doc.evaluate();
    assert!(matches!(ev.state(bad), Some(FeatureState::Error { .. })));
    assert_eq!(ev.state(cut), Some(&FeatureState::Ok));
    let full = (5000.0 - PI * 100.0) * 10.0;
    assert_relative_eq!(volume(&ev), full - 1000.0, max_relative = 1e-6);

    doc.set_suppressed(cut, true).unwrap();
    let ev = doc.evaluate();
    assert_eq!(ev.state(cut), Some(&FeatureState::Suppressed));
    assert_relative_eq!(volume(&ev), full, max_relative = 1e-6);

    doc.rollback = Some(1);
    let ev = doc.evaluate();
    assert!(ev.body.is_none());
    assert_eq!(ev.status.iter().filter(|s| s.state == FeatureState::RolledBack).count(), 3);

    // Cortar sin cuerpo
    let mut d2 = Document::new();
    let c = d2.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Sphere { radius: 1.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    }));
    assert!(matches!(d2.evaluate().state(c), Some(FeatureState::Error { .. })));
}

#[test]
fn document_editing_rules() {
    let (mut doc, sk, _) = plate_doc();
    let ext = doc.features[1].id;
    assert_eq!(doc.features[1].name, "Extrusión 1");
    assert!(matches!(doc.remove(sk), Err(ModelError::HasDependents(_))));
    assert!(matches!(doc.move_feature(ext, 0), Err(ModelError::BreaksOrder(_))));
    doc.remove(ext).unwrap();
    doc.remove(sk).unwrap();
    assert!(doc.features.is_empty());

    // Agregar con retroceso activo inserta en esa posición
    let (mut doc, _, _) = plate_doc();
    doc.rollback = Some(1);
    let id = doc.add(FeatureKind::Split { plane: PlaneSpec::Xy, flip: false });
    assert_eq!(doc.index_of(id), Some(1));
    assert_eq!(doc.rollback, Some(2));
}

#[test]
fn serde_roundtrip_and_step_import() {
    if !occt() {
        return;
    }
    let (doc, _, _) = plate_doc();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("\"type\":\"extrude\""));
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
    let v = volume(&back.evaluate());

    // Exportar a STEP e importarlo como operación
    let step = doc.evaluate().body.unwrap().to_step().unwrap();
    let mut d2 = Document::new();
    d2.add(FeatureKind::Import { format: ImportFormat::Step, data: step, op: BodyOp::Join });
    let json = serde_json::to_string(&d2).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_relative_eq!(volume(&back.evaluate()), v, max_relative = 1e-6);
}

#[test]
fn cut_on_face_points_into_material() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 20.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let mut s = Sketch::new();
    s.circle([0.0, 0.0], 5.0);
    let top = FaceRef { point: [10.0, 10.0, 20.0], normal: [0.0, 0.0, 1.0], ..Default::default() };
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: top }, offset: 0.0, sketch: s });
    let cut = doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 5.0 },
        reverse: false, // hacia afuera de la cara: se da vuelta sola
        op: BodyOp::Cut,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    // El origen del sketch en una cara es la proyección del origen del mundo:
    // el círculo (0,0) r=5 queda en la esquina y corta un cuarto de cilindro
    assert_relative_eq!(volume(&ev), 8000.0 - PI * 25.0 * 5.0 / 4.0, max_relative = 1e-6);

    if let FeatureKind::Extrude(e) = &mut doc.get_mut(cut).unwrap().kind {
        e.extent = Extent::ThroughAll;
    }
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), 8000.0 - PI * 25.0 * 20.0 / 4.0, max_relative = 1e-6);
}
