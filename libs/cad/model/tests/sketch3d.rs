//! Sketch 3D, sketch envuelto sobre una cara y perforación.

use approx::assert_relative_eq;
use cad_model::occt::SurfaceKind;
use cad_model::sketch3d::Targets;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn line(s: &mut Sketch3d, a: u32, b: u32) -> u32 {
    s.add_entity(Geometry3d::Line { start: a, end: b })
}

/// Escalera en 3D: arranca fija en el origen, un tramo por eje con su largo
fn stairs() -> (Sketch3d, [u32; 4]) {
    let mut s = Sketch3d::default();
    let p = [[0.0, 0.0, 0.0], [9.0, 0.5, 0.2], [9.3, 6.0, -0.4], [9.0, 6.4, 4.0]].map(|q| s.add_point(q));
    let l = [line(&mut s, p[0], p[1]), line(&mut s, p[1], p[2]), line(&mut s, p[2], p[3])];
    s.constraints.push(Constraint3d::Fixed { point: p[0], at: [0.0; 3] });
    for (k, (axis, len)) in [(WorldAxis::X, 10.0), (WorldAxis::Y, 5.0), (WorldAxis::Z, 4.0)].into_iter().enumerate() {
        s.constraints.push(Constraint3d::AlongAxis { line: l[k], axis });
        s.constraints.push(Constraint3d::Length { line: l[k], value: len });
    }
    (s, p)
}

#[test]
fn stairs_along_the_axes_are_fully_defined() {
    let (mut s, p) = stairs();
    let r = s.solve(&Targets::default()).unwrap();
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    assert_eq!(r.dof, 0);
    let end = s.point(p[3]).unwrap();
    for (a, b) in end.iter().zip([10.0, 5.0, 4.0]) {
        assert_relative_eq!(*a, b, epsilon = 1e-7);
    }
    // Sin el largo del último, le queda un grado (desliza sobre Z)
    s.constraints.pop();
    let r = s.solve(&Targets::default()).unwrap();
    assert_eq!(r.status, SketchStatus::UnderConstrained);
    assert_eq!(r.dof, 1);
    assert_eq!(r.free_points, vec![p[3]]);
}

#[test]
fn conflict_keeps_the_rest() {
    let (mut s, p) = stairs();
    // Otro largo para el primer tramo: choca
    let first = s.entities[0].id;
    s.constraints.push(Constraint3d::Length { line: first, value: 12.0 });
    let r = s.solve(&Targets::default()).unwrap();
    assert_eq!(r.status, SketchStatus::OverConstrained);
    assert_eq!(r.conflicting.len(), 1, "{r:?}");
    // Lo demás se cumple: los tramos siguen sobre sus ejes
    let (b, c) = (s.point(p[1]).unwrap(), s.point(p[2]).unwrap());
    assert_relative_eq!(b[1], 0.0, epsilon = 1e-7);
    assert_relative_eq!(c[0], b[0], epsilon = 1e-7);
    assert!((b[0] - 10.0).abs() < 1e-6 || (b[0] - 12.0).abs() < 1e-6, "{b:?}");
}

#[test]
fn tangent_arc_and_perpendicular() {
    // Línea por X, arco tangente que sube en el plano XZ, línea vertical al final
    let mut s = Sketch3d::default();
    let o = s.add_point([0.0, 0.0, 0.0]);
    let a = s.add_point([10.0, 0.0, 0.0]);
    let m = s.add_point([13.5, 0.0, 1.6]);
    let b = s.add_point([15.2, 0.0, 5.3]);
    let c = s.add_point([15.0, 0.0, 15.0]);
    let l1 = line(&mut s, o, a);
    let arc = s.add_entity(Geometry3d::Arc { start: a, mid: m, end: b });
    let l2 = line(&mut s, b, c);
    s.constraints.extend([
        Constraint3d::Fixed { point: o, at: [0.0; 3] },
        Constraint3d::AlongAxis { line: l1, axis: WorldAxis::X },
        Constraint3d::Length { line: l1, value: 10.0 },
        Constraint3d::AlongAxis { line: l2, axis: WorldAxis::Z },
        Constraint3d::Tangent { a: l1, b: arc },
        Constraint3d::Tangent { a: arc, b: l2 },
        Constraint3d::Perpendicular { a: l1, b: l2 },
    ]);
    let r = s.solve(&Targets::default()).unwrap();
    assert!(r.conflicting.is_empty(), "{r:?}");
    // El arco es un cuarto de vuelta: su centro está sobre la vertical de `a`
    let (pa, pb) = (s.point(a).unwrap(), s.point(b).unwrap());
    let center = sketch3d::circumcenter(pa, s.point(m).unwrap(), pb).unwrap();
    assert_relative_eq!(center[0], pa[0], epsilon = 1e-6);
    assert_relative_eq!(center[2], pb[2], epsilon = 1e-6);
    let chains = s.chains().unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].len(), 3);
}

#[test]
fn sweep_along_a_3d_sketch_attached_to_the_model() {
    if !occt() {
        return;
    }
    // Camino en L sobre el plano a 5 de la planta, que arranca en un punto de
    // referencia; perfil: círculo de radio 1 en el plano YZ que la perfora
    let mut doc = Document::new();
    let start = doc.add(FeatureKind::Point { def: PointSpec::At { point: [0.0, 0.0, 5.0] } });
    let mut s = Sketch3d::default();
    let p = [[0.3, 0.2, 4.0], [20.0, 1.0, 5.5], [20.5, 15.0, 6.0]].map(|q| s.add_point(q));
    let l1 = line(&mut s, p[0], p[1]);
    let l2 = line(&mut s, p[1], p[2]);
    s.constraints.extend([
        Constraint3d::Attach { point: p[0], target: PointSpec::Reference { feature: start } },
        Constraint3d::AlongAxis { line: l1, axis: WorldAxis::X },
        Constraint3d::AlongAxis { line: l2, axis: WorldAxis::Y },
        Constraint3d::Length { line: l1, value: 20.0 },
        Constraint3d::Length { line: l2, value: 15.0 },
    ]);
    for q in p {
        s.constraints.push(Constraint3d::OnPlane { point: q, plane: PlaneSpec::Xy, offset: 5.0 });
    }
    let path = doc.add(FeatureKind::Sketch3d { sketch: s });
    // Perfil en el plano YZ (x del sketch = Y, y del sketch = Z): el centro lo pone la perforación
    let mut prof = Sketch::default();
    let c = prof.circle([1.0, 1.0], 1.0);
    let Geometry::Circle { center, .. } = prof.entity(c).unwrap().geometry else { unreachable!() };
    prof.constrain(SketchConstraint::Pierce { point: center, curve: path.0, at: [1.0, 1.0] });
    prof.constrain(SketchConstraint::Radius { entity: c, value: 1.0, reference: false, opts: Default::default() });
    let pf = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Yz, offset: 0.0, sketch: prof });
    doc.add(FeatureKind::Sweep(Sweep { sketch: pf, regions: RegionSelection::All, path: SweepPath::Curve { feature: path }, op: BodyOp::Join }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let r = &ev.sketches3d[&path].report;
    assert_eq!(r.status, SketchStatus::WellConstrained, "{r:?}");
    // La perforación dejó el centro donde el camino cruza el plano YZ: (0, 5)
    let prof = &ev.sketches[&pf].sketch;
    let cc = prof.point(center).unwrap();
    assert_relative_eq!(cc[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(cc[1], 5.0, epsilon = 1e-6);
    let body = ev.body.unwrap();
    assert!(body.is_valid());
    let v = body.mass().unwrap().volume;
    assert!(v > PI * 30.0 && v < PI * 35.0, "{v}");
    // Las curvas se ven en el visor
    assert!(matches!(&ev.references[&path], RefGeom::Curves { lines } if lines.len() == 2));
}

#[test]
fn attach_to_a_solid_vertex() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 30.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    // La arista vertical en (10, 20): su extremo de arriba
    let (k, info) = body.edges().unwrap().into_iter().enumerate().find(|(_, e)| (e.start[0] - 10.0).abs() < 1e-9 && (e.start[1] - 20.0).abs() < 1e-9 && (e.end[0] - 10.0).abs() < 1e-9 && (e.end[1] - 20.0).abs() < 1e-9).unwrap();
    let end = info.end[2] > info.start[2];
    let edge = ev.edge_ref(k).unwrap();
    let mut s = Sketch3d::default();
    let a = s.add_point([0.0, 0.0, 0.0]);
    let b = s.add_point([3.0, 4.0, 5.0]);
    line(&mut s, a, b);
    s.constraints.push(Constraint3d::Fixed { point: a, at: [0.0; 3] });
    s.constraints.push(Constraint3d::Attach { point: b, target: PointSpec::EdgeEnd { edge, end } });
    let id = doc.add(FeatureKind::Sketch3d { sketch: s });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let q = ev.sketches3d[&id].sketch.point(b).unwrap();
    assert_relative_eq!(q[0], 10.0, epsilon = 1e-7);
    assert_relative_eq!(q[1], 20.0, epsilon = 1e-7);
    assert_relative_eq!(q[2], 30.0, epsilon = 1e-7);
}

#[test]
fn surface_sketch_wraps_onto_a_cylinder() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 10.0, height: 40.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let k = (0..body.face_count()).find(|&i| body.face_info(i).unwrap().surface == SurfaceKind::Cylinder).unwrap();
    let face = ev.face_ref(k).unwrap();
    // Una recta de 20 mm a lo largo de u (la vuelta): sobre el cilindro es un
    // arco de 20 mm de largo, a radio 10 y a la misma altura
    let mut s = Sketch::default();
    s.line([-10.0, 0.0], [10.0, 0.0]);
    let id = doc.add(FeatureKind::SurfaceSketch { face, sketch: s });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let RefGeom::Curves { lines } = &ev.references[&id] else { panic!("sin curvas") };
    let pts: Vec<P3> = lines.concat();
    for p in &pts {
        assert_relative_eq!(p[0].hypot(p[1]), 10.0, epsilon = 1e-3);
        assert_relative_eq!(p[2], pts[0][2], epsilon = 1e-3);
    }
    let len: f64 = lines.iter().map(|l| l.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt()).sum::<f64>()).sum();
    assert_relative_eq!(len, 20.0, max_relative = 2e-3);
    // El editor del sketch lo ve en el plano tangente
    let plane = ev.sketches[&id].plane;
    assert_relative_eq!(plane.origin[0].hypot(plane.origin[1]), 10.0, epsilon = 1e-6);
}

#[test]
fn pierce_finds_the_helix_crossing() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let hx = doc.add(FeatureKind::Helix { axis: AxisSpec::Z, radius: 10.0, pitch: 5.0, turns: 2.0, left: false });
    // El plano XZ corta la hélice en x = ±10; el punto arranca cerca de (10, 0)
    let mut s = Sketch::default();
    let p = s.add_point(9.0, 1.0);
    s.add_entity(Geometry::Point { point: p });
    s.constrain(SketchConstraint::Pierce { point: p, curve: hx.0, at: [9.0, 1.0] });
    let id = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let sk = &ev.sketches[&id];
    let w = sk.plane.to_world(sk.sketch.point(p).unwrap());
    assert_relative_eq!(w[0], 10.0, epsilon = 1e-6);
    assert_relative_eq!(w[1], 0.0, epsilon = 1e-6);
    // Primer cruce por x = +10 arriba de z = 0: la hélice arranca ahí
    assert!(w[2].abs() < 1e-6 || (w[2] - 5.0).abs() < 1e-6, "{w:?}");
    assert!(doc.get(id).unwrap().kind.dependencies().contains(&hx));
}


