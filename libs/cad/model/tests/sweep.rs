//! Barrido y transición.

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

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

#[test]
fn straight_sweep_is_an_extrusion_and_l_path_adds_up() {
    if !occt() {
        return;
    }
    // Círculo de radio 2 en la planta; camino: recta de 20 hacia arriba (en el
    // plano XZ las coordenadas del sketch son x, z)
    let mut doc = Document::new();
    let mut prof = Sketch::default();
    prof.circle([0.0, 0.0], 2.0);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: prof });
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    path.add_line(a, b);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path.clone() });
    let sw = doc.add(FeatureKind::Sweep(Sweep {
        sketch: p,
        regions: RegionSelection::All,
        path: SweepPath::Sketch { sketch: c, entities: vec![] },
        op: BodyOp::Join, orient: SweepOrient::Follow, circle: None, thin: None, guide: None, twist: None,
    }));
    assert_relative_eq!(vol(&doc), PI * 4.0 * 20.0, max_relative = 1e-6);
    // Camino en L (arriba 20, luego 10 de costado), con las entidades al revés: se encadenan solas
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    let c2 = path.add_point(10.0, 20.0);
    path.add_line(c2, b);
    path.add_line(a, b);
    let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(c).unwrap().kind else { unreachable!() };
    *sketch = path;
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!(ev.body.as_ref().unwrap().is_valid(), "esquina a inglete: sólido válido");
    let v = ev.body.unwrap().mass().unwrap().volume;
    // Entre las dos rectas (30 de largo) menos lo que se pisa en la esquina
    assert!(v > PI * 4.0 * 25.0 && v < PI * 4.0 * 30.0, "{v}");
    assert!(doc.get(sw).unwrap().kind.dependencies().contains(&c));
}

#[test]
fn loft_between_squares() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    let b = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 10.0, sketch: square(10.0) });
    let l = doc.add(FeatureKind::Loft(Loft {
        sections: vec![LoftSection { sketch: a, regions: RegionSelection::All }, LoftSection { sketch: b, regions: RegionSelection::All }],
        ruled: true,
        op: BodyOp::Join, start_point: None, end_point: None, closed: false, thin: None, path: None, guide: None,
    }));
    assert_relative_eq!(vol(&doc), 1000.0, max_relative = 1e-6);
    // De 10 a 6 reglada: tronco de pirámide
    let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(b).unwrap().kind else { unreachable!() };
    *sketch = square(6.0);
    assert_relative_eq!(vol(&doc), 10.0 / 3.0 * (100.0 + 36.0 + 60.0), max_relative = 1e-6);
    // Con una sola sección: error claro
    if let FeatureKind::Loft(x) = &mut doc.get_mut(l).unwrap().kind {
        x.sections.pop();
    }
    assert!(matches!(doc.evaluate().state(l), Some(FeatureState::Error { .. })));
}

#[test]
fn spring_along_a_helix_and_thicken() {
    if !occt() {
        return;
    }
    // Hélice de radio 10, paso 5, 2 vueltas sobre Z; perfil: círculo de radio 1 en
    // el plano XZ donde arranca la hélice
    let mut doc = Document::new();
    let hx = doc.add(FeatureKind::Helix { axis: AxisSpec::Z, radius: 10.0, pitch: 5.0, turns: 2.0, left: false, radius2: None, flat: false });
    let ev = doc.evaluate();
    let Some(RefGeom::Curve { points }) = ev.references.get(&hx) else { panic!("sin curva") };
    let start = points[0];
    assert!((start[0].hypot(start[1]) - 10.0).abs() < 1e-6 && start[2].abs() < 1e-6, "{start:?}");
    // Perfil en el plano que contiene el eje y el arranque (normal tangente a la hélice ≈ ⊥ radio)
    let radial = [start[0] / 10.0, start[1] / 10.0, 0.0];
    let tangent = [-radial[1], radial[0], 0.0];
    let plane = Plane { origin: start, normal: tangent, x_dir: radial };
    let mut prof = Sketch::default();
    prof.circle([0.0, 0.0], 1.0);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Custom { plane }, offset: 0.0, sketch: prof });
    doc.add(FeatureKind::Sweep(Sweep { sketch: p, regions: RegionSelection::All, path: SweepPath::Curve { feature: hx }, op: BodyOp::Join, orient: SweepOrient::Follow, circle: None, thin: None, guide: None, twist: None }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let len = 2.0 * ((2.0 * PI * 10.0f64).powi(2) + 25.0).sqrt();
    let v = ev.body.as_ref().unwrap().mass().unwrap().volume;
    assert!((v - PI * len).abs() < 0.02 * PI * len, "{v} vs {}", PI * len);

    // Engrosar la cara de arriba de una caja 2 mm: suma una placa encima
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([5.0, 5.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    doc.add(FeatureKind::Thicken { faces: vec![ev.face_ref(top).unwrap()], thickness: 2.0, op: BodyOp::Join });
    assert_relative_eq!(vol(&doc), 1200.0, max_relative = 1e-6);
    assert_relative_eq!(doc.evaluate().body.unwrap().mass().unwrap().bbox_max[2], 12.0, epsilon = 1e-6);
}

/// Camino: cuarto de círculo de radio 10 en el frente, que sale del origen
/// hacia arriba (Z) y termina horizontal en (10, 10)
fn arc_path(doc: &mut Document) -> FeatureId {
    let mut path = Sketch::default();
    let c = path.add_point(10.0, 0.0);
    let a = path.add_point(10.0, 10.0);
    let b = path.add_point(0.0, 0.0);
    path.add_entity(Geometry::Arc { center: c, start: a, end: b });
    doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path })
}

fn tube(sketch: FeatureId, path: FeatureId, circle: Option<f64>, thin: Option<f64>, orient: SweepOrient) -> FeatureKind {
    FeatureKind::Sweep(Sweep { sketch, regions: RegionSelection::All, path: SweepPath::Sketch { sketch: path, entities: vec![] }, op: BodyOp::Join, orient, circle, thin, guide: None, twist: None })
}

#[test]
fn circular_and_thin_sweeps() {
    if !occt() {
        return;
    }
    // Recta de 20: tubo macizo de radio 2 sin sketch de perfil
    let mut doc = Document::new();
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    path.add_line(a, b);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path });
    let sw = doc.add(tube(c, c, Some(2.0), None, SweepOrient::Follow));
    assert_relative_eq!(vol(&doc), PI * 4.0 * 20.0, max_relative = 1e-6);
    // Delgado: caño de radio medio 3 y pared de 1
    doc.get_mut(sw).unwrap().kind = tube(c, c, Some(3.0), Some(1.0), SweepOrient::Follow);
    assert_relative_eq!(vol(&doc), PI * (3.5 * 3.5 - 2.5 * 2.5) * 20.0, max_relative = 1e-6);
    // Radio cero: error
    doc.get_mut(sw).unwrap().kind = tube(c, c, Some(0.0), None, SweepOrient::Follow);
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn fixed_orientation_keeps_the_profile_still() {
    if !occt() {
        return;
    }
    // Cuadrado de 4 en la planta por el cuarto de círculo. Siguiendo, al final
    // queda parado (llega a z = 12); fijo, sigue horizontal (no pasa de z = 10)
    let top = |orient| {
        let mut doc = Document::new();
        let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(4.0) });
        let path = arc_path(&mut doc);
        doc.add(tube(p, path, None, None, orient));
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{orient:?}: {:?}", ev.errors());
        ev.body.unwrap().mass().unwrap().bbox_max[2]
    };
    assert_relative_eq!(top(SweepOrient::Follow), 12.0, epsilon = 1e-3);
    assert!(top(SweepOrient::Fixed) < 10.0 + 1e-3);
}

#[test]
fn twisted_sweep_keeps_the_volume_and_turns_the_profile() {
    if !occt() {
        return;
    }
    let twisted = |path: FeatureId, doc: &mut Document, p: FeatureId, deg: f64| {
        let FeatureKind::Sweep(sw) = tube(p, path, None, None, SweepOrient::Follow) else { unreachable!() };
        doc.add(FeatureKind::Sweep(Sweep { twist: Some(deg), ..sw }))
    };
    // Recta de 20 con 45°: la esquina del cuadrado de 4 pasa por el eje x (llega a 2√2)
    let mut doc = Document::new();
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(4.0) });
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    path.add_line(a, b);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path });
    twisted(c, &mut doc, p, 45.0);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let m = ev.body.unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 16.0 * 20.0, max_relative = 1e-5);
    assert_relative_eq!(m.bbox_max[0], 2.0 * 2f64.sqrt(), epsilon = 1e-2);
    // Por el cuarto de círculo de radio 10 con 90°: el volumen es el mismo que sin torsión
    let mut doc = Document::new();
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(4.0) });
    let path = arc_path(&mut doc);
    twisted(path, &mut doc, p, 90.0);
    assert_relative_eq!(vol(&doc), 16.0 * PI / 2.0 * 10.0, max_relative = 1e-4);
}

#[test]
fn surface_sweep_of_an_open_curve() {
    if !occt() {
        return;
    }
    // Una recta de 6 en la planta barrida 20 hacia arriba: superficie de 120
    let mut doc = Document::new();
    let mut prof = Sketch::default();
    let a = prof.add_point(-3.0, 0.0);
    let b = prof.add_point(3.0, 0.0);
    prof.add_line(a, b);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: prof });
    let mut path = Sketch::default();
    let c0 = path.add_point(0.0, 0.0);
    let c1 = path.add_point(0.0, 20.0);
    path.add_line(c0, c1);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path });
    doc.add(FeatureKind::SurfaceSweep { sketch: p, entities: vec![], path: SweepPath::Sketch { sketch: c, entities: vec![] }, orient: SweepOrient::Follow });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 1);
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(ev.parts[0].shape.mass().unwrap().area, 120.0, max_relative = 1e-6);
}

fn loft(sections: Vec<FeatureId>) -> Loft {
    Loft {
        sections: sections.into_iter().map(|sketch| LoftSection { sketch, regions: RegionSelection::All }).collect(),
        ruled: true,
        op: BodyOp::Join,
        start_point: None,
        end_point: None,
        closed: false,
        thin: None,
        path: None,
        guide: None,
    }
}

#[test]
fn loft_to_a_point_thin_and_along_a_path() {
    if !occt() {
        return;
    }
    // Cuadrado de 10 a z = 0 hasta el punto (0, 0, 10): pirámide
    let mut doc = Document::new();
    let a = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    let l = doc.add(FeatureKind::Loft(Loft { end_point: Some(PointSpec::At { point: [0.0, 0.0, 10.0] }), ..loft(vec![a]) }));
    assert_relative_eq!(vol(&doc), 1000.0 / 3.0, max_relative = 1e-6);

    // Delgada entre dos cuadrados iguales (z = 0 y 10): un caño cuadrado de pared 1
    let b = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 10.0, sketch: square(10.0) });
    doc.get_mut(l).unwrap().kind = FeatureKind::Loft(Loft { thin: Some(1.0), ..loft(vec![a, b]) });
    // Mover la transición después del segundo sketch
    let f = doc.features.iter().position(|x| x.id == l).unwrap();
    let moved = doc.features.remove(f);
    doc.features.push(moved);
    let outer = 100.0 + 0.5 * 40.0 + PI * 0.25;
    assert_relative_eq!(vol(&doc), (outer - 81.0) * 10.0, max_relative = 1e-4);

    // Por una línea central recta de 10: como la transición de siempre
    let mut path = Sketch::default();
    let p0 = path.add_point(0.0, 0.0);
    let p1 = path.add_point(0.0, 10.0);
    path.add_line(p0, p1);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path });
    doc.get_mut(l).unwrap().kind = FeatureKind::Loft(Loft { path: Some(SweepPath::Sketch { sketch: c, entities: vec![] }), ..loft(vec![a, b]) });
    let f = doc.features.iter().position(|x| x.id == l).unwrap();
    let moved = doc.features.remove(f);
    doc.features.push(moved);
    assert_relative_eq!(vol(&doc), 1000.0, max_relative = 1e-4);

    // Cerrada con dos secciones: error
    doc.get_mut(l).unwrap().kind = FeatureKind::Loft(Loft { closed: true, ..loft(vec![a, b]) });
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn surface_loft_between_two_lines() {
    if !occt() {
        return;
    }
    let line = |doc: &mut Document, z: f64| {
        let mut s = Sketch::default();
        let a = s.add_point(-5.0, 0.0);
        let b = s.add_point(5.0, 0.0);
        s.add_line(a, b);
        doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: z, sketch: s })
    };
    let mut doc = Document::new();
    let a = line(&mut doc, 0.0);
    let b = line(&mut doc, 10.0);
    doc.add(FeatureKind::SurfaceLoft { sections: vec![CurveSection { sketch: a, entities: vec![] }, CurveSection { sketch: b, entities: vec![] }], ruled: true });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(ev.parts[0].shape.mass().unwrap().area, 100.0, max_relative = 1e-6);
}
