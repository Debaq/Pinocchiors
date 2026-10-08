//! Roscas coordinadas: un tornillo puesto en un agujero roscado, y una rosca
//! tallada en un agujero alrededor de un tornillo, calzan filete con filete.

use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn spec(left: bool) -> ThreadSpec {
    ThreadSpec { nominal: 6.0, pitch: 1.0, clearance: 0.0, left }
}

/// Ángulo de la cresta de `t` a la altura `a` sobre su eje (contando desde su origen).
fn crest_angle(t: &ThreadAxis, a: f64) -> f64 {
    let turn = 2.0 * PI * a / t.spec.pitch;
    if t.spec.left { -turn } else { turn }
}

/// Punto de la hélice de `t` en el parámetro `u` (vueltas).
fn helix_point(t: &ThreadAxis, u: f64) -> P3 {
    let z = geom::normalize(t.dir);
    let x = geom::normalize(t.x);
    let y = geom::cross(z, x);
    let a = 2.0 * PI * u * if t.spec.left { -1.0 } else { 1.0 };
    let r = 3.0;
    [0, 1, 2].map(|i| t.origin[i] + r * (a.cos() * x[i] + a.sin() * y[i]) + u * t.spec.pitch * z[i])
}

/// ¿El punto está sobre la hélice de `t` (mismo ángulo de cresta a su altura)?
fn on_helix(t: &ThreadAxis, p: P3) -> bool {
    let z = geom::normalize(t.dir);
    let x = geom::normalize(t.x);
    let y = geom::cross(z, x);
    let v = geom::sub(p, t.origin);
    let ang = geom::dot(v, y).atan2(geom::dot(v, x));
    let diff = (ang - crest_angle(t, geom::dot(v, z))).rem_euclid(2.0 * PI);
    diff.min(2.0 * PI - diff) < 1e-9
}

fn axis(origin: P3, dir: P3, x: P3, left: bool) -> ThreadAxis {
    ThreadAxis {
        feature: FeatureId(1),
        index: 0,
        spec: spec(left),
        internal: true,
        modeled: true,
        origin,
        dir,
        x,
        mouth: origin,
        out: [0.0, 0.0, 1.0],
        length: 10.0,
        blind: false,
        placed: None,
        link: None,
    }
}

#[test]
fn phase_follows_the_same_helix() {
    let s = 0.5f64.sqrt();
    for left in [false, true] {
        let src = axis([1.0, 2.0, 3.0], [0.0, s, s], [1.0, 0.0, 0.0], left);
        // Seguidoras en el mismo eje, corridas, en el mismo sentido y al revés
        for (shift, flip) in [(0.37, false), (-2.15, false), (4.2, true), (-0.73, true)] {
            let origin = geom::add(src.origin, geom::scale(src.dir, shift));
            let dir = if flip { geom::scale(src.dir, -1.0) } else { src.dir };
            let follower = ThreadAxis { origin, dir, x: src.phase_x(origin, dir), ..src.clone() };
            for k in 0..20 {
                let p = helix_point(&follower, k as f64 * 0.137);
                assert!(on_helix(&src, p), "left={left} shift={shift} flip={flip} u={}", k as f64 * 0.137);
            }
        }
    }
}

/// Placa 40×40×10 (z 0..10) con un sketch arriba y dos puntos (x = −10 y 10).
fn plate(doc: &mut Document, op: BodyOp) -> (FeatureId, FeatureId) {
    let b = doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op,
        link: None,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([15.0, 15.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let mut s = Sketch::default();
    for x in [-10.0, 10.0] {
        let p = s.add_point(x, 0.0);
        s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Point { point: p }, construction: false });
        s.next_id += 1;
    }
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: ev.face_ref(top).unwrap() }, offset: 0.0, sketch: s });
    (b, sk)
}

fn bolt(length: f64, origin: P3, x: P3, link: Option<ThreadLink>) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Bolt { size: "M6".into(), length, head: BoltHead::Socket, modeled: true },
        origin,
        z: [0.0, 0.0, 1.0],
        x,
        op: BodyOp::New,
        link,
    })
}

/// Volumen en común entre las piezas creadas por `a` y por `b`.
fn overlap(ev: &Evaluation, a: FeatureId, b: FeatureId) -> f64 {
    let part = |f: FeatureId| &ev.parts.iter().find(|p| p.id.feature == f).unwrap().shape;
    let common = part(a).intersect(part(b)).expect("intersección");
    // Sin nada en común OCCT da un compuesto vacío (sin masa)
    common.mass().map_or(0.0, |m| m.volume)
}

#[test]
fn bolt_follows_a_threaded_hole() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let (p, sk) = plate(&mut doc, BodyOp::Join);
    let t = ThreadSpec { nominal: 6.0, pitch: 1.0, clearance: 0.2, left: false };
    let h = doc.add(FeatureKind::Hole(Hole {
        sketch: sk,
        points: vec![],
        diameter: 5.0,
        depth: HoleDepth::ThroughAll,
        style: HoleStyle::Simple,
        tip_angle: 0.0,
        thread: None,
        modeled: Some(t),
        link: None,
        reverse: None,
    }));
    // Un M8 cualquiera en el origen: el vínculo lo lleva al agujero de x = 10
    // como M6, con la cabeza medio milímetro afuera
    let link = ThreadLink { feature: h, index: 1, flip: false, offset: -0.5 };
    let mut shape = bolt(8.0, [0.0; 3], [1.0, 0.0, 0.0], Some(link));
    if let FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Bolt { size, .. }, .. }) = &mut shape {
        *size = "M8".into();
    }
    let b = doc.add(shape);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let bt = ev.threads.iter().find(|t| t.feature == b).unwrap();
    let [o, z, x] = bt.placed.unwrap();
    assert!(geom::norm(geom::sub(o, [10.0, 0.0, 10.5])) < 1e-6, "{o:?}");
    assert!(geom::norm(geom::sub(z, [0.0, 0.0, 1.0])) < 1e-9);
    assert_eq!(bt.spec.nominal, 6.0);
    assert_eq!(bt.link, Some(link));
    let fit = overlap(&ev, p, b);

    // El mismo tornillo suelto en el mismo lugar con el filete medio paso corrido
    let ht = ev.threads.iter().find(|t| t.feature == h && t.index == 1).unwrap().clone();
    let mut doc2 = doc.clone();
    let half = [-x[0], -x[1], -x[2]];
    *doc2.get_mut(b).unwrap() = Feature { kind: bolt(8.0, o, half, None), ..doc.get(b).unwrap().clone() };
    let ev2 = doc2.evaluate();
    assert!(ev2.errors().is_empty(), "{:?}", ev2.errors());
    let clash = overlap(&ev2, p, b);
    eprintln!("en común: alineado {fit:.4} mm³, corrido {clash:.4} mm³");
    assert!(fit < 1e-3, "alineado choca {fit}");
    assert!(clash > 5.0, "corrido medio paso tendría que chocar: {clash}");
    // La rosca del agujero no cambió
    assert_eq!(ht.spec, t);
}

#[test]
fn thread_in_a_hole_follows_an_existing_bolt() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    // Primero el tornillo, después la placa con un agujero liso a su alrededor
    let b = doc.add(bolt(12.0, [10.0, 0.0, 10.5], [0.3, 0.8, 0.0], None));
    let (p, sk) = plate(&mut doc, BodyOp::New);
    let d1 = 6.0 - 1.082_532;
    let hid = doc.add(FeatureKind::Hole(Hole {
        sketch: sk,
        points: vec![],
        diameter: d1,
        depth: HoleDepth::ThroughAll,
        style: HoleStyle::Simple,
        tip_angle: 0.0,
        thread: None,
        modeled: None,
        link: None,
        reverse: None,
    }));
    doc.get_mut(hid).unwrap().scope = vec![PartId { feature: p, index: 0 }];
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // Con el tornillo como otra pieza el agujero igual ve el material de la placa
    assert_eq!(ev.status.iter().find(|s| s.id == hid).unwrap().state, FeatureState::Ok);
    let body = ev.body.as_ref().unwrap();
    let (face, _) = body.closest_face([10.0 + d1 / 2.0, 0.0, 5.0], Some([-1.0, 0.0, 0.0]), 0.9).unwrap();
    let face = ev.face_ref(face).unwrap();
    // La rosca toma paso y diámetro del tornillo (los suyos no importan)
    let link = ThreadLink { feature: b, index: 0, flip: false, offset: 0.0 };
    let th = doc.add(FeatureKind::Thread { face, pitch: 3.0, length: 0.0, flip: false, left: true, clearance: 0.2, link: Some(link) });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let t = ev.threads.iter().find(|t| t.feature == th).unwrap();
    assert_eq!((t.spec.nominal, t.spec.pitch, t.spec.left, t.internal), (6.0, 1.0, false, true));
    let fit = overlap(&ev, p, b);
    eprintln!("en común con la rosca coordinada: {fit:.4} mm³");
    assert!(fit < 1e-3, "choca {fit}");

    // Sin vínculo (paso 1, fase propia) choca
    if let FeatureKind::Thread { link, pitch, left, .. } = &mut doc.get_mut(th).unwrap().kind {
        *link = None;
        *pitch = 1.0;
        *left = false;
    }
    let ev = doc.evaluate();
    let clash = overlap(&ev, p, b);
    eprintln!("sin coordinar: {clash:.4} mm³");
    assert!(clash > fit + 0.05, "{clash}");
}

#[test]
fn link_errors() {
    if !occt() {
        return;
    }
    // Tornillo asociado a otro tornillo: no es una rosca interior
    let mut doc = Document::new();
    let a = doc.add(bolt(10.0, [0.0; 3], [1.0, 0.0, 0.0], None));
    let b = doc.add(bolt(10.0, [20.0, 0.0, 0.0], [1.0, 0.0, 0.0], Some(ThreadLink { feature: a, index: 0, flip: false, offset: 0.0 })));
    let ev = doc.evaluate();
    let e = ev.errors();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].0, b);
    assert!(e[0].1.contains("interior"), "{}", e[0].1);
    // Depende de la rosca: no se puede borrar
    assert!(doc.remove(a).is_err());
    assert_eq!(doc.dependents(a), vec![b]);
}
