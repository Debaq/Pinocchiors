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
