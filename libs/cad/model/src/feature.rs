//! Operaciones del árbol. Cada una guarda su receta (parámetros), nunca el
//! sólido resultante. Distancias en mm, ángulos en grados.

use serde::{Deserialize, Serialize};

use crate::geom::{P2, P3, Plane};
use crate::sketch::Sketch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureId(pub u32);

/// Origen de una cara: la operación que la creó y qué parte de ella es
/// ("inicio", "fin", "lado:7" = lateral que barre la entidad 7 del sketch,
/// "+z" de una caja, "redondeo:0"…). Se recalcula en cada recálculo y viaja por
/// las booleanas y redondeos con la historia de OpenCASCADE: una cara partida
/// por un agujero conserva su origen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FaceTag {
    pub feature: FeatureId,
    pub name: String,
}

/// Una cara del sólido. Se resuelve por su origen (`tags`) y, entre las caras
/// con ese origen, la más cercana a `point`; sin origen reconocible, por
/// geometría (la cara más cercana con normal parecida).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FaceRef {
    pub point: P3,
    pub normal: P3,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<FaceTag>,
}

/// Una arista: un punto sobre ella y su dirección ahí, más los orígenes de las
/// dos caras que separa ("la arista entre la tapa y el frente").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeRef {
    pub point: P3,
    pub direction: P3,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<Vec<FaceTag>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum PlaneSpec {
    #[default]
    Xy,
    Xz,
    Yz,
    /// Plano de una cara plana del sólido (sigue a la cara si esta se mueve).
    Face { face: FaceRef },
    Custom { plane: Plane },
    /// Un plano de referencia del historial.
    Reference { feature: FeatureId },
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisSpec {
    X,
    Y,
    Z,
    /// Una línea de un sketch (típico eje de revolución).
    SketchLine { sketch: FeatureId, line: u32 },
    /// Una arista recta o el eje de una arista circular.
    Edge { edge: EdgeRef },
    Custom { origin: P3, direction: P3 },
    /// Un eje de referencia del historial.
    Reference { feature: FeatureId },
}

/// Un punto: fijo, en una arista o de referencia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PointSpec {
    /// Coordenadas (mm).
    At { point: P3 },
    /// Centro de una arista circular (o el medio de una recta).
    Center { edge: EdgeRef },
    /// Sobre una arista recta, a una fracción de su largo (0 = inicio, 1 = fin).
    OnEdge { edge: EdgeRef, at: f64 },
    /// Un punto de referencia del historial.
    Reference { feature: FeatureId },
}

/// Cómo se define un plano de referencia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneDef {
    /// Paralelo a otro, a una distancia.
    Offset { base: PlaneSpec, distance: f64 },
    /// Otro plano girado alrededor de un eje (grados).
    Angle { base: PlaneSpec, axis: AxisSpec, angle: f64 },
    /// A mitad de camino entre dos planos paralelos.
    Midplane { a: PlaneSpec, b: PlaneSpec },
    /// Por tres puntos.
    ThreePoints { points: [PointSpec; 3] },
}

/// Cómo se define un eje de referencia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisDef {
    TwoPoints { a: PointSpec, b: PointSpec },
    /// Arista recta, o el eje de una circular.
    Edge { edge: EdgeRef },
    /// Eje de una cara cilíndrica o cónica.
    Face { face: FaceRef },
    /// Intersección de dos planos.
    Planes { a: PlaneSpec, b: PlaneSpec },
}

fn origin_point() -> PointSpec {
    PointSpec::At { point: [0.0; 3] }
}

fn plane_deps(p: &PlaneSpec) -> Vec<FeatureId> {
    match p {
        PlaneSpec::Reference { feature } => vec![*feature],
        _ => vec![],
    }
}

fn axis_deps(a: &AxisSpec) -> Vec<FeatureId> {
    match a {
        AxisSpec::SketchLine { sketch, .. } => vec![*sketch],
        AxisSpec::Reference { feature } => vec![*feature],
        _ => vec![],
    }
}

fn point_deps(p: &PointSpec) -> Vec<FeatureId> {
    match p {
        PointSpec::Reference { feature } => vec![*feature],
        _ => vec![],
    }
}

/// Qué hace la herramienta con las piezas. Unir funde la herramienta con las
/// piezas que toca (si no toca ninguna, es una pieza nueva); restar e
/// intersectar actúan sobre cada pieza; `New` siempre crea una pieza aparte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyOp {
    #[default]
    Join,
    Cut,
    Intersect,
    New,
}

/// Pieza del diseño: la operación que la creó y el número de pieza dentro de
/// ella (una unión que junta piezas conserva el de la primera).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PartId {
    pub feature: FeatureId,
    pub index: u32,
}

/// Qué regiones del sketch usar.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegionSelection {
    /// Todas las de profundidad par (anillo sí, su agujero no).
    #[default]
    All,
    /// Las regiones que contienen estos puntos (coordenadas del sketch).
    Points { points: Vec<P2> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Extent {
    Blind { distance: f64 },
    /// Mitad hacia cada lado del plano.
    Symmetric { distance: f64 },
    /// Atraviesa todo el cuerpo.
    ThroughAll,
    /// Hasta el plano de una cara.
    UpToFace { face: FaceRef },
    /// Hacia los dos lados del plano: `distance` hacia la normal y `second` hacia atrás.
    TwoSides { distance: f64, second: f64 },
    /// Hasta la primera cara del sólido que encuentra.
    UpToNext,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extrude {
    pub sketch: FeatureId,
    #[serde(default)]
    pub regions: RegionSelection,
    pub extent: Extent,
    /// Extruir hacia el lado contrario de la normal del plano.
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub op: BodyOp,
    /// Ángulo de desmolde de las paredes (grados; positivo = se angosta al avanzar).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub draft: f64,
    /// Extrusión delgada: una pared de este espesor siguiendo el contorno.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thin: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revolve {
    pub sketch: FeatureId,
    #[serde(default)]
    pub regions: RegionSelection,
    pub axis: AxisSpec,
    pub angle: f64,
    #[serde(default)]
    pub op: BodyOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrimitiveShape {
    /// `centered`: centrada en el origen en X e Y; `centered_z`: también en Z
    /// (el centro de la caja en el origen, como las nuevas). Sin `centered_z`
    /// la base queda en el origen; los documentos viejos la tienen en una esquina.
    Box {
        dx: f64,
        dy: f64,
        dz: f64,
        #[serde(default)]
        centered: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        centered_z: bool,
    },
    Cylinder { radius: f64, height: f64 },
    Cone { r1: f64, r2: f64, height: f64 },
    Sphere { radius: f64 },
    Torus { major: f64, minor: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Primitive {
    pub shape: PrimitiveShape,
    /// Origen; eje Z y X de la primitiva.
    pub origin: P3,
    #[serde(default = "default_z")]
    pub z: P3,
    #[serde(default = "default_x")]
    pub x: P3,
    #[serde(default)]
    pub op: BodyOp,
}

fn default_z() -> P3 {
    [0.0, 0.0, 1.0]
}
fn default_x() -> P3 {
    [1.0, 0.0, 0.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternKind {
    Linear { direction: P3, count: u32, spacing: f64 },
    /// `angle` total; 360 reparte las copias en la vuelta completa.
    Circular { axis: AxisSpec, count: u32, angle: f64 },
    /// Copias repartidas a lo largo de un camino (de punta a punta), trasladadas.
    Curve { path: SweepPath, count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    Step,
    Brep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureKind {
    Sketch {
        #[serde(default)]
        plane: PlaneSpec,
        /// Desplazamiento a lo largo de la normal del plano.
        #[serde(default)]
        offset: f64,
        sketch: Sketch,
    },
    Extrude(Extrude),
    Revolve(Revolve),
    Primitive(Primitive),
    /// Redondeo; con `radius2`, variable de `radius` al comienzo de cada arista a `radius2` al final.
    Fillet {
        edges: Vec<EdgeRef>,
        radius: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius2: Option<f64>,
    },
    /// Chaflán: `distance` en las dos caras, o asimétrico con `second`.
    Chamfer {
        edges: Vec<EdgeRef>,
        distance: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        second: Option<ChamferSecond>,
    },
    /// Ahuecar dejando paredes de `thickness` y abriendo `faces`.
    Shell { faces: Vec<FaceRef>, thickness: f64 },
    /// Desmolde: inclina `faces` respecto de la normal de `neutral`.
    Draft { faces: Vec<FaceRef>, neutral: PlaneSpec, angle: f64 },
    /// Repite las herramientas de `features` (vacío = todo el cuerpo).
    Pattern { features: Vec<FeatureId>, pattern: PatternKind },
    /// Refleja las herramientas de `features` (vacío = todo el cuerpo) y las une.
    Mirror { features: Vec<FeatureId>, plane: PlaneSpec },
    /// Corta por un plano y conserva el lado de la normal (o el otro con `flip`).
    Split {
        plane: PlaneSpec,
        #[serde(default)]
        flip: bool,
    },
    Import {
        format: ImportFormat,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        #[serde(default)]
        op: BodyOp,
    },
    /// Booleana entre piezas: unir todas en la primera de `targets`, restar
    /// `tools` de cada una de `targets` (conservándolas o no) o dejar lo común.
    Boolean {
        op: PartBoolean,
        targets: Vec<PartId>,
        #[serde(default)]
        tools: Vec<PartId>,
        #[serde(default)]
        keep_tools: bool,
    },
    /// Separa en piezas los sólidos sueltos de cada pieza (vacío = todas).
    SplitParts {
        #[serde(default)]
        parts: Vec<PartId>,
    },
    /// Saca piezas del diseño (desde acá en adelante).
    DeleteParts { parts: Vec<PartId> },
    /// Geometría de referencia: no cambia el sólido; sirve para sketches,
    /// revoluciones, patrones y simetrías.
    Plane { def: PlaneDef },
    Axis { def: AxisDef },
    Point { def: PointSpec },
    /// Perfil llevado a lo largo de un camino.
    Sweep(Sweep),
    /// Sólido que pasa por varias secciones (una región por sketch).
    Loft(Loft),
    /// Agujeros en los puntos de un sketch.
    Hole(Hole),
    /// Curva helicoidal de referencia (camino de resortes y roscas).
    Helix {
        axis: AxisSpec,
        radius: f64,
        pitch: f64,
        turns: f64,
        #[serde(default)]
        left: bool,
    },
    /// Mueve caras planas a lo largo de su normal (afuera suma, adentro resta).
    MoveFace { faces: Vec<FaceRef>, distance: f64 },
    /// Escala las piezas (o las del alcance) alrededor de un punto, por eje.
    Scale {
        factor: P3,
        #[serde(default = "origin_point")]
        center: PointSpec,
    },
    /// Da espesor a caras del sólido (hacia afuera; negativo, hacia adentro).
    Thicken {
        faces: Vec<FaceRef>,
        thickness: f64,
        #[serde(default)]
        op: BodyOp,
    },
    /// Nervio: pared de `thickness` centrada en el plano del sketch, desde sus
    /// líneas hasta el sólido (del lado donde lo encuentra; `flip` prueba
    /// primero el otro).
    Rib {
        sketch: FeatureId,
        thickness: f64,
        #[serde(default)]
        flip: bool,
    },
    /// Lleva caras planas hasta un plano (una cara plana del sólido o uno de
    /// referencia): suma o resta lo que hay entre la cara y el plano.
    ReplaceFace { faces: Vec<FaceRef>, target: PlaneSpec },
    /// Rosca sobre una cara cilíndrica: exterior en un eje (el cilindro es el
    /// diámetro nominal), interior en un agujero (el agujero es el diámetro
    /// menor). `length` 0 = toda la cara; arranca en un extremo (`flip`: el otro).
    Thread {
        face: FaceRef,
        pitch: f64,
        #[serde(default)]
        length: f64,
        #[serde(default)]
        flip: bool,
        #[serde(default)]
        left: bool,
        #[serde(default)]
        clearance: f64,
    },
}

/// La otra medida de un chaflán asimétrico; `distance` se mide sobre una de
/// las dos caras de la arista (la otra con `flip`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChamferSecond {
    Distance {
        distance: f64,
        #[serde(default)]
        flip: bool,
    },
    Angle {
        degrees: f64,
        #[serde(default)]
        flip: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hole {
    /// Sketch de los centros: sus puntos sueltos (o los de `points`); si no
    /// tiene, los centros de sus círculos. El agujero entra contra la normal
    /// del plano (en un sketch sobre una cara, hacia adentro del material).
    pub sketch: FeatureId,
    #[serde(default)]
    pub points: Vec<u32>,
    pub diameter: f64,
    pub depth: HoleDepth,
    #[serde(default)]
    pub style: HoleStyle,
    /// Ángulo de la punta en los ciegos (grados; 0 = fondo plano).
    #[serde(default)]
    pub tip_angle: f64,
    /// Rosca cosmética (solo dato, p. ej. "M6").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// Rosca modelada: el agujero sale con el filete de verdad (para imprimir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modeled: Option<ThreadSpec>,
}

/// Rosca métrica ISO (perfil de 60°).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThreadSpec {
    /// Diámetro nominal (el mayor), p. ej. 6 para M6.
    pub nominal: f64,
    pub pitch: f64,
    /// Holgura en el diámetro: agranda las roscas interiores y achica las
    /// exteriores (impresas a medida exacta no entran; 0,2–0,4 mm en FDM).
    #[serde(default)]
    pub clearance: f64,
    #[serde(default)]
    pub left: bool,
}

impl ThreadSpec {
    /// Diámetro menor del perfil básico ISO: d − 1,0825 P.
    pub fn minor(&self) -> f64 {
        self.nominal - 1.082_532 * self.pitch
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoleDepth {
    Blind { depth: f64 },
    ThroughAll,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoleStyle {
    #[default]
    Simple,
    /// Caja para la cabeza: diámetro y profundidad.
    Counterbore { diameter: f64, depth: f64 },
    /// Avellanado: diámetro en la superficie y ángulo total (grados).
    Countersink { diameter: f64, angle: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sweep {
    /// Sketch del perfil y qué regiones.
    pub sketch: FeatureId,
    #[serde(default)]
    pub regions: RegionSelection,
    pub path: SweepPath,
    #[serde(default)]
    pub op: BodyOp,
}

/// Camino de un barrido.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SweepPath {
    /// Entidades de un sketch encadenadas por sus extremos (vacío = todas las que
    /// no son de construcción).
    Sketch {
        sketch: FeatureId,
        #[serde(default)]
        entities: Vec<u32>,
    },
    /// Una curva del historial (una hélice).
    Curve { feature: FeatureId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loft {
    pub sections: Vec<LoftSection>,
    /// Caras planas entre secciones (si no, superficies suaves).
    #[serde(default)]
    pub ruled: bool,
    #[serde(default)]
    pub op: BodyOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoftSection {
    pub sketch: FeatureId,
    #[serde(default)]
    pub regions: RegionSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartBoolean {
    Union,
    Subtract,
    Intersect,
}

impl FeatureKind {
    /// Operaciones de las que depende explícitamente.
    pub fn dependencies(&self) -> Vec<FeatureId> {
        let axis_dep = axis_deps;
        match self {
            FeatureKind::Sketch { plane, .. } => plane_deps(plane),
            FeatureKind::Draft { neutral, .. } => plane_deps(neutral),
            FeatureKind::Split { plane, .. } => plane_deps(plane),
            FeatureKind::Plane { def } => match def {
                PlaneDef::Offset { base, .. } => plane_deps(base),
                PlaneDef::Angle { base, axis, .. } => [plane_deps(base), axis_deps(axis)].concat(),
                PlaneDef::Midplane { a, b } => [plane_deps(a), plane_deps(b)].concat(),
                PlaneDef::ThreePoints { points } => points.iter().flat_map(point_deps).collect(),
            },
            FeatureKind::Axis { def } => match def {
                AxisDef::TwoPoints { a, b } => [point_deps(a), point_deps(b)].concat(),
                AxisDef::Planes { a, b } => [plane_deps(a), plane_deps(b)].concat(),
                _ => vec![],
            },
            FeatureKind::Point { def } => point_deps(def),
            FeatureKind::Sweep(s) => match &s.path {
                SweepPath::Sketch { sketch, .. } => vec![s.sketch, *sketch],
                SweepPath::Curve { feature } => vec![s.sketch, *feature],
            },
            FeatureKind::Helix { axis, .. } => axis_deps(axis),
            FeatureKind::Scale { center, .. } => point_deps(center),
            FeatureKind::Loft(l) => l.sections.iter().map(|s| s.sketch).collect(),
            FeatureKind::Hole(h) => vec![h.sketch],
            FeatureKind::Rib { sketch, .. } => vec![*sketch],
            FeatureKind::ReplaceFace { target, .. } => plane_deps(target),
            FeatureKind::Extrude(e) => vec![e.sketch],
            FeatureKind::Revolve(r) => {
                let mut d = vec![r.sketch];
                d.extend(axis_dep(&r.axis));
                d
            }
            FeatureKind::Pattern { features, pattern } => {
                let mut d = features.clone();
                match pattern {
                    PatternKind::Circular { axis, .. } => d.extend(axis_dep(axis)),
                    PatternKind::Curve { path: SweepPath::Sketch { sketch, .. }, .. } => d.push(*sketch),
                    PatternKind::Curve { path: SweepPath::Curve { feature }, .. } => d.push(*feature),
                    _ => {}
                }
                d
            }
            FeatureKind::Mirror { features, plane } => [features.clone(), plane_deps(plane)].concat(),
            // Las operaciones que crearon las piezas que usa
            FeatureKind::Boolean { targets, tools, .. } => targets.iter().chain(tools).map(|p| p.feature).collect(),
            FeatureKind::SplitParts { parts } | FeatureKind::DeleteParts { parts } => parts.iter().map(|p| p.feature).collect(),
            _ => vec![],
        }
    }

    /// Nombre por defecto para mostrar.
    pub fn label(&self) -> &'static str {
        match self {
            FeatureKind::Sketch { .. } => "Sketch",
            FeatureKind::Extrude(_) => "Extrusión",
            FeatureKind::Revolve(_) => "Revolución",
            FeatureKind::Primitive(p) => match p.shape {
                PrimitiveShape::Box { .. } => "Caja",
                PrimitiveShape::Cylinder { .. } => "Cilindro",
                PrimitiveShape::Cone { .. } => "Cono",
                PrimitiveShape::Sphere { .. } => "Esfera",
                PrimitiveShape::Torus { .. } => "Toro",
            },
            FeatureKind::Fillet { .. } => "Redondeo",
            FeatureKind::Chamfer { .. } => "Chaflán",
            FeatureKind::Shell { .. } => "Vaciado",
            FeatureKind::Draft { .. } => "Desmolde",
            FeatureKind::Pattern { .. } => "Patrón",
            FeatureKind::Mirror { .. } => "Simetría",
            FeatureKind::Split { .. } => "Corte",
            FeatureKind::Import { .. } => "Importado",
            FeatureKind::Boolean { .. } => "Booleana",
            FeatureKind::SplitParts { .. } => "Separar piezas",
            FeatureKind::DeleteParts { .. } => "Borrar pieza",
            FeatureKind::Plane { .. } => "Plano",
            FeatureKind::Axis { .. } => "Eje",
            FeatureKind::Point { .. } => "Punto",
            FeatureKind::Sweep(_) => "Barrido",
            FeatureKind::Loft(_) => "Transición",
            FeatureKind::Hole(_) => "Agujero",
            FeatureKind::Helix { .. } => "Hélice",
            FeatureKind::Thicken { .. } => "Engrosar",
            FeatureKind::Rib { .. } => "Nervio",
            FeatureKind::ReplaceFace { .. } => "Reemplazar cara",
            FeatureKind::Thread { .. } => "Rosca",
            FeatureKind::MoveFace { .. } => "Mover cara",
            FeatureKind::Scale { .. } => "Escala",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub name: String,
    #[serde(default)]
    pub suppressed: bool,
    pub kind: FeatureKind,
    /// Con qué piezas une, resta o interseca (vacío = las que toca).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<PartId>,
}
