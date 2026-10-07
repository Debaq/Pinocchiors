//! Referencias por origen: caras y aristas que se siguen encontrando cuando
//! cambian las medidas, también donde la geometría sola se confunde.

use approx::assert_relative_eq;
use cad_model::occt::SurfaceKind;
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn caja(dz: f64) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    })
}

fn set_box_height(doc: &mut Document, id: FeatureId, dz: f64) {
    if let FeatureKind::Primitive(p) = &mut doc.get_mut(id).unwrap().kind
        && let PrimitiveShape::Box { dz: h, .. } = &mut p.shape
    {
        *h = dz;
    }
}

/// Altura (z) del eje del redondeo: dice qué arista se redondeó.
fn fillet_axis_z(ev: &Evaluation) -> f64 {
    let body = ev.body.as_ref().unwrap();
    let f = body.faces().unwrap().into_iter().find(|f| f.surface == SurfaceKind::Cylinder).expect("sin redondeo");
    f.axis.unwrap().origin[2]
}

/// Arista paralela a X en y=0 a la altura z.
fn front_edge(ev: &Evaluation, z: f64) -> EdgeRef {
    let body = ev.body.as_ref().unwrap();
    let e = (0..body.edge_count())
        .find(|&e| {
            let i = body.edge_info(e).unwrap();
            i.tangent[0].abs() > 0.99 && i.mid[1].abs() < 1e-9 && (i.mid[2] - z).abs() < 1e-9
        })
        .unwrap();
    ev.edge_ref(e).unwrap()
}

#[test]
fn fillet_follows_top_edge_when_height_doubles() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let b = doc.add(caja(10.0));
    let ev = doc.evaluate();
    let edge = front_edge(&ev, 10.0);
    assert_eq!(edge.sides.len(), 2, "la arista lleva el origen de sus caras");
    doc.add(FeatureKind::Fillet { edges: vec![edge.clone()], radius: 1.0 });
    assert_relative_eq!(fillet_axis_z(&doc.evaluate()), 9.0, epsilon = 1e-9);

    // Con el doble de alto, la arista de arriba y la de abajo quedan a la misma
    // distancia del punto guardado (z = 10): por geometría es un empate
    set_box_height(&mut doc, b, 20.0);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_relative_eq!(fillet_axis_z(&ev), 19.0, epsilon = 1e-9);

    // Sin orígenes (referencia vieja) se elige la de abajo: el caso que se arregla
    if let FeatureKind::Fillet { edges, .. } = &mut doc.features[1].kind {
        edges[0].sides.clear();
    }
    let ev = doc.evaluate();
    assert!((fillet_axis_z(&ev) - 19.0).abs() > 1.0, "la geometría sola elegía otra arista");
}

#[test]
fn origin_survives_booleans_that_split_faces() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let b = doc.add(caja(10.0));
    // Ranura que parte la cara superior en dos
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 2.0, dy: 12.0, dz: 4.0, centered: false, centered_z: false },
        origin: [4.0, -1.0, 8.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Cut,
    }));
    let ev = doc.evaluate();
    // Las dos mitades de la tapa conservan el origen "+z" de la caja
    let tops: Vec<usize> = (0..ev.face_tags.len())
        .filter(|&f| ev.face_tags[f].iter().any(|t| t.feature == b && t.name == "+z"))
        .collect();
    assert_eq!(tops.len(), 2);
    // Arista superior frontal de la mitad derecha (x 6..10)
    let body = ev.body.as_ref().unwrap();
    let e = (0..body.edge_count())
        .find(|&e| {
            let i = body.edge_info(e).unwrap();
            i.tangent[0].abs() > 0.99 && i.mid[1].abs() < 1e-9 && (i.mid[2] - 10.0).abs() < 1e-9 && i.mid[0] > 6.0
        })
        .unwrap();
    let r = ev.edge_ref(e).unwrap();
    doc.add(FeatureKind::Fillet { edges: vec![r], radius: 1.0 });
    // Más alta pero con la ranura (z 8..12) todavía partiendo la tapa
    set_box_height(&mut doc, b, 11.0);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let body = ev.body.as_ref().unwrap();
    let cyl = body.faces().unwrap().into_iter().find(|f| f.surface == SurfaceKind::Cylinder).unwrap();
    assert_relative_eq!(cyl.axis.unwrap().origin[2], 10.0, epsilon = 1e-9);
    assert!(cyl.center[0] > 6.0, "sigue en la mitad derecha");
}

#[test]
fn extruded_faces_are_named_by_sketch_entity() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let mut s = Sketch::new();
    let lines = s.rectangle([0.0, 0.0], [40.0, 20.0]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let ext = doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 10.0 },
        reverse: false,
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    let name = |f: usize| ev.face_tags[f].iter().map(|t| t.name.clone()).collect::<Vec<_>>();
    let body = ev.body.as_ref().unwrap();
    let mut names: Vec<String> = (0..body.face_count()).flat_map(name).collect();
    names.sort();
    let mut expected: Vec<String> = lines.iter().map(|l| format!("lado:{l}")).collect();
    expected.extend(["fin".to_string(), "inicio".to_string()]);
    expected.sort();
    assert_eq!(names, expected);
    assert!(ev.face_tags.iter().flatten().all(|t| t.feature == ext));

    // Sketch sobre la tapa ("fin") y ranura pasante; después la extrusión crece
    let top = (0..body.face_count()).find(|&f| name(f) == ["fin"]).unwrap();
    let top_ref = ev.face_ref(top).unwrap();
    let mut s2 = Sketch::new();
    s2.circle([20.0, 10.0], 3.0);
    let sk2 = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: top_ref }, offset: 0.0, sketch: s2 });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk2,
        regions: RegionSelection::All,
        extent: Extent::Blind { distance: 4.0 },
        reverse: false,
        op: BodyOp::Join,
    }));
    if let FeatureKind::Extrude(e) = &mut doc.get_mut(ext).unwrap().kind {
        e.extent = Extent::Blind { distance: 50.0 };
    }
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // El tetón sigue a la tapa: arranca en z = 50, no en la vieja z = 10
    assert_relative_eq!(ev.sketches[&sk2].plane.origin[2], 50.0, epsilon = 1e-9);
    assert_relative_eq!(ev.body.as_ref().unwrap().mass().unwrap().bbox_max[2], 54.0, epsilon = 1e-6);
}

#[test]
fn pattern_copies_get_their_own_names() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let pin = doc.add(caja(5.0));
    doc.add(FeatureKind::Pattern {
        features: vec![pin],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 3, spacing: 20.0 },
    });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let all: Vec<String> = ev.face_tags.iter().flatten().map(|t| t.name.clone()).collect();
    for n in ["+z", "+z#1", "+z#2"] {
        assert!(all.contains(&n.to_string()), "falta {n}: {all:?}");
    }
}

#[test]
fn refs_with_origins_roundtrip_json() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(caja(10.0));
    let ev = doc.evaluate();
    let edge = front_edge(&ev, 10.0);
    doc.add(FeatureKind::Fillet { edges: vec![edge], radius: 1.0 });
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("\"sides\""));
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
    // Documentos viejos sin orígenes siguen abriendo
    let old = json.replace("\"sides\"", "\"sin_uso\"");
    let back: Document = serde_json::from_str(&old).unwrap();
    assert!(back.evaluate().errors().is_empty());
}

fn caja_en(x: f64) -> FeatureKind {
    let FeatureKind::Primitive(mut p) = caja(10.0) else { unreachable!() };
    p.origin = [x, 0.0, 0.0];
    FeatureKind::Primitive(p)
}

/// Arista paralela a X más cercana al punto.
fn edge_near(ev: &Evaluation, p: [f64; 3]) -> EdgeRef {
    let (e, _) = ev.body.as_ref().unwrap().closest_edge(p, Some([1.0, 0.0, 0.0]), 0.9).unwrap();
    ev.edge_ref(e).unwrap()
}

#[test]
fn lost_edges_are_reported_and_the_rest_still_applies() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(caja_en(0.0));
    let lejos = doc.add(caja_en(100.0));
    let ev = doc.evaluate();
    let a = edge_near(&ev, [5.0, 0.0, 10.0]);
    let b = edge_near(&ev, [105.0, 0.0, 10.0]);
    let fillet = doc.add(FeatureKind::Fillet { edges: vec![a.clone(), b], radius: 1.0 });
    assert_eq!(doc.evaluate().state(fillet), Some(&FeatureState::Ok));

    // Sin la caja lejana, su arista ya no está: advertencia y se redondea la otra
    doc.get_mut(lejos).unwrap().suppressed = true;
    let ev = doc.evaluate();
    let Some(FeatureState::Warning { missing, message }) = ev.state(fillet) else {
        panic!("se esperaba advertencia: {:?}", ev.state(fillet));
    };
    assert_eq!(missing, &vec![MissingRef { field: "edges".into(), index: 1 }]);
    assert!(message.contains("1 de 2"), "{message}");
    assert_relative_eq!(fillet_axis_z(&ev), 9.0, epsilon = 1e-9);
    assert!(ev.errors().is_empty(), "una advertencia no es error");

    // Si no queda ninguna, falla y dice cuál
    if let FeatureKind::Fillet { edges, .. } = &mut doc.get_mut(fillet).unwrap().kind {
        edges.remove(0);
    }
    let ev = doc.evaluate();
    let Some(FeatureState::Error { missing, .. }) = ev.state(fillet) else {
        panic!("se esperaba error: {:?}", ev.state(fillet));
    };
    assert_eq!(missing, &vec![MissingRef { field: "edges".into(), index: 0 }]);

    // Lo guardado se vuelve a leer igual
    let json = serde_json::to_string(&ev.status).unwrap();
    let back: Vec<FeatureStatus> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ev.status);
    let _ = a;
}

#[test]
fn lost_sketch_face_marks_the_plane() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(caja_en(0.0));
    let lejos = doc.add(caja_en(100.0));
    let ev = doc.evaluate();
    let (f, _) = ev.body.as_ref().unwrap().closest_face([110.0, 5.0, 5.0], Some([1.0, 0.0, 0.0]), 0.9).unwrap();
    let face = ev.face_ref(f).unwrap();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face }, offset: 0.0, sketch: Sketch::default() });
    assert_eq!(doc.evaluate().state(sk), Some(&FeatureState::Ok));
    doc.get_mut(lejos).unwrap().suppressed = true;
    let ev = doc.evaluate();
    let Some(FeatureState::Error { missing, .. }) = ev.state(sk) else { panic!("{:?}", ev.state(sk)) };
    assert_eq!(missing, &vec![MissingRef { field: "plane".into(), index: 0 }]);
}
