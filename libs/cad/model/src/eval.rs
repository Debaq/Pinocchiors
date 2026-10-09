//! Recálculo del árbol. Una operación que falla queda marcada con su error y el
//! cuerpo sigue como estaba: las siguientes se recalculan igual.

use std::cell::RefCell;
use std::collections::HashMap;

use cad_occt::{Axis, Curve, Frame, History, Shape, SurfaceKind, with_history};
use serde::{Deserialize, Serialize};

use crate::document::{Document, ResolvedValue};
use crate::feature::*;
use crate::geom::*;
use crate::regions::{Loop, LoopPiece, Region, arc_sweep, find_regions};
use crate::sketch::{Geometry, Sketch, SolveReport};
use crate::standard::Threading;
use crate::threads::{ThreadAxis, default_x, spec_name};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FeatureState {
    Ok,
    /// Falló; `missing` dice qué referencias no se encontraron (si fue por eso).
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        missing: Vec<MissingRef>,
    },
    /// Se calculó con parte de lo elegido: el resto ya no está.
    Warning { message: String, missing: Vec<MissingRef> },
    Suppressed,
    RolledBack,
}

/// Referencia de una operación que no se encontró: el campo (como se llama en
/// la operación: `edges`, `faces`, `regions`, `plane`, `neutral`, `axis`,
/// `extent`) y su posición en la lista (0 si es una sola).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissingRef {
    pub field: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureStatus {
    pub id: FeatureId,
    #[serde(flatten)]
    pub state: FeatureState,
    /// Milisegundos que tardó la última vez que se calculó (de la caché, el de entonces).
    #[serde(default)]
    pub ms: f64,
    /// Piezas que eligió sola (sin alcance elegido; p. ej. la más cercana a un agujero).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_scope: Vec<PartId>,
    /// Flecha para el visor (extrusiones, chapas y agujeros).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<Handle>,
}

/// Flecha de una operación en el visor: hacia dónde va y hasta dónde llega.
/// Al pincharla se invierte; al arrastrarla cambia el largo (según `kind`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    pub origin: P3,
    /// Sentido en que avanza (unitario).
    pub dir: P3,
    /// Hasta dónde llega la punta (mm desde `origin`).
    pub length: f64,
    /// Sentido efectivo: en la extrusión y la chapa, el `reverse`/`flip` con
    /// que se calculó; en el agujero, si va a favor de la normal del plano.
    pub reversed: bool,
    pub kind: HandleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleKind {
    /// La punta es la distancia (o la profundidad).
    Blind,
    /// La punta es la mitad de la distancia.
    Symmetric,
    /// La punta es la distancia del primer lado.
    TwoSides,
    /// El largo no se cambia arrastrando (pasante, hasta una cara...).
    Fixed,
}

/// Sketch ya resuelto y ubicado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchResult {
    pub plane: Plane,
    pub sketch: Sketch,
    pub report: SolveReport,
    pub regions: Vec<Region>,
}

/// Sólido con el origen de cada cara (`tags[i]` = orígenes de la cara i).
#[derive(Debug, Clone)]
struct Tagged {
    shape: Shape,
    tags: Vec<Vec<FaceTag>>,
}

/// Geometría de referencia calculada (mm, Z arriba).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefGeom {
    Plane { plane: Plane },
    Axis { origin: P3, dir: P3 },
    Point { point: P3 },
    /// Curva (hélice), como polilínea para dibujarla.
    Curve { points: Vec<P3> },
}

/// Pieza calculada.
#[derive(Debug, Clone)]
pub struct Part {
    pub id: PartId,
    pub shape: Shape,
    /// Orígenes de cada cara de la pieza.
    pub tags: Vec<Vec<FaceTag>>,
}

impl Part {
    /// Superficie (caras sin volumen) en vez de sólido.
    pub fn is_surface(&self) -> bool {
        is_surface(&self.shape)
    }
}

/// Sin sólidos adentro: caras o conchas sueltas.
pub fn is_surface(shape: &Shape) -> bool {
    shape.solids().is_ok_and(|s| s.is_empty())
}

#[derive(Debug, Default)]
pub struct Evaluation {
    /// Todas las piezas juntas (la pieza misma si hay una sola): sus caras y
    /// aristas son las de cada pieza, en el orden de `parts`.
    pub body: Option<Shape>,
    /// Orígenes de cada cara del cuerpo (mismo orden que sus índices).
    pub face_tags: Vec<Vec<FaceTag>>,
    /// Piezas, en el orden en que se crearon.
    pub parts: Vec<Part>,
    pub status: Vec<FeatureStatus>,
    pub sketches: HashMap<FeatureId, SketchResult>,
    /// Planos, ejes y puntos de referencia.
    pub references: HashMap<FeatureId, RefGeom>,
    /// Curvas de referencia (alambres), para barridos.
    pub curves: HashMap<FeatureId, Shape>,
    /// Herramienta de cada operación que la tiene (para patrones y simetrías).
    tools: HashMap<FeatureId, (Tagged, BodyOp)>,
    /// Parámetros y campos vinculados, ya calculados.
    pub parameters: Vec<ResolvedValue>,
    pub bindings: Vec<ResolvedValue>,
    /// Cuántas operaciones se calcularon (las demás salieron de la caché).
    pub recomputed: usize,
    /// Roscas (agujeros roscados, roscas, tornillos y tuercas), en orden.
    pub threads: Vec<ThreadAxis>,
    /// Dobleces de las pestañas, en orden (para los alivios de esquina).
    pub bends: Vec<BendInfo>,
}

/// Arista de la chapa de la que sale una pestaña.
#[derive(Debug, Clone, Serialize)]
pub struct BendInfo {
    pub feature: FeatureId,
    pub part: PartId,
    pub start: P3,
    pub end: P3,
    /// Normal de la cara de la arista (la chapa queda del otro lado)
    pub normal: P3,
    pub thickness: f64,
    /// Hacia dónde dobla, normal del canto (afuera), radio interior, ángulo
    /// (grados) y largo de la pared
    pub bend: P3,
    pub outward: P3,
    pub radius: f64,
    pub angle: f64,
    pub length: f64,
}

/// Lo que una pestaña sabe de su arista, para los alivios.
struct FlangeGeom {
    start: P3,
    end: P3,
    /// De `start` a `end`
    dir: P3,
    /// Normal de la cara de la arista
    n: P3,
    /// Normal del canto (hacia afuera de la chapa)
    m: P3,
    t: f64,
    r: f64,
    /// Hacia dónde dobla
    b: P3,
    angle: f64,
    length: f64,
}

/// Estado después de una operación, guardado por la huella de todo lo que
/// llevó hasta ahí (la operación y las anteriores).
#[derive(Debug, Clone)]
struct CacheEntry {
    body: Option<Shape>,
    face_tags: Vec<Vec<FaceTag>>,
    parts: Vec<Part>,
    state: FeatureState,
    ms: f64,
    sketch: Option<SketchResult>,
    reference: Option<RefGeom>,
    curve: Option<Shape>,
    tool: Option<(Tagged, BodyOp)>,
    threads: Vec<ThreadAxis>,
    bends: Vec<BendInfo>,
    auto_scope: Vec<PartId>,
    handle: Option<Handle>,
    /// Para descartar las menos usadas.
    used: u64,
}

/// Caché del recálculo por operación: cambiar algo al final de un historial
/// largo solo recalcula desde ahí. Las huellas encadenan cada operación con
/// las anteriores, así se reutiliza cualquier prefijo ya calculado (la vista
/// previa de una operación y el documento guardado comparten el comienzo).
#[derive(Debug)]
pub struct EvalCache {
    entries: HashMap<u64, CacheEntry>,
    clock: u64,
    /// Máximo de estados guardados.
    pub capacity: usize,
}

impl Default for EvalCache {
    fn default() -> Self {
        Self { entries: HashMap::new(), clock: 0, capacity: 128 }
    }
}

impl EvalCache {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn get(&mut self, key: u64) -> Option<&CacheEntry> {
        self.clock += 1;
        let clock = self.clock;
        let e = self.entries.get_mut(&key)?;
        e.used = clock;
        Some(e)
    }

    fn put(&mut self, key: u64, mut e: CacheEntry) {
        self.clock += 1;
        e.used = self.clock;
        self.entries.insert(key, e);
        if self.entries.len() > self.capacity {
            // Fuera la cuarta parte menos usada
            let mut by_use: Vec<(u64, u64)> = self.entries.iter().map(|(k, e)| (e.used, *k)).collect();
            by_use.sort_unstable();
            for (_, k) in by_use.iter().take(self.entries.len() / 4) {
                self.entries.remove(k);
            }
        }
    }
}

/// Huella de una operación encadenada con la anterior: todo lo que el cálculo
/// lee (la operación ya resuelta, si está suprimida o detrás de la barra). El
/// nombre no entra: renombrar no recalcula.
fn fingerprint(prev: u64, f: &Feature, rolled_back: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    prev.hash(&mut h);
    f.id.hash(&mut h);
    f.suppressed.hash(&mut h);
    rolled_back.hash(&mut h);
    serde_json::to_string(&f.kind).unwrap_or_default().hash(&mut h);
    serde_json::to_string(&f.scope).unwrap_or_default().hash(&mut h);
    h.finish()
}

impl Evaluation {
    pub fn state(&self, id: FeatureId) -> Option<&FeatureState> {
        self.status.iter().find(|s| s.id == id).map(|s| &s.state)
    }

    /// Caras y aristas de cada pieza dentro del cuerpo: (pieza, caras, aristas).
    pub fn part_ranges(&self) -> Vec<(PartId, std::ops::Range<usize>, std::ops::Range<usize>)> {
        let (mut f, mut e) = (0, 0);
        self.parts
            .iter()
            .map(|p| {
                let (nf, ne) = (p.shape.face_count(), p.shape.edge_count());
                let r = (p.id, f..f + nf, e..e + ne);
                f += nf;
                e += ne;
                r
            })
            .collect()
    }

    /// Pieza dueña de una cara del cuerpo.
    pub fn part_of_face(&self, face: usize) -> Option<PartId> {
        self.part_ranges().into_iter().find(|(_, f, _)| f.contains(&face)).map(|(id, _, _)| id)
    }

    pub fn errors(&self) -> Vec<(FeatureId, String)> {
        self.status
            .iter()
            .filter_map(|s| match &s.state {
                FeatureState::Error { message, .. } => Some((s.id, message.clone())),
                _ => None,
            })
            .collect()
    }

    /// Herramienta de una operación (el prisma de una extrusión, la primitiva…)
    /// y cómo se combinó con el cuerpo; para la vista previa.
    pub fn tool(&self, id: FeatureId) -> Option<(&Shape, BodyOp)> {
        self.tools.get(&id).map(|(t, op)| (&t.shape, *op))
    }

    /// Referencia estable a una cara del cuerpo actual.
    pub fn face_ref(&self, face: usize) -> Option<FaceRef> {
        let info = self.body.as_ref()?.face_info(face).ok()?;
        let tags = self.face_tags.get(face).cloned().unwrap_or_default();
        Some(FaceRef { point: info.point, normal: info.normal, tags })
    }

    /// Referencia estable a una arista del cuerpo actual.
    pub fn edge_ref(&self, edge: usize) -> Option<EdgeRef> {
        let body = self.body.as_ref()?;
        let info = body.edge_info(edge).ok()?;
        let pair = body.edge_face_pairs().ok()?.get(edge).copied()?;
        let side = |f: Option<usize>| f.and_then(|f| self.face_tags.get(f).cloned()).unwrap_or_default();
        let sides = vec![side(pair[0]), side(pair[1])];
        let sides = if sides.iter().all(|s| !s.is_empty()) { sides } else { vec![] };
        Some(EdgeRef { point: info.mid, direction: info.tangent, sides })
    }

    fn diag(&self) -> f64 {
        self.body.as_ref().and_then(|b| b.mass().ok()).map_or(1.0, |m| norm(sub(m.bbox_max, m.bbox_min)).max(1.0))
    }

    /// Índice de la cara del cuerpo que corresponde a la referencia.
    pub fn resolve_face(&self, r: &FaceRef) -> R<usize> {
        let body = self.body.as_ref().ok_or("todavía no hay un sólido")?;
        // Por origen: las caras que lo conservan (la primera etiqueta es la
        // principal); entre ellas, la más cercana al punto guardado
        for wanted in [&r.tags[..r.tags.len().min(1)], &r.tags[..]] {
            let candidates: Vec<usize> = (0..self.face_tags.len())
                .filter(|&f| self.face_tags[f].iter().any(|t| wanted.contains(t)))
                .collect();
            if let Some(best) = candidates
                .iter()
                .map(|&f| (f, body.face_distance(f, r.point).unwrap_or(f64::MAX)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                return Ok(best.0);
            }
        }
        let (i, d) = body
            .closest_face(r.point, Some(r.normal), 0.9)
            .ok_or("la cara de referencia ya no existe")?;
        if d > self.diag() * 0.5 {
            return Err("la cara de referencia ya no existe".into());
        }
        Ok(i)
    }

    /// Índice de la arista del cuerpo que corresponde a la referencia.
    pub fn resolve_edge(&self, r: &EdgeRef) -> R<usize> {
        let body = self.body.as_ref().ok_or("todavía no hay un sólido")?;
        // Por origen: la arista entre una cara de cada lado
        if r.sides.len() == 2 && r.sides.iter().all(|s| !s.is_empty()) {
            let has = |f: Option<usize>, side: &[FaceTag]| {
                f.and_then(|f| self.face_tags.get(f)).is_some_and(|ts| ts.iter().any(|t| side.contains(t)))
            };
            let pairs = body.edge_face_pairs().map_err(err)?;
            let best = pairs
                .iter()
                .enumerate()
                .filter(|(_, [a, b])| {
                    (has(*a, &r.sides[0]) && has(*b, &r.sides[1])) || (has(*a, &r.sides[1]) && has(*b, &r.sides[0]))
                })
                .map(|(e, _)| (e, body.edge_distance(e, r.point).unwrap_or(f64::MAX)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((e, _)) = best {
                return Ok(e);
            }
        }
        let (i, d) = body
            .closest_edge(r.point, Some(r.direction), 0.9)
            .ok_or("la arista de referencia ya no existe")?;
        if d > self.diag() * 0.5 {
            return Err("la arista de referencia ya no existe".into());
        }
        Ok(i)
    }
}

type R<T> = Result<T, String>;
type ShapeFn = Box<dyn Fn(&Shape) -> R<Shape>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn tag(feature: FeatureId, name: impl Into<String>) -> FaceTag {
    FaceTag { feature, name: name.into() }
}

/// Orígenes de las caras de un resultado a partir de los de sus entradas.
/// Devuelve también cuántas entradas de la historia se consumieron (lo que
/// sigue son caras generadas por aristas, en redondeos y chaflanes).
fn propagate(inputs: &[&[Vec<FaceTag>]], h: &History, n_out: usize) -> (Vec<Vec<FaceTag>>, usize) {
    let mut out = vec![Vec::new(); n_out];
    let mut k = 0;
    for tags in inputs {
        for t in tags.iter() {
            if let Some(images) = h.images.get(k) {
                for &f in images {
                    if let Some(slot) = out.get_mut(f) {
                        for x in t {
                            if !slot.contains(x) {
                                slot.push(x.clone());
                            }
                        }
                    }
                }
            }
            k += 1;
        }
    }
    (out, k)
}

/// Copia de los orígenes con un sufijo (copias de patrones y simetrías).
fn suffixed(tags: &[Vec<FaceTag>], h: &History, n_out: usize, suffix: &str) -> Vec<Vec<FaceTag>> {
    let renamed: Vec<Vec<FaceTag>> =
        tags.iter().map(|ts| ts.iter().map(|t| FaceTag { feature: t.feature, name: format!("{}{suffix}", t.name) }).collect()).collect();
    propagate(&[&renamed], h, n_out).0
}

fn occt_err(e: String) -> cad_occt::Error {
    cad_occt::Error(e)
}

/// Aviso de un corte que no se lleva nada.
const NOTHING_CUT: &str = "no toca ninguna pieza: prueba invertir el sentido";

struct Ctx<'a> {
    doc: &'a Document,
    ev: Evaluation,
    /// Referencias no encontradas y avisos de la operación en curso.
    missing: RefCell<Vec<MissingRef>>,
    warnings: RefCell<Vec<String>>,
    /// Alcance de la operación en curso (vacío = las piezas que toca).
    scope: Vec<PartId>,    /// Piezas que la operación en curso eligió sola.
    auto_scope: Vec<PartId>,
    /// Flecha de la operación en curso (la última herramienta calculada).
    handle: RefCell<Option<Handle>>,
}

pub fn evaluate(doc: &Document) -> Evaluation {
    evaluate_with(doc, &mut EvalCache::default())
}

/// Como `evaluate`, reutilizando lo que la caché ya tiene calculado.
pub fn evaluate_with(doc: &Document, cache: &mut EvalCache) -> Evaluation {
    // Primero las fórmulas: el árbol se calcula con los números que dan
    let res = doc.resolve();
    let doc = &res.document;
    let mut ctx = Ctx {
        doc,
        ev: Evaluation { parameters: res.parameters, bindings: res.bindings, ..Default::default() },
        missing: RefCell::default(),
        warnings: RefCell::default(),
        scope: Vec::new(),
        auto_scope: Vec::new(),
        handle: RefCell::default(),
    };
    let limit = doc.rollback.unwrap_or(usize::MAX);
    let mut key = 0u64;
    for (i, f) in doc.features.iter().enumerate() {
        key = fingerprint(key, f, i >= limit);
        if let Some(e) = cache.get(key) {
            ctx.ev.body = e.body.clone();
            ctx.ev.face_tags = e.face_tags.clone();
            ctx.ev.parts = e.parts.clone();
            if let Some(s) = &e.sketch {
                ctx.ev.sketches.insert(f.id, s.clone());
            }
            if let Some(r) = &e.reference {
                ctx.ev.references.insert(f.id, r.clone());
            }
            if let Some(c) = &e.curve {
                ctx.ev.curves.insert(f.id, c.clone());
            }
            if let Some(t) = &e.tool {
                ctx.ev.tools.insert(f.id, t.clone());
            }
            ctx.ev.threads.extend(e.threads.iter().cloned());
            ctx.ev.bends.extend(e.bends.iter().cloned());
            ctx.ev.status.push(FeatureStatus { id: f.id, state: e.state.clone(), ms: e.ms, auto_scope: e.auto_scope.clone(), handle: e.handle.clone() });
            continue;
        }
        let start = std::time::Instant::now();
        ctx.auto_scope.clear();
        ctx.handle.take();
        let state = if i >= limit {
            FeatureState::RolledBack
        } else if f.suppressed {
            FeatureState::Suppressed
        } else {
            ctx.ev.recomputed += 1;
            ctx.scope = f.scope.clone();
            let result = ctx.feature(f);
            let missing = ctx.missing.take();
            let warnings = ctx.warnings.take();
            match result {
                Ok(()) if missing.is_empty() && warnings.is_empty() => FeatureState::Ok,
                Ok(()) => FeatureState::Warning { message: warnings.join("; "), missing },
                Err(message) => FeatureState::Error { message, missing },
            }
        };
        // En centésimas: el JSON queda corto y se vuelve a leer exacto
        let ms = (start.elapsed().as_secs_f64() * 100_000.0).round() / 100.0;
        cache.put(
            key,
            CacheEntry {
                body: ctx.ev.body.clone(),
                face_tags: ctx.ev.face_tags.clone(),
                parts: ctx.ev.parts.clone(),
                state: state.clone(),
                ms,
                sketch: ctx.ev.sketches.get(&f.id).cloned(),
                reference: ctx.ev.references.get(&f.id).cloned(),
                curve: ctx.ev.curves.get(&f.id).cloned(),
                tool: ctx.ev.tools.get(&f.id).cloned(),
                threads: ctx.ev.threads.iter().filter(|t| t.feature == f.id).cloned().collect(),
                bends: ctx.ev.bends.iter().filter(|b| b.feature == f.id).cloned().collect(),
                auto_scope: ctx.auto_scope.clone(),
                handle: ctx.handle.borrow().clone(),
                used: 0,
            },
        );
        ctx.ev.status.push(FeatureStatus { id: f.id, state, ms, auto_scope: std::mem::take(&mut ctx.auto_scope), handle: ctx.handle.take() });
    }
    ctx.ev
}

impl Ctx<'_> {
    fn body(&self) -> R<&Shape> {
        self.ev.body.as_ref().ok_or_else(|| "todavía no hay un sólido".to_string())
    }

    /// Rehace el cuerpo (todas las piezas juntas) y sus orígenes de caras.
    fn sync(&mut self) {
        let parts = &self.ev.parts;
        match parts.len() {
            0 => {
                self.ev.body = None;
                self.ev.face_tags.clear();
            }
            1 => {
                self.ev.body = Some(parts[0].shape.clone());
                self.ev.face_tags = parts[0].tags.clone();
            }
            _ => {
                let shapes: Vec<Shape> = parts.iter().map(|p| p.shape.clone()).collect();
                self.ev.body = Shape::compound(&shapes).ok();
                self.ev.face_tags = parts.iter().flat_map(|p| p.tags.iter().cloned()).collect();
            }
        }
    }

    fn new_part(&mut self, feature: FeatureId, tool: Tagged) {
        let index = self.ev.parts.iter().filter(|p| p.id.feature == feature).count() as u32;
        self.ev.parts.push(Part { id: PartId { feature, index }, shape: tool.shape, tags: tool.tags });
    }

    /// Piezas que la forma toca (se cruzan o se apoyan).
    fn touching(&self, tool: &Shape) -> Vec<usize> {
        let Ok(tm) = tool.mass() else { return vec![] };
        let tol = self.diag().max(norm(sub(tm.bbox_max, tm.bbox_min))) * 1e-7;
        // Las superficies solo cambian si se eligen en el alcance
        self.ev
            .parts
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_surface())
            .filter(|(_, p)| {
                let Ok(pm) = p.shape.mass() else { return false };
                let apart = (0..3).any(|k| pm.bbox_min[k] > tm.bbox_max[k] + tol || tm.bbox_min[k] > pm.bbox_max[k] + tol);
                !apart && p.shape.min_distance(tool).is_some_and(|(d, _, _)| d <= tol)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Por cada punto, la pieza más cercana entre las que toca la herramienta.
    fn nearest_parts(&self, tool: &Shape, points: &[P3]) -> Vec<PartId> {
        let touched = self.touching(tool);
        let mut out = Vec::new();
        for &p in points {
            let Ok(v) = Shape::vertex(p) else { continue };
            let best = touched
                .iter()
                .filter_map(|&i| self.ev.parts[i].shape.min_distance(&v).map(|(d, _, _)| (d, i)))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, i)) = best
                && !out.contains(&self.ev.parts[i].id)
            {
                out.push(self.ev.parts[i].id);
            }
        }
        out
    }

    /// Reemplaza la pieza `i` por el resultado de una operación sobre ella:
    /// los orígenes pasan por la historia y `extra` nombra lo generado
    /// después. Si no queda nada, la pieza se va.
    fn replace_part(&mut self, i: usize, new: Shape, h: &History, extra: impl Fn(usize) -> Option<FaceTag>) {
        let n = new.face_count();
        if n == 0 {
            self.ev.parts.remove(i);
            return;
        }
        let (mut tags, used) = propagate(&[&self.ev.parts[i].tags], h, n);
        for (k, images) in h.images.iter().enumerate().skip(used) {
            if let Some(t) = extra(k - used) {
                for &f in images {
                    if let Some(slot) = tags.get_mut(f)
                        && !slot.contains(&t)
                    {
                        slot.push(t.clone());
                    }
                }
            }
        }
        let p = &mut self.ev.parts[i];
        p.shape = new;
        p.tags = tags;
    }

    /// Posiciones en `parts` de las piezas pedidas; las que no están se anotan
    /// como referencias perdidas del campo `field` (falla si no queda ninguna).
    fn find_parts(&self, field: &str, ids: &[PartId]) -> R<Vec<usize>> {
        let mut out = Vec::new();
        for (k, id) in ids.iter().enumerate() {
            match self.ev.parts.iter().position(|p| p.id == *id) {
                Some(i) if !out.contains(&i) => out.push(i),
                Some(_) => {}
                None => self.miss(field, k),
            }
        }
        if out.is_empty() && !ids.is_empty() {
            return Err("no se encontró ninguna de las piezas elegidas".into());
        }
        if out.len() < ids.len() {
            self.warn(format!("faltan {} de {} piezas", ids.len() - out.len(), ids.len()));
        }
        Ok(out)
    }

    /// Piezas sobre las que actúa la operación: las del alcance o las que toca.
    fn targets(&self, tool: &Shape) -> R<Vec<usize>> {
        if self.scope.is_empty() {
            return Ok(self.touching(tool));
        }
        let mut v = self.find_parts("scope", &self.scope)?;
        v.sort_unstable();
        Ok(v)
    }

    fn apply(&mut self, feature: FeatureId, tool: Tagged, op: BodyOp) -> R<()> {
        if self.ev.parts.is_empty() && matches!(op, BodyOp::Cut | BodyOp::Intersect) {
            return Err("no hay sólido que cortar".into());
        }
        // Una superficie no se une ni resta: queda como pieza aparte
        let op = if is_surface(&tool.shape) { BodyOp::New } else { op };
        match op {
            BodyOp::New => self.new_part(feature, tool),
            BodyOp::Join => {
                let touched = self.targets(&tool.shape)?;
                if touched.is_empty() {
                    self.new_part(feature, tool);
                } else {
                    // Las piezas que toca y la herramienta, en una: la primera
                    let mut inputs: Vec<&Tagged> = Vec::new();
                    let parts: Vec<Tagged> =
                        touched.iter().map(|&i| Tagged { shape: self.ev.parts[i].shape.clone(), tags: self.ev.parts[i].tags.clone() }).collect();
                    inputs.extend(parts.iter());
                    inputs.push(&tool);
                    let shapes: Vec<Shape> = inputs.iter().map(|t| t.shape.clone()).collect();
                    let (new, h) = with_history(|| Shape::fuse_all(&shapes)).map_err(err)?;
                    let tag_lists: Vec<&[Vec<FaceTag>]> = inputs.iter().map(|t| t.tags.as_slice()).collect();
                    let tags = propagate(&tag_lists, &h, new.face_count()).0;
                    let first = touched[0];
                    for &i in touched.iter().skip(1).rev() {
                        self.ev.parts.remove(i);
                    }
                    let p = &mut self.ev.parts[first];
                    p.shape = new;
                    p.tags = tags;
                }
            }
            BodyOp::Cut => {
                let targets = self.targets(&tool.shape)?;
                if targets.is_empty() {
                    self.warn(NOTHING_CUT.into());
                }
                for i in targets.into_iter().rev() {
                    let part = self.ev.parts[i].shape.clone();
                    let (new, h) = with_history(|| part.cut(&tool.shape)).map_err(err)?;
                    let n = new.face_count();
                    if n == 0 {
                        self.ev.parts.remove(i);
                        continue;
                    }
                    let tags = propagate(&[&self.ev.parts[i].tags, &tool.tags], &h, n).0;
                    let p = &mut self.ev.parts[i];
                    p.shape = new;
                    p.tags = tags;
                }
            }
            BodyOp::Intersect => {
                // Automático: lo que la herramienta no toca desaparece; con
                // alcance, solo cambian las piezas elegidas
                let explicit = !self.scope.is_empty();
                let touched = self.targets(&tool.shape)?;
                for i in (0..self.ev.parts.len()).rev() {
                    if !touched.contains(&i) {
                        if !explicit {
                            self.ev.parts.remove(i);
                        }
                        continue;
                    }
                    let part = self.ev.parts[i].shape.clone();
                    let (new, h) = with_history(|| part.intersect(&tool.shape)).map_err(err)?;
                    let n = new.face_count();
                    if n == 0 {
                        self.ev.parts.remove(i);
                        continue;
                    }
                    let tags = propagate(&[&self.ev.parts[i].tags, &tool.tags], &h, n).0;
                    let p = &mut self.ev.parts[i];
                    p.shape = new;
                    p.tags = tags;
                }
            }
        }
        self.sync();
        Ok(())
    }

    /// Reparte índices del cuerpo (caras o aristas) por pieza: (pieza, índices locales).
    fn by_part(&self, global: &[usize], edges: bool) -> Vec<(usize, Vec<usize>)> {
        let ranges = self.ev.part_ranges();
        let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
        for &g in global {
            let Some((pi, r)) = ranges.iter().enumerate().map(|(i, (_, f, e))| (i, if edges { e } else { f })).find(|(_, r)| r.contains(&g))
            else {
                continue;
            };
            let local = g - r.start;
            match out.iter_mut().find(|(p, _)| *p == pi) {
                Some((_, v)) => {
                    if !v.contains(&local) {
                        v.push(local)
                    }
                }
                None => out.push((pi, vec![local])),
            }
        }
        out
    }

    /// Aplica `op` a cada pieza con sus índices locales (de atrás para
    /// adelante: una pieza que se vacía se puede ir sin correr las demás);
    /// `extra(k)` nombra lo generado por el k-ésimo índice de todos.
    fn per_part(
        &mut self,
        groups: Vec<(usize, Vec<usize>)>,
        op: impl Fn(&Shape, &[usize]) -> cad_occt::Result<Shape>,
        extra: impl Fn(usize) -> Option<FaceTag>,
    ) -> R<()> {
        let offsets: Vec<usize> = groups.iter().scan(0, |acc, (_, v)| {
            let o = *acc;
            *acc += v.len();
            Some(o)
        }).collect();
        let mut work: Vec<(usize, Vec<usize>, usize)> = groups.into_iter().zip(offsets).map(|((p, v), o)| (p, v, o)).collect();
        work.sort_by(|a, b| b.0.cmp(&a.0));
        for (pi, local, offset) in work {
            let part = self.ev.parts[pi].shape.clone();
            let (new, h) = with_history(|| op(&part, &local)).map_err(err)?;
            self.replace_part(pi, new, &h, |k| extra(k + offset));
        }
        self.sync();
        Ok(())
    }

    /// Booleana entre piezas ya hechas.
    fn boolean(&mut self, op: PartBoolean, targets: &[PartId], tools: &[PartId], keep_tools: bool) -> R<()> {
        if targets.is_empty() {
            return Err("elegir las piezas".into());
        }
        let t = self.find_parts("targets", targets)?;
        let tl = self.find_parts("tools", tools)?;
        let tagged = |i: usize| Tagged { shape: self.ev.parts[i].shape.clone(), tags: self.ev.parts[i].tags.clone() };
        let mut remove: Vec<usize> = Vec::new();
        match op {
            PartBoolean::Union => {
                // Todas en la primera
                let all: Vec<usize> = t.iter().chain(tl.iter()).copied().fold(Vec::new(), |mut v, i| {
                    if !v.contains(&i) {
                        v.push(i);
                    }
                    v
                });
                if all.len() < 2 {
                    return Err("hacen falta al menos dos piezas para unir".into());
                }
                let fused = fuse_tagged(all.iter().map(|&i| tagged(i)).collect())?;
                let p = &mut self.ev.parts[all[0]];
                p.shape = fused.shape;
                p.tags = fused.tags;
                remove.extend(&all[1..]);
            }
            PartBoolean::Subtract => {
                let tl: Vec<usize> = tl.into_iter().filter(|i| !t.contains(i)).collect();
                if tl.is_empty() {
                    return Err("elegir las piezas que restan".into());
                }
                let tool = fuse_tagged(tl.iter().map(|&i| tagged(i)).collect())?;
                for &i in &t {
                    let part = self.ev.parts[i].shape.clone();
                    let (new, h) = with_history(|| part.cut(&tool.shape)).map_err(err)?;
                    let n = new.face_count();
                    if n == 0 {
                        remove.push(i);
                        continue;
                    }
                    let tags = propagate(&[&self.ev.parts[i].tags, &tool.tags], &h, n).0;
                    let p = &mut self.ev.parts[i];
                    p.shape = new;
                    p.tags = tags;
                }
                if !keep_tools {
                    remove.extend(tl);
                }
            }
            PartBoolean::Intersect => {
                // Lo común a todas, en la primera
                let all: Vec<usize> = t.iter().chain(tl.iter()).copied().fold(Vec::new(), |mut v, i| {
                    if !v.contains(&i) {
                        v.push(i);
                    }
                    v
                });
                if all.len() < 2 {
                    return Err("hacen falta al menos dos piezas para intersecar".into());
                }
                let mut acc = tagged(all[0]);
                for &i in &all[1..] {
                    let other = tagged(i);
                    let (new, h) = with_history(|| acc.shape.intersect(&other.shape)).map_err(err)?;
                    let tags = propagate(&[&acc.tags, &other.tags], &h, new.face_count()).0;
                    acc = Tagged { shape: new, tags };
                }
                remove.extend(&all[1..]);
                if acc.shape.face_count() == 0 {
                    remove.push(all[0]);
                } else {
                    let p = &mut self.ev.parts[all[0]];
                    p.shape = acc.shape;
                    p.tags = acc.tags;
                }
            }
        }
        remove.sort_unstable();
        remove.dedup();
        for i in remove.into_iter().rev() {
            self.ev.parts.remove(i);
        }
        self.sync();
        Ok(())
    }

    /// Todas las piezas, cada una con su lista vacía.
    fn all_parts(&self) -> Vec<(usize, Vec<usize>)> {
        (0..self.ev.parts.len()).map(|i| (i, vec![])).collect()
    }

    fn diag(&self) -> f64 {
        self.ev.diag()
    }

    /// Anota una referencia no encontrada (una sola vez: la extrusión puede
    /// calcularse dos veces para darse vuelta).
    fn miss(&self, field: &str, index: usize) {
        let m = MissingRef { field: field.to_string(), index };
        let mut v = self.missing.borrow_mut();
        if !v.contains(&m) {
            v.push(m);
        }
    }

    /// Una referencia sola: si no está, la operación falla.
    fn face(&self, field: &str, r: &FaceRef) -> R<usize> {
        self.body()?;
        self.ev.resolve_face(r).inspect_err(|_| self.miss(field, 0))
    }

    fn edge(&self, field: &str, r: &EdgeRef) -> R<usize> {
        self.body()?;
        self.ev.resolve_edge(r).inspect_err(|_| self.miss(field, 0))
    }

    /// Una lista de referencias: se sigue con las que están (la operación
    /// queda con advertencia); si no está ninguna, falla.
    fn all<T>(&self, field: &str, noun: &str, refs: &[T], resolve: impl Fn(&T) -> R<usize>) -> R<Vec<usize>> {
        self.body()?;
        let mut found = Vec::new();
        let mut lost = 0;
        for (i, r) in refs.iter().enumerate() {
            match resolve(r) {
                Ok(x) => found.push(x),
                Err(_) => {
                    self.miss(field, i);
                    lost += 1;
                }
            }
        }
        if lost > 0 {
            if found.is_empty() {
                return Err(format!("no se encontró ninguna de las {noun} elegidas"));
            }
            self.warn(format!("faltan {lost} de {} {noun}", refs.len()));
        }
        Ok(found)
    }

    fn warn(&self, message: String) {
        let mut v = self.warnings.borrow_mut();
        if !v.contains(&message) {
            v.push(message);
        }
    }

    fn faces(&self, field: &str, refs: &[FaceRef]) -> R<Vec<usize>> {
        self.all(field, "caras", refs, |r| self.ev.resolve_face(r))
    }

    fn edges(&self, field: &str, refs: &[EdgeRef]) -> R<Vec<usize>> {
        self.all(field, "aristas", refs, |r| self.ev.resolve_edge(r))
    }

    fn plane(&self, field: &str, spec: &PlaneSpec) -> R<Plane> {
        Ok(match spec {
            PlaneSpec::Xy => Plane::XY,
            PlaneSpec::Xz => Plane::XZ,
            PlaneSpec::Yz => Plane::YZ,
            PlaneSpec::Custom { plane } => *plane,
            PlaneSpec::Reference { feature } => match self.ev.references.get(feature) {
                Some(RefGeom::Plane { plane }) => *plane,
                _ => {
                    self.miss(field, 0);
                    return Err("el plano de referencia no está calculado".into());
                }
            },
            PlaneSpec::Face { face } => {
                let i = self.face(field, face)?;
                let info = self.body()?.face_info(i).map_err(err)?;
                if info.surface != cad_occt::SurfaceKind::Plane {
                    return Err("la cara elegida no es plana".into());
                }
                // Origen = proyección del origen del mundo: estable si la cara
                // se desplaza a lo largo de su normal.
                let n = normalize(info.normal);
                Plane::from_normal(scale(n, dot(info.point, n)), n)
            }
        })
    }

    fn axis(&self, field: &str, spec: &AxisSpec) -> R<Axis> {
        Ok(match spec {
            AxisSpec::X => Axis { origin: [0.0; 3], dir: [1.0, 0.0, 0.0] },
            AxisSpec::Y => Axis { origin: [0.0; 3], dir: [0.0, 1.0, 0.0] },
            AxisSpec::Z => Axis { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] },
            AxisSpec::Custom { origin, direction } => Axis { origin: *origin, dir: *direction },
            AxisSpec::Reference { feature } => match self.ev.references.get(feature) {
                Some(RefGeom::Axis { origin, dir }) => Axis { origin: *origin, dir: *dir },
                _ => {
                    self.miss(field, 0);
                    return Err("el eje de referencia no está calculado".into());
                }
            },
            AxisSpec::SketchLine { sketch, line } => {
                let s = self.ev.sketches.get(sketch).ok_or("el sketch del eje no está calculado")?;
                let Geometry::Line { start, end } = s.sketch.entity(*line).map_err(err)?.geometry else {
                    return Err("el eje debe ser una línea".into());
                };
                let a = s.plane.to_world(s.sketch.point(start).map_err(err)?);
                let b = s.plane.to_world(s.sketch.point(end).map_err(err)?);
                Axis { origin: a, dir: normalize(sub(b, a)) }
            }
            AxisSpec::Edge { edge } => {
                let i = self.edge(field, edge)?;
                let info = self.body()?.edge_info(i).map_err(err)?;
                match info.circle {
                    Some((center, axis, _)) => Axis { origin: center, dir: axis },
                    None => Axis { origin: info.start, dir: normalize(sub(info.end, info.start)) },
                }
            }
        })
    }

    fn point(&self, field: &str, spec: &PointSpec) -> R<P3> {
        Ok(match spec {
            PointSpec::At { point } => *point,
            PointSpec::Center { edge } => {
                let i = self.edge(field, edge)?;
                let info = self.body()?.edge_info(i).map_err(err)?;
                info.circle.map_or(info.mid, |c| c.0)
            }
            PointSpec::OnEdge { edge, at } => {
                let i = self.edge(field, edge)?;
                let info = self.body()?.edge_info(i).map_err(err)?;
                if info.circle.is_some() || norm(sub(info.end, info.start)) < 1e-12 {
                    return Err("un punto sobre la arista necesita una arista recta".into());
                }
                add(info.start, scale(sub(info.end, info.start), *at))
            }
            PointSpec::Reference { feature } => match self.ev.references.get(feature) {
                Some(RefGeom::Point { point }) => *point,
                _ => {
                    self.miss(field, 0);
                    return Err("el punto de referencia no está calculado".into());
                }
            },
        })
    }

    /// Herramienta de los agujeros: por cada centro, el cilindro (con punta en
    /// los ciegos) más la caja o el avellanado; entra contra la normal del plano
    /// (o a favor, si solo de ese lado hay material).
    /// Con `src`, el filete del centro que está en su eje sale alineado con
    /// ella. Devuelve también las roscas (una por centro, si es roscado) y los
    /// centros.
    fn hole_tool(&self, id: FeatureId, h: &Hole, src: Option<&ThreadAxis>) -> R<(Tagged, Vec<ThreadAxis>, Vec<P3>)> {
        let s = self.ev.sketches.get(&h.sketch).ok_or("el sketch de los centros no está calculado")?;
        let ids: Vec<u32> = if !h.points.is_empty() {
            h.points.clone()
        } else {
            let loose: Vec<u32> = s
                .sketch
                .entities
                .iter()
                .filter_map(|e| match e.geometry {
                    Geometry::Point { point } if !e.construction => Some(point),
                    _ => None,
                })
                .collect();
            if loose.is_empty() {
                s.sketch.entities.iter().filter_map(|e| match e.geometry {
                    Geometry::Circle { center, .. } if !e.construction => Some(center),
                    _ => None,
                }).collect()
            } else {
                loose
            }
        };
        if ids.is_empty() {
            return Err("el sketch no tiene puntos ni círculos para los agujeros".into());
        }
        if h.diameter <= 0.0 {
            return Err("el diámetro tiene que ser mayor que cero".into());
        }
        let r = h.diameter / 2.0;
        let mut down = scale(normalize(s.plane.normal), -1.0);
        let x = normalize(s.plane.x_dir);
        // Un poco por encima del plano: que la herramienta no quede pegada a la cara
        let lift = (self.diag() * 1e-4).max(1e-3);
        let depth = match h.depth {
            HoleDepth::Blind { depth } if depth > 0.0 => depth,
            HoleDepth::Blind { .. } => return Err("la profundidad tiene que ser mayor que cero".into()),
            HoleDepth::ThroughAll => {
                let center = self.body().ok().and_then(|b| b.mass().ok()).map_or(s.plane.origin, |m| scale(add(m.bbox_min, m.bbox_max), 0.5));
                self.diag() * 2.0 + 2.0 * norm(sub(center, s.plane.origin))
            }
        };
        if let Some(t) = &h.modeled
            && !(t.pitch > 0.0 && t.minor() > 0.0)
        {
            return Err("la rosca necesita un paso positivo y menor que el diámetro".into());
        }
        // Con rosca modelada el taladro es el diámetro menor (más la holgura)
        let r = h.modeled.map_or(r, |t| (t.minor() + t.clearance) / 2.0);
        let centers = ids.iter().map(|pid| s.sketch.point(*pid).map(|p| s.plane.to_world(p))).collect::<Result<Vec<P3>, _>>().map_err(err)?;
        // Sin sentido elegido: si contra la normal no hay material y del otro
        // lado sí (un sketch en un plano con el sólido encima), hacia allá
        if h.reverse == Some(true) {
            down = scale(down, -1.0);
        }
        if !self.ev.parts.is_empty() {
            // Pieza por pieza: con varias, el cuerpo es un compuesto y la intersección falla
            let cuts = |dir: P3| {
                centers.iter().any(|&c| {
                    let Ok(cyl) = Shape::cylinder(Frame { origin: sub(c, scale(dir, lift)), z: dir, x }, r, depth + lift) else {
                        return false;
                    };
                    self.ev.parts.iter().any(|p| {
                        cyl.intersect(&p.shape)
                            .and_then(|i| i.mass())
                            // Más que lo que sobresale del plano (`lift`)
                            .is_ok_and(|m| m.volume > 3.0 * std::f64::consts::PI * r * r * lift)
                    })
                })
            };
            if !cuts(down) {
                if h.reverse.is_none() && cuts(scale(down, -1.0)) {
                    down = scale(down, -1.0);
                } else {
                    self.warn(NOTHING_CUT.into());
                }
            }
        }
        let bbox = self.body().ok().and_then(|b| b.mass().ok()).map(|m| (m.bbox_min, m.bbox_max));
        // La flecha, desde el primer centro hasta el fondo (o hasta donde termina el sólido)
        let c0 = centers[0];
        let (length, kind) = match (&h.depth, bbox) {
            (HoleDepth::Blind { .. }, _) => (depth, HandleKind::Blind),
            (HoleDepth::ThroughAll, Some((lo, hi))) => {
                let far = (0..8)
                    .map(|i| {
                        let corner = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
                        dot(sub(corner, c0), down)
                    })
                    .fold(0.0, f64::max);
                (far, HandleKind::Fixed)
            }
            _ => (depth, HandleKind::Fixed),
        };
        *self.handle.borrow_mut() = Some(Handle { origin: c0, dir: down, length, reversed: dot(down, s.plane.normal) > 0.0, kind });
        let mut parts = Vec::new();
        let mut probes: Vec<(P3, String)> = Vec::new();
        let mut threads = Vec::new();
        // Rosca cosmética: la medida sale de la tabla
        let cosmetic = match src {
            Some(t) => Some(ThreadSpec { clearance: 0.0, ..t.spec }),
            None => h.thread.as_deref().and_then(crate::standard::size).map(|m| ThreadSpec { nominal: m.d, pitch: m.pitch, clearance: 0.0, left: false }),
        };
        let mut aligned = false;
        for (k, &c) in centers.iter().enumerate() {
            let top = sub(c, scale(down, lift));
            let frame = |o: P3| Frame { origin: o, z: down, x };
            // La fase del filete: la de la rosca asociada si este centro está en su eje
            let hx = match src {
                Some(t) if t.coaxial(c, down) => {
                    aligned = true;
                    t.phase_x(top, down)
                }
                _ => x,
            };
            let through_len = |pitch: f64| match (&h.depth, bbox) {
                (HoleDepth::ThroughAll, Some((lo, hi))) => (0..8)
                    .map(|i| {
                        let corner = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
                        dot(sub(corner, c), down)
                    })
                    .fold(0.0, f64::max)
                    + pitch,
                _ => depth,
            };
            if let Some(spec) = h.modeled.or(cosmetic) {
                threads.push(ThreadAxis {
                    feature: id,
                    index: k as u32,
                    spec,
                    internal: true,
                    modeled: h.modeled.is_some(),
                    origin: top,
                    dir: down,
                    x: hx,
                    mouth: c,
                    out: scale(down, -1.0),
                    length: through_len(0.0),
                    blind: matches!(h.depth, HoleDepth::Blind { .. }),
                    placed: None,
                    link: h.link,
                });
            }
            match &h.modeled {
                Some(t) => {
                    // Pasante: hasta donde termina el sólido (no el largo de sobra de
                    // siempre: cada vuelta del filete cuesta)
                    let len = through_len(t.pitch);
                    let helix = Frame { origin: top, z: down, x: hx };
                    parts.push(Shape::thread(helix, r, (t.nominal + t.clearance) / 2.0, t.pitch, len + lift, t.left).map_err(err)?);
                }
                None => parts.push(Shape::cylinder(frame(top), r, depth + lift).map_err(err)?),
            }
            probes.push((add(add(c, scale(down, depth / 2.0)), scale(x, r)), format!("agujero:{k}:pared")));
            // Punta de broca en los ciegos
            if matches!(h.depth, HoleDepth::Blind { .. }) && h.tip_angle > 0.0 && h.tip_angle < 180.0 {
                let tip = r / (h.tip_angle.to_radians() / 2.0).tan();
                parts.push(Shape::cone(frame(add(c, scale(down, depth))), r, 0.0, tip).map_err(err)?);
            }
            match h.style {
                HoleStyle::Simple => {}
                HoleStyle::Counterbore { diameter, depth: d } => {
                    if diameter <= h.diameter || d <= 0.0 {
                        return Err("la caja tiene que ser más ancha que el agujero y tener profundidad".into());
                    }
                    parts.push(Shape::cylinder(frame(top), diameter / 2.0, d + lift).map_err(err)?);
                    probes.push((add(add(c, scale(down, d / 2.0)), scale(x, diameter / 2.0)), format!("agujero:{k}:caja")));
                }
                HoleStyle::Countersink { diameter, angle } => {
                    if diameter <= h.diameter || angle <= 0.0 || angle >= 180.0 {
                        return Err("el avellanado tiene que ser más ancho que el agujero, con ángulo entre 0 y 180°".into());
                    }
                    let height = (diameter - h.diameter) / 2.0 / (angle.to_radians() / 2.0).tan();
                    // Desde un poco más arriba, para que el borde quede justo en la superficie
                    let extra = lift * (angle.to_radians() / 2.0).tan();
                    parts.push(Shape::cone(frame(top), diameter / 2.0 + extra, r, height + lift).map_err(err)?);
                }
            }
        }
        if let Some(t) = src
            && !aligned
        {
            self.miss("link", 0);
            self.warn(format!("ningún centro está en el eje de la rosca {}: se tomó solo la medida", spec_name(&t.spec)));
        }
        let shape = fuse(parts)?;
        let mut tags = vec![Vec::new(); shape.face_count()];
        for (p, name) in probes {
            mark(&shape, &mut tags, p, tag(id, name));
        }
        for (i, slot) in tags.iter_mut().enumerate() {
            if slot.is_empty() {
                slot.push(tag(id, format!("cara:{i}")));
            }
        }
        Ok((Tagged { shape, tags }, threads, centers))
    }

    /// Marco y forma de una primitiva. Un tornillo o una tuerca asociados a
    /// otra rosca van en su eje, desde su boca, con su medida y el filete
    /// girado para calzar; también devuelve el paso y la mano de la rosca.
    fn place_standard(&self, p: &Primitive) -> R<(Frame, PrimitiveShape, Threading)> {
        let frame = Frame { origin: p.origin, z: p.z, x: p.x };
        let Some(link) = &p.link else {
            return Ok((frame, p.shape.clone(), Threading::default()));
        };
        let t = self.thread_source(link)?;
        let name = spec_name(&t.spec);
        let m = crate::standard::size_by_nominal(t.spec.nominal).ok_or_else(|| format!("no hay piezas estándar para la rosca {name}"))?;
        let (mouth, out) = t.entry(link.flip);
        let origin = sub(mouth, scale(out, link.offset));
        let mut shape = p.shape.clone();
        let (z, helix) = match &mut shape {
            PrimitiveShape::Bolt { size, length, .. } => {
                if !t.internal {
                    return Err(format!("un tornillo va en una rosca interior (agujero o tuerca), no en {name} exterior"));
                }
                *size = m.name.to_string();
                let f = Frame { origin, z: out, x: default_x(out) };
                (out, crate::standard::bolt_helix(f, *length, None))
            }
            PrimitiveShape::Nut { size, .. } => {
                if t.internal {
                    return Err(format!("una tuerca va en una rosca exterior (tornillo o eje), no en {name} interior"));
                }
                *size = m.name.to_string();
                let z = scale(out, -1.0);
                (z, crate::standard::nut_helix(Frame { origin, z, x: default_x(z) }, t.spec.pitch, None))
            }
            _ => return Err("solo los tornillos y las tuercas se asocian a una rosca".into()),
        };
        let x = t.phase_x(helix[0], helix[1]);
        Ok((Frame { origin, z, x }, shape, Threading { pitch: Some(t.spec.pitch), left: t.spec.left, x: None }))
    }

    /// La rosca a la que apunta un vínculo (tiene que estar antes en el historial).
    fn thread_source(&self, link: &ThreadLink) -> R<ThreadAxis> {
        if let Some(t) = self.ev.threads.iter().find(|t| t.feature == link.feature && t.index == link.index) {
            return Ok(t.clone());
        }
        let Some(f) = self.doc.get(link.feature) else {
            return Err("la rosca asociada ya no existe".into());
        };
        let name = &f.name;
        Err(match self.ev.state(link.feature) {
            None => format!("{name} está después en el historial: la rosca asociada tiene que ir antes"),
            Some(FeatureState::Ok | FeatureState::Warning { .. }) => format!("{name} no tiene esa rosca"),
            Some(_) => format!("{name} no está calculada: sin ella no hay rosca que seguir"),
        })
    }

    /// Lleva cada entidad usada a su arista proyectada en el plano del sketch
    /// (si la arista ya no está, queda donde estaba y la operación avisa).
    fn project_uses(&self, plane: &Plane, sketch: &mut Sketch) {
        let Ok(body) = self.body().cloned() else {
            self.warn("las aristas usadas necesitan un sólido antes del sketch".into());
            return;
        };
        let mut lost = 0;
        for (k, u) in sketch.uses.clone().iter().enumerate() {
            let Ok(i) = self.ev.resolve_edge(&u.edge) else {
                self.miss("uses", k);
                lost += 1;
                continue;
            };
            let (Ok(info), Ok(e)) = (body.edge_info(i), sketch.entity(u.entity)) else { continue };
            let local = |p: P3| plane.to_local(p);
            match e.geometry.clone() {
                Geometry::Line { start, end } => {
                    let _ = sketch.set_point(start, local(info.start));
                    let _ = sketch.set_point(end, local(info.end));
                }
                Geometry::Circle { center, .. } => {
                    if let Some((c, _, r)) = info.circle {
                        let _ = sketch.set_point(center, local(c));
                        if let Some(Geometry::Circle { radius, .. }) = sketch.entities.iter_mut().find(|x| x.id == u.entity).map(|x| &mut x.geometry) {
                            *radius = r;
                        }
                    }
                }
                Geometry::Arc { center, start, end } => {
                    if let Some((c, _, _)) = info.circle {
                        // Los arcos del sketch van antihorario: si la arista gira al revés
                        // en este plano, se dan vuelta sus extremos
                        let (c, a, m, b) = (local(c), local(info.start), local(info.mid), local(info.end));
                        let ccw = (a[0] - c[0]) * (m[1] - c[1]) - (a[1] - c[1]) * (m[0] - c[0]) > 0.0;
                        let (a, b) = if ccw { (a, b) } else { (b, a) };
                        let _ = sketch.set_point(center, c);
                        let _ = sketch.set_point(start, a);
                        let _ = sketch.set_point(end, b);
                    }
                }
                _ => {}
            }
        }
        if lost > 0 {
            self.warn(format!("faltan {lost} de {} aristas usadas", sketch.uses.len()));
        }
    }

    /// Pared del nervio: por cada línea del sketch, rayos en el plano hasta el
    /// sólido; el polígono entre la línea y lo que tocan, con espesor centrado.
    fn rib_tool(&self, id: FeatureId, sketch: FeatureId, thickness: f64, flip: bool) -> R<Tagged> {
        if thickness <= 0.0 {
            return Err("el espesor tiene que ser mayor que cero".into());
        }
        let s = self.ev.sketches.get(&sketch).ok_or("el sketch del nervio no está calculado")?;
        let body = self.body()?.clone();
        let n = normalize(s.plane.normal);
        // Un poco adentro del sólido, para que la unión no quede apenas tocando
        let overlap = (self.diag() * 1e-4).max(1e-3);
        let mut walls = Vec::new();
        for e in &s.sketch.entities {
            let Geometry::Line { start, end } = e.geometry else { continue };
            if e.construction {
                continue;
            }
            let a = s.plane.to_world(s.sketch.point(start).map_err(err)?);
            let b = s.plane.to_world(s.sketch.point(end).map_err(err)?);
            let len = norm(sub(b, a));
            if len < 1e-9 {
                continue;
            }
            let side = normalize(cross(n, sub(b, a)));
            let sides = if flip { [scale(side, -1.0), side] } else { [side, scale(side, -1.0)] };
            // Muestras a lo largo de la línea (las puntas, apenas adentro)
            const N: usize = 32;
            let inset = (len * 1e-4).max(1e-6);
            let samples: Vec<P3> = (0..=N)
                .map(|k| {
                    let t = inset + (len - 2.0 * inset) * k as f64 / N as f64;
                    add(a, scale(sub(b, a), t / len))
                })
                .collect();
            let hits = sides.iter().find_map(|&dir| {
                let ts: Option<Vec<f64>> = samples.iter().map(|&p| body.ray_hit(p, dir)).collect();
                ts.map(|ts| (dir, ts))
            });
            let Some((dir, ts)) = hits else {
                return Err("el nervio no llega al sólido en todo el largo de la línea".into());
            };
            // El polígono va de punta a punta (los rayos salen apenas adentro)
            let at = |k: usize| add(a, scale(sub(b, a), k as f64 / N as f64));
            let mut poly = vec![a, b];
            poly.extend(ts.iter().enumerate().rev().map(|(k, &t)| add(at(k), scale(dir, t + overlap))));
            let face = Shape::polygon(&poly).map_err(err)?;
            let wall = face
                .translate(scale(n, -thickness / 2.0))
                .and_then(|f| f.prism(scale(n, thickness)))
                .map_err(err)?;
            walls.push(wall);
        }
        if walls.is_empty() {
            return Err("el sketch no tiene líneas para el nervio".into());
        }
        let shape = fuse(walls)?;
        let tags = (0..shape.face_count()).map(|i| vec![tag(id, format!("cara:{i}"))]).collect();
        Ok(Tagged { shape, tags })
    }

    /// Alambre del camino de un barrido: las entidades encadenadas por sus
    /// extremos, empezando por la punta más cercana a `near` (el perfil: el
    /// barrido arranca en el comienzo del alambre).
    fn path_wire(&self, path: &SweepPath, near: Option<P3>) -> R<Shape> {
        let (sketch, entities) = match path {
            SweepPath::Sketch { sketch, entities } => (sketch, entities),
            SweepPath::Curve { feature } => {
                return self.ev.curves.get(feature).cloned().ok_or_else(|| {
                    self.miss("path", 0);
                    "la curva del camino no está calculada".to_string()
                });
            }
        };
        let s = self.ev.sketches.get(sketch).ok_or("el sketch del camino no está calculado")?;
        let ends = |id: u32| -> Option<(u32, u32)> {
            match &s.sketch.entity(id).ok()?.geometry {
                Geometry::Line { start, end } => Some((*start, *end)),
                Geometry::Arc { start, end, .. } => Some((*start, *end)),
                Geometry::Spline { points, closed: false, .. } => Some((*points.first()?, *points.last()?)),
                _ => None,
            }
        };
        let ids: Vec<u32> = if entities.is_empty() {
            s.sketch.entities.iter().filter(|e| !e.construction && ends(e.id).is_some()).map(|e| e.id).collect()
        } else {
            entities.clone()
        };
        if ids.is_empty() {
            return Err("el camino no tiene líneas, arcos ni splines".into());
        }
        let mut left: Vec<(u32, (u32, u32))> = ids.iter().map(|&i| ends(i).map(|e| (i, e)).ok_or_else(|| "el camino tiene que ser de líneas, arcos o splines abiertas".to_string())).collect::<R<_>>()?;
        // Arranque: una entidad con un extremo que no comparte con nadie
        let degree = |p: u32, l: &[(u32, (u32, u32))]| l.iter().filter(|(_, (a, b))| *a == p || *b == p).count();
        let first = left
            .iter()
            .position(|(_, (a, b))| degree(*a, &left) == 1 || degree(*b, &left) == 1)
            .unwrap_or(0);
        let (id, (a, b)) = left.remove(first);
        let reversed = degree(a, &left) > 0 && degree(b, &left) == 0;
        let mut pieces = vec![LoopPiece { entity: id, reversed }];
        let mut tip = if reversed { a } else { b };
        while let Some(k) = left.iter().position(|(_, (a, b))| *a == tip || *b == tip) {
            let (id, (a, b)) = left.remove(k);
            let reversed = b == tip;
            pieces.push(LoopPiece { entity: id, reversed });
            tip = if reversed { a } else { b };
        }
        if !left.is_empty() {
            return Err("el camino no es continuo".into());
        }
        let start = if pieces[0].reversed { ends(pieces[0].entity).unwrap().1 } else { ends(pieces[0].entity).unwrap().0 };
        let world = |p: u32| s.sketch.point(p).ok().map(|q| s.plane.to_world(q));
        if let (Some(near), Some(a), Some(b)) = (near, world(start), world(tip))
            && norm(sub(b, near)) < norm(sub(a, near))
        {
            pieces.reverse();
            for p in &mut pieces {
                p.reversed = !p.reversed;
            }
        }
        let curves = loop_curves(&s.sketch, &s.plane, &Loop { pieces, polygon: Vec::new(), area: 0.0 })?;
        Shape::wire(&curves).map_err(err)
    }

    /// Alambres de las curvas de un sketch (vacío = todas las que no son de
    /// construcción): cada curva cerrada sola, y las abiertas encadenadas por
    /// sus extremos (cada cadena, un alambre).
    fn curve_wires(&self, sketch: FeatureId, entities: &[u32]) -> R<Vec<Shape>> {
        let s = self.ev.sketches.get(&sketch).ok_or("el sketch no está calculado")?;
        let ends = |id: u32| -> Option<(u32, u32)> {
            match &s.sketch.entity(id).ok()?.geometry {
                Geometry::Line { start, end } => Some((*start, *end)),
                Geometry::Arc { start, end, .. } => Some((*start, *end)),
                Geometry::Spline { points, closed: false, .. } => Some((*points.first()?, *points.last()?)),
                _ => None,
            }
        };
        let closed = |id: u32| {
            matches!(
                s.sketch.entity(id).map(|e| &e.geometry),
                Ok(Geometry::Circle { .. } | Geometry::Ellipse { .. } | Geometry::Spline { closed: true, .. })
            )
        };
        let ids: Vec<u32> = if entities.is_empty() {
            s.sketch.entities.iter().filter(|e| !e.construction && (ends(e.id).is_some() || closed(e.id))).map(|e| e.id).collect()
        } else {
            entities.to_vec()
        };
        if ids.is_empty() {
            return Err("el sketch no tiene curvas".into());
        }
        let mut chains: Vec<Vec<LoopPiece>> = Vec::new();
        let mut left: Vec<(u32, (u32, u32))> = Vec::new();
        for &id in &ids {
            if closed(id) {
                chains.push(vec![LoopPiece { entity: id, reversed: false }]);
            } else {
                left.push((id, ends(id).ok_or("las curvas tienen que ser líneas, arcos, círculos, elipses o splines")?));
            }
        }
        let degree = |p: u32, l: &[(u32, (u32, u32))]| l.iter().filter(|(_, (a, b))| *a == p || *b == p).count();
        while !left.is_empty() {
            // Desde un extremo suelto si lo hay (si no, la cadena es un lazo)
            let first = left.iter().position(|(_, (a, b))| degree(*a, &left) == 1 || degree(*b, &left) == 1).unwrap_or(0);
            let (id, (a, b)) = left.remove(first);
            let reversed = degree(a, &left) > 0 && degree(b, &left) == 0;
            let mut pieces = vec![LoopPiece { entity: id, reversed }];
            let mut tip = if reversed { a } else { b };
            while let Some(k) = left.iter().position(|(_, (a, b))| *a == tip || *b == tip) {
                let (id, (a, b)) = left.remove(k);
                let reversed = b == tip;
                pieces.push(LoopPiece { entity: id, reversed });
                tip = if reversed { a } else { b };
            }
            chains.push(pieces);
        }
        chains
            .into_iter()
            .map(|pieces| {
                let curves = loop_curves(&s.sketch, &s.plane, &Loop { pieces, polygon: Vec::new(), area: 0.0 })?;
                Shape::wire(&curves).map_err(err)
            })
            .collect()
    }

    /// Superficies de una operación, juntas en una pieza, con un origen por cara.
    fn surface_tool(&self, id: FeatureId, mut shapes: Vec<Shape>) -> R<Tagged> {
        let shape = match shapes.len() {
            0 => return Err("no quedó ninguna superficie".into()),
            1 => shapes.pop().unwrap(),
            _ => Shape::compound(&shapes).map_err(err)?,
        };
        let tags = (0..shape.face_count()).map(|k| vec![tag(id, format!("cara:{k}"))]).collect();
        Ok(Tagged { shape, tags })
    }

    fn reference_plane(&self, def: &PlaneDef) -> R<Plane> {
        Ok(match def {
            PlaneDef::Offset { base, distance } => self.plane("base", base)?.offset(*distance),
            PlaneDef::Angle { base, axis, angle } => {
                let p = self.plane("base", base)?;
                let ax = self.axis("axis", axis)?;
                let m = cad_occt::rotation_matrix(ax, angle.to_radians());
                let dir = |v: P3| [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]);
                let pt = |v: P3| [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3]);
                Plane { origin: pt(p.origin), normal: normalize(dir(p.normal)), x_dir: normalize(dir(p.x_dir)) }
            }
            PlaneDef::Midplane { a, b } => {
                let (pa, pb) = (self.plane("a", a)?, self.plane("b", b)?);
                if dot(normalize(pa.normal), normalize(pb.normal)).abs() < 1.0 - 1e-6 {
                    return Err("los planos no son paralelos".into());
                }
                // A mitad de camino a lo largo de la normal del primero
                let n = normalize(pa.normal);
                let d = dot(sub(pb.origin, pa.origin), n);
                Plane { origin: add(pa.origin, scale(n, d / 2.0)), ..pa }
            }
            PlaneDef::ThreePoints { points } => {
                let [a, b, c] = [0, 1, 2].map(|k| self.point("points", &points[k]));
                let (a, b, c) = (a?, b?, c?);
                let n = cross(sub(b, a), sub(c, a));
                if norm(n) < 1e-9 * norm(sub(b, a)).max(1.0).powi(2) {
                    return Err("los tres puntos están alineados".into());
                }
                Plane { origin: a, normal: normalize(n), x_dir: normalize(sub(b, a)) }
            }
        })
    }

    fn reference_axis(&self, def: &AxisDef) -> R<Axis> {
        Ok(match def {
            AxisDef::TwoPoints { a, b } => {
                let (a, b) = (self.point("a", a)?, self.point("b", b)?);
                if norm(sub(b, a)) < 1e-9 {
                    return Err("los dos puntos coinciden".into());
                }
                Axis { origin: a, dir: normalize(sub(b, a)) }
            }
            AxisDef::Edge { edge } => self.axis("edge", &AxisSpec::Edge { edge: edge.clone() })?,
            AxisDef::Face { face } => {
                let i = self.face("face", face)?;
                let info = self.body()?.face_info(i).map_err(err)?;
                info.axis.ok_or("la cara no es cilíndrica ni cónica")?
            }
            AxisDef::Planes { a, b } => {
                let (pa, pb) = (self.plane("a", a)?, self.plane("b", b)?);
                let (n1, n2) = (normalize(pa.normal), normalize(pb.normal));
                let dir = cross(n1, n2);
                let l2 = dot(dir, dir);
                if l2 < 1e-12 {
                    return Err("los planos son paralelos".into());
                }
                // Un punto de los dos: (d1 (n2 × dir) + d2 (dir × n1)) / |dir|²
                let (d1, d2) = (dot(n1, pa.origin), dot(n2, pb.origin));
                let p = scale(add(scale(cross(n2, dir), d1), scale(cross(dir, n1), d2)), 1.0 / l2);
                Axis { origin: p, dir: normalize(dir) }
            }
        })
    }

    fn feature(&mut self, f: &Feature) -> R<()> {
        match &f.kind {
            FeatureKind::Sketch { plane, offset, sketch } => {
                let plane = self.plane("plane", plane)?.offset(*offset);
                let mut solved = sketch.clone();
                if !solved.uses.is_empty() {
                    self.project_uses(&plane, &mut solved);
                }
                let report = solved.solve().map_err(err)?;
                let regions = find_regions(&solved).map_err(err)?;
                self.ev.sketches.insert(f.id, SketchResult { plane, sketch: solved, report, regions });
                Ok(())
            }
            FeatureKind::Extrude(e) => {
                let tool = self.extrude(f.id, e)?;
                self.ev.tools.insert(f.id, (tool.clone(), e.op));
                self.apply(f.id, tool, e.op)
            }
            FeatureKind::Revolve(r) => {
                let axis = self.axis("axis", &r.axis)?;
                let (faces, entities, samples) = self.profile(r.sketch, &r.regions)?;
                let angle = r.angle.to_radians();
                if angle.abs() < 1e-9 {
                    return Err("ángulo cero".into());
                }
                let solids = faces.iter().map(|fc| fc.revolve(axis, angle)).collect::<Result<Vec<_>, _>>().map_err(err)?;
                let shape = fuse(solids)?;
                let mut tags = vec![Vec::new(); shape.face_count()];
                let rot = |p: P3, a: f64| {
                    let m = cad_occt::rotation_matrix(axis, a);
                    [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
                };
                // Lateral de cada entidad: su punto medio girado a la mitad del ángulo
                for (id, p) in &entities {
                    mark(&shape, &mut tags, rot(*p, angle / 2.0), tag(f.id, format!("lado:{id}")));
                }
                if angle.abs() < 2.0 * std::f64::consts::PI - 1e-9 {
                    for p in &samples {
                        mark(&shape, &mut tags, *p, tag(f.id, "inicio"));
                        mark(&shape, &mut tags, rot(*p, angle), tag(f.id, "fin"));
                    }
                }
                let tool = Tagged { shape, tags };
                self.ev.tools.insert(f.id, (tool.clone(), r.op));
                self.apply(f.id, tool, r.op)
            }
            FeatureKind::Primitive(p) => {
                let (frame, shape, threading) = self.place_standard(p)?;
                let tool = match &shape {
                    PrimitiveShape::Box { dx, dy, dz, centered, centered_z } => {
                        let (dx, dy, dz, centered, centered_z) = (*dx, *dy, *dz, *centered, *centered_z);
                        let frame = if centered { box_centered(frame, dx, dy) } else { frame };
                        let frame = if centered_z { Frame { origin: sub(frame.origin, scale(normalize(frame.z), dz / 2.0)), ..frame } } else { frame };
                        Shape::make_box(frame, dx, dy, dz)
                    }
                    PrimitiveShape::Cylinder { radius, height } => Shape::cylinder(frame, *radius, *height),
                    PrimitiveShape::Cone { r1, r2, height } => Shape::cone(frame, *r1, *r2, *height),
                    PrimitiveShape::Sphere { radius } => Shape::sphere(p.origin, *radius),
                    PrimitiveShape::Torus { major, minor } => Shape::torus(frame, *major, *minor),
                    PrimitiveShape::Bolt { size, length, head, modeled } => {
                        Ok(crate::standard::bolt_with(frame, size, *length, *head, modeled.then_some(threading))?)
                    }
                    PrimitiveShape::Nut { size, modeled } => Ok(crate::standard::nut_with(frame, size, modeled.then_some(threading))?),
                    PrimitiveShape::Washer { size } => Ok(crate::standard::washer(frame, size)?),
                }
                .map_err(err)?;
                let tool = Tagged { tags: primitive_tags(f.id, p, &tool), shape: tool };
                self.ev.tools.insert(f.id, (tool.clone(), p.op));
                self.apply(f.id, tool, p.op)?;
                self.ev.threads.extend(standard_thread(f.id, &shape, frame, threading, p.link));
                Ok(())
            }
            FeatureKind::Import { format, data, op } => {
                let tool = match format {
                    ImportFormat::Step => Shape::from_step(data),
                    ImportFormat::Brep => Shape::from_brep(data),
                }
                .map_err(err)?;
                let tags = (0..tool.face_count()).map(|i| vec![tag(f.id, format!("cara:{i}"))]).collect();
                let tool = Tagged { shape: tool, tags };
                self.ev.tools.insert(f.id, (tool.clone(), *op));
                self.apply(f.id, tool, *op)
            }
            FeatureKind::Fillet { edges, radius, radius2 } => {
                if edges.is_empty() {
                    return Err("elegir al menos una arista".into());
                }
                let idx = self.edges("edges", edges)?;
                let groups = self.by_part(&idx, true);
                let end = radius2.unwrap_or(*radius);
                self.per_part(groups, |s, e| s.fillet_variable(e, *radius, end), |k| Some(tag(f.id, format!("redondeo:{k}"))))
            }
            FeatureKind::Chamfer { edges, distance, second } => {
                if edges.is_empty() {
                    return Err("elegir al menos una arista".into());
                }
                let idx = self.edges("edges", edges)?;
                let groups = self.by_part(&idx, true);
                let second = *second;
                self.per_part(
                    groups,
                    |s, e| match second {
                        None => s.chamfer(e, *distance),
                        Some(ChamferSecond::Distance { distance: d2, flip }) => s.chamfer_two(e, *distance, d2, flip),
                        Some(ChamferSecond::Angle { degrees, flip }) => s.chamfer_angle(e, *distance, degrees, flip),
                    },
                    |k| Some(tag(f.id, format!("chaflan:{k}"))),
                )
            }
            FeatureKind::Shell { faces, thickness } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara para abrir".into());
                }
                let idx = self.faces("faces", faces)?;
                // Solo las piezas de las caras elegidas
                let groups = self.by_part(&idx, false);
                self.per_part(groups, |s, fs| s.shell(fs, -thickness.abs()), |_| None)
            }
            FeatureKind::Draft { faces, neutral, angle } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara".into());
                }
                let idx = self.faces("faces", faces)?;
                let p = self.plane("neutral", neutral)?;
                let groups = self.by_part(&idx, false);
                self.per_part(groups, |s, fs| s.draft(fs, p.normal, angle.to_radians(), p.origin, p.normal), |_| None)
            }
            FeatureKind::Pattern { features, pattern } => {
                // Dónde está lo que se copia: el centro de la primera operación (o del sólido)
                let seed = match features.first() {
                    Some(id) => self.tool(*id)?.0.shape.mass().map_err(err)?,
                    None => self.body()?.mass().map_err(err)?,
                };
                let seed = scale(add(seed.bbox_min, seed.bbox_max), 0.5);
                let transforms = self.pattern_transforms(pattern, seed)?;
                if transforms.is_empty() {
                    return Ok(());
                }
                let copies_of = |src: &Tagged| -> R<Vec<Tagged>> {
                    transforms
                        .iter()
                        .enumerate()
                        .map(|(k, t)| {
                            let (shape, h) = with_history(|| t(&src.shape).map_err(occt_err)).map_err(err)?;
                            let tags = suffixed(&src.tags, &h, shape.face_count(), &format!("#{}", k + 1));
                            Ok(Tagged { shape, tags })
                        })
                        .collect()
                };
                if features.is_empty() {
                    // Todo el sólido: cada pieza con sus copias
                    self.body()?;
                    for i in 0..self.ev.parts.len() {
                        let part = Tagged { shape: self.ev.parts[i].shape.clone(), tags: self.ev.parts[i].tags.clone() };
                        let mut all = vec![part.clone()];
                        all.extend(copies_of(&part)?);
                        let fused = fuse_tagged(all)?;
                        self.ev.parts[i].shape = fused.shape;
                        self.ev.parts[i].tags = fused.tags;
                    }
                    self.sync();
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    self.apply(f.id, fuse_tagged(copies_of(&tool)?)?, op)?;
                }
                Ok(())
            }
            FeatureKind::Mirror { features, plane } => {
                let p = self.plane("plane", plane)?;
                let mirrored = |src: &Tagged| -> R<Tagged> {
                    let (shape, h) = with_history(|| src.shape.mirror(p.origin, p.normal)).map_err(err)?;
                    let tags = suffixed(&src.tags, &h, shape.face_count(), "#espejo");
                    Ok(Tagged { shape, tags })
                };
                if features.is_empty() {
                    // Todo el sólido: cada pieza con su reflejo
                    self.body()?;
                    for i in 0..self.ev.parts.len() {
                        let part = Tagged { shape: self.ev.parts[i].shape.clone(), tags: self.ev.parts[i].tags.clone() };
                        let m = mirrored(&part)?;
                        let fused = fuse_tagged(vec![part, m])?;
                        self.ev.parts[i].shape = fused.shape;
                        self.ev.parts[i].tags = fused.tags;
                    }
                    self.sync();
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    let m = mirrored(&tool)?;
                    self.apply(f.id, m, op)?;
                }
                Ok(())
            }
            FeatureKind::Boolean { op, targets, tools, keep_tools } => self.boolean(*op, targets, tools, *keep_tools),
            FeatureKind::Sweep(sw) => {
                let (faces, _, samples) = self.profile(sw.sketch, &sw.regions)?;
                let spine = self.path_wire(&sw.path, samples.first().copied())?;
                let solids = faces.iter().map(|fc| fc.sweep(&spine)).collect::<Result<Vec<_>, _>>().map_err(err)?;
                let shape = fuse(solids)?;
                // La tapa del perfil; el resto por su número (no hay más origen estable)
                let mut tags = vec![Vec::new(); shape.face_count()];
                for p in &samples {
                    mark(&shape, &mut tags, *p, tag(f.id, "inicio"));
                }
                for (i, slot) in tags.iter_mut().enumerate() {
                    if slot.is_empty() {
                        slot.push(tag(f.id, format!("cara:{i}")));
                    }
                }
                let tool = Tagged { shape, tags };
                self.ev.tools.insert(f.id, (tool.clone(), sw.op));
                self.apply(f.id, tool, sw.op)
            }
            FeatureKind::Helix { axis, radius, pitch, turns, left } => {
                let ax = self.axis("axis", axis)?;
                let ax = Axis { origin: ax.origin, dir: normalize(ax.dir) };
                let wire = Shape::helix(ax, *radius, *pitch, *turns, *left).map_err(err)?;
                // Polilínea para el visor: la del teselado del alambre mismo
                let points: Vec<P3> = wire.tessellate(0.05, 0.1).map_err(err)?.edges.concat();
                self.ev.references.insert(f.id, RefGeom::Curve { points });
                self.ev.curves.insert(f.id, wire);
                Ok(())
            }
            FeatureKind::MoveFace { faces, distance } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara".into());
                }
                if distance.abs() < 1e-9 {
                    return Err("distancia cero".into());
                }
                let idx = self.faces("faces", faces)?;
                let body = self.body()?.clone();
                // Cada cara: el prisma que barre al moverse, sumado o restado de su pieza
                let mut jobs = Vec::new();
                for &i in &idx {
                    let info = body.face_info(i).map_err(err)?;
                    if info.surface != SurfaceKind::Plane {
                        return Err("por ahora solo se mueven caras planas".into());
                    }
                    let part = self.ev.part_of_face(i).ok_or("la cara no es de ninguna pieza")?;
                    let n = normalize(info.normal);
                    let prism = body.face_shape(i).and_then(|fc| fc.prism(scale(n, *distance))).map_err(err)?;
                    jobs.push((part, prism, i));
                }
                let saved = std::mem::take(&mut self.scope);
                for (k, (part, prism, _)) in jobs.into_iter().enumerate() {
                    let tags = vec![vec![tag(f.id, format!("cara:{k}"))]; prism.face_count()];
                    self.scope = vec![part];
                    let op = if *distance > 0.0 { BodyOp::Join } else { BodyOp::Cut };
                    let r = self.apply(f.id, Tagged { shape: prism, tags }, op);
                    if r.is_err() {
                        self.scope = saved;
                        return r;
                    }
                }
                self.scope = saved;
                Ok(())
            }
            FeatureKind::Scale { factor, center } => {
                if factor.iter().any(|v| v.abs() < 1e-12) {
                    return Err("la escala no puede ser cero".into());
                }
                self.body()?;
                let c = self.point("center", center)?;
                let m = [
                    [factor[0], 0.0, 0.0, c[0] - factor[0] * c[0]],
                    [0.0, factor[1], 0.0, c[1] - factor[1] * c[1]],
                    [0.0, 0.0, factor[2], c[2] - factor[2] * c[2]],
                ];
                let which: Vec<usize> = if self.scope.is_empty() {
                    (0..self.ev.parts.len()).collect()
                } else {
                    self.find_parts("scope", &self.scope.clone())?
                };
                for i in which {
                    let part = self.ev.parts[i].shape.clone();
                    let (new, h) = with_history(|| part.transform(m)).map_err(err)?;
                    self.replace_part(i, new, &h, |_| None);
                }
                self.sync();
                Ok(())
            }
            FeatureKind::Thicken { faces, thickness, op } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara".into());
                }
                let idx = self.faces("faces", faces)?;
                let mut solids = Vec::new();
                // Las caras de una superficie, cosidas antes: así las esquinas se
                // engrosan juntas; las de un sólido, cada una
                for (p, local) in self.by_part(&idx, false) {
                    let part = &self.ev.parts[p].shape;
                    let shapes = local.iter().map(|&i| part.face_shape(i)).collect::<Result<Vec<_>, _>>().map_err(err)?;
                    if self.ev.parts[p].is_surface() && shapes.len() > 1 {
                        let shell = Shape::sew(&shapes, 1e-6, false).map_err(err)?;
                        solids.push(shell.thicken(*thickness).map_err(err)?);
                    } else {
                        for fc in shapes {
                            solids.push(fc.thicken(*thickness).map_err(err)?);
                        }
                    }
                }
                let shape = fuse(solids)?;
                let tags = (0..shape.face_count()).map(|i| vec![tag(f.id, format!("cara:{i}"))]).collect();
                let tool = Tagged { shape, tags };
                self.ev.tools.insert(f.id, (tool.clone(), *op));
                self.apply(f.id, tool, *op)
            }
            FeatureKind::Rib { sketch, thickness, flip } => {
                let tool = self.rib_tool(f.id, *sketch, *thickness, *flip)?;
                self.ev.tools.insert(f.id, (tool.clone(), BodyOp::Join));
                self.apply(f.id, tool, BodyOp::Join)
            }
            FeatureKind::ReplaceFace { faces, target } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara".into());
                }
                let idx = self.faces("faces", faces)?;
                let t = self.plane("target", target)?;
                let tn = normalize(t.normal);
                let body = self.body()?.clone();
                let far = self.diag() * 4.0;
                let mut jobs = Vec::new();
                for &i in &idx {
                    let info = body.face_info(i).map_err(err)?;
                    if info.surface != SurfaceKind::Plane {
                        return Err("por ahora solo se reemplazan caras planas".into());
                    }
                    let n = normalize(info.normal);
                    let along = dot(n, tn);
                    if along.abs() < 1e-6 {
                        return Err("el plano es paralelo a la dirección de la cara: no la corta".into());
                    }
                    // Cuánto hay que moverla (en su centro) para llegar al plano
                    let d = dot(sub(t.origin, info.point), tn) / along;
                    if d.abs() < 1e-9 {
                        continue;
                    }
                    // La cara barrida de sobra hacia el plano, recortada en él (del lado de la cara)
                    let keep = if dot(sub(info.point, t.origin), tn) >= 0.0 { tn } else { scale(tn, -1.0) };
                    let prism = body
                        .face_shape(i)
                        .and_then(|fc| fc.prism(scale(n, far.copysign(d))))
                        .and_then(|p| p.split_keep(t.origin, keep))
                        .map_err(err)?;
                    let part = self.ev.part_of_face(i).ok_or("la cara no es de ninguna pieza")?;
                    jobs.push((part, prism, d > 0.0));
                }
                let saved = std::mem::take(&mut self.scope);
                for (k, (part, prism, out)) in jobs.into_iter().enumerate() {
                    let tags = vec![vec![tag(f.id, format!("cara:{k}"))]; prism.face_count()];
                    self.scope = vec![part];
                    let r = self.apply(f.id, Tagged { shape: prism, tags }, if out { BodyOp::Join } else { BodyOp::Cut });
                    if r.is_err() {
                        self.scope = saved;
                        return r;
                    }
                }
                self.scope = saved;
                Ok(())
            }
            FeatureKind::Thread { face, pitch, length, flip, left, clearance, link } => {
                let src = link.as_ref().map(|l| self.thread_source(l)).transpose()?;
                let (pitch, left) = match &src {
                    Some(t) => (&t.spec.pitch, &t.spec.left),
                    None => (pitch, left),
                };
                let i = self.faces("face", std::slice::from_ref(face))?[0];
                let body = self.body()?.clone();
                let info = body.face_info(i).map_err(err)?;
                let (Some(ax), Some(radius)) = (info.axis.filter(|_| info.surface == SurfaceKind::Cylinder), info.radius) else {
                    return Err("la rosca va sobre una cara cilíndrica".into());
                };
                if *pitch <= 0.0 {
                    return Err("el paso tiene que ser mayor que cero".into());
                }
                let dir = normalize(ax.dir);
                // Hasta dónde llega la cara a lo largo del eje
                let along: Vec<f64> = body.face_shape(i).and_then(|fc| fc.vertices()).map_err(err)?.iter().map(|v| dot(sub(*v, ax.origin), dir)).collect();
                let (lo, hi) = along.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| (a.min(t), b.max(t)));
                if !(hi > lo) {
                    return Err("no se pudo medir el largo de la cara".into());
                }
                let len = if *length > 0.0 { length.min(hi - lo) } else { hi - lo };
                let (start, dir) = if *flip { (add(ax.origin, scale(dir, hi)), scale(dir, -1.0)) } else { (add(ax.origin, scale(dir, lo)), dir) };
                // ¿Eje o agujero? La normal saliente apunta afuera del eje en un eje
                let foot = add(ax.origin, scale(normalize(ax.dir), dot(sub(info.point, ax.origin), normalize(ax.dir))));
                let outside = dot(info.normal, sub(info.point, foot)) > 0.0;
                let h = 1.082_532 * pitch / 2.0;
                let margin = (self.diag() * 1e-4).max(1e-3);
                // Un poco más allá de cada punta: con las tapas justo en el mismo
                // plano que las del cilindro la booleana fallaba sin avisar
                let from = sub(start, scale(dir, margin));
                let span = len + 2.0 * margin;
                // Diámetro nominal: el de la rosca asociada o el que da la cara
                let nominal = match &src {
                    Some(t) => t.spec.nominal,
                    None if outside => 2.0 * radius,
                    None => 2.0 * (radius + h),
                };
                if let Some(t) = &src {
                    let name = spec_name(&t.spec);
                    if !t.coaxial(from, dir) {
                        return Err(format!("la cara no está en el eje de la rosca {name}"));
                    }
                    if outside && radius < (nominal / 2.0 - h) + margin {
                        return Err(format!("el eje (Ø{:.2}) es más fino que el núcleo de la rosca {name}", 2.0 * radius));
                    }
                    if !outside && radius > nominal / 2.0 + clearance / 2.0 - margin {
                        return Err(format!("el agujero (Ø{:.2}) es más ancho que la rosca {name}", 2.0 * radius));
                    }
                }
                // La fase: la de la rosca asociada, para que calcen
                let x = match &src {
                    Some(t) => t.phase_x(from, dir),
                    None => default_x(dir),
                };
                let helix = Frame { origin: from, z: dir, x };
                let tool = if outside {
                    // Eje: se quita todo lo que no es filete. Sin holgura y con la cara
                    // justo en el diámetro mayor, la cresta queda apenas afuera (sobre
                    // la cara misma la booleana no resuelve superficies que coinciden)
                    // y la cara del cilindro hace de cresta
                    let major = nominal / 2.0 - clearance / 2.0;
                    let crest = if radius - major > 2.0 * margin { major } else { radius + margin };
                    let rod = Shape::thread(helix, major - h, crest, *pitch, span, *left).map_err(err)?;
                    let sleeve = Shape::cylinder(Frame { origin: from, z: dir, x }, radius + pitch, span).map_err(err)?;
                    sleeve.cut(&rod).map_err(err)?
                } else {
                    // Agujero: el macho talla desde el diámetro menor (más la holgura)
                    let minor = nominal / 2.0 - h + clearance / 2.0;
                    Shape::thread(helix, minor - margin, minor + h, *pitch, span, *left).map_err(err)?
                };
                let thread = ThreadAxis {
                    feature: f.id,
                    index: 0,
                    spec: ThreadSpec { nominal, pitch: *pitch, clearance: *clearance, left: *left },
                    internal: !outside,
                    modeled: true,
                    origin: from,
                    dir,
                    x,
                    mouth: start,
                    out: scale(dir, -1.0),
                    length: len,
                    blind: false,
                    placed: None,
                    link: *link,
                };
                let part = self.ev.part_of_face(i).ok_or("la cara no es de ninguna pieza")?;
                let tags = (0..tool.face_count()).map(|k| vec![tag(f.id, format!("cara:{k}"))]).collect();
                let tool = Tagged { shape: tool, tags };
                self.ev.tools.insert(f.id, (tool.clone(), BodyOp::Cut));
                let saved = std::mem::replace(&mut self.scope, vec![part]);
                let r = self.apply(f.id, tool, BodyOp::Cut);
                self.scope = saved;
                r?;
                self.ev.threads.push(thread);
                Ok(())
            }
            FeatureKind::SheetMetal { sketch, regions, thickness, radius, k_factor, flip, op } => {
                if *thickness <= 0.0 {
                    return Err("el espesor tiene que ser mayor que cero".into());
                }
                if *radius < 0.0 || !(0.0..=1.0).contains(k_factor) {
                    return Err("el radio no puede ser negativo y el factor K va de 0 a 1".into());
                }
                let e = Extrude {
                    sketch: *sketch,
                    regions: regions.clone(),
                    extent: Extent::Blind { distance: *thickness },
                    reverse: *flip,
                    op: *op,
                    draft: 0.0,
                    thin: None,
                };
                let tool = self.extrude_dir(f.id, &e, *flip)?;
                self.ev.tools.insert(f.id, (tool.clone(), *op));
                self.apply(f.id, tool, *op)
            }
            FeatureKind::Flange { edge, length, angle, flip, radius, relief, relief_width, corner_relief, corner_size, closed_corner, corner_gap } => {
                let edge = edge.as_ref().ok_or("elegir la arista del borde de la chapa")?;
                let (tool, part, g) = self.flange_tool(f.id, edge, *length, *angle, *flip, *radius)?;
                // Esquinas cerradas: las extensiones de las paredes van con la pestaña
                let tool = if *closed_corner {
                    let extra = self.closed_corners(part, &g, corner_gap.unwrap_or(0.2))?;
                    if extra.is_empty() {
                        tool
                    } else {
                        let mut all = vec![tool.shape.clone()];
                        all.extend(extra);
                        let shape = Shape::fuse_all(&all).map_err(err)?;
                        let tags = (0..shape.face_count()).map(|k| vec![tag(f.id, format!("pestaña:{k}"))]).collect();
                        Tagged { shape, tags }
                    }
                } else {
                    tool
                };
                // Con la chapa de antes: dónde sigue el material al lado del doblez
                let cut = self.flange_reliefs(f.id, part, &g, *relief, *relief_width, *corner_relief, *corner_size)?;
                // El alivio se resta antes de unir, de la chapa y de la pestaña: unidas
                // primero, dos dobleces que se tocan en una línea dan un sólido roto
                let tool = match &cut {
                    Some(c) => {
                        let (shape, h) = with_history(|| tool.shape.cut(&c.shape)).map_err(err)?;
                        let tags = propagate(&[tool.tags.as_slice(), c.tags.as_slice()], &h, shape.face_count()).0;
                        Tagged { shape, tags }
                    }
                    None => tool,
                };
                self.ev.tools.insert(f.id, (tool.clone(), BodyOp::Join));
                let saved = std::mem::replace(&mut self.scope, vec![part]);
                let mut r = Ok(());
                if let Some(cut) = cut {
                    r = self.apply(f.id, cut, BodyOp::Cut);
                }
                if r.is_ok() {
                    r = self.apply(f.id, tool, BodyOp::Join);
                }
                self.scope = saved;
                r?;
                self.ev.bends.push(BendInfo {
                    feature: f.id,
                    part,
                    start: g.start,
                    end: g.end,
                    normal: g.n,
                    thickness: g.t,
                    bend: g.b,
                    outward: g.m,
                    radius: g.r,
                    angle: g.angle,
                    length: g.length,
                });
                Ok(())
            }
            FeatureKind::SurfaceExtrude { sketch, entities, extent, reverse } => {
                let plane = self.ev.sketches.get(sketch).ok_or("el sketch no está calculado")?.plane;
                let n = if *reverse { scale(plane.normal, -1.0) } else { plane.normal };
                let (from, len) = match extent {
                    Extent::Blind { distance } => (0.0, *distance),
                    Extent::Symmetric { distance } => (-distance / 2.0, *distance),
                    Extent::TwoSides { distance, second } => (-second, distance + second),
                    _ => return Err("una superficie se extruye ciega, simétrica o a los dos lados".into()),
                };
                if len.abs() < 1e-9 {
                    return Err("la distancia tiene que ser mayor que cero".into());
                }
                let mut shapes = Vec::new();
                for w in self.curve_wires(*sketch, entities)? {
                    let w = if from.abs() > 0.0 { w.translate(scale(n, from)).map_err(err)? } else { w };
                    shapes.push(w.prism(scale(n, len)).map_err(err)?);
                }
                let tool = self.surface_tool(f.id, shapes)?;
                self.apply(f.id, tool, BodyOp::New)
            }
            FeatureKind::SurfaceRevolve { sketch, entities, axis, angle } => {
                let axis = self.axis("axis", axis)?;
                let a = angle.to_radians();
                if a.abs() < 1e-9 {
                    return Err("ángulo cero".into());
                }
                let shapes = self
                    .curve_wires(*sketch, entities)?
                    .iter()
                    .map(|w| w.revolve(axis, a))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(err)?;
                let tool = self.surface_tool(f.id, shapes)?;
                self.apply(f.id, tool, BodyOp::New)
            }
            FeatureKind::Fill { edges, tangent } => {
                if edges.is_empty() {
                    return Err("elegir las aristas del borde".into());
                }
                let idx = self.edges("edges", edges)?;
                let body = self.body()?.clone();
                let mut boundary = Vec::new();
                for &i in &idx {
                    let e = body.edge_shape(i).map_err(err)?;
                    // Tangente solo al borde de una superficie (una cara); en un
                    // sólido la arista tiene dos y no se sabe a cuál
                    let face = match body.edge_faces(i).map_err(err)?.as_slice() {
                        [only] if *tangent => Some(body.face_shape(*only).map_err(err)?),
                        _ => None,
                    };
                    boundary.push((e, face));
                }
                let shape = Shape::fill(&boundary, &[]).map_err(err)?;
                let tool = self.surface_tool(f.id, vec![shape])?;
                self.apply(f.id, tool, BodyOp::New)
            }
            FeatureKind::SplitBy { parts, tool } => {
                self.body()?;
                // La herramienta: el plano como una cara grande que cruza todo, o la otra pieza
                let (tool_shape, tool_index) = match tool {
                    SplitTool::Plane { plane } => {
                        let p = self.plane("plane", plane)?;
                        let m = self.body()?.mass().map_err(err)?;
                        let c = scale(add(m.bbox_min, m.bbox_max), 0.5);
                        let size = 2.0 * norm(sub(m.bbox_max, m.bbox_min)) + 1.0;
                        let n = normalize(p.normal);
                        let o = sub(c, scale(n, dot(sub(c, p.origin), n)));
                        let (x, y) = (normalize(p.x_dir), normalize(p.y_dir()));
                        let at = |a: f64, b: f64| add(add(o, scale(x, a)), scale(y, b));
                        (Shape::polygon(&[at(-size, -size), at(size, -size), at(size, size), at(-size, size)]).map_err(err)?, None)
                    }
                    SplitTool::Part { part } => {
                        let i = self.find_parts("tool", std::slice::from_ref(part))?[0];
                        (self.ev.parts[i].shape.clone(), Some(i))
                    }
                };
                let which: Vec<usize> = if parts.is_empty() {
                    (0..self.ev.parts.len()).filter(|&i| Some(i) != tool_index && !self.ev.parts[i].is_surface()).collect()
                } else {
                    self.find_parts("parts", parts)?
                };
                let mut added = 0u32;
                for &i in which.iter().rev() {
                    if self.ev.parts[i].is_surface() {
                        self.warn("las superficies se recortan con Cortar o con una booleana".into());
                        continue;
                    }
                    let whole = self.ev.parts[i].shape.clone();
                    let (split, h) = with_history(|| whole.split_by(std::slice::from_ref(&tool_shape))).map_err(err)?;
                    let cut_tags: Vec<Vec<FaceTag>> = (0..tool_shape.face_count()).map(|_| vec![tag(f.id, "corte")]).collect();
                    let tags = propagate(&[&self.ev.parts[i].tags, &cut_tags], &h, split.face_count()).0;
                    let solids = split.solids().map_err(err)?;
                    if solids.len() < 2 {
                        continue;
                    }
                    let pieces: Vec<Tagged> = solids
                        .iter()
                        .map(|s| {
                            let idx = split.face_indices_of(s).map_err(err)?;
                            Ok(Tagged { tags: idx.iter().map(|k| k.and_then(|k| tags.get(k).cloned()).unwrap_or_default()).collect(), shape: s.clone() })
                        })
                        .collect::<R<_>>()?;
                    let mut it = pieces.into_iter();
                    let first = it.next().unwrap();
                    self.ev.parts[i].shape = first.shape;
                    self.ev.parts[i].tags = first.tags;
                    for t in it {
                        self.ev.parts.push(Part { id: PartId { feature: f.id, index: added }, shape: t.shape, tags: t.tags });
                        added += 1;
                    }
                }
                if added == 0 {
                    self.warn("no partió ninguna pieza: la herramienta no la cruza".into());
                }
                self.sync();
                Ok(())
            }
            FeatureKind::Sew { parts, solid, tolerance } => {
                if parts.len() < 2 && !*solid {
                    return Err("elegir al menos dos superficies".into());
                }
                let mut which = self.find_parts("parts", parts)?;
                let shapes: Vec<Shape> = which.iter().map(|&i| self.ev.parts[i].shape.clone()).collect();
                let shape = Shape::sew(&shapes, tolerance.max(1e-7), *solid).map_err(err)?;
                which.sort_unstable();
                for &i in which.iter().rev() {
                    self.ev.parts.remove(i);
                }
                let tags = (0..shape.face_count()).map(|k| vec![tag(f.id, format!("cara:{k}"))]).collect();
                self.new_part(f.id, Tagged { shape, tags });
                self.sync();
                Ok(())
            }
            FeatureKind::Hole(h) => {
                // Asociado a otra rosca: su medida (la holgura es la propia)
                let src = h.link.as_ref().map(|l| self.thread_source(l)).transpose()?;
                let mut h = h.clone();
                if let Some(t) = &src {
                    let spec = ThreadSpec { clearance: h.modeled.map_or(0.0, |m| m.clearance), ..t.spec };
                    if h.modeled.is_some() {
                        h.modeled = Some(spec);
                    } else {
                        // Cosmética: la broca para roscar
                        h.thread = Some(spec_name(&spec));
                        h.diameter = spec.nominal - spec.pitch;
                    }
                }
                let (tool, threads, centers) = self.hole_tool(f.id, &h, src.as_ref())?;
                self.ev.tools.insert(f.id, (tool.clone(), BodyOp::Cut));
                // Hacia afuera del material no se aplica: rasparía lo que sobresale del plano
                if self.warnings.borrow().iter().any(|w| w == NOTHING_CUT) {
                    return Ok(());
                }
                // Sin alcance elegido: solo la pieza más cercana a cada centro
                let saved = self.scope.clone();
                if self.scope.is_empty() {
                    self.scope = self.nearest_parts(&tool.shape, &centers);
                    self.auto_scope = self.scope.clone();
                }
                let r = self.apply(f.id, tool, BodyOp::Cut);
                self.scope = saved;
                r?;
                self.ev.threads.extend(threads);
                Ok(())
            }
            FeatureKind::Loft(l) => {
                if l.sections.len() < 2 {
                    return Err("hacen falta al menos dos secciones".into());
                }
                let mut faces = Vec::new();
                let mut samples = Vec::new();
                for s in &l.sections {
                    let (fs, _, sm) = self.profile(s.sketch, &s.regions)?;
                    if fs.len() != 1 {
                        return Err("cada sección tiene que ser una sola región".into());
                    }
                    faces.extend(fs);
                    samples.push(sm);
                }
                let shape = Shape::loft(&faces, true, l.ruled).map_err(err)?;
                let mut tags = vec![Vec::new(); shape.face_count()];
                for p in &samples[0] {
                    mark(&shape, &mut tags, *p, tag(f.id, "inicio"));
                }
                for p in samples.last().unwrap() {
                    mark(&shape, &mut tags, *p, tag(f.id, "fin"));
                }
                for (i, slot) in tags.iter_mut().enumerate() {
                    if slot.is_empty() {
                        slot.push(tag(f.id, format!("cara:{i}")));
                    }
                }
                let tool = Tagged { shape, tags };
                self.ev.tools.insert(f.id, (tool.clone(), l.op));
                self.apply(f.id, tool, l.op)
            }
            FeatureKind::Plane { def } => {
                let plane = self.reference_plane(def)?;
                self.ev.references.insert(f.id, RefGeom::Plane { plane });
                Ok(())
            }
            FeatureKind::Axis { def } => {
                let a = self.reference_axis(def)?;
                self.ev.references.insert(f.id, RefGeom::Axis { origin: a.origin, dir: a.dir });
                Ok(())
            }
            FeatureKind::Point { def } => {
                let point = self.point("def", def)?;
                self.ev.references.insert(f.id, RefGeom::Point { point });
                Ok(())
            }
            FeatureKind::SplitParts { parts } => {
                self.body()?;
                let which = if parts.is_empty() { (0..self.ev.parts.len()).collect() } else { self.find_parts("parts", parts)? };
                // De atrás para adelante: las piezas nuevas van al final
                let mut added = 0u32;
                for &i in which.iter().rev() {
                    let whole = self.ev.parts[i].shape.clone();
                    let solids = whole.solids().map_err(err)?;
                    if solids.len() < 2 {
                        continue;
                    }
                    let tags_of = |s: &Shape| -> R<Vec<Vec<FaceTag>>> {
                        let idx = whole.face_indices_of(s).map_err(err)?;
                        Ok(idx.iter().map(|k| k.and_then(|k| self.ev.parts[i].tags.get(k).cloned()).unwrap_or_default()).collect())
                    };
                    let pieces: Vec<Tagged> = solids.iter().map(|s| Ok(Tagged { tags: tags_of(s)?, shape: s.clone() })).collect::<R<_>>()?;
                    let mut it = pieces.into_iter();
                    let first = it.next().unwrap();
                    self.ev.parts[i].shape = first.shape;
                    self.ev.parts[i].tags = first.tags;
                    for t in it {
                        self.ev.parts.push(Part { id: PartId { feature: f.id, index: added }, shape: t.shape, tags: t.tags });
                        added += 1;
                    }
                }
                if added == 0 {
                    self.warn("no había sólidos sueltos para separar".into());
                }
                self.sync();
                Ok(())
            }
            FeatureKind::DeleteParts { parts } => {
                if parts.is_empty() {
                    return Err("elegir al menos una pieza".into());
                }
                let mut which = self.find_parts("parts", parts)?;
                which.sort_unstable();
                for i in which.into_iter().rev() {
                    self.ev.parts.remove(i);
                }
                self.sync();
                Ok(())
            }
            FeatureKind::Split { plane, flip } => {
                let p = self.plane("plane", plane)?;
                let n = if *flip { scale(p.normal, -1.0) } else { p.normal };
                self.body()?;
                // Después de las caras de la pieza vienen las del semiespacio: el corte
                let groups = self.all_parts();
                self.per_part(groups, |s, _| s.split_keep(p.origin, n), |_| Some(tag(f.id, "corte")))
            }
        }
    }

    fn tool(&self, id: FeatureId) -> R<(Tagged, BodyOp)> {
        self.ev
            .tools
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("{} no se calculó o no genera un sólido", self.name(id)))
    }

    fn name(&self, id: FeatureId) -> String {
        self.doc.get(id).map_or_else(|| format!("{id:?}"), |f| format!("«{}»", f.name))
    }

    fn pattern_transforms(&self, p: &PatternKind, seed: P3) -> R<Vec<ShapeFn>> {
        let mut out: Vec<ShapeFn> = Vec::new();
        match p {
            PatternKind::Linear { direction, count, spacing } => {
                if *count < 2 {
                    return Err("se necesitan al menos 2 instancias".into());
                }
                let d = normalize(*direction);
                for k in 1..*count {
                    let v = scale(d, spacing * k as f64);
                    out.push(Box::new(move |s: &Shape| s.translate(v).map_err(err)));
                }
            }
            PatternKind::Curve { path, count } => {
                if *count < 2 {
                    return Err("se necesitan al menos 2 instancias".into());
                }
                let wire = self.path_wire(path, None)?;
                let pts = wire.sample_curve(*count as usize).map_err(err)?;
                let p0 = pts[0].0;
                for (p, _) in pts.into_iter().skip(1) {
                    let v = sub(p, p0);
                    out.push(Box::new(move |s: &Shape| s.translate(v).map_err(err)));
                }
            }
            PatternKind::Table { offsets } => {
                if offsets.is_empty() {
                    return Err("la tabla no tiene filas".into());
                }
                for &v in offsets {
                    out.push(Box::new(move |s: &Shape| s.translate(v).map_err(err)));
                }
            }
            PatternKind::Fill { sketch, regions, spacing, hex, margin } => {
                if *spacing <= 0.0 {
                    return Err("la separación tiene que ser mayor que cero".into());
                }
                let s = self.ev.sketches.get(sketch).ok_or("el sketch no está calculado")?;
                let regions = self.selected_regions(s, regions)?;
                if regions.is_empty() {
                    return Err("el sketch no tiene regiones cerradas".into());
                }
                let c = s.plane.to_local(seed);
                // Distancia al borde (contorno y agujeros) de la región que lo contiene
                let seg = |p: P2, a: P2, b: P2| {
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let l2 = dx * dx + dy * dy;
                    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
                };
                let inside = |p: P2| {
                    regions.iter().any(|r| {
                        r.contains(p)
                            && std::iter::once(&r.outer).chain(&r.holes).all(|l| {
                                let n = l.polygon.len();
                                (0..n).all(|i| seg(p, l.polygon[i], l.polygon[(i + 1) % n]) >= *margin)
                            })
                    })
                };
                let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                for r in &regions {
                    for q in &r.outer.polygon {
                        for k in 0..2 {
                            lo[k] = lo[k].min(q[k]);
                            hi[k] = hi[k].max(q[k]);
                        }
                    }
                }
                let row = if *hex { spacing * 3f64.sqrt() / 2.0 } else { *spacing };
                let (j0, j1) = (((lo[1] - c[1]) / row).floor() as i64, ((hi[1] - c[1]) / row).ceil() as i64);
                let (i0, i1) = (((lo[0] - c[0]) / spacing).floor() as i64 - 1, ((hi[0] - c[0]) / spacing).ceil() as i64 + 1);
                if ((j1 - j0 + 1) * (i1 - i0 + 1)) > 200_000 {
                    return Err("demasiadas copias: subir la separación".into());
                }
                let at = s.plane.to_world(c);
                for j in j0..=j1 {
                    let shift = if *hex && j.rem_euclid(2) == 1 { spacing / 2.0 } else { 0.0 };
                    for i in i0..=i1 {
                        if i == 0 && j == 0 {
                            continue;
                        }
                        let p = [c[0] + i as f64 * spacing + shift, c[1] + j as f64 * row];
                        if inside(p) {
                            let v = sub(s.plane.to_world(p), at);
                            out.push(Box::new(move |s: &Shape| s.translate(v).map_err(err)));
                        }
                    }
                }
                if out.len() > 2000 {
                    return Err(format!("demasiadas copias ({}): subir la separación", out.len()));
                }
                if out.is_empty() {
                    self.warn("no entra ninguna copia en la región".into());
                }
            }
            PatternKind::Circular { axis, count, angle } => {
                if *count < 2 {
                    return Err("se necesitan al menos 2 instancias".into());
                }
                let ax = self.axis("axis", axis)?;
                let full = (angle.abs() - 360.0).abs() < 1e-9;
                let step = if full { angle / *count as f64 } else { angle / (*count - 1) as f64 };
                for k in 1..*count {
                    let a = (step * k as f64).to_radians();
                    out.push(Box::new(move |s: &Shape| s.rotate(ax, a).map_err(err)));
                }
            }
        }
        Ok(out)
    }

    /// Extrusión. Si resta o interseca y hacia ese lado no toca el sólido (un
    /// bolsillo dibujado sobre una cara apunta hacia afuera), se da vuelta sola.
    /// Chapa de la que sale una operación: la última antes de `id`.
    fn sheet_params(&self, id: FeatureId) -> Option<(f64, f64, f64)> {
        let end = self.doc.index_of(id)?;
        self.doc.features[..end].iter().rev().find_map(|f| match &f.kind {
            FeatureKind::SheetMetal { thickness, radius, k_factor, .. } if !f.suppressed => Some((*thickness, *radius, *k_factor)),
            _ => None,
        })
    }

    /// Doblez y pared de una pestaña desde la arista `edge` (recta, entre la
    /// cara grande de la chapa y su canto). Devuelve la herramienta y la pieza.
    fn flange_tool(&self, id: FeatureId, edge: &EdgeRef, length: f64, angle: f64, flip: bool, radius: Option<f64>) -> R<(Tagged, PartId, FlangeGeom)> {
        if !(angle > 0.0 && angle < 180.0) {
            return Err("el ángulo va entre 0 y 180°".into());
        }
        if length < 0.0 {
            return Err("el largo no puede ser negativo".into());
        }
        let i = self.edge("edge", edge)?;
        let body = self.body()?;
        let info = body.edge_info(i).map_err(err)?;
        if info.curve != cad_occt::CurveKind::Line {
            return Err("la pestaña sale de una arista recta".into());
        }
        let dir = normalize(sub(info.end, info.start));
        let faces: Vec<(usize, cad_occt::FaceInfo)> =
            body.edge_faces(i).map_err(err)?.into_iter().filter_map(|k| body.face_info(k).ok().map(|x| (k, x))).collect();
        if faces.len() != 2 || faces.iter().any(|(_, x)| x.surface != cad_occt::SurfaceKind::Plane) {
            return Err("la arista tiene que estar entre dos caras planas de la chapa".into());
        }
        // El canto es la cara angosta (su ancho es el espesor)
        let width = |x: &cad_occt::FaceInfo| x.area / info.length.max(1e-9);
        let (side, top) = if width(&faces[0].1) <= width(&faces[1].1) { (&faces[0], &faces[1]) } else { (&faces[1], &faces[0]) };
        let t = self.sheet_params(id).map(|p| p.0).unwrap_or_else(|| width(&side.1));
        let r = radius.or(self.sheet_params(id).map(|p| p.1)).unwrap_or(t);
        if r < 0.0 {
            return Err("el radio no puede ser negativo".into());
        }
        let n = normalize(top.1.normal);
        let m = normalize(side.1.normal);
        if dot(n, m).abs() > 1e-3 || dot(n, dir).abs() > 1e-3 || dot(m, dir).abs() > 1e-3 {
            return Err("la arista tiene que ser de un canto recto de la chapa".into());
        }
        // Hacia dónde dobla: el lado de la cara de la arista, o el otro
        let b = if flip { scale(n, -1.0) } else { n };
        // El eje queda a un radio del borde del canto de ese lado
        let edge_on_b = if flip { sub(info.start, scale(n, t)) } else { info.start };
        let axis = Axis { origin: add(edge_on_b, scale(b, r)), dir };
        // Girar el canto del lado del material (−b) hacia afuera (m)
        let sign = if dot(cross(dir, scale(b, -1.0)), m) >= 0.0 { 1.0 } else { -1.0 };
        let a = sign * angle.to_radians();
        // El canto a lo largo de la arista y a través del espesor. No la cara del
        // cuerpo: junto a otra pestaña se funde con la tapa de su doblez (son
        // coplanares) y se revolucionaría todo eso
        let below = scale(n, -t);
        let face = Shape::polygon(&[info.start, info.end, add(info.end, below), add(info.start, below)]).map_err(err)?;
        let bend = face.revolve(axis, a).map_err(err)?;
        let shape = if length > 0.0 {
            let rot = cad_occt::rotation_matrix(axis, a);
            let d = [0, 1, 2].map(|k| rot[k][0] * m[0] + rot[k][1] * m[1] + rot[k][2] * m[2]);
            let wall = face.rotate(axis, a).map_err(err)?.prism(scale(d, length)).map_err(err)?;
            bend.union(&wall).map_err(err)?
        } else {
            bend
        };
        let part = self.ev.part_of_face(side.0).ok_or("la arista no es de ninguna pieza")?;
        let tags = (0..shape.face_count()).map(|k| vec![tag(id, format!("pestaña:{k}"))]).collect();
        let g = FlangeGeom { start: info.start, end: info.end, dir, n, m, t, r, b, angle, length };
        Ok((Tagged { shape, tags }, part, g))
    }

    /// Esquinas cerradas: en cada extremo que se junta con otra pestaña de la
    /// misma pieza (las dos a 90°, hacia el mismo lado, mismo espesor y radio),
    /// la extensión de esta pared hasta tapar el canto de la otra y la de la
    /// otra hasta quedar a `gap` de esta. Con otras pestañas, aviso.
    fn closed_corners(&self, part: PartId, g: &FlangeGeom, gap: f64) -> R<Vec<Shape>> {
        let tol = 1e-6 * (1.0 + norm(sub(g.end, g.start)));
        let right = |a: f64| (a - 90.0).abs() < 1e-6;
        let mut out = Vec::new();
        // La pared ocupa, desde la arista del lado del doblez, [R, R + t] hacia
        // afuera del canto y [R, R + largo] hacia donde dobla
        let base = |e: P3, b: P3, n: P3, t: f64| if dot(b, n) > 0.0 { e } else { sub(e, scale(n, t)) };
        let slab = |p0: P3, a: P3, a0: f64, a1: f64, c: P3, c0: f64, c1: f64, h: P3, h0: f64, h1: f64| -> R<Shape> {
            let at = |x: f64, y: f64| add(add(add(p0, scale(a, x)), scale(c, y)), scale(h, h0));
            Shape::polygon(&[at(a0, c0), at(a1, c0), at(a1, c1), at(a0, c1)]).map_err(err)?.prism(scale(h, h1 - h0)).map_err(err)
        };
        for (e, sigma) in [(g.start, -1.0), (g.end, 1.0)] {
            let u = scale(g.dir, sigma);
            for other in self.ev.bends.iter().filter(|b| b.part == part) {
                let Some(ue) = [(other.start, -1.0), (other.end, 1.0)]
                    .iter()
                    .find(|(p, _)| norm(sub(*p, e)) < tol)
                    .map(|(_, s)| scale(normalize(sub(other.end, other.start)), *s))
                else {
                    continue;
                };
                // Pestañas en ángulo recto entre sí: el extremo de cada una apunta afuera de la otra
                if dot(u, other.outward) < 0.99 || dot(ue, g.m) < 0.99 {
                    continue;
                }
                if !right(g.angle) || !right(other.angle) || dot(g.b, other.bend) < 0.999 || (g.t - other.thickness).abs() > tol || (g.r - other.radius).abs() > tol {
                    self.warn("la esquina cerrada sale solo con dos pestañas a 90° hacia el mismo lado, del mismo espesor y radio".into());
                    continue;
                }
                let (r, t) = (g.r, g.t);
                let p0 = base(e, g.b, g.n, t);
                // Esta: hasta tapar el canto de la otra
                out.push(slab(p0, g.m, r, r + t, u, 0.0, r + t, g.b, r, r + g.length)?);
                // La otra: hasta quedar a la holgura de esta
                if r - gap > 1e-6 {
                    out.push(slab(p0, other.outward, r, r + t, ue, 0.0, r - gap, g.b, r, r + other.length)?);
                }
            }
        }
        Ok(out)
    }

    /// Alivios de una pestaña, para restar después de unirla: en cada extremo,
    /// si se junta con el doblez de otra pestaña, el alivio de esquina
    /// (centrado en el vértice, a través del espesor); si no, y la chapa sigue
    /// al lado del doblez (esquina hacia adentro), una ranura desde el canto
    /// hasta pasado el doblez. Sin alivios que hacer, `None`.
    #[allow(clippy::too_many_arguments)]
    fn flange_reliefs(
        &self,
        id: FeatureId,
        part: PartId,
        g: &FlangeGeom,
        relief: BendRelief,
        relief_width: Option<f64>,
        corner: CornerRelief,
        corner_size: Option<f64>,
    ) -> R<Option<Tagged>> {
        let plate = &self.ev.parts.iter().find(|p| p.id == part).ok_or("la pieza de la chapa no está")?.shape;
        let scale_len = norm(sub(g.end, g.start)).max(g.t);
        let tol = 1e-6 * (1.0 + scale_len);
        let eps = (g.t * 0.05).max(1e-3);
        // Prisma a través del espesor (de un poco arriba de la cara de la arista
        // a un poco abajo de la otra) desde un contorno en el plano de la cara
        let through = |pts: &[P3]| -> R<Shape> {
            let lifted: Vec<P3> = pts.iter().map(|p| add(*p, scale(g.n, eps))).collect();
            Shape::polygon(&lifted).map_err(err)?.prism(scale(g.n, -(g.t + 2.0 * eps))).map_err(err)
        };
        // `seam`: hacia dónde queda la costura del cilindro (en un plano de una
        // cara, la booleana da un sólido al revés)
        let disk = |c: P3, radius: f64, seam: P3| -> R<Shape> {
            let frame = Frame { origin: add(c, scale(g.n, eps)), z: scale(g.n, -1.0), x: seam };
            Shape::cylinder(frame, radius, g.t + 2.0 * eps).map_err(err)
        };
        let mut cuts: Vec<Shape> = Vec::new();
        for (e, sigma) in [(g.start, -1.0), (g.end, 1.0)] {
            let u = scale(g.dir, sigma);
            // ¿Termina en el doblez de otra pestaña (no paralela) de la misma pieza?
            let meets = self.ev.bends.iter().any(|b| {
                b.part == part
                    && b.feature != id
                    && dot(normalize(sub(b.end, b.start)), g.dir).abs() < 0.99
                    && [b.start, b.end].iter().any(|p| {
                        let d = sub(*p, e);
                        let along = dot(d, g.n);
                        norm(sub(d, scale(g.n, along))) < tol && along.abs() <= g.t + tol
                    })
            });
            if meets {
                let rho = corner_size.unwrap_or(g.t);
                if rho <= 0.0 && corner != CornerRelief::None {
                    return Err("el alivio de esquina tiene que ser mayor que cero".into());
                }
                match corner {
                    CornerRelief::None => {}
                    // Costura hacia el hueco de la esquina (pasado el extremo, afuera del canto)
                    CornerRelief::Round => cuts.push(disk(e, rho, normalize(add(u, g.m)))?),
                    CornerRelief::Square => {
                        let (a, b) = (scale(g.dir, rho), scale(g.m, rho));
                        cuts.push(through(&[sub(sub(e, a), b), sub(add(e, a), b), add(add(e, a), b), add(sub(e, a), b)])?);
                    }
                }
                continue;
            }
            if relief == BendRelief::None {
                continue;
            }
            let w = relief_width.unwrap_or(g.t);
            if w <= 0.0 {
                return Err("el ancho del alivio tiene que ser mayor que cero".into());
            }
            // Material al lado del doblez: un cubito pasado el extremo, afuera del canto
            let d = (g.t.min(w) * 0.25).max(1e-4);
            let probe = add(add(add(e, scale(g.n, -g.t / 2.0)), scale(u, d)), scale(g.m, d));
            let frame = Frame { origin: sub(probe, [d / 4.0; 3]), ..Frame::WORLD };
            let cube = Shape::make_box(frame, d / 2.0, d / 2.0, d / 2.0).map_err(err)?;
            let touches = plate.intersect(&cube).ok().and_then(|s| s.mass().ok()).is_some_and(|m| m.volume > 1e-12);
            if !touches {
                continue;
            }
            // Desde un poco adentro del canto hasta pasado el doblez (R + t)
            let (inner, outer) = (-w / 2.0, g.r + g.t + w / 2.0);
            let at = |a: f64, b: f64| add(add(e, scale(u, a)), scale(g.m, b));
            match relief {
                BendRelief::Rectangle => cuts.push(through(&[at(0.0, inner), at(w, inner), at(w, outer), at(0.0, outer)])?),
                BendRelief::Obround => {
                    let end = outer - w / 2.0;
                    let slot = through(&[at(0.0, inner), at(w, inner), at(w, end), at(0.0, end)])?;
                    cuts.push(slot.union(&disk(at(w / 2.0, end), w / 2.0, g.m)?).map_err(err)?);
                }
                BendRelief::None => {}
            }
        }
        if cuts.is_empty() {
            return Ok(None);
        }
        let shape = if cuts.len() == 1 { cuts.pop().unwrap() } else { Shape::fuse_all(&cuts).map_err(err)? };
        let tags = (0..shape.face_count()).map(|k| vec![tag(id, format!("alivio:{k}"))]).collect();
        Ok(Some(Tagged { shape, tags }))
    }

    fn extrude(&self, id: FeatureId, e: &Extrude) -> R<Tagged> {
        let tool = self.extrude_dir(id, e, e.reverse)?;
        let auto_flip = matches!(e.op, BodyOp::Cut | BodyOp::Intersect)
            && matches!(e.extent, Extent::Blind { .. } | Extent::ThroughAll);
        if auto_flip
            && let Some(body) = &self.ev.body
            && body.intersect(&tool.shape).ok().and_then(|s| s.mass().ok()).is_none_or(|m| m.volume.abs() < 1e-9)
        {
            return self.extrude_dir(id, e, !e.reverse);
        }
        Ok(tool)
    }

    fn extrude_dir(&self, id: FeatureId, e: &Extrude, reverse: bool) -> R<Tagged> {
        let plane = self.ev.sketches.get(&e.sketch).ok_or("el sketch no está calculado")?.plane;
        let (faces, entities, samples) = self.profile(e.sketch, &e.regions)?;
        // Delgada: el anillo entre el contorno desplazado hacia afuera y hacia adentro
        let faces = match e.thin {
            Some(t) if t > 0.0 => faces
                .iter()
                .map(|f| {
                    let outer = f.offset_face(t / 2.0).map_err(err)?;
                    match f.offset_face(-t / 2.0) {
                        Ok(inner) => outer.cut(&inner).map_err(err),
                        // Más angosto que la pared: queda lleno
                        Err(_) => Ok(outer),
                    }
                })
                .collect::<R<Vec<_>>>()?,
            Some(_) => return Err("el espesor tiene que ser mayor que cero".into()),
            None => faces,
        };
        let mut n = if reverse { scale(plane.normal, -1.0) } else { plane.normal };
        let drafted = e.draft.abs() > 1e-9;
        // Tramos (desde, largo) a lo largo de `n`; con desmolde, cada lado aparte
        // (las paredes se angostan a partir del plano del sketch)
        let segments: Vec<(f64, f64)> = match &e.extent {
            Extent::Blind { distance } => vec![(0.0, *distance)],
            Extent::Symmetric { distance } if drafted => vec![(0.0, distance / 2.0), (0.0, -distance / 2.0)],
            Extent::Symmetric { distance } => vec![(-distance / 2.0, *distance)],
            Extent::TwoSides { distance, second } if drafted => vec![(0.0, *distance), (0.0, -second)],
            Extent::TwoSides { distance, second } => vec![(-second, distance + second)],
            Extent::ThroughAll => {
                let body = self.body()?;
                let m = body.mass().map_err(err)?;
                let center = scale(add(m.bbox_min, m.bbox_max), 0.5);
                vec![(0.0, self.diag() * 2.0 + 2.0 * norm(sub(center, plane.origin)))]
            }
            Extent::UpToFace { face } => {
                let i = self.face("extent", face)?;
                let info = self.body()?.face_info(i).map_err(err)?;
                let d = dot(sub(info.point, plane.origin), n);
                if d.abs() < 1e-9 {
                    return Err("la cara está sobre el plano del sketch".into());
                }
                if d < 0.0 {
                    n = scale(n, -1.0);
                }
                vec![(0.0, d.abs())]
            }
            Extent::UpToNext => {
                // La primera cara que cruzan los rayos desde el perfil
                let body = self.body()?;
                let from = samples.iter().chain(entities.iter().map(|(_, p)| p));
                let t = from.filter_map(|p| body.ray_hit(*p, n)).fold(f64::INFINITY, f64::min);
                if !t.is_finite() {
                    return Err("no hay ninguna cara del sólido hacia ese lado".into());
                }
                vec![(0.0, t)]
            }
        };
        if segments.iter().all(|(_, l)| l.abs() < 1e-9) {
            return Err("distancia cero".into());
        }
        // La flecha, desde el centro del perfil (de la caja que lo envuelve)
        let boxes: Vec<_> = faces.iter().filter_map(|f| f.mass().ok()).collect();
        let origin = if boxes.is_empty() {
            plane.origin
        } else {
            let lo = boxes.iter().fold([f64::INFINITY; 3], |a, m| [0, 1, 2].map(|k| a[k].min(m.bbox_min[k])));
            let hi = boxes.iter().fold([f64::NEG_INFINITY; 3], |a, m| [0, 1, 2].map(|k| a[k].max(m.bbox_max[k])));
            scale(add(lo, hi), 0.5)
        };
        let (length, kind) = match &e.extent {
            Extent::Blind { distance } => (*distance, HandleKind::Blind),
            Extent::Symmetric { distance } => (distance / 2.0, HandleKind::Symmetric),
            Extent::TwoSides { distance, .. } => (*distance, HandleKind::TwoSides),
            _ => (segments.iter().map(|(s, l)| s + l).fold(0.0, f64::max), HandleKind::Fixed),
        };
        *self.handle.borrow_mut() = Some(Handle { origin, dir: n, length, reversed: reverse, kind });
        let mut solids = Vec::new();
        for f in &faces {
            for &(start, length) in &segments {
                if length.abs() < 1e-9 {
                    continue;
                }
                let f = if start != 0.0 { f.translate(scale(n, start)).map_err(err)? } else { f.clone() };
                if drafted {
                    // La cara mira hacia la normal del plano: la altura lleva el signo del lado
                    let height = if dot(n, plane.normal) > 0.0 { length } else { -length };
                    solids.push(f.draft_prism(height, e.draft.to_radians()).map_err(err)?);
                } else {
                    solids.push(f.prism(scale(n, length)).map_err(err)?);
                }
            }
        }
        let shape = fuse(solids)?;
        let lo = segments.iter().map(|(s, l)| s.min(s + l)).fold(f64::INFINITY, f64::min);
        let hi = segments.iter().map(|(s, l)| s.max(s + l)).fold(f64::NEG_INFINITY, f64::max);
        // Tapas por su posición a lo largo de la dirección; laterales por la
        // entidad del sketch que barren (su punto medio, a media altura)
        let mut tags = vec![Vec::new(); shape.face_count()];
        let scale_tol = self.diag().max(hi - lo) * 1e-6;
        for (i, slot) in tags.iter_mut().enumerate() {
            let info = shape.face_info(i).map_err(err)?;
            if info.surface == SurfaceKind::Plane && dot(info.normal, n).abs() > 0.999 {
                let d = dot(sub(info.point, plane.origin), n);
                if (d - lo).abs() <= scale_tol {
                    slot.push(tag(id, "inicio"));
                } else if (d - hi).abs() <= scale_tol {
                    slot.push(tag(id, "fin"));
                }
            }
        }
        for (eid, p) in &entities {
            let probe = add(*p, scale(n, (lo + hi) / 2.0));
            mark(&shape, &mut tags, probe, tag(id, format!("lado:{eid}")));
        }
        Ok(Tagged { shape, tags })
    }

    /// Caras B-Rep de las regiones elegidas, el punto medio (en el mundo) de
    /// cada entidad que las borde y un punto interior de cada región.
    #[allow(clippy::type_complexity)]
    fn profile(&self, sketch: FeatureId, sel: &RegionSelection) -> R<(Vec<Shape>, Vec<(u32, P3)>, Vec<P3>)> {
        let s = self.ev.sketches.get(&sketch).ok_or("el sketch no está calculado")?;
        let regions = self.selected_regions(s, sel)?;
        let faces = regions
            .iter()
            .map(|r| {
                let mut loops = vec![loop_curves(&s.sketch, &s.plane, &r.outer)?];
                for h in &r.holes {
                    loops.push(loop_curves(&s.sketch, &s.plane, h)?);
                }
                Shape::face(&loops).map_err(err)
            })
            .collect::<R<Vec<_>>>()?;
        let mut entities: Vec<(u32, P3)> = Vec::new();
        for r in &regions {
            for l in std::iter::once(&r.outer).chain(&r.holes) {
                for piece in &l.pieces {
                    if entities.iter().any(|(e, _)| *e == piece.entity) {
                        continue;
                    }
                    if let Some(p) = entity_midpoint(&s.sketch, piece.entity) {
                        entities.push((piece.entity, s.plane.to_world(p)));
                    }
                }
            }
        }
        let samples = regions.iter().map(|r| s.plane.to_world(r.sample)).collect();
        Ok((faces, entities, samples))
    }

    /// Regiones elegidas de un sketch; las que ya no están se anotan como
    /// referencias perdidas y se sigue con el resto.
    fn selected_regions<'s>(&self, s: &'s SketchResult, sel: &RegionSelection) -> R<Vec<&'s Region>> {
        let regions: Vec<&Region> = match sel {
            RegionSelection::All => s.regions.iter().filter(|r| r.depth % 2 == 0).collect(),
            RegionSelection::Points { points } => {
                let mut v = Vec::new();
                let mut lost = 0;
                for (i, p) in points.iter().enumerate() {
                    // La región más interna que contiene el punto
                    match s.regions.iter().filter(|r| r.contains(*p)).max_by_key(|r| r.depth) {
                        Some(r) if !v.contains(&r) => v.push(r),
                        Some(_) => {}
                        None => {
                            self.miss("regions", i);
                            lost += 1;
                        }
                    }
                }
                if lost > 0 && !v.is_empty() {
                    self.warn(format!("faltan {lost} de {} regiones", points.len()));
                }
                if v.is_empty() && lost > 0 {
                    return Err("no se encontró ninguna de las regiones elegidas".into());
                }
                v
            }
        };
        if regions.is_empty() {
            return Err("el sketch no tiene regiones cerradas".into());
        }
        Ok(regions)
    }
}

/// Agrega `t` a la cara de `shape` que pasa por `p` (si alguna pasa).
fn mark(shape: &Shape, tags: &mut [Vec<FaceTag>], p: P3, t: FaceTag) {
    if let Some(m) = shape.mass().ok()
        && let Some((f, d)) = shape.closest_face(p, None, 0.0)
        && d <= norm(sub(m.bbox_max, m.bbox_min)).max(1.0) * 1e-6
        && let Some(slot) = tags.get_mut(f)
        && !slot.contains(&t)
    {
        slot.push(t);
    }
}

/// Punto medio de una entidad del sketch (sobre la curva).
fn entity_midpoint(s: &Sketch, id: u32) -> Option<P2> {
    let e = s.entity(id).ok()?;
    let p = |i: u32| s.point(i).ok();
    Some(match &e.geometry {
        Geometry::Line { start, end } => {
            let (a, b) = (p(*start)?, p(*end)?);
            [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
        }
        Geometry::Circle { center, radius } => {
            let c = p(*center)?;
            [c[0] + radius, c[1]]
        }
        Geometry::Arc { center, start, end } => {
            let (c, a, b) = (p(*center)?, p(*start)?, p(*end)?);
            let r = dist2(c, a);
            let t = (a[1] - c[1]).atan2(a[0] - c[0]) + arc_sweep(c, a, b) / 2.0;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        }
        // La spline pasa por sus puntos: el del medio está sobre la curva
        Geometry::Spline { points, .. } => p(points[points.len() / 2])?,
        Geometry::Point { point } => p(*point)?,
        Geometry::Ellipse { major, .. } => p(*major)?,
    })
}

/// Marco de una caja centrada en X e Y: la esquina queda en −dx/2, −dy/2.
fn box_centered(f: Frame, dx: f64, dy: f64) -> Frame {
    let z = normalize(f.z);
    let x = normalize(sub(f.x, scale(z, dot(f.x, z))));
    let y = cross(z, x);
    let origin = sub(sub(f.origin, scale(x, dx / 2.0)), scale(y, dy / 2.0));
    Frame { origin, ..f }
}

/// Origen de las caras de una primitiva según su posición en el marco propio.
fn primitive_tags(id: FeatureId, p: &Primitive, shape: &Shape) -> Vec<Vec<FaceTag>> {
    let z = normalize(p.z);
    let x = normalize(sub(p.x, scale(z, dot(p.x, z))));
    let y = cross(z, x);
    // Las piezas estándar se nombran por índice: sin medir cada cara (el área
    // de las caras de una rosca se integra y tardaba segundos)
    if matches!(p.shape, PrimitiveShape::Bolt { .. } | PrimitiveShape::Nut { .. } | PrimitiveShape::Washer { .. }) {
        return (0..shape.face_count()).map(|i| vec![tag(id, format!("cara:{i}"))]).collect();
    }
    (0..shape.face_count())
        .map(|i| {
            let Ok(info) = shape.face_info(i) else { return vec![] };
            let name = match p.shape {
                PrimitiveShape::Box { .. } => {
                    let axes = [("x", x), ("y", y), ("z", z)];
                    axes.iter().find_map(|(a, v)| {
                        let d = dot(info.normal, *v);
                        (d.abs() > 0.9).then(|| format!("{}{a}", if d > 0.0 { "+" } else { "-" }))
                    })
                }
                PrimitiveShape::Cylinder { .. } | PrimitiveShape::Cone { .. } => Some(
                    if info.surface == SurfaceKind::Plane {
                        if dot(info.normal, z) > 0.0 { "arriba" } else { "abajo" }
                    } else {
                        "lado"
                    }
                    .to_string(),
                ),
                PrimitiveShape::Sphere { .. } | PrimitiveShape::Torus { .. } => Some("lado".to_string()),
                PrimitiveShape::Bolt { .. } | PrimitiveShape::Nut { .. } | PrimitiveShape::Washer { .. } => unreachable!(),
            };
            name.map(|n| vec![tag(id, n)]).unwrap_or_default()
        })
        .collect()
}

/// Rosca de un tornillo o una tuerca ya ubicados (`None` para las demás primitivas).
fn standard_thread(id: FeatureId, shape: &PrimitiveShape, frame: Frame, t: Threading, link: Option<ThreadLink>) -> Option<ThreadAxis> {
    let (size, modeled) = match shape {
        PrimitiveShape::Bolt { size, modeled, .. } | PrimitiveShape::Nut { size, modeled } => (size, *modeled),
        _ => return None,
    };
    let m = crate::standard::size(size)?;
    let spec = ThreadSpec { nominal: m.d, pitch: t.pitch.unwrap_or(m.pitch), clearance: 0.0, left: t.left };
    let z = normalize(frame.z);
    let ([origin, dir, x], internal, out, length) = match shape {
        PrimitiveShape::Bolt { length, .. } => (crate::standard::bolt_helix(frame, *length, None), false, z, *length),
        _ => (crate::standard::nut_helix(frame, spec.pitch, None), true, scale(z, -1.0), m.nut_m),
    };
    Some(ThreadAxis {
        feature: id,
        index: 0,
        spec,
        internal,
        modeled,
        origin,
        dir,
        x,
        mouth: frame.origin,
        out,
        length,
        blind: false,
        placed: Some([frame.origin, z, x]),
        link,
    })
}

/// Une sólidos etiquetados en uno, propagando los orígenes.
fn fuse_tagged(mut parts: Vec<Tagged>) -> R<Tagged> {
    if parts.len() == 1 {
        return Ok(parts.pop().unwrap());
    }
    let shapes: Vec<Shape> = parts.iter().map(|p| p.shape.clone()).collect();
    let (shape, h) = with_history(|| Shape::fuse_all(&shapes)).map_err(err)?;
    let inputs: Vec<&[Vec<FaceTag>]> = parts.iter().map(|p| p.tags.as_slice()).collect();
    let tags = propagate(&inputs, &h, shape.face_count()).0;
    Ok(Tagged { shape, tags })
}

fn fuse(mut shapes: Vec<Shape>) -> R<Shape> {
    match shapes.len() {
        0 => Err("nada que unir".into()),
        1 => Ok(shapes.pop().unwrap()),
        _ => Shape::fuse_all(&shapes).map_err(err),
    }
}

/// Curvas 3D de un lazo, en el sentido del lazo.
fn loop_curves(s: &Sketch, plane: &Plane, l: &Loop) -> R<Vec<Curve>> {
    let w = |id: u32| s.point(id).map(|p| plane.to_world(p)).map_err(err);
    let mut out = Vec::new();
    for piece in &l.pieces {
        let e = s.entity(piece.entity).map_err(err)?;
        let c = match &e.geometry {
            Geometry::Line { start, end } => {
                let (a, b) = (w(*start)?, w(*end)?);
                if piece.reversed { Curve::Line(b, a) } else { Curve::Line(a, b) }
            }
            Geometry::Circle { center, radius } => {
                let n = if piece.reversed { scale(plane.normal, -1.0) } else { plane.normal };
                Curve::Circle { center: w(*center)?, normal: n, radius: *radius }
            }
            Geometry::Arc { center, start, end } => {
                let (c, a, b) = (s.point(*center).map_err(err)?, s.point(*start).map_err(err)?, s.point(*end).map_err(err)?);
                let r = dist2(c, a);
                let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
                let mid_ang = a0 + arc_sweep(c, a, b) / 2.0;
                let m = plane.to_world([c[0] + r * mid_ang.cos(), c[1] + r * mid_ang.sin()]);
                let (a, b) = (plane.to_world(a), plane.to_world(b));
                if piece.reversed { Curve::Arc(b, m, a) } else { Curve::Arc(a, m, b) }
            }
            Geometry::Spline { points, closed, start_handle, end_handle } if !*closed && (start_handle.is_some() || end_handle.is_some()) => {
                let mut pts = points.iter().map(|p| w(*p)).collect::<R<Vec<_>>>()?;
                let n = pts.len();
                // Sin manija en un extremo: la dirección hacia el punto vecino
                let start = match start_handle {
                    Some(h) => sub(w(*h)?, pts[0]),
                    None => sub(pts[1], pts[0]),
                };
                let end = match end_handle {
                    Some(h) => sub(w(*h)?, pts[n - 1]),
                    None => sub(pts[n - 1], pts[n - 2]),
                };
                if piece.reversed {
                    pts.reverse();
                    Curve::SplineEnds { points: pts, start: scale(end, -1.0), end: scale(start, -1.0) }
                } else {
                    Curve::SplineEnds { points: pts, start, end }
                }
            }
            Geometry::Spline { points, closed, .. } => {
                let mut pts = points.iter().map(|p| w(*p)).collect::<R<Vec<_>>>()?;
                if *closed && let Some(&f) = pts.first() {
                    pts.push(f);
                }
                if piece.reversed {
                    pts.reverse();
                }
                Curve::Spline(pts)
            }
            // Un punto suelto no forma lazos
            Geometry::Point { .. } => continue,
            Geometry::Ellipse { center, major, minor } => {
                let (c, a, b) = (w(*center)?, w(*major)?, w(*minor)?);
                let (u, v) = (sub(a, c), sub(b, c));
                let n = if piece.reversed { scale(plane.normal, -1.0) } else { plane.normal };
                Curve::Ellipse { center: c, normal: n, major: normalize(u), a: norm(u), b: norm(v) }
            }
        };
        out.push(c);
    }
    Ok(out)
}
