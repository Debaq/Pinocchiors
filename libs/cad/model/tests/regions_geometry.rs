//! Regiones con geometría difícil (plan urgente del sketch, U5): lazos sin
//! área, puentes a islas, curvas que se cruzan a sí mismas, cruces sobre la
//! curva y tolerancias que no dependen de lo lejano.
use cad_model::*;

fn spline(s: &mut Sketch, pts: &[[f64; 2]], closed: bool) -> u32 {
    let points = pts.iter().map(|p| s.add_point(p[0], p[1])).collect();
    s.add_entity(Geometry::Spline { points, closed, start_handle: None, end_handle: None, handles: vec![] })
}

/// Un círculo de radio casi cero y una elipse aplastada no son regiones (en
/// «Todas» rompían la extrusión)
#[test]
fn loops_without_area_are_not_regions() {
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    s.circle([30.0, 30.0], 1e-9);
    let c = s.add_point(50.0, 0.0);
    let a = s.add_point(55.0, 0.0);
    let b = s.add_point(50.0, 0.0);
    s.add_entity(Geometry::Ellipse { center: c, major: a, minor: b });
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1, "{:?}", regions.iter().map(|r| r.area()).collect::<Vec<_>>());
}

/// Una línea del contorno a una isla no la deja como región suelta: el
/// contorno sigue con su agujero y la isla adentro, como sin la línea
#[test]
fn a_bridge_to_an_island_keeps_the_hole() {
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [20.0, 20.0]);
    s.rectangle([5.0, 5.0], [15.0, 15.0]);
    s.line([0.0, 10.0], [5.0, 10.0]);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 2);
    let ring = regions.iter().find(|r| r.depth == 0).expect("el anillo");
    assert_eq!(ring.holes.len(), 1);
    assert!((ring.area() - 300.0).abs() < 1e-6, "{}", ring.area());
    let island = regions.iter().find(|r| r.depth == 1).expect("la isla adentro");
    assert!((island.area() - 100.0).abs() < 1e-6);
}

/// Una spline cerrada en ocho son dos regiones (antes, un polígono cruzado)
#[test]
fn a_figure_eight_spline_makes_two_regions() {
    let mut s = Sketch::new();
    spline(&mut s, &[[10.0, 5.0], [10.0, -5.0], [-10.0, 5.0], [-10.0, -5.0]], true);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 2, "{:?}", regions.iter().map(|r| r.area()).collect::<Vec<_>>());
    let (a, b) = (regions[0].area(), regions[1].area());
    assert!(a > 10.0 && b > 10.0 && (a - b).abs() < 0.05 * a.max(b), "{a} {b}");
}

/// Abierta con un rulo (una «α»): el rulo es región, las colas no
#[test]
fn an_open_spline_with_a_loop_makes_the_loop() {
    let mut s = Sketch::new();
    spline(&mut s, &[[-10.0, -10.0], [5.0, 5.0], [10.0, 0.0], [5.0, -5.0], [-10.0, 10.0]], false);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    assert!(regions[0].area() > 10.0);
}

/// El cruce de una recta con una elipse queda sobre la elipse, no sobre la
/// cuerda del muestreo
#[test]
fn crossing_an_ellipse_lands_on_the_ellipse() {
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(10.0, 0.0);
    let b = s.add_point(0.0, 5.0);
    s.add_entity(Geometry::Ellipse { center: c, major: a, minor: b });
    let line = s.line([3.0, -10.0], [3.0, 10.0]);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 2);
    let spans: Vec<[[f64; 2]; 2]> = regions.iter().flat_map(|r| r.outer.pieces.iter()).filter(|p| p.entity == line).filter_map(|p| p.span).collect();
    assert!(!spans.is_empty());
    for [p, q] in spans {
        for x in [p, q] {
            let e = (x[0] / 10.0).powi(2) + (x[1] / 5.0).powi(2);
            assert!((e - 1.0).abs() < 1e-9, "{x:?} fuera de la elipse ({e})");
        }
    }
}

/// Lo mismo con una B-spline (cuadrática de tres polos: una parábola,
/// y = 32·t·(1 − t) con x = 20·t): el cruce sobre la curva
#[test]
fn crossing_a_bspline_lands_on_the_curve() {
    let mut s = Sketch::new();
    let poles = [[0.0, 0.0], [10.0, 16.0], [20.0, 0.0]].iter().map(|p| s.add_point(p[0], p[1])).collect();
    s.add_entity(Geometry::BSpline { poles, degree: 2, closed: false, weights: vec![], knots: vec![] });
    s.line([0.0, 0.0], [20.0, 0.0]);
    let cut = s.line([7.0, -5.0], [7.0, 20.0]);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 2);
    let top = regions
        .iter()
        .flat_map(|r| r.outer.pieces.iter())
        .filter(|p| p.entity == cut)
        .filter_map(|p| p.span)
        .flat_map(|[a, b]| [a, b])
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .unwrap();
    assert!((top[0] - 7.0).abs() < 1e-9 && (top[1] - 7.28).abs() < 1e-9, "{top:?}");
}

/// Un arco de vuelta entera con inicio y fin distintos (en el mismo lugar)
/// es una región; antes desaparecía
#[test]
fn a_full_turn_arc_is_a_region() {
    let mut s = Sketch::new();
    s.arc([0.0, 0.0], [5.0, 0.0], [5.0, 0.0]);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    assert!((regions[0].area() - std::f64::consts::PI * 25.0).abs() < 0.2, "{}", regions[0].area());
}

/// Una construcción lejísimos no agranda la tolerancia y no fusiona lo chico
#[test]
fn far_construction_does_not_merge_small_shapes() {
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [0.05, 0.05]);
    let far = s.line([0.0, 0.0], [1e6, 0.0]);
    s.entities.iter_mut().find(|e| e.id == far).unwrap().construction = true;
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    assert!((regions[0].area() - 0.0025).abs() < 1e-9);
}
