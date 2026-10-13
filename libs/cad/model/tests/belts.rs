//! Poleas dentadas, piñones de cadena y dientes cicloidales.

use approx::assert_relative_eq;
use cad_model::belts::{belt, belt_length, chain, chain_length, default_width};
use cad_model::gear_part::{GearPart, info};
use cad_model::gears::{GearKind, GearSpec, ToothForm, pair};
use cad_model::*;
use std::f64::consts::PI;

fn pulley(form: ToothForm, z: u32) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Pulley, form, teeth: z, ..Default::default() }, width: default_width(form), ..Default::default() }
}

fn sprocket(form: ToothForm, z: u32) -> GearPart {
    GearPart { spec: GearSpec { kind: GearKind::Sprocket, form, teeth: z, ..Default::default() }, width: default_width(form), ..Default::default() }
}

fn poly_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

fn build(g: GearPart) -> Result<Shape, String> {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(g), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None }));
    let ev = doc.evaluate();
    if let Some(e) = ev.errors().first() {
        return Err(format!("{e:?}"));
    }
    Ok(ev.body.unwrap())
}

#[test]
fn polea_gt2_tiene_el_diametro_exterior_de_la_tabla() {
    // GT2 2 mm, 20 dientes: primitivo 12,732; exterior 12,732 − 2·0,254 = 12,224
    let g = pulley(ToothForm::Gt2Mm2, 20);
    let i = info(&g);
    assert!(i.error.is_none(), "{:?}", i.error);
    assert_relative_eq!(i.pitch_diameter, 20.0 * 2.0 / PI, epsilon = 1e-9);
    assert_relative_eq!(i.tip_diameter, 20.0 * 2.0 / PI - 0.508, epsilon = 1e-9);
    // El pie está una profundidad de ranura más abajo (0,764)
    assert_relative_eq!(i.tip_diameter - i.root_diameter, 2.0 * 0.764, epsilon = 1e-9);
    // Todas las correas tienen su paso y las ranuras entran
    for f in ToothForm::BELTS {
        let b = belt(*f).unwrap();
        let g = pulley(*f, 24);
        let p = g.spec.profile().unwrap();
        let area = poly_area(&p.points);
        let r = p.circles.tip;
        let full = PI * r * r;
        assert!(area < full && area > PI * p.circles.root.powi(2), "{}: área {area} fuera de [{}, {full}]", b.name, PI * p.circles.root.powi(2));
        // 24 ranuras: la ranura de la tabla × 24 es lo que falta (aprox. por el área de un segmento)
        let groove = (full - area) / 24.0;
        assert!(groove > 0.3 * b.width * b.depth && groove < 1.2 * b.width * b.depth, "{}: ranura de {groove} mm²", b.name);
    }
}

#[test]
fn polea_solida_valida_con_cubo_bridas_y_agujero() {
    if !occt::available() {
        return;
    }
    let base = pulley(ToothForm::Gt2Mm2, 20);
    let plain = build(base.clone()).unwrap();
    assert!(plain.is_valid() && plain.solids().unwrap().len() == 1);
    let area = poly_area(&base.spec.profile().unwrap().points);
    assert_relative_eq!(plain.mass().unwrap().volume, area * base.width, max_relative = 5e-3);
    // Con agujero, cubo, bridas y prisionero
    let full = GearPart { bore: 5.0, hub_diameter: 12.0, hub_length: 6.0, set_screw: 3.0, flange: 16.0, ..base };
    let s = build(full.clone()).unwrap();
    assert!(s.is_valid() && s.solids().unwrap().len() == 1);
    let b = s.bounds().unwrap();
    // 1,2 mm de brida a cada lado del canal de 6 mm, y 6 mm de cubo arriba
    assert_relative_eq!(b.1[2] - b.0[2], 1.2 + 6.0 + 1.2 + 6.0, epsilon = 1e-6);
    assert!(b.1[0] - b.0[0] >= 16.0 - 1e-6);
    // La brida tiene que pasar el exterior de la polea
    assert!(build(GearPart { flange: 12.0, ..pulley(ToothForm::Gt2Mm2, 20) }).is_err());
    // Y demasiado pocos dientes para la correa se avisa
    assert!(info(&pulley(ToothForm::Htd5M, 6)).error.is_some());
}

#[test]
fn pinon_de_cadena_el_rodillo_asienta_en_el_diente() {
    let ch = chain(ToothForm::Chain35).unwrap();
    for z in [11u32, 17, 25] {
        let g = sprocket(ToothForm::Chain35, z);
        let i = info(&g);
        assert!(i.error.is_none(), "z={z}: {:?}", i.error);
        // Primitivo p / sen(180°/z)
        assert_relative_eq!(i.pitch_diameter, ch.pitch / (PI / z as f64).sin(), epsilon = 1e-9);
        let p = g.spec.profile().unwrap();
        // El rodillo (d₁) con el centro en el primitivo, en el asiento 0, toca el contorno sin pisarlo
        let c = [p.circles.pitch, 0.0];
        let min = p
            .points
            .iter()
            .map(|q| (q[0] - c[0]).hypot(q[1] - c[1]))
            .fold(f64::MAX, f64::min);
        assert!(min >= ch.roller / 2.0 - 1e-3, "z={z}: el rodillo pisa el diente ({min} < {})", ch.roller / 2.0);
        assert!(min <= 0.505 * ch.roller + 0.02, "z={z}: el asiento queda lejos del rodillo ({min})");
        // Cabeza D_a = p(0,6 + cot(180°/z))
        assert_relative_eq!(2.0 * p.circles.tip, ch.pitch * (0.6 + 1.0 / (PI / z as f64).tan()), epsilon = 1e-9);
        // El área del contorno es la de un disco con z huecos
        let area = poly_area(&p.points);
        assert!(area > PI * p.circles.root.powi(2) && area < PI * p.circles.tip.powi(2));
    }
    for f in ToothForm::CHAINS {
        let g = sprocket(*f, 19);
        assert!(info(&g).error.is_none(), "{:?}: {:?}", f, info(&g).error);
    }
    // Menos de 9 dientes no es un piñón
    assert!(info(&sprocket(ToothForm::Chain35, 7)).error.is_some());
}

#[test]
fn pinon_de_cadena_solido_valido() {
    if !occt::available() {
        return;
    }
    let g = GearPart { bore: 8.0, keyway: true, hub_diameter: 18.0, hub_length: 5.0, ..sprocket(ToothForm::Chain40, 15) };
    let s = build(g.clone()).unwrap();
    assert!(s.is_valid() && s.solids().unwrap().len() == 1);
    let area = poly_area(&GearPart { bore: 0.0, hub_length: 0.0, ..g.clone() }.spec.profile().unwrap().points);
    // Más que el hueco del agujero y menos que el disco entero
    let v = s.mass().unwrap().volume;
    assert!(v < area * g.width + PI * 81.0 * 5.0 && v > 0.5 * area * g.width);
    // Ni polea ni piñón se engranan entre sí
    let mut doc = Document::new();
    let ia = doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(g.clone()), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None }));
    let mut b = pulley(ToothForm::Gt2Mm2, 20);
    b.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
    doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(b), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None }));
    let errors = format!("{:?}", doc.evaluate().errors());
    assert!(errors.contains("correa o una cadena"), "{errors}");
}

#[test]
fn largo_de_correa_y_de_cadena() {
    // GT2 2 mm, 20/20 dientes a 100 mm: 120 dientes (240 mm) y centros que valen justo eso
    let r = belt_length(ToothForm::Gt2Mm2, 20, 20, 100.0).unwrap();
    assert_eq!(r.count, ((2.0 * 100.0 + PI * 20.0 * 2.0 / PI) / 2.0).round() as u32);
    assert_relative_eq!(r.length, r.count as f64 * 2.0, epsilon = 1e-9);
    // Poleas distintas: el largo entero vuelve a dar su distancia entre centros (la fórmula inversa)
    let d = |z: f64| z * 2.0 / PI;
    let r = belt_length(ToothForm::Gt2Mm2, 16, 40, 120.0).unwrap();
    let l = 2.0 * r.center_distance + PI * (d(16.0) + d(40.0)) / 2.0 + (d(40.0) - d(16.0)).powi(2) / (4.0 * r.center_distance);
    assert_relative_eq!(l, r.length, epsilon = 1e-9);
    assert!((r.center_distance - 120.0).abs() < 1.0);
    // Cadena ANSI 40 (12,7 mm), 17/34 dientes a 400 mm: número par de eslabones
    let c = chain_length(ToothForm::Chain40, 17, 34, 400.0).unwrap();
    assert_eq!(c.count % 2, 0);
    let k = ((34.0 - 17.0) / (2.0 * PI)).powi(2);
    let links = 2.0 * c.center_distance / 12.7 + (17.0 + 34.0) / 2.0 + k * 12.7 / c.center_distance;
    assert_relative_eq!(links, c.count as f64, epsilon = 1e-9);
    // Avisos y errores
    assert!(chain_length(ToothForm::Chain40, 11, 34, 400.0).unwrap().notes.iter().any(|n| n.contains("17")));
    assert!(belt_length(ToothForm::Gt2Mm2, 20, 20, 5.0).is_err());
    assert!(belt_length(ToothForm::Chain40, 20, 20, 100.0).is_err());
    let wrap = belt_length(ToothForm::Gt2Mm2, 10, 80, 60.0);
    assert!(wrap.map_or(true, |w| w.notes.iter().any(|n| n.contains("abraza"))));
}

fn cycloidal(z: u32) -> GearPart {
    GearPart { spec: GearSpec { teeth: z, module: 2.0, form: ToothForm::Cycloidal, ..Default::default() }, width: 8.0, ..Default::default() }
}

#[test]
fn diente_cicloidal_pasa_por_el_primitivo_y_cierra() {
    let g = cycloidal(24);
    let p = g.spec.profile().unwrap();
    let r = 2.0 * 24.0 / 2.0;
    assert_relative_eq!(p.circles.pitch, r, epsilon = 1e-9);
    // Los flancos cruzan el círculo primitivo a ±s/(2r) del eje del diente: el espesor de πm/2
    let s = PI * 2.0 / 2.0;
    // Los vértices del perfil que caen sobre el primitivo: el punto donde se unen hipocicloide y epicicloide
    let mut on_pitch: Vec<f64> = p.points.iter().filter(|q| (q[0].hypot(q[1]) - r).abs() < 1e-9).map(|q| q[1].atan2(q[0])).collect();
    on_pitch.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(on_pitch.len(), 2 * 24);
    // El hueco centrado en 0: los cruces vecinos a 0 son ±(π/z − s/(2r)·…) → su separación es el hueco
    let tooth_angle = |a: f64, b: f64| ((b - a).rem_euclid(2.0 * PI)) * r;
    let gaps: Vec<f64> = (0..on_pitch.len()).map(|i| tooth_angle(on_pitch[i], on_pitch[(i + 1) % on_pitch.len()])).collect();
    let sum: f64 = gaps.iter().sum();
    assert_relative_eq!(sum, 2.0 * PI * r, epsilon = 1e-6 * r);
    assert!(gaps.iter().any(|x| (x - s).abs() < 0.05), "no hay un diente de {s} mm: {gaps:?}");
    // Cabeza y pie
    assert_relative_eq!(p.circles.tip, r + 2.0, epsilon = 1e-9);
    assert!(poly_area(&p.points) > PI * p.circles.root.powi(2));
    // Círculo generador muy chico para la altura: error claro
    let bad = GearSpec { cycloid_circle: 0.3, ..g.spec };
    assert!(bad.profile().unwrap_err().to_string().contains("generador"));
    // Con hélice o corrección, no
    assert!(GearSpec { helix_angle: 10.0, ..g.spec }.profile().is_err());
    assert!(GearSpec { shift: 0.2, ..g.spec }.profile().is_err());
}

#[test]
fn par_cicloidal_calza_sin_chocar() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    let ia = doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(cycloidal(18)), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None }));
    let mut b = cycloidal(30);
    b.link = Some(GearLink { feature: ia, angle: 40.0, offset: 0.0, distance: None, along: 0.0 });
    doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(b), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let g = &ev.gears;
    // Centros: m(z₁ + z₂)/2 = 48 mm
    assert_relative_eq!(g[1].center_distance, 48.0, epsilon = 1e-9);
    let solid = |ax: &GearAxis| {
        let mut part = ax.part.clone();
        part.link = None;
        cad_model::gear_part::gear(occt::Frame { origin: ax.origin, z: ax.z, x: ax.x }, &part).unwrap()
    };
    let (sa, sb) = (solid(&g[0]), solid(&g[1]));
    let tooth = sa.mass().unwrap().volume / 18.0;
    let hit = |a: &Shape, b: &Shape| a.intersect(b).ok().and_then(|s| s.mass().ok()).map_or(0.0, |m| m.volume);
    assert!(hit(&sa, &sb) < 2e-2 * tooth, "choque {} (diente {tooth})", hit(&sa, &sb));
    // Control: el segundo girado medio paso choca
    let mut off = g[1].clone();
    let h = PI / 30.0;
    let y = [off.z[1] * off.x[2] - off.z[2] * off.x[1], off.z[2] * off.x[0] - off.z[0] * off.x[2], off.z[0] * off.x[1] - off.z[1] * off.x[0]];
    off.x = [off.x[0] * h.cos() + y[0] * h.sin(), off.x[1] * h.cos() + y[1] * h.sin(), off.x[2] * h.cos() + y[2] * h.sin()];
    assert!(hit(&sa, &solid(&off)) > 0.1 * tooth, "el control debe chocar");
    // Razón de contacto de un cicloidal normal, y las incompatibilidades se dicen
    let p = pair(&cycloidal(18).spec, &cycloidal(30).spec).unwrap();
    assert!((1.05..2.5).contains(&p.contact_ratio), "ε = {}", p.contact_ratio);
    assert!(pair(&cycloidal(18).spec, &GearSpec { teeth: 30, module: 2.0, ..Default::default() }).is_err());
    assert!(pair(&cycloidal(18).spec, &GearSpec { cycloid_circle: 1.2, ..cycloidal(30).spec }).is_err());
}
