//! Escaneo sintético: una pieza CAD conocida, teselada y con ruido, se
//! reconstruye con las herramientas de escaneo → CAD.

use approx::assert_relative_eq;
use cad_model::occt::{self, Frame, Shape};
use cad_model::*;
use cad_model::geom::{add, dot, scale, sub};
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
        thin: None, direction: None, start: 0.0,
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


/// Lazo con puntos cada `step` a lo largo de los lados del polígono, con ruido chico.
fn noisy_loop(corners: &[[f64; 2]], step: f64, noise: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut k = 0u32;
    for i in 0..corners.len() {
        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let n = (len / step).ceil() as usize;
        for j in 0..n {
            let t = j as f64 / n as f64;
            // Ruido determinista
            k = k.wrapping_mul(1103515245).wrapping_add(12345);
            let r = ((k >> 16) & 0x7fff) as f64 / 32767.0 - 0.5;
            out.push([a[0] + (b[0] - a[0]) * t + r * noise, a[1] + (b[1] - a[1]) * t - r * noise]);
        }
    }
    out
}

#[test]
fn outline_infers_parallel_and_perpendicular_without_conflicts() {
    // Rectángulo de 40 × 20 girado 30°
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let rot = |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    let corners = [[0.0, 0.0], [40.0, 0.0], [40.0, 20.0], [0.0, 20.0]].map(rot);
    let lp = noisy_loop(&corners, 0.5, 0.02);
    let mut sk = outline_sketch(&[lp], &OutlineOptions { tolerance: 0.1, ..Default::default() });
    let count = |f: &dyn Fn(&SketchConstraint) -> bool| sk.constraints.iter().filter(|c| f(c)).count();
    assert_eq!(count(&|c| matches!(c, SketchConstraint::Parallel { .. })), 2, "{:?}", sk.constraints);
    assert_eq!(count(&|c| matches!(c, SketchConstraint::Perpendicular { .. })), 1);
    assert_eq!(count(&|c| matches!(c, SketchConstraint::Horizontal { .. } | SketchConstraint::Vertical { .. })), 0);
    let report = sk.solve().unwrap();
    assert!(!matches!(report.status, SketchStatus::OverConstrained | SketchStatus::Failed), "{:?}", report.status);
    // Ya resuelto: ángulo recto exacto entre los dos primeros lados
    let lines: Vec<_> = sk.entities.iter().filter_map(|e| match e.geometry { Geometry::Line { start, end } => Some((start, end)), _ => None }).collect();
    let dir = |(a, b): (u32, u32)| {
        let (p, q) = (sk.point(a).unwrap(), sk.point(b).unwrap());
        [q[0] - p[0], q[1] - p[1]]
    };
    let (u, v) = (dir(lines[0]), dir(lines[1]));
    let cos = (u[0] * v[0] + u[1] * v[1]) / ((u[0] * u[0] + u[1] * u[1]).sqrt() * (v[0] * v[0] + v[1] * v[1]).sqrt());
    assert!(cos.abs() < 1e-9, "{cos}");

    // Una L alineada a los ejes: solo horizontales y verticales
    let l = noisy_loop(&[[0.0, 0.0], [30.0, 0.0], [30.0, 10.0], [10.0, 10.0], [10.0, 25.0], [0.0, 25.0]], 0.5, 0.02);
    let sk = outline_sketch(&[l], &OutlineOptions { tolerance: 0.1, round_to: 0.5, ..Default::default() });
    assert_eq!(sk.constraints.iter().filter(|c| matches!(c, SketchConstraint::Horizontal { .. })).count(), 3);
    assert_eq!(sk.constraints.iter().filter(|c| matches!(c, SketchConstraint::Vertical { .. })).count(), 3);
    assert!(!sk.constraints.iter().any(|c| matches!(c, SketchConstraint::Parallel { .. } | SketchConstraint::Perpendicular { .. })));
    // Redondeado a 0,5: las esquinas quedan en la grilla
    assert!(sk.points.iter().filter(|p| Some(p.id) != sk.origin).all(|p| (p.x * 2.0).fract().abs() < 1e-9 && (p.y * 2.0).fract().abs() < 1e-9));
}

#[test]
fn big_planes_come_out_whole() {
    if !occt::available() {
        return;
    }
    // Las caras de arriba y de abajo de la placa (con triángulos largos y finos
    // del teselado) salían partidas en dos zonas cada una
    let mesh = scan_of(&part(), 0.02);
    let found = detect_all(&mesh, &DetectOptions { tolerance: Some(0.1), ..Default::default() });
    let planes: Vec<(P3, f64)> = found
        .iter()
        .filter_map(|d| match &d.shape {
            DetectedShape::Plane { plane } => Some((plane.normal, d.area)),
            _ => None,
        })
        .collect();
    assert_eq!(planes.len(), 7, "6 de la placa + la tapa del tetón: {planes:?}");
    let area = |nz: f64| planes.iter().filter(|(n, a)| (n[2] - nz).abs() < 0.01 && *a > 1000.0).map(|p| p.1).collect::<Vec<_>>();
    assert_eq!(area(-1.0).len(), 1);
    assert_relative_eq!(area(-1.0)[0], 6000.0 - PI * 64.0, max_relative = 0.01);
    assert_relative_eq!(area(1.0)[0], 6000.0 - PI * 64.0 - PI * 100.0, max_relative = 0.01);
}

/// Las referencias tomadas del escaneo lo siguen si se mueve (mismo
/// triángulo) y lo encuentran si la malla se rehízo (punto más cercano).
#[test]
fn references_follow_the_scan() {
    let scan = scan_of(&part(), 0.01);
    let opts = PickOptions::default();
    let top = face_where(&scan, |c, n| n[2] > 0.99 && c[2] > 19.0 && c[0] < 50.0);
    let at = scan.face_centroids[top as usize];
    let seed = scan.seed(top, at).unwrap();
    let hole = face_where(&scan, |c, n| n[2].abs() < 0.1 && (c[0] - 30.0).hypot(c[1] - 30.0) < 8.5 && c[2] > 2.0 && c[2] < 18.0);
    let hole_seed = scan.seed(hole, scan.face_centroids[hole as usize]).unwrap();

    // Igual que el día que se tomó
    let ScanGeom::Plane(p) = fit_reference(&scan, &ScanFit::Zone { seed: seed.clone() }, &opts).unwrap() else { panic!() };
    assert_relative_eq!(p.origin[2], 20.0, epsilon = 0.05);
    assert!(p.normal[2] > 0.999);

    // El escaneo girado 90° en Z y subido 5 mm: la referencia va con él
    let moved: Vec<P3> = scan.vertices.iter().map(|v| [-v[1], v[0], v[2] + 5.0]).collect();
    let moved = ScanMesh::new(&moved, &scan.triangles);
    assert_eq!(moved.topology(), scan.topology());
    let ScanGeom::Plane(p) = fit_reference(&moved, &ScanFit::Zone { seed: seed.clone() }, &opts).unwrap() else { panic!() };
    assert_relative_eq!(dot(sub(p.origin, [0.0, 0.0, 25.0]), p.normal), 0.0, epsilon = 0.05);
    let ScanGeom::Axis { origin, direction } = fit_reference(&moved, &ScanFit::Cylinder { seed: hole_seed.clone() }, &opts).unwrap() else { panic!() };
    // El eje del agujero (30, 30) girado queda en (−30, 30)
    let t = dot(sub([-30.0, 30.0, 0.0], origin), direction);
    let foot = add(origin, scale(direction, t));
    assert!(direction[2].abs() > 0.999);
    assert_relative_eq!(foot[0], -30.0, epsilon = 0.1);
    assert_relative_eq!(foot[1], 30.0, epsilon = 0.1);
    let ScanGeom::Point(q) = fit_reference(&moved, &ScanFit::Point { seed: seed.clone() }, &opts).unwrap() else { panic!() };
    assert_relative_eq!(q[0], -at[1], epsilon = 1e-9);
    assert_relative_eq!(q[2], at[2] + 5.0, epsilon = 1e-9);

    // Otra malla (más fina): por el punto más cercano a donde estaba
    let t = part().tessellate(0.01, 0.05).unwrap();
    let finer = ScanMesh::new(&t.positions, &t.triangles);
    assert_ne!(finer.topology(), scan.topology());
    let ScanGeom::Plane(p) = fit_reference(&finer, &ScanFit::Zone { seed }, &opts).unwrap() else { panic!() };
    assert_relative_eq!(p.origin[2], 20.0, epsilon = 0.05);
}

/// El escaneo como cuerpo: cerrado y corrido la holgura, restado de un bloque
/// deja la cuna donde calza la pieza.
#[test]
fn scan_body_leaves_a_cradle_with_clearance() {
    if !occt::available() {
        return;
    }
    let scan = scan_of(&part(), 0.02);
    let clearance = 0.3;
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 8000, offset: clearance, thickness: 0.0 }).unwrap();
    assert_eq!(solid_problems(&body.vertices, &body.triangles), Default::default());
    assert_eq!(open_edges(&body.triangles), 0);
    let solid = Shape::from_mesh(&body.vertices, &body.triangles, 1e-4).unwrap();
    // Volumen + área × holgura (las aristas se redondean: algo menos)
    let m = part().mass().unwrap();
    let grown = solid.mass().unwrap().volume;
    assert_relative_eq!(grown, m.volume + m.area * clearance, max_relative = 0.02);

    // Bloque que toma la mitad de abajo de la pieza, menos el escaneo
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 120.0, dy: 80.0, dz: 20.0, centered: false, centered_z: false },
        origin: [-10.0, -10.0, -10.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
    }));
    let id = doc.add(FeatureKind::ScanBody(ScanBody { offset: clearance, faces: 8000, op: BodyOp::Cut, ..Default::default() }));
    // Sin malla (sin escaneo): la operación avisa
    let ev = doc.evaluate();
    assert!(matches!(ev.status[1].state, FeatureState::Error { .. }), "{:?}", ev.status[1].state);
    let FeatureKind::ScanBody(b) = &mut doc.features[1].kind else { unreachable!() };
    b.mesh = Some(ScanBodyMesh { hash: 1, data: std::sync::Arc::new((body.vertices.clone(), body.triangles.clone())), error: None });
    let ev = doc.evaluate();
    assert_eq!(ev.status[1].state, FeatureState::Ok, "{id:?}");
    let cradle = ev.body.as_ref().unwrap();
    assert!(cradle.is_valid());
    // Lo que se llevó: la mitad de abajo de la pieza agrandada (placa de 20 mm
    // entre z = 0 y 10 → la mitad), sin tocar el tetón que va arriba
    let removed = 120.0 * 80.0 * 20.0 - cradle.mass().unwrap().volume;
    let half = (100.0 + 2.0 * clearance) * (60.0 + 2.0 * clearance) * (10.0 + clearance) - PI * (8.0 - clearance).powi(2) * (10.0 + clearance);
    assert_relative_eq!(removed, half, max_relative = 0.02);
    // La huella viaja en el JSON (caché del recálculo) pero la malla no
    let json = serde_json::to_string(&doc.features[1].kind).unwrap();
    assert!(json.contains("\"mesh\":\"0000000000000001\""), "{json}");
    assert!(json.len() < 200);
}

/// Extruir hasta el escaneo: un cilindro que baja desde un sketch hasta una
/// esfera escaneada se detiene en su piel curva, a la separación pedida.
#[test]
fn extrude_up_to_the_scan_follows_the_curved_skin() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let gap = 0.5;
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap, thickness: 0.0 }).unwrap();
    let mesh = ScanBodyMesh { hash: 7, data: std::sync::Arc::new((body.vertices, body.triangles)), error: None };

    let r = 5.0;
    let mut sketch = Sketch::default();
    sketch.circle([0.0, 0.0], r);
    let mut doc = Document::new();
    // Sketch a 30 mm: el escaneo queda abajo, del otro lado de la normal
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch });
    doc.add(FeatureKind::Extrude(Extrude {
        sketch: sk,
        regions: RegionSelection::All,
        extent: Extent::UpToScan { offset: gap, mesh: Some(mesh) },
        reverse: false,
        op: BodyOp::New,
        draft: 0.0,
        thin: None, direction: None, start: 0.0,
    }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let m = ev.body.unwrap().mass().unwrap();
    // Cilindro hasta la esfera de radio 20 + 0,5: π r² · 30 − ∫ 2πρ √(R² − ρ²)
    let big = 20.0 + gap;
    let expected = PI * r * r * 30.0 - 2.0 * PI / 3.0 * (big.powi(3) - (big * big - r * r).powf(1.5));
    assert_relative_eq!(m.volume, expected, max_relative = 0.01);
    assert_relative_eq!(m.bbox_max[2], 30.0, epsilon = 1e-6);
    // Abajo, en el borde del círculo: a la altura de la esfera agrandada
    assert_relative_eq!(m.bbox_min[2], (big * big - r * r).sqrt(), epsilon = 0.1);
}

/// Esfera escaneada de radio 20 con centro a 40 mm de altura (abajo en z = 20)
/// y su malla corrida `gap`.
fn floating_ball(gap: f64) -> ScanBodyMesh {
    let t = Shape::sphere([0.0, 0.0, 40.0], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap, thickness: 0.0 }).unwrap();
    ScanBodyMesh { hash: 11 + gap.to_bits(), data: std::sync::Arc::new((body.vertices, body.triangles)), error: None }
}

/// Altura de la panza de la esfera agrandada sobre (x, y).
fn belly(x: f64, y: f64, gap: f64) -> f64 {
    40.0 - ((20.0 + gap).powi(2) - x * x - y * y).sqrt()
}

fn block(dx: f64, dy: f64, dz: f64) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx, dy, dz, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
    })
}

/// La cara de arriba de un bloque llevada hasta la panza de la esfera; y un
/// nervio desde el bloque hasta la esfera.
#[test]
fn face_and_rib_up_to_the_scan() {
    if !occt::available() {
        return;
    }
    let gap = 0.5;
    let ball = floating_ball(gap);
    let mut doc = Document::new();
    doc.add(block(10.0, 10.0, 10.0));
    let face = FaceRef { point: [0.0, 0.0, 10.0], normal: [0.0, 0.0, 1.0], ..Default::default() };
    doc.add(FeatureKind::FaceToScan { faces: vec![face], offset: gap, mesh: Some(ball.clone()) });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    // Bloque + lo que hay entre z = 10 y la panza (integral en una grilla)
    let n = 200;
    let h = 10.0 / n as f64;
    let mut added = 0.0;
    for i in 0..n {
        for j in 0..n {
            let (x, y) = (-5.0 + (i as f64 + 0.5) * h, -5.0 + (j as f64 + 0.5) * h);
            added += (belly(x, y, gap) - 10.0) * h * h;
        }
    }
    let m = ev.body.unwrap().mass().unwrap();
    assert_relative_eq!(m.volume, 1000.0 + added, max_relative = 0.01);
    assert_relative_eq!(m.bbox_max[2], belly(5.0, 5.0, gap), epsilon = 0.1);

    // Una cara más ancha que la esfera no tiene hasta dónde llegar
    let mut wide = Document::new();
    wide.add(block(60.0, 60.0, 10.0));
    let face = FaceRef { point: [0.0, 0.0, 10.0], normal: [0.0, 0.0, 1.0], ..Default::default() };
    wide.add(FeatureKind::FaceToScan { faces: vec![face], offset: gap, mesh: Some(ball.clone()) });
    let ev = wide.evaluate();
    assert!(matches!(&ev.status[1].state, FeatureState::Error { message, .. } if message.contains("frente al escaneo")), "{:?}", ev.status[1].state);

    // Nervio: línea sobre el bloque (plano XZ, z = 10) hasta la esfera, 2 mm de espesor
    let mut doc = Document::new();
    doc.add(block(10.0, 10.0, 10.0));
    let mut sketch = Sketch::default();
    let a = sketch.add_point(-4.0, 10.0);
    let b = sketch.add_point(4.0, 10.0);
    sketch.add_line(a, b);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch });
    doc.add(FeatureKind::Rib { sketch: sk, thickness: 2.0, flip: false, scan: Some(ScanReach { offset: gap, mesh: Some(ball) }) });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let rib: f64 = (0..n).map(|i| -4.0 + (i as f64 + 0.5) * 8.0 / n as f64).map(|x| (belly(x, 0.0, gap) - 10.0) * 2.0 * 8.0 / n as f64).sum();
    let m = ev.body.unwrap().mass().unwrap();
    // El nervio sigue la piel por tramos (32 por línea)
    assert_relative_eq!(m.volume, 1000.0 + rib, max_relative = 0.01);
    assert_eq!(ev.parts.len(), 1, "el nervio se une al bloque");
}

/// Carcasa que copia la piel de una esfera escaneada (dos pieles: la de
/// afuera y la del hueco) y el parche que cae bajo un círculo de un sketch
/// puesto arriba: solo la tapa de arriba, no la de abajo.
#[test]
fn scan_shell_and_patch_under_a_sketch() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let (gap, thick) = (0.5, 2.0);
    let options = BodyOptions { faces: 20000, offset: gap, thickness: thick };
    let shell = prepare_body(&scan.vertices, &scan.triangles, &options).unwrap();
    assert_eq!(solid_problems(&shell.vertices, &shell.triangles), Default::default());
    let solid = Shape::from_mesh(&shell.vertices, &shell.triangles, 1e-4).unwrap();
    assert_eq!(solid.kind(), occt::ShapeKind::Solid);
    assert!(solid.is_valid());
    let (r1, r2) = (20.0 + gap, 20.0 + gap + thick);
    let ball = |r: f64| 4.0 / 3.0 * PI * r * r * r;
    assert_relative_eq!(solid.mass().unwrap().volume, ball(r2) - ball(r1), max_relative = 0.02);
    // El hueco: un punto adentro de la esfera queda afuera del sólido
    assert_eq!(solid.ray_hit([0.0; 3], [0.0, 0.0, 1.0]).map(|t| (t - r1).abs() < 0.2), Some(true));

    // Parche bajo un círculo de radio 10 a 30 mm de altura
    let mesh = ScanBodyMesh { hash: 5, data: std::sync::Arc::new((shell.vertices, shell.triangles)), error: None };
    let r = 10.0;
    let mut sketch = Sketch::default();
    sketch.circle([0.0, 0.0], r);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch });
    let region = Some(ScanRegion { sketch: sk, regions: RegionSelection::All });
    doc.add(FeatureKind::ScanBody(ScanBody { offset: gap, thickness: thick, region, mesh: Some(mesh), ..Default::default() }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(doc.features[1].kind.label(), "Parche del escaneo");
    let m = ev.body.unwrap().mass().unwrap();
    // ∫ 2πρ (√(r2² − ρ²) − √(r1² − ρ²)) dρ, ρ de 0 a 10
    let cap = |big: f64| 2.0 * PI / 3.0 * (big.powi(3) - (big * big - r * r).powf(1.5));
    assert_relative_eq!(m.volume, cap(r2) - cap(r1), max_relative = 0.02);
    assert!(m.bbox_min[2] > 15.0, "solo la tapa de arriba: {:?}", m.bbox_min);
    assert_relative_eq!(m.bbox_max[0], r, epsilon = 0.05);
}

/// Parche pintado con el pincel sobre una esfera escaneada: un toque arriba
/// deja la tapa de ese radio (borde perpendicular a la piel = vertical aquí) y
/// uno abajo no entra si no se pintó.
#[test]
fn scan_paint_patch_follows_the_brush() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let (gap, thick) = (0.5, 2.0);
    let shell = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap, thickness: thick }).unwrap();
    let mesh = ScanBodyMesh { hash: 6, data: std::sync::Arc::new((shell.vertices, shell.triangles)), error: None };
    // El triángulo más alto de la esfera
    let top = (0..scan.triangles.len() as u32).filter(|&f| scan.face_areas[f as usize] > 1e-6).max_by(|&a, &b| scan.face_centroids[a as usize][2].total_cmp(&scan.face_centroids[b as usize][2])).unwrap();
    let at = scan.face_centroids[top as usize];
    let dab = scan.dab(top, at).expect("toque");
    assert!(dab.normal[2] > 0.99, "normal hacia afuera: {:?}", dab.normal);
    let r = 8.0;
    let mut doc = Document::new();
    let paint = Some(ScanPaint { radius: r, dabs: vec![dab] });
    doc.add(FeatureKind::ScanBody(ScanBody { offset: gap, thickness: thick, paint, mesh: Some(mesh), ..Default::default() }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(doc.features[0].kind.label(), "Parche del escaneo");
    let m = ev.body.unwrap().mass().unwrap();
    let (r1, r2) = (20.0 + gap, 20.0 + gap + thick);
    let cap = |big: f64| 2.0 * PI / 3.0 * (big.powi(3) - (big * big - r * r).powf(1.5));
    assert_relative_eq!(m.volume, cap(r2) - cap(r1), max_relative = 0.04);
    assert!(m.bbox_min[2] > 15.0, "solo la tapa de arriba: {:?}", m.bbox_min);
}

/// Molde de una esfera escaneada: dos mitades que no se tocan por dentro, la
/// cavidad esférica (con la holgura), llaves solo en su mitad y bebedero.
#[test]
fn scan_mold_makes_two_halves_with_keys_and_sprue() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let gap = 0.3;
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap, thickness: 0.0 }).unwrap();
    let mesh = ScanBodyMesh { hash: 7, data: std::sync::Arc::new((body.vertices, body.triangles)), error: None };
    let wall = 6.0;
    let options = MoldOptions { axis: 2, at: 0.5, wall, gap, keys: true, sprue: 2.0 };
    let mut doc = Document::new();
    add_mold(&mut doc, scan.bbox_min, scan.bbox_max, &options).unwrap();
    for f in &mut doc.features {
        if let FeatureKind::ScanBody(b) = &mut f.kind {
            b.mesh = Some(mesh.clone());
        }
    }
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 2, "dos mitades");
    let side = 40.0 + 2.0 * wall;
    let boxes = side * side * (40.0 + 2.0 * wall);
    let ball = 4.0 / 3.0 * PI * (20.0 + gap).powi(3);
    let total: f64 = ev.parts.iter().map(|p| p.shape.mass().unwrap().volume).sum();
    // Menos el bebedero (r 2 sobre la mitad de arriba), más las llaves que sobresalen: poco
    let sprue = PI * 4.0 * (20.0 + wall);
    assert_relative_eq!(total, boxes - ball - sprue, max_relative = 0.03);
    // Cada mitad queda de su lado del plano de partición (las llaves entran en la otra)
    let (lower, upper) = (ev.parts[0].shape.mass().unwrap(), ev.parts[1].shape.mass().unwrap());
    assert!(lower.bbox_min[2] < -20.0 && upper.bbox_max[2] > 20.0);
    assert!(upper.bbox_min[2] > -0.01, "arriba no pasa del plano: {:?}", upper.bbox_min);
    assert!(lower.bbox_max[2] > 0.5, "los pernos de abajo salen del plano: {:?}", lower.bbox_max);
}

/// Camino más corto sobre una esfera escaneada: un cuarto de círculo máximo.
#[test]
fn scan_geodesic_follows_the_great_circle() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let stop = |p: [f64; 3]| {
        let (tri, bary) = scan.closest(p).unwrap();
        let [a, b, c] = scan.triangles[tri as usize].map(|i| scan.vertices[i as usize]);
        (tri, add(add(scale(a, bary[0]), scale(b, bary[1])), scale(c, bary[2])))
    };
    let (a, b) = (stop([20.0, 0.0, 0.0]), stop([0.0, 20.0, 0.0]));
    let path = scan.geodesic(&[a, b], false).expect("camino");
    assert!(path.len() >= 4);
    assert_eq!(path[0], a.1);
    assert_eq!(path[path.len() - 1], b.1);
    let length: f64 = path.windows(2).map(|w| dot(sub(w[1], w[0]), sub(w[1], w[0])).sqrt()).sum();
    assert_relative_eq!(length, 20.0 * PI / 2.0, max_relative = 0.015);
    // Sobre la piel (la teselación queda apenas adentro de la esfera) y en el plano del ecuador
    for p in &path {
        assert!((dot(*p, *p).sqrt() - 20.0).abs() < 0.1, "fuera de la piel: {p:?}");
        assert!(p[2].abs() < 0.3, "se aparta del ecuador: {p:?}");
    }
    // Cerrada por tres puntos: un círculo máximo entero no (tres puntos del ecuador a 120°): perímetro ≈ 2πR
    let ring: Vec<_> = [0.0f64, 120.0, 240.0].iter().map(|d| stop([20.0 * d.to_radians().cos(), 20.0 * d.to_radians().sin(), 0.0])).collect();
    let closed = scan.geodesic(&ring, true).expect("camino cerrado");
    let length: f64 = closed.windows(2).map(|w| dot(sub(w[1], w[0]), sub(w[1], w[0])).sqrt()).sum();
    assert_relative_eq!(length, 2.0 * PI * 20.0, max_relative = 0.015);
}

/// Un círculo de un sketch llevado a la piel de la esfera escaneada, y un camino
/// sobre el escaneo como curva del diseño.
#[test]
fn scan_curves_project_and_path() {
    if !occt::available() {
        return;
    }
    let t = Shape::sphere([0.0; 3], 20.0).unwrap().tessellate(0.02, 0.05).unwrap();
    let scan = ScanMesh::new(&t.positions, &t.triangles);
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: 0.0, thickness: 0.0 }).unwrap();
    let mesh = ScanBodyMesh { hash: 9, data: std::sync::Arc::new((body.vertices, body.triangles)), error: None };
    let mut sketch = Sketch::default();
    sketch.circle([0.0, 0.0], 5.0);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch });
    let proj = doc.add(FeatureKind::Curve { def: CurveDef::ScanProjected { sketch: sk, entities: vec![], flip: false, offset: 0.0, mesh: Some(mesh) } });
    let points = vec![[20.0, 0.0, 0.0], [14.1, 14.1, 0.0], [0.0, 20.0, 0.0]];
    let path = doc.add(FeatureKind::Curve { def: CurveDef::ScanPath { seeds: vec![], closed: false, points } });
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(doc.features[1].kind.label(), "Curva proyectada al escaneo");
    let Some(RefGeom::Curves { lines }) = ev.references.get(&proj) else { panic!("sin curva proyectada") };
    let pts: Vec<_> = lines.iter().flatten().collect();
    assert!(pts.len() > 8);
    for p in &pts {
        // Sobre la esfera, bajo el círculo (la proyección conserva x e y)
        assert!((dot(**p, **p).sqrt() - 20.0).abs() < 0.2, "fuera de la piel: {p:?}");
        assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 5.0).abs() < 0.15, "se corrió del círculo: {p:?}");
    }
    assert!(matches!(ev.references.get(&path), Some(RefGeom::Curves { lines }) if !lines.is_empty()));
}

fn mesh_of(shape: &Shape) -> ScanMesh {
    let t = shape.tessellate(0.02, 0.05).unwrap();
    ScanMesh::new(&t.positions, &t.triangles)
}

/// Cono inclinado: eje, semiángulo, vértice y tramo salen de un clic en su lado.
#[test]
fn scan_picks_a_tilted_cone() {
    if !occt::available() {
        return;
    }
    let axis = cad_model::geom::normalize([1.0, 1.0, 1.0]);
    let x = cad_model::geom::normalize(cad_model::geom::cross(axis, [0.0, 0.0, 1.0]));
    let origin = [5.0, 3.0, 2.0];
    let cone = Shape::cone(Frame { origin, z: axis, x }, 10.0, 4.0, 30.0).unwrap();
    let scan = mesh_of(&cone);
    let seed = (0..scan.face_count() as u32)
        .filter(|&f| dot(scan.face_normals[f as usize], axis).abs() < 0.5 && scan.face_areas[f as usize] > 1e-3)
        .max_by(|&a, &b| scan.face_areas[a as usize].total_cmp(&scan.face_areas[b as usize]))
        .unwrap();
    let pick = pick_cone(&scan, seed, &PickOptions::default()).expect("cono");
    // tan(α) = (10 − 4) / 30; el vértice está a 10 / tan(α) = 50 de la base, del lado de la punta
    assert_relative_eq!(pick.half_angle, (6.0f64 / 30.0).atan(), epsilon = 0.01);
    assert!(dot(pick.direction, axis).abs() > 0.999, "{:?}", pick.direction);
    let expected_apex = add(origin, scale(axis, 50.0));
    let gap = sub(pick.apex, expected_apex);
    assert!(dot(gap, gap).sqrt() < 0.3, "vértice {:?} en vez de {:?}", pick.apex, expected_apex);
    assert!(dot(pick.direction, axis) < 0.0, "se abre hacia la base");
    assert_relative_eq!(pick.end - pick.start, 30.0, epsilon = 0.5);
    assert!(!pick.hole);
    // La primitiva del CAD: base de radio 10 y tapa de 4
    let r_start = pick.start * pick.half_angle.tan();
    let r_end = pick.end * pick.half_angle.tan();
    assert_relative_eq!(r_start.min(r_end), 4.0, epsilon = 0.2);
    assert_relative_eq!(r_start.max(r_end), 10.0, epsilon = 0.2);
}

/// Toro: el clic en cualquier lado da eje, radio mayor y menor.
#[test]
fn scan_picks_a_torus() {
    if !occt::available() {
        return;
    }
    let axis = cad_model::geom::normalize([0.0, 1.0, 1.0]);
    let x = cad_model::geom::normalize(cad_model::geom::cross(axis, [1.0, 0.0, 0.0]));
    let center = [1.0, -2.0, 3.0];
    let torus = Shape::torus(Frame { origin: center, z: axis, x }, 20.0, 4.0).unwrap();
    let scan = mesh_of(&torus);
    let seed = (0..scan.face_count() as u32).max_by(|&a, &b| scan.face_areas[a as usize].total_cmp(&scan.face_areas[b as usize])).unwrap();
    let pick = pick_torus(&scan, seed, &PickOptions::default()).expect("toro");
    assert_relative_eq!(pick.major, 20.0, epsilon = 0.1);
    assert_relative_eq!(pick.minor, 4.0, epsilon = 0.1);
    assert!(dot(pick.axis, axis).abs() > 0.999, "{:?}", pick.axis);
    let gap = sub(pick.center, center);
    assert!(dot(gap, gap).sqrt() < 0.2, "{:?}", pick.center);
}

/// Detección de todo el escaneo: un cono, un toro y el redondeo de un bloque.
#[test]
fn scan_detects_cone_torus_and_fillet() {
    if !occt::available() {
        return;
    }
    // Teselado de CAD no más fino que la tolerancia: con triángulos más chicos que el ruido ninguna normal es confiable
    let kinds_at = |shape: &Shape, deflection: f64, angle: f64| -> Vec<DetectedShape> {
        let t = shape.tessellate(deflection, angle).unwrap();
        detect_all(&ScanMesh::new(&t.positions, &t.triangles), &DetectOptions::default()).into_iter().map(|d| d.shape).collect()
    };
    let kinds = |shape: &Shape| kinds_at(shape, 0.02, 0.05);
    let cone = Shape::cone(Frame { origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0] }, 10.0, 4.0, 30.0).unwrap();
    assert!(
        kinds(&cone).iter().any(|s| matches!(s, DetectedShape::Cone { half_angle, .. } if (half_angle - (6.0f64 / 30.0).atan()).abs() < 0.02)),
        "cono: {:?}",
        kinds(&cone)
    );
    // Toro de malla propia, de triángulos mayores que el ruido (el teselado fino de CAD los deja todos "poco confiables")
    let (nu, nv) = (96usize, 48usize);
    let mut pos = Vec::new();
    for i in 0..nu {
        for j in 0..nv {
            let (u, v) = (i as f64 / nu as f64 * 2.0 * PI, j as f64 / nv as f64 * 2.0 * PI);
            pos.push([(20.0 + 4.0 * v.cos()) * u.cos(), (20.0 + 4.0 * v.cos()) * u.sin(), 4.0 * v.sin()]);
        }
    }
    let at = |i: usize, j: usize| ((i % nu) * nv + j % nv) as u32;
    let mut tris = Vec::new();
    for i in 0..nu {
        for j in 0..nv {
            tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    let found = detect_all(&ScanMesh::new(&pos, &tris), &DetectOptions::default());
    assert!(
        found.iter().any(|d| matches!(&d.shape, DetectedShape::Torus { major, minor, .. } if (major - 20.0).abs() < 0.2 && (minor - 4.0).abs() < 0.2)),
        "toro: {:?}",
        found.iter().map(|d| &d.shape).collect::<Vec<_>>()
    );
    // Bloque de 40 × 30 × 20 con una arista vertical redondeada a r = 5
    let block = Shape::make_box(Frame::at([0.0; 3]), 40.0, 30.0, 20.0).unwrap();
    let edge = (0..block.edge_count())
        .find(|&e| {
            let i = block.edge_info(e).unwrap();
            (i.length - 20.0).abs() < 1e-6 && (i.mid[0] - 40.0).abs() < 1e-6 && (i.mid[1] - 30.0).abs() < 1e-6
        })
        .expect("arista");
    let rounded = block.fillet(&[edge], 5.0).unwrap();
    // Con un teselado fino y la tolerancia a la medida (los lados de 10° se pegarían a los planos de al lado)
    let t = rounded.tessellate(0.003, 0.03).unwrap();
    let found = detect_all(&ScanMesh::new(&t.positions, &t.triangles), &DetectOptions { tolerance: Some(0.01), ..Default::default() });
    assert!(
        found.iter().any(|d| matches!(&d.shape, DetectedShape::Fillet { radius, angle, .. } if (radius - 5.0).abs() < 0.1 && (*angle - 90.0).abs() <= 15.0)),
        "redondeo: {:?}",
        found.iter().map(|d| &d.shape).collect::<Vec<_>>()
    );
}
