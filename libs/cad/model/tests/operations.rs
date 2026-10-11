//! Operaciones del plan de operaciones (F1–F3, las fáciles): simetría como
//! pieza aparte, mover/girar/copiar cuerpos, patrones con copias omitidas y en
//! dos direcciones, revolución simétrica o a dos ángulos y extrusión hasta un punto.

use approx::assert_relative_eq;
use cad_model::*;
use cad_model::geom::{cross, dot, norm, normalize, sub};
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn cube(x: f64, size: f64, op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: size, dy: size, dz: size, centered: false, centered_z: false },
        origin: [x, 0.0, 0.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op,
        link: None,
    })
}

fn eval_ok(doc: &Document) -> Evaluation {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev
}

fn bbox(ev: &Evaluation) -> ([f64; 3], [f64; 3], f64) {
    let m = ev.body.as_ref().unwrap().mass().unwrap();
    (m.bbox_min, m.bbox_max, m.volume)
}

fn extrude(sketch: FeatureId, extent: Extent, op: BodyOp) -> FeatureKind {
    FeatureKind::Extrude(Extrude { sketch, regions: RegionSelection::All, extent, reverse: false, op, draft: 0.0, thin: None, direction: None, start: 0.0 })
}

/// Placa de 100 × 60 × 2 con un agujero de Ø4 en (10, 10)
fn plate_with_hole() -> (Document, FeatureId) {
    let mut doc = Document::new();
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [100.0, 60.0]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(extrude(sk, Extent::Blind { distance: 2.0 }, BodyOp::Join));
    let mut c = Sketch::default();
    let o = c.add_point(10.0, 10.0);
    c.add_entity(Geometry::Circle { center: o, radius: 2.0 });
    let hs = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: c });
    let hole = doc.add(extrude(hs, Extent::ThroughAll, BodyOp::Cut));
    (doc, hole)
}

const HOLE: f64 = PI * 4.0 * 2.0;

#[test]
fn mirror_body_as_new_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(10.0, 10.0, BodyOp::Join));
    let m = doc.add(FeatureKind::Mirror { features: vec![], plane: PlaneSpec::Yz, op: BodyOp::New });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    assert_eq!(ev.parts[1].id.feature, m);
    for p in &ev.parts {
        assert_relative_eq!(p.shape.mass().unwrap().volume, 1000.0, max_relative = 1e-9);
    }
    let (lo, hi, _) = bbox(&ev);
    assert_relative_eq!(lo[0], -20.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 20.0, epsilon = 1e-6);
    // Unir (como antes): una sola pieza
    if let FeatureKind::Mirror { op, .. } = &mut doc.get_mut(m).unwrap().kind {
        *op = BodyOp::Join;
    }
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
}

#[test]
fn transform_moves_rotates_and_copies_parts() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let t = doc.add(FeatureKind::Transform { parts: vec![], motion: Motion::Translate { vector: [5.0, 0.0, 0.0] }, copy: false });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
    let (lo, hi, v) = bbox(&ev);
    assert_relative_eq!(lo[0], 5.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 15.0, epsilon = 1e-6);
    assert_relative_eq!(v, 1000.0, max_relative = 1e-9);

    // Girar 90° alrededor de Z: X pasa a Y
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform { parts: vec![], motion: Motion::Rotate { axis: AxisSpec::Z, angle: 90.0 }, copy: false };
    let (lo, hi, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[0], -10.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[1], 10.0, epsilon = 1e-6);

    // De un punto a otro
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform {
        parts: vec![],
        motion: Motion::Points { from: PointSpec::At { point: [0.0, 0.0, 0.0] }, to: PointSpec::At { point: [0.0, 0.0, 30.0] } },
        copy: false,
    };
    let (lo, _, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[2], 30.0, epsilon = 1e-6);

    // Copiar: la original queda y la copia es una pieza nueva
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform { parts: vec![], motion: Motion::Translate { vector: [20.0, 0.0, 0.0] }, copy: true };
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    assert_eq!(ev.parts[1].id.feature, t);
    let (lo, hi, v) = bbox(&ev);
    assert_relative_eq!(lo[0], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 30.0, epsilon = 1e-6);
    assert_relative_eq!(v, 2000.0, max_relative = 1e-9);
}

#[test]
fn transform_only_the_chosen_part() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let b = doc.add(cube(30.0, 10.0, BodyOp::New));
    doc.add(FeatureKind::Transform { parts: vec![PartId { feature: b, index: 0 }], motion: Motion::Translate { vector: [0.0, 0.0, 50.0] }, copy: false });
    let ev = eval_ok(&doc);
    let lo = |i: usize| ev.parts[i].shape.mass().unwrap().bbox_min;
    assert_relative_eq!(lo(0)[2], 0.0, epsilon = 1e-6);
    assert_relative_eq!(lo(1)[2], 50.0, epsilon = 1e-6);
}

#[test]
fn pattern_skips_copies() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear { direction: [1.0, 0.0, 0.0], count: 4, spacing: 20.0, second: None },
        skip: vec![2],
    });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 3.0 * HOLE, max_relative = 1e-6);
    // Todas las copias omitidas: queda el original
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![1, 2, 3];
    }
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - HOLE, max_relative = 1e-6);
}

#[test]
fn linear_pattern_in_two_directions() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    let p = doc.add(FeatureKind::Pattern {
        features: vec![hole],
        pattern: PatternKind::Linear {
            direction: [1.0, 0.0, 0.0],
            count: 3,
            spacing: 20.0,
            second: Some(LinearSecond { direction: [0.0, 1.0, 0.0], count: 2, spacing: 30.0 }),
        },
        skip: vec![],
    });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 6.0 * HOLE, max_relative = 1e-6);
    // Copia 4 = segunda fila, segunda columna (k = j·3 + i)
    if let FeatureKind::Pattern { skip, .. } = &mut doc.get_mut(p).unwrap().kind {
        *skip = vec![4];
    }
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 5.0 * HOLE, max_relative = 1e-6);
}

/// Anillo de [5, 8] × [0, 20] en el plano frontal, girado alrededor de Z
fn ring(extent: RevolveExtent, angle: f64) -> Document {
    let mut s = Sketch::new();
    s.rectangle([5.0, 0.0], [8.0, 20.0]);
    let axis = s.line([0.0, 0.0], [0.0, 10.0]);
    s.entities.iter_mut().find(|e| e.id == axis).unwrap().construction = true;
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Revolve(Revolve {
        sketch: sk,
        regions: RegionSelection::All,
        axis: AxisSpec::SketchLine { sketch: sk, line: axis },
        angle,
        op: BodyOp::Join,
        extent,
        thin: None,
    }));
    doc
}

#[test]
fn revolve_symmetric_and_two_angles() {
    if !occt() {
        return;
    }
    let full = PI * (64.0 - 25.0) * 20.0;
    // Simétrica: la mitad hacia cada lado del plano del sketch
    let (lo, hi, v) = bbox(&eval_ok(&ring(RevolveExtent::Symmetric, 90.0)));
    assert_relative_eq!(v, full / 4.0, max_relative = 1e-6);
    assert_relative_eq!(lo[1], -hi[1], epsilon = 1e-6);
    assert!(hi[1] > 1.0);
    // Dos ángulos: 90 + 30 = un tercio de vuelta
    let (_, _, v) = bbox(&eval_ok(&ring(RevolveExtent::TwoAngles { second: 30.0 }, 90.0)));
    assert_relative_eq!(v, full / 3.0, max_relative = 1e-6);
    // Más de una vuelta entre los dos: error
    let ev = ring(RevolveExtent::TwoAngles { second: 200.0 }, 200.0).evaluate();
    assert!(!ev.errors().is_empty());
    // Los documentos de antes (sin `extent`) siguen abriendo: un solo lado
    let json = r#"{"sketch":1,"axis":{"type":"z"},"angle":90.0}"#;
    let r: Revolve = serde_json::from_str(json).unwrap();
    assert_eq!(r.extent, RevolveExtent::One);
}

#[test]
fn extrude_up_to_vertex() {
    if !occt() {
        return;
    }
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let e = doc.add(extrude(sk, Extent::UpToVertex { point: PointSpec::At { point: [50.0, 50.0, 7.0] } }, BodyOp::Join));
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 700.0, max_relative = 1e-9);
    assert_relative_eq!(lo[2], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 7.0, epsilon = 1e-6);
    // Un punto del otro lado: va hacia allá
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.extent = Extent::UpToVertex { point: PointSpec::At { point: [0.0, 0.0, -4.0] } };
    }
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 400.0, max_relative = 1e-9);
    assert_relative_eq!(lo[2], -4.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 0.0, epsilon = 1e-6);
}

// ─── Segunda tanda: patrón por puntos, revolución delgada, espiral y superficies ───

/// Sketch en la planta con puntos sueltos en `at`
fn points_sketch(at: &[[f64; 2]]) -> (Sketch, Vec<u32>) {
    let mut s = Sketch::default();
    let mut ids = Vec::new();
    for q in at {
        let p = s.add_point(q[0], q[1]);
        s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Point { point: p }, construction: false, infinite: false, axis: false, layer: None });
        s.next_id += 1;
        ids.push(p);
    }
    (s, ids)
}

#[test]
fn pattern_on_sketch_points() {
    if !occt() {
        return;
    }
    let (mut doc, hole) = plate_with_hole();
    // El primer punto está sobre el agujero original: ahí no se copia
    let (s, ids) = points_sketch(&[[10.0, 10.0], [40.0, 10.0], [70.0, 30.0], [90.0, 50.0]]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let p = doc.add(FeatureKind::Pattern { features: vec![hole], pattern: PatternKind::Points { sketch: sk, points: vec![] }, skip: vec![] });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 4.0 * HOLE, max_relative = 1e-6);
    // Solo dos de los puntos (el del original y otro)
    if let FeatureKind::Pattern { pattern: PatternKind::Points { points, .. }, .. } = &mut doc.get_mut(p).unwrap().kind {
        *points = vec![ids[0], ids[2]];
    }
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0 - 2.0 * HOLE, max_relative = 1e-6);
}

#[test]
fn revolve_thin() {
    if !occt() {
        return;
    }
    let mut doc = ring(RevolveExtent::One, 360.0);
    let last = doc.features.last().unwrap().id;
    if let FeatureKind::Revolve(r) = &mut doc.get_mut(last).unwrap().kind {
        r.thin = Some(1.0);
    }
    // Banda de 1 mm alrededor del rectángulo de 3 × 20 (esquinas de afuera
    // redondeadas), centrada en x = 6,5: Pappus
    let outer = 60.0 + 0.5 * 46.0 + PI * 0.25;
    let inner = 2.0 * 19.0;
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 2.0 * PI * 6.5 * (outer - inner), max_relative = 1e-4);
}

fn curve_points(doc: &Document, id: FeatureId) -> Vec<P3> {
    let ev = eval_ok(doc);
    match ev.references.get(&id) {
        Some(RefGeom::Curve { points }) => points.clone(),
        other => panic!("sin curva: {other:?}"),
    }
}

#[test]
fn conical_helix_and_flat_spiral() {
    if !occt() {
        return;
    }
    let radial = |p: &P3| (p[0] * p[0] + p[1] * p[1]).sqrt();
    // Cónica: de radio 10 a 5 en 2 vueltas de paso 4 (sube 8)
    let mut doc = Document::new();
    let h = doc.add(FeatureKind::Helix { axis: AxisSpec::Z, radius: 10.0, pitch: 4.0, turns: 2.0, left: false, radius2: Some(5.0), flat: false });
    let pts = curve_points(&doc, h);
    let top = pts.iter().copied().max_by(|a, b| a[2].total_cmp(&b[2])).unwrap();
    let bottom = pts.iter().copied().min_by(|a, b| a[2].total_cmp(&b[2])).unwrap();
    assert_relative_eq!(top[2], 8.0, epsilon = 1e-3);
    assert_relative_eq!(radial(&top), 5.0, epsilon = 0.05);
    assert_relative_eq!(radial(&bottom), 10.0, epsilon = 0.05);
    assert!(doc.get(h).unwrap().kind.label() == "Hélice");

    // Espiral plana: de radio 2 a 10 en 3 vueltas, toda en z = 0
    doc.get_mut(h).unwrap().kind = FeatureKind::Helix { axis: AxisSpec::Z, radius: 2.0, pitch: 4.0, turns: 3.0, left: false, radius2: Some(10.0), flat: true };
    let pts = curve_points(&doc, h);
    assert!(pts.iter().all(|p| p[2].abs() < 1e-6));
    let rmax = pts.iter().map(radial).fold(0.0, f64::max);
    let rmin = pts.iter().map(radial).fold(f64::INFINITY, f64::min);
    assert_relative_eq!(rmax, 10.0, epsilon = 0.05);
    assert_relative_eq!(rmin, 2.0, epsilon = 0.05);
    assert_eq!(doc.get(h).unwrap().kind.label(), "Espiral");
    // Plana con un solo radio: error
    doc.get_mut(h).unwrap().kind = FeatureKind::Helix { axis: AxisSpec::Z, radius: 2.0, pitch: 4.0, turns: 3.0, left: false, radius2: None, flat: true };
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn planar_and_offset_surfaces() {
    if !occt() {
        return;
    }
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 20.0]);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    let plane = doc.add(FeatureKind::SurfacePlanar { sketch: sk, regions: RegionSelection::All });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
    assert!(ev.parts[0].is_surface());
    assert_relative_eq!(ev.parts[0].shape.mass().unwrap().area, 200.0, max_relative = 1e-9);

    // Desfasada 5: otra superficie igual, a 5 del plano
    doc.add(FeatureKind::SurfaceOffset { parts: vec![PartId { feature: plane, index: 0 }], distance: 5.0 });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    let m = ev.parts[1].shape.mass().unwrap();
    assert!(ev.parts[1].is_surface());
    assert_relative_eq!(m.area, 200.0, max_relative = 1e-6);
    assert_relative_eq!(m.bbox_min[2].abs(), 5.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[2], m.bbox_min[2], epsilon = 1e-6);

    // De un sólido queda su piel desfasada (aristas y esquinas redondeadas), sin volumen
    let mut doc = Document::new();
    let c = doc.add(cube(0.0, 10.0, BodyOp::Join));
    doc.add(FeatureKind::SurfaceOffset { parts: vec![PartId { feature: c, index: 0 }], distance: 2.0 });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    assert!(ev.parts[1].is_surface());
    let skin = 600.0 + 12.0 * (10.0 * PI * 2.0 * 2.0 / 4.0) + 4.0 * PI * 4.0;
    assert_relative_eq!(ev.parts[1].shape.mass().unwrap().area, skin, max_relative = 1e-4);
}

#[test]
fn inch_thread_names() {
    let t = standard::inch("1/4-20 UNC").unwrap();
    assert_relative_eq!(t.d, 6.35, epsilon = 1e-9);
    assert_relative_eq!(t.pitch, 1.27, epsilon = 1e-9);
    let spec = ThreadSpec { nominal: t.d, pitch: t.pitch, clearance: 0.0, left: false };
    assert_eq!(threads::spec_name(&spec), "1/4-20 UNC");
    assert_eq!(threads::spec_name(&ThreadSpec { left: true, ..spec }), "1/4-20 UNC izq.");
    assert_eq!(standard::INCH.len(), 20);
    // La métrica sigue igual
    assert_eq!(threads::spec_name(&ThreadSpec { nominal: 6.0, pitch: 1.0, clearance: 0.0, left: false }), "M6");
}

// ─── Tanda media ───

/// Referencia a la cara del cuerpo más cercana a `p` con normal `n`
fn face_at(doc: &Document, p: P3, n: P3) -> FaceRef {
    let ev = eval_ok(doc);
    let (i, _) = ev.body.as_ref().unwrap().closest_face(p, Some(n), 0.99).unwrap();
    ev.face_ref(i).unwrap()
}

#[test]
fn transform_aligns_a_face_to_a_plane() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let top = face_at(&doc, [5.0, 5.0, 10.0], [0.0, 0.0, 1.0]);
    // La cara de arriba contra el frente (normal −Y): la caja queda del lado −Y, apoyada en y = 0
    let t = doc.add(FeatureKind::Transform { parts: vec![], motion: Motion::Align { face: top.clone(), to: PlaneSpec::Xz, flip: false }, copy: false });
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 1000.0, max_relative = 1e-9);
    for (a, b) in [(lo, [0.0, -10.0, 5.0]), (hi, [10.0, 0.0, 15.0])] {
        for k in 0..3 {
            assert_relative_eq!(a[k], b[k], epsilon = 1e-6);
        }
    }
    // Dada vuelta: del lado +Y
    doc.get_mut(t).unwrap().kind = FeatureKind::Transform { parts: vec![], motion: Motion::Align { face: top, to: PlaneSpec::Xz, flip: true }, copy: false };
    let (lo, hi, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[1], 0.0, epsilon = 1e-6);
    assert_relative_eq!(hi[1], 10.0, epsilon = 1e-6);
}

/// Primer índice de arista del cuerpo cuyo medio está cerca de `p`
fn edge_at(doc: &Document, p: P3) -> EdgeRef {
    let ev = eval_ok(doc);
    let body = ev.body.as_ref().unwrap();
    let i = (0..body.edge_count()).min_by(|&a, &b| {
        let d = |k: usize| {
            let m = body.edge_info(k).unwrap().mid;
            (0..3).map(|j| (m[j] - p[j]).powi(2)).sum::<f64>()
        };
        d(a).total_cmp(&d(b))
    });
    ev.edge_ref(i.unwrap()).unwrap()
}

#[test]
fn delete_face_closes_a_hole() {
    if !occt() {
        return;
    }
    let (mut doc, _) = plate_with_hole();
    let wall = face_at(&doc, [12.0, 10.0, 1.0], [-1.0, 0.0, 0.0]);
    doc.add(FeatureKind::DeleteFace { faces: vec![wall] });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 12000.0, max_relative = 1e-6);
}

#[test]
fn shell_with_a_thicker_face() {
    if !occt() {
        return;
    }
    // Caja de 20 abierta arriba: paredes de 1 y el fondo de 3
    let mut doc = Document::new();
    doc.add(cube(0.0, 20.0, BodyOp::Join));
    let top = face_at(&doc, [10.0, 10.0, 20.0], [0.0, 0.0, 1.0]);
    let bottom = face_at(&doc, [10.0, 10.0, 0.0], [0.0, 0.0, -1.0]);
    doc.add(FeatureKind::Shell { faces: vec![top], thickness: 1.0, overrides: vec![FaceThickness { face: bottom, thickness: 3.0 }] });
    let (_, _, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 8000.0 - 18.0 * 18.0 * 17.0, max_relative = 1e-6);
}

#[test]
fn fillet_through_radius_points() {
    if !occt() {
        return;
    }
    // Arista de arriba al frente de una caja de 20: 2 en las puntas y 5 en el medio
    let mut doc = Document::new();
    doc.add(cube(0.0, 20.0, BodyOp::Join));
    let e = edge_at(&doc, [10.0, 0.0, 20.0]);
    let f = doc.add(FeatureKind::Fillet { edges: vec![e], radius: 2.0, radius2: None, points: vec![FilletPoint { at: 0.5, radius: 5.0 }] });
    let (_, _, v) = bbox(&eval_ok(&doc));
    // Quita entre lo de radio 2 y lo de radio 5 a lo largo de los 20
    let cut = |r: f64| (1.0 - PI / 4.0) * r * r * 20.0;
    assert!(v < 8000.0 - cut(2.0) && v > 8000.0 - cut(5.0), "volumen {v}");
    // Un punto fuera de la arista: error
    if let FeatureKind::Fillet { points, .. } = &mut doc.get_mut(f).unwrap().kind {
        points[0].at = 1.5;
    }
    assert!(!doc.evaluate().errors().is_empty());
}

fn square_at(doc: &mut Document, z: f64) -> FeatureId {
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: z, sketch: s })
}

#[test]
fn extrude_from_an_offset_start() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = square_at(&mut doc, 0.0);
    let e = doc.add(extrude(sk, Extent::Blind { distance: 5.0 }, BodyOp::Join));
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.start = 3.0;
    }
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 500.0, max_relative = 1e-9);
    assert_relative_eq!(lo[2], 3.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 8.0, epsilon = 1e-6);
    // Hasta un punto a z = 10 desde z = 3: 7 de alto
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.extent = Extent::UpToVertex { point: PointSpec::At { point: [0.0, 0.0, 10.0] } };
    }
    let (lo, hi, _) = bbox(&eval_ok(&doc));
    assert_relative_eq!(lo[2], 3.0, epsilon = 1e-6);
    assert_relative_eq!(hi[2], 10.0, epsilon = 1e-6);
}

#[test]
fn extrude_along_its_own_direction() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let sk = square_at(&mut doc, 0.0);
    let e = doc.add(extrude(sk, Extent::Blind { distance: 10.0 }, BodyOp::Join));
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.direction = Some(AxisSpec::Custom { origin: [0.0; 3], direction: [1.0, 0.0, 1.0] });
    }
    // Prisma oblicuo a 45°: 10 a lo largo de la dirección
    let h = 10.0 / 2f64.sqrt();
    let (lo, hi, v) = bbox(&eval_ok(&doc));
    assert_relative_eq!(v, 100.0 * h, max_relative = 1e-9);
    assert_relative_eq!(hi[2], h, epsilon = 1e-6);
    assert_relative_eq!(hi[0], 10.0 + h, epsilon = 1e-6);
    assert_relative_eq!(lo[0], 0.0, epsilon = 1e-6);
    // A lo largo del plano: error
    if let FeatureKind::Extrude(x) = &mut doc.get_mut(e).unwrap().kind {
        x.direction = Some(AxisSpec::X);
    }
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn extrude_up_to_a_face_with_offset() {
    if !occt() {
        return;
    }
    // Caja de 20 (z de 0 a 20) y un cuadrado a z = 40 que baja hasta 2 antes de la cara de arriba
    let mut doc = Document::new();
    doc.add(cube(30.0, 20.0, BodyOp::Join));
    let top = face_at(&doc, [40.0, 10.0, 20.0], [0.0, 0.0, 1.0]);
    let sk = square_at(&mut doc, 40.0);
    doc.add(extrude(sk, Extent::UpToFace { face: top, offset: 2.0 }, BodyOp::Join));
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 2);
    let m = ev.parts[1].shape.mass().unwrap();
    assert_relative_eq!(m.bbox_min[2], 22.0, epsilon = 1e-6);
    assert_relative_eq!(m.volume, 1800.0, max_relative = 1e-9);
}

#[test]
fn extend_a_planar_surface() {
    if !occt() {
        return;
    }
    // Superficie plana de 10 × 20; el borde de x = 10 se extiende 5: queda de 15 × 20
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 20.0]);
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
    doc.add(FeatureKind::SurfacePlanar { sketch: sk, regions: RegionSelection::All });
    let edge = edge_at(&doc, [10.0, 10.0, 0.0]);
    doc.add(FeatureKind::SurfaceExtend { edges: vec![edge], distance: 5.0 });
    let ev = eval_ok(&doc);
    assert_eq!(ev.parts.len(), 1);
    let m = ev.parts[0].shape.mass().unwrap();
    assert_relative_eq!(m.area, 300.0, max_relative = 1e-6);
    assert_relative_eq!(m.bbox_max[0], 15.0, epsilon = 1e-6);
    // De un sólido: error
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let edge = edge_at(&doc, [10.0, 5.0, 10.0]);
    doc.add(FeatureKind::SurfaceExtend { edges: vec![edge], distance: 5.0 });
    assert!(!doc.evaluate().errors().is_empty());
}

// ─── Referencias por clics ───

fn plane_of(ev: &Evaluation, id: FeatureId) -> Plane {
    match ev.references.get(&id) {
        Some(RefGeom::Plane { plane }) => *plane,
        other => panic!("sin plano: {other:?}"),
    }
}

fn picked_plane(doc: &mut Document, refs: Vec<RefPick>, offset: f64, angle: f64) -> (Plane, String) {
    let id = doc.add(FeatureKind::Plane { def: PlaneDef::Picked { refs, offset, angle, flip: false } });
    let ev = eval_ok(doc);
    let out = (plane_of(&ev, id), ev.ref_kinds.get(&id).cloned().unwrap_or_default());
    doc.features.pop();
    out
}

fn assert_plane(p: &Plane, normal: P3, through: P3) {
    let n = normalize(p.normal);
    assert!((dot(n, normalize(normal)).abs() - 1.0).abs() < 1e-6, "normal {n:?} (esperada ±{normal:?})");
    assert!(dot(sub(through, p.origin), n).abs() < 1e-6, "el plano no pasa por {through:?}");
}

fn vertex(p: P3) -> RefPick {
    RefPick::Point { point: PointSpec::At { point: p } }
}

#[test]
fn planes_by_clicks() {
    if !occt() {
        return;
    }
    // Caja de 10 en la esquina del origen
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let top = || RefPick::Face { face: face_at(&doc, [5.0, 5.0, 10.0], [0.0, 0.0, 1.0]) };
    let bottom = RefPick::Face { face: face_at(&doc, [5.0, 5.0, 0.0], [0.0, 0.0, -1.0]) };
    let side = RefPick::Face { face: face_at(&doc, [10.0, 5.0, 5.0], [1.0, 0.0, 0.0]) };
    let front_top = RefPick::Edge { edge: edge_at(&doc, [5.0, 0.0, 10.0]) };
    let upright = RefPick::Edge { edge: edge_at(&doc, [10.0, 0.0, 5.0]) };
    let t = top();

    let (p, how) = picked_plane(&mut doc, vec![t.clone()], 5.0, 0.0);
    assert_plane(&p, [0.0, 0.0, 1.0], [0.0, 0.0, 15.0]);
    assert_eq!(how, "Desfasado de un plano o una cara");

    let (p, how) = picked_plane(&mut doc, vec![t.clone(), bottom], 0.0, 0.0);
    assert_plane(&p, [0.0, 0.0, 1.0], [0.0, 0.0, 5.0]);
    assert_eq!(how, "Plano medio");

    let (p, how) = picked_plane(&mut doc, vec![t.clone(), side], 0.0, 0.0);
    assert_plane(&p, [-1.0, 0.0, 1.0], [10.0, 0.0, 10.0]);
    assert_eq!(how, "Bisector de dos planos");

    // La planta y la arista de arriba al frente: paralelo por la arista, y a 90° vertical
    let (p, _) = picked_plane(&mut doc, vec![RefPick::Plane { plane: PlaneSpec::Xy }, front_top.clone()], 0.0, 0.0);
    assert_plane(&p, [0.0, 0.0, 1.0], [0.0, 0.0, 10.0]);
    let (p, how) = picked_plane(&mut doc, vec![RefPick::Plane { plane: PlaneSpec::Xy }, front_top], 0.0, 90.0);
    assert_plane(&p, [0.0, 1.0, 0.0], [3.0, 0.0, 10.0]);
    assert_eq!(how, "Por una línea, en ángulo con el plano");

    // La arista vertical del frente y el origen: el plano del frente
    let (p, how) = picked_plane(&mut doc, vec![upright.clone(), vertex([0.0, 0.0, 0.0])], 0.0, 0.0);
    assert_plane(&p, [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(how, "Por una línea y un punto");
    // La misma arista y un punto sobre ella: normal a la arista ahí
    let (p, how) = picked_plane(&mut doc, vec![upright, vertex([10.0, 0.0, 4.0])], 0.0, 0.0);
    assert_plane(&p, [0.0, 0.0, 1.0], [0.0, 0.0, 4.0]);
    assert_eq!(how, "Normal a la línea por el punto");

    let (p, how) = picked_plane(&mut doc, vec![vertex([0.0, 0.0, 0.0]), vertex([10.0, 0.0, 0.0]), vertex([0.0, 0.0, 10.0])], 0.0, 0.0);
    assert_plane(&p, [0.0, 1.0, 0.0], [5.0, 0.0, 5.0]);
    assert_eq!(how, "Por tres puntos");

    // Lo que no forma un plano: error que dice qué se puede
    doc.add(FeatureKind::Plane { def: PlaneDef::Picked { refs: vec![vertex([0.0; 3]), vertex([1.0, 0.0, 0.0])], offset: 0.0, angle: 0.0, flip: false } });
    let ev = doc.evaluate();
    assert!(ev.errors().iter().any(|(_, m)| m.contains("no sé armar un plano")), "{:?}", ev.errors());
}

#[test]
fn planes_on_curved_geometry_by_clicks() {
    if !occt() {
        return;
    }
    // Cilindro de radio 5 y alto 10 sobre el origen
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 5.0, height: 10.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let wall = RefPick::Face { face: face_at(&doc, [5.0, 0.0, 5.0], [1.0, 0.0, 0.0]) };
    let rim = RefPick::Edge { edge: edge_at(&doc, [0.0, 5.0, 10.0]) };
    // Tangente a la pared por un punto de ella: el plano x = 5
    let (p, how) = picked_plane(&mut doc, vec![wall.clone(), vertex([5.0, 0.0, 3.0])], 0.0, 0.0);
    assert_plane(&p, [1.0, 0.0, 0.0], [5.0, 0.0, 3.0]);
    assert_eq!(how, "Tangente a la cara por el punto");
    // El borde de arriba: el plano de la circunferencia
    let (p, how) = picked_plane(&mut doc, vec![rim.clone()], 0.0, 0.0);
    assert_plane(&p, [0.0, 0.0, 1.0], [0.0, 0.0, 10.0]);
    assert_eq!(how, "Plano de la circunferencia");
    // Normal al borde en un punto de él (tangente en y)
    let (p, how) = picked_plane(&mut doc, vec![rim.clone(), vertex([5.0, 0.0, 10.0])], 0.0, 0.0);
    assert_plane(&p, [0.0, 1.0, 0.0], [5.0, 0.0, 10.0]);
    assert_eq!(how, "Normal a la curva en el punto");

    // Eje de la pared y centro del borde
    let ax = doc.add(FeatureKind::Axis { def: AxisDef::Picked { refs: vec![wall] } });
    let pt = doc.add(FeatureKind::Point { def: PointSpec::Picked { refs: vec![rim] } });
    let ev = eval_ok(&doc);
    match ev.references.get(&ax) {
        Some(RefGeom::Axis { origin, dir }) => {
            assert!(norm(cross(normalize(*dir), [0.0, 0.0, 1.0])) < 1e-6);
            assert!(origin[0].abs() < 1e-6 && origin[1].abs() < 1e-6);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(ev.ref_kinds.get(&ax).map(String::as_str), Some("Eje de la cara"));
    match ev.references.get(&pt) {
        Some(RefGeom::Point { point }) => {
            for (k, v) in [0.0, 0.0, 10.0].iter().enumerate() {
                assert_relative_eq!(point[k], *v, epsilon = 1e-6);
            }
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(ev.ref_kinds.get(&pt).map(String::as_str), Some("Centro de la circunferencia"));
}

#[test]
fn axes_and_points_by_clicks() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let upright = RefPick::Edge { edge: edge_at(&doc, [10.0, 0.0, 5.0]) };
    let top = RefPick::Face { face: face_at(&doc, [5.0, 5.0, 10.0], [0.0, 0.0, 1.0]) };
    let side = RefPick::Face { face: face_at(&doc, [10.0, 5.0, 5.0], [1.0, 0.0, 0.0]) };
    let point = |doc: &mut Document, refs: Vec<RefPick>| {
        let id = doc.add(FeatureKind::Point { def: PointSpec::Picked { refs } });
        let ev = eval_ok(doc);
        let p = match ev.references.get(&id) {
            Some(RefGeom::Point { point }) => *point,
            other => panic!("{other:?}"),
        };
        let how = ev.ref_kinds.get(&id).cloned().unwrap_or_default();
        doc.features.pop();
        (p, how)
    };
    let near = |a: P3, b: P3| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-6);
    let (p, how) = point(&mut doc, vec![upright.clone()]);
    assert!(near(p, [10.0, 0.0, 5.0]) && how == "Medio de la arista", "{p:?} {how}");
    let (p, how) = point(&mut doc, vec![vertex([0.0; 3]), vertex([10.0, 10.0, 0.0])]);
    assert!(near(p, [5.0, 5.0, 0.0]) && how == "Punto medio");
    let (p, how) = point(&mut doc, vec![upright.clone(), RefPick::Plane { plane: PlaneSpec::Custom { plane: Plane { origin: [0.0, 0.0, 4.0], ..Plane::XY } } }]);
    assert!(near(p, [10.0, 0.0, 4.0]) && how == "Cruce de la arista con la cara", "{p:?} {how}");
    let (p, how) = point(&mut doc, vec![vertex([3.0, 3.0, 20.0]), top.clone()]);
    assert!(near(p, [3.0, 3.0, 10.0]) && how == "Proyección sobre el plano");

    // Ejes: cruce de dos caras y normal a una cara por un punto
    let ax = doc.add(FeatureKind::Axis { def: AxisDef::Picked { refs: vec![top.clone(), side] } });
    let ax2 = doc.add(FeatureKind::Axis { def: AxisDef::Picked { refs: vec![top, vertex([2.0, 2.0, 10.0])] } });
    let ev = eval_ok(&doc);
    match ev.references.get(&ax) {
        Some(RefGeom::Axis { origin, dir }) => {
            assert!(norm(cross(normalize(*dir), [0.0, 1.0, 0.0])) < 1e-6);
            assert!((origin[0] - 10.0).abs() < 1e-6 && (origin[2] - 10.0).abs() < 1e-6);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(ev.ref_kinds.get(&ax2).map(String::as_str), Some("Normal al plano por el punto"));
}

// ─── Curvas como operación ───

fn curve_lines(ev: &Evaluation, id: FeatureId) -> Vec<P3> {
    match ev.references.get(&id) {
        Some(RefGeom::Curves { lines }) => lines.concat(),
        other => panic!("sin curva: {other:?}"),
    }
}

fn tube_along(curve: FeatureId, r: f64) -> FeatureKind {
    FeatureKind::Sweep(Sweep {
        sketch: curve,
        regions: RegionSelection::All,
        path: SweepPath::Curve { feature: curve },
        op: BodyOp::New,
        orient: SweepOrient::Follow,
        circle: Some(r),
        thin: None,
    })
}

#[test]
fn curve_through_points_works_as_a_path() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let at = |p: P3| PointSpec::At { point: p };
    let c = doc.add(FeatureKind::Curve { def: CurveDef::Points { points: vec![at([0.0; 3]), at([10.0, 0.0, 0.0]), at([10.0, 10.0, 5.0])], closed: false } });
    let ev = eval_ok(&doc);
    let pts = curve_lines(&ev, c);
    let top = pts.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
    assert_relative_eq!(top, 5.0, epsilon = 1e-3);
    // Un tubo a lo largo de ella
    doc.add(tube_along(c, 1.0));
    let ev = eval_ok(&doc);
    assert!(ev.body.unwrap().mass().unwrap().volume > PI * 20.0);
    // Cerrada con dos puntos: error
    doc.get_mut(c).unwrap().kind = FeatureKind::Curve { def: CurveDef::Points { points: vec![at([0.0; 3]), at([10.0, 0.0, 0.0])], closed: true } };
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn composite_curve_of_two_edges() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let a = edge_at(&doc, [5.0, 0.0, 10.0]);
    let b = edge_at(&doc, [10.0, 5.0, 10.0]);
    let c = doc.add(FeatureKind::Curve { def: CurveDef::Composite { edges: vec![a, b], curves: vec![] } });
    let ev = eval_ok(&doc);
    let pts = curve_lines(&ev, c);
    // Las dos aristas de arriba: del frente y del costado, en z = 10
    assert!(pts.iter().all(|p| (p[2] - 10.0).abs() < 1e-6));
    assert!(pts.iter().any(|p| (p[0] - 10.0).abs() < 1e-6 && (p[1] - 10.0).abs() < 1e-6));
    assert!(pts.iter().any(|p| p[0].abs() < 1e-6 && p[1].abs() < 1e-6));
}

#[test]
fn projected_and_intersection_curves() {
    if !occt() {
        return;
    }
    // Un círculo de radio 2 a z = 30 proyectado sobre la cara de arriba de la caja (z = 10)
    let mut doc = Document::new();
    doc.add(cube(0.0, 10.0, BodyOp::Join));
    let top = face_at(&doc, [5.0, 5.0, 10.0], [0.0, 0.0, 1.0]);
    let mut s = Sketch::default();
    let o = s.add_point(5.0, 5.0);
    s.add_entity(Geometry::Circle { center: o, radius: 2.0 });
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 30.0, sketch: s });
    let c = doc.add(FeatureKind::Curve { def: CurveDef::Projected { sketch: sk, entities: vec![], faces: vec![top] } });
    let ev = eval_ok(&doc);
    let pts = curve_lines(&ev, c);
    assert!(pts.iter().all(|p| (p[2] - 10.0).abs() < 1e-6));
    assert!(pts.iter().all(|p| ((p[0] - 5.0).hypot(p[1] - 5.0) - 2.0).abs() < 1e-3));

    // Cruce de la pared de un cilindro (r 5) con la cara de arriba de una placa a z = 4
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 20.0, dy: 20.0, dz: 4.0, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
    }));
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 5.0, height: 10.0 },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
    }));
    let plate = face_at(&doc, [8.0, 8.0, 4.0], [0.0, 0.0, 1.0]);
    let wall = face_at(&doc, [5.0, 0.0, 8.0], [1.0, 0.0, 0.0]);
    let c = doc.add(FeatureKind::Curve { def: CurveDef::Intersection { a: vec![wall], b: vec![plate] } });
    let ev = eval_ok(&doc);
    let pts = curve_lines(&ev, c);
    assert!(!pts.is_empty());
    assert!(pts.iter().all(|p| (p[2] - 4.0).abs() < 1e-4), "fuera de z = 4");
    assert!(pts.iter().all(|p| (p[0].hypot(p[1]) - 5.0).abs() < 1e-3), "fuera del radio 5");
}
