//! Cotas II y restricciones II del sketch: ángulo suplementario, largo total,
//! distancia mínima y máxima con círculos, opciones de cota, coradial,
//! simetría de entidades, punto sobre elipse y spline, intersección y bloqueo.
use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn pt(s: &Sketch, id: u32) -> P2 {
    s.point(id).unwrap()
}

fn fix(s: &mut Sketch, p: u32) {
    let [x, y] = pt(s, p);
    s.constrain(SketchConstraint::Fixed { point: p, x, y });
}

fn ends(s: &Sketch, line: u32) -> (u32, u32) {
    let Geometry::Line { start, end } = s.entity(line).unwrap().geometry else { unreachable!() };
    (start, end)
}

fn center(s: &Sketch, id: u32) -> u32 {
    match s.entity(id).unwrap().geometry {
        Geometry::Circle { center, .. } | Geometry::Arc { center, .. } | Geometry::Ellipse { center, .. } => center,
        _ => unreachable!(),
    }
}

fn no_opts() -> DimOpts {
    DimOpts::default()
}

#[test]
fn supplementary_angle() {
    let mut s = Sketch::new();
    let base = s.line([0.0, 0.0], [10.0, 0.0]);
    let (a, b) = ends(&s, base);
    fix(&mut s, a);
    fix(&mut s, b);
    // Sale del origen hacia arriba a la derecha
    let other = s.add_point(5.0, 5.0);
    let l = s.add_line(a, other);
    s.constrain(SketchConstraint::Length { line: l, value: 10.0, reference: false, opts: no_opts() });
    // Suplementario de −120° (con signo, como el ángulo) = 60° entre las dos
    let c = s.constrain(SketchConstraint::Angle { a: base, b: l, degrees: -120.0, supplementary: true, reference: false, opts: no_opts() });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let p = pt(&s, other);
    assert_relative_eq!(p[1].atan2(p[0]).to_degrees(), 60.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[c].clone()).unwrap(), -120.0, epsilon = 1e-6);
}

#[test]
fn total_curve_length() {
    // Línea fija de 10 y una vertical que sigue: largo total 25 → la vertical mide 15
    let mut s = Sketch::new();
    let l1 = s.line([0.0, 0.0], [10.0, 0.0]);
    let (a, b) = ends(&s, l1);
    fix(&mut s, a);
    fix(&mut s, b);
    let top = s.add_point(10.0, 4.0);
    let l2 = s.add_line(b, top);
    s.constrain(SketchConstraint::Vertical { line: l2 });
    // Con un arco de radio fijo 5 y media vuelta en la cadena (5π)
    let arc = s.arc([20.0, 20.0], [25.0, 20.0], [15.0, 20.0]);
    let Geometry::Arc { center: c, start: st, end: en } = s.entity(arc).unwrap().geometry else { unreachable!() };
    for p in [c, st, en] {
        fix(&mut s, p);
    }
    let k = s.constrain(SketchConstraint::CurveLength { entities: vec![l1, l2, arc], value: 25.0 + 5.0 * PI, reference: false, opts: no_opts() });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(pt(&s, top)[1], 15.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[k].clone()).unwrap(), 25.0 + 5.0 * PI, epsilon = 1e-6);
    // Un círculo cuenta su circunferencia
    let mut s = Sketch::new();
    let circ = s.circle([0.0, 0.0], 2.0);
    {
        let k = center(&s, circ);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::CurveLength { entities: vec![circ], value: 10.0 * PI, reference: false, opts: no_opts() });
    s.solve().unwrap();
    assert_relative_eq!(s.radius(circ).unwrap(), 5.0, epsilon = 1e-6);
}

/// Dos círculos con centros fijos en (0,0) y (20,0); el primero de radio 3
fn two_circles(r2: f64) -> (Sketch, u32, u32) {
    let mut s = Sketch::new();
    let c1 = s.circle([0.0, 0.0], 3.0);
    let c2 = s.circle([20.0, 0.0], r2);
    {
        let k = center(&s, c1);
        fix(&mut s, k);
    }
    {
        let k = center(&s, c2);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: c1, value: 3.0, reference: false, opts: no_opts() });
    (s, c1, c2)
}

#[test]
fn min_and_max_distance_between_circles() {
    let (mut s, c1, c2) = two_circles(2.0);
    let k = s.constrain(SketchConstraint::CircleDistance { a: c1, b: c2, max: false, value: 10.0, reference: false, opts: no_opts() });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(s.radius(c2).unwrap(), 7.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[k].clone()).unwrap(), 10.0, epsilon = 1e-6);

    let (mut s, c1, c2) = two_circles(2.0);
    s.constrain(SketchConstraint::CircleDistance { a: c1, b: c2, max: true, value: 30.0, reference: false, opts: no_opts() });
    s.solve().unwrap();
    assert_relative_eq!(s.radius(c2).unwrap(), 7.0, epsilon = 1e-6);
}

#[test]
fn min_distance_inside_a_circle() {
    // Círculo chico adentro del grande (radio 10, centros a 2): mínima 5 → radio 3
    let mut s = Sketch::new();
    let big = s.circle([0.0, 0.0], 10.0);
    let small = s.circle([2.0, 0.0], 1.0);
    {
        let k = center(&s, big);
        fix(&mut s, k);
    }
    {
        let k = center(&s, small);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: big, value: 10.0, reference: false, opts: no_opts() });
    s.constrain(SketchConstraint::CircleDistance { a: big, b: small, max: false, value: 5.0, reference: false, opts: no_opts() });
    s.solve().unwrap();
    assert_relative_eq!(s.radius(small).unwrap(), 3.0, epsilon = 1e-6);

    // Punto y línea contra un círculo de radio 3 en el origen
    let mut s = Sketch::new();
    let c = s.circle([0.0, 0.0], 3.0);
    {
        let k = center(&s, c);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: c, value: 3.0, reference: false, opts: no_opts() });
    let p = s.add_point(8.0, 0.0);
    s.constrain(SketchConstraint::HorizontalPoints { a: center(&s, c), b: p });
    s.constrain(SketchConstraint::CircleDistance { a: p, b: c, max: false, value: 4.0, reference: false, opts: no_opts() });
    let l = s.line([-5.0, -6.0], [5.0, -6.0]);
    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::HorizontalDistance { a: ends(&s, l).0, b: ends(&s, l).1, value: 10.0, reference: false, opts: no_opts() });
    {
        let k = ends(&s, l).0;
        fix(&mut s, k);
    }
    // Máxima entre la recta (y = −6) y el círculo: distancia al centro + radio
    let k = s.constrain(SketchConstraint::CircleDistance { a: l, b: c, max: true, value: 0.0, reference: true, opts: no_opts() });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(pt(&s, p)[0], 7.0, epsilon = 1e-6);
    assert_relative_eq!(s.constraints[k].value().unwrap(), 9.0, epsilon = 1e-6);
}

#[test]
fn dimension_options_round_trip() {
    let c: SketchConstraint = serde_json::from_str(r#"{"type":"distance","a":1,"b":2,"value":3,"opts":{"locked":true,"offset":[1.5,-2]}}"#).unwrap();
    let SketchConstraint::Distance { opts, .. } = &c else { unreachable!() };
    assert!(opts.locked);
    assert_eq!(opts.offset, Some([1.5, -2.0]));
    let j = serde_json::to_string(&c).unwrap();
    assert!(j.contains("\"locked\":true"), "{j}");
    // Sin opciones no se escribe nada
    let plain = SketchConstraint::Length { line: 1, value: 2.0, reference: false, opts: no_opts() };
    assert!(!serde_json::to_string(&plain).unwrap().contains("opts"));
    let a: SketchConstraint = serde_json::from_str(r#"{"type":"angle","a":1,"b":2,"degrees":30,"supplementary":true}"#).unwrap();
    assert!(matches!(a, SketchConstraint::Angle { supplementary: true, .. }));
}

#[test]
fn coradial_circles() {
    let mut s = Sketch::new();
    let a = s.circle([0.0, 0.0], 4.0);
    let b = s.circle([3.0, 1.0], 1.0);
    {
        let k = center(&s, a);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: a, value: 4.0, reference: false, opts: no_opts() });
    s.constrain(SketchConstraint::Coradial { a, b });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(s.radius(b).unwrap(), 4.0, epsilon = 1e-6);
    let cb = pt(&s, center(&s, b));
    assert_relative_eq!(cb[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(cb[1], 0.0, epsilon = 1e-6);
}

/// Eje vertical fijo en x = 0
fn axis(s: &mut Sketch) -> u32 {
    let l = s.line([0.0, -10.0], [0.0, 10.0]);
    let (a, b) = ends(s, l);
    fix(s, a);
    fix(s, b);
    l
}

#[test]
fn symmetric_lines_arcs_and_circles() {
    let mut s = Sketch::new();
    let ax = axis(&mut s);
    // Línea a la izquierda fija; la de la derecha empieza "dada vuelta"
    let left = s.line([-5.0, 0.0], [-2.0, 4.0]);
    let (l1, l2) = ends(&s, left);
    fix(&mut s, l1);
    fix(&mut s, l2);
    let right = s.line([2.5, 4.5], [4.0, -1.0]);
    s.constrain(SketchConstraint::SymmetricEntities { a: left, b: right, line: ax });
    // Arcos: el reflejo cambia el sentido
    let arc = s.arc([-6.0, 0.0], [-3.0, 0.0], [-6.0, 3.0]);
    let Geometry::Arc { center: c, start: st, end: en } = s.entity(arc).unwrap().geometry else { unreachable!() };
    for p in [c, st, en] {
        fix(&mut s, p);
    }
    let arc2 = s.arc([5.0, 1.0], [5.5, 3.0], [3.0, 0.5]);
    s.constrain(SketchConstraint::SymmetricEntities { a: arc, b: arc2, line: ax });
    // Círculos: centro reflejado y mismo radio
    let c1 = s.circle([-8.0, 5.0], 1.5);
    {
        let k = center(&s, c1);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: c1, value: 1.5, reference: false, opts: no_opts() });
    let c2 = s.circle([7.0, 6.0], 1.0);
    s.constrain(SketchConstraint::SymmetricEntities { a: c1, b: c2, line: ax });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let (r1, r2) = ends(&s, right);
    // La de la derecha quedó con el inicio arriba (como estaba, sin cruzarse)
    assert_relative_eq!(pt(&s, r1)[0], 2.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, r1)[1], 4.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, r2)[0], 5.0, epsilon = 1e-6);
    let Geometry::Arc { center: c, start: st, end: en } = s.entity(arc2).unwrap().geometry else { unreachable!() };
    assert_relative_eq!(pt(&s, c)[0], 6.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, en)[0], 3.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, st)[0], 6.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, st)[1], 3.0, epsilon = 1e-6);
    assert_relative_eq!(s.radius(c2).unwrap(), 1.5, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, center(&s, c2))[0], 8.0, epsilon = 1e-6);
}

#[test]
fn point_on_ellipse_and_spline() {
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let ma = s.add_point(6.0, 0.0);
    let mi = s.add_point(0.0, 3.0);
    let e = s.add_entity(Geometry::Ellipse { center: c, major: ma, minor: mi });
    for p in [c, ma, mi] {
        fix(&mut s, p);
    }
    let p = s.add_point(4.0, 4.0);
    s.constrain(SketchConstraint::PointOnCurve { point: p, curve: e });
    // Spline fija por tres puntos
    let ids: Vec<u32> = [[10.0, 0.0], [15.0, 5.0], [20.0, 0.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
    for &q in &ids {
        fix(&mut s, q);
    }
    let sp = s.add_entity(Geometry::Spline { points: ids.clone(), closed: false, start_handle: None, end_handle: None, handles: vec![] });
    let q = s.add_point(13.0, 6.0);
    s.constrain(SketchConstraint::PointOnCurve { point: q, curve: sp });
    s.solve().unwrap();
    let [x, y] = pt(&s, p);
    assert_relative_eq!((x / 6.0).powi(2) + (y / 3.0).powi(2), 1.0, epsilon = 1e-6);
    let pts: Vec<_> = ids.iter().map(|&i| nalgebra::Vector2::new(pt(&s, i)[0], pt(&s, i)[1])).collect();
    let [x, y] = pt(&s, q);
    let on = cad_solver::closest_on_spline(&pts, false, &[], nalgebra::Vector2::new(x, y));
    assert!((on - nalgebra::Vector2::new(x, y)).norm() < 1e-5, "{on:?} vs {x},{y}");
}

#[test]
fn point_at_intersection() {
    // Línea horizontal y = 3 y círculo de radio 5 en el origen: (4, 3)
    let mut s = Sketch::new();
    let l = s.line([-10.0, 3.0], [10.0, 3.0]);
    let (a, b) = ends(&s, l);
    fix(&mut s, a);
    fix(&mut s, b);
    let c = s.circle([0.0, 0.0], 5.0);
    {
        let k = center(&s, c);
        fix(&mut s, k);
    }
    s.constrain(SketchConstraint::Radius { entity: c, value: 5.0, reference: false, opts: no_opts() });
    let p = s.add_point(3.5, 2.0);
    s.constrain(SketchConstraint::Intersection { point: p, a: l, b: c });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(pt(&s, p)[0], 4.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, p)[1], 3.0, epsilon = 1e-6);
}

#[test]
fn locked_entity_does_not_move() {
    let mut s = Sketch::new();
    let l = s.line([1.0, 1.0], [6.0, 3.0]);
    let c = s.circle([10.0, 0.0], 2.0);
    s.constrain(SketchConstraint::Lock { entity: l });
    s.constrain(SketchConstraint::Lock { entity: c });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let (a, _) = ends(&s, l);
    s.solve_drag(a, [4.0, 4.0]).unwrap();
    assert_relative_eq!(pt(&s, a)[0], 1.0, epsilon = 1e-9);
    assert_relative_eq!(s.radius(c).unwrap(), 2.0, epsilon = 1e-9);
    // Borrar la entidad se lleva el bloqueo
    s.remove_entity(l).unwrap();
    assert_eq!(s.constraints.len(), 1);
}
