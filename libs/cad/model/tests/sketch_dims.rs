//! Cotas del sketch: punto-línea, entre paralelas, simétrica respecto de un
//! eje y largo de arco.
use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn pt(s: &Sketch, id: u32) -> P2 {
    s.point(id).unwrap()
}

/// Línea fija sobre el eje X (de (0,0) a (10,0)) y sus puntos
fn base_line(s: &mut Sketch) -> u32 {
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    s.constrain(SketchConstraint::Fixed { point: a, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Fixed { point: b, x: 10.0, y: 0.0 });
    s.add_line(a, b)
}

#[test]
fn point_to_line_distance() {
    let mut s = Sketch::new();
    let l = base_line(&mut s);
    let p = s.add_point(3.0, 2.0);
    let c = s.constrain(SketchConstraint::PointLineDistance { point: p, line: l, value: 7.5, reference: false });
    s.solve().unwrap();
    assert_relative_eq!(pt(&s, p)[1], 7.5, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[c].clone()).unwrap(), 7.5, epsilon = 1e-6);
}

#[test]
fn distance_between_parallel_lines() {
    let mut s = Sketch::new();
    let l = base_line(&mut s);
    let other = s.line([0.0, 3.0], [8.0, 3.5]);
    let Geometry::Line { start, end } = s.entity(other).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Parallel { a: l, b: other });
    s.constrain(SketchConstraint::PointLineDistance { point: start, line: l, value: 4.0, reference: false });
    s.solve().unwrap();
    // Paralela a la base: los dos extremos a 4
    assert_relative_eq!(pt(&s, start)[1].abs(), 4.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, end)[1].abs(), 4.0, epsilon = 1e-6);
}

#[test]
fn axis_diameter_is_twice_the_distance() {
    // Eje vertical en x = 0 (perfil de revolución): ⌀ 30 deja el punto a 15
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 10.0);
    s.constrain(SketchConstraint::Fixed { point: a, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Fixed { point: b, x: 0.0, y: 10.0 });
    let axis = s.add_line(a, b);
    let p = s.add_point(9.0, 5.0);
    let c = s.constrain(SketchConstraint::AxisDiameter { point: p, line: axis, value: 30.0, reference: false });
    s.solve().unwrap();
    assert_relative_eq!(pt(&s, p)[0], 15.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[c].clone()).unwrap(), 30.0, epsilon = 1e-6);
}

#[test]
fn arc_length_sets_the_sweep() {
    // Centro y comienzo fijos (radio 10): largo 5π → un cuarto de vuelta
    let mut s = Sketch::new();
    let arc = s.arc([0.0, 0.0], [10.0, 0.0], [7.0, 7.0]);
    let Geometry::Arc { center, start, end } = s.entity(arc).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: center, x: 0.0, y: 0.0 });
    s.constrain(SketchConstraint::Fixed { point: start, x: 10.0, y: 0.0 });
    let c = s.constrain(SketchConstraint::ArcLength { arc, value: 5.0 * PI, reference: false });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let e = pt(&s, end);
    assert_relative_eq!(e[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(e[1], 10.0, epsilon = 1e-6);
    assert_relative_eq!(s.measure(&s.constraints[c].clone()).unwrap(), 5.0 * PI, epsilon = 1e-6);
    // Más de media vuelta también
    s.constraints[c].set_value(15.0 * PI);
    s.solve().unwrap();
    let e = pt(&s, end);
    assert_relative_eq!(e[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(e[1], -10.0, epsilon = 1e-6);
}

#[test]
fn new_dimensions_as_reference_only_measure() {
    let mut s = Sketch::new();
    let l = base_line(&mut s);
    let p = s.add_point(3.0, 2.0);
    s.constrain(SketchConstraint::Fixed { point: p, x: 3.0, y: 2.0 });
    let c = s.constrain(SketchConstraint::PointLineDistance { point: p, line: l, value: 99.0, reference: true });
    let r = s.solve().unwrap();
    assert_ne!(r.status, SketchStatus::OverConstrained);
    assert_relative_eq!(pt(&s, p)[1], 2.0, epsilon = 1e-9);
    assert_relative_eq!(s.constraints[c].value().unwrap(), 2.0, epsilon = 1e-6);
}

#[test]
fn json_names() {
    let c = SketchConstraint::AxisDiameter { point: 1, line: 2, value: 3.0, reference: false };
    let j = serde_json::to_string(&c).unwrap();
    assert!(j.contains("\"type\":\"axis_diameter\""), "{j}");
    let back: SketchConstraint = serde_json::from_str(r#"{"type":"arc_length","arc":4,"value":2.5}"#).unwrap();
    assert_eq!(back, SketchConstraint::ArcLength { arc: 4, value: 2.5, reference: false });
}
