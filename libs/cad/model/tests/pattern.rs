//! Patrones por tabla y de relleno.

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

fn volume(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

/// Placa de 100 × 60 × 2 con un agujero de Ø4 en (10, 10); devuelve el documento y el agujero
fn plate_with_hole() -> (Document, FeatureId) {
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: rect_sketch(0.0, 0.0, 100.0, 60.0) });
    doc.add(FeatureKind::Extrude(Extrude { sketch: sk, regions: RegionSelection::All, extent: Extent::Blind { distance: 2.0 }, reverse: false, op: BodyOp::Join, draft: 0.0, thin: None, direction: None, start: 0.0 }));
    let mut c = Sketch::default();
    let o = c.add_point(10.0, 10.0);
    c.add_entity(Geometry::Circle { center: o, radius: 2.0 });
    let hs = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: c });
    let hole = doc.add(FeatureKind::Extrude(Extrude { sketch: hs, regions: RegionSelection::All, extent: Extent::ThroughAll, reverse: false, op: BodyOp::Cut, draft: 0.0, thin: None, direction: None, start: 0.0 }));
    (doc, hole)
}

#[test]
fn table_pattern_copies_by_offsets() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    doc.add(FeatureKind::Pattern { features: vec![hole], pattern: PatternKind::Table { offsets: vec![[30.0, 0.0, 0.0], [0.0, 25.0, 0.0], [70.0, 40.0, 0.0]] }, skip: vec![], vary: None });
    assert_relative_eq!(volume(&doc), 12000.0 - 4.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
}

#[test]
fn fill_pattern_square_and_hex() {
    if !occt() {
        return;
    }
    for (hex, holes) in [(false, 45.0), (true, 43.0)] {
        let (mut doc, hole) = plate_with_hole();
        // Región de 5 a 95 por 5 a 55, a 3 del borde: centros en [8, 92] × [8, 52]
        let region = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: rect_sketch(5.0, 5.0, 95.0, 55.0) });
        doc.add(FeatureKind::Pattern {
            features: vec![hole],
            pattern: PatternKind::Fill { sketch: region, regions: RegionSelection::All, spacing: 10.0, hex, margin: 3.0 },
            skip: vec![], vary: None,
        });
        assert_relative_eq!(volume(&doc), 12000.0 - holes * PI * 4.0 * 2.0, max_relative = 1e-6);
    }
}

#[test]
fn circular_pattern_in_rings() {
    if !occt() {
        return;
    }
    // Eje en el centro de la placa; el agujero (10, 10) y su opuesto (90, 50)
    let axis = AxisSpec::Custom { origin: [50.0, 30.0, 0.0], direction: [0.0, 0.0, 1.0] };
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Circular { axis: axis.clone(), count: 2, angle: 360.0, rings: Some(CircularRings { count: 2, spacing: 5.0 }) },
        skip: vec![], vary: None,
    });
    // Original, su opuesto y los dos del anillo de afuera (todavía dentro de la placa)
    assert_relative_eq!(volume(&doc), 12000.0 - 4.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
    // Se puede omitir una copia del anillo (la 3: primera del anillo de afuera)
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![2];
    }
    assert_relative_eq!(volume(&doc), 12000.0 - 3.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
    // Sobre el eje no hay hacia dónde sacar los anillos
    let (mut doc, hole) = plate_with_hole();
    doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Circular { axis: AxisSpec::Custom { origin: [10.0, 10.0, 0.0], direction: [0.0, 0.0, 1.0] }, count: 2, angle: 360.0, rings: Some(CircularRings { count: 2, spacing: 5.0 }) },
        skip: vec![], vary: None,
    });
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn variable_pattern_grows_each_copy() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 3, spacing: 20.0, second: None },
        skip: vec![],
        vary: Some(PatternVary { scale: 50.0, axis: None }),
    });
    // Radios 2, 3 y 4: el agujero crece la mitad del original en cada copia
    assert_relative_eq!(volume(&doc), 12000.0 - PI * (4.0 + 9.0 + 16.0) * 2.0, max_relative = 1e-6);
    // Solo a lo largo de X: elipses de semiejes (2·f, 2)
    if let FeatureKind::Pattern { vary, .. } = &mut doc.get_mut(p).unwrap().kind {
        *vary = Some(PatternVary { scale: 50.0, axis: Some([1.0, 0.0, 0.0]) });
    }
    // La transformación no uniforme aproxima la superficie: tolerancia más suelta
    assert_relative_eq!(volume(&doc), 12000.0 - PI * (4.0 + 6.0 + 8.0) * 2.0, max_relative = 1e-3);
    // Si encoge hasta cero, avisa
    if let FeatureKind::Pattern { vary, .. } = &mut doc.get_mut(p).unwrap().kind {
        *vary = Some(PatternVary { scale: -60.0, axis: None });
    }
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn spiral_pattern_turns_and_moves_in() {
    if !occt() {
        return;
    }
    let axis = AxisSpec::Custom { origin: [50.0, 30.0, 0.0], direction: [0.0, 0.0, 1.0] };
    let hole_v = PI * 4.0 * 2.0;
    // Sin giro: una fila hacia el eje (el agujero queda a 44,7 del eje, cada copia 10 mm más cerca)
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Spiral { axis: axis.clone(), count: 3, angle: 0.0, step: -10.0 },
        skip: vec![],
        vary: None,
    });
    assert_relative_eq!(volume(&doc), 12000.0 - 3.0 * hole_v, max_relative = 1e-6);
    // Con media vuelta por copia, la segunda sale del otro lado del eje
    if let FeatureKind::Pattern { pattern, .. } = &mut doc.get_mut(p).unwrap().kind {
        *pattern = PatternKind::Spiral { axis: axis.clone(), count: 2, angle: 180.0, step: -10.0 };
    }
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let m = ev.body.unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 12000.0 - 2.0 * hole_v, max_relative = 1e-6);
    // Sobre el eje no hay hacia dónde abrirla
    let (mut doc, hole) = plate_with_hole();
    doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Spiral { axis: AxisSpec::Custom { origin: [10.0, 10.0, 0.0], direction: [0.0, 0.0, 1.0] }, count: 3, angle: 30.0, step: 5.0 },
        skip: vec![],
        vary: None,
    });
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn surface_pattern_follows_a_curved_wall() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 10.0, height: 20.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let (i, _) = ev.body.as_ref().unwrap().closest_face([10.0, 0.0, 10.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    let wall = ev.face_ref(i).unwrap();
    let cyl = PI * 100.0 * 20.0;
    // Un taco de 2 × 2 × 2 clavado en la pared, del lado +X
    let stud = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 2.0, dy: 2.0, dz: 2.0, centered: true, centered_z: true },
        origin: [10.5, 0.0, 10.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let one = volume(&doc) - cyl;
    assert!(one > 1.0 && one < 8.0, "{one}");
    // Cuatro alrededor: giran con la pared, todos iguales por simetría
    let p = doc.add(FeatureKind::Pattern {
        features: vec![stud],
        pattern: PatternKind::Surface { face: wall.clone(), count_u: 4, count_v: 1 },
        skip: vec![],
        vary: None,
    });
    assert_relative_eq!(volume(&doc) - cyl, 4.0 * one, max_relative = 1e-4);
    // Omitir una copia
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![2];
    }
    assert_relative_eq!(volume(&doc) - cyl, 3.0 * one, max_relative = 1e-4);
}

#[test]
fn pattern_of_a_pattern_repeats_the_block() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let row = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 3, spacing: 20.0, second: None },
        skip: vec![],
        vary: None,
    });
    assert_relative_eq!(volume(&doc), 12000.0 - 3.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
    // La fila entera, repetida 30 mm más arriba
    doc.add(FeatureKind::Pattern {
        features: vec![row],
        pattern: PatternKind::Linear { direction: [0.0, 1.0, 0.0], count: 2, spacing: 30.0, second: None },
        skip: vec![],
        vary: None,
    });
    assert_relative_eq!(volume(&doc), 12000.0 - 6.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
}
