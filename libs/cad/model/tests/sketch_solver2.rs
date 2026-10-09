//! Solver II: grados libres a la vista, sugerencias, definir todo, cambios
//! grandes de cota y resolución parcial con conflicto.
use approx::assert_relative_eq;
use cad_model::*;

fn pt(s: &Sketch, id: u32) -> P2 {
    s.point(id).unwrap()
}

fn ends(s: &Sketch, line: u32) -> (u32, u32) {
    let Geometry::Line { start, end } = s.entity(line).unwrap().geometry else { unreachable!() };
    (start, end)
}

/// Área con signo del polígono de esos puntos (+ antihorario).
fn area(s: &Sketch, ids: &[u32]) -> f64 {
    let p: Vec<P2> = ids.iter().map(|&i| pt(s, i)).collect();
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        a[0] * b[1] - a[1] * b[0]
    }).sum::<f64>() / 2.0
}

#[test]
fn free_directions_and_entity_dof() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    // Base fija sobre el eje X; un punto que solo puede deslizar sobre ella
    let b = s.add_point(10.0, 0.0);
    s.constrain(SketchConstraint::Fixed { point: b, x: 10.0, y: 0.0 });
    let base = s.add_line(o, b);
    let p = s.add_point(4.0, 0.5);
    s.constrain(SketchConstraint::PointOnLine { point: p, line: base });
    // Línea suelta (4 grados) y círculo con el centro fijo (radio libre)
    let free = s.line([0.0, 5.0], [3.0, 6.0]);
    let c = s.circle([20.0, 0.0], 2.0);
    let Geometry::Circle { center, .. } = s.entity(c).unwrap().geometry else { unreachable!() };
    s.constrain(SketchConstraint::Fixed { point: center, x: 20.0, y: 0.0 });
    let r = s.solve().unwrap();
    let dir = r.free_dirs.iter().find(|d| d.0 == p).expect("el punto sobre la línea tiene una dirección").1;
    assert_relative_eq!(dir[1].abs(), 0.0, epsilon = 1e-6);
    assert_relative_eq!(dir[0].abs(), 1.0, epsilon = 1e-6);
    let dof = |e: u32| r.entity_dof.iter().find(|d| d.0 == e).map(|d| d.1);
    assert_eq!(dof(free), Some(4));
    assert_eq!(dof(c), Some(1));
    assert!(r.free_radius.contains(&c));
    assert_eq!(dof(base), None, "la base está fija");
    // Los puntos de la línea suelta van en cualquier dirección: no llevan flecha
    let (a, _) = ends(&s, free);
    assert!(r.free_points.contains(&a) && !r.free_dirs.iter().any(|d| d.0 == a));
}

/// Rectángulo dibujado un poco torcido, con una esquina en el origen
fn skewed_rectangle(s: &mut Sketch) -> [u32; 4] {
    let o = s.ensure_origin();
    let b = s.add_point(20.0, 0.25);
    let c = s.add_point(20.3, 10.0);
    let d = s.add_point(-0.2, 10.1);
    [s.add_line(o, b), s.add_line(b, c), s.add_line(c, d), s.add_line(d, o)]
}

#[test]
fn suggests_what_is_almost_true_and_useful() {
    let mut s = Sketch::new();
    let lines = skewed_rectangle(&mut s);
    let sug = s.suggest().unwrap();
    let has = |c: &SketchConstraint| sug.iter().any(|x| &x.constraint == c);
    assert!(has(&SketchConstraint::Horizontal { line: lines[0] }));
    assert!(has(&SketchConstraint::Vertical { line: lines[1] }));
    assert!(has(&SketchConstraint::Horizontal { line: lines[2] }));
    assert!(has(&SketchConstraint::Vertical { line: lines[3] }));
    // Con las cuatro, paralelas y perpendiculares no agregan nada
    assert!(!sug.iter().any(|x| matches!(x.constraint, SketchConstraint::Parallel { .. } | SketchConstraint::Perpendicular { .. })), "{sug:?}");
    assert!(sug.iter().all(|x| !x.why.is_empty()));
    // Con las relaciones puestas, ya no las sugiere; quedan las cotas
    for l in [0, 2] {
        s.constrain(SketchConstraint::Horizontal { line: lines[l] });
    }
    for l in [1, 3] {
        s.constrain(SketchConstraint::Vertical { line: lines[l] });
    }
    s.solve().unwrap();
    let sug = s.suggest().unwrap();
    assert!(!sug.iter().any(|x| matches!(x.constraint, SketchConstraint::Horizontal { .. } | SketchConstraint::Vertical { .. })));
    let lengths = sug.iter().filter(|x| matches!(x.constraint, SketchConstraint::Length { .. })).count();
    assert_eq!(lengths, 2, "ancho y alto: los otros dos lados salen de ahí ({sug:?})");
}

#[test]
fn auto_define_leaves_it_fully_defined_without_moving() {
    for relations in [true, false] {
        let mut s = Sketch::new();
        skewed_rectangle(&mut s);
        let circle = s.circle([8.0, 5.0], 2.5);
        let before: Vec<SketchPoint> = s.points.clone();
        let add = s.auto_define(relations).unwrap();
        assert!(!add.is_empty());
        for c in add {
            s.constrain(c);
        }
        let r = s.solve().unwrap();
        assert_eq!(r.status, SketchStatus::WellConstrained, "relaciones {relations}: {r:?}");
        assert!(r.conflicting.is_empty());
        if !relations {
            // Solo cotas con lo que mide: nada se mueve
            for p in &before {
                let q = pt(&s, p.id);
                assert_relative_eq!(q[0], p.x, epsilon = 1e-5);
                assert_relative_eq!(q[1], p.y, epsilon = 1e-5);
            }
        }
        assert_relative_eq!(s.radius(circle).unwrap(), 2.5, epsilon = 1e-5);
        let n = s.constraints.len();
        // Y si ya está definido no agrega nada
        assert!(s.auto_define(relations).unwrap().is_empty(), "{:?}", &s.constraints[..n]);
    }
}

#[test]
fn large_dimension_change_keeps_the_shape() {
    // Cuadrilátero articulado: A y D fijos, B y C sueltos; agrandar mucho
    // el lado de arriba no tiene que cruzar el polígono
    let mut s = Sketch::new();
    let a = s.ensure_origin();
    let d = s.add_point(10.0, 0.0);
    s.constrain(SketchConstraint::Fixed { point: d, x: 10.0, y: 0.0 });
    let b = s.add_point(1.0, 4.0);
    let c = s.add_point(9.0, 4.0);
    let ab = s.add_line(a, b);
    let bc = s.add_line(b, c);
    let cd = s.add_line(c, d);
    s.add_line(d, a);
    let opts = DimOpts::default;
    s.constrain(SketchConstraint::Length { line: ab, value: (17.0f64).sqrt(), reference: false, opts: opts() });
    s.constrain(SketchConstraint::Length { line: cd, value: (17.0f64).sqrt(), reference: false, opts: opts() });
    s.constrain(SketchConstraint::Horizontal { line: bc });
    let top = s.constrain(SketchConstraint::Length { line: bc, value: 8.0, reference: false, opts: opts() });
    s.solve().unwrap();
    let before = area(&s, &[a, d, c, b]);
    assert!(before > 0.0);
    // 8 → 14: B y C se abren hacia afuera, la figura sigue del mismo lado
    s.constraints[top].set_value(14.0);
    let r = s.solve().unwrap();
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert_relative_eq!(s.measure(&s.constraints[top].clone()).unwrap(), 14.0, epsilon = 1e-6);
    assert!(pt(&s, b)[1] > 0.0 && pt(&s, c)[1] > 0.0, "no se dio vuelta: {:?} {:?}", pt(&s, b), pt(&s, c));
    assert!(pt(&s, b)[0] < pt(&s, c)[0], "B sigue a la izquierda de C");
}

#[test]
fn large_angle_change_goes_the_short_way() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x);
    s.constrain(SketchConstraint::Horizontal { line: base });
    s.constrain(SketchConstraint::Length { line: base, value: 10.0, reference: false, opts: Default::default() });
    let tip = s.add_point(9.8, 1.7);
    let arm = s.add_line(o, tip);
    s.constrain(SketchConstraint::Length { line: arm, value: 10.0, reference: false, opts: Default::default() });
    let ang = s.constrain(SketchConstraint::Angle { a: base, b: arm, degrees: 10.0, supplementary: false, reference: false, opts: Default::default() });
    s.solve().unwrap();
    s.constraints[ang].set_value(150.0);
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    let p = pt(&s, tip);
    assert_relative_eq!(p[1].atan2(p[0]).to_degrees(), 150.0, epsilon = 1e-4);
}

#[test]
fn partial_solve_satisfies_everything_but_the_conflict() {
    let mut s = Sketch::new();
    let o = s.ensure_origin();
    let p = s.add_point(6.0, 1.0);
    let l = s.add_line(o, p);
    s.constrain(SketchConstraint::Horizontal { line: l });
    s.constrain(SketchConstraint::Length { line: l, value: 5.0, reference: false, opts: Default::default() });
    let other = s.line([0.0, 5.0], [3.0, 6.0]);
    let (q, _) = ends(&s, other);
    // q en el extremo de l… y otro largo que choca con el primero
    s.constrain(SketchConstraint::Distance { a: p, b: q, value: 4.0, reference: false, opts: Default::default() });
    s.constrain(SketchConstraint::Length { line: l, value: 7.0, reference: false, opts: Default::default() });
    let r = s.solve().unwrap();
    assert_eq!(r.status, SketchStatus::OverConstrained);
    assert!(r.partial, "{r:?}");
    assert!(!r.conflicting.is_empty());
    // Horizontal y la distancia a q se cumplen enteras; el largo es uno de los dos
    assert_relative_eq!(pt(&s, p)[1], 0.0, epsilon = 1e-6);
    let len = pt(&s, p)[0].hypot(pt(&s, p)[1]);
    assert!((len - 5.0).abs() < 1e-6 || (len - 7.0).abs() < 1e-6, "largo {len}");
    assert_relative_eq!(dist(pt(&s, p), pt(&s, q)), 4.0, epsilon = 1e-6);
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
