//! Patrones por tabla y de relleno.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn rect_sketch(x0: f64, y0: f64, x1: f64, y1: f64) -> Sketch {
    let mut s = Sketch::default();
    let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

fn volume(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

/// Placa de 100 × 60 × 2 con un agujero de Ø4 en (10, 10); devuelve el documento y el agujero
fn plate_with_hole() -> (Document, FeatureId) {
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: rect_sketch(0.0, 0.0, 100.0, 60.0) });
    doc.add(FeatureKind::Extrude(Extrude { sketch: sk, regions: RegionSelection::All, extent: Extent::Blind { distance: 2.0 }, reverse: false, op: BodyOp::Join, draft: 0.0, thin: None }));
    let mut c = Sketch::default();
    let o = c.add_point(10.0, 10.0);
    c.add_entity(Geometry::Circle { center: o, radius: 2.0 });
    let hs = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: c });
    let hole = doc.add(FeatureKind::Extrude(Extrude { sketch: hs, regions: RegionSelection::All, extent: Extent::ThroughAll, reverse: false, op: BodyOp::Cut, draft: 0.0, thin: None }));
    (doc, hole)
}

#[test]
fn table_pattern_copies_by_offsets() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    doc.add(FeatureKind::Pattern { features: vec![hole], pattern: PatternKind::Table { offsets: vec![[30.0, 0.0, 0.0], [0.0, 25.0, 0.0], [70.0, 40.0, 0.0]] } });
    assert_relative_eq!(volume(&doc), 12000.0 - 4.0 * PI * 4.0 * 2.0, max_relative = 1e-6);
}

#[test]
fn fill_pattern_square_and_hex() {
    if !occt() {
        return;
    }
    for (hex, holes) in [(false, 45.0), (true, 43.0)] {
        let (mut doc, hole) = plate_with_hole();
        // Región de 5 a 95 por 5 a 55, a 3 del borde: centros en [8, 92] × [8, 52]
        let region = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: rect_sketch(5.0, 5.0, 95.0, 55.0) });
        doc.add(FeatureKind::Pattern {
            features: vec![hole],
            pattern: PatternKind::Fill { sketch: region, regions: RegionSelection::All, spacing: 10.0, hex, margin: 3.0 },
        });
        assert_relative_eq!(volume(&doc), 12000.0 - holes * PI * 4.0 * 2.0, max_relative = 1e-6);
    }
}
