//! Relieve y grabado sobre la piel: un círculo de radio 5 sobre una esfera
//! de radio 20, en las cuatro formas de altura, hacia afuera y hacia adentro.

use approx::assert_relative_eq;
use cad_model::occt::{self, Shape};
use cad_model::*;
use cad_scan::*;
use std::f64::consts::PI;

const R: f64 = 20.0;
const RHO: f64 = 5.0;

/// ∫ sobre el disco de radio `RHO` de √(big² − ρ²)
fn cap(big: f64) -> f64 {
    2.0 * PI / 3.0 * (big.powi(3) - (big * big - RHO * RHO).powf(1.5))
}

fn area() -> f64 {
    PI * RHO * RHO
}

fn skin(offset: f64, thickness: f64, hash: u64) -> ScanBodyMesh {
    let t = Shape::sphere([0.0; 3], R).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let b = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset, thickness }).unwrap();
    ScanBodyMesh { hash, data: std::sync::Arc::new((b.vertices, b.triangles)), error: None }
}

/// Volumen del relieve con el círculo en el plano z = `z`
fn relief(z: f64, mode: ReliefMode, inward: bool, height: f64) -> (f64, Evaluation) {
    let mut sketch = Sketch::default();
    sketch.circle([0.0, 0.0], RHO);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: z, sketch });
    let shell = (mode == ReliefMode::Even).then(|| skin(0.0, if inward { -height } else { height }, 2));
    doc.add(FeatureKind::Relief(Relief {
        sketch: sk,
        regions: RegionSelection::All,
        map: ReliefMap::Project,
        target: ReliefTarget::Scan,
        height,
        mode,
        inward,
        op: BodyOp::New,
        faces: 20000,
        mesh: Some(skin(0.0, 0.0, 1)),
        shell,
        skin: None,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{mode:?} adentro={inward}: {:?}", ev.errors());
    let v = ev.body.as_ref().unwrap().mass().unwrap().volume;
    (v, ev)
}

#[test]
fn relief_four_heights_outward() {
    if !occt::available() {
        return;
    }
    // Pareja: la carcasa de 20 a 22 bajo el disco
    let (v, ev) = relief(30.0, ReliefMode::Even, false, 2.0);
    assert_relative_eq!(v, cap(22.0) - cap(20.0), max_relative = 0.04);
    // Las dos medidas: en el plano 10 × 10; sobre la piel, algo más
    let k = ev.status[1].note.clone().unwrap_or_default();
    assert!(k.starts_with("en el plano 10,0 × 10,0 mm · sobre la piel 10,"), "{k}");
    // Tope plano en el plano del sketch (z = 30)
    let (v, _) = relief(30.0, ReliefMode::FlatTop, false, 0.0);
    assert_relative_eq!(v, area() * 30.0 - cap(R), max_relative = 0.03);
    // Tope plano automático: 2 sobre el punto más alto (z = 22)
    let (v, _) = relief(30.0, ReliefMode::FlatAuto, false, 2.0);
    assert_relative_eq!(v, area() * 22.0 - cap(R), max_relative = 0.04);
    // Según la dirección: la piel subida 2 → 2 × área
    let (v, _) = relief(30.0, ReliefMode::Along, false, 2.0);
    assert_relative_eq!(v, 2.0 * area(), max_relative = 0.04);
}

#[test]
fn relief_four_heights_inward() {
    if !occt::available() {
        return;
    }
    let (v, _) = relief(30.0, ReliefMode::Even, true, 2.0);
    assert_relative_eq!(v, cap(R) - cap(18.0), max_relative = 0.05);
    // Fondo a 2 bajo el punto más hondo de la piel bajo el disco
    let deepest = (R * R - RHO * RHO).sqrt();
    let (v, _) = relief(30.0, ReliefMode::FlatAuto, true, 2.0);
    assert_relative_eq!(v, cap(R) - area() * (deepest - 2.0), max_relative = 0.04);
    let (v, _) = relief(30.0, ReliefMode::Along, true, 2.0);
    assert_relative_eq!(v, 2.0 * area(), max_relative = 0.04);
    // Fondo plano en el plano del sketch, adentro (z = 15): de ahí a la piel
    let (v, _) = relief(15.0, ReliefMode::FlatTop, true, 0.0);
    assert_relative_eq!(v, cap(R) - area() * 15.0, max_relative = 0.04);
}

/// Una barra de 20 × 4 mm sobre la esfera: proyectada se estira sobre la piel
/// (2R·asin(10/R) ≈ 20,9 mm); envuelta mide 20 mm sobre la piel.
#[test]
fn wrap_keeps_lengths_on_the_skin() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], R).unwrap().tessellate(0.01, 0.03).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let raw = ScanBodyMesh { hash: 9, data: std::sync::Arc::new((scan.vertices.clone(), scan.triangles.clone())), error: None };
    let run = |map: ReliefMap| {
        let mut sketch = Sketch::default();
        sketch.rectangle([-10.0, -2.0], [10.0, 2.0]);
        let mut doc = Document::new();
        let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch });
        doc.add(FeatureKind::Relief(Relief {
            sketch: sk,
            regions: RegionSelection::All,
            map,
            target: ReliefTarget::Scan,
            height: 1.0,
            mode: ReliefMode::Along,
            inward: false,
            op: BodyOp::New,
            faces: 20000,
            mesh: Some(skin(0.0, 0.0, 1)),
            shell: None,
            skin: Some(raw.clone()),
        }));
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{:?}", ev.errors());
        ev.status[1].note.clone().unwrap_or_default()
    };
    let projected = run(ReliefMap::Project);
    assert!(projected.contains("sobre la piel 20,9") || projected.contains("sobre la piel 21,0"), "{projected}");
    let wrapped = run(ReliefMap::Wrap { anchor: None, angle: 0.0 });
    assert!(wrapped.contains("sobre la piel 20,0") || wrapped.contains("sobre la piel 19,9") || wrapped.contains("sobre la piel 20,1"), "{wrapped}");
    assert!(wrapped.contains("envuelto"), "{wrapped}");
}
