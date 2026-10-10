//! Sketch 2D: puntos, entidades que los comparten y restricciones.
//!
//! Los ids son estables (no se reutilizan): la UI y las referencias de otras
//! operaciones (eje de revolución = una línea del sketch) no se rompen al borrar.
//! Las entidades comparten puntos por id: un rectángulo son 4 puntos y 4 líneas.

use std::collections::HashMap;

use cad_solver::{BSplineRef, Constraint, ConstraintSystem, CurveEnd, CurvePart, Point2, SolveStatus};
use serde::{Deserialize, Serialize};

use crate::geom::{P2, dist2};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchPoint {
    pub id: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchEntity {
    pub id: u32,
    /// Geometría de construcción: restringe pero no forma perfiles.
    #[serde(default)]
    pub construction: bool,
    /// Línea infinita (de construcción, para referencias): se dibuja de punta
    /// a punta de la vista; para las restricciones es una línea más.
    #[serde(default, skip_serializing_if = "is_false")]
    pub infinite: bool,
    /// Línea central (de construcción): el eje de revolución y de las cotas
    /// de diámetro que se toma sin elegirlo.
    #[serde(default, skip_serializing_if = "is_false")]
    pub axis: bool,
    pub geometry: Geometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry {
    Line { start: u32, end: u32 },
    Circle { center: u32, radius: f64 },
    /// Arco antihorario (visto desde la normal del plano) de `start` a `end`.
    Arc { center: u32, start: u32, end: u32 },
    /// Spline interpolada por los puntos; `closed` une el último con el primero.
    /// Manijas opcionales (solo abierta): la tangente de salida va del primer
    /// punto a `start_handle` y la de llegada del último a `end_handle` (las
    /// dos en el sentido de avance).
    Spline {
        points: Vec<u32>,
        closed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_handle: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_handle: Option<u32>,
        /// Manijas en puntos intermedios: `[i, h]` da la dirección en
        /// `points[i]` (de ese punto a `h`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        handles: Vec<[u32; 2]>,
    },
    /// Punto suelto (para agujeros y referencias): no forma perfiles.
    Point { point: u32 },
    /// Elipse: `major` y `minor` son los extremos de los semiejes (a 90°, la
    /// perpendicular la pone el solver); acotar sus distancias al centro da
    /// los radios.
    Ellipse { center: u32, major: u32, minor: u32 },
    /// Arco de elipse antihorario de `start` a `end` (el solver los deja
    /// sobre la elipse de `center`, `major` y `minor`).
    EllipseArc { center: u32, major: u32, minor: u32, start: u32, end: u32 },
    /// B-spline por sus polos: abierta, pasa por el primero y el último;
    /// cerrada, periódica. Con `weights` es racional (cónicas: grado 2 con
    /// el peso del medio = rho / (1 − rho)). `knots` vacío = uniforme.
    #[serde(rename = "bspline")]
    BSpline {
        poles: Vec<u32>,
        degree: u32,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        knots: Vec<f64>,
    },
}

impl Geometry {
    pub fn point_ids(&self) -> Vec<u32> {
        match self {
            Geometry::Line { start, end } => vec![*start, *end],
            Geometry::Circle { center, .. } => vec![*center],
            Geometry::Arc { center, start, end } => vec![*center, *start, *end],
            Geometry::Spline { points, start_handle, end_handle, handles, .. } => {
                points.iter().chain(start_handle).chain(end_handle).copied().chain(handles.iter().map(|h| h[1])).collect()
            }
            Geometry::Point { point } => vec![*point],
            Geometry::Ellipse { center, major, minor } => vec![*center, *major, *minor],
            Geometry::EllipseArc { center, major, minor, start, end } => vec![*center, *major, *minor, *start, *end],
            Geometry::BSpline { poles, .. } => poles.clone(),
        }
    }

    /// Extremos de una curva abierta (comienzo y fin, en su sentido).
    pub fn ends(&self) -> Option<(u32, u32)> {
        match self {
            Geometry::Line { start, end } | Geometry::Arc { start, end, .. } | Geometry::EllipseArc { start, end, .. } => Some((*start, *end)),
            Geometry::Spline { points, closed: false, .. } => Some((*points.first()?, *points.last()?)),
            Geometry::BSpline { poles, closed: false, .. } => Some((*poles.first()?, *poles.last()?)),
            _ => None,
        }
    }

    /// Curva cerrada por sí sola.
    pub fn is_closed(&self) -> bool {
        matches!(self, Geometry::Circle { .. } | Geometry::Ellipse { .. } | Geometry::Spline { closed: true, .. } | Geometry::BSpline { closed: true, .. })
    }
}

/// Restricciones de alto nivel (las que ve el usuario). Distancias en mm,
/// ángulos en grados. Las cotas con `reference` no restringen: muestran la
/// medida (se actualiza al resolver), como las cotas entre paréntesis de Onshape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SketchConstraint {
    Coincident { a: u32, b: u32 },
    Fixed { point: u32, x: f64, y: f64 },
    Horizontal { line: u32 },
    Vertical { line: u32 },
    /// Dos puntos a la misma altura (`a.y = b.y`).
    HorizontalPoints { a: u32, b: u32 },
    /// Dos puntos en la misma vertical (`a.x = b.x`).
    VerticalPoints { a: u32, b: u32 },
    Parallel { a: u32, b: u32 },
    /// Dos líneas sobre la misma recta.
    Collinear { a: u32, b: u32 },
    Perpendicular { a: u32, b: u32 },
    /// Líneas del mismo largo, o círculos/arcos del mismo radio.
    Equal { a: u32, b: u32 },
    /// Tangencia entre línea y círculo/arco, o entre dos círculos/arcos (por
    /// fuera o por dentro, según cómo estén al resolver).
    Tangent { a: u32, b: u32 },
    /// Círculos o arcos con el mismo centro.
    Concentric { a: u32, b: u32 },
    PointOnLine { point: u32, line: u32 },
    PointOnCircle { point: u32, circle: u32 },
    Midpoint { point: u32, line: u32 },
    Symmetric { a: u32, b: u32, line: u32 },
    /// Patrón lineal: `b2 − b1 = a2 − a1` (puntos).
    EqualOffset { a1: u32, a2: u32, b1: u32, b2: u32 },
    /// Patrón circular: `b2` es `b1` girado alrededor de `center` lo mismo que
    /// `a2` respecto de `a1` (puntos).
    EqualRotation { center: u32, a1: u32, a2: u32, b1: u32, b2: u32 },
    Distance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// `b.x − a.x = value`
    HorizontalDistance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// `b.y − a.y = value`
    VerticalDistance { a: u32, b: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    Length { line: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    Radius { entity: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    Diameter { entity: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// Ángulo de `a` a `b`; con `supplementary`, de `a` a `b` invertida (180° − el ángulo).
    Angle {
        a: u32,
        b: u32,
        degrees: f64,
        #[serde(default, skip_serializing_if = "is_false")]
        supplementary: bool,
        #[serde(default, skip_serializing_if = "is_false")] reference: bool,
        #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts,
    },
    /// Distancia de un punto a la recta de una línea (entre paralelas: un
    /// extremo de una y la otra).
    PointLineDistance { point: u32, line: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// Cota simétrica respecto de un eje: el doble de la distancia del punto
    /// a la línea (el diámetro en un perfil de revolución).
    AxisDiameter { point: u32, line: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// Largo de un arco.
    ArcLength { arc: u32, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// Largo total de una cadena de líneas, arcos y círculos.
    CurveLength { entities: Vec<u32>, value: f64, #[serde(default, skip_serializing_if = "is_false")] reference: bool, #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts },
    /// Distancia mínima (o máxima con `max`) entre un círculo o arco y otro,
    /// un punto o una línea. `a` y `b` son ids de entidad o de punto (comparten
    /// numeración). La mínima entre círculos es por fuera, o por dentro si uno
    /// contiene al otro (según cómo están al resolver).
    CircleDistance {
        a: u32,
        b: u32,
        #[serde(default, skip_serializing_if = "is_false")]
        max: bool,
        value: f64,
        #[serde(default, skip_serializing_if = "is_false")] reference: bool,
        #[serde(default, skip_serializing_if = "DimOpts::is_default")] opts: DimOpts,
    },
    /// Mismo centro y mismo radio.
    Coradial { a: u32, b: u32 },
    /// Dos entidades del mismo tipo simétricas respecto de una línea.
    SymmetricEntities { a: u32, b: u32, line: u32 },
    /// Punto sobre cualquier curva (línea, círculo, arco, elipse o spline).
    PointOnCurve { point: u32, curve: u32 },
    /// Punto en la intersección de dos curvas.
    Intersection { point: u32, a: u32, b: u32 },
    /// Entidad bloqueada entera: sus puntos y su radio quedan donde están.
    Lock { entity: u32 },
    /// Perforación: el punto va donde la curva `curve` (una operación con
    /// curva: hélice, sketch 3D o envuelto) cruza el plano del sketch. `at` lo
    /// pone el historial al recalcular (para el solver es un punto fijo).
    Pierce { point: u32, curve: u32, at: [f64; 2] },
    /// Continuidad de curvatura (G2) donde dos curvas se juntan: tangentes y
    /// con la misma curvatura. Una de las dos es una B-spline; la otra, una
    /// línea, un arco u otra B-spline.
    Curvature { a: u32, b: u32 },
}

/// Cómo se muestra y se trata una cota (no cambia lo que restringe).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DimOpts {
    /// No se edita por error ni la cambia transformar.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// Lugar del texto respecto de donde lo pondría el sketch (mm del plano).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<[f64; 2]>,
    /// Se dibuja como cota de ordenadas: el valor junto al punto medido.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ordinate: bool,
}

impl DimOpts {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl SketchConstraint {
    /// Restricción con valor editable (cota).
    pub fn value(&self) -> Option<f64> {
        use SketchConstraint::*;
        match self {
            Distance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | PointLineDistance { value, .. }
            | AxisDiameter { value, .. }
            | ArcLength { value, .. }
            | CurveLength { value, .. }
            | CircleDistance { value, .. } => Some(*value),
            Angle { degrees, .. } => Some(*degrees),
            _ => None,
        }
    }

    /// Cota de referencia (no restringe).
    pub fn is_reference(&self) -> bool {
        use SketchConstraint::*;
        matches!(
            self,
            Distance { reference: true, .. }
                | HorizontalDistance { reference: true, .. }
                | VerticalDistance { reference: true, .. }
                | Length { reference: true, .. }
                | Radius { reference: true, .. }
                | Diameter { reference: true, .. }
                | Angle { reference: true, .. }
                | PointLineDistance { reference: true, .. }
                | AxisDiameter { reference: true, .. }
                | ArcLength { reference: true, .. }
                | CurveLength { reference: true, .. }
                | CircleDistance { reference: true, .. }
        )
    }

    pub fn set_value(&mut self, v: f64) -> bool {
        use SketchConstraint::*;
        match self {
            Distance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | PointLineDistance { value, .. }
            | AxisDiameter { value, .. }
            | ArcLength { value, .. }
            | CurveLength { value, .. }
            | CircleDistance { value, .. } => *value = v,
            Angle { degrees, .. } => *degrees = v,
            _ => return false,
        }
        true
    }

    /// Ids (de puntos o entidades) que menciona: para borrar en cascada.
    fn mentions(&self, point: Option<u32>, entity: Option<u32>) -> bool {
        use SketchConstraint::*;
        let p = |id: &u32| Some(*id) == point;
        let e = |id: &u32| Some(*id) == entity;
        match self {
            Coincident { a, b } | HorizontalPoints { a, b } | VerticalPoints { a, b } => p(a) || p(b),
            Fixed { point: q, .. } | Pierce { point: q, .. } => p(q),
            Horizontal { line } | Vertical { line } | Length { line, .. } => e(line),
            Parallel { a, b } | Collinear { a, b } | Perpendicular { a, b } | Equal { a, b } | Tangent { a, b } | Concentric { a, b } | Angle { a, b, .. } => {
                e(a) || e(b)
            }
            PointOnLine { point: q, line } | Midpoint { point: q, line } | PointLineDistance { point: q, line, .. } | AxisDiameter { point: q, line, .. } => {
                p(q) || e(line)
            }
            ArcLength { arc, .. } => e(arc),
            PointOnCircle { point: q, circle } => p(q) || e(circle),
            Symmetric { a, b, line } => p(a) || p(b) || e(line),
            EqualOffset { a1, a2, b1, b2 } => p(a1) || p(a2) || p(b1) || p(b2),
            EqualRotation { center, a1, a2, b1, b2 } => p(center) || p(a1) || p(a2) || p(b1) || p(b2),
            Distance { a, b, .. } | HorizontalDistance { a, b, .. } | VerticalDistance { a, b, .. } => p(a) || p(b),
            Radius { entity: x, .. } | Diameter { entity: x, .. } | Lock { entity: x } => e(x),
            CurveLength { entities, .. } => entities.iter().any(e),
            // Ids de punto o de entidad
            CircleDistance { a, b, .. } => p(a) || e(a) || p(b) || e(b),
            Coradial { a, b } | Curvature { a, b } => e(a) || e(b),
            SymmetricEntities { a, b, line } => e(a) || e(b) || e(line),
            PointOnCurve { point: q, curve } => p(q) || e(curve),
            Intersection { point: q, a, b } => p(q) || e(a) || e(b),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchStatus {
    /// Totalmente definido.
    WellConstrained,
    /// Resuelto pero con grados de libertad.
    UnderConstrained,
    /// Restricciones en conflicto (las de `conflicting`).
    OverConstrained,
    /// El solver no convergió.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveReport {
    pub status: SketchStatus,
    pub dof: i32,
    pub residual: f64,
    /// Índices en `Sketch::constraints` de las restricciones en conflicto.
    pub conflicting: Vec<usize>,
    /// Puntos que todavía pueden moverse.
    pub free_points: Vec<u32>,
    /// Entidades que todavía pueden moverse o cambiar de tamaño (alguno de
    /// sus puntos, o su radio, está libre).
    #[serde(default)]
    pub free_entities: Vec<u32>,
    /// Puntos con un solo grado libre y hacia dónde se mueven (unitaria); los
    /// demás libres se mueven en cualquier dirección.
    #[serde(default)]
    pub free_dirs: Vec<(u32, [f64; 2])>,
    /// Grados libres de cada entidad libre.
    #[serde(default)]
    pub entity_dof: Vec<(u32, i32)>,
    /// Círculos con el radio libre.
    #[serde(default)]
    pub free_radius: Vec<u32>,
    /// Hubo conflicto y se resolvió todo menos las que chocan.
    #[serde(default)]
    pub partial: bool,
}

/// Sistema del solver armado desde un sketch.
pub(crate) struct Built {
    pub sys: ConstraintSystem,
    /// Índice en el solver de cada punto (por id)
    pub index: HashMap<u32, usize>,
    /// Punto de borde de cada círculo o arco (el radio)
    pub rims: HashMap<u32, usize>,
    /// Cuántas ecuaciones implícitas van primero en `sys.constraints`
    pub implicit: usize,
    /// Restricción del sketch de cada ecuación desde `implicit`
    pub origin: Vec<usize>,
}

/// Ángulo en grados llevado a (−180, 180].
fn wrap_degrees(d: f64) -> f64 {
    let w = d.rem_euclid(360.0);
    if w > 180.0 { w - 360.0 } else { w }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SketchError {
    #[error("el punto {0} no existe")]
    NoPoint(u32),
    #[error("la entidad {0} no existe")]
    NoEntity(u32),
    #[error("la entidad {0} no es {1}")]
    WrongKind(u32, &'static str),
    #[error("restricción no soportada: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sketch {
    pub points: Vec<SketchPoint>,
    pub entities: Vec<SketchEntity>,
    pub constraints: Vec<SketchConstraint>,
    #[serde(default)]
    pub next_id: u32,
    /// Punto origen del sketch, fijo en (0, 0): se usa como cualquier otro
    /// punto (anclar, acotar) pero no se mueve ni se borra. Los sketches
    /// viejos no lo tienen hasta que se editan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<u32>,
    /// Textos insertados como curvas (se mueven en bloque y se pueden rehacer).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SketchText>,
    /// Aristas del sólido traídas al sketch ("Usar"): la entidad sigue a la
    /// arista proyectada cada vez que se recalcula; para el solver, fija.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<SketchUse>,
    /// Dirección horizontal (x) del sketch: la de este eje o arista
    /// proyectada al plano. Sin ella, la del plano.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis: Option<crate::feature::AxisSpec>,
    /// Normal invertida (se mira y se extruye desde el otro lado).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_normal: bool,
}

/// Entidad ligada al modelo: sigue a una arista del sólido (`edge`) o a otra
/// fuente (`source`) cada vez que se recalcula.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchUse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<crate::feature::EdgeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<UseSource>,
    pub entity: u32,
}

/// De dónde sale una entidad ligada que no es una arista.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UseSource {
    /// Una de las curvas donde el plano del sketch corta el sólido.
    Section,
    /// Una de las curvas del contorno del sólido visto desde la normal del plano.
    Silhouette,
    /// Una entidad de un sketch anterior.
    Sketch { feature: crate::feature::FeatureId, entity: u32 },
}

/// Texto del sketch: sus curvas se mueven en bloque con `anchor` (el comienzo
/// de la línea base), que se puede arrastrar, acotar o anclar como cualquier
/// punto. El texto, el tamaño y la fuente sirven para rehacerlo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchText {
    pub id: u32,
    pub text: String,
    pub size: f64,
    #[serde(default)]
    pub font: String,
    pub anchor: u32,
    pub entities: Vec<u32>,
    pub points: Vec<u32>,
    /// Negrita, cursiva, alineación y curva que sigue (para rehacerlo igual).
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    pub style: TextStyle,
}

/// Cómo se arma un texto del sketch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    /// "left" (o vacío), "center" o "right": respecto del ancla.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub align: String,
    /// Entidad que siguen las letras (el ancla se proyecta sobre ella).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<u32>,
    /// Giro o reflejo que recibió el texto (matriz 2×2 por filas, respecto del
    /// ancla): al rehacerlo se aplica igual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<[f64; 4]>,
}

impl TextStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Sketch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mueve un punto (sin resolver).
    pub fn set_point(&mut self, id: u32, p: P2) -> Result<(), SketchError> {
        let q = self.points.iter_mut().find(|q| q.id == id).ok_or(SketchError::NoPoint(id))?;
        (q.x, q.y) = (p[0], p[1]);
        Ok(())
    }

    fn fresh_id(&mut self) -> u32 {
        // Tolerar documentos armados a mano sin next_id
        let max = self.points.iter().map(|p| p.id).chain(self.entities.iter().map(|e| e.id)).max();
        self.next_id = self.next_id.max(max.map_or(0, |m| m + 1));
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // --- Construcción ---

    /// Id del punto origen; lo crea en (0, 0) si el sketch no lo tiene.
    pub fn ensure_origin(&mut self) -> u32 {
        if let Some(id) = self.origin.filter(|id| self.points.iter().any(|p| p.id == *id)) {
            return id;
        }
        let id = self.add_point(0.0, 0.0);
        self.origin = Some(id);
        id
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> u32 {
        let id = self.fresh_id();
        self.points.push(SketchPoint { id, x, y });
        id
    }

    pub fn add_entity(&mut self, geometry: Geometry) -> u32 {
        let id = self.fresh_id();
        self.entities.push(SketchEntity { id, construction: false, infinite: false, axis: false, geometry });
        id
    }

    pub fn add_line(&mut self, a: u32, b: u32) -> u32 {
        self.add_entity(Geometry::Line { start: a, end: b })
    }

    /// Línea con puntos nuevos.
    pub fn line(&mut self, a: P2, b: P2) -> u32 {
        let (p, q) = (self.add_point(a[0], a[1]), self.add_point(b[0], b[1]));
        self.add_line(p, q)
    }

    pub fn circle(&mut self, center: P2, radius: f64) -> u32 {
        let c = self.add_point(center[0], center[1]);
        self.add_entity(Geometry::Circle { center: c, radius })
    }

    /// Arco antihorario de `start` a `end` alrededor de `center`.
    pub fn arc(&mut self, center: P2, start: P2, end: P2) -> u32 {
        let c = self.add_point(center[0], center[1]);
        let s = self.add_point(start[0], start[1]);
        let e = self.add_point(end[0], end[1]);
        self.add_entity(Geometry::Arc { center: c, start: s, end: e })
    }

    /// Rectángulo con esquinas compartidas y restricciones horizontal/vertical.
    /// Devuelve las 4 líneas (abajo, derecha, arriba, izquierda).
    pub fn rectangle(&mut self, a: P2, b: P2) -> [u32; 4] {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        let p = [
            self.add_point(x0, y0),
            self.add_point(x1, y0),
            self.add_point(x1, y1),
            self.add_point(x0, y1),
        ];
        let l = [
            self.add_line(p[0], p[1]),
            self.add_line(p[1], p[2]),
            self.add_line(p[2], p[3]),
            self.add_line(p[3], p[0]),
        ];
        self.constraints.push(SketchConstraint::Horizontal { line: l[0] });
        self.constraints.push(SketchConstraint::Horizontal { line: l[2] });
        self.constraints.push(SketchConstraint::Vertical { line: l[1] });
        self.constraints.push(SketchConstraint::Vertical { line: l[3] });
        l
    }

    /// Polígono cerrado de líneas (puntos compartidos).
    pub fn polyline(&mut self, pts: &[P2]) -> Vec<u32> {
        let ids: Vec<u32> = pts.iter().map(|p| self.add_point(p[0], p[1])).collect();
        (0..ids.len()).map(|i| self.add_line(ids[i], ids[(i + 1) % ids.len()])).collect()
    }

    pub fn constrain(&mut self, c: SketchConstraint) -> usize {
        self.constraints.push(c);
        self.constraints.len() - 1
    }

    /// Borra una entidad, las restricciones que la nombran y los puntos que
    /// quedan sin usar.
    pub fn remove_entity(&mut self, id: u32) -> Result<(), SketchError> {
        let pos = self.entities.iter().position(|e| e.id == id).ok_or(SketchError::NoEntity(id))?;
        let removed = self.entities.remove(pos);
        self.constraints.retain(|c| !c.mentions(None, Some(id)));
        for p in removed.geometry.point_ids() {
            if Some(p) != self.origin && !self.entities.iter().any(|e| e.geometry.point_ids().contains(&p)) {
                self.points.retain(|q| q.id != p);
                self.constraints.retain(|c| !c.mentions(Some(p), None));
            }
        }
        Ok(())
    }

    // --- Consultas ---

    pub fn point(&self, id: u32) -> Result<P2, SketchError> {
        self.points.iter().find(|p| p.id == id).map(|p| [p.x, p.y]).ok_or(SketchError::NoPoint(id))
    }

    pub fn entity(&self, id: u32) -> Result<&SketchEntity, SketchError> {
        self.entities.iter().find(|e| e.id == id).ok_or(SketchError::NoEntity(id))
    }

    pub(crate) fn line_points(&self, id: u32) -> Result<(u32, u32), SketchError> {
        match self.entity(id)?.geometry {
            Geometry::Line { start, end } => Ok((start, end)),
            _ => Err(SketchError::WrongKind(id, "una línea")),
        }
    }

    /// Radio actual de un círculo o arco.
    pub fn radius(&self, id: u32) -> Result<f64, SketchError> {
        match self.entity(id)?.geometry {
            Geometry::Circle { radius, .. } => Ok(radius),
            Geometry::Arc { center, start, .. } => Ok(dist2(self.point(center)?, self.point(start)?)),
            _ => Err(SketchError::WrongKind(id, "un círculo o arco")),
        }
    }

    // --- Resolver ---

    /// Ajusta los puntos para cumplir las restricciones.
    pub fn solve(&mut self) -> Result<SolveReport, SketchError> {
        self.run_solver(None)
    }

    /// Resuelve con `point` arrastrado hacia `target` (lo más cerca posible
    /// si las restricciones no lo dejan llegar).
    pub fn solve_drag(&mut self, point: u32, target: P2) -> Result<SolveReport, SketchError> {
        self.point(point)?;
        self.run_solver(Some((point, target)))
    }

    fn run_solver(&mut self, drag: Option<(u32, P2)>) -> Result<SolveReport, SketchError> {
        if self.texts.is_empty() {
            return self.solve_system(drag);
        }
        // Los puntos de los textos que no toca nada más no entran al solver (un
        // texto tiene cientos y haría lento cada arrastre): se mueven con su ancla
        let order: HashMap<u32, usize> = self.points.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        let held = self.hold_text_points();
        // Arrastrar uno de esos puntos es arrastrar el texto entero
        let drag = drag.map(|(p, t)| match held.iter().find(|h| h.0 == p) {
            Some(&(_, a, d)) => (a, [t[0] - d[0], t[1] - d[1]]),
            None => (p, t),
        });
        let result = self.solve_system(drag);
        for &(id, a, d) in &held {
            let at = self.point(a)?;
            self.points.push(SketchPoint { id, x: at[0] + d[0], y: at[1] + d[1] });
        }
        self.points.sort_by_key(|p| order.get(&p.id).copied().unwrap_or(usize::MAX));
        let mut report = result?;
        // Si el ancla está libre, el texto entero lo está
        for t in &self.texts {
            if report.free_points.contains(&t.anchor) {
                report.free_points.extend(&t.points);
                report.free_entities.extend(&t.entities);
            }
        }
        report.free_points.sort_unstable();
        report.free_points.dedup();
        report.free_entities.sort_unstable();
        report.free_entities.dedup();
        Ok(report)
    }

    /// Saca de `points` los puntos de textos que no usa ninguna restricción ni
    /// otra entidad; devuelve cada uno con su ancla y su distancia a ella.
    fn hold_text_points(&mut self) -> Vec<(u32, u32, P2)> {
        let mut keep: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for e in &self.entities {
            let in_text = self.texts.iter().any(|t| t.entities.contains(&e.id));
            if !in_text || self.constraints.iter().any(|c| c.mentions(None, Some(e.id))) {
                keep.extend(e.geometry.point_ids());
            }
        }
        for p in &self.points {
            if self.constraints.iter().any(|c| c.mentions(Some(p.id), None)) {
                keep.insert(p.id);
            }
        }
        keep.extend(self.origin);
        keep.extend(self.texts.iter().map(|t| t.anchor));
        let mut held = Vec::new();
        for t in &self.texts {
            let Ok(a) = self.point(t.anchor) else { continue };
            for &q in &t.points {
                if !keep.contains(&q)
                    && let Ok(p) = self.point(q)
                {
                    held.push((q, t.anchor, [p[0] - a[0], p[1] - a[1]]));
                    keep.insert(q);
                }
            }
        }
        let gone: std::collections::HashSet<u32> = held.iter().map(|h| h.0).collect();
        self.points.retain(|p| !gone.contains(&p.id));
        held
    }

    /// Arma el sistema del solver: puntos, ecuaciones implícitas (radios,
    /// arcos, elipses, origen, aristas usadas, textos) y las restricciones.
    pub(crate) fn build(&self) -> Result<Built, SketchError> {
        let index: HashMap<u32, usize> = self.points.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        let mut sys = ConstraintSystem::new();
        for p in &self.points {
            sys.add_point(p.x, p.y);
        }
        let ix = |id: u32| index.get(&id).copied().ok_or(SketchError::NoPoint(id));

        // El radio es una incógnita más: cada círculo lleva un punto oculto en
        // su borde, a la derecha del centro (radio = distancia al centro). En
        // los arcos el borde es el inicio.
        let mut rims: HashMap<u32, usize> = HashMap::new();
        for e in &self.entities {
            match e.geometry {
                Geometry::Circle { center, radius } => {
                    let c = ix(center)?;
                    let p = self.point(center)?;
                    let rim = sys.add_point(p[0] + radius.abs().max(1e-9), p[1]);
                    sys.add_constraint(Constraint::Horizontal { p1_idx: c, p2_idx: rim });
                    rims.insert(e.id, rim);
                }
                Geometry::Arc { start, .. } => {
                    rims.insert(e.id, ix(start)?);
                }
                _ => {}
            }
        }
        // Implícitas: los extremos de un arco equidistan del centro; los
        // semiejes de una elipse son perpendiculares
        for e in &self.entities {
            if let Geometry::Arc { center, start, end } = e.geometry {
                let (c, s, t) = (ix(center)?, ix(start)?, ix(end)?);
                sys.add_constraint(Constraint::EqualLength { l1_p1: c, l1_p2: s, l2_p1: c, l2_p2: t });
            }
            if let Geometry::Ellipse { center, major, minor } = e.geometry {
                let (c, a, b) = (ix(center)?, ix(major)?, ix(minor)?);
                sys.add_constraint(Constraint::Perpendicular { l1_p1: c, l1_p2: a, l2_p1: c, l2_p2: b });
            }
            // Arco de elipse: semiejes a 90° y los extremos sobre la elipse
            if let Geometry::EllipseArc { center, major, minor, start, end } = e.geometry {
                let (c, a, b) = (ix(center)?, ix(major)?, ix(minor)?);
                sys.add_constraint(Constraint::Perpendicular { l1_p1: c, l1_p2: a, l2_p1: c, l2_p2: b });
                for p in [start, end] {
                    sys.add_constraint(Constraint::PointOnEllipse { p_idx: ix(p)?, center: c, major: a, minor: b });
                }
            }
        }
        if let Some(o) = self.origin {
            sys.add_constraint(Constraint::Fixed { p_idx: ix(o)?, position: Point2::new(0.0, 0.0) });
        }
        // Aristas usadas: quedan donde las dejó la proyección (con su radio)
        for u in &self.uses {
            let Ok(e) = self.entity(u.entity) else { continue };
            let mut fixed: Vec<usize> = e.geometry.point_ids().iter().filter_map(|id| index.get(id).copied()).collect();
            fixed.extend(rims.get(&u.entity));
            for i in fixed {
                let p = &sys.points[i];
                let at = Point2::new(p.x(), p.y());
                sys.add_constraint(Constraint::Fixed { p_idx: i, position: at });
            }
        }
        // Textos rígidos: cada punto guarda su distancia al ancla (la de ahora)
        for t in &self.texts {
            let Some(&a) = index.get(&t.anchor) else { continue };
            for q in &t.points {
                let Some(&i) = index.get(q) else { continue };
                if i == a {
                    continue;
                }
                let (pq, pa) = (&self.points[i], &self.points[a]);
                sys.add_constraint(Constraint::HorizontalDist { p1_idx: i, p2_idx: a, distance: pq.x - pa.x });
                sys.add_constraint(Constraint::VerticalDist { p1_idx: i, p2_idx: a, distance: pq.y - pa.y });
            }
        }
        // Qué restricción de alto nivel generó cada ecuación del solver
        let implicit = sys.constraints.len();
        let mut origin: Vec<usize> = Vec::new();
        for (ci, c) in self.constraints.iter().enumerate() {
            for low in self.lower(c, &ix, &rims, &sys.points)? {
                sys.add_constraint(low);
                origin.push(ci);
            }
        }

        Ok(Built { sys, index, rims, implicit, origin })
    }

    /// Copia al sketch los puntos resueltos (y los radios de los círculos).
    fn write_back(&mut self, index: &HashMap<u32, usize>, rims: &HashMap<u32, usize>, points: &[Point2]) {
        for (p, q) in self.points.iter_mut().zip(points) {
            p.x = q.x();
            p.y = q.y();
        }
        for e in &mut self.entities {
            if let Geometry::Circle { center, radius } = &mut e.geometry
                && let (Some(&rim), Some(&c)) = (rims.get(&e.id), index.get(center))
            {
                *radius = (points[rim].co - points[c].co).norm();
            }
        }
        // El origen queda exacto (el solver lo deja a 1e-13)
        if let Some(o) = self.points.iter_mut().find(|p| Some(p.id) == self.origin) {
            (o.x, o.y) = (0.0, 0.0);
        }
    }

    /// Resuelve sin diagnóstico; deja la geometría solo si cumple todo.
    pub(crate) fn solve_quiet(&mut self) -> bool {
        let Ok(Built { mut sys, index, rims, .. }) = self.build() else { return false };
        match cad_solver::solve(&mut sys, &cad_solver::SolverParams::default()) {
            Ok(r) if r.residual < 1e-6 && matches!(r.status, SolveStatus::Converged | SolveStatus::UnderConstrained) => {
                self.write_back(&index, &rims, &r.points);
                true
            }
            _ => false,
        }
    }

    /// Cambio grande de una cota (más de un 25 %, o de 15°): se llega en pasos,
    /// cada uno desde el anterior, para que la geometría no se dé vuelta ni
    /// salte a otra solución lejos de la que había.
    fn ease_large_changes(&mut self) {
        let size = self.extent().max(1e-3);
        let mut moves: Vec<(usize, f64, f64, bool)> = Vec::new();
        let mut steps = 1.0f64;
        for (i, c) in self.constraints.iter().enumerate() {
            if c.is_reference() {
                continue;
            }
            let (Some(to), Some(from)) = (c.value(), self.measure(c)) else { continue };
            let angle = matches!(c, SketchConstraint::Angle { .. });
            let n = if angle {
                (wrap_degrees(to - from).abs() / 15.0).ceil()
            } else if from.abs() > 1e-9 && to * from > 0.0 {
                ((to / from).ln().abs() / 1.25f64.ln()).ceil()
            } else {
                ((to - from).abs() / (0.1 * size)).ceil()
            };
            if n > 1.0 && n.is_finite() {
                moves.push((i, from, to, angle));
                steps = steps.max(n);
            }
        }
        if moves.is_empty() {
            return;
        }
        let steps = steps.min(16.0) as usize;
        let at = |&(_, from, to, angle): &(usize, f64, f64, bool), t: f64| {
            if angle {
                wrap_degrees(from + t * wrap_degrees(to - from))
            } else if from.abs() > 1e-9 && to * from > 0.0 {
                from * (to / from).powf(t)
            } else {
                from + t * (to - from)
            }
        };
        for k in 1..steps {
            let t = k as f64 / steps as f64;
            for m in &moves {
                self.constraints[m.0].set_value(at(m, t));
            }
            if !self.solve_quiet() {
                break;
            }
        }
        for m in &moves {
            self.constraints[m.0].set_value(m.2);
        }
    }

    /// Lado mayor de la caja de los puntos.
    pub(crate) fn extent(&self) -> f64 {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &self.points {
            lo = [lo[0].min(p.x), lo[1].min(p.y)];
            hi = [hi[0].max(p.x), hi[1].max(p.y)];
        }
        if lo[0] > hi[0] { 0.0 } else { (hi[0] - lo[0]).max(hi[1] - lo[1]) }
    }

    fn solve_system(&mut self, drag: Option<(u32, P2)>) -> Result<SolveReport, SketchError> {
        if drag.is_none() {
            self.ease_large_changes();
        }
        let Built { mut sys, index, rims, implicit, origin } = self.build()?;
        let ix = |id: u32| index.get(&id).copied().ok_or(SketchError::NoPoint(id));
        let start = sys.points.clone();

        let result = match drag {
            Some((p, t)) => cad_solver::solve_drag(&mut sys, ix(p)?, Point2::new(t[0], t[1])),
            None => cad_solver::solve(&mut sys, &cad_solver::SolverParams::default()),
        };
        let (status, dof, residual) = match &result {
            Ok(r) => {
                let pts = r.points.clone();
                self.write_back(&index, &rims, &pts);
                (r.status, r.dof, r.residual)
            }
            Err(_) => (SolveStatus::NotConverged, 0, f64::INFINITY),
        };

        let mut conflicting = Vec::new();
        let mut free_points = Vec::new();
        let diag = cad_solver::diagnose(&sys);
        if status != SolveStatus::Converged || residual > 1e-6 {
            for &i in diag.minimal_conflict_set.iter().chain(&diag.conflicting) {
                if i >= implicit {
                    let ci = origin[i - implicit];
                    if !conflicting.contains(&ci) {
                        conflicting.push(ci);
                    }
                }
            }
        }
        // Consistente pero con una cota que no agrega nada: también sobra
        // (Onshape la marca igual); la más nueva es la que se ofrece quitar
        let mut redundant_dim = false;
        if conflicting.is_empty()
            && let Some(ci) = diag
                .redundant
                .iter()
                .filter(|&&i| i >= implicit)
                .map(|&i| origin[i - implicit])
                .filter(|&ci| self.constraints[ci].value().is_some() && !self.constraints[ci].is_reference())
                .max()
        {
            conflicting.push(ci);
            redundant_dim = true;
        }
        // Resolución parcial: sin las que chocan, lo demás se cumple entero (en
        // vez de un término medio que no cumple ninguna); las que chocan
        // quedan marcadas
        let mut partial = false;
        if !conflicting.is_empty() && !redundant_dim {
            let culprits: Vec<usize> = {
                let min: Vec<usize> = diag.minimal_conflict_set.iter().filter(|&&i| i >= implicit).map(|&i| origin[i - implicit]).collect();
                if min.is_empty() { conflicting.clone() } else { min }
            };
            let mut part = ConstraintSystem::new();
            part.points = start;
            for (i, c) in sys.constraints.iter().enumerate() {
                if i < implicit || !culprits.contains(&origin[i - implicit]) {
                    part.add_constraint(c.clone());
                }
            }
            let r = match drag {
                Some((p, t)) => cad_solver::solve_drag(&mut part, ix(p)?, Point2::new(t[0], t[1])),
                None => cad_solver::solve(&mut part, &cad_solver::SolverParams::default()),
            };
            if let Ok(r) = r
                && r.residual < 1e-6
            {
                self.write_back(&index, &rims, &r.points);
                partial = true;
            }
        }
        let free: std::collections::HashSet<usize> = diag.dof_per_point.iter().filter(|(_, d)| *d > 0).map(|(pi, _)| *pi).collect();
        for &pi in &free {
            if pi < self.points.len() {
                free_points.push(self.points[pi].id);
            }
        }
        free_points.sort_unstable();
        let mut free_entities = Vec::new();
        let (mut entity_dof, mut free_radius) = (Vec::new(), Vec::new());
        for e in &self.entities {
            let mut idx: Vec<usize> = e.geometry.point_ids().iter().filter_map(|id| index.get(id).copied()).collect();
            idx.extend(rims.get(&e.id));
            if idx.iter().any(|i| free.contains(i)) {
                free_entities.push(e.id);
                entity_dof.push((e.id, cad_solver::points_dof(&diag.nullspace, &idx)));
                if let (Some(&rim), Geometry::Circle { center, .. }) = (rims.get(&e.id), &e.geometry)
                    && let Some(&c) = index.get(center)
                    && cad_solver::points_dof(&diag.nullspace, &[c, rim]) > cad_solver::points_dof(&diag.nullspace, &[c])
                {
                    free_radius.push(e.id);
                }
            }
        }
        let free_dirs: Vec<(u32, [f64; 2])> =
            diag.free_dirs.iter().filter(|(pi, _)| *pi < self.points.len()).map(|&(pi, d)| (self.points[pi].id, d)).collect();
        // Las cotas de referencia muestran lo que mide el sketch resuelto
        for i in 0..self.constraints.len() {
            if self.constraints[i].is_reference()
                && let Some(v) = self.measure(&self.constraints[i])
            {
                self.constraints[i].set_value(v);
            }
        }
        let status = if redundant_dim || residual > 1e-6 || matches!(status, SolveStatus::OverConstrained) && residual > 1e-9 {
            SketchStatus::OverConstrained
        } else {
            match status {
                SolveStatus::NotConverged => SketchStatus::Failed,
                _ if dof > 0 => SketchStatus::UnderConstrained,
                _ => SketchStatus::WellConstrained,
            }
        };
        Ok(SolveReport { status, dof, residual, conflicting, free_points, free_entities, free_dirs, entity_dof, free_radius, partial })
    }

    /// Lo que mide una cota en la geometría actual.
    pub fn measure(&self, c: &SketchConstraint) -> Option<f64> {
        use SketchConstraint as S;
        let p = |id: u32| self.point(id).ok();
        let dir = |id: u32| -> Option<P2> {
            let (a, b) = self.line_points(id).ok()?;
            let (a, b) = (p(a)?, p(b)?);
            Some([b[0] - a[0], b[1] - a[1]])
        };
        Some(match *c {
            S::Distance { a, b, .. } => dist2(p(a)?, p(b)?),
            S::HorizontalDistance { a, b, .. } => p(b)?[0] - p(a)?[0],
            S::VerticalDistance { a, b, .. } => p(b)?[1] - p(a)?[1],
            S::Length { line, .. } => {
                let d = dir(line)?;
                d[0].hypot(d[1])
            }
            S::Radius { entity, .. } => self.radius(entity).ok()?,
            S::Diameter { entity, .. } => 2.0 * self.radius(entity).ok()?,
            S::Angle { a, b, supplementary, .. } => {
                let (d1, mut d2) = (dir(a)?, dir(b)?);
                if supplementary {
                    d2 = [-d2[0], -d2[1]];
                }
                (d1[0] * d2[1] - d1[1] * d2[0]).atan2(d1[0] * d2[0] + d1[1] * d2[1]).to_degrees()
            }
            S::PointLineDistance { point, line, .. } | S::AxisDiameter { point, line, .. } => {
                let (a, _) = self.line_points(line).ok()?;
                let (d, a, q) = (dir(line)?, p(a)?, p(point)?);
                let l = d[0].hypot(d[1]);
                let dist = if l < 1e-15 { dist2(a, q) } else { (d[0] * (q[1] - a[1]) - d[1] * (q[0] - a[0])).abs() / l };
                if matches!(c, S::AxisDiameter { .. }) { 2.0 * dist } else { dist }
            }
            S::ArcLength { arc, .. } => {
                let Geometry::Arc { center, start, end } = self.entity(arc).ok()?.geometry else { return None };
                let (c, a, b) = (p(center)?, p(start)?, p(end)?);
                let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
                let mut sweep = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
                if sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                u[0].hypot(u[1]) * sweep
            }
            S::CurveLength { ref entities, .. } => entities.iter().map(|&e| self.curve_length(e)).sum::<Result<f64, _>>().ok()?,
            S::CircleDistance { a, b, max, .. } => self.circle_gap(a, b, max).ok()?.0,
            _ => return None,
        })
    }

    /// Largo de una línea, un arco o un círculo.
    fn curve_length(&self, id: u32) -> Result<f64, SketchError> {
        match self.entity(id)?.geometry {
            Geometry::Line { start, end } => Ok(dist2(self.point(start)?, self.point(end)?)),
            Geometry::Arc { center, start, end } => {
                let (c, a, b) = (self.point(center)?, self.point(start)?, self.point(end)?);
                let (u, v) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
                let mut sweep = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
                if sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                Ok(u[0].hypot(u[1]) * sweep)
            }
            Geometry::Circle { radius, .. } => Ok(std::f64::consts::TAU * radius),
            // Curvas sin fórmula: por muestras
            Geometry::BSpline { .. } | Geometry::EllipseArc { .. } | Geometry::Ellipse { .. } | Geometry::Spline { .. } => {
                let pts = crate::regions::sample_entity(self, id)?;
                Ok(pts.windows(2).map(|w| dist2(w[0], w[1])).sum())
            }
            _ => Err(SketchError::WrongKind(id, "una curva")),
        }
    }

    /// Qué es un id en una cota con círculos: punto, línea o círculo/arco.
    fn dim_target(&self, id: u32) -> Result<Target, SketchError> {
        let Ok(e) = self.entity(id) else {
            self.point(id)?;
            return Ok(Target::Point(id));
        };
        match e.geometry {
            Geometry::Line { start, end } => Ok(Target::Line(start, end)),
            Geometry::Circle { center, .. } | Geometry::Arc { center, .. } => Ok(Target::Round(id, center)),
            Geometry::Point { point } => Ok(Target::Point(point)),
            _ => Err(SketchError::WrongKind(id, "un punto, una línea, un círculo o un arco")),
        }
    }

    /// Distancia mínima o máxima con círculos en la geometría actual: valor,
    /// signos de (distancia base, radio de a, radio de b) y los dos extremos
    /// en el orden del solver (`a` nunca es la línea).
    fn circle_gap(&self, a: u32, b: u32, max: bool) -> Result<(f64, [f64; 3], Target, Target), SketchError> {
        let (mut ta, mut tb) = (self.dim_target(a)?, self.dim_target(b)?);
        if matches!(ta, Target::Line(..)) {
            std::mem::swap(&mut ta, &mut tb);
        }
        if matches!(ta, Target::Line(..)) || !(matches!(ta, Target::Round(..)) || matches!(tb, Target::Round(..))) {
            return Err(SketchError::Unsupported("la distancia mínima o máxima necesita un círculo o arco".into()));
        }
        let at = |t: Target| match t {
            Target::Point(p) | Target::Round(_, p) | Target::Line(p, _) => self.point(p),
        };
        let pa = at(ta)?;
        let base = match tb {
            Target::Line(s, e) => {
                let (s, e) = (self.point(s)?, self.point(e)?);
                let d = [e[0] - s[0], e[1] - s[1]];
                let l = d[0].hypot(d[1]);
                if l < 1e-15 { dist2(pa, s) } else { (d[0] * (pa[1] - s[1]) - d[1] * (pa[0] - s[0])).abs() / l }
            }
            _ => dist2(pa, at(tb)?),
        };
        let r = |t: Target| match t {
            Target::Round(e, _) => self.radius(e),
            _ => Ok(0.0),
        };
        let (ra, rb) = (r(ta)?, r(tb)?);
        let both = matches!((ta, tb), (Target::Round(..), Target::Round(..)));
        let signs = if max {
            [1.0, 1.0, 1.0]
        } else if both && base < (ra - rb).abs() {
            // Uno adentro del otro
            if ra >= rb { [-1.0, 1.0, -1.0] } else { [-1.0, -1.0, 1.0] }
        } else if both || base >= ra + rb {
            [1.0, -1.0, -1.0]
        } else {
            // Punto o línea adentro del círculo
            [-1.0, 1.0, 1.0]
        };
        Ok((signs[0] * base + signs[1] * ra + signs[2] * rb, signs, ta, tb))
    }

    /// Traduce una restricción a ecuaciones del solver.
    pub(crate) fn lower(
        &self,
        c: &SketchConstraint,
        ix: &dyn Fn(u32) -> Result<usize, SketchError>,
        rims: &HashMap<u32, usize>,
        at: &[Point2],
    ) -> Result<Vec<Constraint>, SketchError> {
        use SketchConstraint as S;
        if c.is_reference() {
            return Ok(vec![]);
        }
        // Centro y punto de borde (radio variable) de un círculo o arco
        let round = |id: u32| -> Result<(usize, usize), SketchError> {
            match (&self.entity(id)?.geometry, rims.get(&id)) {
                (Geometry::Circle { center, .. } | Geometry::Arc { center, .. }, Some(&rim)) => Ok((ix(*center)?, rim)),
                _ => Err(SketchError::WrongKind(id, "un círculo o arco")),
            }
        };
        let line = |id| -> Result<(usize, usize), SketchError> {
            let (a, b) = self.line_points(id)?;
            Ok((ix(a)?, ix(b)?))
        };
        // B-spline con sus polos como índices del solver
        let bspline_ref = |id: u32| -> Result<BSplineRef, SketchError> {
            match &self.entity(id)?.geometry {
                Geometry::BSpline { poles, degree, closed, weights, knots } => Ok(BSplineRef {
                    poles: poles.iter().map(|&p| ix(p)).collect::<Result<_, _>>()?,
                    weights: weights.clone(),
                    knots: knots.clone(),
                    degree: *degree as usize,
                    closed: *closed,
                }),
                _ => Err(SketchError::WrongKind(id, "una B-spline")),
            }
        };
        // Extremo común de dos curvas abiertas
        let shared_end = |a: u32, b: u32| -> Result<u32, SketchError> {
            let (ea, eb) = (self.entity(a)?.geometry.ends(), self.entity(b)?.geometry.ends());
            let (Some((a0, a1)), Some((b0, b1))) = (ea, eb) else {
                return Err(SketchError::Unsupported("hace falta que las dos curvas sean abiertas".into()));
            };
            [a0, a1].into_iter().find(|p| *p == b0 || *p == b1).ok_or_else(|| SketchError::Unsupported("las curvas no tienen un extremo en común".into()))
        };
        // Tangencia en el extremo común cuando una de las dos es una B-spline:
        // la tangente en la punta va hacia el polo vecino
        let bspline_tangent = |a: u32, b: u32| -> Result<Option<Vec<Constraint>>, SketchError> {
            let (ga, gb) = (&self.entity(a)?.geometry, &self.entity(b)?.geometry);
            if !matches!(ga, Geometry::BSpline { .. }) && !matches!(gb, Geometry::BSpline { .. }) {
                return Ok(None);
            }
            let shared = shared_end(a, b)?;
            // (punta, polo vecino) de una B-spline abierta en el extremo común
            let tip = |g: &Geometry| -> Option<(usize, usize)> {
                let Geometry::BSpline { poles, .. } = g else { return None };
                let n = poles.len();
                let (p, q) = if poles[0] == shared { (poles[0], poles[1]) } else { (poles[n - 1], poles[n - 2]) };
                Some((ix(p).ok()?, ix(q).ok()?))
            };
            let s = ix(shared)?;
            let other = |g: &Geometry, (tp, nb): (usize, usize)| -> Result<Vec<Constraint>, SketchError> {
                Ok(match g {
                    Geometry::Line { start, end } => vec![Constraint::Parallel { l1_p1: ix(*start)?, l1_p2: ix(*end)?, l2_p1: tp, l2_p2: nb }],
                    Geometry::Arc { center, .. } => vec![Constraint::Perpendicular { l1_p1: ix(*center)?, l1_p2: s, l2_p1: tp, l2_p2: nb }],
                    Geometry::BSpline { .. } => {
                        let (_, nb2) = tip(g).ok_or(SketchError::NoEntity(b))?;
                        // Los dos polos vecinos alineados con la punta
                        vec![Constraint::PointOnLine { p_idx: nb2, line_p1: nb, line_p2: tp }]
                    }
                    _ => return Err(SketchError::Unsupported("tangencia de una B-spline con esa curva".into())),
                })
            };
            Ok(Some(match (tip(ga), tip(gb)) {
                (Some(t), _) => other(gb, t)?,
                (None, Some(t)) => other(ga, t)?,
                _ => unreachable!(),
            }))
        };
        // Punto (índice del solver) sobre cualquier curva
        let on_curve = |q: usize, id: u32| -> Result<Vec<Constraint>, SketchError> {
            Ok(match &self.entity(id)?.geometry {
                Geometry::Line { start, end } => vec![Constraint::PointOnLine { p_idx: q, line_p1: ix(*start)?, line_p2: ix(*end)? }],
                Geometry::Circle { .. } | Geometry::Arc { .. } => {
                    let (c, rim) = round(id)?;
                    vec![Constraint::EqualLength { l1_p1: c, l1_p2: q, l2_p1: c, l2_p2: rim }]
                }
                Geometry::Ellipse { center, major, minor } => {
                    vec![Constraint::PointOnEllipse { p_idx: q, center: ix(*center)?, major: ix(*major)?, minor: ix(*minor)? }]
                }
                Geometry::Spline { points, closed, start_handle, end_handle, handles } => {
                    let n = points.len();
                    let mut hs: Vec<(usize, usize)> = Vec::new();
                    if !*closed {
                        hs.extend(start_handle.map(ix).transpose()?.map(|h| (0, h)));
                        hs.extend(end_handle.map(ix).transpose()?.map(|h| (n.saturating_sub(1), h)));
                    }
                    for [i, h] in handles {
                        hs.push((*i as usize, ix(*h)?));
                    }
                    vec![Constraint::PointOnCurveSpline { p_idx: q, points: points.iter().map(|&p| ix(p)).collect::<Result<_, _>>()?, closed: *closed, handles: hs }]
                }
                Geometry::EllipseArc { center, major, minor, .. } => {
                    vec![Constraint::PointOnEllipse { p_idx: q, center: ix(*center)?, major: ix(*major)?, minor: ix(*minor)? }]
                }
                Geometry::BSpline { .. } => vec![Constraint::PointOnBSpline { p_idx: q, curve: bspline_ref(id)? }],
                Geometry::Point { .. } => return Err(SketchError::WrongKind(id, "una curva")),
            })
        };
        Ok(match *c {
            S::Coincident { a, b } => vec![Constraint::Coincident { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::Fixed { point, x, y } | S::Pierce { point, at: [x, y], .. } => vec![Constraint::Fixed { p_idx: ix(point)?, position: Point2::new(x, y) }],
            S::Horizontal { line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Horizontal { p1_idx: a, p2_idx: b }]
            }
            S::Vertical { line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Vertical { p1_idx: a, p2_idx: b }]
            }
            S::HorizontalPoints { a, b } => vec![Constraint::Horizontal { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::VerticalPoints { a, b } => vec![Constraint::Vertical { p1_idx: ix(a)?, p2_idx: ix(b)? }],
            S::Parallel { a, b } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![Constraint::Parallel { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2 }]
            }
            S::Collinear { a, b } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![
                    Constraint::Parallel { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2 },
                    Constraint::PointOnLine { p_idx: b1, line_p1: a1, line_p2: a2 },
                ]
            }
            S::Perpendicular { a, b } => {
                let ((a1, a2), (b1, b2)) = (line(a)?, line(b)?);
                vec![Constraint::Perpendicular { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2 }]
            }
            S::Angle { a, b, degrees, supplementary, .. } => {
                let ((a1, a2), (mut b1, mut b2)) = (line(a)?, line(b)?);
                if supplementary {
                    std::mem::swap(&mut b1, &mut b2);
                }
                vec![Constraint::Angle { l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2, angle_rad: degrees.to_radians() }]
            }
            S::Equal { a, b } => match (&self.entity(a)?.geometry, &self.entity(b)?.geometry) {
                (Geometry::Line { start: a1, end: a2 }, Geometry::Line { start: b1, end: b2 }) => {
                    vec![Constraint::EqualLength { l1_p1: ix(*a1)?, l1_p2: ix(*a2)?, l2_p1: ix(*b1)?, l2_p2: ix(*b2)? }]
                }
                (Geometry::Circle { .. } | Geometry::Arc { .. }, Geometry::Circle { .. } | Geometry::Arc { .. }) => {
                    let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                    vec![Constraint::EqualLength { l1_p1: c1, l1_p2: r1, l2_p1: c2, l2_p2: r2 }]
                }
                _ => return Err(SketchError::Unsupported("igualdad entre entidades de distinto tipo".into())),
            },
            S::Tangent { a, b } if bspline_tangent(a, b)?.is_some() => bspline_tangent(a, b)?.unwrap_or_default(),
            S::Curvature { a, b } => {
                let shared = shared_end(a, b)?;
                let end_of = |id: u32| -> Result<CurveEnd, SketchError> {
                    Ok(match &self.entity(id)?.geometry {
                        Geometry::Line { .. } => CurveEnd::Line,
                        Geometry::Arc { center, start, .. } => CurveEnd::Arc { center: ix(*center)?, start: ix(*start)?, at_start: *start == shared },
                        Geometry::BSpline { poles, .. } => CurveEnd::BSpline { curve: bspline_ref(id)?, at_start: poles.first() == Some(&shared) },
                        _ => return Err(SketchError::WrongKind(id, "una línea, un arco o una B-spline")),
                    })
                };
                let mut out = bspline_tangent(a, b)?.ok_or_else(|| SketchError::Unsupported("la curvatura igual necesita una B-spline".into()))?;
                out.push(Constraint::EqualCurvature { a: end_of(a)?, b: end_of(b)? });
                out
            }
            S::Tangent { a, b } => {
                // Dos arcos que comparten un extremo: los centros quedan alineados
                // con el punto de contacto (tangencia en ese punto)
                if let (Geometry::Arc { center: c1, start: s1, end: e1 }, Geometry::Arc { center: c2, start: s2, end: e2 }) =
                    (&self.entity(a)?.geometry, &self.entity(b)?.geometry)
                {
                    let shared = [s1, e1].into_iter().find(|p| *p == s2 || *p == e2).ok_or_else(|| {
                        SketchError::Unsupported("arcos tangentes sin un extremo en común".into())
                    })?;
                    return Ok(vec![Constraint::PointOnLine { p_idx: ix(*c2)?, line_p1: ix(*c1)?, line_p2: ix(*shared)? }]);
                }
                let (l, other) = match (&self.entity(a)?.geometry, &self.entity(b)?.geometry) {
                    (Geometry::Line { .. }, _) => (a, b),
                    (_, Geometry::Line { .. }) => (b, a),
                    _ => {
                        // Dos curvas: por fuera o por dentro, según cómo están ahora
                        let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                        let len = |p: usize, q: usize| (at[q].co - at[p].co).norm();
                        let (d, ra, rb) = (len(c1, c2), len(c1, r1), len(c2, r2));
                        let internal = (d - (ra - rb).abs()).abs() < (d - (ra + rb)).abs();
                        return Ok(vec![Constraint::TangentCircles { c1, rim1: r1, c2, rim2: r2, internal }]);
                    }
                };
                let (l1, l2) = self.line_points(l)?;
                match self.entity(other)?.geometry {
                    Geometry::Circle { .. } => {
                        let (center_idx, rim_idx) = round(other)?;
                        vec![Constraint::TangentLineCircleVar { line_p1: ix(l1)?, line_p2: ix(l2)?, center_idx, rim_idx }]
                    }
                    Geometry::Arc { center, start, end } => {
                        // Si comparten extremo, tangencia exacta: la línea es
                        // perpendicular al radio en ese punto (sin fijar el radio).
                        let shared = [start, end].into_iter().find(|p| *p == l1 || *p == l2);
                        match shared {
                            Some(p) => vec![Constraint::Perpendicular {
                                l1_p1: ix(l1)?,
                                l1_p2: ix(l2)?,
                                l2_p1: ix(center)?,
                                l2_p2: ix(p)?,
                            }],
                            None => vec![Constraint::TangentLineCircleVar {
                                line_p1: ix(l1)?,
                                line_p2: ix(l2)?,
                                center_idx: ix(center)?,
                                rim_idx: ix(start)?,
                            }],
                        }
                    }
                    _ => return Err(SketchError::WrongKind(other, "un círculo o arco")),
                }
            }
            S::PointOnLine { point, line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::PointOnLine { p_idx: ix(point)?, line_p1: a, line_p2: b }]
            }
            S::PointOnCircle { point, circle } => {
                let (c, rim) = round(circle)?;
                vec![Constraint::EqualLength { l1_p1: c, l1_p2: ix(point)?, l2_p1: c, l2_p2: rim }]
            }
            S::Concentric { a, b } => {
                let ((c1, _), (c2, _)) = (round(a)?, round(b)?);
                vec![Constraint::Coincident { p1_idx: c1, p2_idx: c2 }]
            }
            S::Midpoint { point, line: l } => {
                let (a, b) = line(l)?;
                vec![Constraint::Midpoint { p_idx: ix(point)?, line_p1: a, line_p2: b }]
            }
            S::Symmetric { a, b, line: l } => {
                let (l1, l2) = line(l)?;
                vec![Constraint::Symmetric { p1_idx: ix(a)?, p2_idx: ix(b)?, line_p1: l1, line_p2: l2 }]
            }
            S::EqualOffset { a1, a2, b1, b2 } => {
                vec![Constraint::EqualVector { a1: ix(a1)?, a2: ix(a2)?, b1: ix(b1)?, b2: ix(b2)? }]
            }
            S::EqualRotation { center, a1, a2, b1, b2 } => vec![Constraint::EqualRotation {
                center: ix(center)?,
                a1: ix(a1)?,
                a2: ix(a2)?,
                b1: ix(b1)?,
                b2: ix(b2)?,
            }],
            S::Distance { a, b, value, .. } => vec![Constraint::Distance { p1_idx: ix(a)?, p2_idx: ix(b)?, distance: value }],
            S::HorizontalDistance { a, b, value, .. } => {
                vec![Constraint::HorizontalDist { p1_idx: ix(b)?, p2_idx: ix(a)?, distance: value }]
            }
            S::VerticalDistance { a, b, value, .. } => {
                vec![Constraint::VerticalDist { p1_idx: ix(b)?, p2_idx: ix(a)?, distance: value }]
            }
            S::Length { line: l, value, .. } => {
                let (a, b) = line(l)?;
                vec![Constraint::Distance { p1_idx: a, p2_idx: b, distance: value }]
            }
            S::Radius { entity, value, .. } | S::Diameter { entity, value, .. } => {
                let r = if matches!(c, S::Diameter { .. }) { value / 2.0 } else { value };
                let (center, rim) = round(entity)?;
                vec![Constraint::Distance { p1_idx: center, p2_idx: rim, distance: r }]
            }
            S::PointLineDistance { point, line: l, value, .. } | S::AxisDiameter { point, line: l, value, .. } => {
                let d = if matches!(c, S::AxisDiameter { .. }) { value / 2.0 } else { value };
                let (a, b) = line(l)?;
                vec![Constraint::DistancePointLine { p_idx: ix(point)?, line_p1: a, line_p2: b, distance: d }]
            }
            S::ArcLength { arc, value, .. } => match self.entity(arc)?.geometry {
                Geometry::Arc { center, start, end } => {
                    vec![Constraint::ArcLength { center_idx: ix(center)?, start_idx: ix(start)?, end_idx: ix(end)?, length: value }]
                }
                _ => return Err(SketchError::WrongKind(arc, "un arco")),
            },
            S::CurveLength { ref entities, value, .. } => {
                let mut parts = Vec::new();
                for &e in entities {
                    parts.push(match self.entity(e)?.geometry {
                        Geometry::Line { start, end } => CurvePart::Segment { a: ix(start)?, b: ix(end)? },
                        Geometry::Arc { center, start, end } => CurvePart::Arc { center: ix(center)?, start: ix(start)?, end: ix(end)? },
                        Geometry::Circle { .. } => {
                            let (center, rim) = round(e)?;
                            CurvePart::Circle { center, rim }
                        }
                        _ => return Err(SketchError::WrongKind(e, "una línea, un arco o un círculo")),
                    });
                }
                vec![Constraint::CurveLength { parts, length: value }]
            }
            S::CircleDistance { a, b, max, value, .. } => {
                let (_, signs, ta, tb) = self.circle_gap(a, b, max)?;
                let (pa, a_rim) = match ta {
                    Target::Round(e, c) => (ix(c)?, Some(round(e)?.1)),
                    Target::Point(p) | Target::Line(p, _) => (ix(p)?, None),
                };
                let (pb, b_end, b_rim) = match tb {
                    Target::Point(p) => (ix(p)?, None, None),
                    Target::Line(s, e) => (ix(s)?, Some(ix(e)?), None),
                    Target::Round(e, c) => (ix(c)?, None, Some(round(e)?.1)),
                };
                vec![Constraint::RimDistance { a: pa, a_rim, b: pb, b_end, b_rim, signs, distance: value }]
            }
            S::Coradial { a, b } => {
                let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                vec![
                    Constraint::Coincident { p1_idx: c1, p2_idx: c2 },
                    Constraint::EqualLength { l1_p1: c1, l1_p2: r1, l2_p1: c2, l2_p2: r2 },
                ]
            }
            S::PointOnCurve { point, curve } => on_curve(ix(point)?, curve)?,
            S::Intersection { point, a, b } => {
                let q = ix(point)?;
                let mut out = on_curve(q, a)?;
                out.extend(on_curve(q, b)?);
                out
            }
            S::Lock { entity } => {
                let e = self.entity(entity)?;
                let mut idx = e.geometry.point_ids().into_iter().map(ix).collect::<Result<Vec<_>, _>>()?;
                idx.extend(rims.get(&entity));
                idx.sort_unstable();
                idx.dedup();
                idx.into_iter().map(|i| Constraint::Fixed { p_idx: i, position: at[i] }).collect()
            }
            S::SymmetricEntities { a, b, line: l } => {
                let (l1, l2) = line(l)?;
                let (o, d) = (at[l1].co, at[l2].co - at[l1].co);
                // Distancia entre el reflejo de i y j: para emparejar extremos como están
                let near = |i: usize, j: usize| {
                    let v = at[i].co - o;
                    let foot = o + d * (v.dot(&d) / d.norm_squared().max(1e-30));
                    (foot * 2.0 - at[i].co - at[j].co).norm()
                };
                let sym = |i: usize, j: usize| Constraint::Symmetric { p1_idx: i, p2_idx: j, line_p1: l1, line_p2: l2 };
                let ids = |v: &[u32]| v.iter().map(|&p| ix(p)).collect::<Result<Vec<_>, _>>();
                match (&self.entity(a)?.geometry, &self.entity(b)?.geometry) {
                    (Geometry::Point { point: p }, Geometry::Point { point: q }) => vec![sym(ix(*p)?, ix(*q)?)],
                    (Geometry::Line { start: a1, end: a2 }, Geometry::Line { start: b1, end: b2 }) => {
                        let (a1, a2, b1, b2) = (ix(*a1)?, ix(*a2)?, ix(*b1)?, ix(*b2)?);
                        if near(a1, b1) + near(a2, b2) <= near(a1, b2) + near(a2, b1) {
                            vec![sym(a1, b1), sym(a2, b2)]
                        } else {
                            vec![sym(a1, b2), sym(a2, b1)]
                        }
                    }
                    (Geometry::Circle { .. }, Geometry::Circle { .. }) => {
                        let ((c1, r1), (c2, r2)) = (round(a)?, round(b)?);
                        vec![sym(c1, c2), Constraint::EqualLength { l1_p1: c1, l1_p2: r1, l2_p1: c2, l2_p2: r2 }]
                    }
                    // El reflejo da vuelta el sentido: el inicio de uno es el final del otro
                    (Geometry::Arc { center: c1, start: s1, end: e1 }, Geometry::Arc { center: c2, start: s2, end: e2 }) => {
                        vec![sym(ix(*c1)?, ix(*c2)?), sym(ix(*s1)?, ix(*e2)?), sym(ix(*e1)?, ix(*s2)?)]
                    }
                    (Geometry::Ellipse { center: c1, major: m1, minor: n1 }, Geometry::Ellipse { center: c2, major: m2, minor: n2 }) => {
                        vec![sym(ix(*c1)?, ix(*c2)?), sym(ix(*m1)?, ix(*m2)?), sym(ix(*n1)?, ix(*n2)?)]
                    }
                    (
                        Geometry::EllipseArc { center: c1, major: m1, minor: n1, start: s1, end: e1 },
                        Geometry::EllipseArc { center: c2, major: m2, minor: n2, start: s2, end: e2 },
                    ) => vec![
                        sym(ix(*c1)?, ix(*c2)?),
                        sym(ix(*m1)?, ix(*m2)?),
                        sym(ix(*n1)?, ix(*n2)?),
                        sym(ix(*s1)?, ix(*e2)?),
                        sym(ix(*e1)?, ix(*s2)?),
                    ],
                    (Geometry::BSpline { poles: p1, .. }, Geometry::BSpline { poles: p2, .. }) if p1.len() == p2.len() => {
                        let (p1, mut p2) = (ids(p1)?, ids(p2)?);
                        let fwd: f64 = p1.iter().zip(&p2).map(|(&i, &j)| near(i, j)).sum();
                        let rev: f64 = p1.iter().zip(p2.iter().rev()).map(|(&i, &j)| near(i, j)).sum();
                        if rev < fwd {
                            p2.reverse();
                        }
                        p1.iter().zip(&p2).map(|(&i, &j)| sym(i, j)).collect()
                    }
                    (
                        Geometry::Spline { points: p1, start_handle: h1, end_handle: k1, .. },
                        Geometry::Spline { points: p2, start_handle: h2, end_handle: k2, .. },
                    ) if p1.len() == p2.len() => {
                        let (p1, mut p2) = (ids(p1)?, ids(p2)?);
                        let (mut h2, mut k2) = (*h2, *k2);
                        let fwd: f64 = p1.iter().zip(&p2).map(|(&i, &j)| near(i, j)).sum();
                        let rev: f64 = p1.iter().zip(p2.iter().rev()).map(|(&i, &j)| near(i, j)).sum();
                        if rev < fwd {
                            p2.reverse();
                            std::mem::swap(&mut h2, &mut k2);
                        }
                        let mut out: Vec<Constraint> = p1.iter().zip(&p2).map(|(&i, &j)| sym(i, j)).collect();
                        for (h, k) in [(*h1, h2), (*k1, k2)] {
                            if let (Some(h), Some(k)) = (h, k) {
                                out.push(sym(ix(h)?, ix(k)?));
                            }
                        }
                        out
                    }
                    _ => return Err(SketchError::Unsupported("simetría entre entidades de distinto tipo (o splines con distinta cantidad de puntos)".into())),
                }
            }
        })
    }
}

/// Un extremo de una cota con círculos.
#[derive(Debug, Clone, Copy)]
enum Target {
    Point(u32),
    Line(u32, u32),
    /// Entidad (círculo o arco) y su centro
    Round(u32, u32),
}
