//! Validación según la operación: la revolución con el perfil a los dos lados
//! del eje, el barrido con un camino con ramas y la transición con secciones
//! de varias regiones avisan qué pasa en vez de dar un sólido roto.

use cad_model::*;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Sketch {
    let mut s = Sketch::default();
    let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

fn error_of(doc: &Document, id: FeatureId) -> Option<String> {
    doc.evaluate().errors().into_iter().find(|(f, _)| *f == id).map(|(_, m)| m)
}

#[test]
fn revolve_profile_across_the_axis() {
    if !occt::available() {
        return;
    }
    // En el frente (x, z): rectángulo de x −2..5 alrededor del eje Z
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: rect(-2.0, 0.0, 5.0, 10.0) });
    let rv = doc.add(FeatureKind::Revolve(Revolve { sketch: sk, regions: RegionSelection::All, axis: AxisSpec::Z, angle: 360.0, op: BodyOp::Join }));
    let e = error_of(&doc, rv).expect("tiene que avisar");
    assert!(e.contains("cruza el eje"), "{e}");
    // Tocando el eje (x 0..5) está bien
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: rect(0.0, 0.0, 5.0, 10.0) });
    let rv = doc.add(FeatureKind::Revolve(Revolve { sketch: sk, regions: RegionSelection::All, axis: AxisSpec::Z, angle: 360.0, op: BodyOp::Join }));
    assert_eq!(error_of(&doc, rv), None);
}

#[test]
fn sweep_path_with_branches() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let mut prof = Sketch::default();
    prof.circle([0.0, 0.0], 1.0);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: prof });
    // Una T: tres tramos llegan a (0, 10)
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 10.0);
    let c = path.add_point(-5.0, 10.0);
    let d = path.add_point(5.0, 10.0);
    path.add_line(a, b);
    path.add_line(b, c);
    path.add_line(b, d);
    let ps = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path });
    let sw = doc.add(FeatureKind::Sweep(Sweep { sketch: p, regions: RegionSelection::All, path: SweepPath::Sketch { sketch: ps, entities: vec![] }, op: BodyOp::Join }));
    let e = error_of(&doc, sw).expect("tiene que avisar");
    assert!(e.contains("ramas") && e.contains("3 tramos"), "{e}");
}

#[test]
fn loft_section_with_two_regions() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let mut two = rect(0.0, 0.0, 4.0, 4.0);
    let q = [[10.0, 0.0], [14.0, 0.0], [14.0, 4.0], [10.0, 4.0]].map(|q: [f64; 2]| two.add_point(q[0], q[1]));
    for k in 0..4 {
        two.add_line(q[k], q[(k + 1) % 4]);
    }
    let a = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: two });
    let b = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 10.0, sketch: rect(0.0, 0.0, 4.0, 4.0) });
    let lf = doc.add(FeatureKind::Loft(Loft {
        sections: vec![LoftSection { sketch: a, regions: RegionSelection::All }, LoftSection { sketch: b, regions: RegionSelection::All }],
        ruled: false,
        op: BodyOp::Join,
    }));
    let e = error_of(&doc, lf).expect("tiene que avisar");
    assert!(e.contains("sección 1") && e.contains("2 regiones"), "{e}");
}

/// Lo de la organización del sketch viaja entero (ida y vuelta por JSON y el solver)
#[test]
fn sketch_organization_round_trip() {
    let json = r#"{"points":[{"id":0,"x":0,"y":0},{"id":1,"x":1,"y":0}],"entities":[{"id":2,"geometry":{"type":"line","start":0,"end":1},"layer":3}],
      "constraints":[],"origin":0,"layers":[{"id":3,"name":"A","hidden":true}],
      "blocks":[{"id":1,"name":"B","points":[{"id":0,"x":0,"y":0}],"entities":[],"constraints":[]}],
      "images":[{"id":1,"name":"i","data":"data:image/png;base64,AA","at":[0,0],"width":10,"height":5}],
      "texts":[{"id":9,"text":"B","size":1,"font":"","anchor":1,"entities":[2],"points":[],"block":1,"style":{"frame":[0,-1,1,0]}}]}"#;
    let mut s: Sketch = serde_json::from_str(json).unwrap();
    s.solve().unwrap();
    let back: serde_json::Value = serde_json::to_value(&s).unwrap();
    assert_eq!(back["texts"][0]["block"], 1);
    assert_eq!(back["texts"][0]["style"]["frame"][1], -1.0);
    assert_eq!(back["entities"][0]["layer"], 3);
    assert_eq!(back["layers"][0]["hidden"], true);
    assert_eq!(back["blocks"][0]["name"], "B");
    assert_eq!(back["images"][0]["opacity"], 1.0);
}
