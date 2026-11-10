//! Equidistante de una spline o una elipse: la copia queda atada a la original
//! por la cota (cada punto suyo a esa distancia de la curva).
use approx::assert_relative_eq;
use cad_model::*;

/// Distancia de `p` a la elipse de centro (0, 0) y semiejes `a` (en X) y `b` (en Y), por muestras
fn to_ellipse(p: P2, a: f64, b: f64) -> f64 {
    (0..20000)
        .map(|k| {
            let t = k as f64 / 20000.0 * std::f64::consts::TAU;
            (p[0] - a * t.cos()).hypot(p[1] - b * t.sin())
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn equidistante_de_elipse_sigue_a_la_cota_y_a_la_original() {
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let ma = s.add_point(20.0, 0.0);
    let mi = s.add_point(0.0, 10.0);
    for (p, x, y) in [(c, 0.0, 0.0), (ma, 20.0, 0.0)] {
        s.constrain(SketchConstraint::Fixed { point: p, x, y });
    }
    let e = s.add_entity(Geometry::Ellipse { center: c, major: ma, minor: mi });
    let axis = s.add_entity(Geometry::Line { start: c, end: mi });
    s.constrain(SketchConstraint::Vertical { line: axis });
    let minor = s.constrain(SketchConstraint::Length { line: axis, value: 10.0, reference: false, opts: Default::default() });
    // Copia por fuera, a 3 mm (dibujada un poco corrida: el solver la acomoda)
    let ids: Vec<u32> = (0..12)
        .map(|k| {
            let t = k as f64 / 12.0 * std::f64::consts::TAU;
            s.add_point(23.5 * t.cos(), 13.2 * t.sin())
        })
        .collect();
    let copy = s.add_entity(Geometry::Spline { points: ids.clone(), closed: true, start_handle: None, end_handle: None, handles: vec![] });
    let dim = s.constrain(SketchConstraint::Offset { pairs: vec![[e, copy]], value: 3.0, reference: false, opts: Default::default() });
    s.solve().unwrap();
    for &q in &ids {
        assert_relative_eq!(to_ellipse(s.point(q).unwrap(), 20.0, 10.0), 3.0, epsilon = 1e-3);
    }
    // La cota mueve toda la copia
    s.constraints[dim].set_value(5.0);
    s.solve().unwrap();
    for &q in &ids {
        assert_relative_eq!(to_ellipse(s.point(q).unwrap(), 20.0, 10.0), 5.0, epsilon = 1e-3);
    }
    // La original cambia: la copia la sigue
    s.constraints[minor].set_value(12.0);
    s.solve().unwrap();
    for &q in &ids {
        assert_relative_eq!(to_ellipse(s.point(q).unwrap(), 20.0, 12.0), 5.0, epsilon = 1e-3);
    }
}
