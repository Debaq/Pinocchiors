//! Sketches con datos malos o grandes: nada se cae y lo sano se resuelve
//! igual (plan urgente del sketch, U1 y U2).
use cad_model::*;

fn ends(s: &Sketch, line: u32) -> (u32, u32) {
    let Geometry::Line { start, end } = s.entity(line).unwrap().geometry else { unreachable!() };
    (start, end)
}

/// Una restricción que nombra algo de otro tipo queda afuera y marcada; lo
/// demás se resuelve y las regiones salen igual
#[test]
fn a_broken_constraint_does_not_take_the_whole_sketch_down() {
    let mut s = Sketch::new();
    let [bottom, ..] = s.rectangle([0.0, 0.0], [10.0, 5.0]);
    let tilted = s.line([20.0, 0.0], [25.0, 1.0]);
    // Radio de una línea: no se puede armar
    let bad = s.constrain(SketchConstraint::Radius { entity: bottom, value: 3.0, reference: false, opts: Default::default() });
    let good = s.constrain(SketchConstraint::Horizontal { line: tilted });
    let r = s.solve().expect("se resuelve aunque haya una rota");
    assert!(r.invalid.iter().any(|(i, _)| *i == bad), "{:?}", r.invalid);
    assert!(r.conflicting.contains(&bad), "también en conflicto, para quitarla");
    assert!(!r.invalid.iter().any(|(i, _)| *i == good));
    assert_eq!(r.status, SketchStatus::OverConstrained);
    let (a, b) = ends(&s, tilted);
    assert!((s.point(a).unwrap()[1] - s.point(b).unwrap()[1]).abs() < 1e-6, "la horizontal se cumplió");
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1, "el rectángulo sigue siendo una región");
}

/// Cotas que no son números o tamaños que no son positivos: afuera y marcadas
#[test]
fn nan_and_non_positive_dimensions_are_marked_not_solved() {
    let mut s = Sketch::new();
    let c = s.circle([0.0, 0.0], 5.0);
    let l = s.line([10.0, 0.0], [20.0, 0.0]);
    let zero = s.constrain(SketchConstraint::Radius { entity: c, value: 0.0, reference: false, opts: Default::default() });
    let nan = s.constrain(SketchConstraint::Length { line: l, value: f64::NAN, reference: false, opts: Default::default() });
    let r = s.solve().unwrap();
    let marked: Vec<usize> = r.invalid.iter().map(|(i, _)| *i).collect();
    assert!(marked.contains(&zero) && marked.contains(&nan), "{:?}", r.invalid);
    let Geometry::Circle { radius, .. } = s.entity(c).unwrap().geometry else { unreachable!() };
    assert!((radius - 5.0).abs() < 1e-9, "el círculo no colapsó");
    assert!(r.residual.is_finite());
    // El reporte va y vuelve por JSON (el infinito salía `null`)
    let back: SolveReport = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back.invalid.len(), 2);
}

/// Tangencia con una B-spline de un solo polo: antes un índice fuera de rango
#[test]
fn tangent_with_a_one_pole_bspline_does_not_panic() {
    let mut s = Sketch::new();
    let p = s.add_point(0.0, 0.0);
    let q = s.add_point(5.0, 0.0);
    let line = s.add_line(p, q);
    let spline = s.add_entity(Geometry::BSpline { poles: vec![q], degree: 3, closed: false, weights: vec![], knots: vec![] });
    let t = s.constrain(SketchConstraint::Tangent { a: line, b: spline });
    let r = s.solve().unwrap();
    assert!(r.invalid.iter().any(|(i, _)| *i == t), "{:?}", r.invalid);
    find_regions(&s).unwrap();
}

/// Arrastrar sin diagnóstico (rápido); resolver normal trae lo libre
#[test]
fn a_drag_that_converges_skips_the_diagnosis() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let b = s.add_point(10.0, 0.0);
    let line = s.add_line(o, b);
    s.constrain(SketchConstraint::Horizontal { line });
    let full = s.solve().unwrap();
    assert!(!full.quick && full.free_points.contains(&b));
    let dragged = s.solve_drag(b, [14.0, 0.0]).unwrap();
    assert!(dragged.quick, "cumplido: sin diagnóstico");
    assert!(dragged.free_points.is_empty());
    assert!((s.point(b).unwrap()[0] - 14.0).abs() < 1e-6);
}

/// Con algo libre en otro lado (el solver dice «sin definir») el arrastre
/// también va sin diagnóstico: antes solo con el sketch entero definido
#[test]
fn a_drag_with_something_free_elsewhere_is_quick_too() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let b = s.add_point(10.0, 0.0);
    let line = s.add_line(o, b);
    s.constrain(SketchConstraint::Horizontal { line });
    s.line([0.0, 5.0], [8.0, 7.0]);
    let r = s.solve_drag(b, [14.0, 0.0]).unwrap();
    assert!(r.quick, "{:?}", r.status);
    assert_eq!(r.status, SketchStatus::UnderConstrained);
}

/// Un arrastre con conflicto sí diagnostica (para marcar lo que choca)
#[test]
fn a_drag_with_a_conflict_still_reports_it() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let b = s.add_point(10.0, 0.0);
    let line = s.add_line(o, b);
    s.constrain(SketchConstraint::Horizontal { line });
    // Horizontal y a 3 de altura del origen: no se pueden las dos
    s.constrain(SketchConstraint::VerticalDistance { a: o, b, value: 3.0, reference: false, opts: Default::default() });
    let r = s.solve_drag(b, [12.0, 3.0]).unwrap();
    assert!(!r.quick);
    assert!(!r.conflicting.is_empty());
}

/// Borrar una entidad no deja textos nombrándola ni se lleva el ancla de un
/// texto (aunque ninguna otra entidad la use)
#[test]
fn removing_an_entity_leaves_no_dangling_references() {
    let mut s = Sketch::new();
    let path = s.line([0.0, 0.0], [10.0, 0.0]);
    let (anchor, end) = ends(&s, path);
    let glyph = s.line([0.0, 5.0], [1.0, 6.0]);
    let (g0, g1) = ends(&s, glyph);
    let mut style = TextStyle::default();
    style.path = Some(path);
    s.texts.push(SketchText { id: 99, text: "A".into(), size: 1.0, font: String::new(), anchor, entities: vec![glyph], points: vec![g0, g1], style, block: None });
    s.remove_entity(path).unwrap();
    assert!(s.point(anchor).is_ok(), "el ancla del texto se queda");
    assert!(s.point(end).is_err());
    assert_eq!(s.texts[0].style.path, None);
    s.remove_entity(glyph).unwrap();
    assert!(s.texts[0].entities.is_empty());
    assert!(s.texts[0].points.is_empty(), "sin puntos que ya no existen");
    s.solve().expect("se resuelve igual");
}

/// Tangencia y curvatura con la spline interpolada (antes solo B-spline):
/// sin manija la dirección en la punta va hacia el punto vecino; con manija,
/// hacia la manija.
#[test]
fn tangent_and_curvature_with_interpolated_spline() {
    use cad_model::{Geometry, Sketch, SketchConstraint};
    let dir = |s: &Sketch, a: u32, b: u32| {
        let (p, q) = (s.point(a).unwrap(), s.point(b).unwrap());
        let l = (q[0] - p[0]).hypot(q[1] - p[1]);
        [(q[0] - p[0]) / l, (q[1] - p[1]) / l]
    };
    // Línea que llega al comienzo de la spline, torcida: queda alineada con el primer tramo
    let mut s = Sketch::new();
    let a = s.add_point(-10.0, 1.0);
    let p: Vec<u32> = [[0.0, 0.0], [10.0, 0.0], [20.0, 5.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
    let l = s.add_line(a, p[0]);
    let sp = s.add_entity(Geometry::Spline { points: p.clone(), closed: false, start_handle: None, end_handle: None, handles: vec![] });
    s.constrain(SketchConstraint::Tangent { a: l, b: sp });
    s.solve().unwrap();
    let (u, v) = (dir(&s, a, p[0]), dir(&s, p[0], p[1]));
    assert!((u[0] * v[1] - u[1] * v[0]).abs() < 1e-6, "{u:?} {v:?}");
    // Con manija en la punta: la línea sigue a la manija
    let mut s = Sketch::new();
    let a = s.add_point(-10.0, 0.0);
    let p: Vec<u32> = [[0.0, 0.0], [10.0, 0.0], [20.0, 5.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
    let h = s.add_point(3.0, 3.0);
    let l = s.add_line(a, p[0]);
    let sp = s.add_entity(Geometry::Spline { points: p.clone(), closed: false, start_handle: Some(h), end_handle: None, handles: vec![] });
    s.constrain(SketchConstraint::Tangent { a: l, b: sp });
    s.solve().unwrap();
    let (u, v) = (dir(&s, a, p[0]), dir(&s, p[0], h));
    assert!((u[0] * v[1] - u[1] * v[0]).abs() < 1e-6, "{u:?} {v:?}");
    // Curvatura igual con un arco que llega al final de la spline: se resuelve
    let mut s = Sketch::new();
    let p: Vec<u32> = [[0.0, 0.0], [10.0, 2.0], [20.0, 0.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
    let sp = s.add_entity(Geometry::Spline { points: p.clone(), closed: false, start_handle: None, end_handle: None, handles: vec![] });
    let c = s.add_point(20.0, -10.0);
    let e = s.add_point(30.0, -10.0);
    let arc = s.add_entity(Geometry::Arc { center: c, start: e, end: p[2] });
    s.constrain(SketchConstraint::Curvature { a: sp, b: arc });
    let r = s.solve().unwrap();
    assert!(r.residual < 1e-6, "{r:?}");
}

/// Un archivo de una versión más nueva: la restricción y la entidad que no se
/// conocen quedan en su lugar (no corren los índices de las cotas) y no
/// restringen ni forman perfiles.
#[test]
fn unknown_constraint_and_geometry_still_load() {
    use cad_model::{find_regions, Geometry, Sketch, SketchConstraint};
    let json = r#"{"points":[{"id":1,"x":0,"y":0},{"id":2,"x":10,"y":0},{"id":3,"x":10,"y":10},{"id":4,"x":0,"y":10}],
      "entities":[{"id":5,"geometry":{"type":"line","start":1,"end":2}},{"id":6,"geometry":{"type":"line","start":2,"end":3}},
        {"id":7,"geometry":{"type":"line","start":3,"end":4}},{"id":8,"geometry":{"type":"line","start":4,"end":1}},
        {"id":9,"geometry":{"type":"clothoid","start":1,"end":3,"k":0.5}}],
      "constraints":[{"type":"horizontal","line":5},{"type":"g3","a":5,"b":6},{"type":"length","line":5,"value":12}]}"#;
    let mut s: Sketch = serde_json::from_str(json).unwrap();
    assert_eq!(s.entities[4].geometry, Geometry::Unknown);
    assert_eq!(s.constraints[1], SketchConstraint::Unknown);
    let r = s.solve().unwrap();
    assert!(r.residual < 1e-9, "{r:?}");
    let (a, b) = (s.point(1).unwrap(), s.point(2).unwrap());
    assert!(((b[0] - a[0]).hypot(b[1] - a[1]) - 12.0).abs() < 1e-9 && (b[1] - a[1]).abs() < 1e-9, "{a:?} {b:?}");
    // Al guardar queda en su lugar
    let back = serde_json::to_value(&s).unwrap();
    assert_eq!(back["constraints"][1]["type"], "unknown");
    assert_eq!(find_regions(&s).unwrap().len(), 1);
}

/// Una cota nueva que choca con lo ya cotado se queda sin cumplir y marcada: lo anterior no se mueve
#[test]
fn a_new_conflicting_dimension_does_not_move_what_was_already_dimensioned() {
    let mut s = Sketch::new();
    let l = s.line([0.0, 0.0], [32.0, 0.0]);
    let (a, b) = ends(&s, l);
    s.constrain(SketchConstraint::Fixed { point: a, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::Length { line: l, value: 32.0, reference: false, opts: Default::default() });
    s.solve().unwrap();
    // La cota nueva (la última) pide otro largo para la misma línea
    let new = s.constrain(SketchConstraint::Length { line: l, value: 5.0, reference: false, opts: Default::default() });
    let r = s.solve().unwrap();
    assert!(r.conflicting.contains(&new), "{:?}", r.conflicting);
    assert_eq!(r.conflicting.len(), 1, "solo la nueva queda marcada: {:?}", r.conflicting);
    let end = s.point(b).unwrap();
    assert!((end[0] - 32.0).abs() < 1e-6, "la línea cotada no se movió: {end:?}");
}
