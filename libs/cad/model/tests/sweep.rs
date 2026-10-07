//! Barrido y transición.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn square(size: f64) -> Sketch {
    let mut s = Sketch::default();
    let h = size / 2.0;
    let p = [[-h, -h], [h, -h], [h, h], [-h, h]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

#[test]
fn straight_sweep_is_an_extrusion_and_l_path_adds_up() {
    if !occt() {
        return;
    }
    // Círculo de radio 2 en la planta; camino: recta de 20 hacia arriba (en el
    // plano XZ las coordenadas del sketch son x, z)
    let mut doc = Document::new();
    let mut prof = Sketch::default();
    prof.circle([0.0, 0.0], 2.0);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: prof });
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    path.add_line(a, b);
    let c = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: path.clone() });
    let sw = doc.add(FeatureKind::Sweep(Sweep {
        sketch: p,
        regions: RegionSelection::All,
        path: SweepPath::Sketch { sketch: c, entities: vec![] },
        op: BodyOp::Join,
    }));
    assert_relative_eq!(vol(&doc), PI * 4.0 * 20.0, max_relative = 1e-6);
    // Camino en L (arriba 20, luego 10 de costado), con las entidades al revés: se encadenan solas
    let mut path = Sketch::default();
    let a = path.add_point(0.0, 0.0);
    let b = path.add_point(0.0, 20.0);
    let c2 = path.add_point(10.0, 20.0);
    path.add_line(c2, b);
    path.add_line(a, b);
    let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(c).unwrap().kind else { unreachable!() };
    *sketch = path;
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!(ev.body.as_ref().unwrap().is_valid(), "esquina a inglete: sólido válido");
    let v = ev.body.unwrap().mass().unwrap().volume;
    // Entre las dos rectas (30 de largo) menos lo que se pisa en la esquina
    assert!(v > PI * 4.0 * 25.0 && v < PI * 4.0 * 30.0, "{v}");
    assert!(doc.get(sw).unwrap().kind.dependencies().contains(&c));
}

#[test]
fn loft_between_squares() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let a = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: square(10.0) });
    let b = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 10.0, sketch: square(10.0) });
    let l = doc.add(FeatureKind::Loft(Loft {
        sections: vec![LoftSection { sketch: a, regions: RegionSelection::All }, LoftSection { sketch: b, regions: RegionSelection::All }],
        ruled: true,
        op: BodyOp::Join,
    }));
    assert_relative_eq!(vol(&doc), 1000.0, max_relative = 1e-6);
    // De 10 a 6 reglada: tronco de pirámide
    let FeatureKind::Sketch { sketch, .. } = &mut doc.get_mut(b).unwrap().kind else { unreachable!() };
    *sketch = square(6.0);
    assert_relative_eq!(vol(&doc), 10.0 / 3.0 * (100.0 + 36.0 + 60.0), max_relative = 1e-6);
    // Con una sola sección: error claro
    if let FeatureKind::Loft(x) = &mut doc.get_mut(l).unwrap().kind {
        x.sections.pop();
    }
    assert!(matches!(doc.evaluate().state(l), Some(FeatureState::Error { .. })));
}

#[test]
fn spring_along_a_helix_and_thicken() {
    if !occt() {
        return;
    }
    // Hélice de radio 10, paso 5, 2 vueltas sobre Z; perfil: círculo de radio 1 en
    // el plano XZ donde arranca la hélice
    let mut doc = Document::new();
    let hx = doc.add(FeatureKind::Helix { axis: AxisSpec::Z, radius: 10.0, pitch: 5.0, turns: 2.0, left: false });
    let ev = doc.evaluate();
    let Some(RefGeom::Curve { points }) = ev.references.get(&hx) else { panic!("sin curva") };
    let start = points[0];
    assert!((start[0].hypot(start[1]) - 10.0).abs() < 1e-6 && start[2].abs() < 1e-6, "{start:?}");
    // Perfil en el plano que contiene el eje y el arranque (normal tangente a la hélice ≈ ⊥ radio)
    let radial = [start[0] / 10.0, start[1] / 10.0, 0.0];
    let tangent = [-radial[1], radial[0], 0.0];
    let plane = Plane { origin: start, normal: tangent, x_dir: radial };
    let mut prof = Sketch::default();
    prof.circle([0.0, 0.0], 1.0);
    let p = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Custom { plane }, offset: 0.0, sketch: prof });
    doc.add(FeatureKind::Sweep(Sweep { sketch: p, regions: RegionSelection::All, path: SweepPath::Curve { feature: hx }, op: BodyOp::Join }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let len = 2.0 * ((2.0 * PI * 10.0f64).powi(2) + 25.0).sqrt();
    let v = ev.body.as_ref().unwrap().mass().unwrap().volume;
    assert!((v - PI * len).abs() < 0.02 * PI * len, "{v} vs {}", PI * len);

    // Engrosar la cara de arriba de una caja 2 mm: suma una placa encima
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([5.0, 5.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    doc.add(FeatureKind::Thicken { faces: vec![ev.face_ref(top).unwrap()], thickness: 2.0, op: BodyOp::Join });
    assert_relative_eq!(vol(&doc), 1200.0, max_relative = 1e-6);
    assert_relative_eq!(doc.evaluate().body.unwrap().mass().unwrap().bbox_max[2], 12.0, epsilon = 1e-6);
}
