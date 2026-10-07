//! Escaneo sintético: una pieza CAD conocida, teselada y con ruido, se
//! reconstruye con las herramientas de escaneo → CAD.

use approx::assert_relative_eq;
use cad_model::occt::{self, Frame, Shape};
use cad_model::*;
use cad_scan::*;
use std::f64::consts::PI;

/// Placa 100×60×20 con agujero pasante r=8 en (30,30) y tetón r=10 h=15 en (70,30).
fn part() -> Shape {
    let plate = Shape::make_box(Frame::WORLD, 100.0, 60.0, 20.0).unwrap();
    let hole = Shape::cylinder(Frame::at([30.0, 30.0, -1.0]), 8.0, 22.0).unwrap();
    let boss = Shape::cylinder(Frame::at([70.0, 30.0, 20.0]), 10.0, 15.0).unwrap();
    plate.cut(&hole).unwrap().union(&boss).unwrap()
}

fn part_volume() -> f64 {
    100.0 * 60.0 * 20.0 - PI * 64.0 * 20.0 + PI * 100.0 * 15.0
}

/// Teselado fino + ruido determinista de ±`amp` mm.
fn scan_of(shape: &Shape, amp: f64) -> ScanMesh {
    let t = shape.tessellate(0.05, 0.1).unwrap();
    let clean = ScanMesh::new(&t.positions, &t.triangles);
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 20001) as f64 / 10000.0 - 1.0
    };
    let noisy: Vec<P3> = clean.vertices.iter().map(|v| [v[0] + amp * rnd(), v[1] + amp * rnd(), v[2] + amp * rnd()]).collect();
    ScanMesh::new(&noisy, &clean.triangles)
}

fn face_where(m: &ScanMesh, f: impl Fn(P3, P3) -> bool) -> u32 {
    (0..m.face_count()).find(|&i| f(m.face_centroids[i], m.face_normals[i]) && m.face_areas[i] > 1e-4).unwrap() as u32
}

#[test]
fn reverse_engineer_plate() {
    if !occt::available() {
        return;
    }
    let mesh = scan_of(&part(), 0.02);
    let opts = PickOptions { tolerance: Some(0.1), ..Default::default() };

    // Cara superior de la placa (lejos del tetón y del agujero)
    let seed = face_where(&mesh, |c, n| n[2] > 0.9 && (c[2] - 20.0).abs() < 0.2 && c[0] < 15.0);
    let top = pick_plane(&mesh, seed, &opts).unwrap();
    assert_relative_eq!(top.plane.normal[2], 1.0, epsilon = 1e-3);
    assert_relative_eq!(top.plane.origin[2], 20.0, epsilon = 0.05);
    assert_relative_eq!(top.area, 6000.0 - PI * 64.0 - PI * 100.0, max_relative = 0.01);
    assert_eq!(top.boundary.len(), 3, "contorno + agujero + base del tetón");
    let depth = top.depth(&mesh).unwrap();
    assert_relative_eq!(depth, 20.0, epsilon = 0.2);

    // Pared del agujero
    let seed = face_where(&mesh, |c, _| {
        let r = ((c[0] - 30.0).powi(2) + (c[1] - 30.0).powi(2)).sqrt();
        (r - 8.0).abs() < 0.1 && (c[2] - 10.0).abs() < 5.0
    });
    let hole = pick_cylinder(&mesh, seed, &opts).unwrap();
    assert!(hole.hole);
    assert_relative_eq!(hole.radius, 8.0, epsilon = 0.05);
    assert_relative_eq!(hole.length, 20.0, epsilon = 0.2);
    assert!(hole.coverage > 0.95);
    assert!(hole.direction[2].abs() > 0.999);

    // Pared del tetón
    let seed = face_where(&mesh, |c, _| {
        let r = ((c[0] - 70.0).powi(2) + (c[1] - 30.0).powi(2)).sqrt();
        (r - 10.0).abs() < 0.1 && c[2] > 25.0
    });
    let boss = pick_cylinder(&mesh, seed, &opts).unwrap();
    assert!(!boss.hole);
    assert_relative_eq!(boss.radius, 10.0, epsilon = 0.05);
    assert_relative_eq!(boss.length, 15.0, epsilon = 0.2);

    // Corte a media altura: rectángulo + círculo
    let mid = Plane::XY.offset(10.0);
    let sections = slice(&mesh, &mid);
    assert_eq!(sections.len(), 2);
    assert!(sections.iter().all(|s| s.closed));
    let loops: Vec<Vec<P2>> = sections.iter().map(|s| s.points.clone()).collect();
    let sk = outline_sketch(&loops, &OutlineOptions { tolerance: 0.15, ..Default::default() });
    let circles: Vec<_> = sk.entities.iter().filter(|e| matches!(e.geometry, Geometry::Circle { .. })).collect();
    assert_eq!(circles.len(), 1);
    assert_relative_eq!(sk.radius(circles[0].id).unwrap(), 8.0, epsilon = 0.05);
    let lines = sk.entities.iter().filter(|e| matches!(e.geometry, Geometry::Line { .. })).count();
    assert_eq!(lines, 4, "el rectángulo con ruido se simplifica a 4 lados");
    assert_eq!(sk.constraints.len(), 4, "y queda enderezado");

    // Reconstrucción: corte → sketch → extrusión simétrica; tetón desde el cilindro
    let mut doc = Document::new();
    let s = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Custom { plane: mid }, offset: 0.0, sketch: sk });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: s,
        regions: RegionSelection::All,
        extent: Extent::Symmetric { distance: depth },
        reverse: false,
        op: BodyOp::Join,
        draft: 0.0,
        thin: None,
    }));
    doc.add(boss.feature());
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let v = ev.body.as_ref().unwrap().mass().unwrap().volume;
    assert_relative_eq!(v, part_volume(), max_relative = 0.01);

    // Alternativa: sketch del contorno de la cara superior extruido hacia adentro
    let mut doc = Document::new();
    let s = doc.add(top.sketch_feature(&OutlineOptions { tolerance: 0.15, ..Default::default() }));
    doc.add(extrude_into(s, depth, BodyOp::Join));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // Sin el tetón, pero la base del tetón queda como agujero del contorno: se
    // extruye el anillo y la isla circular (profundidad par) no; el tetón falta.
    let v = ev.body.as_ref().unwrap().mass().unwrap().volume;
    assert_relative_eq!(v, (6000.0 - PI * 64.0 - PI * 100.0) * 20.0, max_relative = 0.01);
}

#[test]
fn detect_everything() {
    if !occt::available() {
        return;
    }
    let mesh = scan_of(&part(), 0.02);
    let found = detect_all(&mesh, &DetectOptions { tolerance: Some(0.1), ..Default::default() });
    let planes = found.iter().filter(|d| matches!(d.shape, DetectedShape::Plane { .. })).count();
    let cyl: Vec<f64> = found
        .iter()
        .filter_map(|d| match d.shape {
            DetectedShape::Cylinder { radius, .. } => Some(radius),
            _ => None,
        })
        .collect();
    // 6 caras de la caja + tapa del tetón
    assert!(planes >= 7, "planos: {planes}");
    assert!(cyl.iter().any(|r| (r - 8.0).abs() < 0.1), "{cyl:?}");
    assert!(cyl.iter().any(|r| (r - 10.0).abs() < 0.1), "{cyl:?}");
}

#[test]
fn simplify_and_circle_fit() {
    let circle: Vec<P2> = (0..100).map(|i| {
        let a = i as f64 / 100.0 * 2.0 * PI;
        [5.0 + 3.0 * a.cos(), -2.0 + 3.0 * a.sin()]
    }).collect();
    let (c, r, rms) = fit_circle(&circle).unwrap();
    assert_relative_eq!(c[0], 5.0, epsilon = 1e-9);
    assert_relative_eq!(c[1], -2.0, epsilon = 1e-9);
    assert_relative_eq!(r, 3.0, epsilon = 1e-9);
    assert!(rms < 1e-9);

    // Cuadrado con puntos intermedios: queda en 4 esquinas
    let mut sq = Vec::new();
    for k in 0..4 {
        let (a, b) = ([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]][k], [[10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]][k]);
        for i in 0..10 {
            let t = i as f64 / 10.0;
            sq.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    assert_eq!(simplify_closed(&sq, 0.01).len(), 4);
}

