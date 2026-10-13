//! Engranes coordinados: se ubican a la distancia justa y calzan sin chocar.

use approx::assert_relative_eq;
use cad_model::gear_part::{GearAxis, GearLink, GearPart};
use cad_model::gears::{GearKind, GearSpec};
use cad_model::*;
use std::f64::consts::PI;

fn prim(g: GearPart, op: BodyOp) -> FeatureKind {
    FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(g), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op, link: None })
}

fn gear(kind: GearKind, z: u32) -> GearPart {
    GearPart { spec: GearSpec { kind, teeth: z, module: 2.0, ..Default::default() }, width: 10.0, outer: if kind == GearKind::Internal { 80.0 } else { 6.0 }, ..Default::default() }
}

/// Dos engranes: el segundo engranado con el primero; devuelve los dos ejes calculados
fn pair(a: GearPart, b: GearPart, link: GearLink) -> (Evaluation, Vec<GearAxis>) {
    let mut doc = Document::new();
    let ia = doc.add(prim(a, BodyOp::New));
    let mut b = b;
    b.link = Some(GearLink { feature: ia, ..link });
    doc.add(prim(b, BodyOp::New));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let gears = ev.gears.clone();
    (ev, gears)
}

fn solid(ax: &GearAxis) -> Shape {
    let mut part = ax.part.clone();
    part.link = None;
    cad_model::gear_part::gear(occt::Frame { origin: ax.origin, z: ax.z, x: ax.x }, &part).unwrap()
}

fn overlap(a: &Shape, b: &Shape) -> f64 {
    match a.intersect(b) {
        Ok(s) => s.mass().map(|m| m.volume).unwrap_or(0.0),
        Err(_) => 0.0,
    }
}

fn link(angle: f64) -> GearLink {
    GearLink { feature: FeatureId(0), angle, offset: 0.0, distance: None }
}

#[test]
fn exterior_con_exterior_calza_en_cualquier_direccion() {
    if !occt::available() {
        return;
    }
    for (za, zb, angle) in [(20u32, 30u32, 0.0), (20, 30, 37.0), (24, 24, 200.0), (18, 41, -75.0)] {
        let (_, g) = pair(gear(GearKind::External, za), gear(GearKind::External, zb), link(angle));
        assert_eq!(g.len(), 2);
        let d = (g[1].origin[0].powi(2) + g[1].origin[1].powi(2)).sqrt();
        assert_relative_eq!(d, 2.0 * (za + zb) as f64 / 2.0, epsilon = 1e-9);
        assert_relative_eq!(g[1].origin[1].atan2(g[1].origin[0]).to_degrees().rem_euclid(360.0), (angle as f64).rem_euclid(360.0), epsilon = 1e-6);
        let (a, b) = (solid(&g[0]), solid(&g[1]));
        let tooth = a.mass().unwrap().volume / za as f64;
        assert!(overlap(&a, &b) < 1e-3 * tooth, "z={za}/{zb} a {angle}°: choque de {}", overlap(&a, &b));
        // Control: girado medio paso, los dientes chocan
        let mut off = g[1].clone();
        let h = PI / zb as f64;
        let (c, s) = ((h).cos(), (h).sin());
        off.x = [off.x[0] * c - off.x[1] * s, off.x[0] * s + off.x[1] * c, 0.0];
        assert!(overlap(&a, &solid(&off)) > 0.05 * tooth, "el control debe chocar");
    }
}

#[test]
fn copia_modulo_y_angulo() {
    let mut b = gear(GearKind::External, 30);
    b.spec.module = 5.0;
    b.spec.pressure_angle = 25.0;
    let (_, g) = pair(gear(GearKind::External, 20), b, link(0.0));
    assert_eq!(g[1].part.spec.module, 2.0);
    assert_eq!(g[1].part.spec.pressure_angle, 20.0);
    assert_relative_eq!(g[1].center_distance, 50.0, epsilon = 1e-9);
}

#[test]
fn distancia_pedida_ajusta_la_correccion() {
    if !occt::available() {
        return;
    }
    let (_, g) = pair(gear(GearKind::External, 20), gear(GearKind::External, 30), GearLink { distance: Some(51.0), ..link(0.0) });
    assert_relative_eq!(g[1].center_distance, 51.0, epsilon = 1e-9);
    assert!(g[1].part.spec.shift > 0.0);
    let (a, b) = (solid(&g[0]), solid(&g[1]));
    assert!(overlap(&a, &b) < 1e-3 * a.mass().unwrap().volume / 20.0);
    // Menos que el mínimo no existe
    let mut doc = Document::new();
    let ia = doc.add(prim(gear(GearKind::External, 20), BodyOp::New));
    let mut b = gear(GearKind::External, 30);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: Some(40.0) });
    doc.add(prim(b, BodyOp::New));
    assert!(!doc.evaluate().errors().is_empty());
}

#[test]
fn piñon_dentro_de_corona_y_al_reves() {
    if !occt::available() {
        return;
    }
    // Corona como engranada con el piñón
    let (_, g) = pair(gear(GearKind::External, 12), gear(GearKind::Internal, 30), link(30.0));
    assert_relative_eq!(g[1].center_distance, 18.0, epsilon = 1e-9);
    let (p, r) = (solid(&g[0]), solid(&g[1]));
    assert!(overlap(&p, &r) < 1e-3 * p.mass().unwrap().volume / 12.0, "choque {}", overlap(&p, &r));
    // Piñón engranado con una corona anterior
    let (_, g) = pair(gear(GearKind::Internal, 30), gear(GearKind::External, 12), link(110.0));
    assert_relative_eq!(g[1].center_distance, 18.0, epsilon = 1e-9);
    let (r, p) = (solid(&g[0]), solid(&g[1]));
    assert!(overlap(&p, &r) < 1e-3 * p.mass().unwrap().volume / 12.0, "choque {}", overlap(&p, &r));
}

#[test]
fn cremallera_con_piñon() {
    if !occt::available() {
        return;
    }
    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 9, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    for angle in [-90.0, -50.0, 0.0] {
        let (_, g) = pair(gear(GearKind::External, 20), rack.clone(), link(angle));
        let (p, r) = (solid(&g[0]), solid(&g[1]));
        assert!(overlap(&p, &r) < 1e-3 * p.mass().unwrap().volume / 20.0, "{angle}°: choque {}", overlap(&p, &r));
        assert_relative_eq!(g[1].center_distance, 20.0, epsilon = 1e-9);
    }
}

#[test]
fn cremallera_helicoidal_con_piñon_de_las_dos_manos() {
    if !occt::available() {
        return;
    }
    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 9, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    for helix in [20.0, -20.0] {
        let mut pinion = gear(GearKind::External, 20);
        pinion.spec.helix_angle = helix;
        let (_, g) = pair(pinion, rack.clone(), link(-30.0));
        assert_relative_eq!(g[1].part.spec.helix_angle, -helix, epsilon = 1e-9);
        let (p, r) = (solid(&g[0]), solid(&g[1]));
        let tooth = p.mass().unwrap().volume / 20.0;
        assert!(overlap(&p, &r) < 1e-3 * tooth, "hélice {helix}°: choque {}", overlap(&p, &r));
        // Control: la cremallera inclinada al otro lado choca
        let mut wrong = g[1].clone();
        wrong.part.spec.helix_angle = helix;
        assert!(overlap(&p, &solid(&wrong)) > 0.05 * tooth, "el control debe chocar");
    }
}

#[test]
fn errores_de_enlace() {
    let mut doc = Document::new();
    let ia = doc.add(prim(gear(GearKind::Internal, 50), BodyOp::New));
    let mut b = gear(GearKind::Internal, 40);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(b, BodyOp::New));
    if occt::available() {
        assert!(!doc.evaluate().errors().is_empty());
    }
    // Engranar con algo que viene después
    let mut doc = Document::new();
    let mut a = gear(GearKind::External, 20);
    a.link = Some(GearLink { feature: FeatureId(99), angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(a, BodyOp::New));
    if occt::available() {
        assert!(!doc.evaluate().errors().is_empty());
    }
}

#[test]
fn desplazamiento_axial() {
    if !occt::available() {
        return;
    }
    let (_, g) = pair(gear(GearKind::External, 20), gear(GearKind::External, 30), GearLink { offset: 4.0, ..link(0.0) });
    assert_relative_eq!(g[1].origin[2], 4.0, epsilon = 1e-9);
}

#[test]
fn helicoidales_calzan_con_mano_contraria() {
    if !occt::available() {
        return;
    }
    let helical = |z: u32, beta: f64, herring: bool| {
        let mut g = gear(GearKind::External, z);
        g.spec.helix_angle = beta;
        g.herringbone = herring;
        g
    };
    for (angle, herring) in [(0.0, false), (65.0, false), (20.0, true)] {
        let (_, g) = pair(helical(20, 18.0, herring), helical(28, 18.0, herring), link(angle));
        // La mano del segundo se invirtió sola
        assert_eq!(g[1].part.spec.helix_angle, -18.0);
        let (a, b) = (solid(&g[0]), solid(&g[1]));
        let tooth = a.mass().unwrap().volume / 20.0;
        let hit = overlap(&a, &b);
        assert!(hit < 5e-3 * tooth, "{angle}° espina={herring}: choque de {hit} (diente {tooth})");
        // Control: girado medio paso chocan
        let mut off = g[1].clone();
        let h = PI / 28.0;
        off.x = [off.x[0] * h.cos() - off.x[1] * h.sin(), off.x[0] * h.sin() + off.x[1] * h.cos(), 0.0];
        assert!(overlap(&a, &solid(&off)) > 0.05 * tooth, "el control debe chocar");
    }
}

#[test]
fn par_helicoidal_pide_manos_contrarias() {
    use cad_model::gears::pair;
    let mk = |z: u32, beta: f64, kind: GearKind| GearSpec { teeth: z, helix_angle: beta, kind, ..Default::default() };
    assert!(pair(&mk(20, 15.0, GearKind::External), &mk(30, 15.0, GearKind::External)).is_err());
    assert!(pair(&mk(20, 15.0, GearKind::External), &mk(30, -15.0, GearKind::External)).is_ok());
    assert!(pair(&mk(20, 15.0, GearKind::External), &mk(60, 15.0, GearKind::Internal)).is_ok());
    assert!(pair(&mk(20, 15.0, GearKind::External), &mk(60, -15.0, GearKind::Internal)).is_err());
}

fn bevel(z: u32, cone: f64, sigma: f64) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Bevel, teeth: z, module: 2.0, cone_angle: cone, shaft_angle: sigma, ..Default::default() }, width: 6.0, ..Default::default() }
}

#[test]
fn conicos_comparten_vertice_y_calzan() {
    if !occt::available() {
        return;
    }
    // (dientes a, dientes b, ángulo entre ejes, cono ideal de a, dirección)
    for (za, zb, sigma, angle) in [(20u32, 20u32, 90.0f64, 0.0), (16, 32, 90.0, 40.0), (18, 27, 75.0, -100.0)] {
        let ideal = f64::to_degrees(f64::atan2(f64::sin(sigma.to_radians()), zb as f64 / za as f64 + f64::cos(sigma.to_radians())));
        let (ev, g) = pair(bevel(za, ideal, 90.0), bevel(zb, 45.0, sigma), link(angle));
        assert!(ev.status.iter().all(|s| !matches!(s.state, FeatureState::Warning { .. })), "no debería avisar: {:?}", ev.status);
        let (a, b) = (&g[0], &g[1]);
        // Los vértices coinciden
        let apex = |ax: &GearAxis| {
            let r = ax.part.spec.module * ax.part.spec.teeth as f64 / 2.0;
            let h = r / ax.part.spec.cone_angle.to_radians().tan();
            [ax.origin[0] + ax.z[0] * h, ax.origin[1] + ax.z[1] * h, ax.origin[2] + ax.z[2] * h]
        };
        let (pa, pb) = (apex(a), apex(b));
        assert!(((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt() < 1e-9, "{pa:?} {pb:?}");
        // Los ejes forman el ángulo entre ejes
        let cos = a.z[0] * b.z[0] + a.z[1] * b.z[1] + a.z[2] * b.z[2];
        assert_relative_eq!(cos.acos().to_degrees(), sigma, epsilon = 1e-6);
        assert_relative_eq!(a.part.spec.cone_angle + b.part.spec.cone_angle, sigma, epsilon = 1e-9);
        let (sa, sb) = (solid(a), solid(b));
        let tooth = sa.mass().unwrap().volume / za as f64;
        let hit = overlap(&sa, &sb);
        assert!(hit < 8e-3 * tooth, "{za}/{zb} Σ={sigma}: choque de {hit} (diente {tooth})");
        // Control: el segundo girado medio paso choca
        let mut off = b.clone();
        let h = PI / zb as f64;
        let (c, s) = (h.cos(), h.sin());
        let y = [b.z[1] * b.x[2] - b.z[2] * b.x[1], b.z[2] * b.x[0] - b.z[0] * b.x[2], b.z[0] * b.x[1] - b.z[1] * b.x[0]];
        off.x = [b.x[0] * c + y[0] * s, b.x[1] * c + y[1] * s, b.x[2] * c + y[2] * s];
        assert!(overlap(&sa, &solid(&off)) > 0.05 * tooth, "el control debe chocar");
    }
}

#[test]
fn conico_con_cono_equivocado_avisa() {
    // 16/32 a 90° pide 26,57° en el primero; con 45° los conos no ruedan juntos
    let mut doc = Document::new();
    let ia = doc.add(prim(bevel(16, 45.0, 90.0), BodyOp::New));
    let mut b = bevel(32, 45.0, 90.0);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(b, BodyOp::New));
    let ev = doc.evaluate();
    if occt::available() {
        assert!(ev.status.iter().any(|s| matches!(s.state, FeatureState::Warning { .. })));
    }
    // Un cónico no engrana con un recto
    let mut doc = Document::new();
    let ia = doc.add(prim(gear(GearKind::External, 20), BodyOp::New));
    let mut b = bevel(20, 45.0, 90.0);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(b, BodyOp::New));
    if occt::available() {
        assert!(!doc.evaluate().errors().is_empty());
    }
}

fn worm(starts: u32, left: bool) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: starts, module: 2.0, left_hand: left, ..Default::default() }, width: 45.0, ..Default::default() }
}

#[test]
fn sin_fin_y_rueda_calzan() {
    if !occt::available() {
        return;
    }
    for (starts, left, z, angle) in [(1u32, false, 20u32, 0.0), (2, true, 24, 90.0)] {
        let mut wheel = gear(GearKind::External, z);
        wheel.width = 12.0;
        wheel.bore = 8.0;
        let (_, g) = pair(worm(starts, left), wheel, link(angle));
        let (w, r) = (&g[0], &g[1]);
        // Distancia = d1/2 + radio primitivo de la rueda (módulo axial 2)
        assert_relative_eq!(g[1].center_distance, 10.0 + 2.0 * z as f64 / 2.0, epsilon = 1e-9);
        // Ejes cruzados a 90°
        let dot = w.z[0] * r.z[0] + w.z[1] * r.z[1] + w.z[2] * r.z[2];
        assert!(dot.abs() < 1e-9);
        // La rueda toma la hélice del avance y la mano del sin fin
        let gamma = (starts as f64 * 2.0 / 20.0).atan().to_degrees();
        assert_relative_eq!(r.part.spec.helix_angle, if left { -gamma } else { gamma }, epsilon = 1e-9);
        assert!(r.part.throat > 0.0);
        let (sw, sr) = (solid(w), solid(r));
        let tooth = sr.mass().unwrap().volume / z as f64;
        let hit = overlap(&sw, &sr);
        assert!(hit < 0.02 * tooth, "{starts} entradas z={z}: choque de {hit} (diente {tooth})");
        // Control: la rueda girada medio paso choca
        let mut off = r.clone();
        let h = PI / z as f64;
        let y = [r.z[1] * r.x[2] - r.z[2] * r.x[1], r.z[2] * r.x[0] - r.z[0] * r.x[2], r.z[0] * r.x[1] - r.z[1] * r.x[0]];
        off.x = [r.x[0] * h.cos() + y[0] * h.sin(), r.x[1] * h.cos() + y[1] * h.sin(), r.x[2] * h.cos() + y[2] * h.sin()];
        assert!(overlap(&sw, &solid(&off)) > 0.1 * tooth, "el control debe chocar");
    }
}

#[test]
fn rueda_como_fuente_o_con_otro_tipo_falla() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let ia = doc.add(prim(gear(GearKind::External, 20), BodyOp::New));
    let mut b = worm(1, false);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(b, BodyOp::New));
    assert!(!doc.evaluate().errors().is_empty());
}
