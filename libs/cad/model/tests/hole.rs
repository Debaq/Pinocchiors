//! Agujeros: simple, con caja, avellanado y ciego con punta.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

/// Placa 40×40×10 (z 0..10) y un sketch en la cara de arriba con dos puntos.
fn plate() -> (Document, FeatureId) {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([0.0, 0.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let mut s = Sketch::default();
    for x in [-10.0, 10.0] {
        let p = s.add_point(x, 0.0);
        s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Point { point: p }, construction: false, infinite: false, axis: false, layer: None });
        s.next_id += 1;
    }
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: ev.face_ref(top).unwrap() }, offset: 0.0, sketch: s });
    (doc, sk)
}

fn hole(sketch: FeatureId, diameter: f64, depth: HoleDepth, style: HoleStyle) -> FeatureKind {
    FeatureKind::Hole(Hole { sketch, points: vec![], diameter, depth, style, tip_angle: 0.0, thread: None, modeled: None, link: None, reverse: None })
}

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

#[test]
fn through_counterbore_and_countersink() {
    if !occt() {
        return;
    }
    let (mut doc, sk) = plate();
    let h = doc.add(hole(sk, 6.6, HoleDepth::ThroughAll, HoleStyle::Simple));
    let plate = 40.0 * 40.0 * 10.0;
    let a = PI * 3.3 * 3.3;
    assert_relative_eq!(vol(&doc), plate - 2.0 * a * 10.0, max_relative = 1e-6);
    // Caja de 11 × 6
    if let FeatureKind::Hole(x) = &mut doc.get_mut(h).unwrap().kind {
        x.style = HoleStyle::Counterbore { diameter: 11.0, depth: 6.0 };
    }
    let cb = PI * 5.5 * 5.5 * 6.0 + a * 4.0;
    assert_relative_eq!(vol(&doc), plate - 2.0 * cb, max_relative = 1e-6);
    // Avellanado de 12 a 90°: cono de 12 a 6,6 con alto 2,7
    if let FeatureKind::Hole(x) = &mut doc.get_mut(h).unwrap().kind {
        x.style = HoleStyle::Countersink { diameter: 12.0, angle: 90.0 };
    }
    let hgt = (12.0 - 6.6) / 2.0;
    let cone = PI * hgt / 3.0 * (36.0 + 3.3 * 3.3 + 6.0 * 3.3);
    assert_relative_eq!(vol(&doc), plate - 2.0 * (cone + a * (10.0 - hgt)), max_relative = 1e-6);
}

#[test]
fn blind_with_tip_and_no_points() {
    if !occt() {
        return;
    }
    let (mut doc, sk) = plate();
    let h = doc.add(FeatureKind::Hole(Hole {
        sketch: sk,
        points: vec![],
        diameter: 5.0,
        depth: HoleDepth::Blind { depth: 6.0 },
        style: HoleStyle::Simple,
        tip_angle: 118.0,
        thread: Some("M6".into()),
        modeled: None,
        link: None,
        reverse: None,
    }));
    let tip = 2.5 / (59.0f64).to_radians().tan();
    let one = PI * 6.25 * 6.0 + PI * 6.25 * tip / 3.0;
    assert_relative_eq!(vol(&doc), 16000.0 - 2.0 * one, max_relative = 1e-6);
    // El fondo no llega abajo: la cara de abajo sigue entera
    let ev = doc.evaluate();
    let (bottom, _) = ev.body.as_ref().unwrap().closest_face([0.0, 0.0, 0.0], Some([0.0, 0.0, -1.0]), 0.99).unwrap();
    assert_relative_eq!(ev.body.as_ref().unwrap().face_info(bottom).unwrap().area, 1600.0, max_relative = 1e-9);
    // Un sketch sin puntos ni círculos: error claro
    let mut doc2 = Document::new();
    let empty = doc2.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: Sketch::default() });
    let h2 = doc2.add(hole(empty, 5.0, HoleDepth::ThroughAll, HoleStyle::Simple));
    assert!(matches!(doc2.evaluate().state(h2), Some(FeatureState::Error { .. })));
    let _ = h;
}

/// Volumen por mm de un macho M`d` × `p` (perfil ISO básico): núcleo + filete por Pappus.
fn rod_per_mm(d: f64, p: f64) -> f64 {
    let d1 = d - 1.082_532 * p;
    let (r1, h) = (d1 / 2.0, (d - d1) / 2.0);
    let (top, bottom) = (p / 8.0, p / 8.0 + 2.0 * h * (PI / 6.0).tan());
    let rc = r1 + h * (bottom + 2.0 * top) / (3.0 * (bottom + top));
    PI * r1 * r1 + (top + bottom) / 2.0 * h / p * 2.0 * PI * rc
}

#[test]
fn modeled_thread_through_hole() {
    if !occt() {
        return;
    }
    let (mut doc, sk) = plate();
    let mut h = Hole { sketch: sk, points: vec![], diameter: 5.0, depth: HoleDepth::ThroughAll, style: HoleStyle::Simple, tip_angle: 0.0, thread: Some("M6".into()), modeled: None, link: None, reverse: None };
    h.modeled = Some(ThreadSpec { nominal: 6.0, pitch: 1.0, clearance: 0.0, left: false });
    doc.add(FeatureKind::Hole(h));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let body = ev.body.unwrap();
    assert!(body.is_valid());
    // Lo que se va en 10 mm es el macho entero (por vuelta el volumen no depende de la fase)
    assert_relative_eq!(body.mass().unwrap().volume, 16000.0 - 2.0 * 10.0 * rod_per_mm(6.0, 1.0), max_relative = 1e-4);
}

fn cylinder_doc(r: f64, h: f64) -> Document {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: r, height: h },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    doc
}

fn side_face(doc: &Document, at: P3, normal: P3) -> FaceRef {
    let ev = doc.evaluate();
    let (f, _) = ev.body.as_ref().unwrap().closest_face(at, Some(normal), 0.9).unwrap();
    ev.face_ref(f).unwrap()
}

#[test]
fn external_thread_on_a_pin() {
    if !occt() {
        return;
    }
    // Eje de Ø6 × 10: queda el macho M6
    let mut doc = cylinder_doc(3.0, 10.0);
    let face = side_face(&doc, [3.0, 0.0, 5.0], [1.0, 0.0, 0.0]);
    let th = doc.add(FeatureKind::Thread { face, pitch: 1.0, length: 0.0, flip: false, left: false, clearance: 0.0, link: None });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // (sin holgura la cresta sobresale 1 µm para no coincidir con la cara: 0,06 % de más)
    assert_relative_eq!(ev.body.unwrap().mass().unwrap().volume, 10.0 * rod_per_mm(6.0, 1.0), max_relative = 1e-3);
    // Solo 4 mm desde arriba: el resto queda liso
    if let FeatureKind::Thread { length, flip, .. } = &mut doc.get_mut(th).unwrap().kind {
        *length = 4.0;
        *flip = true;
    }
    let v = doc.evaluate().body.unwrap().mass().unwrap().volume;
    assert_relative_eq!(v, 6.0 * PI * 9.0 + 4.0 * rod_per_mm(6.0, 1.0), max_relative = 1e-3);
}

#[test]
fn internal_thread_on_a_hole() {
    if !occt() {
        return;
    }
    // Placa con un agujero al diámetro menor de M6: la rosca lo talla
    let (mut doc, sk) = plate();
    let d1 = 6.0 - 1.082_532;
    doc.add(FeatureKind::Hole(Hole { sketch: sk, points: vec![], diameter: d1, depth: HoleDepth::ThroughAll, style: HoleStyle::Simple, tip_angle: 0.0, thread: None, modeled: None, link: None, reverse: None }));
    let face = side_face(&doc, [-10.0 + d1 / 2.0, 0.0, 5.0], [-1.0, 0.0, 0.0]);
    doc.add(FeatureKind::Thread { face, pitch: 1.0, length: 0.0, flip: false, left: false, clearance: 0.0, link: None });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // Uno de los dos agujeros roscado, el otro liso
    let plain = PI * d1 * d1 / 4.0 * 10.0;
    assert_relative_eq!(ev.body.unwrap().mass().unwrap().volume, 16000.0 - plain - 10.0 * rod_per_mm(6.0, 1.0), max_relative = 1e-4);
}

#[test]
fn hole_cuts_only_the_nearest_part_unless_scope_says_otherwise() {
    if !occt() {
        return;
    }
    // Otra placa debajo, separada: el pasante entra solo en la de arriba
    let (mut doc, sk) = plate();
    let below = doc.insert(
        0,
        FeatureKind::Primitive(Primitive {
            shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false },
            origin: [0.0, 0.0, -20.0],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::New,
            link: None,
        }),
    );
    let h = doc.add(hole(sk, 6.0, HoleDepth::ThroughAll, HoleStyle::Simple));
    let full = 40.0 * 40.0 * 10.0;
    let cut = 2.0 * PI * 9.0 * 10.0;
    let volumes = |doc: &Document| {
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{:?}", ev.errors());
        let mut v: Vec<(f64, f64)> = ev.parts.iter().map(|p| { let m = p.shape.mass().unwrap(); (m.bbox_min[2], m.volume) }).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|(_, v)| v).collect::<Vec<_>>()
    };
    let v = volumes(&doc);
    assert_relative_eq!(v[0], full, max_relative = 1e-9);
    assert_relative_eq!(v[1], full - cut, max_relative = 1e-6);
    // El estado dice cuál eligió: la de arriba (no la de `below`)
    let ev = doc.evaluate();
    let auto = &ev.status.iter().find(|s| s.id == h).unwrap().auto_scope;
    assert_eq!(auto.len(), 1);
    assert_ne!(auto[0].feature, below);
    // Eligiendo las dos piezas, atraviesa ambas
    let ev = doc.evaluate();
    let ids: Vec<PartId> = ev.parts.iter().map(|p| p.id).collect();
    assert!(ids.iter().any(|p| p.feature == below));
    doc.get_mut(h).unwrap().scope = ids;
    let v = volumes(&doc);
    assert_relative_eq!(v[0], full - cut, max_relative = 1e-6);
    assert_relative_eq!(v[1], full - cut, max_relative = 1e-6);
}

#[test]
fn hole_from_a_base_plane_goes_into_the_block_above() {
    if !occt() {
        return;
    }
    // Sketch en la planta con un cuadrado y un círculo; el bloque sale hacia
    // arriba con las dos regiones y el agujero usa el mismo sketch (el centro
    // del círculo): contra la normal no hay material, entra hacia arriba
    let mut doc = Document::new();
    let mut s = Sketch::default();
    let c = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)].map(|(x, y)| s.add_point(x, y));
    for k in 0..4 {
        s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Line { start: c[k], end: c[(k + 1) % 4] }, construction: false, infinite: false, axis: false, layer: None });
        s.next_id += 1;
    }
    let o = s.add_point(0.0, 0.0);
    s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Circle { center: o, radius: 8.0 }, construction: false, infinite: false, axis: false, layer: None });
    s.next_id += 1;
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        // El anillo del cuadrado y el disco del círculo
        regions: RegionSelection::Points { points: vec![[15.0, 15.0], [0.0, 0.0]] },
        extent: Extent::Blind { distance: 10.0 },
        reverse: false,
        op: BodyOp::Join,
        draft: 0.0,
        thin: None,
    }));
    assert_relative_eq!(vol(&doc), 40.0 * 40.0 * 10.0, max_relative = 1e-6);
    let h = doc.add(hole(sk, 6.0, HoleDepth::ThroughAll, HoleStyle::Simple));
    assert_relative_eq!(vol(&doc), 40.0 * 40.0 * 10.0 - PI * 9.0 * 10.0, max_relative = 1e-6);
    // Flechas: el bloque sube 10 desde el medio de las regiones; el agujero
    // sube desde el centro (a favor de la normal) hasta la tapa
    let ev = doc.evaluate();
    let handle = |id| ev.status.iter().find(|s| s.id == id).unwrap().handle.clone().unwrap();
    let e = handle(doc.features[1].id);
    assert_eq!((e.dir, e.length, e.reversed, e.kind), ([0.0, 0.0, 1.0], 10.0, false, HandleKind::Blind));
    assert!(e.origin.iter().all(|v| v.abs() < 1e-6), "{:?}", e.origin);
    let k = handle(h);
    assert_eq!((k.origin, k.dir, k.reversed, k.kind), ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], true, HandleKind::Fixed));
    assert_relative_eq!(k.length, 10.0, epsilon = 1e-6);
    // Hacia abajo a la fuerza: no corta nada y lo dice
    if let FeatureKind::Hole(x) = &mut doc.get_mut(h).unwrap().kind {
        x.reverse = Some(false);
        x.depth = HoleDepth::Blind { depth: 5.0 };
    }
    let ev = doc.evaluate();
    assert_relative_eq!(ev.body.as_ref().unwrap().mass().unwrap().volume, 40.0 * 40.0 * 10.0, max_relative = 1e-12);
    match &ev.status.iter().find(|s| s.id == h).unwrap().state {
        FeatureState::Warning { message, .. } => assert!(message.contains("no toca ninguna pieza"), "{message}"),
        other => panic!("{other:?}"),
    }
    let k = ev.status.iter().find(|s| s.id == h).unwrap().handle.clone().unwrap();
    assert_eq!((k.dir, k.length, k.reversed, k.kind), ([0.0, 0.0, -1.0], 5.0, false, HandleKind::Blind));
}
