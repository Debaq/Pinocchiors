//! Resistencia de un par de engranes: flexión en el pie del diente (ISO 6336-3, método C simplificado, con
//! el factor de forma medido en el perfil real y la carga en la punta) y presión de contacto de Hertz
//! (ISO 6336-2). Son cuentas de orden de magnitud para decidir módulo, ancho y material; en piezas impresas
//! los límites dependen mucho de las capas y del relleno, por eso llevan un factor de impresión.

use crate::gear_part::GearPart;
use crate::gears::{self, GearKind, GearSpec};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Material con sus límites de fatiga (orientativos, ~10⁷ ciclos) y constantes elásticas.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Material {
    pub id: &'static str,
    pub name: &'static str,
    /// Módulo de Young (MPa) y Poisson
    pub e_modulus: f64,
    pub poisson: f64,
    /// Límite de fatiga en flexión del pie y en contacto (MPa)
    pub sigma_f_lim: f64,
    pub sigma_h_lim: f64,
}

pub const MATERIALS: &[Material] = &[
    Material { id: "pla", name: "PLA (impreso)", e_modulus: 3500.0, poisson: 0.36, sigma_f_lim: 15.0, sigma_h_lim: 35.0 },
    Material { id: "petg", name: "PETG (impreso)", e_modulus: 2100.0, poisson: 0.38, sigma_f_lim: 14.0, sigma_h_lim: 30.0 },
    Material { id: "abs", name: "ABS (impreso)", e_modulus: 2200.0, poisson: 0.35, sigma_f_lim: 12.0, sigma_h_lim: 28.0 },
    Material { id: "nylon", name: "Nylon PA12 (impreso)", e_modulus: 1700.0, poisson: 0.39, sigma_f_lim: 18.0, sigma_h_lim: 35.0 },
    Material { id: "resina", name: "Resina (impreso)", e_modulus: 2500.0, poisson: 0.38, sigma_f_lim: 14.0, sigma_h_lim: 32.0 },
    Material { id: "pom", name: "Acetal POM (mecanizado)", e_modulus: 2800.0, poisson: 0.35, sigma_f_lim: 30.0, sigma_h_lim: 60.0 },
    Material { id: "c45", name: "Acero C45 normalizado", e_modulus: 206_000.0, poisson: 0.30, sigma_f_lim: 190.0, sigma_h_lim: 520.0 },
    Material { id: "16mncr5", name: "Acero 16MnCr5 cementado", e_modulus: 206_000.0, poisson: 0.30, sigma_f_lim: 430.0, sigma_h_lim: 1500.0 },
    Material { id: "bronce", name: "Bronce CuSn12", e_modulus: 105_000.0, poisson: 0.34, sigma_f_lim: 90.0, sigma_h_lim: 250.0 },
];

pub fn material(id: &str) -> Option<&'static Material> {
    MATERIALS.iter().find(|m| m.id == id)
}

/// Lo que se pide: el par en el primer engrane (N·mm), su velocidad (rpm) y los factores.
#[derive(Debug, Clone, Deserialize)]
pub struct StrengthInput {
    pub torque: f64,
    pub rpm: f64,
    /// Factor de aplicación K_A (1: uniforme, 1,25: choques leves, 1,5–2: fuertes)
    #[serde(default = "one")]
    pub service: f64,
    pub material_a: String,
    pub material_b: String,
    /// Factor de impresión de cada uno: multiplica sus límites (capas, relleno, orientación)
    #[serde(default = "one")]
    pub factor_a: f64,
    #[serde(default = "one")]
    pub factor_b: f64,
}

fn one() -> f64 {
    1.0
}

/// Flexión en el pie de un diente
#[derive(Debug, Clone, Serialize)]
pub struct Bending {
    /// Factor de forma (carga en la punta), de corrección de tensión y ancho de la sección crítica (30°)
    pub y_f: f64,
    pub y_s: f64,
    pub s_fn: f64,
    pub h_fe: f64,
    pub rho_f: f64,
    /// Tensión (MPa), límite admisible (MPa) y coeficiente de seguridad
    pub sigma_f: f64,
    pub allowable: f64,
    pub safety: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrengthResult {
    /// Fuerza tangencial en el primitivo (N) y velocidad tangencial (m/s)
    pub tangential_force: f64,
    pub speed: f64,
    /// Factor dinámico K_v y razón de contacto transversal
    pub dynamic: f64,
    pub contact_ratio: f64,
    pub bending_a: Option<Bending>,
    pub bending_b: Option<Bending>,
    /// Presión de contacto (MPa), límite admisible y coeficiente de seguridad
    pub sigma_h: f64,
    pub allowable_h: f64,
    pub safety_h: f64,
    pub notes: Vec<String>,
}

type R<T> = Result<T, String>;

/// Factor de forma Y_F, de corrección Y_S y dimensiones de la sección crítica de un diente recto, medidos
/// en su perfil: sección donde la tangente al redondeo del pie forma 30° con el eje del diente; carga
/// en la punta (caso más desfavorable).
pub fn tooth_form(spec: &GearSpec) -> R<ToothForm> {
    let profile = spec.profile().map_err(|e| e.to_string())?;
    let (pts, tip, base) = (&profile.points, profile.tip_radius, profile.circles.base);
    let m = spec.module;
    let radius = |p: &[f64; 2]| p[0].hypot(p[1]);
    // Eje del diente: el ángulo medio de los puntos de la cabeza del primer diente
    let top = (0..pts.len()).max_by(|&a, &b| radius(&pts[a]).total_cmp(&radius(&pts[b]))).ok_or("perfil vacío")?;
    let near: Vec<f64> = pts.iter().filter(|p| radius(p) >= tip - 1e-6 && (p[1].atan2(p[0]) - pts[top][1].atan2(pts[top][0])).abs() < PI / spec.teeth as f64).map(|p| p[1].atan2(p[0])).collect();
    let axis = near.iter().sum::<f64>() / near.len().max(1) as f64;
    let (ca, sa) = (axis.cos(), axis.sin());
    // (u, v): u a lo largo del eje del diente, v de lado; solo el flanco con v ≥ 0 hasta el centro del hueco
    let local: Vec<(f64, f64, f64)> = pts
        .iter()
        .map(|p| (p[0] * ca + p[1] * sa, -p[0] * sa + p[1] * ca, radius(p)))
        .filter(|&(u, v, _)| u > 0.0 && v >= -1e-9)
        .collect();
    let half_pitch = PI / spec.teeth as f64;
    let mut chain: Vec<(f64, f64, f64)> = local.into_iter().filter(|&(u, v, _)| v.atan2(u) <= half_pitch + 1e-9).collect();
    chain.sort_by(|a, b| a.1.atan2(a.0).total_cmp(&b.1.atan2(b.0)));
    if chain.len() < 8 {
        return Err("el perfil tiene muy pocos puntos para medir el pie".into());
    }
    // Esquina de la punta: el último punto del flanco con el radio de cabeza
    let corner = chain.iter().rposition(|c| c.2 >= tip - 1e-3).ok_or("sin punta")?;
    let (u_tip, v_tip) = (chain[corner].0, chain[corner].1);
    // Sección crítica: pasada la parte de evolvente, el primer punto del redondeo con tangente a ≥ 30° del eje
    let start = chain.iter().position(|c| c.2 <= profile.involute_start).unwrap_or(corner + 1).max(corner + 1);
    let mut crit = chain.len() - 2;
    for i in start..chain.len() - 1 {
        let (du, dv) = (chain[i + 1].0 - chain[i].0, chain[i + 1].1 - chain[i].1);
        if dv.abs().atan2(du.abs()) >= 30f64.to_radians() {
            crit = i;
            break;
        }
    }
    let (u_c, v_c, _) = chain[crit];
    let s_fn = 2.0 * v_c;
    let h_fe = u_tip - u_c;
    if s_fn <= 0.0 || h_fe <= 0.0 {
        return Err("no se pudo medir la sección crítica del pie".into());
    }
    // Radio del redondeo en la sección crítica: círculo por tres puntos vecinos
    let (a, b, c) = (chain[crit.saturating_sub(2)], chain[crit], chain[(crit + 2).min(chain.len() - 1)]);
    let rho = circumradius((a.0, a.1), (b.0, b.1), (c.0, c.1)).clamp(0.05 * m, 1.0 * m);
    // Ángulo de la carga: α_Fen = α_punta − γ_punta
    let alpha_tip = (base / tip).clamp(-1.0, 1.0).acos();
    let gamma = v_tip.atan2(u_tip);
    let alpha_fen = alpha_tip - gamma;
    let an = spec.pressure_angle.to_radians();
    let y_f = 6.0 * (h_fe / m) * alpha_fen.cos() / ((s_fn / m).powi(2) * an.cos());
    let l = s_fn / h_fe;
    let q_s = s_fn / (2.0 * rho);
    let y_s = (1.2 + 0.13 * l) * q_s.powf(1.0 / (1.21 + 2.3 / l));
    Ok(ToothForm { y_f, y_s, s_fn, h_fe, rho_f: rho })
}

/// Resultado de [`tooth_form`]
#[derive(Debug, Clone, Copy)]
pub struct ToothForm {
    pub y_f: f64,
    pub y_s: f64,
    pub s_fn: f64,
    pub h_fe: f64,
    pub rho_f: f64,
}

fn circumradius(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let (ab, bc, ca) = ((b.0 - a.0).hypot(b.1 - a.1), (c.0 - b.0).hypot(c.1 - b.1), (a.0 - c.0).hypot(a.1 - c.1));
    let area2 = ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs();
    if area2 < 1e-12 { f64::MAX } else { ab * bc * ca / (2.0 * area2) }
}

/// Engrane cilíndrico equivalente de un diente (cónico: dientes virtuales; helicoidal: dientes en el plano normal)
fn virtual_spur(spec: &GearSpec, module: f64) -> GearSpec {
    let (beta, delta) = (spec.helix_angle.to_radians(), spec.cone_angle.to_radians());
    let z = match spec.kind {
        GearKind::Bevel => spec.teeth as f64 / delta.cos(),
        GearKind::Rack => 300.0,
        _ => {
            let beta_b = (beta.tan() * spec.pressure_angle_t().cos()).atan();
            spec.teeth as f64 / (beta_b.cos().powi(2) * beta.cos())
        }
    };
    GearSpec { kind: GearKind::External, module, teeth: z.round().max(7.0) as u32, helix_angle: 0.0, cone_angle: 45.0, spiral_angle: 0.0, backlash: 0.0, ..*spec }
}

/// Factor dinámico K_v (AGMA 2001, calidad Q = 6) con la velocidad en m/s
fn dynamic_factor(v: f64) -> f64 {
    let q: f64 = 6.0;
    let b = 0.25 * (12.0 - q).powf(2.0 / 3.0);
    let a = 50.0 + 56.0 * (1.0 - b);
    ((a + (200.0 * v).sqrt()) / a).powf(b)
}

/// Resistencia del par `a` (el primero, con el par pedido) y `b` (el que engrana con él).
pub fn analyze(a: &GearPart, b: &GearPart, input: &StrengthInput) -> R<StrengthResult> {
    let (ea, eb) = (a.effective_spec(), b.effective_spec());
    for k in [ea.kind, eb.kind] {
        if k == GearKind::Worm {
            return Err("el sin fin se calcula distinto (rozamiento y calentamiento): esta cuenta es para dentados de evolvente".into());
        }
    }
    if input.torque <= 0.0 || input.rpm < 0.0 || input.service < 1.0 {
        return Err("el par debe ser positivo, las rpm no negativas y el factor de aplicación al menos 1".into());
    }
    let mat_a = material(&input.material_a).ok_or("material desconocido en el primer engrane")?;
    let mat_b = material(&input.material_b).ok_or("material desconocido en el segundo engrane")?;
    let bevel = ea.kind == GearKind::Bevel;
    let ca = ea.circles().map_err(|e| e.to_string())?;
    let width = a.width.min(b.width);
    let mut notes = vec!["orientativo: límites de fatiga típicos de 10⁷ ciclos; ensaya un prototipo, sobre todo si está impreso".to_string()];
    // Diámetro y módulo en la sección media (en el cónico se achican hacia el vértice)
    let (d1, module) = if bevel {
        let cone = ea.cone_distance();
        let k = (cone - a.width / 2.0) / cone;
        notes.push("cónico: cuentas con los dientes virtuales en la sección media de la cara".to_string());
        (2.0 * ca.pitch * k, ea.module * k)
    } else {
        (2.0 * ca.pitch, ea.module)
    };
    let f_t = 2.0 * input.torque / d1;
    let v = PI * d1 * input.rpm / 60_000.0;
    let k_v = dynamic_factor(v);
    // Cargas de partida: desalineación y reparto entre dientes de piezas poco precisas
    let (k_a, k_beta, k_alpha) = (input.service, 1.2, 1.1);
    notes.push(format!("K_A = {k_a}, K_v = {k_v:.2}, K_β = {k_beta}, K_α = {k_alpha}"));
    // Hélice
    let beta = ea.helix_angle.to_radians();
    let (pair_contact, working, u) = match (ea.kind, eb.kind) {
        (GearKind::External | GearKind::Internal, GearKind::External | GearKind::Internal) => {
            let p = gears::pair(&ea, &eb).map_err(|e| e.to_string())?;
            (p.contact_ratio, p.working_angle, eb.teeth as f64 / ea.teeth as f64)
        }
        (GearKind::External, GearKind::Rack) | (GearKind::Rack, GearKind::External) => (1.5, ca.alpha_t, 1e6),
        (GearKind::Bevel, GearKind::Bevel) => (1.4, ca.alpha_t, eb.teeth as f64 / ea.teeth as f64),
        _ => return Err("esa combinación no se calcula".into()),
    };
    if pair_contact < 1.1 {
        notes.push(format!("razón de contacto {pair_contact:.2}: menos de 1,1, el engrane no es continuo"));
    }
    let eps_beta = (width * beta.sin().abs() / (PI * module)).min(1.0);
    let y_beta = (1.0 - eps_beta * ea.helix_angle.abs() / 120.0).max(0.75);
    // Flexión de cada diente
    let bending = |spec: &GearSpec, mat: &Material, factor: f64| -> R<Bending> {
        if spec.kind == GearKind::Internal {
            // La corona se calcula como el piñón de su par (el diente de una corona es más grueso)
            return Err("corona".into());
        }
        let v_spec = virtual_spur(spec, module);
        let form = tooth_form(&v_spec)?;
        let sigma_f = f_t / (width * module) * form.y_f * form.y_s * y_beta * k_a * k_v * k_beta * k_alpha;
        let allowable = mat.sigma_f_lim * factor;
        Ok(Bending { y_f: form.y_f, y_s: form.y_s, s_fn: form.s_fn, h_fe: form.h_fe, rho_f: form.rho_f, sigma_f, allowable, safety: allowable / sigma_f })
    };
    let bending_a = bending(&ea, mat_a, input.factor_a).ok();
    let bending_b = match bending(&eb, mat_b, input.factor_b) {
        Ok(x) => Some(x),
        Err(e) if e == "corona" => {
            notes.push("corona: la flexión del diente de la corona no limita (es más grueso que el del piñón)".to_string());
            None
        }
        Err(e) => return Err(e),
    };
    // Contacto (Hertz)
    let cos_bb = (beta.tan() * ca.alpha_t.cos()).atan().cos();
    let z_h = (2.0 * cos_bb * working.cos() / (ca.alpha_t.cos().powi(2) * working.sin())).sqrt();
    let z_e = (1.0 / (PI * ((1.0 - mat_a.poisson.powi(2)) / mat_a.e_modulus + (1.0 - mat_b.poisson.powi(2)) / mat_b.e_modulus))).sqrt();
    let z_eps = if eps_beta == 0.0 {
        ((4.0 - pair_contact) / 3.0).max(0.1).sqrt()
    } else if eps_beta < 1.0 {
        (((4.0 - pair_contact) / 3.0) * (1.0 - eps_beta) + eps_beta / pair_contact).max(0.1).sqrt()
    } else {
        (1.0 / pair_contact).sqrt()
    };
    let z_beta = beta.cos().sqrt();
    let inner = if eb.kind == GearKind::Internal { (u - 1.0) / u } else { (u + 1.0) / u };
    let sigma_h0 = z_h * z_e * z_eps * z_beta * (f_t / (width * d1) * inner).sqrt();
    let sigma_h = sigma_h0 * (k_a * k_v * k_beta * k_alpha).sqrt();
    let allowable_h = (mat_a.sigma_h_lim * input.factor_a).min(mat_b.sigma_h_lim * input.factor_b);
    let result = StrengthResult {
        tangential_force: f_t,
        speed: v,
        dynamic: k_v,
        contact_ratio: pair_contact,
        bending_a,
        bending_b,
        sigma_h,
        allowable_h,
        safety_h: allowable_h / sigma_h,
        notes,
    };
    Ok(result)
}

/// Coeficiente de elasticidad Z_E (√MPa) de dos materiales
pub fn elasticity_factor(a: &Material, b: &Material) -> f64 {
    (1.0 / (PI * ((1.0 - a.poisson.powi(2)) / a.e_modulus + (1.0 - b.poisson.powi(2)) / b.e_modulus))).sqrt()
}
