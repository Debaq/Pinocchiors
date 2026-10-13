//! Pieza Engrane: sólidos válidos y volúmenes contra el perfil.

use approx::assert_relative_eq;
use cad_model::gear_part::{GearPart, info, keyway_for};
use cad_model::gears::{GearKind, GearSpec};
use cad_model::*;
use std::f64::consts::PI;

fn build(g: GearPart) -> Result<Shape, String> {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(g), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::Join, link: None }));
    let ev = doc.evaluate();
    if let Some(e) = ev.errors().first() {
        return Err(format!("{e:?}"));
    }
    Ok(ev.body.unwrap())
}

fn part(g: GearPart) -> Shape {
    let b = build(g).unwrap();
    assert!(b.is_valid());
    assert_eq!(b.solids().unwrap().len(), 1);
    b
}

fn vol(s: &Shape) -> f64 {
    s.mass().unwrap().volume
}

fn poly_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

fn spur(z: u32) -> GearPart {
    GearPart { spec: GearSpec { teeth: z, module: 2.0, ..Default::default() }, width: 10.0, ..Default::default() }
}

#[test]
fn volumen_igual_al_area_del_perfil() {
    if !occt::available() {
        return;
    }
    for z in [12u32, 24, 40] {
        let g = spur(z);
        let area = poly_area(&g.spec.profile().unwrap().points);
        // El polígono denso queda por dentro de la curva: tolerancia de cuerda
        assert_relative_eq!(vol(&part(g)), area * 10.0, max_relative = 2e-3);
    }
}

#[test]
fn agujero_cubo_y_chavetero() {
    if !occt::available() {
        return;
    }
    let base = vol(&part(spur(30)));
    let con_agujero = vol(&part(GearPart { bore: 10.0, ..spur(30) }));
    assert_relative_eq!(base - con_agujero, PI * 25.0 * 10.0, max_relative = 1e-6);
    let con_cubo = vol(&part(GearPart { bore: 10.0, hub_diameter: 20.0, hub_length: 6.0, ..spur(30) }));
    assert_relative_eq!(con_cubo - con_agujero, PI * (100.0 - 25.0) * 6.0, max_relative = 1e-6);
    let con_chaveta = vol(&part(GearPart { bore: 10.0, hub_diameter: 20.0, hub_length: 6.0, keyway: true, ..spur(30) }));
    // 4 mm de ancho, 1,8 mm de hondo sobre el radio, 16 mm de largo (el segmento
    // circular del agujero ya estaba vacío: se quita del cubo solo lo que sobra)
    let (b, t2) = keyway_for(10.0).unwrap();
    assert_eq!((b, t2), (3.0, 1.4));
    let quitado = con_cubo - con_chaveta;
    assert!(quitado > 0.9 * b * t2 * 16.0 && quitado < 1.1 * b * (t2 + 0.2) * 16.0, "{quitado}");
}

#[test]
fn aligeramientos() {
    if !occt::available() {
        return;
    }
    let base = vol(&part(GearPart { bore: 10.0, ..spur(40) }));
    let g = GearPart { bore: 10.0, hole_count: 6, hole_diameter: 8.0, hole_circle: 44.0, ..spur(40) };
    assert_relative_eq!(base - vol(&part(g)), 6.0 * PI * 16.0 * 10.0, max_relative = 1e-6);
    assert!(build(GearPart { bore: 10.0, hole_count: 6, hole_diameter: 8.0, hole_circle: 70.0, ..spur(40) }).is_err());
}

#[test]
fn corona_y_cremallera() {
    if !occt::available() {
        return;
    }
    let ring = GearPart { spec: GearSpec { kind: GearKind::Internal, teeth: 40, module: 2.0, ..Default::default() }, width: 10.0, outer: 100.0, ..Default::default() };
    let v = vol(&part(ring.clone()));
    let area = PI * 50.0 * 50.0 - poly_area(&ring.spec.profile().unwrap().points);
    assert_relative_eq!(v, area * 10.0, max_relative = 2e-3);
    assert!(build(GearPart { outer: 80.0, ..ring }).is_err());

    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 5, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    let area = poly_area(&rack.spec.rack_outline(5, 6.0).unwrap());
    assert_relative_eq!(vol(&part(rack)), area.abs() * 10.0, max_relative = 1e-6);
}

#[test]
fn holgura_de_impresion_quita_material() {
    if !occt::available() {
        return;
    }
    let a = vol(&part(spur(24)));
    let b = vol(&part(GearPart { print_clearance: 0.2, ..spur(24) }));
    assert!(b < a);
}

#[test]
fn avisos_y_errores() {
    let i = info(&spur(8));
    assert!(i.warnings.iter().any(|w| w.contains("Socavado")));
    assert_relative_eq!(i.pitch_diameter, 16.0);
    assert!(info(&GearPart { width: 0.0, ..spur(20) }).error.is_some());
    assert!(info(&spur(30)).error.is_none());
    if occt::available() {
        let mut g = spur(20);
        g.spec.kind = GearKind::Rack;
        g.spec.helix_angle = 15.0;
        g.outer = 6.0;
        assert!(build(g).is_err());
        assert!(build(GearPart { bore: 40.0, ..spur(20) }).is_err());
        assert!(build(GearPart { bore: 5.0, keyway: true, ..spur(20) }).is_err());
    }
}

#[test]
fn json_de_la_interfaz() {
    // Lo que escribe el front: tipo «gear» con las medidas al mismo nivel
    let shape: PrimitiveShape = serde_json::from_value(serde_json::json!({
        "type": "gear",
        "spec": { "kind": "internal", "module": 2.0, "teeth": 40, "pressure_angle": 20.0 },
        "width": 10.0, "outer": 100.0
    }))
    .unwrap();
    let PrimitiveShape::Gear(g) = &shape else { panic!() };
    assert_eq!(g.spec.kind, GearKind::Internal);
    assert_eq!(g.spec.dedendum, 1.25);
    let back = serde_json::to_value(&shape).unwrap();
    assert_eq!(back["type"], "gear");
    assert_eq!(back["spec"]["kind"], "internal");
    assert_eq!(back["width"], 10.0);
}

fn helical(z: u32, beta: f64) -> GearPart {
    let mut g = spur(z);
    g.spec.helix_angle = beta;
    g
}

#[test]
fn helicoidal_vale_area_por_ancho_y_gira_a_la_derecha() {
    if !occt::available() {
        return;
    }
    for beta in [20.0, -20.0] {
        let g = helical(24, beta);
        let c = g.spec.circles().unwrap();
        let area = poly_area(&g.spec.profile().unwrap().points);
        let b = part(g.clone());
        assert_relative_eq!(vol(&b), area * 10.0, max_relative = 3e-3);
        // Arriba el diente quedó girado w·tanβ/r: coincide con el engrane recto girado así
        let turn = 10.0 * beta.to_radians().tan() / c.pitch;
        let top = |a: f64| {
            let mut s = spur(24);
            s.width = 0.5;
            let frame = occt::Frame { origin: [0.0, 0.0, 9.5], z: [0.0, 0.0, 1.0], x: [a.cos(), a.sin(), 0.0] };
            cad_model::gear_part::gear(frame, &s).unwrap()
        };
        let slab = b.intersect(&top(0.0).clone()).ok();
        let _ = slab;
        let hit = |a: f64| top(a).intersect(&b).ok().and_then(|x| x.mass().ok()).map(|m| m.volume).unwrap_or(0.0);
        assert!(hit(turn) > hit(-turn), "β={beta}: el giro va al revés ({} vs {})", hit(turn), hit(-turn));
    }
}

#[test]
fn espina_de_pescado() {
    if !occt::available() {
        return;
    }
    let area = |g: &GearPart| poly_area(&g.spec.profile().unwrap().points);
    let g = GearPart { herringbone: true, ..helical(24, 20.0) };
    assert_relative_eq!(vol(&part(g.clone())), area(&g) * 10.0, max_relative = 3e-3);
    // Con canal: sale el volumen de las dos mitades más el núcleo del canal
    let c = GearPart { herringbone: true, groove: 2.0, ..helical(24, 20.0) };
    let root = c.spec.circles().unwrap().root;
    let esperado = area(&c) * 8.0 + PI * root * root * 2.0;
    assert_relative_eq!(vol(&part(c.clone())), esperado, max_relative = 4e-3);
    assert!(build(GearPart { groove: 2.0, ..helical(24, 0.0) }).is_ok());
    assert!(build(GearPart { herringbone: true, ..helical(24, 0.0) }).is_err());
}

#[test]
fn corona_helicoidal_y_cremallera() {
    if !occt::available() {
        return;
    }
    let mut ring = GearPart { spec: GearSpec { kind: GearKind::Internal, teeth: 40, module: 2.0, helix_angle: 15.0, ..Default::default() }, width: 10.0, outer: 100.0, ..Default::default() };
    let area = PI * 2500.0 - poly_area(&ring.spec.profile().unwrap().points);
    assert_relative_eq!(vol(&part(ring.clone())), area * 10.0, max_relative = 4e-3);
    ring.spec.kind = GearKind::Rack;
    assert!(build(ring).is_err());
}

fn bevel(z: u32, cone: f64) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Bevel, teeth: z, module: 2.0, cone_angle: cone, ..Default::default() }, width: 8.0, ..Default::default() }
}

#[test]
fn conico_es_un_solido_valido_de_tamano_razonable() {
    if !occt::available() {
        return;
    }
    for (z, cone) in [(20u32, 45.0), (24, 30.0), (30, 60.0)] {
        let g = bevel(z, cone);
        let b = part(g.clone());
        // Contra el tronco de cono del primitivo (los dientes quitan algo, no mucho)
        let d = f64::to_radians(cone);
        let r = z as f64;
        let (h, zt) = (r / d.tan(), 8.0 * d.cos());
        let k = (h - zt) / h;
        let reference = PI * zt / 3.0 * (r * r + r * r * k + r * r * k * k);
        let ratio = vol(&b) / reference;
        assert!(ratio > 0.8 && ratio < 1.05, "z={z} δ={cone}: {ratio}");
    }
    // Con agujero, cubo y chavetero
    let g = GearPart { bore: 10.0, hub_diameter: 18.0, hub_length: 6.0, keyway: true, ..bevel(24, 45.0) };
    let con = part(g.clone());
    assert!(vol(&con) > 0.0);
    // El ancho de cara no puede pasar de la generatriz
    assert!(build(GearPart { width: 40.0, ..bevel(24, 45.0) }).is_err());
    assert!(build(GearPart { spec: GearSpec { helix_angle: 10.0, ..bevel(24, 45.0).spec }, ..bevel(24, 45.0) }).is_err());
}

fn worm(starts: u32, left: bool) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: starts, module: 2.0, left_hand: left, ..Default::default() }, width: 45.0, ..Default::default() }
}

#[test]
fn sin_fin_es_un_solido_valido_con_el_volumen_del_perfil() {
    if !occt::available() {
        return;
    }
    for (starts, left) in [(1u32, false), (2, true), (3, false)] {
        let g = worm(starts, left);
        let (r_root, r_tip, hw_in, hw_out) = g.spec.worm_profile().unwrap();
        let (_, gamma, lead) = g.spec.worm_dims();
        assert_relative_eq!(gamma.tan(), starts as f64 * 2.0 / 20.0, epsilon = 1e-12);
        let b = part(g.clone());
        // Núcleo + filetes: la sección axial de un filete (trapecio) girada alrededor del eje (Pappus)
        // aproximada con el radio del centro; `starts` filetes por avance, largo 45
        let h = r_tip - r_root;
        let area = (hw_in + hw_out) * h;
        let r_c = r_root + h * (hw_in + 2.0 * hw_out) / (3.0 * (hw_in + hw_out));
        let per_mm = PI * r_root * r_root + area * 2.0 * PI * r_c / (lead / starts as f64);
        assert_relative_eq!(vol(&b), per_mm * 45.0, max_relative = 0.05);
    }
    // Con agujero y chavetero
    let g = GearPart { bore: 8.0, keyway: true, ..worm(1, false) };
    let con = vol(&part(g));
    assert!(con < vol(&part(worm(1, false))));
    assert!(build(GearPart { width: 10.0, ..worm(2, false) }).is_err());
    assert!(build(GearPart { hub_length: 4.0, hub_diameter: 12.0, ..worm(1, false) }).is_err());
}

#[test]
fn cotas_de_medicion_del_engrane() {
    use cad_model::gear_part::measurements;
    // m=1, z=20, 20°: W3 = 7,6604 (tabla); m=2 lo duplica
    let (k, w, _, _) = measurements(&GearSpec { teeth: 20, module: 1.0, ..Default::default() }).unwrap();
    assert_eq!(k, 3);
    assert_relative_eq!(w, 7.6604, epsilon = 2e-4);
    let (_, w2, d, m_pins) = measurements(&GearSpec { teeth: 20, module: 2.0, ..Default::default() }).unwrap();
    assert_relative_eq!(w2, 2.0 * 7.6604, epsilon = 4e-4);
    // Sobre bolas (par): el círculo base + bola; con un diente más (impar) cambia poco
    assert!(m_pins > 2.0 * 20.0 * 0.94 && m_pins < 2.0 * 20.0 + d * 1.5, "{m_pins}");
    let (_, _, _, odd) = measurements(&GearSpec { teeth: 21, module: 2.0, ..Default::default() }).unwrap();
    assert!(odd > m_pins && odd < m_pins + 2.2 * 2.0);
    // Con corrección de perfil la cota sobre dientes crece 2·x·m·sen α
    let (_, wx, ..) = measurements(&GearSpec { teeth: 20, module: 2.0, shift: 0.5, ..Default::default() }).unwrap();
    assert_relative_eq!(wx - w2, 2.0 * 0.5 * 2.0 * 20f64.to_radians().sin(), epsilon = 1e-9);
    assert!(measurements(&GearSpec { kind: GearKind::Internal, teeth: 40, ..Default::default() }).is_none());
    let i = info(&spur(20));
    assert_eq!(i.span_teeth, 3);
}

#[test]
fn perfil_del_engrane_como_bloque_del_sketch() {
    use cad_model::regions::find_regions;
    use cad_model::sketch::{Geometry, Sketch};
    for g in [spur(18), GearPart { spec: GearSpec { kind: GearKind::Bevel, teeth: 20, module: 2.0, ..Default::default() }, width: 6.0, ..Default::default() }] {
        let block = cad_model::gear_part::sketch_block(&g).unwrap();
        assert_eq!(block.points[0].id, 0);
        assert_eq!(block.entities.len(), 7 * g.spec.teeth as usize);
        let ids: std::collections::HashSet<u32> = block.points.iter().map(|p| p.id).collect();
        for e in &block.entities {
            assert!(!ids.contains(&e.id));
            assert!(e.geometry.point_ids().iter().all(|p| ids.contains(p)));
            if let Geometry::Arc { center, .. } = e.geometry {
                assert_eq!(center, 0);
            }
        }
        // Armado como sketch cierra un solo perfil con el área del engrane
        let s = Sketch { points: block.points.clone(), entities: block.entities.clone(), constraints: vec![], next_id: 100_000, ..Default::default() };
        let regions = find_regions(&s).unwrap();
        assert_eq!(regions.len(), 1, "{} regiones", regions.len());
        let area = poly_area(&g.spec.profile().unwrap().points);
        let got = poly_area(&regions[0].outer.polygon);
        assert_relative_eq!(got.abs(), area.abs(), max_relative = 4e-3);
    }
    // La cremallera y el sin fin
    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 4, module: 2.0, ..Default::default() }, outer: 6.0, ..Default::default() };
    assert!(cad_model::gear_part::sketch_block(&rack).unwrap().entities.iter().all(|e| matches!(e.geometry, Geometry::Line { .. })));
    assert!(cad_model::gear_part::sketch_block(&worm(1, false)).is_err());
}

#[test]
fn worm_efficiency_locks_at_small_lead_angle() {
    use cad_model::gear_part::worm_efficiency;
    let a = 20f64.to_radians();
    let (eta, locks) = worm_efficiency(2f64.to_radians(), a);
    assert!(locks && eta < 0.4, "{eta}");
    let (eta, locks) = worm_efficiency(25f64.to_radians(), a);
    assert!(!locks && eta > 0.7, "{eta}");
}

#[test]
fn design_teeth_by_ratio_and_distance() {
    use cad_model::gear_part::design_teeth;
    let pinion = GearPart { spec: GearSpec { teeth: 20, module: 2.0, ..Default::default() }, ..Default::default() };
    // Relación 3:1 → 60 dientes
    assert_eq!(design_teeth(&pinion, GearKind::External, Some(3.0), None), Ok(60));
    // Distancia 60 mm con m=2: z1+z2 = 60 → 40 dientes
    assert_eq!(design_teeth(&pinion, GearKind::External, None, Some(60.0)), Ok(40));
    // Corona a 20 mm del piñón: z2−z1 = 20 → 40
    assert_eq!(design_teeth(&pinion, GearKind::Internal, None, Some(20.0)), Ok(40));
    // Helicoidal: el módulo transversal manda
    let mut h = pinion.clone();
    h.spec.helix_angle = 30.0;
    let z = design_teeth(&h, GearKind::External, None, Some(60.0)).unwrap();
    assert_eq!(z, (2.0 * 60.0 * 30f64.to_radians().cos() / 2.0).round() as u32 - 20);
    // Piñón de una corona
    let ring = GearPart { spec: GearSpec { kind: GearKind::Internal, teeth: 60, module: 2.0, ..Default::default() }, outer: 140.0, ..Default::default() };
    assert_eq!(design_teeth(&ring, GearKind::External, None, Some(40.0)), Ok(20));
    assert!(design_teeth(&ring, GearKind::External, Some(2.0), None).is_err());
    assert!(design_teeth(&pinion, GearKind::External, None, None).is_err());
    assert!(design_teeth(&pinion, GearKind::Rack, Some(2.0), None).is_err());
}
