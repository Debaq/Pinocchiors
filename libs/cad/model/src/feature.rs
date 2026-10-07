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
    Fillet { edges: Vec<EdgeRef>, radius: f64 },
    Chamfer { edges: Vec<EdgeRef>, distance: f64 },
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
}

impl FeatureKind {
    /// Operaciones de las que depende explícitamente.
    pub fn dependencies(&self) -> Vec<FeatureId> {
        let axis_dep = |a: &AxisSpec| match a {
            AxisSpec::SketchLine { sketch, .. } => vec![*sketch],
            _ => vec![],
        };
        match self {
            FeatureKind::Extrude(e) => vec![e.sketch],
            FeatureKind::Revolve(r) => {
                let mut d = vec![r.sketch];
                d.extend(axis_dep(&r.axis));
                d
            }
            FeatureKind::Pattern { features, pattern } => {
                let mut d = features.clone();
                if let PatternKind::Circular { axis, .. } = pattern {
                    d.extend(axis_dep(axis));
                }
                d
            }
            FeatureKind::Mirror { features, .. } => features.clone(),
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
}
