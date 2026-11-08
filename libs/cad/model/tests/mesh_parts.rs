//! Piezas de malla (libs/cad/PLAN_MALLAS.md): el escaneo como cuerpo queda
//! como pieza de malla y sus booleanas van por triángulos (manifold-rust), no
//! por OCCT.

use std::sync::Arc;

use cad_model::*;

fn occt() -> bool {
    occt::available()
}

/// «Escaneo» de prueba: una esfera de radio 20 teselada fino (cerrada)
fn sphere_scan() -> ScanBodyMesh {
    let s = Shape::sphere([0.0, 0.0, 0.0], 20.0).unwrap();
    let data = cad_model::mesh_bool::shape_mesh(&s).unwrap();
    ScanBodyMesh { hash: 7, data, error: None }
}

fn scan_body(op: BodyOp) -> FeatureKind {
    FeatureKind::ScanBody(ScanBody { exact: true, op, mesh: Some(sphere_scan()), ..Default::default() })
}

fn block(origin: P3, size: [f64; 3], op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: size[0], dy: size[1], dz: size[2], centered: false, centered_z: false },
        origin,
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op,
        link: None,
        on: None,
    })
}

fn volume(p: &Part) -> f64 {
    p.shape.mass().unwrap().volume
}

#[test]
fn scan_body_is_a_mesh_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::default();
    doc.add(scan_body(BodyOp::New));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 1);
    assert!(ev.parts[0].mesh.is_some(), "el escaneo tiene que quedar como pieza de malla");
    let sphere = 4.0 / 3.0 * std::f64::consts::PI * 20f64.powi(3);
    assert!((volume(&ev.parts[0]) - sphere).abs() / sphere < 0.01);
}

#[test]
fn cut_join_and_split_by_mesh() {
    if !occt() {
        return;
    }
    let mut doc = Document::default();
    doc.add(scan_body(BodyOp::New));
    let before = volume(&doc.evaluate().parts[0]);

    // Un bloque que se come la esquina de arriba: resta por malla
    doc.add(block([0.0, 0.0, 10.0], [30.0, 30.0, 30.0], BodyOp::Cut));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 1);
    assert!(ev.parts[0].mesh.is_some());
    let after_cut = volume(&ev.parts[0]);
    assert!(after_cut < before - 100.0, "{before} → {after_cut}");

    // Una placa que la atraviesa por el medio: dos piezas
    doc.add(block([-1.0, -30.0, -30.0], [2.0, 60.0, 60.0], BodyOp::Cut));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 2, "la placa tiene que partirla en dos");
    assert!(ev.parts.iter().all(|p| p.mesh.is_some()));

    // Un bloque que une las dos mitades: una sola pieza otra vez
    doc.add(block([-5.0, -5.0, -15.0], [10.0, 10.0, 10.0], BodyOp::Join));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 1, "la unión tiene que juntar las dos mitades");
    assert!(ev.parts[0].mesh.is_some());
}

#[test]
fn cad_parts_still_use_occt() {
    if !occt() {
        return;
    }
    // Sin mallas, nada cambia: el bloque cortado sigue con caras exactas
    let mut doc = Document::default();
    doc.add(block([0.0, 0.0, 0.0], [20.0, 20.0, 20.0], BodyOp::New));
    doc.add(block([10.0, 10.0, 10.0], [20.0, 20.0, 20.0], BodyOp::Cut));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!(ev.parts[0].mesh.is_none());
    assert_eq!(ev.parts[0].shape.face_count(), 9);
    assert!((volume(&ev.parts[0]) - 7000.0).abs() < 1e-6);
}
