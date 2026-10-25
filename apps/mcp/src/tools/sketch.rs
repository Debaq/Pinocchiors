//! Sketches: crear uno con figuras y restricciones, y agregarle más.

use std::collections::HashMap;

use cad_model::{FeatureKind, Geometry, P2, PlaneSpec, Sketch, SketchConstraint};
use serde_json::{json, Value};

use super::cad::{add_feature, apply, finish, find_feature, num, open_design, resolve_refs, sketch_state, tab_prop, Design};
use super::{arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::schema::{any_object, array, id_or_name, integer, number, object, one_of, string};

pub const TOOLS: &[Tool] = &[CREATE, ADD];

const SHAPES_DOC: &str = "Shapes (sketch coordinates in mm): \
{\"type\":\"rectangle\",\"corner\":[x,y],\"opposite\":[x,y]} or {\"type\":\"rectangle\",\"center\":[x,y],\"width\":w,\"height\":h}; \
{\"type\":\"circle\",\"center\":[x,y],\"radius\":r} (or \"diameter\"); \
{\"type\":\"line\",\"from\":[x,y],\"to\":[x,y]}; \
{\"type\":\"polyline\",\"points\":[[x,y],...],\"closed\":true}; \
{\"type\":\"arc\",\"center\":[x,y],\"radius\":r,\"start_angle\":deg,\"end_angle\":deg} (counter-clockwise; or \"start\"/\"end\" points); \
{\"type\":\"polygon\",\"center\":[x,y],\"radius\":r,\"sides\":n} (regular, radius to the corners); \
{\"type\":\"slot\",\"from\":[x,y],\"to\":[x,y],\"width\":w}; \
{\"type\":\"point\",\"at\":[x,y]}. Any shape takes \"construction\": true (helper geometry, not part of the profile).";

const CONSTRAINTS_DOC: &str = "Constraints are objects with a `type` and ids of sketch points/entities; instead of ids you can use \
names of shapes from this call: \"s0\" (the entity of shape 0 when it is a single line/circle/arc), \"s0.top\", \"s0.center\", \
\"s1.start\"… (the result lists all names), or \"origin\" (the fixed sketch origin). Examples: \
{\"type\":\"coincident\",\"a\":\"s0.bottom_left\",\"b\":\"origin\"}, {\"type\":\"length\",\"line\":\"s0.bottom\",\"value\":40}, \
{\"type\":\"diameter\",\"entity\":\"s1\",\"value\":8}, {\"type\":\"horizontal_distance\",\"a\":\"origin\",\"b\":\"s1.center\",\"value\":20}. \
`help` topic `sketch` lists every constraint type.";

const CREATE: Tool = Tool {
    name: "sketch_create",
    title: "Create a sketch with shapes",
    description: "Creates a 2D sketch on a plane and draws shapes on it, optionally with constraints and dimensions, in \
one call. Planes: `xy` (top; sketch x→X, y→Y, extrudes toward +Z), `xz` (front; x→X, y→Z, extrudes toward -Y), `yz` \
(right; x→Y, y→Z, extrudes toward +X), or a planar face of the solid with `on_face` (number from `cad_geometry`), or a \
reference plane operation with `on_plane`. Returns the sketch id, the ids/names of every point and entity, whether it is \
fully defined and its closed regions (what `cad_extrude`/`cad_revolve` will use). Shapes drawn at exact coordinates \
are already where they should be; constraints are optional and make the sketch hold its shape when edited.",
    input: || {
        object(&[
            tab_prop(),
            ("plane", one_of("Sketch plane (default xy). Ignored with on_face / on_plane", &["xy", "xz", "yz"]), false),
            ("on_face", integer("Draw on this planar face of the solid (number from `cad_geometry`)", 0), false),
            ("on_plane", id_or_name("Draw on this reference plane operation (id or name)"), false),
            ("offset", number("Move the sketch plane along its normal (mm)"), false),
            ("name", string("Name shown to the user (optional)"), false),
            ("shapes", array(SHAPES_DOC, any_object("A shape: {\"type\": ..., ...}")), false),
            ("constraints", array(CONSTRAINTS_DOC, any_object("A constraint: {\"type\": ..., ...}")), false),
        ])
    },
    effect: Effect::Change,
    run: create,
};

const ADD: Tool = Tool {
    name: "sketch_add",
    title: "Add shapes or constraints to a sketch",
    description: "Adds shapes and/or constraints to an existing sketch (by its operation id or name). Points and \
entities already in the sketch are referred to by their ids (see the result of `sketch_create` or `cad_design` with \
`detail`); new shapes by \"s0\", \"s0.top\"… as in `sketch_create`. Operations that use the sketch (e.g. an extrusion) \
update. Example: {\"sketch\": 0, \"constraints\": [{\"type\":\"length\",\"line\":5,\"value\":30}]}.",
    input: || {
        object(&[
            tab_prop(),
            ("sketch", id_or_name("The sketch operation (id or name)"), true),
            ("shapes", array(SHAPES_DOC, any_object("A shape")), false),
            ("constraints", array(CONSTRAINTS_DOC, any_object("A constraint")), false),
            ("remove_constraints", array("Indices (in the sketch's constraint list) to delete", integer("index", 0)), false),
        ])
    },
    effect: Effect::Change,
    run: add,
};

/// Nombres → ids de lo dibujado en esta llamada
type Names = HashMap<String, u32>;

fn p2(v: &Value, what: &str) -> Result<P2, Fail> {
    match v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()) {
        Some(c) if c.len() == 2 => Ok([c[0], c[1]]),
        _ => Err(Fail::new(format!("{what}: expected [x, y], got {v}"))),
    }
}

fn f64_of(shape: &Value, key: &str) -> Option<f64> {
    shape.get(key).and_then(Value::as_f64)
}

fn need(shape: &Value, key: &str, i: usize) -> Result<f64, Fail> {
    f64_of(shape, key).ok_or_else(|| Fail::new(format!("shape {i}: missing number \"{key}\"")))
}

/// Dibuja las figuras; deja sus nombres en `names` y devuelve una línea por figura
fn draw(sketch: &mut Sketch, shapes: &[Value], names: &mut Names) -> Result<Vec<String>, Fail> {
    let mut lines = Vec::new();
    let base = names.keys().filter(|k| k.starts_with('s') && !k.contains('.')).count();
    for (k, shape) in shapes.iter().enumerate() {
        let i = base + k;
        let ty = shape.get("type").and_then(Value::as_str).unwrap_or("");
        let at = |key: &str| shape.get(key).map(|v| p2(v, &format!("shape {i}.{key}")));
        let mut named: Vec<(String, u32)> = Vec::new();
        let mut entities: Vec<u32> = Vec::new();
        match ty {
            "line" => {
                let (a, b) = (at("from").ok_or_else(|| Fail::new(format!("shape {i}: missing \"from\"")))??, at("to").ok_or_else(|| Fail::new(format!("shape {i}: missing \"to\"")))??);
                let e = sketch.line(a, b);
                let (s, t) = ends(sketch, e);
                named.extend([(String::new(), e), ("start".into(), s), ("end".into(), t)]);
                entities.push(e);
            }
            "rectangle" => {
                let (a, b) = match (at("corner"), at("opposite"), at("center")) {
                    (Some(a), Some(b), _) => (a?, b?),
                    (_, _, Some(c)) => {
                        let c = c?;
                        let (w, h) = (need(shape, "width", i)?, need(shape, "height", i)?);
                        ([c[0] - w / 2.0, c[1] - h / 2.0], [c[0] + w / 2.0, c[1] + h / 2.0])
                    }
                    _ => return Err(Fail::new(format!("shape {i}: a rectangle needs \"corner\"+\"opposite\" or \"center\"+\"width\"+\"height\""))),
                };
                let l = sketch.rectangle(a, b);
                for (n, e) in ["bottom", "right", "top", "left"].iter().zip(l) {
                    named.push(((*n).into(), e));
                }
                let corners = [ends(sketch, l[0]).0, ends(sketch, l[1]).0, ends(sketch, l[2]).0, ends(sketch, l[3]).0];
                for (n, p) in ["bottom_left", "bottom_right", "top_right", "top_left"].iter().zip(corners) {
                    named.push(((*n).into(), p));
                }
                entities.extend(l);
            }
            "circle" => {
                let c = at("center").ok_or_else(|| Fail::new(format!("shape {i}: missing \"center\"")))??;
                let r = f64_of(shape, "radius").or(f64_of(shape, "diameter").map(|d| d / 2.0)).ok_or_else(|| Fail::new(format!("shape {i}: a circle needs \"radius\" or \"diameter\"")))?;
                let e = sketch.circle(c, r);
                let Geometry::Circle { center, .. } = sketch.entity(e).unwrap().geometry else { unreachable!() };
                named.extend([(String::new(), e), ("center".into(), center)]);
                entities.push(e);
            }
            "arc" => {
                let c = at("center").ok_or_else(|| Fail::new(format!("shape {i}: missing \"center\"")))??;
                let (s, t) = match (at("start"), at("end")) {
                    (Some(s), Some(t)) => (s?, t?),
                    _ => {
                        let r = need(shape, "radius", i)?;
                        let (a0, a1) = (need(shape, "start_angle", i)?.to_radians(), need(shape, "end_angle", i)?.to_radians());
                        ([c[0] + r * a0.cos(), c[1] + r * a0.sin()], [c[0] + r * a1.cos(), c[1] + r * a1.sin()])
                    }
                };
                let e = sketch.arc(c, s, t);
                let Geometry::Arc { center, start, end } = sketch.entity(e).unwrap().geometry else { unreachable!() };
                named.extend([(String::new(), e), ("center".into(), center), ("start".into(), start), ("end".into(), end)]);
                entities.push(e);
            }
            "polyline" | "polygon" => {
                let pts: Vec<P2> = if ty == "polygon" {
                    let c = at("center").ok_or_else(|| Fail::new(format!("shape {i}: missing \"center\"")))??;
                    let r = need(shape, "radius", i)?;
                    let n = shape.get("sides").and_then(Value::as_u64).filter(|n| *n >= 3).ok_or_else(|| Fail::new(format!("shape {i}: \"sides\" must be 3 or more")))?;
                    let rot = f64_of(shape, "rotation").unwrap_or(90.0).to_radians();
                    (0..n).map(|k| {
                        let a = rot + std::f64::consts::TAU * k as f64 / n as f64;
                        [c[0] + r * a.cos(), c[1] + r * a.sin()]
                    }).collect()
                } else {
                    let list = shape.get("points").and_then(Value::as_array).ok_or_else(|| Fail::new(format!("shape {i}: missing \"points\"")))?;
                    list.iter().enumerate().map(|(k, v)| p2(v, &format!("shape {i}.points[{k}]"))).collect::<Result<_, _>>()?
                };
                if pts.len() < 2 {
                    return Err(Fail::new(format!("shape {i}: needs at least 2 points")));
                }
                let closed = ty == "polygon" || shape.get("closed").and_then(Value::as_bool).unwrap_or(false);
                let ids: Vec<u32> = pts.iter().map(|p| sketch.add_point(p[0], p[1])).collect();
                let segs = if closed { ids.len() } else { ids.len() - 1 };
                for k in 0..segs {
                    let e = sketch.add_line(ids[k], ids[(k + 1) % ids.len()]);
                    named.push((format!("line{k}"), e));
                    entities.push(e);
                }
                for (k, p) in ids.iter().enumerate() {
                    named.push((format!("p{k}"), *p));
                }
            }
            "slot" => {
                let (a, b) = (at("from").ok_or_else(|| Fail::new(format!("shape {i}: missing \"from\"")))??, at("to").ok_or_else(|| Fail::new(format!("shape {i}: missing \"to\"")))??);
                let r = need(shape, "width", i)? / 2.0;
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-9 || r <= 0.0 {
                    return Err(Fail::new(format!("shape {i}: a slot needs two different points and a positive width")));
                }
                let n = [-dy / len * r, dx / len * r];
                let pt = |c: P2, s: f64| [c[0] + s * n[0], c[1] + s * n[1]];
                let (ca, cb) = (sketch.add_point(a[0], a[1]), sketch.add_point(b[0], b[1]));
                let (a_up, b_up, b_dn, a_dn) = (pt(a, 1.0), pt(b, 1.0), pt(b, -1.0), pt(a, -1.0));
                let ids = [a_up, b_up, b_dn, a_dn].map(|p| sketch.add_point(p[0], p[1]));
                let side1 = sketch.add_line(ids[0], ids[1]);
                let cap_b = sketch.add_entity(Geometry::Arc { center: cb, start: ids[2], end: ids[1] });
                let side2 = sketch.add_line(ids[2], ids[3]);
                let cap_a = sketch.add_entity(Geometry::Arc { center: ca, start: ids[0], end: ids[3] });
                named.extend([("side1".into(), side1), ("side2".into(), side2), ("cap_from".into(), cap_a), ("cap_to".into(), cap_b), ("from".into(), ca), ("to".into(), cb)]);
                entities.extend([side1, cap_b, side2, cap_a]);
            }
            "point" => {
                let p = at("at").ok_or_else(|| Fail::new(format!("shape {i}: missing \"at\"")))??;
                let id = sketch.add_point(p[0], p[1]);
                let e = sketch.add_entity(Geometry::Point { point: id });
                named.extend([(String::new(), e), ("point".into(), id)]);
                entities.push(e);
            }
            other => {
                return Err(Fail::new(format!("shape {i}: unknown type \"{other}\"")).hint("use rectangle, circle, line, polyline, arc, polygon, slot or point"));
            }
        }
        if shape.get("construction").and_then(Value::as_bool) == Some(true) {
            for e in &entities {
                if let Some(x) = sketch.entities.iter_mut().find(|x| x.id == *e) {
                    x.construction = true;
                }
            }
        }
        let mut desc = Vec::new();
        for (n, id) in named {
            let key = if n.is_empty() { format!("s{i}") } else { format!("s{i}.{n}") };
            desc.push(format!("{key}={id}"));
            names.insert(key, id);
        }
        lines.push(format!("  s{i} {ty}: {}", desc.join(", ")));
    }
    Ok(lines)
}

fn ends(sketch: &Sketch, line: u32) -> (u32, u32) {
    match sketch.entity(line).map(|e| &e.geometry) {
        Ok(Geometry::Line { start, end }) => (*start, *end),
        _ => (0, 0),
    }
}

/// Campos de las restricciones que llevan ids de puntos o entidades
const ID_FIELDS: &[&str] = &["a", "b", "line", "point", "entity", "circle", "arc", "curve", "center", "a1", "a2", "b1", "b2"];

/// Restricción con nombres («s0.top», «origin») → con ids
fn constraint(raw: &Value, names: &Names, k: usize) -> Result<SketchConstraint, Fail> {
    let mut v = raw.clone();
    let obj = v.as_object_mut().ok_or_else(|| Fail::new(format!("constraint {k}: expected an object")))?;
    let lookup = |s: &str| -> Result<Value, Fail> {
        names.get(s.trim()).map(|id| json!(id)).ok_or_else(|| {
            let mut known: Vec<&String> = names.keys().collect();
            known.sort();
            Fail::new(format!("constraint {k}: unknown name \"{s}\"")).hint(format!("names in this call: {}", known.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")))
        })
    };
    for field in ID_FIELDS {
        if let Some(Value::String(s)) = obj.get(*field) {
            let id = lookup(&s.clone())?;
            obj.insert((*field).into(), id);
        }
    }
    if let Some(Value::Array(list)) = obj.get_mut("entities") {
        for x in list.iter_mut() {
            if let Value::String(s) = x {
                *x = lookup(&s.clone())?;
            }
        }
    }
    serde_json::from_value(v).map_err(|e| Fail::new(format!("constraint {k} ({}): {e}", raw.get("type").and_then(Value::as_str).unwrap_or("?"))).hint("see `help` topic `sketch` for the fields of each constraint type"))
}

/// Cotas del sketch (las restricciones con valor) con su índice y la ruta para
/// atarlas a un parámetro con `cad_parameters`
fn dimension_lines(sketch: &Sketch, feature: u64) -> Vec<String> {
    sketch
        .constraints
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let v = c.value()?;
            let ty = serde_json::to_value(c).ok()?["type"].as_str()?.to_string();
            let field = if ty == "angle" { "degrees" } else { "value" };
            Some(format!("  [{i}] {ty} = {} → formula path \"{feature}.sketch.constraints.{i}.{field}\"", num(v)))
        })
        .collect()
}

fn base_names(sketch: &mut Sketch) -> Names {
    let mut names = Names::new();
    names.insert("origin".into(), sketch.ensure_origin());
    // Las figuras ya dibujadas siguen la numeración de esta llamada desde 0
    names
}

/// Resumen de un sketch tras recalcular: estado y regiones
fn sketch_report(result: &Value, id: u32) -> (String, Value) {
    let sv = result["sketches"].as_array().and_then(|l| l.iter().find(|s| s["id"] == id)).cloned().unwrap_or_default();
    let regions: Vec<Value> = sv["regions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| json!({ "area": r["outer"]["area"], "holes": r["holes"].as_array().map_or(0, Vec::len), "depth": r["depth"], "inside_point": r["sample"] }))
        .collect();
    // Profundidad par: el perfil que se usa por defecto; impar: dentro de un agujero
    let describe = |r: &Value| {
        let holes = r["holes"].as_u64().unwrap_or(0);
        format!("{} mm² outline{} (inside point {})", num(r["area"].as_f64().unwrap_or(0.0)), if holes > 0 { format!(" with {holes} hole(s)") } else { String::new() }, super::cad::vec2(&r["inside_point"]))
    };
    let profile: Vec<String> = regions.iter().filter(|r| r["depth"].as_u64().unwrap_or(0) % 2 == 0).map(describe).collect();
    let inner: Vec<String> = regions.iter().filter(|r| r["depth"].as_u64().unwrap_or(0) % 2 == 1).map(describe).collect();
    let state = sketch_state(&sv["report"]);
    let mut text = format!("Sketch is {state}. ");
    if profile.is_empty() {
        text.push_str("No closed region yet: nothing to extrude (shapes must form closed loops).");
    } else {
        text.push_str(&format!("Profile used by default ({} region(s)): {}.", profile.len(), profile.join("; ")));
    }
    if !inner.is_empty() {
        text.push_str(&format!(" Also {} region(s) inside holes, used only if picked with `regions`: {}.", inner.len(), inner.join("; ")));
    }
    (text, json!({ "state": sv["report"]["status"], "dof": sv["report"]["dof"], "conflicting": sv["report"]["conflicting"], "regions": regions }))
}

fn create(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let d = open_design(ctx, args)?;
    let plane = if let Some(face) = args.get("on_face").and_then(Value::as_u64) {
        let mut v = json!({ "type": "face", "face": { "$face": face } });
        resolve_refs(ctx, &mut v)?;
        serde_json::from_value(v).map_err(|e| Fail::new(e.to_string()))?
    } else if let Some(p) = args.get("on_plane") {
        PlaneSpec::Reference { feature: find_feature(&d.doc, p)? }
    } else {
        match arg_str(args, "plane").unwrap_or("xy") {
            "xz" => PlaneSpec::Xz,
            "yz" => PlaneSpec::Yz,
            _ => PlaneSpec::Xy,
        }
    };
    let mut sketch = Sketch::new();
    let mut names = base_names(&mut sketch);
    let shape_lines = draw(&mut sketch, args.get("shapes").and_then(Value::as_array).map_or(&[][..], Vec::as_slice), &mut names)?;
    for (k, c) in args.get("constraints").and_then(Value::as_array).into_iter().flatten().enumerate() {
        sketch.constrain(constraint(c, &names, k)?);
    }
    let drawn = sketch.clone();
    let kind = FeatureKind::Sketch { plane, offset: args.get("offset").and_then(Value::as_f64).unwrap_or(0.0), sketch };
    let mut out = add_feature(ctx, d, kind, arg_str(args, "name"), "")?;
    let id = out.data["feature"].as_u64().unwrap_or(0) as u32;
    let dims = dimension_lines(&drawn, id as u64);
    let result = ctx.call("cad_evaluate", json!({}))?;
    let (state, report) = sketch_report(&result, id);
    let mut text = vec![out.text.lines().next().unwrap_or("").to_string(), format!("Origin point = {}.", names["origin"])];
    if !shape_lines.is_empty() {
        text.push("Ids of what was drawn (name=id):".into());
        text.extend(shape_lines);
    }
    if !dims.is_empty() {
        text.push("Dimensions (index, value, path for `cad_parameters` formulas):".into());
        text.extend(dims);
    }
    text.push(state);
    text.push(format!("Next: `cad_extrude` or `cad_revolve` with {{\"sketch\": {id}}}."));
    out.text = text.join("\n");
    out.data["names"] = json!(names);
    out.data["sketch"] = report;
    Ok(out)
}

fn add(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut d: Design = open_design(ctx, args)?;
    let id = find_feature(&d.doc, &args["sketch"])?;
    let before: Vec<u64> = ctx.call("cad_evaluate", json!({}))?["status"].as_array().into_iter().flatten().filter(|s| s["state"] == "error").filter_map(|s| s["id"].as_u64()).collect();
    let fname = d.doc.get(id).map(|f| f.name.clone()).unwrap_or_default();
    let Some(FeatureKind::Sketch { sketch, .. } | FeatureKind::SurfaceSketch { sketch, .. }) = d.doc.get_mut(id).map(|f| &mut f.kind) else {
        return Err(Fail::new(format!("\"{fname}\" is not a sketch")));
    };
    let mut names = base_names(sketch);
    let mut removed = 0;
    if let Some(list) = args.get("remove_constraints").and_then(Value::as_array) {
        let mut idx: Vec<usize> = list.iter().filter_map(Value::as_u64).map(|i| i as usize).collect();
        idx.sort_unstable_by(|a, b| b.cmp(a));
        idx.dedup();
        for i in idx {
            if i >= sketch.constraints.len() {
                return Err(Fail::new(format!("the sketch has no constraint {i}")).hint(format!("it has {}", sketch.constraints.len())));
            }
            sketch.constraints.remove(i);
            removed += 1;
        }
    }
    let shape_lines = draw(sketch, args.get("shapes").and_then(Value::as_array).map_or(&[][..], Vec::as_slice), &mut names)?;
    let mut added = 0;
    for (k, c) in args.get("constraints").and_then(Value::as_array).into_iter().flatten().enumerate() {
        sketch.constrain(constraint(c, &names, k)?);
        added += 1;
    }
    if shape_lines.is_empty() && added == 0 && removed == 0 {
        return Err(Fail::new("nothing to add").hint("pass `shapes`, `constraints` or `remove_constraints`"));
    }
    let origin = names["origin"];
    let dims = dimension_lines(sketch, id.0 as u64);
    let result = apply(ctx, &d)?;
    let mut out = finish(ctx, &d, &result, &before, Some((id, &fname)), &format!("Changed sketch \"{fname}\" (feature {}): {} shape(s), {added} constraint(s) added, {removed} removed.", id.0, shape_lines.len()), "")?;
    let (state, report) = sketch_report(&result, id.0);
    let mut text = vec![out.text.lines().next().unwrap_or("").to_string(), format!("Origin point = {origin}.")];
    if !shape_lines.is_empty() {
        text.push("Ids of what was drawn (name=id):".into());
        text.extend(shape_lines);
    }
    if !dims.is_empty() {
        text.push("Dimensions (index, value, path for `cad_parameters` formulas):".into());
        text.extend(dims);
    }
    text.push(state);
    text.extend(out.text.lines().skip(1).filter(|l| l.starts_with("Warning")).map(str::to_string));
    out.text = text.join("\n");
    out.data["names"] = json!(names);
    out.data["sketch"] = report;
    Ok(out)
}
