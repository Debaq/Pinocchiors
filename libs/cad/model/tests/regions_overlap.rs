//! Regiones con líneas encimadas: un lado dibujado sobre otra línea (en
//! parte o entero) no rompe las caras.
use approx::assert_relative_eq;
use cad_model::*;

fn areas(s: &Sketch) -> Vec<f64> {
    let mut a: Vec<f64> = find_regions(s).unwrap().iter().map(|r| r.outer.area).collect();
    a.sort_by(f64::total_cmp);
    a
}

/// Reportado por el usuario (copiado con «Copiar código»): una horizontal de
/// 20 y otra a 45° desde el origen, y un cuadrado cuyo lado de abajo va sobre
/// la horizontal. Antes el cuadrado no salía como región.
#[test]
fn square_on_top_of_a_line() {
    let code = r#"{"points":[{"id":0,"x":0,"y":0},{"id":1,"x":20,"y":-5.831840364410353e-19},{"id":3,"x":14.14213562373082,"y":14.142135623731082},{"id":5,"x":20,"y":14.142135623731082},{"id":6,"x":14.14213562373082,"y":-5.831840364410353e-19}],"entities":[{"id":2,"construction":false,"geometry":{"type":"line","start":0,"end":1}},{"id":4,"construction":false,"geometry":{"type":"line","start":0,"end":3}},{"id":7,"construction":false,"geometry":{"type":"line","start":3,"end":5}},{"id":8,"construction":false,"geometry":{"type":"line","start":5,"end":1}},{"id":9,"construction":false,"geometry":{"type":"line","start":1,"end":6}},{"id":10,"construction":false,"geometry":{"type":"line","start":6,"end":3}}],"constraints":[{"type":"horizontal","line":2},{"type":"length","line":2,"value":20},{"type":"length","line":4,"value":20},{"type":"angle","a":2,"b":4,"degrees":45},{"type":"horizontal","line":7},{"type":"horizontal","line":9},{"type":"vertical","line":8},{"type":"vertical","line":10}],"next_id":11,"origin":0}"#;
    let s: Sketch = serde_json::from_str(code).unwrap();
    let h = 20.0 * std::f64::consts::FRAC_1_SQRT_2; // 14.142…
    let side = 20.0 - h;
    let a = areas(&s);
    assert_eq!(a.len(), 2, "{a:?}");
    // El cuadrado (6, 1, 5, 3) y el triángulo (0, 6, 3), de menor a mayor
    assert_relative_eq!(a[0], side * h, epsilon = 1e-6);
    assert_relative_eq!(a[1], h * h / 2.0, epsilon = 1e-6);
}

/// Dos líneas idénticas en un triángulo: una sola región.
#[test]
fn duplicated_side() {
    let mut s = Sketch::new();
    let (a, b, c) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(0.0, 10.0));
    s.add_line(a, b);
    s.add_line(b, c);
    s.add_line(c, a);
    s.add_line(b, a);
    let r = areas(&s);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_relative_eq!(r[0], 50.0, epsilon = 1e-9);
}

/// Un rectángulo con su lado de abajo más largo que él (sobresale a los dos lados).
#[test]
fn longer_line_under_a_rectangle() {
    let mut s = Sketch::new();
    s.line([-5.0, 0.0], [15.0, 0.0]);
    let (p, q, r, t) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(10.0, 4.0), s.add_point(0.0, 4.0));
    s.add_line(p, q);
    s.add_line(q, r);
    s.add_line(r, t);
    s.add_line(t, p);
    let a = areas(&s);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_relative_eq!(a[0], 40.0, epsilon = 1e-9);
}
