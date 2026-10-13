//! Poleas dentadas de correa, piñones de cadena de rodillos y dientes cicloidales: contornos planos para las
//! mismas operaciones que el engrane (agujero, cubo, chavetero, aligeramientos…). El gap de un diente (la
//! ranura, el asiento del rodillo o el hueco) queda centrado en el ángulo 0, como en el perfil de evolvente.
//!
//! Las medidas de las correas son las de uso común en bibliotecas de poleas (aproximadas: la ranura se
//! ajusta con el «juego», que ensancha la ranura); las de cadena, las de ISO 606 / ANSI B29.1.

use crate::gears::{GearCircles, GearError, GearKind, GearProfile, GearSpec, ToothForm};
use cad_occt::{Curve, P3};
use serde::Serialize;
use std::f64::consts::{PI, TAU};

/// Correa dentada: paso, profundidad y ancho de la ranura de la polea, y cuánto baja el diámetro exterior
/// respecto del primitivo (línea de paso de la correa).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Belt {
    pub name: &'static str,
    pub pitch: f64,
    pub depth: f64,
    pub width: f64,
    pub offset: f64,
    /// Ranura trapecial (MXL, XL, T) en vez de redonda (GT2, HTD)
    pub trapezoid: bool,
}

pub fn belt(form: ToothForm) -> Option<Belt> {
    let b = |name, pitch, depth, width, offset, trapezoid| Some(Belt { name, pitch, depth, width, offset, trapezoid });
    match form {
        ToothForm::Gt2Mm2 => b("GT2 2 mm", 2.0, 0.764, 1.494, 0.254, false),
        ToothForm::Gt2Mm3 => b("GT2 3 mm", 3.0, 1.169, 2.31, 0.381, false),
        ToothForm::Gt2Mm5 => b("GT2 5 mm", 5.0, 1.969, 3.952, 0.5715, false),
        ToothForm::Htd3M => b("HTD 3M", 3.0, 1.289, 2.27, 0.381, false),
        ToothForm::Htd5M => b("HTD 5M", 5.0, 2.199, 3.781, 0.5715, false),
        ToothForm::Mxl => b("MXL 2,032 mm", 2.032, 0.508, 1.321, 0.254, true),
        ToothForm::Xl => b("XL 5,08 mm", 5.08, 1.27, 3.1, 0.254, true),
        ToothForm::T25 => b("T2,5", 2.5, 0.7, 1.678, 0.6, true),
        ToothForm::T5 => b("T5", 5.0, 1.19, 3.264, 0.6, true),
        _ => None,
    }
}

/// Cadena de rodillos: paso, diámetro del rodillo y ancho interior entre placas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Chain {
    pub name: &'static str,
    pub pitch: f64,
    pub roller: f64,
    pub inner_width: f64,
}

pub fn chain(form: ToothForm) -> Option<Chain> {
    let c = |name, pitch, roller, inner_width| Some(Chain { name, pitch, roller, inner_width });
    match form {
        ToothForm::Chain25 => c("ANSI 25", 6.35, 3.30, 3.18),
        ToothForm::Chain35 => c("ANSI 35", 9.525, 5.08, 4.77),
        ToothForm::Chain40 => c("ANSI 40", 12.7, 7.95, 7.85),
        ToothForm::Chain50 => c("ANSI 50", 15.875, 10.16, 9.40),
        ToothForm::Chain06B => c("ISO 06B", 9.525, 6.35, 5.72),
        ToothForm::Chain08B => c("ISO 08B", 12.7, 8.51, 7.75),
        ToothForm::Chain10B => c("ISO 10B", 15.875, 10.16, 9.65),
        _ => None,
    }
}

/// Una forma de diente para elegir en la interfaz
#[derive(Debug, Clone, Serialize)]
pub struct FormInfo {
    pub form: ToothForm,
    pub name: &'static str,
    /// "gear", "belt" o "chain"
    pub family: &'static str,
    pub pitch: Option<f64>,
    pub width: f64,
}

/// Todas las formas de diente, para el selector
pub fn forms() -> Vec<FormInfo> {
    let family = |f: ToothForm| if f.is_belt() { "belt" } else if f.is_chain() { "chain" } else { "gear" };
    [ToothForm::Involute, ToothForm::Cycloidal]
        .iter()
        .chain(ToothForm::BELTS)
        .chain(ToothForm::CHAINS)
        .map(|&f| FormInfo { form: f, name: form_name(f), family: family(f), pitch: form_pitch(f), width: default_width(f) })
        .collect()
}

/// Nombre para mostrar de una forma de diente
pub fn form_name(form: ToothForm) -> &'static str {
    match form {
        ToothForm::Involute => "Evolvente",
        ToothForm::Cycloidal => "Cicloidal",
        f => belt(f).map(|b| b.name).or_else(|| chain(f).map(|c| c.name)).unwrap_or("?"),
    }
}

/// Paso (mm) de una correa o cadena
pub fn form_pitch(form: ToothForm) -> Option<f64> {
    belt(form).map(|b| b.pitch).or_else(|| chain(form).map(|c| c.pitch))
}

/// Ancho útil de la polea o el piñón (mm) por defecto: el de la correa más común, o el 93 % del ancho interior de la cadena
pub fn default_width(form: ToothForm) -> f64 {
    if let Some(b) = belt(form) {
        return if b.pitch <= 2.6 { 6.0 } else { 10.0 };
    }
    chain(form).map_or(8.0, |c| (0.93 * c.inner_width * 10.0).floor() / 10.0)
}

type R<T> = Result<T, GearError>;

fn bad<T>(m: impl Into<String>) -> R<T> {
    Err(GearError::Invalid(m.into()))
}

fn polar(r: f64, a: f64) -> [f64; 2] {
    [r * a.cos(), r * a.sin()]
}

fn p3(p: [f64; 2]) -> P3 {
    [p[0], p[1], 0.0]
}

/// Puntos de un arco que pasa por `a`, `m`, `b` (en ese orden), `n` tramos.
fn sample_arc(a: [f64; 2], m: [f64; 2], b: [f64; 2], n: usize) -> Vec<[f64; 2]> {
    // Centro: circuncentro de los tres puntos
    let d = 2.0 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
    if d.abs() < 1e-12 {
        return vec![a, b];
    }
    let (a2, m2, b2) = (a[0] * a[0] + a[1] * a[1], m[0] * m[0] + m[1] * m[1], b[0] * b[0] + b[1] * b[1]);
    let c = [(a2 * (m[1] - b[1]) + m2 * (b[1] - a[1]) + b2 * (a[1] - m[1])) / d, (a2 * (b[0] - m[0]) + m2 * (a[0] - b[0]) + b2 * (m[0] - a[0])) / d];
    let r = (a[0] - c[0]).hypot(a[1] - c[1]);
    let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
    let (t0, tm, t1) = (ang(a), ang(m), ang(b));
    // Sentido: el que pasa por `m`
    let norm = |x: f64| x.rem_euclid(TAU);
    let ccw = norm(tm - t0) < norm(t1 - t0);
    let sweep = if ccw { norm(t1 - t0) } else { -norm(t0 - t1) };
    (0..=n).map(|i| polar_at(c, r, t0 + sweep * i as f64 / n as f64)).collect()
}

fn polar_at(c: [f64; 2], r: f64, t: f64) -> [f64; 2] {
    [c[0] + r * t.cos(), c[1] + r * t.sin()]
}

fn rotate(p: [f64; 2], a: f64) -> [f64; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Contorno de una polea de `teeth` ranuras de la correa `b`. `clearance` ensancha la ranura (impresión 3D).
pub fn pulley_profile(teeth: u32, b: &Belt, clearance: f64) -> R<GearProfile> {
    let z = teeth as f64;
    if teeth < 8 {
        return bad("una polea necesita al menos 8 dientes");
    }
    let pitch_r = z * b.pitch / TAU;
    let r_od = pitch_r - b.offset;
    let (w, h) = (b.width + clearance, b.depth + clearance / 2.0);
    let beta = (w / (2.0 * r_od)).clamp(-1.0, 1.0).asin();
    if 2.0 * beta >= 0.9 * TAU / z {
        return bad(format!("{} dientes son muy pocos para la correa {}: las ranuras se tocan", teeth, b.name));
    }
    if h >= r_od - 1.0 {
        return bad("la ranura llega al eje: la polea es muy chica para esta correa");
    }
    let mut curves = Vec::new();
    let mut points = Vec::new();
    let push_arc = |a: [f64; 2], m: [f64; 2], c: [f64; 2], curves: &mut Vec<Curve>, points: &mut Vec<[f64; 2]>| {
        curves.push(Curve::Arc(p3(a), p3(m), p3(c)));
        points.extend(sample_arc(a, m, c, 8));
        points.pop();
    };
    for k in 0..teeth {
        let phi = TAU * k as f64 / z;
        let (a, c) = (polar(r_od, phi - beta), polar(r_od, phi + beta));
        // Ranura
        if b.trapezoid {
            // Flancos a 20° del eje de la ranura (40° incluidos), fondo plano
            let half_bottom = (w / 2.0 - h * 20f64.to_radians().tan()).max(0.2 * w);
            let r_b = r_od - h;
            let (e1, e2) = (rotate([r_b, -half_bottom], phi), rotate([r_b, half_bottom], phi));
            for (u, v) in [(a, e1), (e1, e2), (e2, c)] {
                curves.push(Curve::Line(p3(u), p3(v)));
                points.push(u);
            }
        } else {
            let mid = polar(r_od - h, phi);
            push_arc(a, mid, c, &mut curves, &mut points);
        }
        // Cresta hasta la ranura siguiente
        let next = TAU * (k + 1) as f64 / z;
        let (a2, mid) = (polar(r_od, next - beta), polar(r_od, 0.5 * (phi + beta + next - beta)));
        push_arc(c, mid, a2, &mut curves, &mut points);
    }
    let circles = GearCircles { module_t: b.pitch / PI, alpha_t: 0.0, pitch: pitch_r, base: pitch_r, tip: r_od, root: r_od - h, thickness: 0.0 };
    Ok(GearProfile { circles, points, curves, warnings: Vec::new(), involute_start: r_od - h, tip_radius: r_od })
}

/// Contorno de un piñón de `teeth` dientes para la cadena `c`: asientos de rodillo de radio ~0,505·d₁ con el
/// centro en la circunferencia de paso (D = p / sen(180°/z)), flancos tangentes al asiento y cabeza D_a = p(0,6 + cot(180°/z)).
pub fn sprocket_profile(teeth: u32, c: &Chain, clearance: f64) -> R<GearProfile> {
    let z = teeth as f64;
    if teeth < 9 {
        return bad("un piñón de cadena necesita al menos 9 dientes");
    }
    let pitch_r = c.pitch / (2.0 * (PI / z).sin());
    let seat = 0.505 * c.roller + clearance / 2.0;
    let r_tip = c.pitch * (0.6 + 1.0 / (PI / z).tan()) / 2.0;
    let r_root = pitch_r - seat;
    if r_root <= 1.0 {
        return bad("el piñón es muy chico para esta cadena");
    }
    // El asiento abarca ±θ desde la dirección hacia el eje; los flancos salen tangentes
    let theta = 70f64.to_radians();
    let mut curves = Vec::new();
    let mut points = Vec::new();
    let arc_pts = |a, m, b| sample_arc(a, m, b, 10);
    for k in 0..teeth {
        let phi = TAU * k as f64 / z;
        let (er, et) = ([phi.cos(), phi.sin()], [-phi.sin(), phi.cos()]);
        let center = [pitch_r * er[0], pitch_r * er[1]];
        // u(α) = −cos α·e_r + sen α·e_t
        let u = |al: f64| [-al.cos() * er[0] + al.sin() * et[0], -al.cos() * er[1] + al.sin() * et[1]];
        let at = |al: f64| [center[0] + seat * u(al)[0], center[1] + seat * u(al)[1]];
        let (s_minus, s_plus, s_mid) = (at(-theta), at(theta), at(0.0));
        curves.push(Curve::Arc(p3(s_minus), p3(s_mid), p3(s_plus)));
        let mut p = arc_pts(s_minus, s_mid, s_plus);
        p.pop();
        points.extend(p);
        // Flanco derecho de este asiento y flanco izquierdo del siguiente, hasta la cabeza
        let next = TAU * (k + 1) as f64 / z;
        let (er2, et2) = ([next.cos(), next.sin()], [-next.sin(), next.cos()]);
        let center2 = [pitch_r * er2[0], pitch_r * er2[1]];
        let u2 = |al: f64| [-al.cos() * er2[0] + al.sin() * et2[0], -al.cos() * er2[1] + al.sin() * et2[1]];
        let s2_minus = [center2[0] + seat * u2(-theta)[0], center2[1] + seat * u2(-theta)[1]];
        let d1 = [theta.sin() * er[0] + theta.cos() * et[0], theta.sin() * er[1] + theta.cos() * et[1]];
        let d2 = [theta.sin() * er2[0] - theta.cos() * et2[0], theta.sin() * er2[1] - theta.cos() * et2[1]];
        let hit = |p: [f64; 2], d: [f64; 2]| {
            let b = p[0] * d[0] + p[1] * d[1];
            let cc = p[0] * p[0] + p[1] * p[1] - r_tip * r_tip;
            let disc = b * b - cc;
            (disc >= 0.0).then(|| {
                let s = -b + disc.sqrt();
                [p[0] + s * d[0], p[1] + s * d[1]]
            })
        };
        let (q1, q2) = (hit(s_plus, d1), hit(s2_minus, d2));
        // ¿Se cruzan los flancos antes de la cabeza (diente en punta)?
        let det = d1[0] * (-d2[1]) - d1[1] * (-d2[0]);
        let cross = (det.abs() > 1e-12).then(|| {
            let (rx, ry) = (s2_minus[0] - s_plus[0], s2_minus[1] - s_plus[1]);
            let (t1, t2) = ((rx * (-d2[1]) - ry * (-d2[0])) / det, (d1[0] * ry - d1[1] * rx) / det);
            (t1, t2, [s_plus[0] + t1 * d1[0], s_plus[1] + t1 * d1[1]])
        });
        match (q1, q2, cross) {
            (_, _, Some((t1, t2, x))) if t1 > 0.0 && t2 > 0.0 && x[0].hypot(x[1]) <= r_tip => {
                curves.push(Curve::Line(p3(s_plus), p3(x)));
                curves.push(Curve::Line(p3(x), p3(s2_minus)));
                points.extend([s_plus, x]);
            }
            (Some(q1), Some(q2), _) => {
                let (a1, a2) = (q1[1].atan2(q1[0]), q2[1].atan2(q2[0]));
                let span = (a2 - a1).rem_euclid(TAU);
                let mid = polar(r_tip, a1 + span / 2.0);
                curves.push(Curve::Line(p3(s_plus), p3(q1)));
                curves.push(Curve::Arc(p3(q1), p3(mid), p3(q2)));
                curves.push(Curve::Line(p3(q2), p3(s2_minus)));
                points.push(s_plus);
                let mut t = sample_arc(q1, mid, q2, 4);
                t.pop();
                points.extend(t);
                points.push(q2);
            }
            _ => return bad("el diente del piñón no se puede formar con esta cadena y estos dientes"),
        }
    }
    let circles = GearCircles { module_t: c.pitch / PI, alpha_t: 0.0, pitch: pitch_r, base: pitch_r, tip: r_tip, root: r_root, thickness: 0.0 };
    Ok(GearProfile { circles, points, curves, warnings: Vec::new(), involute_start: r_root, tip_radius: r_tip })
}

// ─── Cicloidal ───────────────────────────────────────────────────────────────────────────────────────────────

/// Epicicloide (rueda `rho` rodando por fuera de un círculo de radio `r`) como (x, y) del ángulo `t` de la línea de centros
fn epicycloid(r: f64, rho: f64, t: f64) -> [f64; 2] {
    let k = (r + rho) / rho;
    [(r + rho) * t.cos() - rho * (k * t).cos(), (r + rho) * t.sin() - rho * (k * t).sin()]
}

/// Hipocicloide (rodando por dentro)
fn hypocycloid(r: f64, rho: f64, t: f64) -> [f64; 2] {
    let k = (r - rho) / rho;
    [(r - rho) * t.cos() + rho * (k * t).cos(), (r - rho) * t.sin() - rho * (k * t).sin()]
}

/// Ángulo `t` (> 0) en que la curva `f` alcanza el radio `target` desde el primitivo `r` (búsqueda por bisección)
fn roll_to(f: impl Fn(f64) -> [f64; 2], r: f64, target: f64, t_max: f64) -> f64 {
    let radius = |t: f64| f(t)[0].hypot(f(t)[1]);
    let outward = target > r;
    let (mut lo, mut hi) = (0.0, t_max);
    // Radio monótono en [0, t_max] (se limita t_max al punto extremo de la curva)
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if (radius(mid) < target) == outward { lo = mid } else { hi = mid }
    }
    0.5 * (lo + hi)
}

/// Un flanco cicloidal de un diente centrado en el ángulo 0, el del lado de ángulos positivos: de la raíz a la
/// cabeza, pasando por el punto primitivo. Devuelve los puntos y el ángulo de rodadura de la cabeza.
fn cycloid_flank(r: f64, rho: f64, theta0: f64, r_root: f64, r_tip: f64) -> R<(Vec<[f64; 2]>, f64)> {
    if r_root < r - 2.0 * rho + 1e-6 || r_tip > r + 2.0 * rho - 1e-6 {
        return bad("el círculo generador es chico para esta altura de diente: sube el radio cicloidal");
    }
    // El radio es monótono hasta el extremo de la curva: R + 2ρ (epi) en t = πρ/(R + ρ), R − 2ρ (hipo) en t = πρ/(R − ρ)
    let t_hypo = roll_to(|t| hypocycloid(r, rho, t), r, r_root, PI * rho / (r - rho) * 0.9999);
    let t_epi = roll_to(|t| epicycloid(r, rho, t), r, r_tip, PI * rho / (r + rho) * 0.9999);
    let n = 14;
    let mut pts = Vec::new();
    // De la raíz (t del hipocicloide hacia el primitivo)
    for i in (0..=n).rev() {
        let t = t_hypo * i as f64 / n as f64;
        pts.push(rotate(hypocycloid(r, rho, t), theta0));
    }
    // Y del primitivo a la cabeza, espejado: el diente se afina
    for i in 1..=n {
        let t = t_epi * i as f64 / n as f64;
        let [x, y] = epicycloid(r, rho, t);
        pts.push(rotate([x, -y], theta0));
    }
    Ok((pts, t_epi))
}

/// Perfil cicloidal: flancos de epicicloide sobre el primitivo e hipocicloide bajo él, con el hueco centrado en 0.
pub fn cycloidal_profile(spec: &GearSpec) -> R<GearProfile> {
    let (m, z) = (spec.module, spec.teeth as f64);
    if spec.helix_angle != 0.0 || spec.shift != 0.0 {
        return bad("el diente cicloidal no lleva hélice ni corrección de perfil");
    }
    let r = m * z / 2.0;
    let rho = spec.cycloid_circle * m;
    if rho <= 0.0 {
        return bad("el radio del círculo generador debe ser positivo");
    }
    let (r_tip, r_root) = (r + spec.addendum * m, r - spec.dedendum * m);
    let s = PI * m / 2.0 - spec.backlash / 2.0;
    let theta0 = s / (2.0 * r);
    let (flank, _) = cycloid_flank(r, rho, theta0, r_root, r_tip)?;
    let mut warnings = Vec::new();
    if rho > r / 2.0 {
        warnings.push(crate::gears::GearWarning::Undercut { min_teeth: 4.0 * spec.cycloid_circle });
    }
    let mut curves = Vec::new();
    let mut points = Vec::new();
    let tau = TAU / z;
    let tangent = |a: [f64; 2], b: [f64; 2]| {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l = d[0].hypot(d[1]).max(1e-12);
        [d[0] / l, d[1] / l, 0.0]
    };
    for j in 0..spec.teeth {
        let psi = tau * (j as f64 + 0.5);
        let right: Vec<[f64; 2]> = flank.iter().map(|&p| rotate(p, psi)).collect();
        // El lado izquierdo es el espejo (y → −y) del derecho, de raíz a cabeza
        let left: Vec<[f64; 2]> = flank.iter().map(|&p| rotate([p[0], -p[1]], psi)).collect();
        let tip_l = *left.last().unwrap();
        let tip_r = *right.last().unwrap();
        // Subida por el flanco izquierdo
        let (p0, p1) = (left[0], left[left.len() - 1]);
        curves.push(Curve::SplineEnds {
            points: left.iter().map(|&p| p3(p)).collect(),
            start: tangent(p0, left[1]),
            end: tangent(left[left.len() - 2], p1),
        });
        points.extend(left.iter().copied());
        // Cabeza
        let mid = polar(r_tip, 0.5 * (tip_l[1].atan2(tip_l[0]) + tip_r[1].atan2(tip_r[0])));
        curves.push(Curve::Arc(p3(tip_l), p3(mid), p3(tip_r)));
        // Bajada por el flanco derecho
        let rev: Vec<[f64; 2]> = right.iter().rev().copied().collect();
        curves.push(Curve::SplineEnds {
            points: rev.iter().map(|&p| p3(p)).collect(),
            start: tangent(rev[0], rev[1]),
            end: tangent(rev[rev.len() - 2], rev[rev.len() - 1]),
        });
        points.extend(sample_arc(tip_l, mid, tip_r, 4).into_iter().skip(1));
        points.extend(rev.iter().copied().skip(1));
        // Fondo hasta el diente siguiente
        let next_left = {
            let l0 = rotate([flank[0][0], -flank[0][1]], tau * (j as f64 + 1.5));
            l0
        };
        let end = *rev.last().unwrap();
        let a0 = end[1].atan2(end[0]);
        let mut a1 = next_left[1].atan2(next_left[0]);
        if a1 < a0 {
            a1 += TAU;
        }
        let root_mid = polar(r_root, 0.5 * (a0 + a1));
        curves.push(Curve::Arc(p3(end), p3(root_mid), p3(next_left)));
        points.extend(sample_arc(end, root_mid, next_left, 4).into_iter().skip(1));
        points.pop();
    }
    let circles = GearCircles { module_t: m, alpha_t: 0.0, pitch: r, base: r, tip: r_tip, root: r_root, thickness: s };
    Ok(GearProfile { circles, points, curves, warnings, involute_start: r_root, tip_radius: r_tip })
}

/// Contacto de un par cicloidal: razón de contacto a partir de los ángulos de rodadura de las cabezas.
pub fn cycloidal_contact_ratio(a: &GearSpec, b: &GearSpec) -> R<f64> {
    let m = a.module;
    let roll = |spec: &GearSpec| -> R<f64> {
        let r = m * spec.teeth as f64 / 2.0;
        let rho = spec.cycloid_circle * m;
        let tip = r + spec.addendum * m;
        if tip > r + 2.0 * rho - 1e-6 {
            return bad("el círculo generador es chico para esta altura de diente");
        }
        Ok(roll_to(|t| epicycloid(r, rho, t), r, tip, PI * rho / (r + rho) * 0.9999))
    };
    let (ra, rb) = (m * a.teeth as f64 / 2.0, m * b.teeth as f64 / 2.0);
    Ok((ra * roll(a)? + rb * roll(b)?) / (PI * m))
}

// ─── Largos de correa y cadena ─────────────────────────────────────────────────────────────────────────────────

/// Correa o cadena entre dos ruedas: dientes (o eslabones) y la distancia entre centros que les corresponde.
#[derive(Debug, Clone, Serialize)]
pub struct DriveLength {
    /// Dientes de la correa o eslabones de la cadena (enteros)
    pub count: u32,
    /// Distancia entre centros (mm) con ese largo entero
    pub center_distance: f64,
    /// Largo (mm) a lo largo de la línea de paso
    pub length: f64,
    pub notes: Vec<String>,
}

/// Dientes de correa que pide una distancia entre centros aproximada, y los centros exactos con ese largo
pub fn belt_length(form: ToothForm, z1: u32, z2: u32, center: f64) -> Result<DriveLength, String> {
    let b = belt(form).ok_or("no es una correa")?;
    let (d1, d2) = (z1 as f64 * b.pitch / PI, z2 as f64 * b.pitch / PI);
    if center <= (d1 + d2) / 2.0 {
        return Err("la distancia entre centros tiene que superar la suma de los radios primitivos".into());
    }
    let l = 2.0 * center + PI * (d1 + d2) / 2.0 + (d2 - d1).powi(2) / (4.0 * center);
    let count = (l / b.pitch).round().max(1.0) as u32;
    let length = count as f64 * b.pitch;
    let bb = 2.0 * length - PI * (d1 + d2);
    let disc = bb * bb - 8.0 * (d2 - d1).powi(2);
    if disc < 0.0 {
        return Err("esa correa es muy corta para las dos poleas".into());
    }
    let mut notes = Vec::new();
    // Dientes abrazados de la polea chica: conviene al menos 6
    let wrap = ((PI - 2.0 * ((d2 - d1).abs() / (2.0 * center)).asin()) / TAU * z1.min(z2) as f64).floor();
    if wrap < 6.0 {
        notes.push(format!("la correa abraza solo {wrap} dientes de la polea chica: conviene 6 o más"));
    }
    Ok(DriveLength { count, center_distance: (bb + disc.sqrt()) / 8.0, length, notes })
}

/// Eslabones de cadena (número par, para no usar eslabón acodado) y los centros exactos
pub fn chain_length(form: ToothForm, z1: u32, z2: u32, center: f64) -> Result<DriveLength, String> {
    let c = chain(form).ok_or("no es una cadena")?;
    let p = c.pitch;
    let (z1f, z2f) = (z1 as f64, z2 as f64);
    let k = ((z2f - z1f) / TAU).powi(2);
    let links = 2.0 * center / p + (z1f + z2f) / 2.0 + k * p / center;
    let count = ((links / 2.0).ceil() * 2.0).max(2.0) as u32;
    let l = count as f64 - (z1f + z2f) / 2.0;
    let disc = l * l - 8.0 * k;
    if disc < 0.0 {
        return Err("esa cadena es muy corta para los dos piñones".into());
    }
    let mut notes = Vec::new();
    if center < 30.0 * p || center > 50.0 * p {
        notes.push(format!("la distancia entre centros ideal está entre {:.0} y {:.0} mm (30 a 50 pasos)", 30.0 * p, 50.0 * p));
    }
    if z1.min(z2) < 17 {
        notes.push("menos de 17 dientes en un piñón desgasta la cadena y da golpes".to_string());
    }
    Ok(DriveLength { count, center_distance: p / 4.0 * (l + disc.sqrt()), length: count as f64 * p, notes })
}

/// Círculos de una polea o un piñón de cadena sin armar el contorno
pub(crate) fn wheel_circles(spec: &GearSpec) -> R<GearCircles> {
    let z = spec.teeth as f64;
    if let Some(b) = belt(spec.form).filter(|_| spec.kind == GearKind::Pulley) {
        let pitch = z * b.pitch / TAU;
        return Ok(GearCircles { module_t: b.pitch / PI, alpha_t: 0.0, pitch, base: pitch, tip: pitch - b.offset, root: pitch - b.offset - (b.depth + spec.backlash / 2.0), thickness: 0.0 });
    }
    if let Some(c) = chain(spec.form).filter(|_| spec.kind == GearKind::Sprocket) {
        let pitch = c.pitch / (2.0 * (PI / z).sin());
        return Ok(GearCircles {
            module_t: c.pitch / PI,
            alpha_t: 0.0,
            pitch,
            base: pitch,
            tip: c.pitch * (0.6 + 1.0 / (PI / z).tan()) / 2.0,
            root: pitch - (0.505 * c.roller + spec.backlash / 2.0),
            thickness: 0.0,
        });
    }
    bad("no es una polea ni un piñón de cadena")
}

/// ¿El tipo es una polea o un piñón? Perfil del contorno para esos tipos.
pub(crate) fn wheel_profile(spec: &GearSpec) -> R<GearProfile> {
    match spec.kind {
        GearKind::Pulley => pulley_profile(spec.teeth, &belt(spec.form).ok_or_else(|| GearError::Invalid("elige una correa para la polea".into()))?, spec.backlash),
        GearKind::Sprocket => sprocket_profile(spec.teeth, &chain(spec.form).ok_or_else(|| GearError::Invalid("elige una cadena para el piñón".into()))?, spec.backlash),
        _ => bad("no es una polea ni un piñón de cadena"),
    }
}
