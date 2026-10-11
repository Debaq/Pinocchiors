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
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 8000, offset: clearance }).unwrap();
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
    let id = doc.add(FeatureKind::ScanBody(ScanBody { offset: clearance, faces: 8000, op: BodyOp::Cut, mesh: None }));
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
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap }).unwrap();
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
        thin: None,
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
    let body = prepare_body(&scan.vertices, &scan.triangles, &BodyOptions { faces: 20000, offset: gap }).unwrap();
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
