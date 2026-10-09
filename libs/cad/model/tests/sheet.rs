//! Chapa metálica: chapa base, pestañas y desarrollo.

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

/// Rectángulo de `w` × `h` con una esquina en el origen
fn rect(w: f64, h: f64) -> Sketch {
    let mut s = Sketch::default();
    let p = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    for k in 0..4 {
        s.add_line(p[k], p[(k + 1) % 4]);
    }
    s
}

/// Chapa de 100 × 50 × 2 (radio 2, K 0,44) sobre el plano XY, z de 0 a 2
fn plate() -> Document {
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: rect(100.0, 50.0) });
    doc.add(FeatureKind::SheetMetal { sketch: sk, regions: RegionSelection::All, thickness: 2.0, radius: 2.0, k_factor: 0.44, flip: false, op: BodyOp::Join });
    doc
}

/// Pestaña en la arista de arriba de la chapa que pasa por `point` a lo largo de X
fn flange(doc: &mut Document, point: P3, length: f64, angle: f64, flip: bool) -> FeatureId {
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let (edge, _) = body.closest_edge(point, Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    let edge = ev.edge_ref(edge).unwrap();
    doc.add(FeatureKind::Flange { edge: Some(edge), length, angle, flip, radius: None, relief: BendRelief::Rectangle, relief_width: None, corner_relief: CornerRelief::Round, corner_size: None, closed_corner: false, corner_gap: None })
}

fn body(doc: &Document) -> Shape {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let b = ev.body.unwrap();
    assert!(b.is_valid());
    b
}

#[test]
fn u_channel_volume_and_flat_pattern() {
    if !occt() {
        return;
    }
    let mut doc = plate();
    assert_relative_eq!(body(&doc).mass().unwrap().volume, 10000.0, max_relative = 1e-9);
    // Dos pestañas de 20 hacia arriba, en los dos bordes largos
    flange(&mut doc, [50.0, 0.0, 2.0], 20.0, 90.0, false);
    flange(&mut doc, [50.0, 50.0, 2.0], 20.0, 90.0, false);
    let b = body(&doc);
    // Cada doblez: un cuarto de anillo de 2 a 4 por 100; cada pared 2 × 20 × 100
    let bend = PI / 4.0 * (16.0 - 4.0) * 100.0;
    assert_relative_eq!(b.mass().unwrap().volume, 10000.0 + 2.0 * (bend + 4000.0), max_relative = 1e-6);
    // Las paredes suben: arriba de la chapa (z hasta 2 + 2 + 20)
    let m = b.mass().unwrap();
    assert_relative_eq!(m.bbox_max[2], 24.0, epsilon = 1e-6);
    // Cada doblez sale R + t = 4 hacia afuera
    assert_relative_eq!(m.bbox_max[1] - m.bbox_min[1], 58.0, epsilon = 1e-6);
    // Desarrollo: 100 por 50 + 2 dobleces de π/2·(2 + 0,44·2) + 2 paredes de 20
    let flat = sheet::flat_pattern(&b, 0.44, Some(2.0), None).unwrap();
    let across = 50.0 + 2.0 * PI / 2.0 * (2.0 + 0.44 * 2.0) + 40.0;
    let size = [flat.max[0] - flat.min[0], flat.max[1] - flat.min[1]];
    let (long, short) = (size[0].max(size[1]), size[0].min(size[1]));
    assert_relative_eq!(long, 100.0, epsilon = 1e-6);
    assert_relative_eq!(short, across, epsilon = 1e-6);
    assert_eq!(flat.bends.len(), 2);
    for bd in &flat.bends {
        assert_relative_eq!(bd.angle, 90.0, epsilon = 1e-6);
        assert_relative_eq!(bd.radius, 2.0, epsilon = 1e-6);
        let l = ((bd.line[1][0] - bd.line[0][0]).powi(2) + (bd.line[1][1] - bd.line[0][1]).powi(2)).sqrt();
        assert_relative_eq!(l, 100.0, epsilon = 1e-6);
    }
    // El contorno es un rectángulo: su perímetro
    let perimeter: f64 = flat.outline.iter().map(|l| l.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum::<f64>()).sum();
    assert_relative_eq!(perimeter, 2.0 * (100.0 + across), epsilon = 1e-6);
}

#[test]
fn flange_down_at_an_angle_and_a_hole() {
    if !occt() {
        return;
    }
    let mut doc = plate();
    // Hacia abajo a 45°, de 30
    flange(&mut doc, [50.0, 0.0, 2.0], 30.0, 45.0, true);
    let b = body(&doc);
    let bend = 45f64.to_radians() / 2.0 * (16.0 - 4.0) * 100.0;
    assert_relative_eq!(b.mass().unwrap().volume, 10000.0 + bend + 6000.0, max_relative = 1e-6);
    assert!(b.mass().unwrap().bbox_min[2] < -15.0, "dobla hacia abajo");
    // Un agujero de Ø10 en la chapa
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: {
        let mut s = Sketch::default();
        let c = s.add_point(50.0, 25.0);
        s.add_entity(Geometry::Circle { center: c, radius: 5.0 });
        s
    } });
    doc.add(FeatureKind::Extrude(Extrude { sketch: sk, regions: RegionSelection::All, extent: Extent::ThroughAll, reverse: false, op: BodyOp::Cut, draft: 0.0, thin: None }));
    let b = body(&doc);
    let flat = sheet::flat_pattern(&b, 0.44, None, None).unwrap();
    assert_relative_eq!(flat.thickness, 2.0, epsilon = 1e-6);
    let across = 50.0 + 45f64.to_radians() * (2.0 + 0.44 * 2.0) + 30.0;
    let size = [flat.max[0] - flat.min[0], flat.max[1] - flat.min[1]];
    assert_relative_eq!(size[0].min(size[1]), across, epsilon = 1e-6);
    assert_eq!(flat.bends.len(), 1);
    assert_relative_eq!(flat.bends[0].angle, 45.0, epsilon = 1e-6);
    // El agujero: un tramo cerrado de perímetro π·10
    let closed = |l: &&Vec<P2>| l.len() > 2 && ((l[0][0] - l[l.len() - 1][0]).abs() + (l[0][1] - l[l.len() - 1][1]).abs()) < 1e-6;
    let hole: f64 = flat
        .outline
        .iter()
        .filter(closed)
        .map(|l| l.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum::<f64>())
        .sum();
    assert_relative_eq!(hole, PI * 10.0, max_relative = 2e-3);
}

#[test]
fn flange_on_a_flange() {
    if !occt() {
        return;
    }
    let mut doc = plate();
    flange(&mut doc, [50.0, 0.0, 2.0], 20.0, 90.0, false);
    // La punta de la pared (z = 24) tiene dos aristas largas; una pestaña hacia afuera de la U
    let ev = doc.evaluate();
    let b = ev.body.as_ref().unwrap();
    let (edge, _) = b.closest_edge([50.0, -4.0, 24.0], Some([1.0, 0.0, 0.0]), 0.99).unwrap();
    let edge = ev.edge_ref(edge).unwrap();
    doc.add(FeatureKind::Flange { edge: Some(edge), length: 10.0, angle: 90.0, flip: false, radius: None, relief: BendRelief::Rectangle, relief_width: None, corner_relief: CornerRelief::Round, corner_size: None, closed_corner: false, corner_gap: None });
    let b = body(&doc);
    let bend = PI / 4.0 * (16.0 - 4.0) * 100.0;
    assert_relative_eq!(b.mass().unwrap().volume, 10000.0 + 2.0 * bend + 4000.0 + 2000.0, max_relative = 1e-6);
    let flat = sheet::flat_pattern(&b, 0.44, Some(2.0), None).unwrap();
    assert_eq!(flat.bends.len(), 2);
    let across = 50.0 + 2.0 * PI / 2.0 * (2.0 + 0.88) + 20.0 + 10.0;
    let size = [flat.max[0] - flat.min[0], flat.max[1] - flat.min[1]];
    assert_relative_eq!(size[0].min(size[1]), across, epsilon = 1e-6);
}

/// Pestaña de 20 a 90° hacia arriba en la arista que pasa por `point` a lo largo de `dir`
fn flange_along(doc: &mut Document, point: P3, dir: P3, relief: BendRelief, corner_relief: CornerRelief) -> FeatureId {
    let ev = doc.evaluate();
    let body = ev.body.as_ref().unwrap();
    let (edge, _) = body.closest_edge(point, Some(dir), 0.99).unwrap();
    let edge = ev.edge_ref(edge).unwrap();
    doc.add(FeatureKind::Flange { edge: Some(edge), length: 20.0, angle: 90.0, flip: false, radius: None, relief, relief_width: None, corner_relief, corner_size: None, closed_corner: false, corner_gap: None })
}

/// Largo de los tramos del desarrollo
fn outline_length(flat: &sheet::FlatPattern) -> f64 {
    flat.outline.iter().map(|l| l.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum::<f64>()).sum()
}

#[test]
fn bend_relief_at_an_inside_corner() {
    if !occt() {
        return;
    }
    // L: 100 × 50 más un brazo de 50 × 30 que baja desde x = 50; la pestaña
    // va en el borde de y = 0 entre x = 0 y 50, y en x = 50 la chapa sigue
    let l_plate = || {
        let mut doc = Document::new();
        let mut s = Sketch::default();
        let pts: Vec<_> = [[0.0, 0.0], [50.0, 0.0], [50.0, -30.0], [100.0, -30.0], [100.0, 50.0], [0.0, 50.0]]
            .iter()
            .map(|q: &[f64; 2]| s.add_point(q[0], q[1]))
            .collect();
        for k in 0..pts.len() {
            s.add_line(pts[k], pts[(k + 1) % pts.len()]);
        }
        let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset: 0.0, sketch: s });
        doc.add(FeatureKind::SheetMetal { sketch: sk, regions: RegionSelection::All, thickness: 2.0, radius: 2.0, k_factor: 0.44, flip: false, op: BodyOp::Join });
        doc
    };
    let plate_v = (100.0 * 50.0 + 50.0 * 30.0) * 2.0;
    let bend = PI / 4.0 * (16.0 - 4.0) * 50.0;
    let wall = 2.0 * 20.0 * 50.0;
    for (relief, cut) in [
        // Ranura de 2 (el espesor) desde 1 adentro del canto hasta 1 pasado el doblez (R + t = 4)
        (BendRelief::Rectangle, 2.0 * 2.0 * 6.0),
        (BendRelief::Obround, 2.0 * 2.0 * 5.0 + PI / 2.0 * 2.0),
        (BendRelief::None, 0.0),
    ] {
        let mut doc = l_plate();
        flange_along(&mut doc, [25.0, 0.0, 2.0], [1.0, 0.0, 0.0], relief, CornerRelief::Round);
        let b = body(&doc);
        assert_relative_eq!(b.mass().unwrap().volume, plate_v + bend + wall - cut, max_relative = 1e-6);
        if relief != BendRelief::None {
            // Con la ranura, el desarrollo sale y el doblez no toca el brazo
            let flat = sheet::flat_pattern(&b, 0.44, Some(2.0), None).unwrap();
            assert_eq!(flat.bends.len(), 1);
            assert!(outline_length(&flat) > 0.0);
        }
    }
}

#[test]
fn corner_relief_where_two_flanges_meet() {
    if !occt() {
        return;
    }
    let tray = |corner: CornerRelief| {
        let mut doc = plate();
        flange_along(&mut doc, [50.0, 0.0, 2.0], [1.0, 0.0, 0.0], BendRelief::Rectangle, corner);
        flange_along(&mut doc, [0.0, 25.0, 2.0], [0.0, 1.0, 0.0], BendRelief::Rectangle, corner);
        doc
    };
    let bend = |len: f64| PI / 4.0 * (16.0 - 4.0) * len;
    let full = 10000.0 + bend(100.0) + bend(50.0) + 2.0 * 20.0 * 150.0;
    // Sin alivio los dobleces se tocan en una línea
    let open = body(&tray(CornerRelief::None));
    assert_relative_eq!(open.mass().unwrap().volume, full, max_relative = 1e-6);
    // Desarrollo: la chapa con las dos tiras (pared + doblez) y la muesca cuadrada en la esquina
    let strip = 20.0 + PI / 2.0 * (2.0 + 0.44 * 2.0);
    let perimeter = 2.0 * (100.0 + strip) + 2.0 * (50.0 + strip);
    let flat = sheet::flat_pattern(&open, 0.44, Some(2.0), None).unwrap();
    assert_eq!(flat.bends.len(), 2);
    assert_relative_eq!(outline_length(&flat), perimeter, epsilon = 1e-6);
    for corner in [CornerRelief::Round, CornerRelief::Square] {
        let doc = tray(corner);
        let b = body(&doc);
        let v = b.mass().unwrap().volume;
        // Lo que se va: la esquina de la chapa (un cuarto del alivio) y el arranque de los dos dobleces
        let quarter = match corner {
            CornerRelief::Round => PI * 4.0 / 4.0 * 2.0,
            _ => 4.0 * 2.0,
        };
        assert!(v < full - quarter + 1e-6, "{corner:?}: {v} contra {full}");
        assert!(v > full - 4.0 * quarter, "{corner:?}: {v}");
        // Los dobleces ya no se tocan: el desarrollo tiene los dos
        let flat = sheet::flat_pattern(&b, 0.44, Some(2.0), None).unwrap();
        assert_eq!(flat.bends.len(), 2, "{corner:?}");
        // El cuadrado de 4 × 4 en la esquina agrega dos lados hacia adentro
        if corner == CornerRelief::Square {
            assert_relative_eq!(outline_length(&flat), perimeter + 8.0, epsilon = 1e-6);
        } else {
            assert!(outline_length(&flat) > perimeter);
        }
        assert_eq!(doc.evaluate().bends.len(), 2);
    }
}


#[test]
fn closed_corner_between_two_flanges() {
    if !occt() {
        return;
    }
    // Bandeja: pestañas de 20 a 90° en y = 0 y en x = 0; la segunda cierra la esquina
    let mut doc = plate();
    flange_along(&mut doc, [50.0, 0.0, 2.0], [1.0, 0.0, 0.0], BendRelief::Rectangle, CornerRelief::None);
    let ev = doc.evaluate();
    let (edge, _) = ev.body.as_ref().unwrap().closest_edge([0.0, 25.0, 2.0], Some([0.0, 1.0, 0.0]), 0.99).unwrap();
    let edge = ev.edge_ref(edge).unwrap();
    doc.add(FeatureKind::Flange {
        edge: Some(edge),
        length: 20.0,
        angle: 90.0,
        flip: false,
        radius: None,
        relief: BendRelief::Rectangle,
        relief_width: None,
        corner_relief: CornerRelief::None,
        corner_size: None,
        closed_corner: true,
        corner_gap: Some(0.2),
    });
    let b = body(&doc);
    let bend = |len: f64| PI / 4.0 * (16.0 - 4.0) * len;
    let open = 10000.0 + bend(100.0) + bend(50.0) + 2.0 * 20.0 * 150.0;
    // Esta pared 2 × (R + t = 4) × 20 más; la otra 2 × (R − 0,2 = 1,8) × 20
    assert_relative_eq!(b.mass().unwrap().volume, open + 160.0 + 72.0, max_relative = 1e-6);
    // El canto de la primera queda tapado: la caja llega a y = −4 en x = −4
    let m = b.mass().unwrap();
    assert_relative_eq!(m.bbox_min[0], -4.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_min[1], -4.0, epsilon = 1e-6);
    // Y el desarrollo sigue saliendo con los dos dobleces
    let flat = sheet::flat_pattern(&b, 0.44, Some(2.0), None).unwrap();
    assert_eq!(flat.bends.len(), 2);
}
