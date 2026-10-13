//! Geometría de engranes de evolvente (Rust puro, sin OpenCASCADE).
//!
//! El perfil sale de fórmulas, no del solver: el flanco es la evolvente exacta
//! del círculo base y el pie es el **trocoide de la herramienta** (cremallera
//! generadora con punta redondeada rodando sobre el círculo primitivo), así que
//! el socavado con pocos dientes sale solo, sin inventar un arco.
//!
//! Unidades: mm y grados en [`GearSpec`]; ángulos internos en radianes.
//! Convención del perfil: el centro del engrane en el origen, dientes en
//! sentido antihorario, el centro de un *hueco* en el ángulo 0.
//!
//! Corrección de perfil `shift` (x): la cremallera se aleja del eje `x·m`.
//! En un engrane interno `x > 0` deja el diente más fino y la cabeza más corta
//! (es el hueco del engrane exterior imaginario de igual corrección).

use cad_occt::{Curve, P3};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GearError {
    #[error("Engrane inválido: {0}")]
    Invalid(String),
    #[error("Los engranes no se pueden acoplar: {0}")]
    Mismatch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GearKind {
    #[default]
    External,
    /// Corona de anillo: los dientes miran al eje
    Internal,
    Rack,
    /// Cónico recto (aproximación de Tredgold): módulo y dientes del extremo grande
    Bevel,
    /// Tornillo sin fin de filete trapecial: `module` axial, `teeth` = entradas
    Worm,
}

/// Receta de un engrane. Con `helix_angle ≠ 0` el módulo y el ángulo de
/// presión son los **normales**; el perfil se calcula en el plano transversal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GearSpec {
    pub kind: GearKind,
    /// Módulo normal (mm)
    pub module: f64,
    pub teeth: u32,
    /// Ángulo de presión normal (°)
    pub pressure_angle: f64,
    /// Ángulo de hélice en el primitivo (°); + mano derecha, − izquierda
    pub helix_angle: f64,
    /// Corrección de perfil x (adimensional)
    pub shift: f64,
    /// Juego entre dientes: se resta del espesor del diente (mm, normal)
    pub backlash: f64,
    /// Coeficientes de cabeza y pie (× módulo)
    pub addendum: f64,
    pub dedendum: f64,
    /// Radio de la punta de la herramienta (× módulo); redondea el pie
    pub root_fillet: f64,
    /// Cónico: semiángulo del cono primitivo (°)
    pub cone_angle: f64,
    /// Cónico: ángulo entre los ejes del par (°); lo usa el engrane que engrana
    pub shaft_angle: f64,
    /// Sin fin: diámetro primitivo (mm; 0: 10·módulo) y mano izquierda
    pub worm_diameter: f64,
    pub left_hand: bool,
}

impl Default for GearSpec {
    fn default() -> Self {
        Self {
            kind: GearKind::External,
            module: 1.0,
            teeth: 20,
            pressure_angle: 20.0,
            helix_angle: 0.0,
            shift: 0.0,
            backlash: 0.0,
            addendum: 1.0,
            dedendum: 1.25,
            root_fillet: 0.25,
            cone_angle: 45.0,
            shaft_angle: 90.0,
            worm_diameter: 0.0,
            left_hand: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GearWarning {
    /// La herramienta corta bajo el círculo base: el flanco empieza más arriba
    Undercut { min_teeth: f64 },
    /// El diente se afila antes de la cabeza: se recortó el círculo de cabeza
    Pointed,
    /// Espesor en la cabeza < 0,2·m: diente débil
    ThinTip,
    /// La cabeza no sale del círculo base: no hay evolvente útil
    NoInvolute,
    /// La corona imaginaria del pie sube sobre el círculo base (z muy bajo)
    FewTeeth,
}

impl GearWarning {
    pub fn message(&self) -> String {
        match self {
            GearWarning::Undercut { min_teeth } => {
                format!("Socavado del pie: con este módulo y corrección harían falta al menos {:.1} dientes", min_teeth)
            }
            GearWarning::Pointed => "El diente termina en punta: se recortó el círculo de cabeza".into(),
            GearWarning::ThinTip => "Diente muy fino en la cabeza (menos de 0,2·módulo)".into(),
            GearWarning::NoInvolute => "La cabeza no sale del círculo base".into(),
            GearWarning::FewTeeth => "Muy pocos dientes para un perfil de evolvente".into(),
        }
    }
}

/// Medidas derivadas, en el plano transversal (mm, rad)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearCircles {
    /// Módulo y ángulo de presión transversales
    pub module_t: f64,
    pub alpha_t: f64,
    /// Primitivo, base, cabeza y pie. En un engrane interno la cabeza es el
    /// radio **menor** (mira al eje) y el pie el mayor.
    pub pitch: f64,
    pub base: f64,
    pub tip: f64,
    pub root: f64,
    /// Espesor del diente en el primitivo (arco, transversal)
    pub thickness: f64,
}

impl GearSpec {
    fn check(&self) -> Result<(), GearError> {
        let bad = |m: &str| Err(GearError::Invalid(m.into()));
        if self.module.is_nan() || self.module <= 0.0 || !self.module.is_finite() {
            return bad("el módulo debe ser positivo");
        }
        if !(1.0..=45.0).contains(&self.pressure_angle) {
            return bad("el ángulo de presión debe estar entre 1° y 45°");
        }
        if self.helix_angle.abs() >= 60.0 {
            return bad("el ángulo de hélice debe ser menor que 60°");
        }
        if self.kind == GearKind::Bevel {
            if self.helix_angle != 0.0 {
                return bad("el cónico recto no lleva ángulo de hélice");
            }
            if !(5.0..=85.0).contains(&self.cone_angle) || !(10.0..=170.0).contains(&self.shaft_angle) {
                return bad("el ángulo del cono va de 5° a 85° y el de los ejes de 10° a 170°");
            }
        }
        if self.kind == GearKind::Worm {
            if !(1..=8).contains(&self.teeth) {
                return bad("el sin fin tiene de 1 a 8 entradas");
            }
            if self.helix_angle != 0.0 {
                return bad("el sin fin no lleva ángulo de hélice: sale del diámetro primitivo y las entradas");
            }
            if self.worm_diameter != 0.0 && self.worm_diameter <= 2.0 * self.dedendum * self.module + 1.0 {
                return bad("el diámetro primitivo del sin fin es muy chico para el pie del filete");
            }
        } else if self.kind != GearKind::Rack && self.teeth < 4 {
            return bad("hacen falta al menos 4 dientes");
        }
        if self.kind == GearKind::Rack && self.teeth < 1 {
            return bad("hace falta al menos 1 diente");
        }
        if self.addendum <= 0.0 || self.dedendum <= 0.0 || self.root_fillet < 0.0 || self.backlash < 0.0 {
            return bad("cabeza, pie, redondeo y juego no pueden ser negativos");
        }
        Ok(())
    }

    fn beta(&self) -> f64 {
        self.helix_angle.to_radians()
    }

    pub fn module_t(&self) -> f64 {
        self.module / self.beta().cos()
    }

    pub fn pressure_angle_t(&self) -> f64 {
        (self.pressure_angle.to_radians().tan() / self.beta().cos()).atan()
    }

    /// Espesor normal del diente en el primitivo
    fn thickness_n(&self) -> f64 {
        let a = self.pressure_angle.to_radians();
        let sign = if self.kind == GearKind::Internal { -1.0 } else { 1.0 };
        PI * self.module / 2.0 + sign * 2.0 * self.shift * self.module * a.tan() - self.backlash / 2.0
    }

    /// Sin fin: diámetro primitivo, ángulo de avance (rad) y avance por vuelta
    pub fn worm_dims(&self) -> (f64, f64, f64) {
        let d1 = if self.worm_diameter > 0.0 { self.worm_diameter } else { 10.0 * self.module };
        let lead = self.teeth as f64 * PI * self.module;
        ((d1), (lead / (PI * d1)).atan(), lead)
    }

    /// Perfil axial del filete del sin fin: (r_núcleo, r_cresta, semiancho en el
    /// núcleo, semiancho en la cresta), semianchos a lo largo del eje
    pub fn worm_profile(&self) -> Result<(f64, f64, f64, f64), GearError> {
        self.check()?;
        let (d1, _, _) = self.worm_dims();
        let (m, rp) = (self.module, d1 / 2.0);
        let (r_root, r_tip) = (rp - self.dedendum * m, rp + self.addendum * m);
        let tan_a = self.pressure_angle.to_radians().tan();
        let half = PI * m / 4.0 - self.backlash / 4.0;
        let (hw_in, hw_out) = (half + (rp - r_root) * tan_a, half - (r_tip - rp) * tan_a);
        if r_root <= 0.5 || hw_out < 0.05 * m {
            return Err(GearError::Invalid("el filete del sin fin termina en punta antes de la cresta o el núcleo no cabe".into()));
        }
        if hw_in >= PI * m / 2.0 - 0.02 * m {
            return Err(GearError::Invalid("los filetes del sin fin se tocan en el fondo: baja el pie o sube el ángulo de presión".into()));
        }
        Ok((r_root, r_tip, hw_in, hw_out))
    }

    pub fn circles(&self) -> Result<GearCircles, GearError> {
        self.check()?;
        if self.kind == GearKind::Worm {
            let (r_root, r_tip, _, _) = self.worm_profile()?;
            let (d1, _, _) = self.worm_dims();
            let a = self.pressure_angle.to_radians();
            return Ok(GearCircles { module_t: self.module, alpha_t: a, pitch: d1 / 2.0, base: d1 / 2.0, tip: r_tip, root: r_root, thickness: PI * self.module / 2.0 - self.backlash / 2.0 });
        }
        let (m, mt, at) = (self.module, self.module_t(), self.pressure_angle_t());
        let pitch = mt * self.teeth as f64 / 2.0;
        let base = pitch * at.cos();
        let x = self.shift;
        let (tip, root) = match self.kind {
            GearKind::Internal => (pitch - (self.addendum - x) * m, pitch + (self.dedendum + x) * m),
            // Las alturas se miden sobre el cono de atrás: en el radio valen cos δ
            GearKind::Bevel => {
                let c = self.cone_angle.to_radians().cos();
                (pitch + (self.addendum + x) * m * c, pitch - (self.dedendum - x) * m * c)
            }
            _ => (pitch + (self.addendum + x) * m, pitch - (self.dedendum - x) * m),
        };
        if self.kind != GearKind::Rack && (root <= 0.0 || tip <= 0.0) {
            return Err(GearError::Invalid("el círculo de pie o de cabeza cae en el eje".into()));
        }
        Ok(GearCircles { module_t: mt, alpha_t: at, pitch, base, tip, root, thickness: self.thickness_n() / self.beta().cos() })
    }

    /// Dientes mínimos sin socavado para la herramienta de este engrane
    pub fn min_teeth_no_undercut(&self) -> f64 {
        let a = self.pressure_angle.to_radians();
        let straight = self.dedendum_tool() - self.root_fillet * (1.0 - a.sin());
        2.0 * (straight - self.shift) / (a.sin().powi(2))
    }

    /// Profundidad de la herramienta bajo su línea primitiva (× m)
    fn dedendum_tool(&self) -> f64 {
        match self.kind {
            GearKind::Internal => self.addendum,
            _ => self.dedendum,
        }
    }
}

fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// Datos del generador: engrane exterior (o el imaginario de uno interno)
#[derive(Debug, Clone, Copy)]
struct Core {
    r: f64,
    n: f64,
    alpha: f64,
    /// Espesor del diente en el primitivo (arco)
    s: f64,
    ra: f64,
    rf: f64,
    rho: f64,
}

/// Medio perfil de un diente del lado de ángulos positivos del hueco
struct Half {
    /// Medio ángulo del arco de pie (desde el centro del hueco)
    root_half: f64,
    /// Trocoide: del final del arco de pie hacia el flanco
    trochoid: Vec<[f64; 2]>,
    /// Evolvente: de su comienzo hasta la cabeza
    involute: Vec<[f64; 2]>,
    /// Radio de cabeza efectivo y su ángulo
    tip_r: f64,
    tip_angle: f64,
    undercut: bool,
    pointed: bool,
    start_r: f64,
}

fn half_thickness_angle(c: &Core, rb: f64, ry: f64) -> f64 {
    let ay = (rb / ry).clamp(-1.0, 1.0).acos();
    c.s / (2.0 * c.r) + inv(c.alpha) - inv(ay)
}

fn polar(r: f64, a: f64) -> [f64; 2] {
    [r * a.cos(), r * a.sin()]
}

const TROCHOID_SAMPLES: usize = 24;
const INVOLUTE_SAMPLES: usize = 20;

fn half_profile(c: &Core) -> Result<Half, GearError> {
    let rb = c.r * c.alpha.cos();
    let tau = TAU / c.n;
    let pitch = tau * c.r;
    let (tan_a, cos_a) = (c.alpha.tan(), c.alpha.cos());
    // Cremallera generadora: espesor de su diente en la línea de rodadura
    let t_r = pitch - c.s;
    let w_tip = c.r - c.rf;
    let rho = c.rho;
    let w_c = w_tip - rho;
    let u_c = (t_r / 2.0 - w_c * tan_a) - rho / cos_a;
    if w_c < 0.0 {
        return Err(GearError::Invalid("el pie es menos profundo que el redondeo de la herramienta".into()));
    }
    if u_c < 0.0 {
        return Err(GearError::Invalid("el hueco es demasiado angosto para el redondeo del pie".into()));
    }
    let phi_b = u_c / c.r;
    let phi_t = (u_c - w_c / tan_a) / c.r;

    // Punto del trocoide (envolvente del redondeo) cuando la cremallera rodó φ
    let troch = |phi: f64| -> [f64; 2] {
        let n = [phi.cos(), phi.sin()];
        let t = [-phi.sin(), phi.cos()];
        let k = u_c - c.r * phi;
        let cx = c.r * n[0] + k * t[0] - w_c * n[0];
        let cy = c.r * n[1] + k * t[1] - w_c * n[1];
        let (dx, dy) = (cx - c.r * n[0], cy - c.r * n[1]);
        let d = dx.hypot(dy);
        if d < 1e-12 || rho == 0.0 { [cx, cy] } else { [cx + rho * dx / d, cy + rho * dy / d] }
    };
    // Ángulo del flanco evolvente en el radio ry (lado del hueco), si existe
    let flank_angle = |ry: f64| -> Option<f64> {
        (ry >= rb).then(|| tau / 2.0 - half_thickness_angle(c, rb, ry))
    };
    // g > 0: el trocoide ya entró en el material del diente
    let g = |phi: f64| -> f64 {
        let p = troch(phi);
        let (ry, th) = (p[0].hypot(p[1]), p[1].atan2(p[0]));
        match flank_angle(ry) {
            Some(f) => th - f,
            None => f64::NEG_INFINITY,
        }
    };

    // Cabeza efectiva: si el diente se afila antes, se recorta
    let mut tip_r = c.ra;
    let mut pointed = false;
    if half_thickness_angle(c, rb, tip_r) < 0.0 {
        pointed = true;
        let (mut lo, mut hi) = (rb.max(c.r), tip_r);
        for _ in 0..80 {
            let mid = (lo + hi) / 2.0;
            if half_thickness_angle(c, rb, mid) > 0.0 { lo = mid } else { hi = mid }
        }
        tip_r = lo;
    }
    if tip_r <= rb {
        return Err(GearError::Invalid("la cabeza no sale del círculo base".into()));
    }

    // Trocoide hasta φ_t o hasta cruzar la evolvente (socavado)
    const TOL: f64 = 1e-7;
    let mut trochoid = vec![troch(phi_b)];
    let mut undercut = false;
    let mut last_phi = phi_b;
    if phi_t < phi_b && rho > 0.0 {
        let steps = TROCHOID_SAMPLES;
        for i in 1..=steps {
            let phi = phi_b + (phi_t - phi_b) * i as f64 / steps as f64;
            if g(phi) > TOL {
                // Cruce entre last_phi y phi
                let (mut a, mut b) = (last_phi, phi);
                for _ in 0..60 {
                    let mid = (a + b) / 2.0;
                    if g(mid) > 0.0 { b = mid } else { a = mid }
                }
                trochoid.push(troch((a + b) / 2.0));
                undercut = true;
                break;
            }
            trochoid.push(troch(phi));
            last_phi = phi;
        }
    }
    let end = *trochoid.last().unwrap();
    let start_r = end[0].hypot(end[1]).max(rb);
    if start_r >= tip_r {
        return Err(GearError::Invalid("el flanco útil es nulo: aumenta los dientes o la cabeza".into()));
    }

    // Evolvente de start_r a tip_r, repartida en el ángulo de rodadura
    let (t0, t1) = (((start_r / rb).powi(2) - 1.0).sqrt(), ((tip_r / rb).powi(2) - 1.0).sqrt());
    // El primer punto es el final del trocoide: encadenan sin hueco (en el
    // socavado profundo el cruce cae en el círculo base y queda un pico de
    // centésimas de mm)
    let involute = (0..=INVOLUTE_SAMPLES)
        .map(|i| {
            let t = t0 + (t1 - t0) * i as f64 / INVOLUTE_SAMPLES as f64;
            let ry = rb * (1.0 + t * t).sqrt();
            if i == 0 { end } else { polar(ry, flank_angle(ry).unwrap()) }
        })
        .collect();
    let tip_angle = flank_angle(tip_r).unwrap();
    Ok(Half { root_half: phi_b, trochoid, involute, tip_r, tip_angle, undercut, pointed, start_r })
}

/// Un período (hueco + diente) del exterior, ya recorrido de 0 a τ
struct Period {
    /// Puntos densos del contorno, sin repetir el último
    points: Vec<[f64; 2]>,
    curves: Vec<Curve>,
    half: Half,
}

fn p3(p: [f64; 2]) -> P3 {
    [p[0], p[1], 0.0]
}

fn mirror(p: [f64; 2], tau: f64) -> [f64; 2] {
    let (r, a) = (p[0].hypot(p[1]), p[1].atan2(p[0]));
    polar(r, tau - a)
}

fn arc_curve(r: f64, a0: f64, a1: f64) -> Curve {
    Curve::Arc(p3(polar(r, a0)), p3(polar(r, (a0 + a1) / 2.0)), p3(polar(r, a1)))
}

fn unit(v: [f64; 2]) -> [f64; 2] {
    let d = v[0].hypot(v[1]);
    [v[0] / d, v[1] / d]
}

fn spline(points: &[[f64; 2]], t0: [f64; 2], t1: [f64; 2]) -> Curve {
    Curve::SplineEnds { points: points.iter().map(|&p| p3(p)).collect(), start: p3(unit(t0)), end: p3(unit(t1)) }
}

fn period(c: &Core, rb: f64) -> Result<Period, GearError> {
    let half = half_profile(c)?;
    let tau = TAU / c.n;
    let (rf, a_root) = (c.rf, half.root_half);
    let mut points = Vec::new();
    let mut curves = Vec::new();

    // Dirección de un punto al siguiente, para las tangentes de los extremos
    let dir = |a: [f64; 2], b: [f64; 2]| [b[0] - a[0], b[1] - a[1]];
    let tro = &half.trochoid;
    let inv_pts = &half.involute;
    let inv_tan = |p: [f64; 2]| {
        let (ry, th) = (p[0].hypot(p[1]), p[1].atan2(p[0]));
        let ay = (rb / ry).clamp(-1.0, 1.0).acos();
        [(th + ay).cos(), (th + ay).sin()]
    };
    let mirror_vec = |v: [f64; 2], at: [f64; 2]| {
        // Refleja una dirección con la misma reflexión que mueve `at`
        let q = mirror([at[0] + v[0] * 1e-3, at[1] + v[1] * 1e-3], tau);
        let m = mirror(at, tau);
        [(q[0] - m[0]) * 1e3, (q[1] - m[1]) * 1e3]
    };

    // Lado positivo: arco de pie [0, φ_b], trocoide, evolvente, cabeza
    // Para que el período empiece en el centro del hueco, el arco de pie
    // va de 0 a φ_b aquí y de τ−φ_b a τ al final (espejo).
    let root_arc = |a0: f64, a1: f64, curves: &mut Vec<Curve>| {
        if a1 - a0 > 1e-12 {
            curves.push(arc_curve(rf, a0, a1));
        }
    };
    root_arc(0.0, a_root, &mut curves);
    points.push(polar(rf, 0.0));
    if a_root > 1e-12 {
        points.push(polar(rf, a_root / 2.0));
    }
    // Trocoide (si tiene más de un punto)
    let joined = tro.len() < 2;
    if !joined {
        let t_start = dir(tro[0], tro[1]);
        let t_end = if half.undercut { dir(tro[tro.len() - 2], tro[tro.len() - 1]) } else { inv_tan(*tro.last().unwrap()) };
        curves.push(spline(tro, t_start, t_end));
        points.extend(tro.iter().copied());
    } else {
        points.push(tro[0]);
    }
    let it_start = inv_tan(inv_pts[0]);
    let it_end = inv_tan(*inv_pts.last().unwrap());
    curves.push(spline(inv_pts, it_start, it_end));
    points.extend(inv_pts.iter().copied());

    // Cabeza: del flanco positivo al espejo
    let tip_a0 = half.tip_angle;
    let tip_a1 = tau - half.tip_angle;
    if tip_a1 - tip_a0 > 1e-12 {
        curves.push(arc_curve(half.tip_r, tip_a0, tip_a1));
        points.push(polar(half.tip_r, (tip_a0 + tip_a1) / 2.0));
    }

    // Lado espejo, recorrido al revés
    let mut inv_m: Vec<[f64; 2]> = inv_pts.iter().rev().map(|&p| mirror(p, tau)).collect();
    let tail_inv_start = mirror_vec(inv_tan(*inv_pts.last().unwrap()), *inv_pts.last().unwrap());
    let tail_inv_end = mirror_vec(inv_tan(inv_pts[0]), inv_pts[0]);
    curves.push(spline(&inv_m, [-tail_inv_start[0], -tail_inv_start[1]], [-tail_inv_end[0], -tail_inv_end[1]]));
    points.append(&mut inv_m);
    if !joined {
        let tro_m: Vec<[f64; 2]> = tro.iter().rev().map(|&p| mirror(p, tau)).collect();
        let n = tro_m.len();
        let t_start = dir(tro_m[0], tro_m[1]);
        let t_end = dir(tro_m[n - 2], tro_m[n - 1]);
        curves.push(spline(&tro_m, t_start, t_end));
        points.extend(tro_m.iter().copied());
    } else {
        points.push(mirror(tro[0], tau));
    }
    if a_root > 1e-12 {
        points.push(polar(rf, tau - a_root / 2.0));
    }
    root_arc(tau - a_root, tau, &mut curves);
    Ok(Period { points, curves, half })
}

fn map_curve(c: &Curve, f: &impl Fn([f64; 2]) -> [f64; 2]) -> Curve {
    let m = |p: &P3| {
        let q = f([p[0], p[1]]);
        [q[0], q[1], p[2]]
    };
    match c {
        Curve::Arc(a, mid, b) => Curve::Arc(m(a), m(mid), m(b)),
        Curve::SplineEnds { points, start, end } => {
            let dir = |p: &P3, t: &P3| {
                let eps = 1e-6 * p[0].hypot(p[1]).max(1e-3);
                let (a, b) = (f([p[0], p[1]]), f([p[0] + t[0] * eps, p[1] + t[1] * eps]));
                let d = [b[0] - a[0], b[1] - a[1]];
                let n = d[0].hypot(d[1]);
                [d[0] / n, d[1] / n, 0.0]
            };
            let (p0, p1) = (points.first().unwrap(), points.last().unwrap());
            Curve::SplineEnds { points: points.iter().map(m).collect(), start: dir(p0, start), end: dir(p1, end) }
        }
        other => other.clone(),
    }
}

fn rotate(p: [f64; 2], a: f64) -> [f64; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

pub(crate) fn rotate_curve(c: &Curve, a: f64) -> Curve {
    let r = |p: &P3| {
        let q = rotate([p[0], p[1]], a);
        [q[0], q[1], p[2]]
    };
    let rd = |p: &P3| r(p);
    match c {
        Curve::Arc(p, m, q) => Curve::Arc(r(p), r(m), r(q)),
        Curve::SplineEnds { points, start, end } => Curve::SplineEnds { points: points.iter().map(r).collect(), start: rd(start), end: rd(end) },
        other => other.clone(),
    }
}

/// Resultado de generar el contorno de un engrane
#[derive(Debug, Clone)]
pub struct GearProfile {
    pub circles: GearCircles,
    /// Contorno cerrado, antihorario, sin repetir el primer punto
    pub points: Vec<[f64; 2]>,
    /// El mismo contorno como curvas encadenadas (plano XY)
    pub curves: Vec<Curve>,
    pub warnings: Vec<GearWarning>,
    /// Radio a partir del cual el flanco es evolvente
    pub involute_start: f64,
    /// Círculo de cabeza efectivo (puede ser menor que el nominal si hubo punta)
    pub tip_radius: f64,
}

impl GearSpec {
    fn core(&self) -> Result<Core, GearError> {
        let c = self.circles()?;
        if self.kind == GearKind::Bevel {
            // Tredgold: engrane recto virtual en el cono de atrás
            let d = self.cone_angle.to_radians().cos();
            let (r, x, m) = (c.pitch / d, self.shift, self.module);
            let core = Core { r, n: self.teeth as f64 / d, alpha: c.alpha_t, s: c.thickness, ra: r + (self.addendum + x) * m, rf: r - (self.dedendum - x) * m, rho: self.root_fillet * m };
            return Ok(core);
        }
        let (ra, rf, s) = match self.kind {
            // El interno es el hueco de un exterior imaginario: espesor πp − s
            GearKind::Internal => (c.root, c.tip, TAU * c.pitch / self.teeth as f64 - c.thickness),
            _ => (c.tip, c.root, c.thickness),
        };
        Ok(Core { r: c.pitch, n: self.teeth as f64, alpha: c.alpha_t, s, ra, rf, rho: self.root_fillet * self.module })
    }

    /// Perfil completo de un engrane exterior o interno
    pub fn profile(&self) -> Result<GearProfile, GearError> {
        if self.kind == GearKind::Rack {
            return Err(GearError::Invalid("la cremallera no tiene perfil circular: usa `rack_outline`".into()));
        }
        if self.kind == GearKind::Worm {
            return Err(GearError::Invalid("el sin fin no tiene perfil de dientes: el filete sale de `worm_profile`".into()));
        }
        let circles = self.circles()?;
        let core = self.core()?;
        let rb = core.r * core.alpha.cos();
        let per = period(&core, rb)?;
        let tau = TAU / self.teeth as f64;
        let mut warnings = Vec::new();
        if per.half.undercut {
            warnings.push(GearWarning::Undercut { min_teeth: self.min_teeth_no_undercut() });
        }
        if per.half.pointed {
            warnings.push(GearWarning::Pointed);
        }
        // Espesor en la cabeza (arco) del exterior imaginario
        let tip_t = 2.0 * per.half.tip_r * half_thickness_angle(&core, rb, per.half.tip_r).max(0.0);
        if !per.half.pointed && tip_t < 0.2 * self.module {
            warnings.push(GearWarning::ThinTip);
        }
        if self.teeth < 8 {
            warnings.push(GearWarning::FewTeeth);
        }
        let (per_points, per_curves) = if self.kind == GearKind::Bevel {
            let f = self.bevel_map()?;
            (per.points.iter().map(|&p| f(p)).collect::<Vec<_>>(), per.curves.iter().map(|c| map_curve(c, &f)).collect::<Vec<_>>())
        } else {
            (per.points.clone(), per.curves.clone())
        };
        let mut points = Vec::with_capacity(per_points.len() * self.teeth as usize);
        let mut curves = Vec::with_capacity(per_curves.len() * self.teeth as usize);
        for k in 0..self.teeth {
            let a = tau * k as f64;
            points.extend(per_points.iter().map(|&p| rotate(p, a)));
            curves.extend(per_curves.iter().map(|c| rotate_curve(c, a)));
        }
        Ok(GearProfile {
            circles,
            points,
            curves,
            warnings,
            involute_start: per.half.start_r,
            tip_radius: per.half.tip_r,
        })
    }

    /// Plano del perfil plano del cónico: el extremo grande, perpendicular al
    /// eje y por el punto primitivo (Z = 0)
    pub fn bevel_plane(&self) -> f64 {
        0.0
    }

    /// Semiángulo del cono primitivo y alto del vértice sobre el extremo grande
    pub fn apex_height(&self) -> f64 {
        let d = self.cone_angle.to_radians();
        self.module * self.teeth as f64 / 2.0 / d.tan()
    }

    /// Distancia del vértice al extremo grande, sobre el cono primitivo
    pub fn cone_distance(&self) -> f64 {
        self.module * self.teeth as f64 / 2.0 / self.cone_angle.to_radians().sin()
    }

    /// Del perfil recto virtual al perfil plano del cónico: cada punto del cono
    /// de atrás se lleva al plano del extremo grande por una recta que pasa por
    /// el vértice (los flancos son rectas por el vértice, así que el sólido es
    /// el cono desde ese perfil hasta el vértice). Radio: ρ → λρcosδ; ángulo: θ/cosδ.
    pub(crate) fn bevel_map(&self) -> Result<impl Fn([f64; 2]) -> [f64; 2] + use<>, GearError> {
        self.check()?;
        let delta = self.cone_angle.to_radians();
        let (d, r) = (delta.cos(), self.module * self.teeth as f64 / 2.0);
        let h = r / delta.tan();
        let zp = self.bevel_plane();
        Ok(move |p: [f64; 2]| {
            let (rho, th) = (p[0].hypot(p[1]), p[1].atan2(p[0]));
            let qz = -r * delta.tan() + rho * delta.sin();
            let lambda = (h - zp) / (h - qz);
            polar(lambda * rho * d, th / d)
        })
    }

    /// Un período del perfil (un diente y un hueco), para patrón circular
    pub fn tooth_curves(&self) -> Result<Vec<Curve>, GearError> {
        if self.kind == GearKind::Rack {
            return Err(GearError::Invalid("la cremallera no tiene perfil circular".into()));
        }
        let core = self.core()?;
        Ok(period(&core, core.r * core.alpha.cos())?.curves)
    }

    /// Contorno de una cremallera de `n` dientes con un cuerpo de `body` mm
    /// bajo el pie. X a lo largo, Y hacia las cabezas, Y=0 en la línea de
    /// rodadura; el primer diente centrado en X=0.
    pub fn rack_outline(&self, n: u32, body: f64) -> Result<Vec<[f64; 2]>, GearError> {
        self.check()?;
        if n == 0 || body <= 0.0 {
            return Err(GearError::Invalid("la cremallera necesita dientes y un cuerpo con alto".into()));
        }
        let (m, mt) = (self.module, self.module_t());
        let at = self.pressure_angle_t();
        let p = PI * mt;
        let s = self.thickness_n() / self.beta().cos();
        let (ha, hf) = ((self.addendum + self.shift) * m, (self.dedendum - self.shift) * m);
        let tan_a = at.tan();
        let mut pts = vec![[-p / 2.0, -hf - body]];
        for k in 0..n {
            let cx = p * k as f64;
            // Flancos: semiespesor en y = s/2 − y·tanα
            let half = |y: f64| s / 2.0 - y * tan_a;
            if half(ha) <= 0.0 {
                return Err(GearError::Invalid("la cremallera termina en punta antes de la cabeza".into()));
            }
            pts.push([cx - p / 2.0, -hf]);
            pts.push([cx - half(-hf), -hf]);
            pts.push([cx - half(ha), ha]);
            pts.push([cx + half(ha), ha]);
            pts.push([cx + half(-hf), -hf]);
            pts.push([cx + p / 2.0, -hf]);
        }
        pts.push([p * (n - 1) as f64 + p / 2.0, -hf - body]);
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
        Ok(pts)
    }
}

/// Datos de dos engranes acoplados
#[derive(Debug, Clone)]
pub struct GearPair {
    /// Distancia entre centros (mm)
    pub center_distance: f64,
    /// Ángulo de presión de funcionamiento (transversal, rad)
    pub working_angle: f64,
    /// Razón de contacto transversal
    pub contact_ratio: f64,
    /// z₂/z₁ (velocidad de giro de 1 sobre la de 2)
    pub ratio: f64,
    pub interference: bool,
}

/// Acopla dos engranes: exterior-exterior o exterior (1) dentro de interno (2)
pub fn pair(a: &GearSpec, b: &GearSpec) -> Result<GearPair, GearError> {
    if (a.module - b.module).abs() > 1e-9 {
        return Err(GearError::Mismatch(format!("módulos distintos ({} y {})", a.module, b.module)));
    }
    if (a.pressure_angle - b.pressure_angle).abs() > 1e-9 {
        return Err(GearError::Mismatch("ángulos de presión distintos".into()));
    }
    if (a.helix_angle.abs() - b.helix_angle.abs()).abs() > 1e-9 {
        return Err(GearError::Mismatch("ángulos de hélice distintos".into()));
    }
    // Ejes paralelos: exterior con exterior pide manos contrarias; con corona, la misma
    if a.helix_angle != 0.0 {
        let same = a.helix_angle.signum() == b.helix_angle.signum();
        if same != (a.kind != b.kind) {
            return Err(GearError::Mismatch(if same { "la misma mano: los ejes quedarían cruzados, usa manos contrarias" } else { "con corona las dos van con la misma mano" }.into()));
        }
    }
    let internal = match (a.kind, b.kind) {
        (GearKind::External, GearKind::External) => false,
        (GearKind::External, GearKind::Internal) => true,
        _ => return Err(GearError::Mismatch("solo exterior con exterior o exterior dentro de interno".into())),
    };
    let (c1, c2) = (a.circles()?, b.circles()?);
    let (z1, z2) = (a.teeth as f64, b.teeth as f64);
    if internal && b.teeth <= a.teeth {
        return Err(GearError::Mismatch("la corona debe tener más dientes que el piñón".into()));
    }
    let (mt, at) = (c1.module_t, c1.alpha_t);
    let tan_n = a.pressure_angle.to_radians().tan();
    let (dx, dz) = if internal { (b.shift - a.shift, z2 - z1) } else { (a.shift + b.shift, z1 + z2) };
    let inv_w = 2.0 * dx / dz * tan_n + inv(at);
    // Resolver inv(α_w) = inv_w por Newton
    let mut aw = at;
    for _ in 0..50 {
        let f = inv(aw) - inv_w;
        let df = aw.tan().powi(2);
        if df < 1e-14 {
            break;
        }
        aw -= f / df;
    }
    let a_c = mt * dz / 2.0 * at.cos() / aw.cos();
    let pb = PI * mt * at.cos();
    let t = |r: f64, rb: f64| (r * r - rb * rb).max(0.0).sqrt();
    let n1n2 = a_c * aw.sin();
    let (contact, interference) = if internal {
        let (ts, te) = (n1n2 - t(c2.tip, c2.base), t(c1.tip, c1.base));
        ((te - ts) / pb, ts < 0.0)
    } else {
        let (t1, t2) = (t(c1.tip, c1.base), t(c2.tip, c2.base));
        ((t1 + t2 - n1n2) / pb, t1 > n1n2 + 1e-9 || t2 > n1n2 + 1e-9)
    };
    Ok(GearPair { center_distance: a_c, working_angle: aw, contact_ratio: contact, ratio: z2 / z1, interference })
}
