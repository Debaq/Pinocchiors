//! Cotas: medidas guardadas en el diseño entre puntos con nombre.

use approx::assert_relative_eq;
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn point(doc: &mut Document, p: P3) -> PointSpec {
    let id = doc.add(FeatureKind::Point { def: PointSpec::At { point: p } });
    PointSpec::Reference { feature: id }
}

fn value(doc: &Document, id: FeatureId) -> (f64, String) {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    match ev.references.get(&id) {
        Some(RefGeom::Dimension { value, unit, lines, .. }) => {
            assert!(!lines.is_empty());
            (*value, unit.clone())
        }
        other => panic!("sin cota: {other:?}"),
    }
}

#[test]
fn distance_axis_and_angle_between_named_points() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = point(&mut doc, [0.0, 0.0, 0.0]);
    let b = point(&mut doc, [3.0, 4.0, 12.0]);
    let c = point(&mut doc, [10.0, 0.0, 0.0]);
    let d = doc.add(FeatureKind::Dimension { measure: DimensionKind::Distance { a: a.clone(), b: b.clone(), axis: None }, parameter: None });
    assert_eq!(value(&doc, d), (13.0, "mm".to_string()));
    let dz = doc.add(FeatureKind::Dimension { measure: DimensionKind::Distance { a: a.clone(), b: b.clone(), axis: Some(DimAxis::Z) }, parameter: None });
    assert_relative_eq!(value(&doc, dz).0, 12.0, epsilon = 1e-12);
    // Ángulo en a entre c (sobre X) y un punto a 45° en el plano XY
    let e = point(&mut doc, [5.0, 5.0, 0.0]);
    let an = doc.add(FeatureKind::Dimension { measure: DimensionKind::Angle { a: c, vertex: a.clone(), b: e }, parameter: None });
    let (deg, unit) = value(&doc, an);
    assert_relative_eq!(deg, 45.0, epsilon = 1e-9);
    assert_eq!(unit, "°");
    // La cota sigue al punto: se mueve b y cambia
    let PointSpec::Reference { feature: bid } = b else { unreachable!() };
    if let FeatureKind::Point { def } = &mut doc.get_mut(bid).unwrap().kind {
        *def = PointSpec::At { point: [0.0, 0.0, 20.0] };
    }
    assert_relative_eq!(value(&doc, d).0, 20.0, epsilon = 1e-12);
    // El vértice repetido: error
    let bad = doc.add(FeatureKind::Dimension { measure: DimensionKind::Angle { a: a.clone(), vertex: a.clone(), b }, parameter: None });
    let ev = doc.evaluate();
    assert!(ev.errors().iter().any(|(id, _)| *id == bad));
}

#[test]
fn scan_dimensions_use_what_the_app_measured() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = point(&mut doc, [0.0, 0.0, 0.0]);
    let b = point(&mut doc, [10.0, 0.0, 0.0]);
    // Sin escaneo (sin lo que pone la app): avisan
    let s = doc.add(FeatureKind::Dimension { measure: DimensionKind::Surface { points: vec![a.clone(), b.clone()], path: vec![] }, parameter: None });
    let di = doc.add(FeatureKind::Dimension { measure: DimensionKind::Diameter { at: a.clone(), fit: None }, parameter: None });
    let th = doc.add(FeatureKind::Dimension { measure: DimensionKind::Thickness { at: a.clone(), span: None }, parameter: None });
    let ev = doc.evaluate();
    for id in [s, di, th] {
        assert!(ev.errors().iter().any(|(e, _)| *e == id), "{id:?}");
    }
    // Con lo medido: el largo del camino, el doble del radio y el tramo
    doc.get_mut(s).unwrap().kind =
        FeatureKind::Dimension { measure: DimensionKind::Surface { points: vec![a.clone(), b], path: vec![[0.0, 0.0, 0.0], [5.0, 0.0, 5.0], [10.0, 0.0, 0.0]] }, parameter: None };
    doc.get_mut(di).unwrap().kind = FeatureKind::Dimension {
        measure: DimensionKind::Diameter { at: a.clone(), fit: Some(DimCylinder { origin: [0.0, 0.0, 0.0], direction: [0.0, 0.0, 1.0], radius: 4.0, at: [4.0, 0.0, 2.0] }) },
        parameter: None,
    };
    doc.get_mut(th).unwrap().kind = FeatureKind::Dimension { measure: DimensionKind::Thickness { at: a, span: Some([[0.0, 0.0, 0.0], [0.0, 0.0, -3.0]]) }, parameter: None };
    assert_relative_eq!(value(&doc, s).0, 2.0 * 50f64.sqrt(), epsilon = 1e-12);
    assert_relative_eq!(value(&doc, di).0, 8.0, epsilon = 1e-12);
    assert_relative_eq!(value(&doc, th).0, 3.0, epsilon = 1e-12);
}

#[test]
fn a_dimension_drives_a_parameter() {
    if !occt() {
        return;
    }
    // La distancia entre dos puntos da su medida al parámetro «ancho» (lo crea)
    let mut doc = Document::new();
    let a = point(&mut doc, [0.0, 0.0, 0.0]);
    let b = point(&mut doc, [0.0, 42.5, 0.0]);
    let d = doc.add(FeatureKind::Dimension { measure: DimensionKind::Distance { a, b: b.clone(), axis: None }, parameter: Some("ancho".into()) });
    let param = |doc: &Document| doc.evaluate().parameters.iter().find(|p| p.key == "ancho").and_then(|p| p.value);
    assert_relative_eq!(param(&doc).unwrap(), 42.5, epsilon = 1e-9);
    // Si ya existía, manda la cota; y otro parámetro puede usarlo
    doc.parameters.push(Parameter { name: "ancho".into(), expr: "10".into() });
    doc.parameters.push(Parameter { name: "doble".into(), expr: "ancho * 2".into() });
    let ev = doc.evaluate();
    let get = |k: &str| ev.parameters.iter().find(|p| p.key == k).and_then(|p| p.value).unwrap();
    assert_relative_eq!(get("ancho"), 42.5, epsilon = 1e-9);
    assert_relative_eq!(get("doble"), 85.0, epsilon = 1e-9);
    // Cambia el punto (el escaneo se alineó): cambia el parámetro
    let PointSpec::Reference { feature } = b else { unreachable!() };
    if let FeatureKind::Point { def } = &mut doc.get_mut(feature).unwrap().kind {
        *def = PointSpec::At { point: [0.0, 50.0, 0.0] };
    }
    assert_relative_eq!(param(&doc).unwrap(), 50.0, epsilon = 1e-9);
    assert!(doc.evaluate().status.iter().any(|s| s.id == d && matches!(s.state, FeatureState::Ok)));
}

#[test]
fn pattern_at_reference_points() {
    if !occt() {
        return;
    }
    // Un cubo de 2 en el origen, repetido en tres puntos (el original queda en el más cercano)
    let mut doc = Document::new();
    let cube = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 2.0, dy: 2.0, dz: 2.0, centered: true, centered_z: true },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
        on: None,
    }));
    let pts = vec![point(&mut doc, [0.0, 0.0, 0.0]), point(&mut doc, [10.0, 0.0, 0.0]), point(&mut doc, [0.0, 10.0, 5.0])];
    doc.add(FeatureKind::Pattern { features: vec![cube], pattern: PatternKind::AtPoints { points: pts }, skip: vec![], vary: None });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let m = ev.body.unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 24.0, max_relative = 1e-9);
    assert_relative_eq!(m.bbox_max[0], 11.0, epsilon = 1e-9);
    assert_relative_eq!(m.bbox_max[2], 6.0, epsilon = 1e-9);
}
