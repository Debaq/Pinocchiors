//! Sketches con datos malos o grandes: nada se cae y lo sano se resuelve
//! igual (plan urgente del sketch, U1).
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
