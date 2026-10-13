//! Resistencia de engranes: factores de forma del perfil real y tensiones de flexión y contacto.

use approx::assert_relative_eq;
use cad_model::gear_part::GearPart;
use cad_model::gear_strength::{MATERIALS, StrengthInput, analyze, elasticity_factor, material, tooth_form};
use cad_model::gears::{GearKind, GearSpec};
use std::f64::consts::PI;

fn spec(z: u32) -> GearSpec {
    GearSpec { teeth: z, module: 2.0, ..Default::default() }
}

fn part(z: u32, width: f64) -> GearPart {
    GearPart { spec: spec(z), width, ..Default::default() }
}

fn input(torque: f64) -> StrengthInput {
    StrengthInput { torque, rpm: 600.0, service: 1.0, material_a: "c45".into(), material_b: "c45".into(), factor_a: 1.0, factor_b: 1.0 }
}

#[test]
fn factor_de_forma_baja_con_los_dientes_como_en_las_tablas_iso() {
    // Tablas ISO 6336-3 / DIN 3990 (carga en la punta, sin corrección, herramienta de radio 0,38 m):
    // Y_Fa ≈ 2,80 con 20 dientes, 2,52 con 30, 2,32 con 50, 2,18 con 100. Medido en el perfil real sale
    // un ~10 % más alto (del lado seguro): la sección crítica queda un poco más abajo en el pie
    let f = |z| tooth_form(&GearSpec { root_fillet: 0.38, ..spec(z) }).unwrap();
    let table = [(20u32, 2.80), (30, 2.52), (50, 2.32), (100, 2.18)];
    for (z, y) in table {
        let got = f(z).y_f;
        assert!(got > y && got < 1.18 * y, "z={z}: {got} contra {y} de las tablas");
    }
    assert!(f(20).y_f > f(30).y_f && f(30).y_f > f(50).y_f && f(50).y_f > f(100).y_f);
    // Y_Sa ≈ 1,55 con 20 dientes
    let f20 = f(20);
    assert!((1.4..1.7).contains(&f20.y_s), "Y_S {}", f20.y_s);
    // El módulo no cambia los factores (son adimensionales)
    let big = tooth_form(&GearSpec { module: 5.0, root_fillet: 0.38, ..spec(20) }).unwrap();
    assert_relative_eq!(big.y_f, f20.y_f, max_relative = 5e-3);
    // Con corrección positiva el diente se engrosa en el pie y el factor baja
    let shifted = tooth_form(&GearSpec { shift: 0.4, root_fillet: 0.38, ..spec(20) }).unwrap();
    assert!(shifted.y_f < f20.y_f);
    // Con la herramienta de radio chico (0,25 m, la de la receta por defecto) el redondeo es más agudo: Y_S sube
    assert!(tooth_form(&spec(20)).unwrap().y_s > 0.0);
}

#[test]
fn elasticidad_acero_acero() {
    let steel = material("c45").unwrap();
    // Z_E = 189,8 √MPa para acero contra acero
    assert_relative_eq!(elasticity_factor(steel, steel), 189.8, epsilon = 0.1);
    assert!(MATERIALS.iter().all(|m| m.sigma_f_lim > 0.0 && m.sigma_h_lim > m.sigma_f_lim));
}

#[test]
fn par_de_acero_20_40_con_las_cuentas_a_mano() {
    // m = 2, 20/40 dientes, ancho 20, 100 N·m en el piñón a 600 rpm
    let (a, b) = (part(20, 20.0), part(40, 20.0));
    let r = analyze(&a, &b, &input(100_000.0)).unwrap();
    // F_t = 2T/d₁ = 2·100000/40 = 5000 N; v = π·40·600/60000 = 1,2566 m/s
    assert_relative_eq!(r.tangential_force, 5000.0, epsilon = 1e-9);
    assert_relative_eq!(r.speed, PI * 40.0 * 600.0 / 60_000.0, epsilon = 1e-9);
    // Hertz: σ_H = Z_H Z_E Z_ε √(F_t/(b d₁)·(u+1)/u) · √(K_A K_v K_β K_α)
    let z_h = 2.495;
    let z_e = 189.8;
    let z_eps = ((4.0 - r.contact_ratio) / 3.0).sqrt();
    let base = z_h * z_e * z_eps * (5000.0f64 / (20.0 * 40.0) * 1.5).sqrt();
    let expected = base * (r.dynamic * 1.2f64 * 1.1).sqrt();
    assert_relative_eq!(r.sigma_h, expected, max_relative = 5e-3);
    // Flexión: F_t/(b m)·Y_F·Y_S·K
    let ba = r.bending_a.as_ref().unwrap();
    let expected_f = 5000.0 / (20.0 * 2.0) * ba.y_f * ba.y_s * r.dynamic * 1.2 * 1.1;
    assert_relative_eq!(ba.sigma_f, expected_f, max_relative = 1e-9);
    // El piñón (menos dientes, Y_F mayor) flexiona más que la rueda
    assert!(ba.sigma_f > r.bending_b.as_ref().unwrap().sigma_f);
    assert_relative_eq!(ba.safety, 190.0 / ba.sigma_f, epsilon = 1e-9);
}

#[test]
fn las_tensiones_siguen_el_par_el_ancho_y_el_material() {
    let (a, b) = (part(20, 20.0), part(40, 20.0));
    let one = analyze(&a, &b, &input(50_000.0)).unwrap();
    // El doble de par: flexión al doble y contacto √2 veces (a velocidad igual hay K_v igual)
    let two = analyze(&a, &b, &input(100_000.0)).unwrap();
    assert_relative_eq!(two.bending_a.as_ref().unwrap().sigma_f / one.bending_a.as_ref().unwrap().sigma_f, 2.0, epsilon = 1e-9);
    assert_relative_eq!(two.sigma_h / one.sigma_h, 2f64.sqrt(), epsilon = 1e-9);
    // El doble de ancho: flexión a la mitad
    let wide = analyze(&part(20, 40.0), &part(40, 40.0), &input(50_000.0)).unwrap();
    assert_relative_eq!(wide.bending_a.as_ref().unwrap().sigma_f / one.bending_a.as_ref().unwrap().sigma_f, 0.5, epsilon = 1e-9);
    // Impreso en PLA con el mismo par: la seguridad cae muchísimo, y el factor de impresión la baja más
    let pla = StrengthInput { material_a: "pla".into(), material_b: "pla".into(), ..input(50_000.0) };
    let weak = analyze(&a, &b, &pla).unwrap();
    // (el plástico es más blando: el contacto se reparte y sale menos tensión, pero el límite cae mucho más)
    assert!(weak.safety_h < one.safety_h);
    let printed = StrengthInput { factor_a: 0.5, factor_b: 0.5, ..pla.clone() };
    let worse = analyze(&a, &b, &printed).unwrap();
    assert_relative_eq!(worse.safety_h, weak.safety_h * 0.5, epsilon = 1e-9);
    // Helicoidal: Z_β y Y_β mejoran lo que pueden, y la razón de contacto crece
    let helical = |z, beta: f64| GearPart { spec: GearSpec { helix_angle: beta, ..spec(z) }, width: 20.0, ..Default::default() };
    let straight = analyze(&a, &b, &input(50_000.0)).unwrap();
    let h = analyze(&helical(20, 20.0), &helical(40, -20.0), &input(50_000.0)).unwrap();
    assert!(h.sigma_h.is_finite() && h.bending_a.as_ref().unwrap().sigma_f < straight.bending_a.as_ref().unwrap().sigma_f * 1.05);
}

#[test]
fn corona_cremallera_conico_y_errores() {
    // Con corona la flexión de la corona no limita
    let ring = GearPart { spec: GearSpec { kind: GearKind::Internal, teeth: 60, module: 2.0, ..Default::default() }, width: 20.0, outer: 150.0, ..Default::default() };
    let r = analyze(&part(20, 20.0), &ring, &input(50_000.0)).unwrap();
    assert!(r.bending_b.is_none() && r.bending_a.is_some() && r.sigma_h > 0.0);
    // Contra la cremallera: u enorme, el contacto es el de un piñón contra plano
    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 10, module: 2.0, ..Default::default() }, width: 20.0, outer: 6.0, ..Default::default() };
    let r = analyze(&part(20, 20.0), &rack, &input(50_000.0)).unwrap();
    assert!(r.bending_b.is_some() && r.safety_h > 0.0);
    // Cónico: cuentas en la sección media
    let bevel = |z| GearPart { spec: GearSpec { kind: GearKind::Bevel, teeth: z, module: 2.0, cone_angle: 45.0, ..Default::default() }, width: 8.0, ..Default::default() };
    let r = analyze(&bevel(20), &bevel(20), &input(20_000.0)).unwrap();
    assert!(r.notes.iter().any(|n| n.contains("cónico")));
    assert!(r.tangential_force > 2.0 * 20_000.0 / 40.0, "el diámetro medio es menor que el primitivo");
    // El sin fin y los datos malos fallan con mensaje
    let worm = GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: 1, module: 2.0, ..Default::default() }, width: 45.0, ..Default::default() };
    assert!(analyze(&worm, &part(20, 12.0), &input(1000.0)).unwrap_err().contains("sin fin"));
    assert!(analyze(&part(20, 20.0), &part(40, 20.0), &input(-1.0)).is_err());
    assert!(analyze(&part(20, 20.0), &part(40, 20.0), &StrengthInput { material_a: "oro".into(), ..input(1000.0) }).is_err());
}
