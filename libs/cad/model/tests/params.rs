//! Parámetros con nombre y campos calculados por fórmula.

use approx::assert_relative_eq;
use cad_model::*;

fn param(name: &str, expr: &str) -> Parameter {
    Parameter { name: name.into(), expr: expr.into() }
}

fn volume(ev: &Evaluation) -> f64 {
    ev.body.as_ref().unwrap().mass().unwrap().volume
}

fn caja() -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 1.0, dy: 10.0, dz: 1.0, centered: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    })
}

#[test]
fn primitive_dimensions_follow_parameters() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let b = doc.add(caja());
    doc.parameters = vec![param("ancho", "40"), param("alto", "ancho / 2")];
    doc.bindings.insert(format!("{}.kind.shape.dx", b.0), "ancho".into());
    doc.bindings.insert(format!("{}.kind.shape.dz", b.0), "alto".into());
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_relative_eq!(volume(&ev), 40.0 * 10.0 * 20.0, epsilon = 1e-6);
    assert_eq!(ev.parameters[1].value, Some(20.0));
    assert!(ev.bindings.iter().all(|b| b.error.is_none()));

    doc.parameters[0].expr = "60".into();
    assert_relative_eq!(volume(&doc.evaluate()), 60.0 * 10.0 * 30.0, epsilon = 1e-6);
    // El documento guardado no cambia: las fórmulas se aplican al recalcular
    let FeatureKind::Primitive(p) = &doc.get(b).unwrap().kind else { unreachable!() };
    assert_eq!(p.shape, PrimitiveShape::Box { dx: 1.0, dy: 10.0, dz: 1.0, centered: false });
}

#[test]
fn sketch_dimensions_and_extrusion_follow_parameters() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let mut s = Sketch::new();
    let l = s.rectangle([0.0, 0.0], [30.0, 10.0]);
    let Geometry::Line { start, .. } = s.entity(l[0]).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: start, x: 0.0, y: 0.0 });
    let largo = s.constrain(SketchConstraint::Length { line: l[0], value: 30.0, reference: false });
    s.constrain(SketchConstraint::Length { line: l[1], value: 10.0, reference: false });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let ext = doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 5.0 },
        reverse: false,
        op: BodyOp::Join,
    }));
    doc.parameters = vec![param("largo", "50"), param("espesor", "largo * 0.1")];
    doc.bindings.insert(format!("{}.kind.sketch.constraints.{largo}.value", sk.0), "largo".into());
    doc.bindings.insert(format!("{}.kind.extent.distance", ext.0), "espesor".into());
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_relative_eq!(volume(&ev), 50.0 * 10.0 * 5.0, epsilon = 1e-6);
    // El sketch resuelto lleva la cota calculada
    let solved = &ev.sketches[&sk].sketch;
    assert_eq!(solved.constraints[largo].value(), Some(50.0));
}

#[test]
fn integer_fields_are_rounded() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let pin = doc.add(caja());
    let pat = doc.add(FeatureKind::Pattern {
        features: vec![pin],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 2, spacing: 5.0 },
    });
    doc.parameters = vec![param("n", "3.6")];
    doc.bindings.insert(format!("{}.kind.pattern.count", pat.0), "n".into());
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_relative_eq!(volume(&ev), 4.0 * 10.0, epsilon = 1e-6);
}

#[test]
fn errors_are_reported_and_the_rest_still_works() {
    let mut doc = Document::new();
    doc.parameters = vec![
        param("a", "b + 1"),
        param("b", "a + 1"),
        param("c", "falta * 2"),
        param("2x", "1"),
        param("d", "(1 +"),
        param("e_ok", "3"),
        param("e_ok", "4"),
    ];
    let (values, report) = doc.parameter_values();
    assert!(report[0].error.as_ref().unwrap().contains("circular"));
    assert!(report[2].error.as_ref().unwrap().contains("falta"));
    assert!(report[3].error.as_ref().unwrap().contains("nombre"));
    assert!(report[4].error.is_some());
    assert_eq!(values.get("e_ok"), Some(&3.0));
    assert!(report[6].error.as_ref().unwrap().contains("repetido"));
    assert_eq!(doc.eval_expr("e_ok * 2").unwrap(), 6.0);

    // Vínculo a una operación que ya no existe: error, el resto se calcula
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let b = doc.add(caja());
    doc.parameters = vec![param("ancho", "20")];
    doc.bindings.insert(format!("{}.kind.shape.dx", b.0), "ancho".into());
    doc.bindings.insert("999.kind.radius".into(), "ancho".into());
    doc.bindings.insert(format!("{}.kind.shape.nada", b.0), "ancho".into());
    let ev = doc.evaluate();
    assert_relative_eq!(volume(&ev), 20.0 * 10.0, epsilon = 1e-6);
    let errs: Vec<_> = ev.bindings.iter().filter(|b| b.error.is_some()).collect();
    assert_eq!(errs.len(), 2);
}

#[test]
fn parameters_roundtrip_json() {
    let mut doc = Document::new();
    let b = doc.add(caja());
    doc.parameters = vec![param("ancho", "40")];
    doc.bindings.insert(format!("{}.kind.shape.dx", b.0), "ancho".into());
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
    // Sin parámetros no se escriben: documentos viejos y nuevos iguales
    let plain = Document::new();
    assert!(!serde_json::to_string(&plain).unwrap().contains("parameters"));
}
