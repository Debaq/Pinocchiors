//! Curvas del sketch de las fases 16 y 17: B-spline por polos (también
//! cónicas), arco de elipse, manijas en puntos intermedios, tangencia y
//! curvatura igual (G2) con B-splines.
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

fn bspline(s: &mut Sketch, pts: &[P2], degree: u32, weights: Vec<f64>) -> (u32, Vec<u32>) {
    let poles: Vec<u32> = pts.iter().map(|p| s.add_point(p[0], p[1])).collect();
    let id = s.add_entity(Geometry::BSpline { poles: poles.clone(), degree, closed: false, weights, knots: vec![] });
    (id, poles)
}

fn extrude_volume(s: Sketch) -> Option<f64> {
    if !occt::available() {
        return None;
    }
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 1.0 },
        reverse: false,
        op: BodyOp::Join,
        draft: 0.0,
        thin: None,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "errores: {:?}", ev.errors());
    Some(ev.body.as_ref().expect("sin cuerpo").mass().unwrap().volume)
}

#[test]
fn conic_quarter_circle_region_and_extrusion() {
    // Cuarto de círculo como cónica (peso √2/2) cerrado con dos radios
    let mut s = Sketch::new();
    let (_, poles) = bspline(&mut s, &[[10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 2, vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0]);
    let o = s.add_point(0.0, 0.0);
    s.add_line(poles[2], o);
    s.add_line(o, poles[0]);
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    assert_relative_eq!(regions[0].area(), PI * 25.0, max_relative = 1e-3);
    if let Some(v) = extrude_volume(s) {
        assert_relative_eq!(v, PI * 25.0, max_relative = 1e-6);
    }
}

#[test]
fn ellipse_arc_ends_stay_on_the_ellipse() {
    // Media elipse (a = 4, b = 2) arriba, cerrada con el diámetro
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let ma = s.add_point(4.0, 0.0);
    let mi = s.add_point(0.0, 2.0);
    let st = s.add_point(4.3, 0.2);
    let en = s.add_point(-3.9, -0.1);
    s.add_entity(Geometry::EllipseArc { center: c, major: ma, minor: mi, start: st, end: en });
    for p in [c, ma, mi] {
        fix(&mut s, p);
    }
    s.add_line(en, st);
    s.constrain(SketchConstraint::HorizontalPoints { a: st, b: c });
    s.constrain(SketchConstraint::HorizontalPoints { a: en, b: c });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_relative_eq!(pt(&s, st)[0], 4.0, epsilon = 1e-6);
    assert_relative_eq!(pt(&s, en)[0], -4.0, epsilon = 1e-6);
    let regions = find_regions(&s).unwrap();
    assert_relative_eq!(regions[0].area(), PI * 4.0, max_relative = 2e-3);
    if let Some(v) = extrude_volume(s) {
        assert_relative_eq!(v, PI * 4.0, max_relative = 1e-6);
    }
}

#[test]
fn closed_bspline_is_a_region() {
    let mut s = Sketch::new();
    let poles: Vec<u32> = [[5.0, 0.0], [0.0, 5.0], [-5.0, 0.0], [0.0, -5.0]].iter().map(|p| s.add_point(p[0], p[1])).collect();
    s.add_entity(Geometry::BSpline { poles, degree: 3, closed: true, weights: vec![], knots: vec![] });
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    let a = regions[0].area();
    if let Some(v) = extrude_volume(s) {
        assert_relative_eq!(v, a, max_relative = 2e-3);
    }
}

#[test]
fn point_on_bspline() {
    let mut s = Sketch::new();
    let (b, poles) = bspline(&mut s, &[[0.0, 0.0], [3.0, 6.0], [7.0, -2.0], [10.0, 3.0]], 3, vec![]);
    for p in poles.clone() {
        fix(&mut s, p);
    }
    let q = s.add_point(5.0, 5.0);
    s.constrain(SketchConstraint::PointOnCurve { point: q, curve: b });
    s.solve().unwrap();
    let pts: Vec<_> = poles.iter().map(|&i| nalgebra::Vector2::new(pt(&s, i)[0], pt(&s, i)[1])).collect();
    let c = cad_solver::BSpline::new(&pts, &[], 3, false, &[]).unwrap();
    let p = nalgebra::Vector2::new(pt(&s, q)[0], pt(&s, q)[1]);
    assert!((c.eval(c.closest_param(p)) - p).norm() < 1e-6);
}

#[test]
fn tangent_and_curvature_continuity_with_an_arc() {
    // Arco fijo de radio 10 (centro en el origen) que termina en (0, 10);
    // de ahí sale una B-spline cúbica con el último polo fijo
    let mut s = Sketch::new();
    let arc = s.arc([0.0, 0.0], [10.0, 0.0], [0.0, 10.0]);
    let Geometry::Arc { center, start, end } = s.entity(arc).unwrap().geometry else { unreachable!() };
    for p in [center, start, end] {
        fix(&mut s, p);
    }
    let p1 = s.add_point(-3.0, 11.0);
    let p2 = s.add_point(-7.0, 12.0);
    let p3 = s.add_point(-12.0, 8.0);
    fix(&mut s, p3);
    let b = s.add_entity(Geometry::BSpline { poles: vec![end, p1, p2, p3], degree: 3, closed: false, weights: vec![], knots: vec![] });
    s.constrain(SketchConstraint::Curvature { a: arc, b });
    let r = s.solve().unwrap();
    assert!(r.status != SketchStatus::OverConstrained && r.status != SketchStatus::Failed, "{r:?}");
    // Tangente: el primer polo vecino sobre la horizontal y = 10 (perpendicular al radio)
    assert_relative_eq!(pt(&s, p1)[1], 10.0, epsilon = 1e-6);
    // Curvatura igual: el arco antihorario dobla a la izquierda con 1/10
    let pts: Vec<_> = [end, p1, p2, p3].iter().map(|&i| nalgebra::Vector2::new(pt(&s, i)[0], pt(&s, i)[1])).collect();
    let c = cad_solver::BSpline::new(&pts, &[], 3, false, &[]).unwrap();
    assert_relative_eq!(c.curvature(c.domain().0), 0.1, epsilon = 1e-6);
}

#[test]
fn tangent_line_into_bspline() {
    let mut s = Sketch::new();
    let l = s.line([-10.0, 0.0], [0.0, 0.0]);
    let Geometry::Line { start, end } = s.entity(l).unwrap().geometry else { unreachable!() };
    fix(&mut s, start);
    fix(&mut s, end);
    let p1 = s.add_point(3.0, 2.0);
    let p2 = s.add_point(6.0, 4.0);
    let b = s.add_entity(Geometry::BSpline { poles: vec![end, p1, p2], degree: 2, closed: false, weights: vec![], knots: vec![] });
    s.constrain(SketchConstraint::Tangent { a: l, b });
    s.solve().unwrap();
    assert_relative_eq!(pt(&s, p1)[1], 0.0, epsilon = 1e-6);
}

#[test]
fn mid_handle_points_belong_to_the_spline() {
    let mut s = Sketch::new();
    let p: Vec<u32> = [[0.0, 0.0], [5.0, 3.0], [10.0, 0.0]].iter().map(|q| s.add_point(q[0], q[1])).collect();
    let h = s.add_point(7.0, 4.0);
    let sp = s.add_entity(Geometry::Spline { points: p.clone(), closed: false, start_handle: None, end_handle: None, handles: vec![[1, h]] });
    assert!(s.entity(sp).unwrap().geometry.point_ids().contains(&h));
    let mut t = s.clone();
    t.remove_entity(sp).unwrap();
    assert!(t.points.is_empty(), "la manija se va con la spline");
    // Con la manija inclinada, la extrusión cambia
    s.add_line(p[2], p[0]);
    let mut plain = s.clone();
    if let Geometry::Spline { handles, .. } = &mut plain.entities[0].geometry {
        handles.clear();
    }
    if let (Some(a), Some(b)) = (extrude_volume(plain), extrude_volume(s)) {
        assert!((a - b).abs() > 0.02 * a, "{a} → {b}");
    }
}

#[test]
fn new_geometry_json() {
    let g: Geometry = serde_json::from_str(r#"{"type":"bspline","poles":[1,2,3],"degree":2,"weights":[1,0.5,1]}"#).unwrap();
    assert!(matches!(g, Geometry::BSpline { closed: false, .. }));
    let j = serde_json::to_string(&g).unwrap();
    assert!(!j.contains("knots") && !j.contains("closed"), "{j}");
    let e: SketchEntity = serde_json::from_str(r#"{"id":4,"infinite":true,"construction":true,"geometry":{"type":"line","start":1,"end":2}}"#).unwrap();
    assert!(e.infinite);
}

#[test]
fn open_bspline_that_returns_to_its_start_is_a_region() {
    // Una spline por puntos cerrada pasada a polos: abierta, con el mismo punto al comienzo y al final
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0);
    let mid: Vec<u32> = [[10.0, -5.0], [20.0, 5.0], [10.0, 15.0], [-5.0, 10.0]].iter().map(|p| s.add_point(p[0], p[1])).collect();
    let mut poles = vec![a];
    poles.extend(&mid);
    poles.push(a);
    s.add_entity(Geometry::BSpline { poles, degree: 3, closed: false, weights: vec![], knots: vec![] });
    let regions = find_regions(&s).unwrap();
    assert_eq!(regions.len(), 1);
    if let Some(v) = extrude_volume(s) {
        assert_relative_eq!(v, regions[0].area(), max_relative = 2e-3);
    }
}

#[test]
fn text_style_round_trip() {
    let t: SketchText = serde_json::from_str(r#"{"id":1,"text":"A","size":5,"anchor":2,"entities":[],"points":[],"style":{"bold":true,"align":"center","path":7}}"#).unwrap();
    assert!(t.style.bold && t.style.align == "center" && t.style.path == Some(7));
    let plain: SketchText = serde_json::from_str(r#"{"id":1,"text":"A","size":5,"anchor":2,"entities":[],"points":[]}"#).unwrap();
    assert!(!serde_json::to_string(&plain).unwrap().contains("style"));
}
