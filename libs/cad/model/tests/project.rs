//! Referencias al modelo en el sketch: intersección, silueta, otro sketch.

use approx::assert_relative_eq;
use cad_model::project::{self, Projected};
use cad_model::*;

fn occt() -> bool {
    if !occt::available() {
        eprintln!("OpenCASCADE no disponible: test omitido");
        return false;
    }
    true
}

fn prim(shape: PrimitiveShape, origin: [f64; 3], op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive { shape, origin, z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op, link: None })
}

fn cube(d: f64) -> FeatureKind {
    prim(PrimitiveShape::Box { dx: d, dy: d, dz: d, centered: true, centered_z: true }, [0.0; 3], BodyOp::Join)
}

fn set_dx(doc: &mut Document, id: FeatureId, v: f64) {
    if let FeatureKind::Primitive(p) = &mut doc.get_mut(id).unwrap().kind
        && let PrimitiveShape::Box { dx, .. } = &mut p.shape
    {
        *dx = v;
    }
}

fn count(curves: &[Projected]) -> (usize, usize, usize) {
    let lines = curves.iter().filter(|c| matches!(c, Projected::Line { .. })).count();
    let circles = curves.iter().filter(|c| matches!(c, Projected::Circle { .. })).count();
    (lines, circles, curves.len() - lines - circles)
}

/// Extremos en x de todas las líneas del sketch.
fn x_range(s: &Sketch) -> [f64; 2] {
    let mut r = [f64::INFINITY, f64::NEG_INFINITY];
    for e in &s.entities {
        for q in e.geometry.point_ids() {
            let p = s.point(q).unwrap();
            r = [r[0].min(p[0]), r[1].max(p[0])];
        }
    }
    r
}

/// Un sketch con las curvas ligadas a `source`.
fn linked(curves: &[Projected], source: UseSource) -> Sketch {
    let mut s = Sketch::new();
    for c in curves {
        let entity = project::add_to(&mut s, c);
        s.uses.push(SketchUse { edge: None, source: Some(source.clone()), entity });
    }
    s
}

#[test]
fn section_of_the_plane_follows_the_solid() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let bx = doc.add(cube(20.0));
    let ev = doc.evaluate();
    let curves = project::section(ev.body.as_ref().unwrap(), &Plane::XY).unwrap();
    assert_eq!(count(&curves), (4, 0, 0), "{curves:?}");
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: linked(&curves, UseSource::Section) });
    set_dx(&mut doc, bx, 30.0);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let r = x_range(&ev.sketches[&sk].sketch);
    assert_relative_eq!(r[0], -15.0, epsilon = 1e-6);
    assert_relative_eq!(r[1], 15.0, epsilon = 1e-6);
    assert_eq!(ev.sketches[&sk].regions.len(), 1);
}

#[test]
fn section_of_a_cylinder_side() {
    if !occt() {
        return;
    }
    // Cilindro de radio 5 en Z cortado por el plano de frente: rectángulo 10 × 10
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Cylinder { radius: 5.0, height: 10.0 }, [0.0; 3], BodyOp::Join));
    let ev = doc.evaluate();
    let curves = project::section(ev.body.as_ref().unwrap(), &Plane::XZ).unwrap();
    assert_eq!(count(&curves).0, 4, "{curves:?}");
}

#[test]
fn silhouette_has_the_through_hole_but_not_the_blind_one() {
    if !occt() {
        return;
    }
    // Placa 20 × 20 × 20 con un agujero pasante (r 3) y uno ciego (r 2) arriba
    let mut doc = Document::new();
    let bx = doc.add(cube(20.0));
    doc.add(prim(PrimitiveShape::Cylinder { radius: 3.0, height: 40.0 }, [-5.0, 0.0, -20.0], BodyOp::Cut));
    doc.add(prim(PrimitiveShape::Cylinder { radius: 2.0, height: 10.0 }, [5.0, 5.0, 5.0], BodyOp::Cut));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let top = Plane::XY.offset(30.0);
    let curves = project::silhouette(ev.body.as_ref().unwrap(), &top).unwrap();
    let circles: Vec<f64> = curves
        .iter()
        .filter_map(|c| match c {
            Projected::Circle { radius, .. } => Some(*radius),
            _ => None,
        })
        .collect();
    assert_eq!(count(&curves).0, 4, "{curves:?}");
    assert_eq!(circles.len(), 1, "{curves:#?}");
    assert_relative_eq!(circles[0], 3.0, epsilon = 1e-6);
    // Ligada: sigue a la placa cuando se ensancha
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch: linked(&curves, UseSource::Silhouette) });
    set_dx(&mut doc, bx, 30.0);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let r = x_range(&ev.sketches[&sk].sketch);
    assert_relative_eq!(r[0], -15.0, epsilon = 1e-6);
    assert_relative_eq!(r[1], 15.0, epsilon = 1e-6);
    // La placa con su isla y el círculo del agujero
    assert_eq!(ev.sketches[&sk].regions.len(), 2);
}

#[test]
fn silhouette_of_a_cylinder_from_the_front_and_an_l() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Cylinder { radius: 5.0, height: 10.0 }, [0.0; 3], BodyOp::Join));
    let ev = doc.evaluate();
    let curves = project::silhouette(ev.body.as_ref().unwrap(), &Plane::XZ.offset(20.0)).unwrap();
    let length: f64 = curves
        .iter()
        .map(|c| match c {
            Projected::Line { start, end } => ((start[0] - end[0]).powi(2) + (start[1] - end[1]).powi(2)).sqrt(),
            _ => 0.0,
        })
        .sum();
    assert!((length - 40.0).abs() < 1e-4, "{curves:#?}");
    // L: dos cajas pegadas; las aristas de adentro no son contorno y la de
    // abajo se corta donde empieza la otra caja
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 20.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false }, [0.0; 3], BodyOp::Join));
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 10.0, centered: false, centered_z: false }, [0.0; 3], BodyOp::Join));
    let ev = doc.evaluate();
    let curves = project::silhouette(ev.body.as_ref().unwrap(), &Plane::XY.offset(20.0)).unwrap();
    let perimeter: f64 = curves
        .iter()
        .map(|c| match c {
            Projected::Line { start, end } => ((start[0] - end[0]).powi(2) + (start[1] - end[1]).powi(2)).sqrt(),
            _ => 100.0,
        })
        .sum();
    assert_relative_eq!(perimeter, 80.0, epsilon = 1e-4);
}

#[test]
fn silhouette_of_a_sphere_is_a_circle() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Sphere { radius: 6.0 }, [1.0, 2.0, 0.0], BodyOp::Join));
    let ev = doc.evaluate();
    let curves = project::silhouette(ev.body.as_ref().unwrap(), &Plane::XY.offset(10.0)).unwrap();
    let r: f64 = curves
        .iter()
        .map(|c| match c {
            Projected::Circle { radius, .. } => *radius,
            Projected::Arc { center, start, .. } => ((center[0] - start[0]).powi(2) + (center[1] - start[1]).powi(2)).sqrt(),
            _ => panic!("{c:?}"),
        })
        .fold(0.0, f64::max);
    assert!((r - 6.0).abs() < 1e-4, "{curves:?}");
}

#[test]
fn entities_of_another_sketch() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let mut first = Sketch::new();
    let circle = first.circle([3.0, 4.0], 5.0);
    let line = first.line([0.0, 0.0], [10.0, 0.0]);
    let base = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: first });
    let ev = doc.evaluate();
    let r = &ev.sketches[&base];
    // Arriba, paralelo: igual; de frente: el círculo de canto es una línea
    let up = project::sketch_entity(&r.sketch, &r.plane, circle, &Plane::XY.offset(10.0)).unwrap();
    assert_eq!(up, Projected::Circle { center: [3.0, 4.0], radius: 5.0 });
    let front = project::sketch_entity(&r.sketch, &r.plane, circle, &Plane::XZ).unwrap();
    let Projected::Line { start, end } = front else { panic!("{front:?}") };
    assert_relative_eq!((start[0] - end[0]).abs(), 10.0, epsilon = 1e-9);
    assert_relative_eq!(start[1], 0.0, epsilon = 1e-9);
    // Inclinado 45°: elipse con semiejes 5 y 5·cos 45°
    let tilted = Plane::from_normal([0.0; 3], [0.0, 1.0, 1.0]);
    let Projected::Ellipse { center, major, minor } = project::sketch_entity(&r.sketch, &r.plane, circle, &tilted).unwrap() else { panic!() };
    let d = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    assert_relative_eq!(d(center, major), 5.0, epsilon = 1e-9);
    assert_relative_eq!(d(center, minor), 5.0 * 0.5f64.sqrt(), epsilon = 1e-9);

    // Ligado: el sketch de arriba sigue al de abajo
    let mut s = Sketch::new();
    for e in [circle, line] {
        let entity = project::add_to(&mut s, &project::sketch_entity(&r.sketch, &r.plane, e, &Plane::XY).unwrap());
        s.uses.push(SketchUse { edge: None, source: Some(UseSource::Sketch { feature: base, entity: e }), entity });
    }
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 10.0, sketch: s });
    if let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(base).unwrap().kind {
        let Geometry::Line { end, .. } = sketch.entity(line).unwrap().geometry else { unreachable!() };
        sketch.set_point(end, [20.0, 0.0]).unwrap();
        if let Geometry::Circle { radius, .. } = &mut sketch.entities.iter_mut().find(|e| e.id == circle).unwrap().geometry {
            *radius = 7.0;
        }
    }
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let s = &ev.sketches[&sk].sketch;
    assert_relative_eq!(x_range(s)[1], 20.0, epsilon = 1e-9);
    assert!(s.entities.iter().any(|e| matches!(e.geometry, Geometry::Circle { radius, .. } if (radius - 7.0).abs() < 1e-9)));
}

#[test]
fn old_edge_uses_still_load() {
    let s: Sketch = serde_json::from_str(
        r#"{"points":[],"entities":[],"constraints":[],"uses":[{"edge":{"point":[0,0,0],"direction":[1,0,0]},"entity":3}]}"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.uses[0].entity, 3);
    assert!(s.uses[0].edge.is_some() && s.uses[0].source.is_none());
}
