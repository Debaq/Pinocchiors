//! Ensambles: instancias de las piezas del diseño, cada una con su posición y
//! giro, unidas por relaciones entre conectores (sistemas de coordenadas
//! sobre las piezas), como los *mates* de Onshape. El solver mueve las
//! instancias libres (6 incógnitas cada una) hasta cumplir las relaciones.

use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};

use crate::eval::Evaluation;
use crate::feature::PartId;
use crate::geom::*;
use cad_occt::Shape;
use std::f64::consts::TAU;

/// Ensamble guardado en el documento.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Assembly {
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub mates: Vec<Mate>,
    /// Acoplamientos entre uniones (engranes, piñón-cremallera, tornillo).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub next_id: u32,
}

/// Acopla el movimiento de dos uniones ya existentes (`Mate`): una ecuación
/// extra en el solver. Si una de las dos tiene su ángulo o distancia impuestos
/// (la que se anima), la otra sale de ahí; si ninguna, el solver las acopla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Relation {
    /// Dos revolutas: `ángulo_b = razón · ángulo_a + fase` (° ). La razón lleva
    /// el signo: negativa si giran al revés (engranes exteriores). Con `carrier`
    /// (planetarios: uno de los dos va montado en el brazo porta-satélites) suma
    /// `razón_brazo · ángulo del brazo`: la unión del brazo con la carcasa.
    Gear {
        a: u32,
        b: u32,
        ratio: f64,
        #[serde(default)]
        phase: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        carrier: Option<u32>,
        #[serde(default, skip_serializing_if = "is_zero")]
        carrier_ratio: f64,
    },
    /// Revoluta y deslizante: `distancia = por_vuelta · ángulo/360 + fase` (mm)
    RackPinion {
        revolute: u32,
        slider: u32,
        per_turn: f64,
        #[serde(default)]
        phase: f64,
    },
    /// Cilíndrica: `distancia = paso · ángulo/360 + fase` (mm)
    Screw {
        cylindrical: u32,
        pitch: f64,
        #[serde(default)]
        phase: f64,
    },
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

fn is_zero_u32(x: &u32) -> bool {
    *x == 0
}

impl Relation {
    pub fn mates(&self) -> Vec<u32> {
        match self {
            Relation::Gear { a, b, carrier, .. } => [*a, *b].into_iter().chain(*carrier).collect(),
            Relation::RackPinion { revolute, slider, .. } => vec![*revolute, *slider],
            Relation::Screw { cylindrical, .. } => vec![*cylindrical],
        }
    }
}

/// Una pieza del diseño puesta en el ensamble.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: u32,
    /// Pestaña de diseño de la pieza (en un ensamble dentro del diseño, o de
    /// los proyectos de antes: 0, sin uso)
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub studio: crate::tabs::TabId,
    pub part: PartId,
    pub name: String,
    /// Traslación (mm) y giro como vector (eje × ángulo en radianes), aplicados a la pieza.
    #[serde(default)]
    pub position: P3,
    #[serde(default)]
    pub rotation: P3,
    /// Fija: el solver no la mueve.
    #[serde(default)]
    pub fixed: bool,
}

/// Sistema de coordenadas sobre una instancia, en coordenadas de la pieza
/// (sin la posición de la instancia): origen, eje Z y eje X.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Connector {
    pub instance: u32,
    pub origin: P3,
    pub z: P3,
    pub x: P3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MateKind {
    /// Pegadas: 0 grados libres.
    Fastened,
    /// Bisagra: gira alrededor del eje Z común.
    Revolute,
    /// Desliza a lo largo del eje Z común, sin girar.
    Slider,
    /// Gira y desliza sobre el eje Z común.
    Cylindrical,
    /// Apoyadas en el plano XY: se mueven en él y giran alrededor de Z.
    Planar,
    /// Rótula: los orígenes coinciden y cada pieza gira libre (3 grados).
    Spherical,
    /// Los ejes Z quedan paralelos; las piezas se mueven y giran libres alrededor de él.
    Parallel,
    /// Pasador en ranura: el origen de B corre por la recta del eje X de A y B gira
    /// alrededor de su Z, que se mantiene paralelo al de A (2 grados).
    PinSlot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mate {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub kind: MateKind,
    pub a: Connector,
    pub b: Connector,
    /// Z de `b` al revés (caras enfrentadas).
    #[serde(default)]
    pub flip: bool,
    /// Ángulo impuesto (grados) en revolutas y cilíndricas: para animar o posar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// Distancia impuesta (mm) a lo largo de Z en deslizantes y cilíndricas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

/// Matriz de giro de un vector de giro (Rodrigues).
pub fn rotation_matrix(r: P3) -> [[f64; 3]; 3] {
    let th = norm(r);
    if th < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let [x, y, z] = scale(r, 1.0 / th);
    let (s, c) = th.sin_cos();
    let t = 1.0 - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}

fn mul(m: &[[f64; 3]; 3], v: P3) -> P3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Matriz 3×4 (giro y traslación) de una instancia.
pub fn instance_matrix(position: P3, rotation: P3) -> [[f64; 4]; 3] {
    let r = rotation_matrix(rotation);
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], position[i]])
}

/// Conector en el mundo: origen, Z y X.
fn world(c: &Connector, pos: P3, rot: P3) -> (P3, P3, P3) {
    let r = rotation_matrix(rot);
    (add(mul(&r, c.origin), pos), mul(&r, normalize(c.z)), mul(&r, normalize(c.x)))
}

/// Ecuaciones de una relación (cero cuando se cumple).
fn residuals(m: &Mate, a: (P3, P3, P3), b: (P3, P3, P3), out: &mut Vec<f64>) {
    let (oa, za, xa) = a;
    let (ob, zb, xb) = b;
    let zb = if m.flip { scale(zb, -1.0) } else { zb };
    let d = sub(ob, oa);
    // Componente de d perpendicular al eje común
    let perp = sub(d, scale(za, dot(d, za)));
    match m.kind {
        MateKind::Fastened => {
            out.extend_from_slice(&d);
            out.extend_from_slice(&sub(zb, za));
            out.extend_from_slice(&sub(xb, xa));
        }
        MateKind::Revolute | MateKind::Cylindrical => {
            if m.kind == MateKind::Revolute {
                out.extend_from_slice(&d);
            } else {
                out.extend_from_slice(&perp);
                if let Some(dist) = m.distance {
                    out.push(dot(d, za) - dist);
                }
            }
            out.extend_from_slice(&sub(zb, za));
            if let Some(angle) = m.angle {
                // X de b = X de a girado `angle` alrededor de Z
                let (s, c) = angle.to_radians().sin_cos();
                let want = add(scale(xa, c), scale(cross(za, xa), s));
                out.extend_from_slice(&sub(xb, want));
            }
        }
        MateKind::Slider => {
            out.extend_from_slice(&perp);
            out.extend_from_slice(&sub(zb, za));
            out.extend_from_slice(&sub(xb, xa));
            if let Some(dist) = m.distance {
                out.push(dot(d, za) - dist);
            }
        }
        MateKind::Planar => {
            out.push(dot(d, za));
            out.extend_from_slice(&sub(zb, za));
        }
        MateKind::Spherical => out.extend_from_slice(&d),
        MateKind::Parallel => out.extend_from_slice(&sub(zb, za)),
        MateKind::PinSlot => {
            // Fuera de la recta de la ranura (eje X de A por su origen)
            let off = sub(d, scale(xa, dot(d, xa)));
            out.extend_from_slice(&off);
            out.extend_from_slice(&sub(zb, za));
            if let Some(dist) = m.distance {
                out.push(dot(d, xa) - dist);
            }
            if let Some(angle) = m.angle {
                let (s, c) = angle.to_radians().sin_cos();
                let want = add(scale(xa, c), scale(cross(za, xa), s));
                out.extend_from_slice(&sub(xb, want));
            }
        }
    }
}

/// Ángulo (rad) de `b` respecto de `a` alrededor de Z de `a`, y distancia (mm)
/// a lo largo de ese Z, de una unión con los conectores en el mundo.
fn measure_mate(m: &Mate, a: (P3, P3, P3), b: (P3, P3, P3)) -> (f64, f64) {
    let (oa, za, xa) = a;
    let (ob, _, xb) = b;
    let _ = m;
    (dot(xb, cross(za, xa)).atan2(dot(xb, xa)), dot(sub(ob, oa), za))
}

/// Pasa a las uniones impuestas lo que las relaciones dicen: la unión que se
/// anima (ángulo o distancia dados) fija la otra. Quedan abiertas las relaciones
/// sin ninguna impuesta, que resuelve el solver.
fn propagate(asm: &Assembly) -> (Vec<Mate>, Vec<Relation>) {
    let mut mates = asm.mates.clone();
    let mut open = asm.relations.clone();
    loop {
        let before = open.len();
        open.retain(|r| !impose(r, &mut mates));
        if open.len() == before {
            break;
        }
    }
    (mates, open)
}

/// ¿La relación quedó resuelta imponiendo valores en las uniones?
fn impose(r: &Relation, mates: &mut [Mate]) -> bool {
    let idx = |id: u32| mates.iter().position(|m| m.id == id);
    match r {
        Relation::Gear { a, b, ratio, phase, carrier, carrier_ratio } => {
            let (Some(i), Some(j)) = (idx(*a), idx(*b)) else { return true };
            let k = match carrier {
                Some(c) => match idx(*c) {
                    Some(k) => Some(k),
                    None => return true,
                },
                None => None,
            };
            // t_b = razón·t_a + razón_brazo·t_c + fase: con una sola incógnita se despeja
            let cr = if k.is_some() { *carrier_ratio } else { 0.0 };
            let tc = k.map_or(Some(0.0), |k| mates[k].angle);
            match (mates[i].angle, mates[j].angle, tc) {
                (Some(x), None, Some(c)) => mates[j].angle = Some(ratio * x + cr * c + phase),
                (None, Some(y), Some(c)) if ratio.abs() > 1e-12 => mates[i].angle = Some((y - cr * c - phase) / ratio),
                (Some(x), Some(y), None) if cr.abs() > 1e-12 => mates[k.unwrap()].angle = Some((y - ratio * x - phase) / cr),
                (Some(_), Some(_), Some(_)) => {}
                _ => return false,
            }
            true
        }
        Relation::RackPinion { revolute, slider, per_turn, phase } => {
            let (Some(i), Some(j)) = (idx(*revolute), idx(*slider)) else { return true };
            match (mates[i].angle, mates[j].distance) {
                (Some(t), None) => mates[j].distance = Some(per_turn * t / 360.0 + phase),
                (None, Some(d)) if per_turn.abs() > 1e-12 => mates[i].angle = Some((d - phase) / per_turn * 360.0),
                (None, None) => return false,
                _ => {}
            }
            true
        }
        Relation::Screw { cylindrical, pitch, phase } => {
            let Some(i) = idx(*cylindrical) else { return true };
            match (mates[i].angle, mates[i].distance) {
                (Some(t), None) => mates[i].distance = Some(pitch * t / 360.0 + phase),
                (None, Some(d)) if pitch.abs() > 1e-12 => mates[i].angle = Some((d - phase) / pitch * 360.0),
                (None, None) => return false,
                _ => {}
            }
            true
        }
    }
}

/// Resultado del solver.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssemblySolution {
    /// Posición y giro de cada instancia (en el orden del ensamble).
    pub poses: Vec<(u32, P3, P3)>,
    /// Norma de los residuos al terminar (0 = todas las relaciones se cumplen).
    pub residual: f64,
    pub converged: bool,
    /// Grados de libertad que quedan (incógnitas menos el rango del sistema).
    pub dof: usize,
}

/// Mueve las instancias libres para cumplir las relaciones (Levenberg-Marquardt
/// con jacobiano numérico). Si ninguna está fija, la primera queda quieta.
pub fn solve(asm: &Assembly) -> AssemblySolution {
    let (mates, open) = propagate(asm);
    let anchor = if asm.instances.iter().any(|i| i.fixed) { None } else { asm.instances.first().map(|i| i.id) };
    let free: Vec<usize> = asm.instances.iter().enumerate().filter(|(_, i)| !i.fixed && Some(i.id) != anchor).map(|(k, _)| k).collect();
    let mut x = DVector::<f64>::zeros(free.len() * 6);
    for (j, &k) in free.iter().enumerate() {
        let i = &asm.instances[k];
        for c in 0..3 {
            x[j * 6 + c] = i.position[c];
            x[j * 6 + 3 + c] = i.rotation[c];
        }
    }
    let pose = |x: &DVector<f64>, id: u32| -> Option<(P3, P3)> {
        let k = asm.instances.iter().position(|i| i.id == id)?;
        match free.iter().position(|&f| f == k) {
            Some(j) => Some(([x[j * 6], x[j * 6 + 1], x[j * 6 + 2]], [x[j * 6 + 3], x[j * 6 + 4], x[j * 6 + 5]])),
            None => Some((asm.instances[k].position, asm.instances[k].rotation)),
        }
    };
    // `pin`: vuelta (múltiplo de 2π) que se resta a cada relación de engrane abierta. Sin ella el
    // residuo se envuelve a (-π, π] y salta justo en 180°: el jacobiano numérico ahí sale basura.
    let eval_pinned = |x: &DVector<f64>, pin: Option<&[f64]>, turns: &mut Vec<f64>| -> DVector<f64> {
        let mut r = Vec::new();
        for m in &mates {
            let (Some((pa, ra)), Some((pb, rb))) = (pose(x, m.a.instance), pose(x, m.b.instance)) else { continue };
            residuals(m, world(&m.a, pa, ra), world(&m.b, pb, rb), &mut r);
        }
        // Relaciones que ninguna unión impuesta resuelve: ecuaciones del solver
        let measure = |id: u32| -> Option<(f64, f64)> {
            let m = mates.iter().find(|m| m.id == id)?;
            let (pa, ra) = pose(x, m.a.instance)?;
            let (pb, rb) = pose(x, m.b.instance)?;
            Some(measure_mate(m, world(&m.a, pa, ra), world(&m.b, pb, rb)))
        };
        for (k, rel) in open.iter().enumerate() {
            match rel {
                Relation::Gear { a, b, ratio, phase, carrier, carrier_ratio } => {
                    let tc = match carrier {
                        Some(c) => measure(*c).map(|m| m.0),
                        None => Some(0.0),
                    };
                    if let (Some((ta, _)), Some((tb, _)), Some(tc)) = (measure(*a), measure(*b), tc) {
                        let e = tb - ratio * ta - carrier_ratio * tc - phase.to_radians();
                        let n = match pin {
                            Some(p) => p[k],
                            None => (e / TAU).round(),
                        };
                        turns.resize(open.len(), 0.0);
                        turns[k] = n;
                        r.push(e - TAU * n);
                    }
                }
                Relation::RackPinion { revolute, slider, per_turn, phase } => {
                    if let (Some((t, _)), Some((_, d))) = (measure(*revolute), measure(*slider)) {
                        r.push(d - per_turn * t / TAU - phase);
                    }
                }
                Relation::Screw { cylindrical, pitch, phase } => {
                    if let Some((t, d)) = measure(*cylindrical) {
                        r.push(d - pitch * t / TAU - phase);
                    }
                }
            }
        }
        DVector::from_vec(r)
    };
    let eval = |x: &DVector<f64>| eval_pinned(x, None, &mut Vec::new());
    let jac = |x: &DVector<f64>, f0: &DVector<f64>| -> DMatrix<f64> {
        let mut j = DMatrix::<f64>::zeros(f0.len(), x.len());
        let mut turns = vec![0.0; open.len()];
        eval_pinned(x, None, &mut turns);
        let eval = |x: &DVector<f64>| eval_pinned(x, Some(&turns), &mut Vec::new());
        for c in 0..x.len() {
            let h = 1e-7 * (1.0 + x[c].abs());
            let mut xp = x.clone();
            xp[c] += h;
            let mut xm = x.clone();
            xm[c] -= h;
            let col = (eval(&xp) - eval(&xm)) / (2.0 * h);
            j.set_column(c, &col);
        }
        j
    };
    let lm = |mut x: DVector<f64>| -> (DVector<f64>, DVector<f64>) {
        let mut f = eval(&x);
        let mut lambda = 1e-3;
        let mut converged = f.norm() < 1e-10;
        for _ in 0..200 {
            if converged || x.is_empty() {
                break;
            }
            let j = jac(&x, &f);
            let jt = j.transpose();
            let mut a = &jt * &j;
            let g = &jt * &f;
            let diag = a.diagonal();
            for k in 0..a.nrows() {
                a[(k, k)] += lambda * (1.0 + diag[k]);
            }
            let Some(step) = a.lu().solve(&(-g)) else { break };
            let xn = &x + &step;
            let fnew = eval(&xn);
            if fnew.norm() < f.norm() {
                x = xn;
                f = fnew;
                lambda = (lambda * 0.3).max(1e-12);
                converged = f.norm() < 1e-10 || step.norm() < 1e-14;
            } else {
                lambda *= 10.0;
                if lambda > 1e12 {
                    break;
                }
            }
        }
        (x, f)
    };
    let (mut x, mut f) = lm(x.clone());
    // Un ángulo pedido justo al otro lado (180°) deja el arranque en un punto muerto del que el
    // descenso no sale: se reintenta desde arranques corridos (siempre los mismos)
    let start = x.clone();
    let mut seed = 0x9e3779b97f4a7c15u64;
    for _ in 0..6 {
        if f.norm() < 1e-6 || start.is_empty() {
            break;
        }
        let mut kick = start.clone();
        for k in 0..free.len() {
            for c in 0..3 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                kick[k * 6 + 3 + c] += ((seed >> 40) as f64 / (1u64 << 24) as f64 - 0.5) * 1.2;
            }
        }
        let (xt, ft) = lm(kick);
        if ft.norm() < f.norm() {
            (x, f) = (xt, ft);
        }
    }
    // Grados libres: incógnitas menos el rango del jacobiano en la solución
    let dof = if x.is_empty() {
        0
    } else {
        let j = jac(&x, &f);
        let rank = if j.nrows() == 0 { 0 } else { j.svd(false, false).rank(1e-6) };
        x.len() - rank
    };
    let poses = asm
        .instances
        .iter()
        .map(|i| {
            let (p, r) = pose(&x, i.id).unwrap();
            (i.id, p, r)
        })
        .collect();
    AssemblySolution { poses, residual: f.norm(), converged: f.norm() < 1e-6, dof }
}

/// Qué se recorre de una unión al animarla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepVar {
    /// El ángulo impuesto (°): bisagras, cilíndricas y pasadores en ranura.
    Angle,
    /// La distancia impuesta (mm): deslizantes, cilíndricas y pasadores en ranura.
    Distance,
}

/// Recorre una unión de `from` a `to` en `frames` cuadros (mínimo 2) y resuelve el ensamble en
/// cada uno, partiendo de la pose del cuadro anterior para que el movimiento sea continuo.
pub fn sweep(asm: &Assembly, mate: u32, var: SweepVar, from: f64, to: f64, frames: usize) -> Result<Vec<AssemblySolution>, String> {
    let frames = frames.max(2);
    let i = asm.mates.iter().position(|m| m.id == mate).ok_or("la unión ya no existe")?;
    let ok = match (asm.mates[i].kind, var) {
        (MateKind::Revolute | MateKind::Cylindrical | MateKind::PinSlot, SweepVar::Angle) => true,
        (MateKind::Slider | MateKind::Cylindrical | MateKind::PinSlot, SweepVar::Distance) => true,
        _ => false,
    };
    if !ok {
        return Err("esa unión no se mueve así: usa el ángulo en bisagras y el desplazamiento en deslizantes".into());
    }
    let mut cur = asm.clone();
    let mut out = Vec::with_capacity(frames);
    for k in 0..frames {
        let v = from + (to - from) * k as f64 / (frames - 1) as f64;
        match var {
            SweepVar::Angle => cur.mates[i].angle = Some(v),
            SweepVar::Distance => cur.mates[i].distance = Some(v),
        }
        let sol = solve(&cur);
        if !sol.converged {
            return Err(format!("el ensamble no se puede resolver con la unión en {v:.1} (cuadro {})", k + 1));
        }
        for (id, p, r) in &sol.poses {
            if let Some(inst) = cur.instances.iter_mut().find(|x| x.id == *id) {
                inst.position = *p;
                inst.rotation = *r;
            }
        }
        out.push(sol);
    }
    Ok(out)
}

/// Cada instancia como forma en su lugar (las piezas que ya no existen se saltean).
pub fn instance_shapes(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution) -> Vec<(u32, Shape)> {
    sol.poses
        .iter()
        .filter_map(|(id, p, r)| {
            let inst = asm.instances.iter().find(|i| i.id == *id)?;
            let part = ev.parts.iter().find(|x| x.id == inst.part)?;
            part.shape.transform(instance_matrix(*p, *r)).ok().map(|s| (*id, s))
        })
        .collect()
}

/// Volumen en común entre cada par de instancias que chocan (mm³).
pub fn interferences(shapes: &[(u32, Shape)]) -> Vec<(u32, u32, f64)> {
    let mut out = Vec::new();
    let boxes: Vec<_> = shapes.iter().map(|(_, s)| s.mass().ok().map(|m| (m.bbox_min, m.bbox_max))).collect();
    for i in 0..shapes.len() {
        for j in i + 1..shapes.len() {
            let (Some((a0, a1)), Some((b0, b1))) = (boxes[i], boxes[j]) else { continue };
            if (0..3).any(|k| a0[k] > b1[k] || b0[k] > a1[k]) {
                continue;
            }
            if let Ok(common) = shapes[i].1.intersect(&shapes[j].1)
                && let Ok(m) = common.mass()
                && m.volume > 1e-9
            {
                out.push((shapes[i].0, shapes[j].0, m.volume));
            }
        }
    }
    out
}

/// Pares de instancias que no se pisan pero quedan a menos de `min_gap` mm (la holgura que pide la
/// impresora): con ese juego se funden o no se arman. También sale el contacto sin juego (0 mm).
/// Los engranes que engranan se saltan: ahí el juego lo da el backlash.
pub fn tight_gaps(ev: &Evaluation, asm: &Assembly, shapes: &[(u32, Shape)], min_gap: f64) -> Vec<(u32, u32, f64)> {
    let linked = |a: u32, b: u32| {
        let feat = |id: u32| asm.instances.iter().find(|i| i.id == id).map(|i| i.part.feature);
        let (Some(fa), Some(fb)) = (feat(a), feat(b)) else { return false };
        let l = |x, y| ev.gears.iter().any(|g| g.feature == x && g.part.link.is_some_and(|l| l.feature == y));
        l(fa, fb) || l(fb, fa)
    };
    let boxes: Vec<_> = shapes.iter().map(|(_, s)| s.mass().ok().map(|m| (m.bbox_min, m.bbox_max))).collect();
    let mut out = Vec::new();
    for i in 0..shapes.len() {
        for j in i + 1..shapes.len() {
            let (Some((a0, a1)), Some((b0, b1))) = (boxes[i], boxes[j]) else { continue };
            if (0..3).any(|k| a0[k] > b1[k] + min_gap || b0[k] > a1[k] + min_gap) || linked(shapes[i].0, shapes[j].0) {
                continue;
            }
            // Si se pisan, ya salen en los choques
            let overlap = shapes[i].1.intersect(&shapes[j].1).ok().and_then(|c| c.mass().ok()).is_some_and(|m| m.volume > 1e-9);
            if overlap {
                continue;
            }
            if let Some((d, _, _)) = shapes[i].1.min_distance(&shapes[j].1)
                && d < min_gap
            {
                out.push((shapes[i].0, shapes[j].0, d));
            }
        }
    }
    out
}

/// Medida actual de una unión en la solución: (ángulo en grados, distancia en mm)
pub fn measure(asm: &Assembly, sol: &AssemblySolution, mate: u32) -> Option<(f64, f64)> {
    let m = asm.mates.iter().find(|m| m.id == mate)?;
    let pose = |id: u32| sol.poses.iter().find(|p| p.0 == id).map(|p| (p.1, p.2));
    let (pa, ra) = pose(m.a.instance)?;
    let (pb, rb) = pose(m.b.instance)?;
    let (t, d) = measure_mate(m, world(&m.a, pa, ra), world(&m.b, pb, rb));
    Some((t.to_degrees(), d))
}

/// Eje del engrane de una instancia en el mundo, y el giro de la instancia
fn gear_world(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution, instance: u32) -> Option<(crate::gear_part::GearAxis, P3, [[f64; 3]; 3])> {
    let inst = asm.instances.iter().find(|i| i.id == instance)?;
    let g = ev.gears.iter().find(|g| g.feature == inst.part.feature)?;
    let rot = sol.poses.iter().find(|p| p.0 == instance)?.2;
    let r = rotation_matrix(rot);
    let axis = mul(&r, normalize(g.z));
    Some((g.clone(), axis, r))
}

/// Relaciones que piden los engranes enlazados (`GearLink`) que están en el
/// ensamble, con la razón real z₂/z₁ y la fase de como están ahora. Cada
/// engrane tiene que estar unido por una revoluta (o cilíndrica) a otra
/// pieza; las parejas que ya tienen relación no se repiten.
pub fn suggest_gear_relations(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution) -> Vec<Relation> {
    use crate::gears::GearKind::*;
    let mut out = Vec::new();
    // La unión giratoria de una instancia: de las que tenga, la de eje más paralelo al del engrane
    let pick = |inst: u32, kinds: &[MateKind], axis: Option<P3>| -> Option<(&Mate, f64)> {
        let score = |m: &Mate| {
            let w = world(&m.a, [0.0; 3], rot_of(sol, m.a.instance)).1;
            axis.map_or(1.0, |ax| dot(normalize(w), normalize(ax)).abs())
        };
        asm.mates
            .iter()
            .filter(|m| kinds.contains(&m.kind) && (m.a.instance == inst || m.b.instance == inst))
            .max_by(|x, y| score(x).total_cmp(&score(y)))
            .map(|m| (m, if m.b.instance == inst { 1.0 } else { -1.0 }))
    };
    let hinge = |inst: u32| -> Option<(&Mate, f64)> {
        let axis = gear_world(ev, asm, sol, inst).map(|g| g.1);
        pick(inst, &[MateKind::Revolute, MateKind::Cylindrical], axis)
    };
    let slide = |inst: u32| -> Option<(&Mate, f64)> { pick(inst, &[MateKind::Slider, MateKind::Cylindrical], None) };
    let exists = |a: u32, b: u32| {
        asm.relations.iter().any(|r| matches!(r, Relation::Gear { a: x, b: y, .. } | Relation::RackPinion { revolute: x, slider: y, .. } if (*x, *y) == (a, b) || (*x, *y) == (b, a)))
    };
    for b_inst in &asm.instances {
        let Some(gb) = ev.gears.iter().find(|g| g.feature == b_inst.part.feature) else { continue };
        let Some(link) = gb.part.link else { continue };
        for a_inst in asm.instances.iter().filter(|i| i.part.feature == link.feature) {
            let (Some((ga, axis_a, _)), Some((_, axis_b, _))) = (gear_world(ev, asm, sol, a_inst.id), gear_world(ev, asm, sol, b_inst.id)) else { continue };
            let (ka, kb) = (ga.part.spec.kind, gb.part.spec.kind);
            if ka == Rack {
                // La cremallera va primero: el piñón es el segundo
                if kb != External {
                    continue;
                }
                let (Some((mp, side_p)), Some((ms, side_s))) = (hinge(b_inst.id), slide(a_inst.id)) else { continue };
                if exists(mp.id, ms.id) {
                    continue;
                }
                let sign_p = side_p * dot(axis_b, normalize(world(&mp.a, [0.0; 3], rot_of(sol, mp.a.instance)).1)).signum();
                let e = mul(&rotation_matrix(rot_of(sol, a_inst.id)), normalize(ga.x));
                let zs = normalize(world(&ms.a, [0.0; 3], rot_of(sol, ms.a.instance)).1);
                let per_turn = TAU * gb.pitch_radius * sign_p * dot(e, zs) * side_s;
                let (Some((t, _)), Some((_, d))) = (measure(asm, sol, mp.id), measure(asm, sol, ms.id)) else { continue };
                out.push(Relation::RackPinion { revolute: mp.id, slider: ms.id, per_turn, phase: d - per_turn * t / 360.0 });
                continue;
            }
            // Corona fija (es la carcasa): el satélite que engrana con ella se acopla con el brazo
            // que lo lleva, por Willis con la corona quieta: φ_sat − φ_brazo = −(z_corona/z_sat)·φ_brazo
            let fixed_ring = asm.instances.iter().any(|i| i.id == b_inst.id && i.fixed);
            if fixed_ring && ka == External && kb == Internal {
                let Some((mp, side_p)) = hinge(a_inst.id) else { continue };
                let arm = if mp.a.instance == a_inst.id { mp.b.instance } else { mp.a.instance };
                let Some(mc) = asm.mates.iter().find(|m| {
                    matches!(m.kind, MateKind::Revolute | MateKind::Cylindrical) && ([m.a.instance, m.b.instance] == [b_inst.id, arm] || [m.b.instance, m.a.instance] == [b_inst.id, arm])
                }) else {
                    continue;
                };
                if exists(mc.id, mp.id) {
                    continue;
                }
                let side_c = if mc.b.instance == arm { 1.0 } else { -1.0 };
                let sign_p = side_p * dot(axis_a, normalize(world(&mp.a, [0.0; 3], rot_of(sol, mp.a.instance)).1)).signum();
                let sign_c = side_c * dot(axis_a, normalize(world(&mc.a, [0.0; 3], rot_of(sol, mc.a.instance)).1)).signum();
                let ratio = -sign_p * sign_c * gb.part.spec.teeth as f64 / ga.part.spec.teeth as f64;
                let (Some((tc, _)), Some((tp, _))) = (measure(asm, sol, mc.id), measure(asm, sol, mp.id)) else { continue };
                out.push(Relation::Gear { a: mc.id, b: mp.id, ratio, phase: tp - ratio * tc, carrier: None, carrier_ratio: 0.0 });
                continue;
            }
            let Some((ma, side_a)) = hinge(a_inst.id) else { continue };
            let sign_a = side_a * dot(axis_a, normalize(world(&ma.a, [0.0; 3], rot_of(sol, ma.a.instance)).1)).signum();
            let (za, zb) = (ga.part.spec.teeth as f64, gb.part.spec.teeth as f64);
            match (ka, kb) {
                (Rack, _) => continue,
                (_, Rack) => {
                    let Some((ms, side_s)) = slide(b_inst.id) else { continue };
                    if exists(ma.id, ms.id) {
                        continue;
                    }
                    // La cremallera se mueve a lo largo de su X (el de su marco)
                    let e = mul(&rotation_matrix(rot_of(sol, b_inst.id)), normalize(gb.x));
                    let zs = normalize(world(&ms.a, [0.0; 3], rot_of(sol, ms.a.instance)).1);
                    let per_turn = TAU * ga.pitch_radius * sign_a * dot(e, zs) * side_s;
                    let (Some((t, _)), Some((_, d))) = (measure(asm, sol, ma.id), measure(asm, sol, ms.id)) else { continue };
                    out.push(Relation::RackPinion { revolute: ma.id, slider: ms.id, per_turn, phase: d - per_turn * t / 360.0 });
                }
                _ => {
                    let Some((mb, side_b)) = hinge(b_inst.id) else { continue };
                    if ma.id == mb.id || exists(ma.id, mb.id) {
                        continue;
                    }
                    let sign_b = side_b * dot(axis_b, normalize(world(&mb.a, [0.0; 3], rot_of(sol, mb.a.instance)).1)).signum();
                    // φ_b = k·φ_a sobre los ejes de los engranes; la corona gira con el piñón
                    let outer = |k| matches!(k, External | Bevel);
                    // Sin fin con su rueda: gira -(entradas/dientes) con el signo de la mano
                    let k = if ka == Worm {
                        if ga.part.spec.left_hand { za / zb } else { -za / zb }
                    } else if kb == Worm {
                        // La rueda va primero: el sin fin gira (dientes/entradas) veces más
                        if gb.part.spec.left_hand { za / zb } else { -za / zb }
                    } else if outer(ka) && outer(kb) {
                        -za / zb
                    } else {
                        za / zb
                    };
                    let ratio = sign_a * sign_b * k;
                    let (Some((ta, _)), Some((tb, _))) = (measure(asm, sol, ma.id), measure(asm, sol, mb.id)) else { continue };
                    // Planetarios: si los dos cuelgan de piezas distintas, una es la carcasa y la otra el brazo
                    let parent = |m: &Mate, inst: u32| if m.a.instance == inst { m.b.instance } else { m.a.instance };
                    let (pa, pb) = (parent(ma, a_inst.id), parent(mb, b_inst.id));
                    let mut rel = Relation::Gear { a: ma.id, b: mb.id, ratio, phase: tb - ratio * ta, carrier: None, carrier_ratio: 0.0 };
                    if pa != pb {
                        let is_fixed = |i: u32| asm.instances.iter().find(|x| x.id == i).is_some_and(|x| x.fixed);
                        let first = |i: u32| asm.instances.first().is_some_and(|x| x.id == i);
                        let (house, arm) = if is_fixed(pa) || (!is_fixed(pb) && first(pa)) { (pa, pb) } else { (pb, pa) };
                        let arm_mate = asm.mates.iter().find(|m| {
                            matches!(m.kind, MateKind::Revolute | MateKind::Cylindrical) && ([m.a.instance, m.b.instance] == [house, arm] || [m.b.instance, m.a.instance] == [house, arm])
                        });
                        if let Some(mc) = arm_mate
                            && let Some((tc, _)) = measure(asm, sol, mc.id)
                        {
                            let side_c = if mc.b.instance == arm { 1.0 } else { -1.0 };
                            let sign_c = side_c * dot(axis_a, normalize(world(&mc.a, [0.0; 3], rot_of(sol, mc.a.instance)).1)).signum();
                            // Willis: (φ_b − φ_c) = k·(φ_a − φ_c), con φ absolutos sobre el eje
                            let (on_a, on_b) = (if pa == arm { 1.0 } else { 0.0 }, if pb == arm { 1.0 } else { 0.0 });
                            let carrier_ratio = sign_b * sign_c * (k * (on_a - 1.0) + 1.0 - on_b);
                            rel = Relation::Gear { a: ma.id, b: mb.id, ratio, phase: tb - ratio * ta - carrier_ratio * tc, carrier: Some(mc.id), carrier_ratio };
                        }
                    }
                    out.push(rel);
                }
            }
        }
    }
    out
}

/// Avisos de los pares de engranes enlazados que están en el ensamble: los rectos y helicoidales
/// solo engranan con ejes paralelos (cónicos y sin fin tienen los suyos, ya fijados por el enlace).
/// Si además los dos son helicoidales, es el caso de helicoidales cruzados, que no se tallan.
pub fn gear_axis_warnings(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution) -> Vec<String> {
    use crate::gears::GearKind::*;
    let mut out = Vec::new();
    for b_inst in &asm.instances {
        let Some(gb) = ev.gears.iter().find(|g| g.feature == b_inst.part.feature) else { continue };
        let Some(link) = gb.part.link else { continue };
        for a_inst in asm.instances.iter().filter(|i| i.part.feature == link.feature) {
            let (Some((ga, axis_a, _)), Some((_, axis_b, _))) = (gear_world(ev, asm, sol, a_inst.id), gear_world(ev, asm, sol, b_inst.id)) else { continue };
            let (ka, kb) = (ga.part.spec.kind, gb.part.spec.kind);
            if matches!(ka, Bevel | Worm) || matches!(kb, Bevel | Worm) {
                continue;
            }
            let angle = dot(normalize(axis_a), normalize(axis_b)).abs().clamp(0.0, 1.0).acos().to_degrees();
            if angle < 0.5 {
                continue;
            }
            let helical = ga.part.spec.helix_angle != 0.0 && gb.part.spec.helix_angle != 0.0;
            let tail = if helical { "; los helicoidales cruzados no se tallan, para ejes cruzados usa un sin fin" } else { "" };
            out.push(format!("{} y {} tienen los ejes a {:.1}°: los engranes rectos y helicoidales solo engranan con ejes paralelos{tail}", a_inst.name, b_inst.name, angle));
        }
    }
    out
}

fn rot_of(sol: &AssemblySolution, instance: u32) -> P3 {
    sol.poses.iter().find(|p| p.0 == instance).map(|p| p.2).unwrap_or([0.0; 3])
}

/// Choques entre instancias, sin contar los de engranes que engranan: a
/// los dientes en contacto les sobra un polvo de volumen, no es un choque.
pub fn interferences_for(ev: &Evaluation, asm: &Assembly, shapes: &[(u32, Shape)]) -> Vec<(u32, u32, f64)> {
    let tooth = |inst: u32| -> Option<f64> {
        let i = asm.instances.iter().find(|i| i.id == inst)?;
        let g = ev.gears.iter().find(|g| g.feature == i.part.feature)?;
        let vol = shapes.iter().find(|s| s.0 == inst)?.1.mass().ok()?.volume;
        Some(vol / g.part.spec.teeth.max(1) as f64)
    };
    let linked = |a: u32, b: u32| {
        let feat = |id: u32| asm.instances.iter().find(|i| i.id == id).map(|i| i.part.feature);
        let (Some(fa), Some(fb)) = (feat(a), feat(b)) else { return false };
        let l = |x, y| ev.gears.iter().any(|g| g.feature == x && g.part.link.is_some_and(|l| l.feature == y));
        l(fa, fb) || l(fb, fa)
    };
    interferences(shapes)
        .into_iter()
        .filter(|&(a, b, v)| !(linked(a, b) && tooth(a).zip(tooth(b)).is_some_and(|(x, y)| v < 0.005 * x.min(y))))
        .collect()
}
