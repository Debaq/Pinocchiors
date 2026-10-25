//! Diseño CAD: pestañas, leer el diseño, caras y aristas, operaciones,
//! parámetros y exportar. Lo común a las herramientas de CAD está acá.

use std::path::PathBuf;

use cad_model::{Document, FeatureId, FeatureKind};
use serde_json::{json, Map, Value};

use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::schema::{any_object, array, boolean, id_or_name, integer, object, one_of, string, Prop};

pub const TOOLS: &[Tool] = &[TAB_CREATE, TAB_RENAME, TAB_DELETE, DESIGN, GEOMETRY, ADD_FEATURE, EDIT_FEATURE, DELETE_FEATURE, PARAMETERS, EXPORT];

// ─── Ayudantes compartidos ──────────────────────────────────────────────────

/// Un diseño abierto para cambiarlo: su pestaña (ya activa) y su documento
pub(super) struct Design {
    pub tab: u32,
    pub tab_name: String,
    pub doc: Document,
}

/// Argumento `tab` de todas las herramientas de diseño
pub(super) fn tab_prop() -> Prop {
    ("tab", id_or_name("Design tab, by id or name (see `status`). Default: the active tab; if there are no tabs, a design tab is created"), false)
}

/// Una pestaña: (id, nombre, "design" | "assembly")
fn find_tab(ctx: &Ctx, want: Option<&Value>, create: bool) -> Result<(u32, String, String), Fail> {
    let mut view = ctx.call("cad_tabs", json!({}))?;
    if view["tabs"].as_array().is_none_or(|t| t.is_empty()) {
        if !create {
            return Err(Fail::new("there are no CAD tabs").hint("start a design with `sketch_create` or `cad_primitive`"));
        }
        view = ctx.call("cad_tab_add", json!({ "kind": "design", "name": null }))?;
    }
    let tabs = view["tabs"].as_array().cloned().unwrap_or_default();
    let pick = |t: &Value| (t["id"].as_u64().unwrap_or(0) as u32, t["name"].as_str().unwrap_or("").to_string(), t["kind"].as_str().unwrap_or("").to_string());
    let list = || tabs.iter().map(|t| format!("{} \"{}\" ({})", t["id"], t["name"].as_str().unwrap_or(""), t["kind"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(", ");
    let found = match want {
        None => tabs.iter().find(|t| t["id"] == view["active"]).or(tabs.first()),
        Some(Value::Number(n)) => tabs.iter().find(|t| t["id"].as_u64() == n.as_u64()),
        Some(Value::String(s)) => tabs.iter().find(|t| t["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(s.trim()))),
        Some(_) => None,
    };
    found.map(pick).ok_or_else(|| Fail::new(format!("no tab {}", want.map_or("".into(), |w| w.to_string()))).hint(format!("tabs: {}", list())))
}

/// Abre la pestaña de diseño de `args.tab` (la deja activa) con su documento
pub(super) fn open_design(ctx: &Ctx, args: &Value) -> Result<Design, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), true)?;
    if kind != "design" {
        return Err(Fail::new(format!("tab \"{tab_name}\" is an assembly, not a design")).hint("pass the id or name of a design tab (see `status`)"));
    }
    ctx.call("cad_tab_activate", json!({ "id": tab }))?;
    let doc = ctx.call("cad_tab_document", json!({ "id": tab }))?;
    let doc: Document = serde_json::from_value(doc).map_err(|e| Fail::new(format!("could not read the design of tab {tab}: {e}")))?;
    Ok(Design { tab, tab_name, doc })
}

/// Guarda el documento en su pestaña y lo recalcula; devuelve el resultado
pub(super) fn apply(ctx: &Ctx, d: &Design) -> Result<Value, Fail> {
    ctx.call("cad_set_document", json!({ "document": d.doc }))
}

/// Operaciones con error: (id, mensaje)
fn errors(result: &Value) -> Vec<(u64, String)> {
    result["status"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["state"] == "error")
        .map(|s| (s["id"].as_u64().unwrap_or(0), s["message"].as_str().unwrap_or("failed").to_string()))
        .collect()
}

fn feature_name(doc: &Document, id: u64) -> String {
    doc.get(FeatureId(id as u32)).map_or_else(|| format!("feature {id}"), |f| f.name.clone())
}

/// Número corto (sin ceros de más)
pub(super) fn num(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

pub(super) fn vec3(v: &Value) -> String {
    let n: Vec<String> = (0..3).map(|k| num(v[k].as_f64().unwrap_or(0.0))).collect();
    format!("({})", n.join(", "))
}

pub(super) fn vec2(v: &Value) -> String {
    format!("({}, {})", num(v[0].as_f64().unwrap_or(0.0)), num(v[1].as_f64().unwrap_or(0.0)))
}

/// Una dirección: «+Z» si es un eje, si no el vector
fn direction(v: &Value) -> String {
    let c: Vec<f64> = (0..3).map(|k| v[k].as_f64().unwrap_or(0.0)).collect();
    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
        if (c[k].abs() - 1.0).abs() < 1e-6 {
            return format!("{}{axis}", if c[k] > 0.0 { "+" } else { "-" });
        }
    }
    vec3(v)
}

/// Lo que quedó: piezas, caja, volumen (de un resultado de recálculo)
pub(super) fn describe_result(result: &Value) -> String {
    let parts = result["parts"].as_array().cloned().unwrap_or_default();
    let Some(body) = result["body"].as_object() else { return "No solid yet.".into() };
    let (lo, hi) = (&body["bbox_min"], &body["bbox_max"]);
    let size: Vec<String> = (0..3).map(|k| num(hi[k].as_f64().unwrap_or(0.0) - lo[k].as_f64().unwrap_or(0.0))).collect();
    let names: Vec<String> = parts.iter().map(|p| format!("\"{}\" (part {}:{})", p["name"].as_str().unwrap_or(""), p["id"]["feature"], p["id"]["index"])).collect();
    format!(
        "Solid: {} part(s) [{}]; size {} mm (box from {} to {}); volume {} mm³; {} faces, {} edges.",
        parts.len(),
        names.join(", "),
        size.join(" × "),
        vec3(lo),
        vec3(hi),
        num(body["volume"].as_f64().unwrap_or(0.0)),
        body["faces"],
        body["edges"],
    )
}

/// Cambia `{"$face": n}`, `{"$edge": n}` y `{"$vertex": [x, y, z]}` por las
/// referencias de verdad (del sólido de la pestaña activa, antes del cambio)
pub(super) fn resolve_refs(ctx: &Ctx, v: &mut Value) -> Result<(), Fail> {
    match v {
        Value::Object(map) if map.len() == 1 && (map.contains_key("$face") || map.contains_key("$edge") || map.contains_key("$vertex")) => {
            let (key, val) = map.iter().next().map(|(k, v)| (k.clone(), v.clone())).unwrap();
            *v = match key.as_str() {
                "$face" => ctx.call("cad_face_ref", json!({ "face": index(&val, "$face")? })).map_err(|e| e.hint("face numbers come from `cad_geometry` and change after every edit"))?,
                "$edge" => ctx.call("cad_edge_ref", json!({ "edge": index(&val, "$edge")? })).map_err(|e| e.hint("edge numbers come from `cad_geometry` and change after every edit"))?,
                _ => ctx.call("cad_vertex_spec", json!({ "point": val }))?,
            };
        }
        Value::Object(map) => {
            for x in map.values_mut() {
                resolve_refs(ctx, x)?;
            }
        }
        Value::Array(list) => {
            for x in list {
                resolve_refs(ctx, x)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn index(v: &Value, what: &str) -> Result<u64, Fail> {
    v.as_u64().ok_or_else(|| Fail::new(format!("{what} needs a number, got {v}")))
}

/// Agrega una operación, recalcula y cuenta qué pasó. Si la operación falla
/// es un error (el servidor deja todo como estaba).
pub(super) fn add_feature(ctx: &Ctx, mut d: Design, kind: FeatureKind, name: Option<&str>, extra: &str) -> Result<Output, Fail> {
    let before: Vec<u64> = errors(&ctx.call("cad_evaluate", json!({}))?).into_iter().map(|e| e.0).collect();
    let id = d.doc.add(kind);
    if let Some(n) = name.filter(|n| !n.trim().is_empty())
        && let Some(f) = d.doc.get_mut(id)
    {
        f.name = n.trim().to_string();
    }
    let fname = feature_name(&d.doc, id.0 as u64);
    let result = apply(ctx, &d)?;
    finish(ctx, &d, &result, &before, Some((id, &fname)), &format!("Added \"{fname}\" (feature {}) to tab \"{}\" (id {}).", id.0, d.tab_name, d.tab), extra)
}

/// Cuenta el resultado de un cambio: falla si la operación tocada falló;
/// avisa de las que se rompieron por el cambio
pub(super) fn finish(_ctx: &Ctx, d: &Design, result: &Value, before: &[u64], touched: Option<(FeatureId, &str)>, headline: &str, extra: &str) -> Result<Output, Fail> {
    let errs = errors(result);
    if let Some((id, fname)) = touched
        && let Some((_, msg)) = errs.iter().find(|e| e.0 == id.0 as u64)
    {
        return Err(Fail::new(format!("\"{fname}\" failed: {msg}")).hint("nothing was changed; check the references and sizes (`cad_design`, `cad_geometry`) and try again"));
    }
    let mut lines = vec![headline.to_string()];
    if !extra.is_empty() {
        lines.push(extra.to_string());
    }
    lines.push(describe_result(result));
    let broken: Vec<String> = errs.iter().filter(|e| !before.contains(&e.0)).map(|e| format!("\"{}\" (feature {}): {}", feature_name(&d.doc, e.0), e.0, e.1)).collect();
    if !broken.is_empty() {
        lines.push(format!("Warning — this change broke: {}. Fix them or `undo`.", broken.join("; ")));
    }
    lines.push("Face and edge numbers may have changed: call `cad_geometry` before picking faces or edges.".into());
    Ok(Output::new(
        lines.join("\n"),
        json!({
            "tab": d.tab,
            "feature": touched.map(|t| t.0 .0),
            "name": touched.map(|t| t.1),
            "parts": result["parts"].as_array().map(|ps| ps.iter().map(|p| json!({ "id": p["id"], "name": p["name"], "volume": p["volume"] })).collect::<Vec<_>>()),
            "body": result["body"],
            "errors": errs.iter().map(|e| json!({ "feature": e.0, "message": e.1 })).collect::<Vec<_>>(),
        }),
    ))
}

/// Una operación del diseño por id o nombre
pub(super) fn find_feature(doc: &Document, want: &Value) -> Result<FeatureId, Fail> {
    let found = match want {
        Value::Number(n) => doc.features.iter().find(|f| Some(f.id.0 as u64) == n.as_u64()),
        Value::String(s) => doc.features.iter().find(|f| f.name.eq_ignore_ascii_case(s.trim())),
        _ => None,
    };
    found.map(|f| f.id).ok_or_else(|| {
        let list: Vec<String> = doc.features.iter().map(|f| format!("{} \"{}\"", f.id.0, f.name)).collect();
        Fail::new(format!("no feature {want}")).hint(if list.is_empty() { "the design has no features yet".into() } else { format!("features: {}", list.join(", ")) })
    })
}

// ─── Pestañas ───────────────────────────────────────────────────────────────

const TAB_CREATE: Tool = Tool {
    name: "cad_tab_create",
    title: "Create a CAD tab",
    description: "Creates a new tab and makes it active. A `design` tab holds one parametric part design (sketches and \
operations); an `assembly` tab positions parts from design tabs. Returns the new tab id. \
Example: {\"kind\": \"design\", \"name\": \"Bracket\"}.",
    input: || object(&[("kind", one_of("design or assembly", &["design", "assembly"]), true), ("name", string("Tab name shown to the user (optional)"), false)]),
    effect: Effect::Change,
    run: tab_create,
};

fn tab_create(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let view = ctx.call("cad_tab_add", json!({ "kind": args["kind"], "name": args.get("name") }))?;
    let tab = view["tabs"].as_array().and_then(|t| t.iter().find(|t| t["id"] == view["active"])).cloned().unwrap_or_default();
    Ok(Output::new(format!("Created {} tab \"{}\" (id {}); it is now active.", args["kind"].as_str().unwrap_or(""), tab["name"].as_str().unwrap_or(""), tab["id"]), json!({ "tab": tab["id"], "name": tab["name"] })))
}

const TAB_RENAME: Tool = Tool {
    name: "cad_tab_rename",
    title: "Rename a CAD tab",
    description: "Renames a design or assembly tab. Example: {\"tab\": 0, \"name\": \"Lid\"}.",
    input: || object(&[("tab", id_or_name("Tab id or current name"), true), ("name", string("New name"), true)]),
    effect: Effect::Change,
    run: tab_rename,
};

fn tab_rename(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (id, old, _) = find_tab(ctx, args.get("tab"), false)?;
    let name = arg_str(args, "name").unwrap_or_default();
    ctx.call("cad_tab_rename", json!({ "id": id, "name": name }))?;
    Ok(Output::new(format!("Renamed tab {id} from \"{old}\" to \"{name}\"."), json!({ "tab": id, "name": name })))
}

const TAB_DELETE: Tool = Tool {
    name: "cad_tab_delete",
    title: "Delete a CAD tab",
    description: "Deletes a design or assembly tab with everything in it. Parts used by other tabs (derived parts, \
assembly instances) are reported as missing there. Can be undone. Example: {\"tab\": \"Old version\"}.",
    input: || object(&[("tab", id_or_name("Tab id or name"), true)]),
    effect: Effect::Change,
    run: tab_delete,
};

fn tab_delete(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (id, name, _) = find_tab(ctx, args.get("tab"), false)?;
    ctx.call("cad_tab_delete", json!({ "id": id }))?;
    Ok(Output::new(format!("Deleted tab \"{name}\" (id {id})."), json!({ "tab": id })))
}

// ─── Leer el diseño ─────────────────────────────────────────────────────────

const DESIGN: Tool = Tool {
    name: "cad_design",
    title: "Read a design",
    description: "Describes a design tab: its operations in order (id, name, type, main values, ok or error), sketches \
(how many entities, whether fully defined, closed regions), parameters with their values, and the resulting parts \
(size, volume). Use it to understand an existing design before changing it. With `detail: true` it also returns the \
full JSON of every operation (the format `cad_add_feature` and `cad_edit_feature` use).",
    input: || object(&[tab_prop(), ("detail", boolean("Include the full JSON of each operation (default false)"), false)]),
    effect: Effect::Read,
    run: design,
};

/// Resumen de los valores de una operación: tipo y campos simples
fn kind_summary(kind: &FeatureKind) -> String {
    let v = serde_json::to_value(kind).unwrap_or_default();
    let ty = v["type"].as_str().unwrap_or("?").to_string();
    let mut parts = Vec::new();
    let walk = |obj: &Map<String, Value>, parts: &mut Vec<String>| {
        for (k, x) in obj {
            if k == "type" || parts.len() >= 8 {
                continue;
            }
            let key = k.clone();
            match x {
                Value::Number(n) => parts.push(format!("{key} {}", num(n.as_f64().unwrap_or(0.0)))),
                Value::Bool(b) => parts.push(format!("{key} {b}")),
                Value::String(s) if s.len() < 24 => parts.push(format!("{key} {s}")),
                Value::Object(o) if o.get("type").is_some_and(|t| t.is_string()) && o.len() <= 3 => {
                    let inner: Vec<String> = o.iter().filter(|(k, _)| *k != "type").filter_map(|(k, v)| v.as_f64().map(|n| format!("{k} {}", num(n)))).collect();
                    parts.push(format!("{key} {}{}", o["type"].as_str().unwrap_or(""), if inner.is_empty() { String::new() } else { format!(" ({})", inner.join(", ")) }));
                }
                Value::Array(a) if !a.is_empty() && a.len() <= 3 && a.iter().all(Value::is_number) => {
                    parts.push(format!("{key} {}", vec3(x)));
                }
                Value::Array(a) if !a.is_empty() => parts.push(format!("{key} [{} item(s)]", a.len())),
                _ => {}
            }
        }
    };
    if let Some(obj) = v.as_object() {
        walk(obj, &mut parts);
    }
    if parts.is_empty() { ty } else { format!("{ty}: {}", parts.join(", ")) }
}

fn design(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    if kind != "design" {
        return Err(Fail::new(format!("tab \"{tab_name}\" is an assembly")).hint("assembly tools are not available yet; pass a design tab"));
    }
    ctx.call("cad_tab_activate", json!({ "id": tab }))?;
    let doc: Document = serde_json::from_value(ctx.call("cad_tab_document", json!({ "id": tab }))?).map_err(|e| Fail::new(e.to_string()))?;
    let result = ctx.call("cad_evaluate", json!({}))?;
    let status = |id: u32| -> String {
        let s = result["status"].as_array().and_then(|l| l.iter().find(|s| s["id"] == id)).cloned().unwrap_or_default();
        match s["state"].as_str() {
            Some("ok") | None => "ok".into(),
            Some("error") => format!("ERROR: {}", s["message"].as_str().unwrap_or("")),
            Some("warning") => format!("warning: {}", s["message"].as_str().unwrap_or("")),
            Some(other) => other.to_string(),
        }
    };
    let mut lines = vec![format!("Design tab \"{tab_name}\" (id {tab}), {} operation(s):", doc.features.len())];
    for f in &doc.features {
        let what = match &f.kind {
            FeatureKind::Sketch { plane, offset, sketch } => {
                let sv = result["sketches"].as_array().and_then(|l| l.iter().find(|s| s["id"] == f.id.0)).cloned().unwrap_or_default();
                let plane = match plane {
                    cad_model::PlaneSpec::Xy => "plane XY".to_string(),
                    cad_model::PlaneSpec::Xz => "plane XZ".to_string(),
                    cad_model::PlaneSpec::Yz => "plane YZ".to_string(),
                    other => format!("plane {}", serde_json::to_value(other).unwrap_or_default()["type"].as_str().unwrap_or("custom")),
                };
                format!(
                    "sketch on {plane}{}: {} entities, {}, {} closed region(s)",
                    if *offset != 0.0 { format!(" offset {} mm", num(*offset)) } else { String::new() },
                    sketch.entities.len(),
                    sketch_state(&sv["report"]),
                    sv["regions"].as_array().map_or(0, Vec::len)
                )
            }
            other => kind_summary(other),
        };
        lines.push(format!("  [{}] \"{}\"{} — {what} — {}", f.id.0, f.name, if f.suppressed { " (suppressed)" } else { "" }, status(f.id.0)));
    }
    if !doc.parameters.is_empty() {
        let vals: Vec<String> = doc
            .parameters
            .iter()
            .map(|p| {
                let r = result["parameters"].as_array().and_then(|l| l.iter().find(|r| r["key"] == p.name.as_str())).cloned().unwrap_or_default();
                match r["value"].as_f64() {
                    Some(v) if p.expr.trim() != num(v) => format!("{} = {} (= {})", p.name, p.expr, num(v)),
                    Some(_) => format!("{} = {}", p.name, p.expr),
                    None => format!("{} = {} (ERROR: {})", p.name, p.expr, r["error"].as_str().unwrap_or("?")),
                }
            })
            .collect();
        lines.push(format!("Parameters: {}.", vals.join(", ")));
    }
    if !doc.bindings.is_empty() {
        let b: Vec<String> = doc.bindings.iter().map(|(k, v)| format!("{k} = {v}")).collect();
        lines.push(format!("Formulas: {}.", b.join(", ")));
    }
    lines.push(describe_result(&result));
    let mut data = json!({
        "tab": tab,
        "name": tab_name,
        "features": doc.features.iter().map(|f| json!({ "id": f.id.0, "name": f.name, "type": serde_json::to_value(&f.kind).unwrap_or_default()["type"], "suppressed": f.suppressed, "status": status(f.id.0) })).collect::<Vec<_>>(),
        "parameters": result["parameters"],
        "parts": result["parts"],
        "body": result["body"],
    });
    if arg_bool(args, "detail") == Some(true) {
        let mut full = serde_json::to_value(&doc.features).unwrap_or_default();
        strip_blobs(&mut full);
        data["detail"] = full;
        lines.push("Full JSON of the operations is in the structured result (`detail`).".into());
    }
    Ok(Output::new(lines.join("\n"), data))
}

/// Estado de un sketch resuelto, en palabras
pub(super) fn sketch_state(report: &Value) -> String {
    match report["status"].as_str() {
        Some("well_constrained") => "fully defined".into(),
        Some("under_constrained") => format!("not fully defined ({} degree(s) of freedom left; fine for extruding)", report["dof"]),
        Some("over_constrained") => format!("CONFLICTING constraints (indices {})", report["conflicting"]),
        Some("failed") => "solver FAILED".into(),
        _ => "not solved".into(),
    }
}

/// Saca los datos binarios grandes (archivos importados, mallas) del JSON
fn strip_blobs(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                if matches!(k.as_str(), "data" | "mesh" | "triangles" | "vertices") && (x.is_array() || x.is_object()) {
                    *x = json!("<omitted>");
                } else {
                    strip_blobs(x);
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(strip_blobs),
        _ => {}
    }
}

// ─── Caras y aristas ────────────────────────────────────────────────────────

const GEOMETRY: Tool = Tool {
    name: "cad_geometry",
    title: "List faces and edges",
    description: "Lists the faces and/or edges of the solid in a design tab with a number for each, what made them \
(e.g. `Extrusión 1 · end`, `side of sketch entity 7`), surface/curve type, normal, centre, area or length and radius. \
Use these numbers to pick faces and edges in other tools (`{\"$face\": n}`, `{\"$edge\": n}`, `on_face`, `edges`). \
Numbers are only valid until the next change. Filter with `feature` (only what that operation made) or `part`, and \
`facing` (e.g. \"+Z\" = faces whose normal points up). Example: {\"kind\": \"faces\", \"facing\": \"+Z\"}.",
    input: || {
        object(&[
            tab_prop(),
            ("kind", one_of("What to list (default both)", &["faces", "edges", "both"]), false),
            ("feature", id_or_name("Only faces/edges made by this operation (id or name)"), false),
            ("part", string("Only this part, as \"feature:index\" (e.g. \"2:0\") or its name"), false),
            ("facing", one_of("Only faces whose normal points this way (planar faces)", &["+X", "-X", "+Y", "-Y", "+Z", "-Z"]), false),
            ("limit", integer("Maximum items per list (default 80)", 1), false),
        ])
    },
    effect: Effect::Read,
    run: geometry,
};

/// Origen de una cara en palabras: «Extrusión 1 · end»
fn origin_text(doc: &Document, tags: &Value) -> String {
    let words: Vec<String> = tags
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let raw = t["name"].as_str().unwrap_or("");
            let what = match raw {
                "inicio" => "start".to_string(),
                "fin" => "end".to_string(),
                "piel" => "skin".to_string(),
                "borde" => "rim".to_string(),
                "corte" => "cut face".to_string(),
                "chaflan" => "chamfer".to_string(),
                r if r.starts_with("lado:") => format!("side of sketch entity {}", &r[5..]),
                r if r.starts_with("redondeo") => "fillet".to_string(),
                r => r.to_string(),
            };
            format!("{} · {what}", feature_name(doc, t["feature"].as_u64().unwrap_or(0)))
        })
        .collect();
    if words.is_empty() { "unknown origin".into() } else { words.join(" + ") }
}

fn geometry(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    if kind != "design" {
        return Err(Fail::new(format!("tab \"{tab_name}\" is an assembly")).hint("pass a design tab"));
    }
    ctx.call("cad_tab_activate", json!({ "id": tab }))?;
    let doc: Document = serde_json::from_value(ctx.call("cad_tab_document", json!({ "id": tab }))?).map_err(|e| Fail::new(e.to_string()))?;
    let geo = ctx.call("cad_geometry", json!({}))?;
    let parts = ctx.call("cad_evaluate", json!({}))?["parts"].clone();
    let what = arg_str(args, "kind").unwrap_or("both");
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(80) as usize;
    let feature = match args.get("feature") {
        Some(f) => Some(find_feature(&doc, f)?.0 as u64),
        None => None,
    };
    let part = match arg_str(args, "part") {
        Some(p) => Some(find_part(&parts, p)?),
        None => None,
    };
    let facing = arg_str(args, "facing").map(|f| {
        let k = match &f[1..] { "X" => 0, "Y" => 1, _ => 2 };
        (k, if f.starts_with('-') { -1.0 } else { 1.0 })
    });
    let in_part = |x: &Value| part.as_ref().is_none_or(|p| &x["part"] == p);
    let faces: Vec<&Value> = geo["faces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| in_part(f))
        .filter(|f| feature.is_none_or(|id| f["origin"].as_array().is_some_and(|o| o.iter().any(|t| t["feature"].as_u64() == Some(id)))))
        .filter(|f| facing.is_none_or(|(k, s)| f["surface"] == "plane" && (f["normal"][k].as_f64().unwrap_or(0.0) * s) > 0.999))
        .collect();
    let face_ids: Vec<u64> = faces.iter().filter_map(|f| f["index"].as_u64()).collect();
    let edges: Vec<&Value> = geo["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| in_part(e))
        .filter(|e| feature.is_none() && facing.is_none() || e["faces"].as_array().is_some_and(|fs| fs.iter().any(|f| f.as_u64().is_some_and(|f| face_ids.contains(&f)))))
        .collect();

    let mut lines = vec![format!("Tab \"{tab_name}\" (id {tab}). Numbers are valid until the next change.")];
    if what != "edges" {
        lines.push(format!("Faces ({}{}):", faces.len(), if faces.len() > limit { format!(", first {limit}") } else { String::new() }));
        for f in faces.iter().take(limit) {
            let radius = f["radius"].as_f64().map_or(String::new(), |r| format!(", radius {}", num(r)));
            lines.push(format!(
                "  face {}: {} — {}; normal {}, centre {}, area {} mm²{radius}",
                f["index"],
                f["surface"].as_str().unwrap_or(""),
                origin_text(&doc, &f["origin"]),
                direction(&f["normal"]),
                vec3(&f["center"]),
                num(f["area"].as_f64().unwrap_or(0.0))
            ));
        }
    }
    if what != "faces" {
        lines.push(format!("Edges ({}{}):", edges.len(), if edges.len() > limit { format!(", first {limit}") } else { String::new() }));
        for e in edges.iter().take(limit) {
            let fs: Vec<String> = e["faces"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|f| f.to_string()).collect();
            let shape = match (e["curve"].as_str(), e["radius"].as_f64(), e["closed"].as_bool()) {
                (Some("circle"), Some(r), Some(true)) => format!("circle radius {} centred near {}", num(r), vec3(&e["mid"])),
                (Some("circle"), Some(r), _) => format!("arc radius {}, {} mm, from {} to {}", num(r), num(e["length"].as_f64().unwrap_or(0.0)), vec3(&e["start"]), vec3(&e["end"])),
                (c, _, _) => format!("{} {} mm from {} to {}", c.unwrap_or("curve"), num(e["length"].as_f64().unwrap_or(0.0)), vec3(&e["start"]), vec3(&e["end"])),
            };
            lines.push(format!("  edge {}: {shape}; between faces {}", e["index"], fs.join(" and ")));
        }
    }
    if geo["faces"].as_array().is_none_or(|f| f.is_empty()) {
        lines.push("There is no solid yet.".into());
    }
    let data = json!({
        "tab": tab,
        "faces": if what != "edges" { json!(faces.iter().take(limit).collect::<Vec<_>>()) } else { json!([]) },
        "edges": if what != "faces" { json!(edges.iter().take(limit).collect::<Vec<_>>()) } else { json!([]) },
    });
    Ok(Output::new(lines.join("\n"), data))
}

/// Pieza por «feature:index» o por nombre → su id como JSON
pub(super) fn find_part(parts: &Value, want: &str) -> Result<Value, Fail> {
    let list = parts.as_array().cloned().unwrap_or_default();
    let found = list.iter().find(|p| {
        let id = format!("{}:{}", p["id"]["feature"], p["id"]["index"]);
        id == want.trim() || p["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(want.trim()))
    });
    found.map(|p| p["id"].clone()).ok_or_else(|| {
        let names: Vec<String> = list.iter().map(|p| format!("{}:{} \"{}\"", p["id"]["feature"], p["id"]["index"], p["name"].as_str().unwrap_or(""))).collect();
        Fail::new(format!("no part \"{want}\"")).hint(if names.is_empty() { "the design has no parts yet".into() } else { format!("parts: {}", names.join(", ")) })
    })
}

// ─── Operaciones en general ─────────────────────────────────────────────────

const ADD_FEATURE: Tool = Tool {
    name: "cad_add_feature",
    title: "Add any operation (raw JSON)",
    description: "Adds any operation to a design from its JSON definition — for operations without their own tool \
(pattern, mirror, hole, sweep, loft, draft, split, boolean, transform, helix, thread, reference planes/axes/points, \
gears…). The JSON has a `type` and that operation's fields; `help` topic `features` lists every type with an example. \
Faces, edges and vertices are written as `{\"$face\": n}`, `{\"$edge\": n}` (numbers from `cad_geometry`) or \
`{\"$vertex\": [x, y, z]}`. Lengths mm, angles degrees. \
Example: {\"feature\": {\"type\": \"mirror\", \"features\": [], \"plane\": {\"type\": \"yz\"}}}.",
    input: || object(&[tab_prop(), ("feature", any_object("The operation: {\"type\": ..., ...fields}"), true), ("name", string("Name shown to the user (optional)"), false)]),
    effect: Effect::Change,
    run: add_any,
};

fn add_any(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let d = open_design(ctx, args)?;
    let mut raw = args["feature"].clone();
    resolve_refs(ctx, &mut raw)?;
    let kind: FeatureKind = serde_json::from_value(raw)
        .map_err(|e| Fail::new(format!("invalid operation: {e}")).hint("see `help` with topic `features` for the fields of each type"))?;
    add_feature(ctx, d, kind, arg_str(args, "name"), "")
}

const EDIT_FEATURE: Tool = Tool {
    name: "cad_edit_feature",
    title: "Change an operation",
    description: "Changes an existing operation: rename it, suppress/unsuppress it, and/or change its fields with \
`changes` (merged into its JSON: only the fields you pass change; `null` removes an optional field). See the current \
JSON with `cad_design` {\"detail\": true}. Faces/edges in `changes` use `{\"$face\": n}` / `{\"$edge\": n}`. \
Example: {\"feature\": \"Extrusión 1\", \"changes\": {\"extent\": {\"type\": \"blind\", \"distance\": 25}}}.",
    input: || {
        object(&[
            tab_prop(),
            ("feature", id_or_name("Operation id or name"), true),
            ("changes", any_object("Fields to change in the operation's JSON (merge)"), false),
            ("name", string("New name"), false),
            ("suppressed", boolean("true = skip this operation when computing; false = use it again"), false),
        ])
    },
    effect: Effect::Change,
    run: edit_feature,
};

/// Mezcla `patch` en `target` (JSON merge patch)
fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

fn edit_feature(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut d = open_design(ctx, args)?;
    let id = find_feature(&d.doc, &args["feature"])?;
    let before: Vec<u64> = errors(&ctx.call("cad_evaluate", json!({}))?).into_iter().map(|e| e.0).collect();
    let mut changed = Vec::new();
    if let Some(changes) = args.get("changes") {
        let mut patch = changes.clone();
        resolve_refs(ctx, &mut patch)?;
        let f = d.doc.get_mut(id).unwrap();
        let mut kind = serde_json::to_value(&f.kind).unwrap_or_default();
        if patch.get("type").is_some_and(|t| t != &kind["type"]) {
            return Err(Fail::new("`changes` cannot change the operation type").hint("delete it and add a new one instead"));
        }
        merge(&mut kind, &patch);
        f.kind = serde_json::from_value(kind).map_err(|e| Fail::new(format!("the changed operation is not valid: {e}")).hint("see its current JSON with `cad_design` {\"detail\": true}"))?;
        changed.push("fields");
    }
    if let Some(name) = arg_str(args, "name") {
        d.doc.get_mut(id).unwrap().name = name.trim().to_string();
        changed.push("name");
    }
    if let Some(s) = arg_bool(args, "suppressed") {
        d.doc.set_suppressed(id, s).map_err(|e| Fail::new(e.to_string()))?;
        changed.push(if s { "suppressed" } else { "unsuppressed" });
    }
    if changed.is_empty() {
        return Err(Fail::new("nothing to change").hint("pass `changes`, `name` or `suppressed`"));
    }
    let fname = feature_name(&d.doc, id.0 as u64);
    let result = apply(ctx, &d)?;
    finish(ctx, &d, &result, &before, Some((id, &fname)), &format!("Changed \"{fname}\" (feature {}): {}.", id.0, changed.join(", ")), "")
}

const DELETE_FEATURE: Tool = Tool {
    name: "cad_delete_feature",
    title: "Delete an operation",
    description: "Deletes an operation from a design. If later operations use it (e.g. an extrusion uses its sketch) \
it refuses and lists them, unless `with_dependents: true`, which deletes those too. Can be undone. \
Example: {\"feature\": 3}.",
    input: || {
        object(&[
            tab_prop(),
            ("feature", id_or_name("Operation id or name"), true),
            ("with_dependents", boolean("Also delete the operations that use it (default false)"), false),
        ])
    },
    effect: Effect::Change,
    run: delete_feature,
};

fn delete_feature(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut d = open_design(ctx, args)?;
    let id = find_feature(&d.doc, &args["feature"])?;
    let before: Vec<u64> = errors(&ctx.call("cad_evaluate", json!({}))?).into_iter().map(|e| e.0).collect();
    let mut gone = vec![feature_name(&d.doc, id.0 as u64)];
    let deps = d.doc.dependents(id);
    if !deps.is_empty() {
        if arg_bool(args, "with_dependents") != Some(true) {
            let names: Vec<String> = deps.iter().map(|f| format!("{} \"{}\"", f.0, feature_name(&d.doc, f.0 as u64))).collect();
            return Err(Fail::new(format!("other operations use it: {}", names.join(", "))).hint("pass `with_dependents: true` to delete them too, or change them first"));
        }
        // Las que dependen, de la última a la primera
        let mut all = deps.clone();
        all.sort_by_key(|f| std::cmp::Reverse(d.doc.index_of(*f)));
        for f in all {
            gone.push(feature_name(&d.doc, f.0 as u64));
            remove_with_dependents(&mut d.doc, f);
        }
    }
    remove_with_dependents(&mut d.doc, id);
    let result = apply(ctx, &d)?;
    finish(ctx, &d, &result, &before, None, &format!("Deleted {}.", gone.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ")), "")
}

/// Borra `id` y, antes, lo que dependa de él (de atrás hacia adelante)
fn remove_with_dependents(doc: &mut Document, id: FeatureId) {
    if doc.get(id).is_none() {
        return;
    }
    for dep in doc.dependents(id) {
        remove_with_dependents(doc, dep);
    }
    let _ = doc.remove(id);
}

// ─── Parámetros ─────────────────────────────────────────────────────────────

const PARAMETERS: Tool = Tool {
    name: "cad_parameters",
    title: "Set parameters and formulas",
    description: "Named parameters of a design (e.g. `width = 40`, `height = width / 2`) and formulas that drive \
operation fields from them. `set` adds or changes parameters (name → expression or number); `remove` deletes them; \
`formulas` binds a field to an expression, the field written as \"<feature id>.<path in its JSON>\" (e.g. \
\"3.extent.distance\" or a sketch dimension \"0.sketch.constraints.4.value\"); an empty expression unbinds it. \
Expressions use + - * / ^, parentheses, sqrt, sin/cos/tan (degrees), pi and other parameter names. \
Example: {\"set\": {\"width\": \"40\", \"wall\": \"width / 10\"}, \"formulas\": {\"3.extent.distance\": \"wall * 2\"}}.",
    input: || {
        object(&[
            tab_prop(),
            ("set", any_object("name → expression (string or number)"), false),
            ("remove", array("Parameter names to delete", string("name")), false),
            ("formulas", any_object("\"<feature id>.<field path>\" → expression; \"\" removes the formula"), false),
        ])
    },
    effect: Effect::Change,
    run: parameters,
};

fn parameters(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut d = open_design(ctx, args)?;
    let before: Vec<u64> = errors(&ctx.call("cad_evaluate", json!({}))?).into_iter().map(|e| e.0).collect();
    let mut done = Vec::new();
    for name in args.get("remove").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let n = d.doc.parameters.len();
        d.doc.parameters.retain(|p| p.name != name);
        if d.doc.parameters.len() == n {
            return Err(Fail::new(format!("no parameter \"{name}\"")));
        }
        done.push(format!("removed {name}"));
    }
    for (name, expr) in args.get("set").and_then(Value::as_object).into_iter().flatten() {
        let expr = match expr {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            other => return Err(Fail::new(format!("parameter {name}: expected an expression, got {other}"))),
        };
        if !cad_model::expr::valid_name(name) {
            return Err(Fail::new(format!("\"{name}\" is not a valid parameter name")).hint("use letters, digits and _, starting with a letter"));
        }
        match d.doc.parameters.iter_mut().find(|p| &p.name == name) {
            Some(p) => p.expr = expr.clone(),
            None => d.doc.parameters.push(cad_model::Parameter { name: name.clone(), expr: expr.clone() }),
        }
        done.push(format!("{name} = {expr}"));
    }
    for (path, expr) in args.get("formulas").and_then(Value::as_object).into_iter().flatten() {
        let expr = expr.as_str().map(str::to_string).or_else(|| expr.as_f64().map(|n| n.to_string())).unwrap_or_default();
        let path = normalize_path(&d.doc, path)?;
        if expr.trim().is_empty() {
            d.doc.bindings.remove(&path);
            done.push(format!("unbound {path}"));
        } else {
            d.doc.bindings.insert(path.clone(), expr.clone());
            done.push(format!("{path} = {expr}"));
        }
    }
    if done.is_empty() {
        return Err(Fail::new("nothing to change").hint("pass `set`, `remove` or `formulas`"));
    }
    let result = apply(ctx, &d)?;
    let bad: Vec<String> = result["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(result["bindings"].as_array().into_iter().flatten())
        .filter(|r| r["error"].is_string())
        .map(|r| format!("{}: {}", r["key"].as_str().unwrap_or(""), r["error"].as_str().unwrap_or("")))
        .collect();
    if !bad.is_empty() {
        return Err(Fail::new(format!("some expressions do not work: {}", bad.join("; "))).hint("nothing was changed; fix the expressions (unknown names, circular references, wrong field paths)"));
    }
    let vals: Vec<String> = result["parameters"].as_array().into_iter().flatten().map(|r| format!("{} = {}", r["key"].as_str().unwrap_or(""), num(r["value"].as_f64().unwrap_or(0.0)))).collect();
    let out = finish(ctx, &d, &result, &before, None, &format!("Updated: {}.", done.join(", ")), &if vals.is_empty() { String::new() } else { format!("Parameter values: {}.", vals.join(", ")) })?;
    Ok(Output { data: json!({ "parameters": result["parameters"], "formulas": result["bindings"], "result": out.data }), ..out })
}

/// "3.extent.distance" → "3.kind.extent.distance" (la ruta guardada pasa por `kind`)
fn normalize_path(doc: &Document, path: &str) -> Result<String, Fail> {
    let (id, rest) = path.split_once('.').ok_or_else(|| Fail::new(format!("\"{path}\" is not a field path")).hint("write \"<feature id>.<field>\", e.g. \"3.extent.distance\""))?;
    let id: u32 = id.parse().map_err(|_| Fail::new(format!("\"{path}\" must start with a feature id")))?;
    if doc.get(FeatureId(id)).is_none() {
        return Err(Fail::new(format!("no feature {id}")));
    }
    Ok(if rest.starts_with("kind.") { path.to_string() } else { format!("{id}.kind.{rest}") })
}

// ─── Exportar ───────────────────────────────────────────────────────────────

const EXPORT: Tool = Tool {
    name: "cad_export",
    title: "Export a design to a file",
    description: "Writes the parts of a design tab to a file: `step` (exact CAD geometry, for other CAD programs and \
machining), `stl` / `3mf` (3D printing), `obj`, `ply`, `glb` (meshes). All parts go in one file unless `part` picks \
one. Will not replace an existing file without `overwrite: true`. Example: {\"path\": \"/home/me/bracket.step\"}.",
    input: || {
        object(&[
            tab_prop(),
            ("path", string("Absolute path of the file to write; the extension sets the format if `format` is omitted"), true),
            ("format", one_of("File format (default: from the extension)", &["step", "stl", "3mf", "obj", "ply", "glb"]), false),
            ("part", string("Only this part: \"feature:index\" (e.g. \"2:0\") or its name"), false),
            ("overwrite", boolean("Replace the file if it exists (default false)"), false),
        ])
    },
    effect: Effect::Write,
    run: export,
};

fn export(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = PathBuf::from(arg_str(args, "path").unwrap_or_default());
    if !path.is_absolute() {
        return Err(Fail::new("`path` must be absolute").hint("e.g. /home/me/part.step"));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let format = arg_str(args, "format").map(str::to_string).unwrap_or(if ext == "stp" { "step".into() } else { ext.clone() });
    if !["step", "stl", "3mf", "obj", "ply", "glb"].contains(&format.as_str()) {
        return Err(Fail::new(format!("unknown format \"{format}\"")).hint("use .step, .stl, .3mf, .obj, .ply or .glb, or pass `format`"));
    }
    if path.exists() && arg_bool(args, "overwrite") != Some(true) {
        return Err(Fail::new(format!("{} already exists", path.display())).hint("pass `overwrite: true` to replace it"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.is_dir()) {
        return Err(Fail::new(format!("folder {} does not exist", dir.display())));
    }
    let d = open_design(ctx, args)?;
    let parts = ctx.call("cad_evaluate", json!({}))?["parts"].clone();
    let part = match arg_str(args, "part") {
        Some(p) => Some(find_part(&parts, p)?),
        None => None,
    };
    let bytes = ctx.call("cad_export", json!({ "path": path, "format": format, "part": part }))?;
    Ok(Output::new(
        format!("Exported tab \"{}\" to {} ({format}, {} bytes).", d.tab_name, path.display(), bytes),
        json!({ "path": path, "format": format, "bytes": bytes }),
    ))
}
