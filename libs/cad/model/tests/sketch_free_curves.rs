//! Restricciones con splines y elipses: mismo largo, misma elipse, largo
//! total de una cadena con curvas libres y tangencias con elipses.
use approx::assert_relative_eq;
use cad_model::*;

fn fix(s: &mut Sketch, p: u32) {
    let [x, y] = s.point(p).unwrap();
    s.constrain(SketchConstraint::Fixed { point: p, x, y });
}

fn length(s: &Sketch, id: u32) -> f64 {
    s.measure(&SketchConstraint::CurveLength { entities: vec![id], value: 0.0, reference: true, opts: Default::default() }).unwrap()
}

/// Spline abierta por tres puntos, con las puntas fijas y el del medio libre
fn spline(s: &mut Sketch) -> u32 {
    let ids = vec![s.add_point(0.0, 0.0), s.add_point(10.0, 4.0), s.add_point(20.0, 0.0)];
    fix(s, ids[0]);
    fix(s, ids[2]);
    s.add_entity(Geometry::Spline { points: ids, closed: false, start_handle: None, end_handle: None, handles: vec![] })
}

#[test]
fn una_linea_del_mismo_largo_que_una_spline() {
    let mut s = Sketch::new();
    let sp = spline(&mut s);
    let a = s.add_point(0.0, 20.0);
    fix(&mut s, a);
    let b = s.add_point(15.0, 20.0);
    let l = s.add_entity(Geometry::Line { start: a, end: b });
    s.constrain(SketchConstraint::Horizontal { line: l });
    // El punto del medio de la spline, a la altura que quedó
    s.constrain(SketchConstraint::Equal { a: sp, b: l });
    s.solve().unwrap();
    assert_relative_eq!(length(&s, l), length(&s, sp), epsilon = 1e-3);
}

#[test]
fn dos_elipses_iguales() {
    let mut s = Sketch::new();
    let ellipse = |s: &mut Sketch, x: f64, a: f64, b: f64| {
        let c = s.add_point(x, 0.0);
        let ma = s.add_point(x + a, 0.0);
        let mi = s.add_point(x, b);
        s.add_entity(Geometry::Ellipse { center: c, major: ma, minor: mi })
    };
    let e1 = ellipse(&mut s, 0.0, 10.0, 4.0);
    let e2 = ellipse(&mut s, 40.0, 13.0, 6.0);
    for p in [0, 1, 2] {
        let id = s.points[p].id;
        fix(&mut s, id);
    }
    s.constrain(SketchConstraint::Equal { a: e1, b: e2 });
    s.solve().unwrap();
    let Geometry::Ellipse { center, major, minor } = s.entity(e2).unwrap().geometry else { unreachable!() };
    let d = |p: u32, q: u32| {
        let ([a, b], [c, e]) = (s.point(p).unwrap(), s.point(q).unwrap());
        (c - a).hypot(e - b)
    };
    assert_relative_eq!(d(center, major), 10.0, epsilon = 1e-6);
    assert_relative_eq!(d(center, minor), 4.0, epsilon = 1e-6);
}

#[test]
fn largo_total_de_una_cadena_con_spline() {
    let mut s = Sketch::new();
    let sp = spline(&mut s);
    // Línea que sigue a la spline desde su punta
    let Geometry::Spline { ref points, .. } = s.entity(sp).unwrap().geometry else { unreachable!() };
    let tip = points[2];
    let q = s.add_point(30.0, 0.0);
    let l = s.add_entity(Geometry::Line { start: tip, end: q });
    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::CurveLength { entities: vec![sp, l], value: 40.0, reference: false, opts: Default::default() });
    s.solve().unwrap();
    assert_relative_eq!(length(&s, sp) + length(&s, l), 40.0, epsilon = 1e-3);
}

/// Distancia del centro (0, 0) de la elipse de semiejes `a` (X) y `b` (Y) a la recta, y su
/// semiancho en la normal: iguales si es tangente
fn tangency_gap(s: &Sketch, l: u32, a: f64, b: f64) -> f64 {
    let Geometry::Line { start, end } = s.entity(l).unwrap().geometry else { unreachable!() };
    let ([x1, y1], [x2, y2]) = (s.point(start).unwrap(), s.point(end).unwrap());
    let (dx, dy) = (x2 - x1, y2 - y1);
    let n = [-dy / dx.hypot(dy), dx / dx.hypot(dy)];
    let dist = (-x1 * n[0] - y1 * n[1]).abs();
    let reach = ((a * n[0]).powi(2) + (b * n[1]).powi(2)).sqrt();
    dist - reach
}

#[test]
fn recta_tangente_a_una_elipse() {
    let mut s = Sketch::new();
    let (c, ma, mi) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(0.0, 5.0));
    for p in [c, ma, mi] {
        fix(&mut s, p);
    }
    let e = s.add_entity(Geometry::Ellipse { center: c, major: ma, minor: mi });
    let p = s.add_point(-20.0, 9.0);
    fix(&mut s, p);
    let q = s.add_point(20.0, 6.5);
    let l = s.add_entity(Geometry::Line { start: p, end: q });
    s.constrain(SketchConstraint::Tangent { a: l, b: e });
    s.solve().unwrap();
    assert_relative_eq!(tangency_gap(&s, l, 10.0, 5.0), 0.0, epsilon = 1e-6);
}

#[test]
fn spline_y_arco_tangentes_a_un_arco_de_elipse_en_la_union() {
    let mut s = Sketch::new();
    let (c, ma, mi) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(0.0, 5.0));
    // Arco de elipse de (10, 0) a (0, 5): el primer cuarto
    let st = ma;
    let en = s.add_point(0.0, 5.0);
    for p in [c, ma, mi] {
        fix(&mut s, p);
    }
    s.constrain(SketchConstraint::Coincident { a: en, b: mi });
    let ea = s.add_entity(Geometry::EllipseArc { center: c, major: ma, minor: mi, start: st, end: en });
    // Spline que sale de (10, 0) torcida: tangente queda vertical
    let p1 = s.add_point(12.0, -5.0);
    let p2 = s.add_point(15.0, -10.0);
    fix(&mut s, p2);
    let sp = s.add_entity(Geometry::Spline { points: vec![st, p1, p2], closed: false, start_handle: None, end_handle: None, handles: vec![] });
    s.constrain(SketchConstraint::Tangent { a: sp, b: ea });
    // Arco que sale de (0, 5) con centro libre: el radio queda en la normal de la elipse (vertical)
    let ac = s.add_point(3.0, 9.0);
    let ae = s.add_point(-6.0, 9.0);
    fix(&mut s, ae);
    let arc = s.add_entity(Geometry::Arc { center: ac, start: ae, end: en });
    s.constrain(SketchConstraint::Tangent { a: arc, b: ea });
    s.solve().unwrap();
    // En (10, 0) la tangente de la elipse es vertical: el punto vecino de la spline, en x = 10
    assert_relative_eq!(s.point(p1).unwrap()[0], 10.0, epsilon = 1e-4);
    // En (0, 5) la normal es vertical: el centro del arco en x = 0
    assert_relative_eq!(s.point(ac).unwrap()[0], 0.0, epsilon = 1e-4);
}
