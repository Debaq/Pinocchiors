//! Varias piezas en un diseño: crear, unir, restar y operar por pieza.

use approx::assert_relative_eq;
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn cube(x: f64, size: f64, op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: size, dy: size, dz: size, centered: false, centered_z: false },
        origin: [x, 0.0, 0.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op,
    })
}

fn hole(x: f64, y: f64, r: f64) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: r, height: 100.0 },
        origin: [x, y, -50.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    })
}

fn volumes(ev: &Evaluation) -> Vec<f64> {
    ev.parts.iter().map(|p| p.shape.mass().unwrap().volume).collect()
}

#[test]
fn new_creates_parts_and_cut_only_touches_its_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = doc.add(cube(0.0, 10.0, BodyOp::Join));
    let b = doc.add(cube(30.0, 10.0, BodyOp::New));
    let ev = doc.evaluate();
    assert_eq!(ev.parts.len(), 2);
    assert_eq!(ev.parts[0].id, PartId { feature: a, index: 0 });
    assert_eq!(ev.parts[1].id, PartId { feature: b, index: 0 });
    // Un agujero en la segunda: la primera queda exactamente igual
    let first = ev.parts[0].shape.mass().unwrap().volume;
    doc.add(hole(35.0, 5.0, 2.0));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let v = volumes(&ev);
    assert_eq!(v[0], first);
    assert_relative_eq!(v[1], 1000.0 - std::f64::consts::PI * 4.0 * 10.0, max_relative = 1e-9);
    // El cuerpo es la suma, con las caras de cada pieza en orden
    assert_relative_eq!(ev.body.as_ref().unwrap().mass().unwrap().volume, v[0] + v[1], max_relative = 1e-9);
    let ranges = ev.part_ranges();
    assert_eq!(ranges[0].1, 0..6);
    let body = ev.body.as_ref().unwrap();
    for (pi, (_, faces, _)) in ranges.iter().enumerate() {
        for (k, f) in faces.clone().enumerate() {
            let (g, l) = (body.face_info(f).unwrap(), ev.parts[pi].shape.face_info(k).unwrap());
            assert_eq!(g.area, l.area, "cara {f}");
            assert_eq!(ev.part_of_face(f), Some(ev.parts[pi].id));
        }
    }
}

#[test]
fn join_merges_what_it_touches_and_otherwise_makes_a_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(cube(30.0, 10.0, BodyOp::Join));
    let ev = doc.evaluate();
    assert_eq!(ev.parts.len(), 2, "unir sin tocar nada es una pieza nueva");
    // Un puente que toca las dos: queda una sola pieza, con el id de la primera
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 30.0, dy: 4.0, dz: 4.0, centered: false, centered_z: false },
        origin: [5.0, 3.0, 3.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    assert_eq!(ev.parts.len(), 1);
    assert_eq!(ev.parts[0].id, PartId { feature: a, index: 0 });
    assert_relative_eq!(volumes(&ev)[0], 2000.0 + 20.0 * 16.0, max_relative = 1e-9);
    // Apoyada sobre otra (comparte una cara): también se unen
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(cube(10.0, 10.0, BodyOp::Join));
    assert_eq!(doc.evaluate().parts.len(), 1);
}

#[test]
fn fillet_and_shell_work_per_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(cube(30.0, 10.0, BodyOp::New));
    let ev = doc.evaluate();
    let first = volumes(&ev)[0];
    let body = ev.body.as_ref().unwrap();
    // Arista de arriba adelante de la segunda pieza
    let (e, _) = body.closest_edge([35.0, 0.0, 10.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    doc.add(FeatureKind::Fillet { edges: vec![ev.edge_ref(e).unwrap()], radius: 2.0 });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let v = volumes(&ev);
    assert_eq!(v[0], first, "la otra pieza no se tocó");
    assert_relative_eq!(v[1], 1000.0 - (4.0 - std::f64::consts::PI) * 10.0, max_relative = 1e-6);
    // Vaciado abriendo la tapa de la primera: la segunda no cambia
    let body = ev.body.as_ref().unwrap();
    let (top, _) = body.closest_face([5.0, 5.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let second = v[1];
    doc.add(FeatureKind::Shell { faces: vec![ev.face_ref(top).unwrap()], thickness: 1.0 });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let v = volumes(&ev);
    assert_relative_eq!(v[0], 1000.0 - 8.0 * 8.0 * 9.0, max_relative = 1e-6);
    assert_eq!(v[1], second);
    assert_eq!(ev.parts.len(), 2);
}

#[test]
fn cutting_a_whole_part_removes_it_and_intersect_drops_untouched() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(cube(30.0, 10.0, BodyOp::New));
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 20.0, centered: false, centered_z: false },
        origin: [25.0, -5.0, -5.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 1, "la segunda pieza desapareció entera");
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(cube(30.0, 10.0, BodyOp::New));
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 5.0, dy: 5.0, dz: 5.0, centered: false, centered_z: false },
        origin: [0.0, 0.0, 0.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Intersect,
    }));
    let ev = doc.evaluate();
    assert_eq!(ev.parts.len(), 1);
    assert_relative_eq!(volumes(&ev)[0], 125.0, max_relative = 1e-9);
}
