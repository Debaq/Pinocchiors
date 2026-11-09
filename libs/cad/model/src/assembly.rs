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
    /// Grupos: piezas que se mueven juntas sin uniones entre ellas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<InstanceGroup>,
    /// Instancias repetidas en fila, en círculo o sobre agujeros: las copias siguen al original.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<InstancePattern>,
    /// Pasos de la animación dentro de la app (y del GLB animado).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motion: Vec<MotionStep>,
    /// Pasos de la vista explosionada.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explode: Vec<ExplodeStep>,
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
    /// Dos deslizantes: `distancia_b = razón · distancia_a + fase` (mm)
    Linear {
        a: u32,
        b: u32,
        ratio: f64,
        #[serde(default)]
        phase: f64,
    },
    /// Correa entre dos poleas (revolutas): giran para el mismo lado,
    /// `ángulo_b = razón · ángulo_a + fase`, con razón = radio_a / radio_b.
    Belt {
        a: u32,
        b: u32,
        ratio: f64,
        #[serde(default)]
        phase: f64,
    },
    /// Leva: el seguidor (deslizante) sube según el ángulo de la leva (revoluta).
    /// `lift`: pares (ángulo °, subida mm) de 0 a 360, interpolados en línea.
    Cam {
        revolute: u32,
        slider: u32,
        lift: Vec<[f64; 2]>,
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

fn is_false(x: &bool) -> bool {
    !*x
}

impl Relation {
    pub fn mates(&self) -> Vec<u32> {
        match self {
            Relation::Gear { a, b, carrier, .. } => [*a, *b].into_iter().chain(*carrier).collect(),
            Relation::RackPinion { revolute, slider, .. } | Relation::Cam { revolute, slider, .. } => vec![*revolute, *slider],
            Relation::Screw { cylindrical, .. } => vec![*cylindrical],
            Relation::Linear { a, b, .. } | Relation::Belt { a, b, .. } => vec![*a, *b],
        }
    }
}

/// Subida de una leva en un ángulo (°), por tramos rectos entre los puntos del perfil (que da la vuelta).
pub fn cam_lift(lift: &[[f64; 2]], angle: f64) -> f64 {
    if lift.is_empty() {
        return 0.0;
    }
    let mut pts: Vec<[f64; 2]> = lift.iter().map(|p| [p[0].rem_euclid(360.0), p[1]]).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let a = angle.rem_euclid(360.0);
    let n = pts.len();
    for k in 0..n {
        let p = pts[k];
        let q = if k + 1 < n { pts[k + 1] } else { [pts[0][0] + 360.0, pts[0][1]] };
        let (a0, a1) = (p[0], q[0]);
        let aa = if a < pts[0][0] { a + 360.0 } else { a };
        if aa >= a0 && aa <= a1 {
            let t = if a1 - a0 > 1e-12 { (aa - a0) / (a1 - a0) } else { 0.0 };
            return p[1] + (q[1] - p[1]) * t;
        }
    }
    pts[0][1]
}

/// Una pieza del diseño puesta en el ensamble.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: u32,
    /// Pestaña de diseño de la pieza (en un ensamble dentro del diseño, o de
    /// los proyectos de antes: 0, sin uso)
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub studio: crate::tabs::TabId,
    /// Variante de su diseño, por nombre (ninguna = la de base). Solo en una pestaña de ensamble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<String>,
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
    /// Oculta: se resuelve pero no se dibuja.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Suprimida: como si no estuviera (ni se resuelve, ni sus uniones, ni se exporta).
    #[serde(default, skip_serializing_if = "is_false")]
    pub suppressed: bool,
    /// Color propio en el ensamble ("#rrggbb"); sin él, el de su pieza.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Espejo: la pieza reflejada en el plano por su origen con esta normal (en coordenadas de la pieza).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<P3>,
    /// Subensamble flexible: sus uniones entran al solver de este ensamble (sin esto, rígido).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flexible: bool,
}

impl Instance {
    /// Lleva un punto de la pieza a la pieza reflejada (si es espejo).
    pub fn mirror_point(&self, p: P3) -> P3 {
        match self.mirror {
            Some(n) if norm(n) > 1e-12 => {
                let n = normalize(n);
                sub(p, scale(n, 2.0 * dot(p, n)))
            }
            _ => p,
        }
    }
}

/// Piezas que se mueven juntas sin uniones: la primera manda y las demás guardan su lugar respecto de ella.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceGroup {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub members: Vec<u32>,
}

/// Copias de una instancia que siguen al original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstancePattern {
    pub id: u32,
    pub source: u32,
    pub kind: PatternKind,
    /// Instancias copia, en orden (la primera es la 2.ª de la fila).
    pub copies: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternKind {
    /// En fila a lo largo de `dir` (en coordenadas de la pieza original), cada `step` mm.
    Linear { dir: P3, step: f64 },
    /// En círculo alrededor del eje (origen y dirección en coordenadas de la pieza original), cada `angle` °.
    Circular { origin: P3, axis: P3, angle: f64 },
    /// Sobre puntos de otra instancia (centros de agujeros): corrimientos en sus coordenadas desde
    /// el punto donde está el original.
    Points { reference: u32, offsets: Vec<P3> },
}

/// Un paso de la animación: una unión recorre de `from` a `to` en `seconds`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionStep {
    pub mate: u32,
    pub var: SweepVar,
    pub from: f64,
    pub to: f64,
    #[serde(default = "one")]
    pub seconds: f64,
}

fn one() -> f64 {
    1.0
}

/// Un paso de la vista explosionada: esas instancias se corren `distance` mm en `dir` (del mundo).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExplodeStep {
    pub instances: Vec<u32>,
    pub dir: P3,
    pub distance: f64,
}

/// Corrimiento de cada instancia con los pasos de la explosión hasta `upto` (0 = armado; 1.5 = el
/// primer paso entero y la mitad del segundo).
pub fn explode_offsets(asm: &Assembly, upto: f64) -> Vec<(u32, P3)> {
    let mut out: Vec<(u32, P3)> = Vec::new();
    for (k, step) in asm.explode.iter().enumerate() {
        let t = (upto - k as f64).clamp(0.0, 1.0);
        if t <= 0.0 {
            break;
        }
        let v = scale(normalize(step.dir), step.distance * t);
        for &id in &step.instances {
            match out.iter_mut().find(|o| o.0 == id) {
                Some(o) => o.1 = add(o.1, v),
                None => out.push((id, v)),
            }
        }
    }
    out
}

/// De dónde sale un conector: así sigue a la pieza cuando cambia su diseño.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectorRef {
    /// Cara plana (su centro y su normal) o cilíndrica/cónica (su eje, a la altura del centro).
    Face { face: crate::feature::FaceRef },
    /// Centro de un borde redondo, con Z por su eje.
    EdgeCenter { edge: crate::feature::EdgeRef },
    /// Punto medio de una arista, con Z por la normal de una cara vecina y X a lo largo de la arista.
    EdgeMid { edge: crate::feature::EdgeRef },
    /// Un vértice (el más cercano a `point`), con Z guardado.
    Vertex { point: P3, #[serde(default)] normal: P3 },
}

/// Sistema de coordenadas sobre una instancia, en coordenadas de la pieza
/// (sin la posición de la instancia): origen, eje Z y eje X. Con `at`, se recalcula
/// desde la pieza en cada recálculo (lo guardado queda de respaldo).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Connector {
    pub instance: u32,
    pub origin: P3,
    pub z: P3,
    pub x: P3,
    /// Radio de la cara cilíndrica o del borde redondo (0 si es plano): para la unión tangente.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub radius: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<ConnectorRef>,
    /// Corrimiento a lo largo de Z (mm).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// Giro de X alrededor de Z (°).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub spin: f64,
    /// Volteado: Z al revés (gira 180° alrededor de X).
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
}

impl Connector {
    /// Aplica corrimiento, giro y volteo a un sistema de base.
    pub fn adjusted(&self, origin: P3, z: P3, x: P3) -> (P3, P3, P3) {
        let z = normalize(z);
        let x = normalize(sub(x, scale(z, dot(x, z))));
        let origin = add(origin, scale(z, self.offset));
        let (s, c) = self.spin.to_radians().sin_cos();
        let x = add(scale(x, c), scale(cross(z, x), s));
        if self.reverse { (origin, scale(z, -1.0), x) } else { (origin, z, x) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MateKind {
    /// Pegadas: 0 grados libres.
    #[default]
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
    /// Tangente: cilindro contra plano o cilindro contra cilindro (los radios de los conectores).
    Tangent,
    /// Centrada: el origen de B queda a mitad de camino entre la cara de A y la cara `c`.
    Width,
    /// Ángulo entre los ejes Z (el `angle` de la unión).
    Angle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mate {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub kind: MateKind,
    pub a: Connector,
    pub b: Connector,
    /// Segunda cara de A (unión centrada).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<Connector>,
    /// Z de `b` al revés (caras enfrentadas).
    #[serde(default)]
    pub flip: bool,
    /// Ángulo impuesto (grados) en revolutas y cilíndricas: para animar o posar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// Distancia impuesta (mm) a lo largo de Z en deslizantes y cilíndricas; separación en planas y pegadas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    /// Topes del ángulo (°) en bisagras, cilíndricas y pasadores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_limits: Option<[f64; 2]>,
    /// Topes de la distancia (mm) en deslizantes, cilíndricas y pasadores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_limits: Option<[f64; 2]>,
    /// Suprimida: no cuenta.
    #[serde(default, skip_serializing_if = "is_false")]
    pub suppressed: bool,
}

impl Mate {
    pub fn connectors(&self) -> impl Iterator<Item = &Connector> {
        [&self.a, &self.b].into_iter().chain(self.c.as_ref())
    }
    pub fn connectors_mut(&mut self) -> impl Iterator<Item = &mut Connector> {
        [&mut self.a, &mut self.b].into_iter().chain(self.c.as_mut())
    }
    /// El ángulo dentro de sus topes.
    fn clamp_angle(&self, v: f64) -> f64 {
        self.angle_limits.map_or(v, |[lo, hi]| v.clamp(lo.min(hi), hi.max(lo)))
    }
    fn clamp_distance(&self, v: f64) -> f64 {
        self.distance_limits.map_or(v, |[lo, hi]| v.clamp(lo.min(hi), hi.max(lo)))
    }
}

impl Assembly {
    /// Quita lo que apunta a cosas que ya no están: uniones de instancias borradas,
    /// acoplamientos de uniones borradas, miembros de grupos, patrones sin original
    /// (sus copias quedan sueltas) y pasos de animación o explosión colgando.
    pub fn prune(&mut self) {
        let ids: Vec<u32> = self.instances.iter().map(|i| i.id).collect();
        self.mates.retain(|m| m.connectors().all(|c| ids.contains(&c.instance)));
        let mates: Vec<u32> = self.mates.iter().map(|m| m.id).collect();
        self.relations.retain(|r| r.mates().iter().all(|m| mates.contains(m)));
        for g in &mut self.groups {
            g.members.retain(|m| ids.contains(m));
        }
        self.groups.retain(|g| g.members.len() > 1);
        self.patterns.retain(|p| ids.contains(&p.source) && !matches!(&p.kind, PatternKind::Points { reference, .. } if !ids.contains(reference)));
        for p in &mut self.patterns {
            if let PatternKind::Points { offsets, .. } = &mut p.kind {
                // Copia borrada: su punto también
                let keep: Vec<bool> = p.copies.iter().map(|c| ids.contains(c)).collect();
                let mut k = 0;
                offsets.retain(|_| {
                    let r = keep.get(k).copied().unwrap_or(false);
                    k += 1;
                    r
                });
            }
            p.copies.retain(|c| ids.contains(c));
        }
        self.patterns.retain(|p| !p.copies.is_empty());
        self.motion.retain(|s| mates.contains(&s.mate));
        for s in &mut self.explode {
            s.instances.retain(|i| ids.contains(i));
        }
        self.explode.retain(|s| !s.instances.is_empty());
    }
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

/// Vector de giro (eje × ángulo) de una matriz de giro: la inversa de [`rotation_matrix`].
pub fn rotation_vector(m: &[[f64; 3]; 3]) -> P3 {
    let tr = m[0][0] + m[1][1] + m[2][2];
    let c = ((tr - 1.0) / 2.0).clamp(-1.0, 1.0);
    let th = c.acos();
    let w = [m[2][1] - m[1][2], m[0][2] - m[2][0], m[1][0] - m[0][1]];
    if th < 1e-9 {
        return scale(w, 0.5);
    }
    if std::f64::consts::PI - th < 1e-6 {
        // Media vuelta: el eje es la columna más larga de (R + I)
        let cols = [0, 1, 2].map(|j| [m[0][j] + if j == 0 { 1.0 } else { 0.0 }, m[1][j] + if j == 1 { 1.0 } else { 0.0 }, m[2][j] + if j == 2 { 1.0 } else { 0.0 }]);
        let best = cols.into_iter().max_by(|a, b| norm(*a).total_cmp(&norm(*b))).unwrap();
        return scale(normalize(best), th);
    }
    scale(w, th / (2.0 * th.sin()))
}

fn mul(m: &[[f64; 3]; 3], v: P3) -> P3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn matmul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j]))
}

fn transpose(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|i| [a[0][i], a[1][i], a[2][i]])
}

/// Matriz 3×4 (giro y traslación) de una instancia.
pub fn instance_matrix(position: P3, rotation: P3) -> [[f64; 4]; 3] {
    let r = rotation_matrix(rotation);
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], position[i]])
}

/// Pose rígida: giro y traslación.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub r: [[f64; 3]; 3],
    pub t: P3,
}

impl Pose {
    pub const IDENTITY: Pose = Pose { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };
    pub fn new(position: P3, rotation: P3) -> Pose {
        Pose { r: rotation_matrix(rotation), t: position }
    }
    pub fn compose(&self, o: &Pose) -> Pose {
        Pose { r: matmul(&self.r, &o.r), t: add(mul(&self.r, o.t), self.t) }
    }
    pub fn inverse(&self) -> Pose {
        let rt = transpose(&self.r);
        Pose { r: rt, t: scale(mul(&rt, self.t), -1.0) }
    }
    pub fn apply(&self, p: P3) -> P3 {
        add(mul(&self.r, p), self.t)
    }
    pub fn turn(&self, v: P3) -> P3 {
        mul(&self.r, v)
    }
    pub fn rotation_vector(&self) -> P3 {
        rotation_vector(&self.r)
    }
    pub fn translation(v: P3) -> Pose {
        Pose { t: v, ..Pose::IDENTITY }
    }
    /// Giro de `angle` rad alrededor del eje por `origin` con dirección `axis`.
    pub fn about(origin: P3, axis: P3, angle: f64) -> Pose {
        let r = rotation_matrix(scale(normalize(axis), angle));
        Pose { r, t: sub(origin, mul(&r, origin)) }
    }
}

type Frame = (P3, P3, P3);

/// Conector en el mundo con la pose de su instancia: origen, Z y X.
fn world_pose(c: &Connector, pose: &Pose) -> Frame {
    (pose.apply(c.origin), pose.turn(normalize(c.z)), pose.turn(normalize(c.x)))
}

/// Conector en el mundo: origen, Z y X.
fn world(c: &Connector, pos: P3, rot: P3) -> Frame {
    world_pose(c, &Pose::new(pos, rot))
}

fn wrap_pi(a: f64) -> f64 {
    let r = (a + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI;
    if r <= -std::f64::consts::PI { r + TAU } else { r }
}

/// Ángulo (rad) de `b` respecto de `a` alrededor de Z de `a`, y distancia (mm)
/// a lo largo de ese Z (de X de `a` en un pasador en ranura).
fn measure_mate(m: &Mate, a: Frame, b: Frame) -> (f64, f64) {
    let (oa, za, xa) = a;
    let (ob, _, xb) = b;
    let along = if m.kind == MateKind::PinSlot { xa } else { za };
    (dot(xb, cross(za, xa)).atan2(dot(xb, xa)), dot(sub(ob, oa), along))
}

/// Filas de la unión: cuántas ecuaciones da y cuáles son ángulos (su diferencia se envuelve).
fn mate_rows(m: &Mate) -> Vec<bool> {
    let n = match m.kind {
        MateKind::Fastened => 9,
        MateKind::Revolute => 6 + if m.angle.is_some() { 3 } else { 0 },
        MateKind::Cylindrical => 6 + m.distance.map_or(0, |_| 1) + if m.angle.is_some() { 3 } else { 0 },
        MateKind::Slider => 9 + m.distance.map_or(0, |_| 1),
        MateKind::Planar => 4 + if m.angle.is_some() { 3 } else { 0 },
        MateKind::Spherical | MateKind::Parallel => 3,
        MateKind::PinSlot => 6 + m.distance.map_or(0, |_| 1) + if m.angle.is_some() { 3 } else { 0 },
        MateKind::Tangent => match (m.a.radius > 0.0, m.b.radius > 0.0) {
            (true, true) => 4,
            (false, false) => 4,
            _ => 2,
        },
        MateKind::Width => usize::from(m.c.is_some()),
        MateKind::Angle => 1,
    };
    let mut rows = vec![false; n];
    // Topes: una fila que solo pesa al pasarse
    if m.angle.is_none() && m.angle_limits.is_some() && matches!(m.kind, MateKind::Revolute | MateKind::Cylindrical | MateKind::PinSlot) {
        rows.push(true);
    }
    if m.distance.is_none() && m.distance_limits.is_some() && matches!(m.kind, MateKind::Slider | MateKind::Cylindrical | MateKind::PinSlot) {
        rows.push(false);
    }
    rows
}

/// Ecuaciones de una relación (cero cuando se cumple), con sus topes al final.
fn residuals(m: &Mate, a: Frame, b: Frame, c: Option<Frame>, out: &mut Vec<f64>) {
    let (oa, za, xa) = a;
    let (ob, zb_raw, xb) = b;
    let zb = if m.flip { scale(zb_raw, -1.0) } else { zb_raw };
    let d = sub(ob, oa);
    // Componente de d perpendicular al eje común
    let perp = sub(d, scale(za, dot(d, za)));
    let turned = |angle: f64| {
        let (s, c) = angle.to_radians().sin_cos();
        add(scale(xa, c), scale(cross(za, xa), s))
    };
    match m.kind {
        MateKind::Fastened => {
            out.extend_from_slice(&sub(d, scale(za, m.distance.unwrap_or(0.0))));
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
                out.extend_from_slice(&sub(xb, turned(angle)));
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
            out.push(dot(d, za) - m.distance.unwrap_or(0.0));
            out.extend_from_slice(&sub(zb, za));
            if let Some(angle) = m.angle {
                out.extend_from_slice(&sub(xb, turned(angle)));
            }
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
                out.extend_from_slice(&sub(xb, turned(angle)));
            }
        }
        MateKind::Tangent => {
            let side = if m.flip { -1.0 } else { 1.0 };
            match (m.a.radius > 0.0, m.b.radius > 0.0) {
                // Cilindro contra cilindro: ejes paralelos a la suma de los radios (o la resta, por dentro)
                (true, true) => {
                    out.extend_from_slice(&cross(za, zb_raw));
                    let want = if m.flip { (m.a.radius - m.b.radius).abs() } else { m.a.radius + m.b.radius };
                    out.push(norm(perp) - want);
                }
                // Plano (A) y cilindro (B): el eje del cilindro paralelo al plano, a un radio
                (false, true) => {
                    out.push(dot(zb_raw, za));
                    out.push(dot(d, za) - side * m.b.radius);
                }
                (true, false) => {
                    out.push(dot(za, zb_raw));
                    out.push(dot(scale(d, -1.0), zb_raw) - side * m.a.radius);
                }
                // Dos planos: apoyados (como una plana)
                (false, false) => {
                    out.push(dot(d, za));
                    out.extend_from_slice(&sub(zb, za));
                }
            }
        }
        MateKind::Width => {
            if let Some((oc, _, _)) = c {
                let mid = scale(add(oa, oc), 0.5);
                out.push(dot(sub(ob, mid), za));
            }
        }
        MateKind::Angle => out.push(dot(za, zb) - m.angle.unwrap_or(90.0).to_radians().cos()),
    }
    // Topes
    if m.angle.is_none() && matches!(m.kind, MateKind::Revolute | MateKind::Cylindrical | MateKind::PinSlot)
        && let Some([lo, hi]) = m.angle_limits
    {
        let (t, _) = measure_mate(m, a, b);
        let v = t.to_degrees();
        let (lo, hi) = (lo.min(hi), hi.max(lo));
        out.push((v - v.clamp(lo, hi)).to_radians());
    }
    if m.distance.is_none() && matches!(m.kind, MateKind::Slider | MateKind::Cylindrical | MateKind::PinSlot)
        && let Some([lo, hi]) = m.distance_limits
    {
        let (_, v) = measure_mate(m, a, b);
        let (lo, hi) = (lo.min(hi), hi.max(lo));
        out.push(v - v.clamp(lo, hi));
    }
}

/// Pasa a las uniones impuestas lo que las relaciones dicen: la unión que se
/// anima (ángulo o distancia dados) fija la otra. Quedan abiertas las relaciones
/// sin ninguna impuesta, que resuelve el solver.
fn propagate(mates: Vec<Mate>, relations: &[Relation]) -> (Vec<Mate>, Vec<Relation>) {
    let mut mates = mates;
    let mut open = relations.to_vec();
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
        Relation::Belt { a, b, ratio, phase } => {
            let (Some(i), Some(j)) = (idx(*a), idx(*b)) else { return true };
            match (mates[i].angle, mates[j].angle) {
                (Some(x), None) => mates[j].angle = Some(ratio * x + phase),
                (None, Some(y)) if ratio.abs() > 1e-12 => mates[i].angle = Some((y - phase) / ratio),
                (None, None) => return false,
                _ => {}
            }
            true
        }
        Relation::Linear { a, b, ratio, phase } => {
            let (Some(i), Some(j)) = (idx(*a), idx(*b)) else { return true };
            match (mates[i].distance, mates[j].distance) {
                (Some(x), None) => mates[j].distance = Some(ratio * x + phase),
                (None, Some(y)) if ratio.abs() > 1e-12 => mates[i].distance = Some((y - phase) / ratio),
                (None, None) => return false,
                _ => {}
            }
            true
        }
        Relation::Cam { revolute, slider, lift, phase } => {
            let (Some(i), Some(j)) = (idx(*revolute), idx(*slider)) else { return true };
            match (mates[i].angle, mates[j].distance) {
                (Some(t), None) => mates[j].distance = Some(cam_lift(lift, t) + phase),
                (None, _) => return false,
                _ => {}
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
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AssemblySolution {
    /// Posición y giro de cada instancia (en el orden del ensamble).
    pub poses: Vec<(u32, P3, P3)>,
    /// Norma de los residuos al terminar (0 = todas las relaciones se cumplen).
    pub residual: f64,
    pub converged: bool,
    /// Grados de libertad que quedan (incógnitas menos el rango del sistema).
    pub dof: usize,
    /// Uniones que no se cumplen: (unión, desvío).
    pub mate_errors: Vec<(u32, f64)>,
    /// Uniones que no agregan nada (lo que piden ya lo piden las otras).
    pub redundant: Vec<u32>,
    /// Instancias que todavía se mueven: (instancia, grados libres).
    pub instance_dof: Vec<(u32, usize)>,
}

/// Cómo sale la pose de una instancia que no es incógnita.
#[derive(Debug, Clone)]
enum Derive {
    /// La de otra compuesta con un corrimiento rígido (grupos, filas, círculos).
    Rel { leader: u32, rel: Pose },
    /// Copia sobre un agujero: ref ∘ T(corrimiento) ∘ ref⁻¹ ∘ original.
    Holes { source: u32, reference: u32, offset: P3 },
}

/// Bloque de ecuaciones: una unión o un acoplamiento abierto.
struct Block {
    kind: BlockKind,
    rows: Vec<bool>,
    /// Instancias incógnita de las que depende
    deps: Vec<u32>,
}

enum BlockKind {
    Mate(usize),
    Relation(usize),
}

struct Solver {
    stored: std::collections::HashMap<u32, Pose>,
    derived: std::collections::HashMap<u32, Derive>,
    /// Instancia incógnita → primera columna en `x`
    slot: std::collections::HashMap<u32, usize>,
    mates: Vec<Mate>,
    open: Vec<Relation>,
    blocks: Vec<Block>,
}

impl Solver {
    fn pose(&self, x: &[f64], id: u32, depth: usize) -> Option<Pose> {
        if let Some(&j) = self.slot.get(&id) {
            return Some(Pose::new([x[j], x[j + 1], x[j + 2]], [x[j + 3], x[j + 4], x[j + 5]]));
        }
        if depth < 16
            && let Some(d) = self.derived.get(&id)
        {
            return match d {
                Derive::Rel { leader, rel } => Some(self.pose(x, *leader, depth + 1)?.compose(rel)),
                Derive::Holes { source, reference, offset } => {
                    let r = self.pose(x, *reference, depth + 1)?;
                    let s = self.pose(x, *source, depth + 1)?;
                    Some(r.compose(&Pose::translation(*offset)).compose(&r.inverse()).compose(&s))
                }
            };
        }
        self.stored.get(&id).copied()
    }

    fn deps(&self, id: u32, out: &mut Vec<u32>, depth: usize) {
        if self.slot.contains_key(&id) {
            if !out.contains(&id) {
                out.push(id);
            }
            return;
        }
        if depth < 16
            && let Some(d) = self.derived.get(&id)
        {
            match d {
                Derive::Rel { leader, .. } => self.deps(*leader, out, depth + 1),
                Derive::Holes { source, reference, .. } => {
                    self.deps(*source, out, depth + 1);
                    self.deps(*reference, out, depth + 1);
                }
            }
        }
    }

    fn frame(&self, x: &[f64], c: &Connector) -> Option<Frame> {
        Some(world_pose(c, &self.pose(x, c.instance, 0)?))
    }

    fn measure(&self, x: &[f64], mate: u32) -> Option<(f64, f64)> {
        let m = self.mates.iter().find(|m| m.id == mate)?;
        Some(measure_mate(m, self.frame(x, &m.a)?, self.frame(x, &m.b)?))
    }

    /// Filas del bloque; si falta algo, ceros.
    fn eval_block(&self, b: &Block, x: &[f64], out: &mut Vec<f64>) {
        let start = out.len();
        match b.kind {
            BlockKind::Mate(k) => {
                let m = &self.mates[k];
                if let (Some(fa), Some(fb)) = (self.frame(x, &m.a), self.frame(x, &m.b)) {
                    let fc = m.c.as_ref().and_then(|c| self.frame(x, c));
                    residuals(m, fa, fb, fc, out);
                }
            }
            BlockKind::Relation(k) => match &self.open[k] {
                Relation::Gear { a, b, ratio, phase, carrier, carrier_ratio } => {
                    let tc = match carrier {
                        Some(c) => self.measure(x, *c).map(|m| m.0),
                        None => Some(0.0),
                    };
                    if let (Some((ta, _)), Some((tb, _)), Some(tc)) = (self.measure(x, *a), self.measure(x, *b), tc) {
                        out.push(wrap_pi(tb - ratio * ta - carrier_ratio * tc - phase.to_radians()));
                    }
                }
                Relation::Belt { a, b, ratio, phase } => {
                    if let (Some((ta, _)), Some((tb, _))) = (self.measure(x, *a), self.measure(x, *b)) {
                        out.push(wrap_pi(tb - ratio * ta - phase.to_radians()));
                    }
                }
                Relation::Linear { a, b, ratio, phase } => {
                    if let (Some((_, da)), Some((_, db))) = (self.measure(x, *a), self.measure(x, *b)) {
                        out.push(db - ratio * da - phase);
                    }
                }
                Relation::Cam { revolute, slider, lift, phase } => {
                    if let (Some((t, _)), Some((_, d))) = (self.measure(x, *revolute), self.measure(x, *slider)) {
                        out.push(d - cam_lift(lift, t.to_degrees()) - phase);
                    }
                }
                Relation::RackPinion { revolute, slider, per_turn, phase } => {
                    if let (Some((t, _)), Some((_, d))) = (self.measure(x, *revolute), self.measure(x, *slider)) {
                        out.push(d - per_turn * t / TAU - phase);
                    }
                }
                Relation::Screw { cylindrical, pitch, phase } => {
                    if let Some((t, d)) = self.measure(x, *cylindrical) {
                        out.push(d - pitch * t / TAU - phase);
                    }
                }
            },
        }
        out.resize(start + b.rows.len(), 0.0);
    }

    fn eval(&self, blocks: &[usize], x: &[f64]) -> DVector<f64> {
        let mut r = Vec::new();
        for &b in blocks {
            self.eval_block(&self.blocks[b], x, &mut r);
        }
        DVector::from_vec(r)
    }

    /// Jacobiano por bloques: cada columna solo vuelve a evaluar las ecuaciones que tocan a su instancia.
    fn jacobian(&self, blocks: &[usize], vars: &[usize], x: &[f64]) -> DMatrix<f64> {
        let rows: usize = blocks.iter().map(|&b| self.blocks[b].rows.len()).sum();
        let mut j = DMatrix::<f64>::zeros(rows, vars.len());
        let col_of: std::collections::HashMap<usize, usize> = vars.iter().enumerate().map(|(c, &v)| (v, c)).collect();
        let mut xp = x.to_vec();
        let mut row = 0;
        let (mut fp, mut fm) = (Vec::new(), Vec::new());
        for &bi in blocks {
            let b = &self.blocks[bi];
            for inst in &b.deps {
                let s = self.slot[inst];
                for v in s..s + 6 {
                    let Some(&c) = col_of.get(&v) else { continue };
                    let h = 1e-7 * (1.0 + x[v].abs());
                    xp[v] = x[v] + h;
                    fp.clear();
                    self.eval_block(b, &xp, &mut fp);
                    xp[v] = x[v] - h;
                    fm.clear();
                    self.eval_block(b, &xp, &mut fm);
                    xp[v] = x[v];
                    for (k, angular) in b.rows.iter().enumerate() {
                        let diff = fp[k] - fm[k];
                        let diff = if *angular { wrap_pi(diff) } else { diff };
                        j[(row + k, c)] = diff / (2.0 * h);
                    }
                }
            }
            row += b.rows.len();
        }
        j
    }

    /// Levenberg-Marquardt sobre las columnas `vars`; devuelve la norma final.
    fn lm(&self, blocks: &[usize], vars: &[usize], x: &mut [f64]) -> f64 {
        let mut f = self.eval(blocks, x);
        let mut lambda = 1e-3;
        let mut converged = f.norm() < 1e-10;
        for _ in 0..200 {
            if converged || vars.is_empty() {
                break;
            }
            let j = self.jacobian(blocks, vars, x);
            let jt = j.transpose();
            let mut a = &jt * &j;
            let g = &jt * &f;
            let diag = a.diagonal();
            for k in 0..a.nrows() {
                a[(k, k)] += lambda * (1.0 + diag[k]);
            }
            let Some(step) = a.lu().solve(&(-g)) else { break };
            let mut xn = x.to_vec();
            for (c, &v) in vars.iter().enumerate() {
                xn[v] += step[c];
            }
            let fnew = self.eval(blocks, &xn);
            if fnew.norm() < f.norm() {
                x.copy_from_slice(&xn);
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
        f.norm()
    }
}

fn rank(j: &DMatrix<f64>) -> usize {
    if j.nrows() == 0 || j.ncols() == 0 {
        return 0;
    }
    j.clone().svd(false, false).rank(1e-6)
}

/// Uniones listas para el solver: sin las suprimidas ni las de instancias que no están, con los
/// conectores de las instancias en espejo reflejados y los valores impuestos dentro de sus topes.
fn active_mates(asm: &Assembly, active: &[u32]) -> Vec<Mate> {
    asm.mates
        .iter()
        .filter(|m| !m.suppressed && m.connectors().all(|c| active.contains(&c.instance)))
        .map(|m| {
            let mut m = m.clone();
            for c in m.connectors_mut() {
                if let Some(inst) = asm.instances.iter().find(|i| i.id == c.instance)
                    && inst.mirror.is_some()
                {
                    c.origin = inst.mirror_point(c.origin);
                    let o = inst.mirror_point([0.0; 3]);
                    c.z = sub(inst.mirror_point(c.z), o);
                    c.x = sub(inst.mirror_point(c.x), o);
                }
            }
            m.angle = m.angle.map(|v| m.clamp_angle(v));
            m.distance = m.distance.map(|v| if matches!(m.kind, MateKind::Slider | MateKind::Cylindrical | MateKind::PinSlot) { m.clamp_distance(v) } else { v });
            m
        })
        .collect()
}

/// Mueve las instancias libres para cumplir las relaciones (Levenberg-Marquardt
/// con jacobiano numérico por bloques), cada grupo de piezas conectadas por su
/// lado. Si ninguna está fija, la primera queda quieta.
pub fn solve(asm: &Assembly) -> AssemblySolution {
    use std::collections::HashMap;
    let active: Vec<u32> = asm.instances.iter().filter(|i| !i.suppressed).map(|i| i.id).collect();
    let (mates, open) = propagate(active_mates(asm, &active), &asm.relations);
    let open: Vec<Relation> = open.into_iter().filter(|r| r.mates().iter().all(|id| mates.iter().any(|m| m.id == *id))).collect();
    let stored: HashMap<u32, Pose> = asm.instances.iter().map(|i| (i.id, Pose::new(i.position, i.rotation))).collect();
    // Copias de patrones y miembros de grupos: su pose sale de otra
    let mut derived: HashMap<u32, Derive> = HashMap::new();
    for p in &asm.patterns {
        if !active.contains(&p.source) {
            continue;
        }
        for (k, &copy) in p.copies.iter().enumerate() {
            let n = (k + 1) as f64;
            let d = match &p.kind {
                PatternKind::Linear { dir, step } => Derive::Rel { leader: p.source, rel: Pose::translation(scale(normalize(*dir), step * n)) },
                PatternKind::Circular { origin, axis, angle } => Derive::Rel { leader: p.source, rel: Pose::about(*origin, *axis, (angle * n).to_radians()) },
                PatternKind::Points { reference, offsets } => match offsets.get(k) {
                    Some(o) => Derive::Holes { source: p.source, reference: *reference, offset: *o },
                    None => continue,
                },
            };
            derived.insert(copy, d);
        }
    }
    for g in &asm.groups {
        let Some(&leader) = g.members.iter().find(|m| active.contains(m) && !derived.contains_key(m)) else { continue };
        let inv = stored[&leader].inverse();
        for &m in &g.members {
            if m != leader && active.contains(&m) && !derived.contains_key(&m) && stored.contains_key(&m) {
                derived.insert(m, Derive::Rel { leader, rel: inv.compose(&stored[&m]) });
            }
        }
    }
    let base: Vec<&Instance> = asm.instances.iter().filter(|i| active.contains(&i.id) && !derived.contains_key(&i.id)).collect();
    let anchor = if base.iter().any(|i| i.fixed) { None } else { base.first().map(|i| i.id) };
    let free: Vec<&Instance> = base.iter().copied().filter(|i| !i.fixed && Some(i.id) != anchor).collect();
    let slot: HashMap<u32, usize> = free.iter().enumerate().map(|(k, i)| (i.id, k * 6)).collect();
    let mut x = vec![0.0; free.len() * 6];
    for (k, i) in free.iter().enumerate() {
        for c in 0..3 {
            x[k * 6 + c] = i.position[c];
            x[k * 6 + 3 + c] = i.rotation[c];
        }
    }
    let mut s = Solver { stored, derived, slot, mates, open, blocks: Vec::new() };
    let mut blocks = Vec::new();
    for (k, m) in s.mates.iter().enumerate() {
        let mut deps = Vec::new();
        for c in m.connectors() {
            s.deps(c.instance, &mut deps, 0);
        }
        blocks.push(Block { kind: BlockKind::Mate(k), rows: mate_rows(m), deps });
    }
    for (k, r) in s.open.iter().enumerate() {
        let mut deps = Vec::new();
        for id in r.mates() {
            if let Some(m) = s.mates.iter().find(|m| m.id == id) {
                for c in m.connectors() {
                    s.deps(c.instance, &mut deps, 0);
                }
            }
        }
        let angular = matches!(r, Relation::Gear { .. } | Relation::Belt { .. });
        blocks.push(Block { kind: BlockKind::Relation(k), rows: vec![angular], deps });
    }
    s.blocks = blocks;
    // Grupos de piezas conectadas (unión de conjuntos sobre las incógnitas)
    let ids: Vec<u32> = free.iter().map(|i| i.id).collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            p[r] = p[p[r]];
            r = p[r];
        }
        r
    }
    let pos = |id: u32| ids.iter().position(|x| *x == id).unwrap();
    for b in &s.blocks {
        for w in b.deps.windows(2) {
            let (ra, rb) = (find(&mut parent, pos(w[0])), find(&mut parent, pos(w[1])));
            parent[ra] = rb;
        }
    }
    let mut comps: Vec<(Vec<u32>, Vec<usize>)> = Vec::new();
    let mut comp_of: HashMap<usize, usize> = HashMap::new();
    for (k, id) in ids.iter().enumerate() {
        let r = find(&mut parent, k);
        let c = *comp_of.entry(r).or_insert_with(|| {
            comps.push((Vec::new(), Vec::new()));
            comps.len() - 1
        });
        comps[c].0.push(*id);
    }
    let mut constant = Vec::new();
    for (bi, b) in s.blocks.iter().enumerate() {
        match b.deps.first() {
            Some(&id) => {
                let c = comp_of[&find(&mut parent, pos(id))];
                comps[c].1.push(bi);
            }
            None => constant.push(bi),
        }
    }
    // Cada grupo por su lado; un ángulo pedido justo al otro lado (180°) deja el arranque en un
    // punto muerto del que el descenso no sale: se reintenta desde arranques corridos (siempre los mismos)
    let mut total2 = 0.0;
    let mut dof = 0;
    let mut redundant = Vec::new();
    let mut instance_dof = Vec::new();
    for (members, blocks) in &comps {
        let vars: Vec<usize> = members.iter().flat_map(|id| { let j = s.slot[id]; j..j + 6 }).collect();
        let start = x.clone();
        let mut fnorm = s.lm(blocks, &vars, &mut x);
        let mut seed = 0x9e3779b97f4a7c15u64;
        for _ in 0..6 {
            if fnorm < 1e-6 || blocks.is_empty() {
                break;
            }
            let mut kick = start.clone();
            for id in members {
                let j = s.slot[id];
                for c in 0..3 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    kick[j + 3 + c] += ((seed >> 40) as f64 / (1u64 << 24) as f64 - 0.5) * 1.2;
                }
            }
            let ft = s.lm(blocks, &vars, &mut kick);
            if ft < fnorm {
                for &v in &vars {
                    x[v] = kick[v];
                }
                fnorm = ft;
            }
        }
        total2 += fnorm * fnorm;
        // Grados libres: incógnitas menos el rango del jacobiano en la solución
        let j = s.jacobian(blocks, &vars, &x);
        let r = rank(&j);
        let comp_dof = vars.len() - r;
        dof += comp_dof;
        // Qué instancias se mueven: las que aparecen en el espacio nulo
        if comp_dof > 0 {
            let null = nullspace(&j, vars.len());
            for (k, id) in members.iter().enumerate() {
                let rows = null.rows(k * 6, 6).clone_owned();
                let n = rank(&rows);
                if n > 0 {
                    instance_dof.push((*id, n));
                }
            }
        }
        // Uniones que sobran: sin sus filas, el rango no baja
        if fnorm < 1e-6 && r > 0 {
            let mut row = 0;
            for &bi in blocks {
                let len = s.blocks[bi].rows.len();
                if let BlockKind::Mate(k) = s.blocks[bi].kind
                    && len > 0
                {
                    let keep: Vec<usize> = (0..j.nrows()).filter(|i| *i < row || *i >= row + len).collect();
                    let sub = j.select_rows(keep.iter());
                    if rank(&sub) == r {
                        redundant.push(s.mates[k].id);
                    }
                }
                row += len;
            }
        }
    }
    let fc = s.eval(&constant, &x);
    total2 += fc.norm_squared();
    // Uniones entre piezas que no se mueven: no agregan nada (si se cumplen)
    for &bi in &constant {
        if let BlockKind::Mate(k) = s.blocks[bi].kind {
            let mut r = Vec::new();
            s.eval_block(&s.blocks[bi], &x, &mut r);
            if r.iter().map(|v| v * v).sum::<f64>().sqrt() < 1e-6 && !r.is_empty() {
                redundant.push(s.mates[k].id);
            }
        }
    }
    // Desvío por unión
    let mut mate_errors = Vec::new();
    for b in &s.blocks {
        if let BlockKind::Mate(k) = b.kind {
            let mut r = Vec::new();
            s.eval_block(b, &x, &mut r);
            let e = r.iter().map(|v| v * v).sum::<f64>().sqrt();
            if e > 1e-6 {
                mate_errors.push((s.mates[k].id, e));
            }
        }
    }
    // Las de las instancias de un grupo que se mueve también se mueven
    for (id, d) in &s.derived {
        let mut deps = Vec::new();
        s.deps(*id, &mut deps, 0);
        if let Some(n) = deps.iter().filter_map(|l| instance_dof.iter().find(|x| x.0 == *l).map(|x| x.1)).max() {
            let _ = d;
            instance_dof.push((*id, n));
        }
    }
    instance_dof.sort_by_key(|x| asm.instances.iter().position(|i| i.id == x.0));
    let poses = asm
        .instances
        .iter()
        .map(|i| match s.pose(&x, i.id, 0) {
            Some(p) if active.contains(&i.id) => (i.id, p.t, p.rotation_vector()),
            _ => (i.id, i.position, i.rotation),
        })
        .collect();
    let residual = total2.sqrt();
    AssemblySolution { poses, residual, converged: residual < 1e-6, dof, mate_errors, redundant, instance_dof }
}

/// Base del espacio nulo de `j` (columnas), con `n` incógnitas.
fn nullspace(j: &DMatrix<f64>, n: usize) -> DMatrix<f64> {
    if j.nrows() == 0 {
        return DMatrix::identity(n, n);
    }
    let jtj = j.transpose() * j;
    let svd = jtj.svd(false, true);
    let vt = svd.v_t.unwrap();
    let cols: Vec<usize> = (0..n).filter(|&k| svd.singular_values[k] < 1e-10).collect();
    let mut out = DMatrix::zeros(n, cols.len());
    for (c, &k) in cols.iter().enumerate() {
        for r in 0..n {
            out[(r, c)] = vt[(k, r)];
        }
    }
    out
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
/// cada uno, partiendo de la pose del cuadro anterior para que el movimiento sea continuo. Con
/// topes, se queda en el tope.
pub fn sweep(asm: &Assembly, mate: u32, var: SweepVar, from: f64, to: f64, frames: usize) -> Result<Vec<AssemblySolution>, String> {
    let mut cur = asm.clone();
    sweep_into(&mut cur, mate, var, from, to, frames)
}

fn sweep_into(cur: &mut Assembly, mate: u32, var: SweepVar, from: f64, to: f64, frames: usize) -> Result<Vec<AssemblySolution>, String> {
    let frames = frames.max(2);
    let i = cur.mates.iter().position(|m| m.id == mate).ok_or("la unión ya no existe")?;
    let ok = match (cur.mates[i].kind, var) {
        (MateKind::Revolute | MateKind::Cylindrical | MateKind::PinSlot, SweepVar::Angle) => true,
        (MateKind::Slider | MateKind::Cylindrical | MateKind::PinSlot, SweepVar::Distance) => true,
        _ => false,
    };
    if !ok {
        return Err("esa unión no se mueve así: usa el ángulo en bisagras y el desplazamiento en deslizantes".into());
    }
    let mut out = Vec::with_capacity(frames);
    for k in 0..frames {
        let v = from + (to - from) * k as f64 / (frames - 1) as f64;
        match var {
            SweepVar::Angle => cur.mates[i].angle = Some(cur.mates[i].clamp_angle(v)),
            SweepVar::Distance => cur.mates[i].distance = Some(cur.mates[i].clamp_distance(v)),
        }
        let sol = solve(cur);
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

/// La animación por pasos: cada paso recorre su unión (a `fps` cuadros por segundo) y la deja
/// donde terminó para los pasos siguientes.
pub fn sweep_steps(asm: &Assembly, steps: &[MotionStep], fps: f64) -> Result<Vec<AssemblySolution>, String> {
    if steps.is_empty() {
        return Err("la animación no tiene pasos".into());
    }
    let mut cur = asm.clone();
    let mut out = Vec::new();
    for (k, s) in steps.iter().enumerate() {
        let frames = ((s.seconds.max(0.05) * fps).round() as usize).max(2);
        let mut part = sweep_into(&mut cur, s.mate, s.var, s.from, s.to, frames).map_err(|e| format!("paso {}: {e}", k + 1))?;
        if k > 0 {
            part.remove(0);
        }
        out.extend(part);
    }
    Ok(out)
}

/// Primer cuadro del recorrido en que dos piezas chocan: (cuadro, valor de la unión, instancias, volumen).
pub fn first_clash(ev: &Evaluation, asm: &Assembly, mate: u32, var: SweepVar, from: f64, to: f64, frames: usize) -> Result<Option<(usize, f64, u32, u32, f64)>, String> {
    let sols = sweep(asm, mate, var, from, to, frames)?;
    let n = sols.len();
    for (k, sol) in sols.iter().enumerate() {
        let shapes = instance_shapes(ev, asm, sol);
        if let Some(&(a, b, v)) = interferences_for(ev, asm, &shapes).first() {
            let value = from + (to - from) * k as f64 / (n.max(2) - 1) as f64;
            return Ok(Some((k, value, a, b, v)));
        }
    }
    Ok(None)
}

/// Cada instancia como forma en su lugar (las piezas que ya no existen se saltean).
pub fn instance_shapes(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution) -> Vec<(u32, Shape)> {
    sol.poses
        .iter()
        .filter_map(|(id, p, r)| {
            let inst = asm.instances.iter().find(|i| i.id == *id && !i.suppressed)?;
            let part = ev.parts.iter().find(|x| x.id == inst.part)?;
            let shape = match inst.mirror {
                Some(n) if norm(n) > 1e-12 => part.shape.mirror([0.0; 3], normalize(n)).ok()?,
                _ => part.shape.clone(),
            };
            shape.transform(instance_matrix(*p, *r)).ok().map(|s| (*id, s))
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
