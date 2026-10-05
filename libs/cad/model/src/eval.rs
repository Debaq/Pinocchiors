//! Recálculo del árbol. Una operación que falla queda marcada con su error y el
//! cuerpo sigue como estaba: las siguientes se recalculan igual.

use std::collections::HashMap;

use cad_occt::{Axis, Curve, Frame, Shape};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::feature::*;
use crate::geom::*;
use crate::regions::{Loop, Region, arc_sweep, find_regions};
use crate::sketch::{Geometry, Sketch, SolveReport};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FeatureState {
    Ok,
    Error { message: String },
    Suppressed,
    RolledBack,
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

#[derive(Debug, Default)]
pub struct Evaluation {
    pub body: Option<Shape>,
    pub status: Vec<FeatureStatus>,
    pub sketches: HashMap<FeatureId, SketchResult>,
    /// Herramienta de cada operación que la tiene (para patrones y simetrías).
    tools: HashMap<FeatureId, (Shape, BodyOp)>,
}

impl Evaluation {
    pub fn state(&self, id: FeatureId) -> Option<&FeatureState> {
        self.status.iter().find(|s| s.id == id).map(|s| &s.state)
    }

    pub fn errors(&self) -> Vec<(FeatureId, String)> {
        self.status
            .iter()
            .filter_map(|s| match &s.state {
                FeatureState::Error { message } => Some((s.id, message.clone())),
                _ => None,
            })
            .collect()
    }

    /// Referencia estable a una cara del cuerpo actual.
    pub fn face_ref(&self, face: usize) -> Option<FaceRef> {
        let info = self.body.as_ref()?.face_info(face).ok()?;
        Some(FaceRef { point: info.point, normal: info.normal })
    }

    /// Referencia estable a una arista del cuerpo actual.
    pub fn edge_ref(&self, edge: usize) -> Option<EdgeRef> {
        let info = self.body.as_ref()?.edge_info(edge).ok()?;
        Some(EdgeRef { point: info.mid, direction: info.tangent })
    }
}

type R<T> = Result<T, String>;
type ShapeFn = Box<dyn Fn(&Shape) -> R<Shape>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

struct Ctx<'a> {
    doc: &'a Document,
    ev: Evaluation,
}

pub fn evaluate(doc: &Document) -> Evaluation {
    let mut ctx = Ctx { doc, ev: Evaluation::default() };
    let limit = doc.rollback.unwrap_or(usize::MAX);
    for (i, f) in doc.features.iter().enumerate() {
        let state = if i >= limit {
            FeatureState::RolledBack
        } else if f.suppressed {
            FeatureState::Suppressed
        } else {
            match ctx.feature(f) {
                Ok(()) => FeatureState::Ok,
                Err(message) => FeatureState::Error { message },
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

    fn apply(&mut self, tool: Shape, op: BodyOp) -> R<()> {
        let new = match (&self.ev.body, op) {
            (None, BodyOp::Join) => tool,
            (None, _) => return Err("no hay sólido que cortar".into()),
            (Some(b), BodyOp::Join) => b.union(&tool).map_err(err)?,
            (Some(b), BodyOp::Cut) => b.cut(&tool).map_err(err)?,
            (Some(b), BodyOp::Intersect) => b.intersect(&tool).map_err(err)?,
        };
        self.ev.body = Some(new);
        Ok(())
    }

    fn diag(&self) -> f64 {
        self.ev.body.as_ref().and_then(|b| b.mass().ok()).map_or(1.0, |m| norm(sub(m.bbox_max, m.bbox_min)).max(1.0))
    }

    fn face(&self, r: &FaceRef) -> R<usize> {
        let body = self.body()?;
        let (i, d) = body
            .closest_face(r.point, Some(r.normal), 0.9)
            .ok_or("la cara de referencia ya no existe")?;
        if d > self.diag() * 0.5 {
            return Err("la cara de referencia ya no existe".into());
        }
        Ok(i)
    }

    fn edge(&self, r: &EdgeRef) -> R<usize> {
        let body = self.body()?;
        let (i, d) = body
            .closest_edge(r.point, Some(r.direction), 0.9)
            .ok_or("la arista de referencia ya no existe")?;
        if d > self.diag() * 0.5 {
            return Err("la arista de referencia ya no existe".into());
        }
        Ok(i)
    }

    fn plane(&self, spec: &PlaneSpec) -> R<Plane> {
        Ok(match spec {
            PlaneSpec::Xy => Plane::XY,
            PlaneSpec::Xz => Plane::XZ,
            PlaneSpec::Yz => Plane::YZ,
            PlaneSpec::Custom { plane } => *plane,
            PlaneSpec::Face { face } => {
                let i = self.face(face)?;
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

    fn axis(&self, spec: &AxisSpec) -> R<Axis> {
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
                let i = self.edge(edge)?;
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
                let plane = self.plane(plane)?.offset(*offset);
                let mut solved = sketch.clone();
                let report = solved.solve().map_err(err)?;
                let regions = find_regions(&solved).map_err(err)?;
                self.ev.sketches.insert(f.id, SketchResult { plane, sketch: solved, report, regions });
                Ok(())
            }
            FeatureKind::Extrude(e) => {
                let tool = self.extrude(e)?;
                self.ev.tools.insert(f.id, (tool.clone(), e.op));
                self.apply(tool, e.op)
            }
            FeatureKind::Revolve(r) => {
                let axis = self.axis(&r.axis)?;
                let faces = self.profile_faces(r.sketch, &r.regions)?;
                let angle = r.angle.to_radians();
                if angle.abs() < 1e-9 {
                    return Err("ángulo cero".into());
                }
                let solids = faces.iter().map(|fc| fc.revolve(axis, angle)).collect::<Result<Vec<_>, _>>().map_err(err)?;
                let tool = fuse(solids)?;
                self.ev.tools.insert(f.id, (tool.clone(), r.op));
                self.apply(tool, r.op)
            }
            FeatureKind::Primitive(p) => {
                let frame = Frame { origin: p.origin, z: p.z, x: p.x };
                let tool = match p.shape {
                    PrimitiveShape::Box { dx, dy, dz } => Shape::make_box(frame, dx, dy, dz),
                    PrimitiveShape::Cylinder { radius, height } => Shape::cylinder(frame, radius, height),
                    PrimitiveShape::Cone { r1, r2, height } => Shape::cone(frame, r1, r2, height),
                    PrimitiveShape::Sphere { radius } => Shape::sphere(p.origin, radius),
                    PrimitiveShape::Torus { major, minor } => Shape::torus(frame, major, minor),
                }
                .map_err(err)?;
                self.ev.tools.insert(f.id, (tool.clone(), p.op));
                self.apply(tool, p.op)
            }
            FeatureKind::Import { format, data, op } => {
                let tool = match format {
                    ImportFormat::Step => Shape::from_step(data),
                    ImportFormat::Brep => Shape::from_brep(data),
                }
                .map_err(err)?;
                self.ev.tools.insert(f.id, (tool.clone(), *op));
                self.apply(tool, *op)
            }
            FeatureKind::Fillet { edges, radius } => {
                let idx = edges.iter().map(|e| self.edge(e)).collect::<R<Vec<_>>>()?;
                let new = self.body()?.fillet(&idx, *radius).map_err(err)?;
                self.ev.body = Some(new);
                Ok(())
            }
            FeatureKind::Chamfer { edges, distance } => {
                let idx = edges.iter().map(|e| self.edge(e)).collect::<R<Vec<_>>>()?;
                let new = self.body()?.chamfer(&idx, *distance).map_err(err)?;
                self.ev.body = Some(new);
                Ok(())
            }
            FeatureKind::Shell { faces, thickness } => {
                let idx = faces.iter().map(|r| self.face(r)).collect::<R<Vec<_>>>()?;
                let new = self.body()?.shell(&idx, -thickness.abs()).map_err(err)?;
                self.ev.body = Some(new);
                Ok(())
            }
            FeatureKind::Draft { faces, neutral, angle } => {
                let idx = faces.iter().map(|r| self.face(r)).collect::<R<Vec<_>>>()?;
                let p = self.plane(neutral)?;
                let new = self.body()?.draft(&idx, p.normal, angle.to_radians(), p.origin, p.normal).map_err(err)?;
                self.ev.body = Some(new);
                Ok(())
            }
            FeatureKind::Pattern { features, pattern } => {
                let transforms = self.pattern_transforms(pattern)?;
                if features.is_empty() {
                    let body = self.body()?.clone();
                    let mut copies = vec![body.clone()];
                    for t in &transforms {
                        copies.push(t(&body)?);
                    }
                    self.ev.body = Some(fuse(copies)?);
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    let copies = transforms.iter().map(|t| t(&tool)).collect::<R<Vec<_>>>()?;
                    self.apply(fuse(copies)?, op)?;
                }
                Ok(())
            }
            FeatureKind::Mirror { features, plane } => {
                let p = self.plane(plane)?;
                if features.is_empty() {
                    let body = self.body()?.clone();
                    let m = body.mirror(p.origin, p.normal).map_err(err)?;
                    self.apply(m, BodyOp::Join)?;
                    return Ok(());
                }
                for id in features {
                    let (tool, op) = self.tool(*id)?;
                    let m = tool.mirror(p.origin, p.normal).map_err(err)?;
                    self.apply(m, op)?;
                }
                Ok(())
            }
            FeatureKind::Split { plane, flip } => {
                let p = self.plane(plane)?;
                let n = if *flip { scale(p.normal, -1.0) } else { p.normal };
                let new = self.body()?.split_keep(p.origin, n).map_err(err)?;
                self.ev.body = Some(new);
                Ok(())
            }
        }
    }

    fn tool(&self, id: FeatureId) -> R<(Shape, BodyOp)> {
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
                let ax = self.axis(axis)?;
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
    fn extrude(&self, e: &Extrude) -> R<Shape> {
        let tool = self.extrude_dir(e, e.reverse)?;
        let auto_flip = matches!(e.op, BodyOp::Cut | BodyOp::Intersect)
            && matches!(e.extent, Extent::Blind { .. } | Extent::ThroughAll);
        if auto_flip
            && let Some(body) = &self.ev.body
            && body.intersect(&tool).ok().and_then(|s| s.mass().ok()).is_none_or(|m| m.volume.abs() < 1e-9)
        {
            return self.extrude_dir(e, !e.reverse);
        }
        Ok(tool)
    }

    fn extrude_dir(&self, e: &Extrude, reverse: bool) -> R<Shape> {
        let plane = self.ev.sketches.get(&e.sketch).ok_or("el sketch no está calculado")?.plane;
        let faces = self.profile_faces(e.sketch, &e.regions)?;
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
                let i = self.face(face)?;
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
        fuse(solids)
    }

    /// Caras B-Rep de las regiones elegidas de un sketch.
    fn profile_faces(&self, sketch: FeatureId, sel: &RegionSelection) -> R<Vec<Shape>> {
        let s = self.ev.sketches.get(&sketch).ok_or("el sketch no está calculado")?;
        let regions: Vec<&Region> = match sel {
            RegionSelection::All => s.regions.iter().filter(|r| r.depth % 2 == 0).collect(),
            RegionSelection::Points { points } => {
                let mut v = Vec::new();
                for p in points {
                    // La región más interna que contiene el punto
                    let r = s
                        .regions
                        .iter()
                        .filter(|r| r.contains(*p))
                        .max_by_key(|r| r.depth)
                        .ok_or("una de las regiones elegidas ya no existe")?;
                    if !v.contains(&r) {
                        v.push(r);
                    }
                }
                v
            }
        };
        if regions.is_empty() {
            return Err("el sketch no tiene regiones cerradas".into());
        }
        regions
            .iter()
            .map(|r| {
                let mut loops = vec![loop_curves(&s.sketch, &s.plane, &r.outer)?];
                for h in &r.holes {
                    loops.push(loop_curves(&s.sketch, &s.plane, h)?);
                }
                Shape::face(&loops).map_err(err)
            })
            .collect()
    }
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
            Geometry::Spline { points, closed } => {
                let mut pts = points.iter().map(|p| w(*p)).collect::<R<Vec<_>>>()?;
                if *closed && let Some(&f) = pts.first() {
                    pts.push(f);
                }
                if piece.reversed {
                    pts.reverse();
                }
                Curve::Spline(pts)
            }
        };
        out.push(c);
    }
    Ok(out)
}
