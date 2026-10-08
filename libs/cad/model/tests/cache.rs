//! Recálculo con caché por operación: tiene que dar exactamente lo mismo que
//! recalcular todo, y recalcular solo desde lo que cambió.

use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn prim(shape: PrimitiveShape, origin: P3, op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive { shape, origin, z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op, link: None })
}

/// Placa con agujero, caja encima, agujero pasante, patrón del agujero,
/// redondeo, chaflán y simetría: un poco de todo.
fn sample() -> Document {
    let mut doc = Document::new();
    let mut s = Sketch::default();
    let p = [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s.circle([15.0, 20.0], 5.0);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 8.0 },
        reverse: false,
        op: BodyOp::Join,
        draft: 0.0,
        thin: None,
    }));
    doc.add(prim(PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 10.0, centered: false, centered_z: false }, [35.0, 10.0, 8.0], BodyOp::Join));
    let hole = doc.add(prim(PrimitiveShape::Cylinder { radius: 2.0, height: 40.0 }, [45.0, 15.0, -5.0], BodyOp::Cut));
    doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear { direction: [0.0, 1.0, 0.0], count: 2, spacing: 10.0 },
    });
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    // Arista de arriba del frente de la placa (y = 0, z = 8) y la vertical de la esquina
    let (top, _) = body.closest_edge([10.0, 0.0, 8.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    let (corner, _) = body.closest_edge([0.0, 40.0, 4.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let (top, corner) = (ev.edge_ref(top).unwrap(), ev.edge_ref(corner).unwrap());
    doc.add(FeatureKind::Fillet { edges: vec![top], radius: 1.5, radius2: None });
    doc.add(FeatureKind::Chamfer { edges: vec![corner], distance: 2.0, second: None });
    doc.add(FeatureKind::Mirror { features: vec![], plane: PlaneSpec::Yz });
    doc
}

/// Mismo resultado: estados, volumen y orígenes de caras.
fn assert_same(a: &Evaluation, b: &Evaluation, what: &str) {
    let states = |e: &Evaluation| e.status.iter().map(|s| (s.id, s.state.clone())).collect::<Vec<_>>();
    assert_eq!(states(a), states(b), "{what}: estados");
    let vol = |e: &Evaluation| e.body.as_ref().map(|b| b.mass().unwrap().volume);
    assert_eq!(vol(a), vol(b), "{what}: volumen");
    assert_eq!(a.face_tags, b.face_tags, "{what}: orígenes de caras");
    assert_eq!(a.sketches.len(), b.sketches.len(), "{what}: sketches");
}

/// Cambia el primer número "de medida" de la operación.
fn bump(kind: &mut FeatureKind) -> bool {
    let mut v = serde_json::to_value(&*kind).unwrap();
    fn walk(v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(m) => {
                for key in ["distance", "radius", "dx", "spacing", "offset"] {
                    if let Some(x) = m.get_mut(key).and_then(|x| x.as_f64()) {
                        m.insert(key.into(), serde_json::json!(x + 0.5));
                        return true;
                    }
                }
                m.values_mut().any(walk)
            }
            serde_json::Value::Array(a) => a.iter_mut().any(walk),
            _ => false,
        }
    }
    let changed = walk(&mut v);
    *kind = serde_json::from_value(v).unwrap();
    changed
}

#[test]
fn cached_matches_full_after_changing_each_feature() {
    if !occt() {
        return;
    }
    let base = sample();
    let mut cache = EvalCache::default();
    let first = base.evaluate_with(&mut cache);
    assert!(first.errors().is_empty(), "{:?}", first.errors());
    assert_eq!(first.recomputed, base.features.len());
    // Lo mismo otra vez: nada que recalcular
    let again = base.evaluate_with(&mut cache);
    assert_eq!(again.recomputed, 0);
    assert_same(&again, &first, "sin cambios");

    let n = base.features.len();
    for i in 0..n {
        let mut doc = base.clone();
        if !bump(&mut doc.features[i].kind) {
            continue;
        }
        let cached = doc.evaluate_with(&mut cache);
        let full = doc.evaluate();
        assert_same(&cached, &full, &format!("cambio en la operación {i}"));
        // Solo desde la que cambió (las suprimidas y retrocedidas no cuentan)
        assert_eq!(cached.recomputed, n - i, "cambio en la operación {i}");
    }
    // Suprimir, retroceder y volver: la caché sigue sirviendo
    let mut doc = base.clone();
    doc.features[3].suppressed = true;
    assert_same(&doc.evaluate_with(&mut cache), &doc.evaluate(), "suprimida");
    doc.features[3].suppressed = false;
    doc.rollback = Some(4);
    assert_same(&doc.evaluate_with(&mut cache), &doc.evaluate(), "retroceso");
    doc.rollback = None;
    let back = doc.evaluate_with(&mut cache);
    assert_eq!(back.recomputed, 0, "volver al documento original sale entero de la caché");
    assert_same(&back, &first, "vuelta");
}

#[test]
fn renaming_does_not_recalculate_and_capacity_is_kept() {
    if !occt() {
        return;
    }
    let mut doc = sample();
    let mut cache = EvalCache::default();
    cache.capacity = 6;
    doc.evaluate_with(&mut cache);
    assert!(cache.len() <= 6, "{}", cache.len());
    let mut cache = EvalCache::default();
    doc.evaluate_with(&mut cache);
    doc.features[2].name = "Otra".into();
    assert_eq!(doc.evaluate_with(&mut cache).recomputed, 0);
}

/// Banco del plan: 30 operaciones (placa, 27 agujeros, redondeo y chaflán).
/// `cargo test --release -p cad-model --test cache bench -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_thirty_operations() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 200.0, dy: 200.0, dz: 20.0, centered: false, centered_z: false }, [0.0; 3], BodyOp::Join));
    for k in 0..27 {
        let (x, y) = (20.0 + 20.0 * (k % 9) as f64, 40.0 + 50.0 * (k / 9) as f64);
        doc.add(prim(PrimitiveShape::Cylinder { radius: 4.0, height: 40.0 }, [x, y, -10.0], BodyOp::Cut));
    }
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let (e1, _) = body.closest_edge([100.0, 0.0, 20.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    let (e2, _) = body.closest_edge([100.0, 200.0, 20.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    doc.add(FeatureKind::Fillet { edges: vec![ev.edge_ref(e1).unwrap()], radius: 3.0, radius2: None });
    doc.add(FeatureKind::Chamfer { edges: vec![ev.edge_ref(e2).unwrap()], distance: 2.0, second: None });
    assert_eq!(doc.features.len(), 30);

    let t = std::time::Instant::now();
    let full = doc.evaluate();
    let full_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(full.errors().is_empty(), "{:?}", full.errors());

    let mut cache = EvalCache::default();
    doc.evaluate_with(&mut cache);
    let mut times = Vec::new();
    for d in [2.5, 3.0, 1.5] {
        if let FeatureKind::Chamfer { distance, .. } = &mut doc.features[29].kind {
            *distance = d;
        }
        let t = std::time::Instant::now();
        let ev = doc.evaluate_with(&mut cache);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(ev.recomputed, 1);
    }
    // Teselar como el visor (0,05 mm, 0,25 rad): la primera vez y tras un cambio al final
    let body = doc.evaluate_with(&mut cache).body.unwrap();
    let t = std::time::Instant::now();
    let mesh = body.tessellate(0.05, 0.25).unwrap();
    let mesh_ms = t.elapsed().as_secs_f64() * 1000.0;
    if let FeatureKind::Chamfer { distance, .. } = &mut doc.features[29].kind {
        *distance = 2.2;
    }
    let body2 = doc.evaluate_with(&mut cache).body.unwrap();
    let t = std::time::Instant::now();
    body2.tessellate(0.05, 0.25).unwrap();
    let remesh_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    body2.tessellate(0.05, 0.25).unwrap();
    println!("teselar otra vez el mismo cuerpo: {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    println!("teselar: {mesh_ms:.0} ms ({} triángulos); tras cambiar el chaflán {remesh_ms:.0} ms", mesh.triangles.len());
    let slowest = full.status.iter().map(|s| s.ms).fold(0.0, f64::max);
    println!("30 operaciones: todo {full_ms:.0} ms; cambiar el chaflán con caché {:?} ms; la más lenta {slowest:.0} ms", times.iter().map(|t| t.round()).collect::<Vec<_>>());
}
