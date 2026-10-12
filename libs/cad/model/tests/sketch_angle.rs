//! Ángulos del sketch: la referencia queda quieta y gira la otra, en el
//! sentido del signo; también con arcos, elipses y extremos de curvas.
use approx::assert_relative_eq;
use cad_model::*;

fn pt(s: &Sketch, id: u32) -> P2 {
    s.point(id).unwrap()
}

fn dir_deg(s: &Sketch, from: u32, to: u32) -> f64 {
    let (a, b) = (pt(s, from), pt(s, to));
    (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees()
}

fn angle(a: u32, b: u32, degrees: f64) -> SketchConstraint {
    SketchConstraint::Angle { a, b, degrees, supplementary: false, a_end: None, b_end: None, reference: false, opts: Default::default() }
}

/// Dos líneas libres con un vértice común: solo gira la segunda.
#[test]
fn reference_stays_and_the_other_turns() {
    let mut s = Sketch::new();
    let v = s.add_point(2.0, 1.0);
    let (p, q) = (s.add_point(12.0, 3.0), s.add_point(5.0, 8.0));
    let (a, b) = (s.add_line(v, p), s.add_line(v, q));
    let c = s.constrain(angle(a, b, 0.0));
    let now = s.measure(&s.constraints[c]).unwrap();
    s.constraints[c].set_value(now);
    s.solve().unwrap();
    let ref_dir = dir_deg(&s, v, p);
    for target in [30.0, 120.0, -45.0, 170.0, -170.0] {
        s.constraints[c].set_value(target);
        s.solve().unwrap();
        assert_relative_eq!(s.measure(&s.constraints[c]).unwrap(), target, epsilon = 1e-6);
        assert_relative_eq!(dir_deg(&s, v, p), ref_dir, epsilon = 1e-6);
        assert_relative_eq!(pt(&s, v)[0], 2.0, epsilon = 1e-6);
    }
}

/// Cambiar el signo deja la segunda del otro lado de la referencia.
#[test]
fn sign_flips_to_the_other_side() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x);
    s.constrain(SketchConstraint::Horizontal { line: base });
    let tip = s.add_point(5.0, 5.0);
    let arm = s.add_line(o, tip);
    let c = s.constrain(angle(base, arm, 45.0));
    s.solve().unwrap();
    assert!(pt(&s, tip)[1] > 0.0);
    s.constraints[c].set_value(-45.0);
    s.solve().unwrap();
    let t = pt(&s, tip);
    assert!(t[1] < 0.0, "{t:?}");
    assert_relative_eq!(t[1].atan2(t[0]).to_degrees(), -45.0, epsilon = 1e-6);
}

/// Si la que debía girar está sujeta (horizontal), gira la referencia.
#[test]
fn held_second_turns_the_first() {
    let mut s = Sketch::new();
    let v = s.add_point(0.0, 0.0);
    let (p, q) = (s.add_point(4.0, 6.0), s.add_point(10.0, 0.0));
    let (a, b) = (s.add_line(v, p), s.add_line(v, q));
    s.constrain(SketchConstraint::Horizontal { line: b });
    let c = s.constrain(angle(a, b, -30.0));
    s.solve().unwrap();
    assert_relative_eq!(s.measure(&s.constraints[c]).unwrap(), -30.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, q)[1], 0.0, epsilon = 1e-6);
    assert_relative_eq!(dir_deg(&s, v, p), 30.0, epsilon = 1e-6);
}

/// Línea y arco que se tocan: el ángulo es con la tangente del arco en ese extremo.
#[test]
fn line_and_arc_tangent() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let x = s.add_point(-10.0, 0.0);
    let line = s.add_line(o, x);
    s.constrain(SketchConstraint::Horizontal { line });
    // Arco antihorario desde el origen (centro arriba): sale hacia +x, tangente a la línea
    let arc = s.arc([0.0, 5.0], [0.0, 0.0], [5.0, 5.0]);
    let Geometry::Arc { start, center, .. } = s.entity(arc).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Coincident { a: start, b: o });
    let mut c = angle(line, arc, 0.0);
    if let SketchConstraint::Angle { b_end, .. } = &mut c {
        *b_end = Some(AngleEnd::Start);
    }
    let c = s.constrain(c);
    // Tangentes: 180° entre la línea (hacia −x) y el arco (hacia +x)
    let now = s.measure(&s.constraints[c]).unwrap();
    assert_relative_eq!(now.abs(), 180.0, epsilon = 1e-6);
    s.constraints[c].set_value(150.0);
    s.solve().unwrap();
    assert_relative_eq!(s.measure(&s.constraints[c]).unwrap(), 150.0, epsilon = 1e-6);
    // La tangente del arco en el origen (el radio girado 90°) quedó a −30°
    let r = dir_deg(&s, center, o);
    assert_relative_eq!((r + 90.0 + 30.0 + 180.0).rem_euclid(360.0) - 180.0, 0.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, x)[1], 0.0, epsilon = 1e-6);
}

/// Elipse: el ángulo es el de su eje mayor.
#[test]
fn ellipse_major_axis() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x);
    s.constrain(SketchConstraint::Horizontal { line: base });
    let (c0, m, n) = (s.add_point(3.0, 3.0), s.add_point(8.0, 4.0), s.add_point(2.6, 5.0));
    let e = s.add_entity(Geometry::Ellipse { center: c0, major: m, minor: n });
    let c = s.constrain(angle(base, e, 60.0));
    s.solve().unwrap();
    assert_relative_eq!(dir_deg(&s, c0, m), 60.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[c]).unwrap(), 60.0, epsilon = 1e-6);
}

/// Un círculo no tiene dirección: el ángulo no se acepta.
#[test]
fn circle_has_no_direction() {
    let mut s = Sketch::new();
    let l = s.line([0.0, 0.0], [5.0, 0.0]);
    let c0 = s.add_point(3.0, 3.0);
    let circle = s.add_entity(Geometry::Circle { center: c0, radius: 2.0 });
    let c = s.constrain(angle(l, circle, 30.0));
    assert!(s.measure(&s.constraints[c]).is_none());
}
