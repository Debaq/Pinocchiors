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
    /// Un extremo de una arista (un vértice del sólido): el final con `end`.
    EdgeEnd {
        edge: EdgeRef,
        #[serde(default)]
        end: bool,
    },
    /// Tomado del escaneo; `point` es lo último ajustado (ver [`ScanFit`]).
    Scan { fit: ScanFit, point: P3 },
    /// Por clics: el tipo sale de lo elegido (vértice, medio o centro de una
    /// arista, centro de una esfera, cruce de arista y cara, punto medio...).
    Picked { refs: Vec<RefPick> },
}

/// Dónde se hizo clic sobre el escaneo: el triángulo y el punto dentro de él
/// (baricéntricas), que siguen al escaneo si se mueve, y el punto en mm de ese
/// momento, para encontrar el lugar si la malla cambió (reparada, remallada).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanSeed {
    pub triangle: u32,
    pub bary: [f64; 3],
    pub at: P3,
    /// Huella de los triángulos del escaneo de ese momento
    pub topology: u32,
}

/// Referencia tomada del escaneo. La app la vuelve a ajustar contra el escaneo
/// antes de cada recálculo; la geometría guardada al lado (plano, eje, punto)
/// es la última ajustada y se usa si no hay escaneo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanFit {
    /// La zona plana alrededor del clic (plano ajustado, normal hacia afuera).
    Zone { seed: ScanSeed },
    /// Tres puntos sobre el escaneo.
    Points { seeds: [ScanSeed; 3] },
    /// El cilindro o agujero alrededor del clic (su eje).
    Cylinder { seed: ScanSeed },
    /// El punto del clic sobre la superficie.
    Point { seed: ScanSeed },
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
    /// Tomado del escaneo; `plane` es lo último ajustado.
    Scan { fit: ScanFit, plane: Plane },
    /// Por clics: el tipo sale de lo elegido (ver [`RefPick`]). `offset`
    /// corre el plano sobre su normal, `angle` lo gira alrededor de la línea
    /// elegida y `flip` da vuelta la normal.
    Picked {
        refs: Vec<RefPick>,
        #[serde(default)]
        offset: f64,
        #[serde(default)]
        angle: f64,
        #[serde(default)]
        flip: bool,
    },
}

/// Cómo se arma una curva de referencia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveDef {
    /// Aristas del sólido y curvas del historial encadenadas en una.
    Composite {
        #[serde(default)]
        edges: Vec<EdgeRef>,
        #[serde(default)]
        curves: Vec<FeatureId>,
    },
    /// Curvas de un sketch (vacío = todas) proyectadas sobre caras a lo largo de la normal del sketch.
    Projected {
        sketch: FeatureId,
        #[serde(default)]
        entities: Vec<u32>,
        faces: Vec<FaceRef>,
    },
    /// Donde se cruzan unas caras con otras.
    Intersection { a: Vec<FaceRef>, b: Vec<FaceRef> },
    /// Spline que pasa por los puntos (cerrada con `closed`).
    Points {
        points: Vec<PointSpec>,
        #[serde(default)]
        closed: bool,
    },
}

/// Algo del modelo elegido con un clic para armar un plano, un eje o un
/// punto de referencia; sigue al modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RefPick {
    /// Un plano base o de referencia.
    Plane { plane: PlaneSpec },
    Face { face: FaceRef },
    Edge { edge: EdgeRef },
    /// Un vértice, un punto de referencia o uno fijo.
    Point { point: PointSpec },
    /// Un eje de referencia o una línea de un sketch.
    Axis { axis: AxisSpec },
}

fn pick_deps(p: &RefPick) -> Vec<FeatureId> {
    match p {
        RefPick::Plane { plane } => plane_deps(plane),
        RefPick::Point { point } => point_deps(point),
        RefPick::Axis { axis } => axis_deps(axis),
        RefPick::Face { .. } | RefPick::Edge { .. } => vec![],
    }
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
    /// Tomado del escaneo; `origin` y `direction` son lo último ajustado.
    Scan { fit: ScanFit, origin: P3, direction: P3 },
    /// Por clics: el tipo sale de lo elegido.
    Picked { refs: Vec<RefPick> },
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
        PointSpec::Picked { refs } => refs.iter().flat_map(pick_deps).collect(),
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

/// Radio de un redondeo variable en un punto de la arista (`at`: fracción del largo, de 0 a 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FilletPoint {
    pub at: f64,
    pub radius: f64,
}

/// Una cara con su propio espesor en un vaciado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceThickness {
    pub face: FaceRef,
    pub thickness: f64,
}

/// Movimiento de [`FeatureKind::Transform`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Motion {
    /// Traslación (mm).
    Translate { vector: P3 },
    /// Giro de `angle` grados alrededor de un eje.
    Rotate { axis: AxisSpec, angle: f64 },
    /// Traslación de un punto a otro.
    Points { from: PointSpec, to: PointSpec },
    /// Lleva una cara plana sobre un plano: la normal de la cara contra la
    /// del plano (con `flip`, a favor) y la cara apoyada en él (se corre solo
    /// a lo largo de la normal). Sin piezas elegidas mueve la de la cara.
    Align {
        face: FaceRef,
        to: PlaneSpec,
        #[serde(default)]
        flip: bool,
    },
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
    /// Hasta el plano de una cara; con `offset`, se queda a esa distancia antes
    /// (negativo: la pasa).
    UpToFace {
        face: FaceRef,
        #[serde(default, skip_serializing_if = "is_zero")]
        offset: f64,
    },
    /// Hacia los dos lados del plano: `distance` hacia la normal y `second` hacia atrás.
    TwoSides { distance: f64, second: f64 },
    /// Hasta la primera cara del sólido que encuentra.
    UpToNext,
    /// Hasta un punto (un vértice): el largo es lo que avanza el punto sobre la dirección.
    UpToVertex { point: PointSpec },
    /// Hasta la piel del escaneo (aunque sea curva), dejando `offset` de
    /// separación; va hacia el lado donde está el escaneo.
    UpToScan {
        #[serde(default)]
        offset: f64,
        /// El escaneo cerrado y corrido `offset`, que pone la app antes de
        /// recalcular (como en [`ScanBody`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mesh: Option<ScanBodyMesh>,
    },
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
    /// Dirección propia (la de un eje, una arista o una línea) en vez de la
    /// normal del plano; las medidas se toman a lo largo de ella.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<AxisSpec>,
    /// Arranca a esta distancia del plano del sketch (a lo largo de su normal).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: f64,
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
    /// Hacia dónde gira `angle` desde el plano del sketch.
    #[serde(default, skip_serializing_if = "RevolveExtent::is_one")]
    pub extent: RevolveExtent,
    /// Revolución delgada: una pared de este espesor siguiendo el contorno.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thin: Option<f64>,
}

/// Cómo se reparte el giro de una revolución alrededor del plano del sketch.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RevolveExtent {
    /// Todo `angle` hacia un lado.
    #[default]
    One,
    /// Mitad de `angle` hacia cada lado.
    Symmetric,
    /// `angle` hacia un lado y `second` (grados) hacia el otro.
    TwoAngles { second: f64 },
    /// Hasta una cara (plana o curva), hacia el lado del signo de `angle`;
    /// la cara tiene que estar a menos de 350° del perfil.
    UpToFace { face: FaceRef },
}

impl RevolveExtent {
    fn is_one(&self) -> bool {
        *self == RevolveExtent::One
    }
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
    /// Tornillo métrico (ver [`crate::standard`]): `size` "M6", largo bajo la
    /// cabeza (el total en el avellanado); `modeled` lleva la rosca de verdad.
    Bolt {
        size: String,
        length: f64,
        #[serde(default)]
        head: BoltHead,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        modeled: bool,
    },
    /// Tuerca hexagonal métrica.
    Nut {
        size: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        modeled: bool,
    },
    /// Arandela plana métrica.
    Washer { size: String },
}

/// Cabeza del tornillo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoltHead {
    /// Hexagonal (ISO 4017)
    Hex,
    /// Cilíndrica con hexágono interior (ISO 4762)
    #[default]
    Socket,
    /// Avellanada con hexágono interior (ISO 10642)
    Countersunk,
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
    /// Tornillo o tuerca puesto en otra rosca: toma su medida y su eje (el
    /// marco de arriba no se usa) y gira el filete para que calce.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadLink>,
}

/// Coordinación entre roscas: la operación que la lleva toma la medida (diámetro,
/// paso, mano) de otra rosca anterior del historial y alinea su filete con el
/// de ella, para que calcen. No es una booleana: cada una sigue siendo lo que es.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThreadLink {
    /// Agujero roscado, rosca, tornillo o tuerca
    pub feature: FeatureId,
    /// Cuál de sus roscas (un agujero tiene una por centro)
    #[serde(default)]
    pub index: u32,
    /// Tornillos y tuercas: entran por la otra boca
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
    /// Tornillos y tuercas: cuánto entran desde la boca a lo largo de la
    /// rosca (negativo: quedan afuera)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
}

fn default_z() -> P3 {
    [0.0, 0.0, 1.0]
}
fn default_x() -> P3 {
    [1.0, 0.0, 0.0]
}

/// Segunda dirección de un patrón lineal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearSecond {
    pub direction: P3,
    pub count: u32,
    pub spacing: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternKind {
    /// Con `second`, también en una segunda dirección (grilla).
    Linear {
        direction: P3,
        count: u32,
        spacing: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        second: Option<LinearSecond>,
    },
    /// `angle` total; 360 reparte las copias en la vuelta completa.
    Circular { axis: AxisSpec, count: u32, angle: f64 },
    /// Copias repartidas a lo largo de un camino (de punta a punta), trasladadas.
    Curve { path: SweepPath, count: u32 },
    /// Copias trasladadas según una tabla de desplazamientos (mm).
    Table { offsets: Vec<P3> },
    /// Una copia en cada punto suelto de un sketch (o en los de `points`),
    /// trasladada desde el punto más cercano a la operación original (ahí no
    /// se copia).
    Points {
        sketch: FeatureId,
        #[serde(default)]
        points: Vec<u32>,
    },
    /// Copias en grilla (cuadrada o hexagonal) que llenan regiones de un
    /// sketch, a `spacing` entre centros y a `margin` como mínimo del borde.
    /// La grilla pasa por la operación original (su centro, en el plano).
    Fill {
        sketch: FeatureId,
        #[serde(default)]
        regions: RegionSelection,
        spacing: f64,
        #[serde(default)]
        hex: bool,
        #[serde(default)]
        margin: f64,
    },
}

/// El escaneo elegido como sólido: se rehace por vóxeles (cerrado, con la
/// holgura) a unos `faces` triángulos y se une, resta o interseca como
/// cualquier herramienta. Sigue al escaneo: si se alinea, repara o cambia,
/// se rehace. Con espesor es una carcasa que copia la piel; con región, solo
/// el parche bajo un sketch (férula, almohadilla, tapa que calza).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanBody {
    /// Holgura (mm): positiva agranda el escaneo (un encaje que no apriete),
    /// negativa lo achica.
    #[serde(default)]
    pub offset: f64,
    /// Triángulos del cuerpo: más es más fiel y hace más lentas las booleanas.
    #[serde(default = "scan_body_faces")]
    pub faces: usize,
    #[serde(default)]
    pub op: BodyOp,
    /// Distinto de cero: carcasa de este espesor (mm) desde la piel corrida
    /// la holgura; positivo hacia afuera, negativo hacia adentro.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub thickness: f64,
    /// Solo lo que cae bajo estas regiones de un sketch, mirando desde su
    /// plano: el primer tramo del escaneo hacia cada lado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<ScanRegion>,
    /// La malla preparada que pone la app antes de cada recálculo (no se
    /// guarda: de ella solo viaja la huella, que entra en la caché).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<ScanBodyMesh>,
}

/// Regiones de un sketch proyectadas sobre el escaneo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRegion {
    pub sketch: FeatureId,
    #[serde(default)]
    pub regions: RegionSelection,
}

fn scan_body_faces() -> usize {
    20_000
}

impl Default for ScanBody {
    fn default() -> Self {
        Self { offset: 0.0, faces: scan_body_faces(), op: BodyOp::New, thickness: 0.0, region: None, mesh: None }
    }
}

/// Llegar hasta la piel del escaneo, a `offset` de ella.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScanReach {
    #[serde(default)]
    pub offset: f64,
    /// El escaneo corrido `offset`, que pone la app (como en [`ScanBody`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<ScanBodyMesh>,
}

/// Malla cerrada del escaneo con la holgura (Z arriba, mm).
#[derive(Debug, Clone)]
pub struct ScanBodyMesh {
    pub hash: u64,
    pub data: std::sync::Arc<(Vec<P3>, Vec<[u32; 3]>)>,
    /// Por qué no se pudo preparar (la malla queda vacía)
    pub error: Option<String>,
}

impl PartialEq for ScanBodyMesh {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Serialize for ScanBodyMesh {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Como texto: JSON no guarda u64 enteros
        s.serialize_str(&format!("{:016x}", self.hash))
    }
}

impl<'de> Deserialize<'de> for ScanBodyMesh {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let hash = u64::from_str_radix(&text, 16).unwrap_or_default();
        Ok(Self { hash, data: Default::default(), error: None })
    }
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
    /// Sketch 3D: curvas en el espacio (caminos de barrido).
    Sketch3d { sketch: crate::sketch3d::Sketch3d },
    /// Sketch envuelto sobre una cara: se dibuja en el plano tangente en el
    /// centro de la cara y sus curvas se llevan a la superficie.
    SurfaceSketch { face: FaceRef, sketch: Sketch },
    Extrude(Extrude),
    Revolve(Revolve),
    Primitive(Primitive),
    /// Redondeo; con `radius2`, variable de `radius` al comienzo de cada arista a `radius2` al final.
    /// Con `points`, además pasa por esos radios en el medio (fracción del largo, radio).
    Fillet {
        edges: Vec<EdgeRef>,
        radius: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius2: Option<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<FilletPoint>,
    },
    /// Chaflán: `distance` en las dos caras, o asimétrico con `second`.
    Chamfer {
        edges: Vec<EdgeRef>,
        distance: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        second: Option<ChamferSecond>,
    },
    /// Ahuecar dejando paredes de `thickness` y abriendo `faces`; las de
    /// `overrides` con su propio espesor.
    Shell {
        faces: Vec<FaceRef>,
        thickness: f64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        overrides: Vec<FaceThickness>,
    },
    /// Saca caras y cierra el hueco extendiendo las vecinas (un agujero, un
    /// redondeo, un resalte).
    DeleteFace { faces: Vec<FaceRef> },
    /// Desmolde: inclina `faces` respecto de la normal de `neutral`.
    Draft { faces: Vec<FaceRef>, neutral: PlaneSpec, angle: f64 },
    /// Repite las herramientas de `features` (vacío = todo el cuerpo);
    /// `skip`: copias que no se hacen (1 es la primera copia, 0 el original).
    Pattern {
        features: Vec<FeatureId>,
        pattern: PatternKind,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skip: Vec<u32>,
    },
    /// Refleja las herramientas de `features` (vacío = todo el cuerpo). Con
    /// todo el cuerpo, `op` `New` deja cada reflejo como pieza aparte en vez
    /// de unirlo.
    Mirror {
        features: Vec<FeatureId>,
        plane: PlaneSpec,
        #[serde(default)]
        op: BodyOp,
    },
    /// Mueve o gira piezas enteras (vacío = todas); con `copy` las deja y
    /// agrega las copias como piezas nuevas.
    Transform {
        #[serde(default)]
        parts: Vec<PartId>,
        motion: Motion,
        #[serde(default)]
        copy: bool,
    },
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
    /// El escaneo como cuerpo, para encajes, cunas y negativos.
    ScanBody(ScanBody),
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
    /// Curva helicoidal de referencia (camino de resortes y roscas). Con
    /// `radius2`, cónica (el radio pasa de `radius` a `radius2`); con `flat`,
    /// espiral plana de Arquímedes (sin avance: `pitch` no cuenta).
    Helix {
        axis: AxisSpec,
        radius: f64,
        pitch: f64,
        turns: f64,
        #[serde(default)]
        left: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius2: Option<f64>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flat: bool,
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
        /// Hasta la piel del escaneo en vez del sólido.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scan: Option<ScanReach>,
    },
    /// Lleva caras planas hasta un plano (una cara plana del sólido o uno de
    /// referencia): suma o resta lo que hay entre la cara y el plano.
    ReplaceFace { faces: Vec<FaceRef>, target: PlaneSpec },
    /// Lleva caras planas hasta la piel del escaneo (a `offset` de ella):
    /// suma si el escaneo está afuera, resta si está adentro de la pieza.
    FaceToScan {
        faces: Vec<FaceRef>,
        #[serde(default)]
        offset: f64,
        /// El escaneo corrido `offset`, que pone la app (como en [`ScanBody`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mesh: Option<ScanBodyMesh>,
    },
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
        /// Paso, mano y diámetro de otra rosca, con el filete alineado
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link: Option<ThreadLink>,
    },
    /// Chapa metálica: las regiones de un sketch extruidas al espesor. Guarda
    /// el radio interior de doblez y el factor K que usan sus pestañas y el
    /// desarrollo (ver [`crate::sheet`]).
    SheetMetal {
        sketch: FeatureId,
        #[serde(default)]
        regions: RegionSelection,
        thickness: f64,
        radius: f64,
        #[serde(default = "default_k_factor")]
        k_factor: f64,
        /// Hacia el lado contrario de la normal del sketch
        #[serde(default)]
        flip: bool,
        #[serde(default)]
        op: BodyOp,
    },
    /// Pestaña: una pared doblada desde una arista del borde de la chapa.
    /// Dobla hacia el lado de la cara de la arista (el otro con `flip`);
    /// `length` es la pared después del doblez.
    Flange {
        /// Sin elegir todavía: la operación avisa
        #[serde(default)]
        edge: Option<EdgeRef>,
        length: f64,
        #[serde(default = "default_flange_angle")]
        angle: f64,
        #[serde(default)]
        flip: bool,
        /// Radio interior (sin él, el de la chapa)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f64>,
        /// Ranura en un extremo donde la chapa sigue al lado del doblez (esquina
        /// hacia adentro): sin ella el doblez queda pegado a ese material
        #[serde(default)]
        relief: BendRelief,
        /// Ancho de la ranura (sin él, el espesor)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relief_width: Option<f64>,
        /// Alivio en el extremo que se junta con el doblez de otra pestaña
        #[serde(default)]
        corner_relief: CornerRelief,
        /// Radio (o medio lado) del alivio de esquina (sin él, el espesor)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corner_size: Option<f64>,
        /// Esquina cerrada donde se junta con otra pestaña (las dos a 90° hacia
        /// el mismo lado): esta pared se extiende hasta tapar el canto de la otra,
        /// y la otra se alarga hasta quedar a `corner_gap` de esta
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        closed_corner: bool,
        /// Holgura de la esquina cerrada (sin ella, 0,2 mm)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corner_gap: Option<f64>,
    },
    /// Superficie (sin espesor) que deja una curva de un sketch empujada a lo
    /// largo de la normal del plano: ciega, simétrica o a los dos lados.
    SurfaceExtrude {
        sketch: FeatureId,
        /// Curvas del sketch (vacío = todas las que no son de construcción);
        /// abiertas o cerradas, cada cadena da una superficie
        #[serde(default)]
        entities: Vec<u32>,
        extent: Extent,
        #[serde(default)]
        reverse: bool,
    },
    /// Superficie plana: las regiones cerradas de un sketch como caras.
    SurfacePlanar {
        sketch: FeatureId,
        #[serde(default)]
        regions: RegionSelection,
    },
    /// Copia desfasada de superficies (o de la piel de sólidos) a `distance`
    /// sobre su normal (negativo, hacia el otro lado); queda como pieza nueva.
    SurfaceOffset { parts: Vec<PartId>, distance: f64 },
    /// Curva de referencia hecha de otras (usable como camino de barridos y patrones).
    Curve { def: CurveDef },
    /// Extiende superficies `distance` más allá de bordes libres (cada cara
    /// por el lado de su parámetro donde está el borde, siguiendo su forma).
    SurfaceExtend { edges: Vec<EdgeRef>, distance: f64 },
    /// Superficie que pasa por curvas de varios sketches (una cadena por sección).
    SurfaceLoft {
        sections: Vec<CurveSection>,
        #[serde(default)]
        ruled: bool,
    },
    /// Superficie que deja una curva de un sketch llevada a lo largo de un camino.
    SurfaceSweep {
        sketch: FeatureId,
        #[serde(default)]
        entities: Vec<u32>,
        path: SweepPath,
        #[serde(default)]
        orient: SweepOrient,
    },
    /// Superficie que deja una curva de un sketch girada alrededor de un eje.
    SurfaceRevolve {
        sketch: FeatureId,
        #[serde(default)]
        entities: Vec<u32>,
        axis: AxisSpec,
        angle: f64,
    },
    /// Relleno: superficie que cierra un borde de aristas (de superficies o
    /// sólidos). Con `tangent`, sigue tangente a la cara de cada arista que
    /// tiene una sola (el borde de una superficie).
    Fill {
        edges: Vec<EdgeRef>,
        #[serde(default)]
        tangent: bool,
    },
    /// Partir piezas (vacío = todos los sólidos) con un plano o con otra pieza
    /// (una superficie curva, por ejemplo): cada pedazo queda como pieza.
    SplitBy {
        #[serde(default)]
        parts: Vec<PartId>,
        tool: SplitTool,
    },
    /// Coser: une superficies por sus bordes en una sola; con `solid`, lo que
    /// cierra un volumen queda sólido. Las piezas cosidas dejan de estar.
    Sew {
        parts: Vec<PartId>,
        #[serde(default)]
        solid: bool,
        /// Separación máxima entre bordes que se juntan
        #[serde(default = "default_sew_tolerance")]
        tolerance: f64,
    },
}

/// Con qué se parte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SplitTool {
    Plane { plane: PlaneSpec },
    /// Otra pieza (queda como estaba)
    Part { part: PartId },
}

fn default_sew_tolerance() -> f64 {
    0.01
}

/// Forma de la ranura de alivio de un doblez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BendRelief {
    #[default]
    Rectangle,
    /// Con el fondo redondo
    Obround,
    None,
}

/// Forma del alivio donde se juntan los dobleces de dos pestañas. Sin
/// alivio igual queda una muesca cuadrada del largo del doblez (los dobleces
/// van por fuera del borde de la chapa y se tocan solo en el vértice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerRelief {
    Round,
    Square,
    #[default]
    None,
}

fn default_k_factor() -> f64 {
    0.44
}

fn default_flange_angle() -> f64 {
    90.0
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
    /// tiene, los centros de sus círculos. El sentido lo da `reverse`.
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
    /// Rosca (cosmética o modelada) con la medida de otra y el filete alineado
    /// en el centro que está en su eje
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadLink>,
    /// Sentido: `false` contra la normal del plano (en un sketch sobre una
    /// cara, hacia adentro del material), `true` a favor; sin elegir, hacia
    /// el lado donde hay material (contra la normal si hay de los dos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
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
    /// Cónico: el agujero se angosta hacia el fondo; `angle` es el de la
    /// pared con el eje (grados; NPT 1°47' = 1,7833).
    Tapered { angle: f64 },
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
    /// Cómo gira el perfil a lo largo del camino.
    #[serde(default, skip_serializing_if = "SweepOrient::is_follow")]
    pub orient: SweepOrient,
    /// Barrido circular simple: un círculo de este radio al comienzo del
    /// camino, perpendicular a él, en vez del sketch (tubos, cables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle: Option<f64>,
    /// Barrido delgado: una pared de este espesor siguiendo el contorno.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thin: Option<f64>,
}

/// Orientación del perfil en un barrido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepOrient {
    /// Sigue el camino sin retorcerse (Frenet corregido).
    #[default]
    Follow,
    /// Triedro de Frenet: gira con la curvatura (hélices exactas).
    Frenet,
    /// Fijo: el perfil se traslada sin girar.
    Fixed,
}

impl SweepOrient {
    fn is_follow(&self) -> bool {
        *self == SweepOrient::Follow
    }

    /// Código del puente (`cad_pipe_ex`).
    pub fn code(self) -> i32 {
        match self {
            SweepOrient::Follow => 0,
            SweepOrient::Frenet => 1,
            SweepOrient::Fixed => 2,
        }
    }
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
    /// Arranca o termina en un punto (una punta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_point: Option<PointSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_point: Option<PointSpec>,
    /// Vuelve a la primera sección (un anillo).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    /// Delgada: una pared de este espesor siguiendo los contornos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thin: Option<f64>,
    /// Línea central: las secciones se llevan a lo largo de ella.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<SweepPath>,
    /// Curva guía (va con la línea central): el perfil gira y se escala para tocarla.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<SweepPath>,
}

/// Una sección de una superficie por recubrimiento: curvas de un sketch (vacío = todas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveSection {
    pub sketch: FeatureId,
    #[serde(default)]
    pub entities: Vec<u32>,
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
            FeatureKind::Sketch { plane, sketch, .. } => {
                let mut d = plane_deps(plane);
                if let Some(a) = &sketch.x_axis {
                    d.extend(axis_dep(a));
                }
                // Las curvas que lo perforan
                d.extend(sketch.constraints.iter().filter_map(|c| match c {
                    crate::sketch::SketchConstraint::Pierce { curve, .. } => Some(FeatureId(*curve)),
                    _ => None,
                }));
                d
            }
            FeatureKind::Sketch3d { sketch } => sketch
                .constraints
                .iter()
                .flat_map(|c| match c {
                    crate::sketch3d::Constraint3d::Attach { target, .. } => point_deps(target),
                    crate::sketch3d::Constraint3d::OnPlane { plane, .. } => plane_deps(plane),
                    _ => vec![],
                })
                .collect(),
            FeatureKind::Draft { neutral, .. } => plane_deps(neutral),
            FeatureKind::Split { plane, .. } => plane_deps(plane),
            FeatureKind::Plane { def } => match def {
                PlaneDef::Offset { base, .. } => plane_deps(base),
                PlaneDef::Angle { base, axis, .. } => [plane_deps(base), axis_deps(axis)].concat(),
                PlaneDef::Midplane { a, b } => [plane_deps(a), plane_deps(b)].concat(),
                PlaneDef::ThreePoints { points } => points.iter().flat_map(point_deps).collect(),
                PlaneDef::Scan { .. } => vec![],
                PlaneDef::Picked { refs, .. } => refs.iter().flat_map(pick_deps).collect(),
            },
            FeatureKind::Axis { def } => match def {
                AxisDef::TwoPoints { a, b } => [point_deps(a), point_deps(b)].concat(),
                AxisDef::Planes { a, b } => [plane_deps(a), plane_deps(b)].concat(),
                AxisDef::Picked { refs } => refs.iter().flat_map(pick_deps).collect(),
                _ => vec![],
            },
            FeatureKind::Point { def } => point_deps(def),
            FeatureKind::Sweep(s) => match &s.path {
                SweepPath::Sketch { sketch, .. } => vec![s.sketch, *sketch],
                SweepPath::Curve { feature } => vec![s.sketch, *feature],
            },
            FeatureKind::SurfaceSweep { sketch, path, .. } => match path {
                SweepPath::Sketch { sketch: p, .. } => vec![*sketch, *p],
                SweepPath::Curve { feature } => vec![*sketch, *feature],
            },
            FeatureKind::Helix { axis, .. } => axis_deps(axis),
            FeatureKind::Curve { def } => match def {
                CurveDef::Composite { curves, .. } => curves.clone(),
                CurveDef::Projected { sketch, .. } => vec![*sketch],
                CurveDef::Intersection { .. } => vec![],
                CurveDef::Points { points, .. } => points.iter().flat_map(point_deps).collect(),
            },
            FeatureKind::Scale { center, .. } => point_deps(center),
            FeatureKind::Loft(l) => {
                let path_dep = |p: &SweepPath| match p {
                    SweepPath::Sketch { sketch, .. } => *sketch,
                    SweepPath::Curve { feature } => *feature,
                };
                let mut d: Vec<FeatureId> = l.sections.iter().map(|s| s.sketch).collect();
                d.extend(l.start_point.iter().chain(&l.end_point).flat_map(point_deps));
                d.extend(l.path.iter().chain(&l.guide).map(path_dep));
                d
            }
            FeatureKind::SurfaceLoft { sections, .. } => sections.iter().map(|s| s.sketch).collect(),
            FeatureKind::Hole(h) => [vec![h.sketch], h.link.iter().map(|l| l.feature).collect()].concat(),
            FeatureKind::Primitive(p) => p.link.iter().map(|l| l.feature).collect(),
            FeatureKind::Thread { link, .. } => link.iter().map(|l| l.feature).collect(),
            FeatureKind::Rib { sketch, .. } => vec![*sketch],
            FeatureKind::ScanBody(b) => b.region.iter().map(|r| r.sketch).collect(),
            FeatureKind::SheetMetal { sketch, .. } => vec![*sketch],
            FeatureKind::ReplaceFace { target, .. } => plane_deps(target),
            FeatureKind::Extrude(e) => {
                let mut d = vec![e.sketch];
                if let Extent::UpToVertex { point } = &e.extent {
                    d.extend(point_deps(point));
                }
                if let Some(a) = &e.direction {
                    d.extend(axis_deps(a));
                }
                d
            }
            FeatureKind::Revolve(r) => {
                let mut d = vec![r.sketch];
                d.extend(axis_dep(&r.axis));
                d
            }
            FeatureKind::SurfaceExtrude { sketch, .. } | FeatureKind::SurfacePlanar { sketch, .. } => vec![*sketch],
            FeatureKind::SurfaceOffset { parts, .. } => parts.iter().map(|p| p.feature).collect(),
            FeatureKind::SurfaceRevolve { sketch, axis, .. } => {
                let mut d = vec![*sketch];
                d.extend(axis_dep(axis));
                d
            }
            FeatureKind::Sew { parts, .. } => parts.iter().map(|p| p.feature).collect(),
            FeatureKind::SplitBy { parts, tool } => {
                let mut d: Vec<FeatureId> = parts.iter().map(|p| p.feature).collect();
                match tool {
                    SplitTool::Plane { plane } => d.extend(plane_deps(plane)),
                    SplitTool::Part { part } => d.push(part.feature),
                }
                d
            }
            FeatureKind::Pattern { features, pattern, .. } => {
                let mut d = features.clone();
                match pattern {
                    PatternKind::Circular { axis, .. } => d.extend(axis_dep(axis)),
                    PatternKind::Curve { path: SweepPath::Sketch { sketch, .. }, .. } => d.push(*sketch),
                    PatternKind::Curve { path: SweepPath::Curve { feature }, .. } => d.push(*feature),
                    PatternKind::Fill { sketch, .. } | PatternKind::Points { sketch, .. } => d.push(*sketch),
                    _ => {}
                }
                d
            }
            FeatureKind::Mirror { features, plane, .. } => [features.clone(), plane_deps(plane)].concat(),
            FeatureKind::Transform { parts, motion, .. } => {
                let mut d: Vec<FeatureId> = parts.iter().map(|p| p.feature).collect();
                match motion {
                    Motion::Translate { .. } => {}
                    Motion::Rotate { axis, .. } => d.extend(axis_deps(axis)),
                    Motion::Points { from, to } => d.extend([point_deps(from), point_deps(to)].concat()),
                    Motion::Align { to, .. } => d.extend(plane_deps(to)),
                }
                d
            }
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
            FeatureKind::Sketch3d { .. } => "Sketch 3D",
            FeatureKind::SurfaceSketch { .. } => "Sketch envuelto",
            FeatureKind::Extrude(_) => "Extrusión",
            FeatureKind::Revolve(_) => "Revolución",
            FeatureKind::Primitive(p) => match p.shape {
                PrimitiveShape::Box { .. } => "Caja",
                PrimitiveShape::Cylinder { .. } => "Cilindro",
                PrimitiveShape::Cone { .. } => "Cono",
                PrimitiveShape::Sphere { .. } => "Esfera",
                PrimitiveShape::Torus { .. } => "Toro",
                PrimitiveShape::Bolt { .. } => "Tornillo",
                PrimitiveShape::Nut { .. } => "Tuerca",
                PrimitiveShape::Washer { .. } => "Arandela",
            },
            FeatureKind::Fillet { .. } => "Redondeo",
            FeatureKind::Chamfer { .. } => "Chaflán",
            FeatureKind::Shell { .. } => "Vaciado",
            FeatureKind::DeleteFace { .. } => "Eliminar cara",
            FeatureKind::Draft { .. } => "Desmolde",
            FeatureKind::Pattern { .. } => "Patrón",
            FeatureKind::Mirror { .. } => "Simetría",
            FeatureKind::Split { .. } => "Corte",
            FeatureKind::Import { .. } => "Importado",
            FeatureKind::ScanBody(b) if b.region.is_some() => "Parche del escaneo",
            FeatureKind::ScanBody(b) if b.thickness != 0.0 => "Carcasa del escaneo",
            FeatureKind::ScanBody(_) => "Cuerpo del escaneo",
            FeatureKind::Boolean { .. } => "Booleana",
            FeatureKind::SplitParts { .. } => "Separar piezas",
            FeatureKind::DeleteParts { .. } => "Borrar pieza",
            FeatureKind::Plane { .. } => "Plano",
            FeatureKind::Axis { .. } => "Eje",
            FeatureKind::Point { .. } => "Punto",
            FeatureKind::Sweep(_) => "Barrido",
            FeatureKind::Loft(_) => "Transición",
            FeatureKind::Hole(_) => "Agujero",
            FeatureKind::Helix { flat: true, .. } => "Espiral",
            FeatureKind::Helix { .. } => "Hélice",
            FeatureKind::Thicken { .. } => "Engrosar",
            FeatureKind::Rib { .. } => "Nervio",
            FeatureKind::ReplaceFace { .. } => "Reemplazar cara",
            FeatureKind::FaceToScan { .. } => "Cara al escaneo",
            FeatureKind::Thread { .. } => "Rosca",
            FeatureKind::SheetMetal { .. } => "Chapa",
            FeatureKind::Flange { .. } => "Pestaña",
            FeatureKind::SurfaceExtrude { .. } => "Superficie extruida",
            FeatureKind::SurfacePlanar { .. } => "Superficie plana",
            FeatureKind::SurfaceSweep { .. } => "Superficie por barrido",
            FeatureKind::SurfaceLoft { .. } => "Superficie por transición",
            FeatureKind::SurfaceExtend { .. } => "Extender superficie",
            FeatureKind::Curve { def: CurveDef::Composite { .. } } => "Curva compuesta",
            FeatureKind::Curve { def: CurveDef::Projected { .. } } => "Curva proyectada",
            FeatureKind::Curve { def: CurveDef::Intersection { .. } } => "Curva de cruce",
            FeatureKind::Curve { def: CurveDef::Points { .. } } => "Curva por puntos",
            FeatureKind::SurfaceOffset { .. } => "Superficie desfasada",
            FeatureKind::SurfaceRevolve { .. } => "Superficie de revolución",
            FeatureKind::Fill { .. } => "Relleno",
            FeatureKind::Sew { .. } => "Coser",
            FeatureKind::SplitBy { .. } => "Partir",
            FeatureKind::MoveFace { .. } => "Mover cara",
            FeatureKind::Scale { .. } => "Escala",
            FeatureKind::Transform { copy: true, .. } => "Copiar cuerpo",
            FeatureKind::Transform { motion: Motion::Rotate { .. }, .. } => "Girar cuerpo",
            FeatureKind::Transform { .. } => "Mover cuerpo",
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
    /// Con qué piezas une, resta o interseca (vacío = las que toca; en los
    /// agujeros, la más cercana a cada centro).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<PartId>,
}
