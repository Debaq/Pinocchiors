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
    let cw = s.constrain(SketchConstraint::Length { line: l[0], value: w, reference: false });
    let ch = s.constrain(SketchConstraint::Length { line: l[1], value: h, reference: false });
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
    s.constrain(SketchConstraint::Length { line: l, value: 10.0, reference: false });
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
    s.constrain(SketchConstraint::Length { line: l, value: 10.0, reference: false });
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
        shape: PrimitiveShape::Box { dx: 40.0, dy: 20.0, dz: 10.0, centered: false },
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
        shape: PrimitiveShape::Box { dx: 1.0, dy: 1.0, dz: 1.0, centered: false },
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
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 30.0, centered: false },
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
        shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 20.0, centered: false },
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

#[test]
fn tangent_arcs_sharing_an_end() {
    // Dos arcos encadenados: al mover el centro del segundo, sigue tangente
    let mut s = Sketch::new();
    let c1 = s.add_point(0.0, 0.0);
    let a = s.add_point(10.0, 0.0);
    let p = s.add_point(0.0, 10.0);
    let c2 = s.add_point(0.0, 15.0);
    let b = s.add_point(0.0, 20.0);
    let arc1 = s.add_entity(Geometry::Arc { center: c1, start: a, end: p });
    let arc2 = s.add_entity(Geometry::Arc { center: c2, start: b, end: p });
    s.constrain(SketchConstraint::Fixed { point: c1, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Fixed { point: p, x: 0.0, y: 10.0 });
    s.constrain(SketchConstraint::Tangent { a: arc1, b: arc2 });
    // Arrastrar el centro del segundo fuera de la recta: vuelve a ella
    let r = s.solve_drag(c2, [3.0, 14.0]).unwrap();
    assert!(r.residual < 1e-8, "{r:?}");
    // c1, p y c2 alineados
    let (c1p, pp, c2p) = (s.point(c1).unwrap(), s.point(p).unwrap(), s.point(c2).unwrap());
    let cross = (pp[0] - c1p[0]) * (c2p[1] - c1p[1]) - (pp[1] - c1p[1]) * (c2p[0] - c1p[0]);
    assert!(cross.abs() < 1e-6, "centros alineados con el contacto");
}

#[test]
fn sketch_origin_is_fixed_and_survives_deletes() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    assert_eq!(s.ensure_origin(), o, "no se duplica");
    // Línea desde el origen, horizontal y con largo: queda definida sin "fijo"
    let p = s.add_point(3.0, 0.5);
    let l = s.add_line(o, p);
    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::Length { line: l, value: 10.0, reference: false });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_eq!(s.point(o).unwrap(), [0.0, 0.0]);
    assert_relative_eq!(s.point(p).unwrap()[0], 10.0, epsilon = 1e-6);
    // Arrastrar el origen no lo mueve
    s.solve_drag(o, [5.0, 5.0]).unwrap();
    assert_relative_eq!(s.point(o).unwrap()[0], 0.0, epsilon = 1e-9);
    // Borrar la línea deja el origen
    s.remove_entity(l).unwrap();
    assert!(s.point(o).is_ok());
    assert!(s.point(p).is_err());
    // Sketch sin entidades: solo el origen, nada libre
    let r = s.solve().unwrap();
    assert_eq!(r.dof, 0, "{r:?}");
}

#[test]
fn sketch_points_aligned_horizontally_and_vertically() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let a = s.add_point(4.0, 0.3);
    let b = s.add_point(-0.2, 7.0);
    s.constrain(SketchConstraint::HorizontalPoints { a: o, b: a });
    s.constrain(SketchConstraint::VerticalPoints { a: o, b });
    s.solve().unwrap();
    assert_relative_eq!(s.point(a).unwrap()[1], 0.0, epsilon = 1e-6);
    assert_relative_eq!(s.point(b).unwrap()[0], 0.0, epsilon = 1e-6);
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("\"horizontal_points\"") && json.contains("\"origin\""), "{json}");
}

#[test]
fn old_sketch_without_origin_still_loads() {
    let json = r#"{"points":[{"id":0,"x":0,"y":0},{"id":1,"x":5,"y":0}],
        "entities":[{"id":2,"geometry":{"type":"line","start":0,"end":1}}],"constraints":[],"next_id":3}"#;
    let mut s: Sketch = serde_json::from_str(json).unwrap();
    assert_eq!(s.origin, None);
    assert!(!serde_json::to_string(&s).unwrap().contains("origin"));
    let o = s.ensure_origin();
    assert_eq!(o, 3, "id nuevo, no pisa los existentes");
}

fn radius_of(s: &Sketch, id: u32) -> f64 {
    let Geometry::Circle { radius, .. } = s.entity(id).unwrap().geometry else { unreachable!() };
    radius
}

fn center_of(s: &Sketch, id: u32) -> [f64; 2] {
    let Geometry::Circle { center, .. } = s.entity(id).unwrap().geometry else { unreachable!() };
    s.point(center).unwrap()
}

#[test]
fn circle_radius_is_solved_through_three_points() {
    // Círculo por tres puntos fijos: centro y radio salen del solver
    let mut s = Sketch::new();
    let c = s.circle([1.0, 1.0], 2.0);
    for (x, y) in [(5.0, 0.0), (-5.0, 0.0), (0.0, 5.0)] {
        let p = s.add_point(x, y);
        s.constrain(SketchConstraint::Fixed { point: p, x, y });
        s.constrain(SketchConstraint::PointOnCircle { point: p, circle: c });
    }
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(radius_of(&s, c), 5.0, epsilon = 1e-6);
    let o = center_of(&s, c);
    assert_relative_eq!(o[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(o[1], 0.0, epsilon = 1e-6);
}

#[test]
fn circle_without_dimension_has_free_radius() {
    let mut s = Sketch::new();
    let c = s.circle([3.0, 4.0], 2.5);
    let Geometry::Circle { center, .. } = s.entity(c).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: center, x: 3.0, y: 4.0 });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::UnderConstrained);
    assert_eq!(r.dof, 1, "solo el radio");
    assert_relative_eq!(radius_of(&s, c), 2.5, epsilon = 1e-9);
    s.constrain(SketchConstraint::Diameter { entity: c, value: 8.0, reference: false });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(radius_of(&s, c), 4.0, epsilon = 1e-6);
}

#[test]
fn circle_tangent_to_two_lines_and_equal_tangent_circles() {
    // Esquina en L fija; círculo tangente a las dos con radio 3 → centro (3, 3)
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let a = s.add_point(20.0, 0.0);
    let b = s.add_point(0.0, 20.0);
    let h = s.add_line(o, a);
    let v = s.add_line(o, b);
    for (p, x, y) in [(a, 20.0, 0.0), (b, 0.0, 20.0)] {
        s.constrain(SketchConstraint::Fixed { point: p, x, y });
    }
    let c1 = s.circle([4.0, 5.0], 2.0);
    s.constrain(SketchConstraint::Tangent { a: h, b: c1 });
    s.constrain(SketchConstraint::Tangent { a: v, b: c1 });
    s.constrain(SketchConstraint::Radius { entity: c1, value: 3.0, reference: false });
    // Otro igual, tangente por fuera y a la misma altura → centro (9, 3)
    let c2 = s.circle([10.0, 4.0], 2.0);
    s.constrain(SketchConstraint::Equal { a: c1, b: c2 });
    s.constrain(SketchConstraint::Tangent { a: c1, b: c2 });
    s.constrain(SketchConstraint::Tangent { a: h, b: c2 });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let (p1, p2) = (center_of(&s, c1), center_of(&s, c2));
    assert_relative_eq!(p1[0], 3.0, epsilon = 1e-6);
    assert_relative_eq!(p1[1], 3.0, epsilon = 1e-6);
    assert_relative_eq!(p2[0], 9.0, epsilon = 1e-6);
    assert_relative_eq!(p2[1], 3.0, epsilon = 1e-6);
    assert_relative_eq!(radius_of(&s, c2), 3.0, epsilon = 1e-6);
}

#[test]
fn concentric_and_internal_tangency() {
    let mut s = Sketch::new();
    let outer = s.circle([0.0, 0.0], 10.0);
    let Geometry::Circle { center, .. } = s.entity(outer).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: center, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Radius { entity: outer, value: 10.0, reference: false });
    // Concéntrico con radio 4
    let ring = s.circle([0.5, 0.3], 4.2);
    s.constrain(SketchConstraint::Concentric { a: outer, b: ring });
    s.constrain(SketchConstraint::Radius { entity: ring, value: 4.0, reference: false });
    // Tangente por dentro al de afuera, de radio 2, sobre el eje x positivo
    let inner = s.circle([7.5, 0.4], 2.2);
    s.constrain(SketchConstraint::Tangent { a: outer, b: inner });
    s.constrain(SketchConstraint::Radius { entity: inner, value: 2.0, reference: false });
    let Geometry::Circle { center: ci, .. } = s.entity(inner).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::HorizontalPoints { a: center, b: ci });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(center_of(&s, ring)[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(center_of(&s, inner)[0], 8.0, epsilon = 1e-6);
}

#[test]
fn old_circle_without_dimension_keeps_its_radius() {
    let json = r#"{"points":[{"id":0,"x":2,"y":0}],
        "entities":[{"id":1,"geometry":{"type":"circle","center":0,"radius":7.5}}],"constraints":[],"next_id":2}"#;
    let mut s: Sketch = serde_json::from_str(json).unwrap();
    s.solve().unwrap();
    assert_relative_eq!(radius_of(&s, 1), 7.5, epsilon = 1e-9);
}

#[test]
fn free_entities_follow_what_is_still_undefined() {
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [10.0, 5.0]);
    let Geometry::Line { start: corner, .. } = s.entity(l[0]).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: corner, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Length { line: l[0], value: 10.0, reference: false });
    let r = s.solve().unwrap();
    // Falta el alto: abajo queda definida, el resto no
    assert!(!r.free_entities.contains(&l[0]), "{r:?}");
    for id in &l[1..] {
        assert!(r.free_entities.contains(id), "{id} {r:?}");
    }
    s.constrain(SketchConstraint::Length { line: l[1], value: 5.0, reference: false });
    let c = s.circle([5.0, 2.5], 1.0);
    let Geometry::Circle { center, .. } = s.entity(c).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: center, x: 5.0, y: 2.5 });
    let r = s.solve().unwrap();
    // Rectángulo definido; el círculo tiene el centro fijo pero el radio libre
    assert_eq!(r.free_entities, vec![c], "{r:?}");
}

#[test]
fn reference_dimension_measures_without_constraining() {
    let (mut s, l, [cw, _]) = plate_sketch(30.0, 40.0);
    let Geometry::Line { start: a, .. } = s.entity(l[0]).unwrap().geometry else { unreachable!() };
    let Geometry::Line { start: b, .. } = s.entity(l[2]).unwrap().geometry else { unreachable!() };
    // Diagonal de referencia: no sobre-define
    let d = s.constrain(SketchConstraint::Distance { a, b, value: 1.0, reference: true });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(s.constraints[d].value().unwrap(), 50.0, epsilon = 1e-6);
    // Cambia con la geometría
    s.constraints[cw].set_value(90.0);
    s.solve().unwrap();
    assert_relative_eq!(s.constraints[d].value().unwrap(), (90.0f64 * 90.0 + 40.0 * 40.0).sqrt(), epsilon = 1e-6);
    let json = serde_json::to_string(&s.constraints[d]).unwrap();
    assert!(json.contains("\"reference\":true"), "{json}");
    assert!(!serde_json::to_string(&s.constraints[cw]).unwrap().contains("reference"));
}

#[test]
fn repeated_dimension_is_reported_even_if_consistent() {
    let (mut s, l, _) = plate_sketch(30.0, 40.0);
    let extra = s.constrain(SketchConstraint::Length { line: l[2], value: 30.0, reference: false });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::OverConstrained, "{r:?}");
    assert_eq!(r.conflicting, vec![extra]);
    // Como referencia ya no sobra
    let SketchConstraint::Length { reference, .. } = &mut s.constraints[extra] else { unreachable!() };
    *reference = true;
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
}

#[test]
fn loose_point_entity_does_not_form_regions() {
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    let p = s.add_point(5.0, 5.0);
    let e = s.add_entity(Geometry::Point { point: p });
    assert_eq!(find_regions(&s).unwrap().len(), 1);
    // Borrar la entidad borra el punto
    s.remove_entity(e).unwrap();
    assert!(s.point(p).is_err());
    let json = serde_json::to_string(&Geometry::Point { point: 3 }).unwrap();
    assert_eq!(json, r#"{"type":"point","point":3}"#);
}

#[test]
fn linear_pattern_copies_follow_spacing_and_size() {
    let (mut s, l, [cw, _]) = plate_sketch(10.0, 5.0);
    s.solve().unwrap();
    // Copia de los 4 puntos, corrida 20 en x; solo el primer par lleva cotas
    let orig: Vec<u32> = l.iter().map(|&id| match s.entity(id).unwrap().geometry {
        Geometry::Line { start, .. } => start,
        _ => unreachable!(),
    }).collect();
    let copy: Vec<u32> = orig.iter().map(|&p| {
        let q = s.point(p).unwrap();
        s.add_point(q[0] + 20.0, q[1])
    }).collect();
    for i in 0..4 {
        s.add_line(copy[i], copy[(i + 1) % 4]);
    }
    let gap = s.constrain(SketchConstraint::HorizontalDistance { a: orig[0], b: copy[0], value: 20.0, reference: false });
    s.constrain(SketchConstraint::VerticalDistance { a: orig[0], b: copy[0], value: 0.0, reference: false });
    for i in 1..4 {
        s.constrain(SketchConstraint::EqualOffset { a1: orig[0], a2: copy[0], b1: orig[i], b2: copy[i] });
    }
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    s.constraints[gap].set_value(30.0);
    s.constraints[cw].set_value(12.0);
    s.solve().unwrap();
    // Esquina opuesta de la copia: (30 + 12, 5)
    let q = s.point(copy[2]).unwrap();
    assert_relative_eq!(q[0], 42.0, epsilon = 1e-6);
    assert_relative_eq!(q[1], 5.0, epsilon = 1e-6);
}

#[test]
fn circular_pattern_copies_rotate_together() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let seg = s.line([10.0, 0.0], [14.0, 1.0]);
    let Geometry::Line { start: p0, end: p1 } = s.entity(seg).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: p0, x: 10.0, y: 0.0 });
    s.constrain(SketchConstraint::Fixed { point: p1, x: 14.0, y: 1.0 });
    // Copia girada 90°: el primer par con radios iguales y ángulo; el resto, mismo giro
    let (q0, q1) = (s.add_point(0.0, 10.0), s.add_point(-1.0, 14.0));
    s.add_line(q0, q1);
    let r0 = s.add_line(o, p0);
    let r1 = s.add_line(o, q0);
    for id in [r0, r1] {
        s.entities.iter_mut().find(|e| e.id == id).unwrap().construction = true;
    }
    s.constrain(SketchConstraint::Equal { a: r0, b: r1 });
    let ang = s.constrain(SketchConstraint::Angle { a: r0, b: r1, degrees: 90.0, reference: false });
    s.constrain(SketchConstraint::EqualRotation { center: o, a1: p0, a2: q0, b1: p1, b2: q1 });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    // A 150°: la copia de (14, 1) es (14, 1) girado 150°
    s.constraints[ang].set_value(150.0);
    s.solve().unwrap();
    let (c, sn) = (150f64.to_radians().cos(), 150f64.to_radians().sin());
    let q = s.point(q1).unwrap();
    assert_relative_eq!(q[0], 14.0 * c - sn, epsilon = 1e-6);
    assert_relative_eq!(q[1], 14.0 * sn + c, epsilon = 1e-6);
}

#[test]
fn ellipse_is_dimensioned_and_extruded_exactly() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    // Dibujada torcida y fuera de medida
    let (m, n) = (s.add_point(4.0, 0.5), s.add_point(-0.3, 2.5));
    s.add_entity(Geometry::Ellipse { center: o, major: m, minor: n });
    s.constrain(SketchConstraint::Distance { a: o, b: m, value: 5.0, reference: false });
    s.constrain(SketchConstraint::Distance { a: o, b: n, value: 2.0, reference: false });
    let r = s.solve().unwrap();
    assert_eq!(r.dof, 1, "falta el giro: {r:?}");
    s.constrain(SketchConstraint::HorizontalPoints { a: o, b: m });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let q = s.point(n).unwrap();
    assert_relative_eq!(q[0].abs(), 0.0, epsilon = 1e-6);
    assert_relative_eq!(q[1].abs(), 2.0, epsilon = 1e-6);
    assert_eq!(find_regions(&s).unwrap().len(), 1);
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 3.0 },
        reverse: false,
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    assert_relative_eq!(volume(&ev), PI * 5.0 * 2.0 * 3.0, max_relative = 1e-6);
}

#[test]
fn spline_handles_set_end_tangents() {
    let build = |handles: bool| {
        let mut s = Sketch::new();
        let p: Vec<u32> = [[0.0, 0.0], [5.0, 3.0], [10.0, 0.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
        let (h0, h1) = if handles { (Some(s.add_point(0.0, 2.0)), Some(s.add_point(10.0, -2.0))) } else { (None, None) };
        s.add_entity(Geometry::Spline { points: p.clone(), closed: false, start_handle: h0, end_handle: h1 });
        s.add_line(p[2], p[0]);
        for &id in &p {
            let q = s.point(id).unwrap();
            s.constrain(SketchConstraint::Fixed { point: id, x: q[0], y: q[1] });
        }
        s
    };
    let s = build(true);
    // Las manijas son puntos de la spline: se borran con ella
    let mut t = s.clone();
    let spline = t.entities[0].id;
    t.remove_entity(spline).unwrap();
    assert_eq!(t.points.len(), 2, "quedan los dos de la línea (sin el del medio ni las manijas)");
    if !occt() {
        return;
    }
    let vol = |s: Sketch| {
        let mut doc = Document::new();
        let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
        doc.add(FeatureKind::Extrude(Extrude {
            sketch: sk,
            regions: RegionSelection::All,
            extent: Extent::Blind { distance: 1.0 },
            reverse: false,
            op: BodyOp::Join,
        }));
        let ev = doc.evaluate();
        assert_all_ok(&ev);
        volume(&ev)
    };
    let (free, held) = (vol(build(false)), vol(build(true)));
    assert!(held > free * 1.05, "{free} → {held}");
}

#[test]
fn centered_box() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    // Eje Y: la base queda en el plano XZ, centrada en X y Z
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 20.0, dz: 10.0, centered: true },
        origin: [0.0; 3],
        z: [0.0, 1.0, 0.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert_all_ok(&ev);
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    for (a, b) in m.bbox_min.iter().zip([-20.0, 0.0, -10.0]).chain(m.bbox_max.iter().zip([20.0, 10.0, 10.0])) {
        assert_relative_eq!(*a, b, epsilon = 1e-6);
    }
    // Sin el campo (documentos viejos): desde la esquina
    let old: PrimitiveShape = serde_json::from_str(r#"{"type":"box","dx":1,"dy":2,"dz":3}"#).unwrap();
    assert_eq!(old, PrimitiveShape::Box { dx: 1.0, dy: 2.0, dz: 3.0, centered: false });
}
