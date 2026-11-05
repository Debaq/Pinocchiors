//! Editar un sketch que ya existe: borrar, mover, girar, escalar, espejo,
//! patrones, equidistante, recortar y construcción (`sketch_edit`); traer
//! aristas, caras, la intersección, la silueta u otro sketch (`sketch_use`).
//! Se trabaja sobre el sketch ya resuelto (como al abrirlo en la app).

use cad_model::project::Projected;
use cad_model::sketch::{SketchUse, UseSource};
use cad_model::sketch_edit::{self as edit, OffsetCaps, Xform};
use cad_model::{EdgeRef, FeatureId, FeatureKind, Geometry, P2, Sketch};
use serde_json::{json, Value};

use super::cad::{apply, find_feature, finish, open_design, Design};
use super::sketch::{dimension_lines, p2, sketch_report};
use super::{Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{any_object, array, boolean, id_or_name, integer, object};

pub const TOOLS: &[Tool] = &[EDIT, USE];

const OPS_DOC: &str = "Operations, applied in order (ids are sketch point/entity ids from `sketch_create`, `sketch_add` or `cad_design` \
with `detail`): \
{\"op\":\"delete\",\"ids\":[...]} (entities; a point id deletes what uses it); \
{\"op\":\"move\",\"ids\":[...],\"by\":[dx,dy]}; {\"op\":\"rotate\",\"ids\":[...],\"center\":[x,y],\"degrees\":a}; \
{\"op\":\"scale\",\"ids\":[...],\"center\":[x,y],\"factor\":k} — these three take \"copy\": true to leave the original and \
add a transformed copy; \
{\"op\":\"set_point\",\"point\":id,\"at\":[x,y]} (drag one point; constraints keep holding); \
{\"op\":\"mirror\",\"ids\":[...],\"axis\":lineId} (reflected copy tied by symmetry); \
{\"op\":\"linear_pattern\",\"ids\":[...],\"count\":n,\"offset\":[dx,dy],\"rows\":m,\"row_offset\":[dx,dy]} (count per row incl. \
the original); \
{\"op\":\"circular_pattern\",\"ids\":[...],\"count\":n,\"center\":pointId,\"span\":360,\"rotate\":true}; \
{\"op\":\"offset\",\"ids\":[chain of lines/arcs],\"distance\":d,\"both\":false,\"caps\":\"open|round|line\"} (positive = left of \
the chain's direction; on a closed counter-clockwise loop, inwards); \
{\"op\":\"trim\",\"entity\":id,\"at\":[x,y]} (removes the piece of that line/arc/circle around `at`, between the nearest \
crossings with other curves); \
{\"op\":\"construction\",\"ids\":[...],\"value\":true}.";

const EDIT: Tool = Tool {
    name: "sketch_edit",
    title: "Edit a sketch (delete, move, mirror, pattern, offset, trim)",
    description: "Edits an existing sketch (by its operation id or name) with a list of operations, like the sketch tools \
in the app: delete, move/rotate/scale (optionally as a copy), drag a point, mirror about a line, linear and circular \
patterns (editable later in the app), offset a chain of lines and arcs, trim a curve at its crossings, toggle \
construction. Works on the solved sketch. Returns what each operation created, whether the sketch is fully defined and \
its closed regions. Example: {\"sketch\": \"Sketch 1\", \"ops\": [{\"op\":\"offset\",\"ids\":[3,4,5,6],\"distance\":2}]}.",
    input: || {
        object(&[
            super::cad::tab_prop(),
            ("sketch", id_or_name("The sketch operation (id or name)"), true),
            ("ops", array(OPS_DOC, any_object("An operation: {\"op\": ..., ...}")), true),
        ])
    },
    effect: Effect::Change,
    run: run_edit,
};

const USE: Tool = Tool {
    name: "sketch_use",
    title: "Use model geometry in a sketch",
    description: "Brings geometry of the model into a sketch, projected onto its plane and linked: when the model changes, \
it follows (like \"Use\" in the app). `edges`: edge numbers and `faces`: face numbers (their whole outline) from \
`cad_geometry`; `section`: the curves where the sketch plane cuts the solid; `silhouette`: the outline of the solid seen \
along the plane's normal; `from_sketch`: every curve of an earlier sketch (id or name). Only geometry that exists \
before the sketch in the history can be followed. Returns the new entity ids.",
    input: || {
        object(&[
            super::cad::tab_prop(),
            ("sketch", id_or_name("The sketch operation (id or name)"), true),
            ("edges", array("Edge numbers (from `cad_geometry`)", integer("edge", 0)), false),
            ("faces", array("Face numbers (from `cad_geometry`): their outline", integer("face", 0)), false),
            ("section", boolean("The intersection of the sketch plane with the solid"), false),
            ("silhouette", boolean("The outline of the solid seen from the sketch normal"), false),
            ("from_sketch", id_or_name("An earlier sketch: all its curves"), false),
        ])
    },
    effect: Effect::Change,
    run: run_use,
};

/// El sketch abierto para editar: el documento, la operación y el sketch ya
/// resuelto (si está calculado) puesto en el documento
struct Open {
    d: Design,
    id: FeatureId,
    name: String,
    /// Operaciones con error antes del cambio (para avisar solo lo nuevo)
    before: Vec<u64>,
    /// Plano del sketch (como lo da el cálculo)
    plane: Value,
}

fn open(ctx: &Ctx, args: &Value) -> Result<Open, Fail> {
    let mut d = open_design(ctx, args)?;
    let id = find_feature(&d.doc, &args["sketch"])?;
    let result = ctx.call("cad_evaluate", json!({}))?;
    let before = result["status"].as_array().into_iter().flatten().filter(|s| s["state"] == "error").filter_map(|s| s["id"].as_u64()).collect();
    let view = result["sketches"].as_array().and_then(|l| l.iter().find(|s| s["id"] == id.0)).cloned().unwrap_or_default();
    let name = d.doc.get(id).map(|f| f.name.clone()).unwrap_or_default();
    let Some(FeatureKind::Sketch { sketch, .. } | FeatureKind::SurfaceSketch { sketch, .. }) = d.doc.get_mut(id).map(|f| &mut f.kind) else {
        return Err(Fail::new(format!("\"{name}\" is not a sketch")));
    };
    // El resuelto trae las posiciones de verdad (lo guardado puede ser el dibujo de antes de resolver)
    if let Ok(solved) = serde_json::from_value::<Sketch>(view["sketch"].clone()) {
        *sketch = solved;
    }
    Ok(Open { d, id, name, before, plane: view["plane"].clone() })
}

fn sketch_mut(o: &mut Open) -> &mut Sketch {
    match o.d.doc.get_mut(o.id).map(|f| &mut f.kind) {
        Some(FeatureKind::Sketch { sketch, .. } | FeatureKind::SurfaceSketch { sketch, .. }) => sketch,
        _ => unreachable!("open() ya vio que es un sketch"),
    }
}

/// Recalcula y arma la respuesta: una línea por operación, estado y regiones
fn close(ctx: &Ctx, o: Open, lines: Vec<String>, headline: String) -> Result<Output, Fail> {
    let dims = {
        let s = match o.d.doc.get(o.id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch { sketch, .. } | FeatureKind::SurfaceSketch { sketch, .. }) => sketch,
            _ => unreachable!(),
        };
        dimension_lines(s, o.id.0 as u64)
    };
    let result = apply(ctx, &o.d)?;
    let mut out = finish(ctx, &o.d, &result, &o.before, Some((o.id, &o.name)), &headline, "")?;
    let (state, report) = sketch_report(&result, o.id.0);
    let mut text = vec![out.text.lines().next().unwrap_or("").to_string()];
    text.extend(lines);
    if !dims.is_empty() {
        text.push("Dimensions (index, value, path for `cad_parameters` formulas):".into());
        text.extend(dims);
    }
    text.push(state);
    // Avisos del sketch (curvas usadas que no están, etc.) y lo que se rompió
    let warn = result["status"].as_array().and_then(|l| l.iter().find(|s| s["id"] == o.id.0)).filter(|s| s["state"] == "warning").and_then(|s| s["message"].as_str().map(str::to_string));
    if let Some(w) = warn {
        text.push(format!("Warning on the sketch: {w}"));
    }
    text.extend(out.text.lines().skip(1).filter(|l| l.starts_with("Warning")).map(str::to_string));
    out.text = text.join("\n");
    out.data["sketch"] = report;
    Ok(out)
}

fn ids_of(op: &Value, k: usize, key: &str) -> Result<Vec<u32>, Fail> {
    let list = op.get(key).and_then(Value::as_array).ok_or_else(|| Fail::new(format!("op {k}: missing \"{key}\" (a list of ids)")))?;
    list.iter().map(|v| v.as_u64().map(|x| x as u32).ok_or_else(|| Fail::new(format!("op {k}: \"{key}\" must hold ids (numbers), got {v}")))).collect()
}

fn id_of(op: &Value, k: usize, key: &str) -> Result<u32, Fail> {
    op.get(key).and_then(Value::as_u64).map(|x| x as u32).ok_or_else(|| Fail::new(format!("op {k}: missing \"{key}\" (an id)")))
}

fn num_of(op: &Value, k: usize, key: &str) -> Result<f64, Fail> {
    op.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| Fail::new(format!("op {k}: missing number \"{key}\"")))
}

fn point_of(op: &Value, k: usize, key: &str) -> Result<P2, Fail> {
    p2(op.get(key).ok_or_else(|| Fail::new(format!("op {k}: missing \"{key}\" ([x, y])")))?, &format!("op {k}.{key}"))
}

fn list(ids: &[u32]) -> String {
    if ids.is_empty() { "nothing".into() } else { format!("[{}]", ids.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")) }
}

/// Entidades nuevas (que no estaban antes)
fn new_entities(s: &Sketch, before: &[u32]) -> Vec<u32> {
    s.entities.iter().map(|e| e.id).filter(|id| !before.contains(id)).collect()
}

/// Tope de copias de un patrón (más es un error del pedido)
const MAX_COPIES: u64 = 1000;

fn one_op(s: &mut Sketch, op: &Value, k: usize) -> Result<String, Fail> {
    let name = op.get("op").and_then(Value::as_str).unwrap_or("");
    let fail = |e: String| Fail::new(format!("op {k} ({name}): {e}"));
    let copy = op.get("copy").and_then(Value::as_bool).unwrap_or(false);
    let before: Vec<u32> = s.entities.iter().map(|e| e.id).collect();
    Ok(match name {
        "delete" => {
            let gone = edit::delete(s, &ids_of(op, k, "ids")?).map_err(fail)?;
            format!("  op {k} delete: removed entities {}", list(&gone))
        }
        "move" | "rotate" | "scale" => {
            let ids = ids_of(op, k, "ids")?;
            let x = match name {
                "move" => Xform::translation(point_of(op, k, "by")?),
                "rotate" => Xform::rotation(point_of(op, k, "center")?, num_of(op, k, "degrees")?),
                _ => {
                    let f = num_of(op, k, "factor")?;
                    if f <= 0.0 {
                        return Err(Fail::new(format!("op {k} (scale): \"factor\" must be greater than zero")));
                    }
                    Xform::scaling(point_of(op, k, "center")?, f)
                }
            };
            let made = edit::transform(s, &ids, &x, copy).map_err(fail)?;
            if copy { format!("  op {k} {name} (copy): new entities {}", list(&made)) } else { format!("  op {k} {name}: done") }
        }
        "set_point" => {
            let (p, at) = (id_of(op, k, "point")?, point_of(op, k, "at")?);
            s.solve_drag(p, at).map_err(|e| fail(e.to_string()))?;
            let now = s.point(p).map_err(|e| fail(e.to_string()))?;
            format!("  op {k} set_point: point {p} is at ({}, {})", super::cad::num(now[0]), super::cad::num(now[1]))
        }
        "mirror" => {
            let made = edit::mirror(s, &ids_of(op, k, "ids")?, id_of(op, k, "axis")?).map_err(fail)?;
            format!("  op {k} mirror: new entities {}", list(&made))
        }
        "linear_pattern" => {
            let count = op.get("count").and_then(Value::as_u64).unwrap_or(0);
            let rows = op.get("rows").and_then(Value::as_u64).unwrap_or(1);
            if count * rows > MAX_COPIES {
                return Err(Fail::new(format!("op {k} (linear_pattern): at most {MAX_COPIES} copies")));
            }
            let row_offset = match op.get("row_offset") {
                Some(v) => p2(v, &format!("op {k}.row_offset"))?,
                None => [0.0, 0.0],
            };
            let r = edit::linear_pattern(s, &ids_of(op, k, "ids")?, count as u32, point_of(op, k, "offset")?, rows as u32, row_offset).map_err(fail)?;
            format!("  op {k} linear_pattern: {} copies, entities per copy {}", r.copies.len(), r.copies.iter().map(|c| list(c)).collect::<Vec<_>>().join(" "))
        }
        "circular_pattern" => {
            let count = op.get("count").and_then(Value::as_u64).unwrap_or(0);
            if count > MAX_COPIES {
                return Err(Fail::new(format!("op {k} (circular_pattern): at most {MAX_COPIES} copies")));
            }
            let span = op.get("span").and_then(Value::as_f64).unwrap_or(360.0);
            let rotate = op.get("rotate").and_then(Value::as_bool).unwrap_or(true);
            let r = edit::circular_pattern(s, &ids_of(op, k, "ids")?, count as u32, id_of(op, k, "center")?, span, rotate).map_err(fail)?;
            format!("  op {k} circular_pattern: {} copies, entities per copy {}", r.copies.len(), r.copies.iter().map(|c| list(c)).collect::<Vec<_>>().join(" "))
        }
        "offset" => {
            let caps = match op.get("caps").and_then(Value::as_str).unwrap_or("open") {
                "round" => OffsetCaps::Round,
                "line" => OffsetCaps::Line,
                _ => OffsetCaps::Open,
            };
            let both = op.get("both").and_then(Value::as_bool).unwrap_or(false);
            let made = edit::offset_chain(s, &ids_of(op, k, "ids")?, num_of(op, k, "distance")?, both, caps).map_err(fail)?;
            format!("  op {k} offset: new entities {}", list(&made))
        }
        "trim" => {
            edit::trim(s, id_of(op, k, "entity")?, point_of(op, k, "at")?).map_err(fail)?;
            let made = new_entities(s, &before);
            format!("  op {k} trim: done{}", if made.is_empty() { String::new() } else { format!(" (split into new entities {})", list(&made)) })
        }
        "construction" => {
            let ids = ids_of(op, k, "ids")?;
            let value = op.get("value").and_then(Value::as_bool).unwrap_or(true);
            let mut n = 0;
            for e in s.entities.iter_mut().filter(|e| ids.contains(&e.id)) {
                e.construction = value;
                n += 1;
            }
            if n == 0 {
                return Err(Fail::new(format!("op {k} (construction): none of those ids is an entity")));
            }
            format!("  op {k} construction: {n} entities {}", if value { "are construction now" } else { "are regular geometry now" })
        }
        "" => return Err(Fail::new(format!("op {k}: missing \"op\""))),
        other => {
            return Err(Fail::new(format!("op {k}: unknown op \"{other}\""))
                .hint("use delete, move, rotate, scale, set_point, mirror, linear_pattern, circular_pattern, offset, trim or construction"));
        }
    })
}

fn run_edit(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let ops = args.get("ops").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| Fail::new("nothing to do").hint("pass `ops`, a list of operations"))?;
    let mut o = open(ctx, args)?;
    let s = sketch_mut(&mut o);
    let mut lines = Vec::new();
    // Todas o ninguna: si una falla, el documento no se toca
    for (k, op) in ops.iter().enumerate() {
        lines.push(one_op(s, op, k)?);
    }
    let headline = format!("Edited sketch \"{}\" (feature {}): {} operation(s).", o.name, o.id.0, ops.len());
    close(ctx, o, lines, headline)
}

/// Agrega curvas proyectadas, ligadas por `link(i)`; los extremos que caen en
/// el mismo lugar comparten punto (así el contorno de una cara cierra), como
/// `addProjected` en la app. Devuelve las entidades.
fn add_projected(s: &mut Sketch, curves: &[Projected], link: impl Fn(usize) -> SketchUse) -> Vec<u32> {
    let mut made: Vec<(u32, P2)> = Vec::new();
    let mut pt = |s: &mut Sketch, p: P2, share: bool| -> u32 {
        let tol = 1e-6 * p[0].hypot(p[1]).max(1.0);
        if share && let Some((id, _)) = made.iter().find(|(_, q)| (q[0] - p[0]).hypot(q[1] - p[1]) <= tol) {
            return *id;
        }
        let id = s.add_point(p[0], p[1]);
        if share {
            made.push((id, p));
        }
        id
    };
    let mut out = Vec::new();
    for (i, c) in curves.iter().enumerate() {
        let g = match c {
            Projected::Point { at } => Geometry::Point { point: pt(s, *at, true) },
            Projected::Line { start, end } => Geometry::Line { start: pt(s, *start, true), end: pt(s, *end, true) },
            Projected::Circle { center, radius } => Geometry::Circle { center: pt(s, *center, false), radius: *radius },
            Projected::Arc { center, start, end } => Geometry::Arc { center: pt(s, *center, false), start: pt(s, *start, true), end: pt(s, *end, true) },
            Projected::Ellipse { center, major, minor } => Geometry::Ellipse { center: pt(s, *center, false), major: pt(s, *major, false), minor: pt(s, *minor, false) },
            Projected::Spline { points, closed, handles } => {
                let n = points.len();
                Geometry::Spline {
                    points: points.iter().enumerate().map(|(k, q)| pt(s, *q, !*closed && (k == 0 || k + 1 == n))).collect(),
                    closed: *closed,
                    start_handle: handles.map(|h| pt(s, h[0], false)),
                    end_handle: handles.map(|h| pt(s, h[1], false)),
                    handles: vec![],
                }
            }
        };
        let entity = s.add_entity(g);
        s.uses.push(SketchUse { entity, ..link(i) });
        out.push(entity);
    }
    out
}

fn run_use(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let nums = |key: &str| -> Vec<u64> { args.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect() };
    let flag = |key: &str| args.get(key).and_then(Value::as_bool).unwrap_or(false);
    let (edges, faces) = (nums("edges"), nums("faces"));
    let from = args.get("from_sketch").filter(|v| !v.is_null());
    if edges.is_empty() && faces.is_empty() && !flag("section") && !flag("silhouette") && from.is_none() {
        return Err(Fail::new("nothing to use").hint("pass `edges`, `faces`, `section`, `silhouette` or `from_sketch`"));
    }
    let mut o = open(ctx, args)?;
    if o.plane.is_null() {
        return Err(Fail::new(format!("\"{}\" has no plane yet (it is not calculated)", o.name)).hint("fix the errors before it (see `cad_design`)"));
    }
    let plane = o.plane.clone();
    let index = o.d.doc.features.iter().position(|f| f.id == o.id).unwrap_or(0);
    let doc = serde_json::to_value(&o.d.doc).map_err(|e| Fail::new(e.to_string()))?;
    let other = from.map(|v| find_feature(&o.d.doc, v)).transpose()?;
    // Primero todo lo que se pide al cálculo (si algo falla, el sketch queda como estaba)
    type Linked = (Vec<Projected>, Vec<SketchUse>, String);
    let mut batches: Vec<Linked> = Vec::new();
    let projected = |v: &Value| serde_json::from_value::<Projected>(v.clone()).map_err(|e| Fail::new(format!("unexpected projection: {e}")));
    let edge_ref = |v: &Value| serde_json::from_value::<EdgeRef>(v["ref"].clone()).map_err(|e| Fail::new(format!("unexpected edge reference: {e}")));
    let use_of = |edge: Option<EdgeRef>, source: Option<UseSource>| SketchUse { edge, source, entity: 0 };
    for e in &edges {
        let v = ctx.call("cad_project_edge", json!({ "edge": e, "plane": plane })).map_err(|f| Fail::new(format!("edge {e}: {}", f.message)).hint("edge numbers come from `cad_geometry`"))?;
        batches.push((vec![projected(&v)?], vec![use_of(Some(edge_ref(&v)?), None)], format!("edge {e}")));
    }
    for f in &faces {
        let v = ctx.call("cad_project_face", json!({ "face": f, "plane": plane })).map_err(|x| Fail::new(format!("face {f}: {}", x.message)).hint("face numbers come from `cad_geometry`"))?;
        let list = v.as_array().cloned().unwrap_or_default();
        let curves = list.iter().map(projected).collect::<Result<Vec<_>, _>>()?;
        let uses = list.iter().map(|x| Ok(use_of(Some(edge_ref(x)?), None))).collect::<Result<Vec<_>, Fail>>()?;
        batches.push((curves, uses, format!("face {f}")));
    }
    for (what, source) in [("section", UseSource::Section), ("silhouette", UseSource::Silhouette)] {
        if !flag(what) {
            continue;
        }
        let v = ctx.call("cad_project_model", json!({ "document": doc, "index": index, "plane": plane, "what": what }))?;
        let curves = v.as_array().into_iter().flatten().map(projected).collect::<Result<Vec<_>, _>>()?;
        let uses = curves.iter().map(|_| use_of(None, Some(source.clone()))).collect();
        batches.push((curves, uses, what.to_string()));
    }
    if let Some(feature) = other {
        if feature == o.id {
            return Err(Fail::new("`from_sketch` is this same sketch"));
        }
        let v = ctx.call("cad_project_sketch", json!({ "feature": feature, "plane": plane }))?;
        let list = v.as_array().cloned().unwrap_or_default();
        let curves = list.iter().map(projected).collect::<Result<Vec<_>, _>>()?;
        let uses = list.iter().map(|x| use_of(None, Some(UseSource::Sketch { feature, entity: x["entity"].as_u64().unwrap_or(0) as u32 }))).collect();
        let fname = o.d.doc.get(feature).map(|f| f.name.clone()).unwrap_or_default();
        batches.push((curves, uses, format!("sketch \"{fname}\"")));
    }
    let s = sketch_mut(&mut o);
    let mut lines = Vec::new();
    let mut total = 0;
    for (curves, uses, what) in &batches {
        if curves.is_empty() {
            lines.push(format!("  {what}: nothing (it does not cross or show on the sketch plane)"));
            continue;
        }
        let made = add_projected(s, curves, |i| uses[i].clone());
        total += made.len();
        lines.push(format!("  {what}: entities {}", list(&made)));
    }
    let headline = format!("Linked {total} curve(s) of the model into sketch \"{}\" (feature {}); they follow the model when it changes.", o.name, o.id.0);
    close(ctx, o, lines, headline)
}
