//! Recálculo del árbol. Una operación que falla queda marcada con su error y el
//! cuerpo sigue como estaba: las siguientes se recalculan igual.

use std::cell::RefCell;
use std::collections::HashMap;

use cad_occt::{Axis, Curve, Frame, History, Shape, SurfaceKind, with_history};
use serde::{Deserialize, Serialize};

use crate::document::{Document, ResolvedValue};
use crate::feature::*;
use crate::geom::*;
use crate::regions::{Loop, Region, arc_sweep, find_regions};
use crate::sketch::{Geometry, Sketch, SolveReport};

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

#[derive(Debug, Default)]
pub struct Evaluation {
    pub body: Option<Shape>,
    /// Orígenes de cada cara del cuerpo (mismo orden que sus índices).
    pub face_tags: Vec<Vec<FaceTag>>,
    pub status: Vec<FeatureStatus>,
    pub sketches: HashMap<FeatureId, SketchResult>,
    /// Herramienta de cada operación que la tiene (para patrones y simetrías).
    tools: HashMap<FeatureId, (Tagged, BodyOp)>,
    /// Parámetros y campos vinculados, ya calculados.
    pub parameters: Vec<ResolvedValue>,
    pub bindings: Vec<ResolvedValue>,
}

impl Evaluation {
    pub fn state(&self, id: FeatureId) -> Option<&FeatureState> {
        self.status.iter().find(|s| s.id == id).map(|s| &s.state)
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

struct Ctx<'a> {
    doc: &'a Document,
    ev: Evaluation,
    /// Referencias no encontradas y avisos de la operación en curso.
    missing: RefCell<Vec<MissingRef>>,
    warnings: RefCell<Vec<String>>,
}

pub fn evaluate(doc: &Document) -> Evaluation {
    // Primero las fórmulas: el árbol se calcula con los números que dan
    let res = doc.resolve();
    let doc = &res.document;
    let mut ctx = Ctx {
        doc,
        ev: Evaluation { parameters: res.parameters, bindings: res.bindings, ..Default::default() },
        missing: RefCell::default(),
        warnings: RefCell::default(),
    };
    let limit = doc.rollback.unwrap_or(usize::MAX);
    for (i, f) in doc.features.iter().enumerate() {
        let state = if i >= limit {
            FeatureState::RolledBack
        } else if f.suppressed {
            FeatureState::Suppressed
        } else {
            let result = ctx.feature(f);
            let missing = ctx.missing.take();
            let warnings = ctx.warnings.take();
            match result {
                Ok(()) if missing.is_empty() => FeatureState::Ok,
                Ok(()) => FeatureState::Warning { message: warnings.join("; "), missing },
                Err(message) => FeatureState::Error { message, missing },
            }
        };
        ctx.ev.status.push(FeatureStatus { id: f.id, state });
    }
    ctx.ev
}

impl Ctx<'_> {
    fn body(&self) -> R<&Shape> {
        self.ev.body.as_ref().ok_or_else(|| "todavía no hay un sólido".to_string())
    }

    fn apply(&mut self, tool: Tagged, op: BodyOp) -> R<()> {
        let Some(b) = &self.ev.body else {
            if op != BodyOp::Join {
                return Err("no hay sólido que cortar".into());
            }
            self.ev.body = Some(tool.shape);
            self.ev.face_tags = tool.tags;
            return Ok(());
        };
        let (new, h) = with_history(|| match op {
            BodyOp::Join => b.union(&tool.shape),
            BodyOp::Cut => b.cut(&tool.shape),
            BodyOp::Intersect => b.intersect(&tool.shape),
        })
        .map_err(err)?;
        let n = new.face_count();
        self.ev.face_tags = propagate(&[&self.ev.face_tags, &tool.tags], &h, n).0;
        self.ev.body = Some(new);
        Ok(())
    }

    /// Reemplaza el cuerpo por el resultado de una operación sobre él; los
    /// orígenes pasan por la historia y `extra` nombra lo generado después.
    fn replace_body(&mut self, new: Shape, h: &History, extra: impl Fn(usize) -> Option<FaceTag>) {
        let n = new.face_count();
        let (mut tags, used) = propagate(&[&self.ev.face_tags], h, n);
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
        self.ev.face_tags = tags;
        self.ev.body = Some(new);
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

    fn feature(&mut self, f: &Feature) -> R<()> {
        match &f.kind {
            FeatureKind::Sketch { plane, offset, sketch } => {
                let plane = self.plane("plane", plane)?.offset(*offset);
                let mut solved = sketch.clone();
                let report = solved.solve().map_err(err)?;
                let regions = find_regions(&solved).map_err(err)?;
                self.ev.sketches.insert(f.id, SketchResult { plane, sketch: solved, report, regions });
                Ok(())
            }
            FeatureKind::Extrude(e) => {
                let tool = self.extrude(f.id, e)?;
                self.ev.tools.insert(f.id, (tool.clone(), e.op));
                self.apply(tool, e.op)
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
                self.apply(tool, r.op)
            }
            FeatureKind::Primitive(p) => {
                let frame = Frame { origin: p.origin, z: p.z, x: p.x };
                let tool = match p.shape {
                    PrimitiveShape::Box { dx, dy, dz, centered } => {
                        let frame = if centered { box_centered(frame, dx, dy) } else { frame };
                        Shape::make_box(frame, dx, dy, dz)
                    }
                    PrimitiveShape::Cylinder { radius, height } => Shape::cylinder(frame, radius, height),
                    PrimitiveShape::Cone { r1, r2, height } => Shape::cone(frame, r1, r2, height),
                    PrimitiveShape::Sphere { radius } => Shape::sphere(p.origin, radius),
                    PrimitiveShape::Torus { major, minor } => Shape::torus(frame, major, minor),
                }
                .map_err(err)?;
                let tool = Tagged { tags: primitive_tags(f.id, p, &tool), shape: tool };
                self.ev.tools.insert(f.id, (tool.clone(), p.op));
                self.apply(tool, p.op)
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
                self.apply(tool, *op)
            }
            FeatureKind::Fillet { edges, radius } => {
                if edges.is_empty() {
                    return Err("elegir al menos una arista".into());
                }
                let idx = self.edges("edges", edges)?;
                let body = self.body()?.clone();
                let (new, h) = with_history(|| body.fillet(&idx, *radius)).map_err(err)?;
                self.replace_body(new, &h, |k| Some(tag(f.id, format!("redondeo:{k}"))));
                Ok(())
            }
            FeatureKind::Chamfer { edges, distance } => {
                if edges.is_empty() {
                    return Err("elegir al menos una arista".into());
                }
                let idx = self.edges("edges", edges)?;
                let body = self.body()?.clone();
                let (new, h) = with_history(|| body.chamfer(&idx, *distance)).map_err(err)?;
                self.replace_body(new, &h, |k| Some(tag(f.id, format!("chaflan:{k}"))));
                Ok(())
            }
            FeatureKind::Shell { faces, thickness } => {
                let idx = self.faces("faces", faces)?;
                let body = self.body()?.clone();
                let (new, h) = with_history(|| body.shell(&idx, -thickness.abs())).map_err(err)?;
                self.replace_body(new, &h, |_| None);
                Ok(())
            }
            FeatureKind::Draft { faces, neutral, angle } => {
                if faces.is_empty() {
                    return Err("elegir al menos una cara".into());
                }
                let idx = self.faces("faces", faces)?;
                let p = self.plane("neutral", neutral)?;
                let body = self.body()?.clone();
                let (new, h) =
                    with_history(|| body.draft(&idx, p.normal, angle.to_radians(), p.origin, p.normal)).map_err(err)?;
                self.replace_body(new, &h, |_| None);
                Ok(())
            }
            FeatureKind::Pattern { features, pattern } => {
                let transforms = self.pattern_transforms(pattern)?;
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
                    let body = Tagged { shape: self.body()?.clone(), tags: self.ev.face_tags.clone() };
                    let mut all = vec![body.clone()];
                    all.extend(copies_of(&body)?);
                    let fused = fuse_tagged(all)?;
                    self.ev.body = Some(fused.shape);
                    self.ev.face_tags = fused.tags;
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    self.apply(fuse_tagged(copies_of(&tool)?)?, op)?;
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
                    let body = Tagged { shape: self.body()?.clone(), tags: self.ev.face_tags.clone() };
                    let m = mirrored(&body)?;
                    self.apply(m, BodyOp::Join)?;
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    let m = mirrored(&tool)?;
                    self.apply(m, op)?;
                }
                Ok(())
            }
            FeatureKind::Split { plane, flip } => {
                let p = self.plane("plane", plane)?;
                let n = if *flip { scale(p.normal, -1.0) } else { p.normal };
                let body = self.body()?.clone();
                let (new, h) = with_history(|| body.split_keep(p.origin, n)).map_err(err)?;
                // Después de las caras del cuerpo vienen las del semiespacio: el corte
                self.replace_body(new, &h, |_| Some(tag(f.id, "corte")));
                Ok(())
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

    fn pattern_transforms(&self, p: &PatternKind) -> R<Vec<ShapeFn>> {
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
        let (faces, entities, _) = self.profile(e.sketch, &e.regions)?;
        let mut n = if reverse { scale(plane.normal, -1.0) } else { plane.normal };
        let (start, length) = match &e.extent {
            Extent::Blind { distance } => (0.0, *distance),
            Extent::Symmetric { distance } => (-distance / 2.0, *distance),
            Extent::ThroughAll => {
                let body = self.body()?;
                let m = body.mass().map_err(err)?;
                let center = scale(add(m.bbox_min, m.bbox_max), 0.5);
                (0.0, self.diag() * 2.0 + 2.0 * norm(sub(center, plane.origin)))
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
                (0.0, d.abs())
            }
        };
        if length.abs() < 1e-9 {
            return Err("distancia cero".into());
        }
        let mut solids = Vec::new();
        for f in faces {
            let f = if start != 0.0 { f.translate(scale(n, start)).map_err(err)? } else { f };
            solids.push(f.prism(scale(n, length)).map_err(err)?);
        }
        let shape = fuse(solids)?;
        // Tapas por su posición a lo largo de la dirección; laterales por la
        // entidad del sketch que barren (su punto medio, a media altura)
        let mut tags = vec![Vec::new(); shape.face_count()];
        let scale_tol = self.diag().max(length.abs()) * 1e-6;
        for (i, slot) in tags.iter_mut().enumerate() {
            let info = shape.face_info(i).map_err(err)?;
            if info.surface == SurfaceKind::Plane && dot(info.normal, n).abs() > 0.999 {
                let d = dot(sub(info.point, plane.origin), n);
                if (d - start).abs() <= scale_tol {
                    slot.push(tag(id, "inicio"));
                } else if (d - start - length).abs() <= scale_tol {
                    slot.push(tag(id, "fin"));
                }
            }
        }
        for (eid, p) in &entities {
            let probe = add(*p, scale(n, start + length / 2.0));
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
            };
            name.map(|n| vec![tag(id, n)]).unwrap_or_default()
        })
        .collect()
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
