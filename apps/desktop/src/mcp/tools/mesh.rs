//! Mallas: importar, ver qué tiene, reparar, remallar, desplegar UV, exportar
//! y pasar una pieza del diseño a malla. Trabajan sobre el modelo del objeto
//! activo de la escena; importar o traer una pieza deja un objeto nuevo y los
//! demás quedan como estaban (la app los muestra en el Outliner).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::cad::{find_part, find_tab, num, tab_prop};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, integer, number_in, object, one_of, string};

pub const TOOLS: &[Tool] = &[IMPORT, FROM_PART, INFO, REPAIR, REMESH, UV, EXPORT];

/// Formatos que se importan
const IMPORTS: &[&str] = &["glb", "gltf", "obj", "stl", "ply"];
/// Formatos que se exportan (el conversor)
const EXPORTS: &[&str] = &["glb", "gltf", "obj", "stl", "ply", "3mf", "usdz", "usda"];

/// El modelo activo, o un error que dice cómo tener uno
pub(super) fn need_model(ctx: &Ctx) -> Result<Value, Fail> {
    let summary = ctx.call("workspace_summary", json!({}))?;
    if summary["model"].is_object() && summary["model"]["faces"].as_u64().unwrap_or(0) > 0 {
        return Ok(summary);
    }
    Err(Fail::new("there is no mesh model loaded").hint("load one with `mesh_import` (a file) or `mesh_from_part` (a part of a CAD design)"))
}

/// Ruta absoluta o un error que la pide
pub(super) fn absolute(path: &str) -> Result<PathBuf, Fail> {
    let p = PathBuf::from(path);
    if p.is_absolute() { Ok(p) } else { Err(Fail::new(format!("\"{path}\" is not an absolute path")).hint("use a full path such as /home/me/model.stl")) }
}

/// Ruta de salida: absoluta, carpeta existente y sin pisar sin `overwrite`
pub(super) fn output_path(args: &Value) -> Result<PathBuf, Fail> {
    let path = absolute(arg_str(args, "path").unwrap_or_default())?;
    if path.exists() && arg_bool(args, "overwrite") != Some(true) {
        return Err(Fail::new(format!("{} already exists", path.display())).hint("pass `overwrite: true` to replace it, or choose another path"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.is_dir()) {
        return Err(Fail::new(format!("folder {} does not exist", dir.display())));
    }
    Ok(path)
}

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Tamaño legible
pub(super) fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / (1 << 10) as f64),
        b => format!("{b} bytes"),
    }
}

/// Medidas X × Y × Z en mm
pub(super) fn dims(d: &Value) -> String {
    format!("{} × {} × {} mm", num(d[0].as_f64().unwrap_or(0.0)), num(d[1].as_f64().unwrap_or(0.0)), num(d[2].as_f64().unwrap_or(0.0)))
}

/// Una línea con lo que quedó del modelo (vértices, triángulos, medidas, si es cerrado)
pub(super) fn model_line(ctx: &Ctx) -> Result<(String, Value), Fail> {
    let a = ctx.call("analyze_print3d", json!({}))?;
    let line = format!(
        "{} vertices, {} triangles, {}, {}{}.",
        a["vertex_count"],
        a["triangle_count"],
        dims(&a["dimensions"]),
        if a["is_closed"] == true { "closed (watertight)" } else { "open (has holes or loose edges)" },
        if a["is_closed"] == true { format!(", volume {} cm³", num(a["volume"].as_f64().unwrap_or(0.0) / 1000.0)) } else { String::new() }
    );
    Ok((line, a))
}

// ─── Objetos de la escena (sin ventana) ─────────────────────────────────────
//
// La app guarda la lista de objetos en la interfaz del proyecto; el motor
// guarda los modelos. Sin ventana el conector lleva las dos cosas igual que
// la app, así el proyecto guardado se abre con todos los objetos.

/// Lo de la interfaz que es del objeto activo (va a su casillero al cambiar)
const OBJECT_UI_KEYS: &[&str] = &[
    "fileName", "meshInfo", "model", "skeleton", "autorig", "retopology", "repair", "uv", "print3d", "animation", "pipeline", "activeQuad", "showQuadMesh", "rig", "remesh",
];

fn next_object_id(ui: &Value) -> u64 {
    ui["objects"].as_array().into_iter().flatten().filter_map(|o| o["id"].as_u64()).max().map_or(1, |m| m + 1)
}

/// Deja activo un objeto nuevo y vacío para un modelo que llega (o reusa el
/// activo si es importado y todavía no tiene malla). Devuelve su número
fn object_for_new_model(ctx: &Ctx, name: &str, source: Value) -> Result<u64, Fail> {
    let summary = ctx.call("workspace_summary", json!({}))?;
    let has_model = summary["model"].is_object();
    let mut active = summary["active_object"].as_u64();
    let ui = ctx.session.ui();
    // El modelo de antes de que hubiera objetos pasa a ser uno
    if active.is_none() && has_model {
        let id = next_object_id(&ui);
        ctx.call("object_adopt", json!({ "id": id }))?;
        let old_name = ui["fileName"].as_str().unwrap_or("Modelo").to_string();
        ctx.session.edit_ui(|ui| {
            push_object(ui, json!({ "id": id, "name": old_name, "source": { "kind": "import" } }));
            ui["activeObject"] = json!(id);
        });
        active = Some(id);
    }
    let ui = ctx.session.ui();
    if let Some(id) = active
        && !has_model
        && source["kind"] == "import"
        && objects(&ui).iter().any(|o| o["id"] == id && o["source"]["kind"] == "import")
    {
        ctx.session.edit_ui(|ui| rename_object(ui, id, name));
        return Ok(id);
    }
    let id = next_object_id(&ui);
    ctx.call("object_activate", json!({ "id": id }))?;
    ctx.session.edit_ui(|ui| {
        // El que estaba activo guarda su interfaz y su historial
        if let Some(prev) = active {
            let mut own = json!({});
            for k in OBJECT_UI_KEYS {
                if let Some(v) = ui.as_object_mut().and_then(|o| o.remove(*k)) {
                    own[*k] = v;
                }
            }
            let history = ui.as_object_mut().and_then(|o| o.remove("history"));
            if let Some(list) = ui["objects"].as_array_mut()
                && let Some(o) = list.iter_mut().find(|o| o["id"] == prev)
            {
                o["ui"] = json!(own.to_string());
                if let Some(h) = history {
                    o["history"] = h;
                }
            }
        }
        push_object(ui, json!({ "id": id, "name": name, "source": source }));
        ui["activeObject"] = json!(id);
    });
    Ok(id)
}

fn objects(ui: &Value) -> Vec<Value> {
    ui["objects"].as_array().cloned().unwrap_or_default()
}

fn push_object(ui: &mut Value, object: Value) {
    if !ui["objects"].is_array() {
        ui["objects"] = json!([]);
    }
    ui["objects"].as_array_mut().unwrap().push(object);
}

fn rename_object(ui: &mut Value, id: u64, name: &str) {
    if let Some(o) = ui["objects"].as_array_mut().and_then(|l| l.iter_mut().find(|o| o["id"] == id)) {
        o["name"] = json!(name);
    }
}

/// El modelo nuevo quedó cargado: la interfaz lo muestra al abrir el proyecto
fn shown_in_ui(ctx: &Ctx, name: &str, info: &Value) {
    ctx.session.edit_ui(|ui| {
        ui["fileName"] = json!(name);
        ui["model"] = json!(true);
        ui["meshInfo"] = json!({ "vertices": info["num_vertices"], "faces": info["num_faces"], "format": info["format"] });
        ui["pipeline"] = json!({ "completed": ["import"] });
    });
}

/// Carga un modelo como objeto nuevo de la escena (sin ventana): `load` lo deja
/// en el motor y devuelve su `MeshInfo` y lo que la herramienta quiere. Todo
/// junto es un paso de `undo`; si algo falla, queda como estaba
pub(super) fn new_object_with<T>(ctx: &Ctx, name: &str, label: &str, load: impl FnOnce() -> Result<(Value, T), Fail>) -> Result<T, Fail> {
    let before = ctx.session.before_change()?;
    let done = (|| {
        object_for_new_model(ctx, name, json!({ "kind": "import" }))?;
        let (info, out) = load()?;
        shown_in_ui(ctx, name, &info);
        Ok::<_, Fail>(out)
    })();
    match done {
        Ok(out) => {
            ctx.session.commit(label, before);
            Ok(out)
        }
        Err(e) => {
            ctx.session.rollback(before).map_err(Fail::new)?;
            Err(e)
        }
    }
}

// ─── Importar ───────────────────────────────────────────────────────────────

const IMPORT: Tool = Tool {
    name: "mesh_import",
    title: "Import a 3D model file",
    description: "Loads a mesh model from a file as a new object of the scene (the objects already there stay). \
Formats: GLB/glTF (with textures, skeleton and animations), OBJ (with its MTL), STL, PLY. Loose texture images next to \
the model are picked up; `textures` adds more. Afterwards `mesh_info`, `mesh_repair`, `print_check` … work on it. \
STL/PLY/3MF are taken as millimetres with Z up. `undo` removes it again. \
Example: {\"path\": \"/home/me/scan.stl\"}.",
    input: || {
        object(&[
            ("path", string("Absolute path of the model file"), true),
            ("textures", array("Extra texture images or the .mtl to attach (absolute paths)", string("path")), false),
        ])
    },
    effect: Effect::Replace,
    run: import,
};

fn import(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = absolute(arg_str(args, "path").unwrap_or_default())?;
    if !path.is_file() {
        return Err(Fail::new(format!("{} does not exist", path.display())).hint("pass the absolute path of an existing model file"));
    }
    let ext = extension(&path);
    if !IMPORTS.contains(&ext.as_str()) {
        return Err(Fail::new(format!(".{ext} is not a format Pinocchio imports")).hint(format!("use one of: {}; STEP files go to CAD", IMPORTS.join(", "))));
    }
    let textures: Vec<String> = args["textures"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    let name = file_name(&path);
    // En vivo lo importa la ventana: objeto nuevo y su paso de deshacer, como con el menú
    if let Some(answer) = ctx.session.backend.ask_with("import", json!({ "path": path, "textures": textures })) {
        let info = answer.map_err(Fail::new)?;
        let (line, a) = model_line(ctx)?;
        return Ok(Output::new(format!("Imported {name} in the app as a new object: {line}"), json!({ "path": path, "info": info, "analysis": a })));
    }
    let info = new_object_with(ctx, &name, &format!("Import {name}"), || ctx.call("import_model", json!({ "path": path, "textures": textures })).map(|i| (i.clone(), i)))?;
    let (line, a) = model_line(ctx)?;
    let mut extra = Vec::new();
    if let Some(rig) = info["rig"].as_object() {
        extra.push(format!("It brings a skeleton of {} bones and {} animation(s).", rig["num_bones"], rig["clips"].as_array().map_or(0, Vec::len)));
    }
    if let Some(t) = info["textures"].as_array().filter(|t| !t.is_empty()) {
        extra.push(format!("Textures attached: {}.", t.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")));
    }
    Ok(Output::new(format!("Imported {name} as a new object: {line} {}", extra.join(" ")).trim().to_string(), json!({ "path": path, "info": info, "analysis": a })))
}

const FROM_PART: Tool = Tool {
    name: "mesh_from_part",
    title: "Turn a CAD part into a mesh model",
    description: "Makes a mesh model from a part of a design tab, as a scene object of its own, so the mesh and print \
tools (`print_check`, `print_cut`, `print_plates` …) can work on it. Calling it again after the design changed \
updates that object's mesh. `part` is the part name or \"feature:index\" from `cad_design`; without it, the tab's only part. \
Example: {\"tab\": \"Bracket\", \"part\": \"Bracket\"}.",
    input: || object(&[tab_prop(), ("part", string("Part name or \"feature:index\" (default: the only part)"), false)]),
    effect: Effect::Replace,
    run: from_part,
};

fn from_part(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    if kind != "design" {
        return Err(Fail::new(format!("tab \"{tab_name}\" is an assembly")).hint("pick a design tab: assemblies are exported with `cad_export` (`assembly: true`)"));
    }
    let parts = ctx.call("cad_tab_parts", json!({ "id": tab }))?;
    let list = parts.as_array().cloned().unwrap_or_default();
    let (part, part_name) = match arg_str(args, "part") {
        Some(want) => {
            let id = find_part(&parts, want)?;
            let name = list.iter().find(|p| p["id"] == id).and_then(|p| p["name"].as_str()).unwrap_or(want).to_string();
            (id, name)
        }
        None if list.len() == 1 => (list[0]["id"].clone(), list[0]["name"].as_str().unwrap_or("Pieza").to_string()),
        None if list.is_empty() => return Err(Fail::new(format!("tab \"{tab_name}\" has no solid parts yet")).hint("make a solid first (`cad_extrude`, `cad_primitive` …)")),
        None => {
            let names: Vec<&str> = list.iter().filter_map(|p| p["name"].as_str()).collect();
            return Err(Fail::new(format!("tab \"{tab_name}\" has {} parts", list.len())).hint(format!("pass `part`: {}", names.join(", "))));
        }
    };
    if let Some(answer) = ctx.session.backend.ask_with("use_part", json!({ "studio": tab, "part": part })) {
        answer.map_err(Fail::new)?;
        let (line, a) = model_line(ctx)?;
        return Ok(Output::new(format!("The app shows the mesh of part \"{part_name}\" as the active object: {line}"), json!({ "tab": tab, "part": part, "analysis": a })));
    }
    let before = ctx.session.before_change()?;
    let done = (|| {
        let ui = ctx.session.ui();
        let existing = objects(&ui).into_iter().find(|o| o["source"]["kind"] == "cad" && o["source"]["part"] == part && o["source"]["studio"].as_u64().unwrap_or(0) == tab as u64);
        let id = match existing.as_ref().and_then(|o| o["id"].as_u64()) {
            Some(id) => {
                ctx.call("object_activate", json!({ "id": id }))?;
                id
            }
            None => object_for_new_model(ctx, &part_name, json!({ "kind": "cad", "studio": tab, "part": part }))?,
        };
        let r = ctx.call("cad_part_to_model", json!({ "part": part, "studio": tab, "known": null }))?;
        let info = r["info"].clone();
        ctx.session.edit_ui(|ui| {
            if let Some(o) = ui["objects"].as_array_mut().and_then(|l| l.iter_mut().find(|o| o["id"] == id)) {
                o["source"]["hash"] = r["hash"].clone();
            }
            ui["activeObject"] = json!(id);
        });
        shown_in_ui(ctx, &part_name, &info);
        Ok::<_, Fail>(())
    })();
    if let Err(e) = done {
        ctx.session.rollback(before).map_err(Fail::new)?;
        return Err(e);
    }
    ctx.session.commit(&format!("Mesh of {part_name}"), before);
    let (line, a) = model_line(ctx)?;
    Ok(Output::new(format!("Part \"{part_name}\" of tab \"{tab_name}\" is now the active mesh object: {line}"), json!({ "tab": tab, "part": part, "analysis": a })))
}

// ─── Mirar ──────────────────────────────────────────────────────────────────

const INFO: Tool = Tool {
    name: "mesh_info",
    title: "Describe the mesh model",
    description: "What the active mesh model is: vertices, triangles, size in mm (X × Y × Z, Z up), volume, whether it is \
closed (watertight) and manifold, mean edge length, textures and skeleton, and the other objects of the scene. \
`diagnose: true` also counts defects (holes, non-manifold edges, flipped or duplicate faces, loose pieces) — what \
`mesh_repair` fixes; `self_intersections: true` adds crossing triangles (slower). Example: {\"diagnose\": true}.",
    input: || {
        object(&[
            ("diagnose", boolean("Count mesh defects (default false)"), false),
            ("self_intersections", boolean("With diagnose: also look for crossing triangles (slower)"), false),
        ])
    },
    effect: Effect::Read,
    run: info,
};

fn info(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let summary = need_model(ctx)?;
    let (line, analysis) = model_line(ctx)?;
    let remesh = ctx.call("remesh_info", json!({}))?;
    let model = &summary["model"];
    let mut lines = vec![format!("Mesh model {}: {line}", model["name"].as_str().map_or_else(String::new, |n| format!("\"{n}\"")))];
    lines.push(format!(
        "Mean edge {} mm; {}; {}{}.",
        num(remesh["mean_edge_mm"].as_f64().unwrap_or(0.0)),
        if remesh["manifold"] == true { "manifold" } else { "not manifold" },
        if remesh["has_skin"] == true { "has UVs/textures" } else { "no UVs" },
        if remesh["has_rig"] == true { ", rigged (skeleton and weights)" } else { "" }
    ));
    let ui = ctx.session.ui();
    let list = objects(&ui);
    if list.len() > 1 {
        let active = summary["active_object"].as_u64();
        let names: Vec<String> = list
            .iter()
            .map(|o| format!("\"{}\"{}{}", o["name"].as_str().unwrap_or(""), if o["source"]["kind"] == "cad" { " (CAD part)" } else { "" }, if o["id"].as_u64() == active { " [active]" } else { "" }))
            .collect();
        lines.push(format!("Scene objects: {}.", names.join(", ")));
    } else if let Some(n) = summary["other_objects"].as_u64().filter(|n| *n > 0) {
        lines.push(format!("Other scene objects: {n}."));
    }
    let mut data = json!({ "analysis": analysis, "surface": remesh, "workspace": summary });
    if arg_bool(args, "diagnose") == Some(true) {
        let d = ctx.call("analyze_mesh", json!({ "config": { "check_self_intersections": arg_bool(args, "self_intersections") == Some(true) } }))?;
        lines.push(diagnosis(&d));
        data["diagnostics"] = d;
    }
    Ok(Output::new(lines.join("\n"), data))
}

/// Los defectos en una frase
fn diagnosis(d: &Value) -> String {
    let n = |k: &str| d[k].as_u64().unwrap_or(0);
    let mut found = Vec::new();
    for (key, what) in [
        ("boundary_loops", "hole(s)"),
        ("non_manifold_edges", "non-manifold edge(s)"),
        ("non_manifold_vertices", "non-manifold vertex(es)"),
        ("inconsistent_edges", "edge(s) between faces of opposite orientation"),
        ("duplicate_faces", "duplicate face(s)"),
        ("degenerate_faces", "zero-area face(s)"),
        ("invalid_faces", "invalid face(s)"),
        ("duplicate_vertices", "duplicate vertex(es) on seams"),
        ("unreferenced_vertices", "unused vertex(es)"),
    ] {
        if n(key) > 0 {
            found.push(format!("{} {what}", n(key)));
        }
    }
    if let Some(x) = d["self_intersections"].as_u64().filter(|x| *x > 0) {
        found.push(format!("{x} crossing triangle pair(s)"));
    }
    if d["normals_outward"] == false {
        found.push("normals point inward".into());
    }
    let parts = n("connected_components");
    let pieces = if parts > 1 { format!(" It has {parts} separate pieces.") } else { String::new() };
    if found.is_empty() {
        format!("Diagnosis: no defects.{pieces}")
    } else {
        format!("Diagnosis: {}.{pieces} {}", found.join(", "), if d["needs_repair"] == true { "`mesh_repair` can fix it." } else { "" }).trim().to_string()
    }
}

// ─── Reparar ────────────────────────────────────────────────────────────────

const REPAIR: Tool = Tool {
    name: "mesh_repair",
    title: "Repair the mesh",
    description: "Fixes the defects of the active mesh model so it can be printed or processed: merges duplicate \
vertices, removes zero-area and duplicate faces, makes normals consistent and pointing outward, splits non-manifold \
geometry and fills holes (smooth patches). Optional: `remove_small_pieces` (loose bits), `max_hole_edges` (leave \
bigger holes open; 0 = fill all), `join_overlapping` (cut through bodies that cross each other and keep the outer \
skin; slower). The texture and the rig follow the repaired mesh. Example: {} or {\"remove_small_pieces\": true}.",
    input: || {
        object(&[
            ("fill_holes", boolean("Fill holes (default true)"), false),
            ("max_hole_edges", integer("Only fill holes with at most this many edges (0 = all, default)", 0), false),
            ("smooth_fill", boolean("Refine and smooth the fill patches (default true)"), false),
            ("remove_small_pieces", boolean("Remove small loose pieces (default false)"), false),
            ("join_overlapping", boolean("Join bodies that cross each other (default false, slower)"), false),
        ])
    },
    effect: Effect::Model,
    run: repair,
};

fn repair(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let config = json!({
        "fill_holes": arg_bool(args, "fill_holes").unwrap_or(true),
        "max_hole_edges": args["max_hole_edges"].as_u64().unwrap_or(0),
        "refine_fill": arg_bool(args, "smooth_fill").unwrap_or(true),
        "remove_small_components": arg_bool(args, "remove_small_pieces").unwrap_or(false),
        "resolve_intersections": arg_bool(args, "join_overlapping").unwrap_or(false),
    });
    let r = ctx.call("repair_mesh", json!({ "config": config }))?;
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    let mut done = Vec::new();
    for (key, what) in [
        ("vertices_merged", "vertices merged"),
        ("degenerate_fixed", "degenerate faces fixed"),
        ("duplicate_faces_removed", "duplicate faces removed"),
        ("faces_flipped", "faces flipped"),
        ("non_manifold_fixed", "non-manifold vertices split"),
        ("components_removed", "loose pieces removed"),
        ("holes_filled", "holes filled"),
        ("holes_skipped", "holes left open"),
        ("intersections_cut", "crossings cut"),
    ] {
        if n(key) > 0 {
            done.push(format!("{} {what}", n(key)));
        }
    }
    let (line, a) = model_line(ctx)?;
    let mut text = format!("Repaired: {}. Now {line} {}", if done.is_empty() { "nothing needed fixing".into() } else { done.join(", ") }, diagnosis(&r["new_diagnostics"]));
    if r["rig_kept"] == true {
        text.push_str(" The rig moved to the repaired mesh.");
    }
    Ok(Output::new(text, json!({ "repair": r, "analysis": a })))
}

// ─── Remallar ───────────────────────────────────────────────────────────────

const REMESH: Tool = Tool {
    name: "mesh_remesh",
    title: "Remesh: simplify, smooth, even triangles, voxels or quads",
    description: "Rebuilds the triangles of the active mesh model. Modes: \
`simplify` (fewer triangles keeping the shape: `ratio` 0–1 of triangles to keep, e.g. 0.25), \
`smooth` (soften noise: `iterations`, `strength` 0–1), \
`isotropic` (even, well-shaped triangles: `edge_mm` target edge, default automatic), \
`voxel` (rebuild as a closed, manifold skin — fixes anything, loses fine detail: `voxel_mm`), \
`quads` (pair triangles into quads). Reports the triangle count before/after and how far the surface moved. \
Texture and rig are kept when the mode allows it. Example: {\"mode\": \"simplify\", \"ratio\": 0.25}.",
    input: || {
        object(&[
            ("mode", one_of("What to do", &["simplify", "smooth", "isotropic", "voxel", "quads"]), true),
            ("ratio", number_in("simplify: fraction of triangles to keep (0.01–1)", 0.01, 1.0), false),
            ("max_error_percent", number_in("simplify: stop before the surface moves more than this % of the model size", 0.0, 100.0), false),
            ("iterations", integer("smooth / isotropic: passes (default 3 / 5)", 1), false),
            ("strength", number_in("smooth: 0–1 (default 0.5)", 0.0, 1.0), false),
            ("edge_mm", number_in("isotropic: target edge length in mm (default automatic)", 0.001, 1e6), false),
            ("voxel_mm", number_in("voxel: voxel size in mm (default automatic)", 0.001, 1e6), false),
            ("keep_sharp_deg", number_in("smooth / isotropic: keep edges sharper than this angle (degrees)", 1.0, 180.0), false),
        ])
    },
    effect: Effect::Model,
    run: remesh,
};

fn remesh(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let f = |k: &str| args[k].as_f64();
    let mode = arg_str(args, "mode").unwrap_or("simplify");
    let params = match mode {
        "simplify" => {
            let ratio = f("ratio").ok_or_else(|| Fail::new("simplify needs `ratio`").hint("e.g. {\"mode\": \"simplify\", \"ratio\": 0.25} keeps a quarter of the triangles"))?;
            json!({ "mode": "simplify", "ratio": ratio, "max_error_percent": f("max_error_percent") })
        }
        "smooth" => json!({ "mode": "smooth", "iterations": args["iterations"].as_u64().unwrap_or(3), "strength": f("strength").unwrap_or(0.5), "sharp_angle": f("keep_sharp_deg") }),
        "isotropic" => json!({ "mode": "isotropic", "edge_mm": f("edge_mm"), "sharp_angle": f("keep_sharp_deg"), "iterations": args["iterations"].as_u64().unwrap_or(5) }),
        "voxel" => json!({ "mode": "voxel", "voxel_mm": f("voxel_mm") }),
        _ => json!({ "mode": "quads", "face_angle": 30.0, "shape_angle": 40.0 }),
    };
    let r = ctx.call("remesh_apply", json!({ "params": params, "options": { "keep_texture": true, "keep_rig": true } }))?;
    let s = &r["stats"];
    let count = |c: &Value| {
        let quads = c["quads"].as_u64().unwrap_or(0);
        format!("{} triangles{}", c["triangles"], if quads > 0 { format!(" + {quads} quads") } else { String::new() })
    };
    let (line, a) = model_line(ctx)?;
    Ok(Output::new(
        format!(
            "Remeshed ({mode}): {} → {}; the surface moved at most {} mm (mean {} mm). Now {line}{}",
            count(&s["before"]),
            count(&s["after"]),
            num(s["deviation"]["max_mm"].as_f64().unwrap_or(0.0)),
            num(s["deviation"]["mean_mm"].as_f64().unwrap_or(0.0)),
            if r["rig_kept"] == true { " The rig moved to the new mesh." } else { "" }
        ),
        json!({ "remesh": r, "analysis": a }),
    ))
}

// ─── UV ─────────────────────────────────────────────────────────────────────

const UV: Tool = Tool {
    name: "mesh_uv",
    title: "Unwrap UVs and bake textures",
    description: "Cuts the active mesh model into UV islands, lays them out on a texture and bakes onto it what the model \
had (its old textures, or vertex colours). `layout`: `paintable` (readable islands, good for painting; default) or \
`compact` (least wasted space). `texture_size` in pixels (default 2048). The shape does not change; vertices split \
along the seams. Example: {\"layout\": \"compact\", \"texture_size\": 4096}.",
    input: || {
        object(&[
            ("layout", one_of("Island layout (default paintable)", &["paintable", "compact"]), false),
            ("texture_size", integer("Baked texture side in pixels, 64–8192 (default 2048)", 64), false),
            ("max_angle", number_in("Max bend inside one island in degrees (default 66)", 15.0, 85.0), false),
        ])
    },
    effect: Effect::Model,
    run: uv,
};

fn uv(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let size = args["texture_size"].as_u64().unwrap_or(2048);
    if size > 8192 {
        return Err(Fail::new(format!("texture_size {size} is above 8192")));
    }
    let config = json!({
        "texture_size": size,
        "padding": 8,
        "max_angle": args["max_angle"].as_f64().unwrap_or(66.0),
        "layout": arg_str(args, "layout").unwrap_or("paintable"),
        "by_parts": false,
    });
    let r = ctx.call("unwrap_original_mesh", json!({ "config": config }))?;
    let uv = &r["uv"];
    Ok(Output::new(
        format!(
            "Unwrapped into {} islands covering {}% of a {size}×{size} texture (stretch {}; 1 is perfect). {} vertices, {} faces.",
            uv["num_charts"],
            num(uv["coverage"].as_f64().unwrap_or(0.0) * 100.0),
            num(uv["stretch"].as_f64().unwrap_or(0.0)),
            r["mesh_info"]["num_vertices"],
            r["mesh_info"]["num_faces"]
        ),
        r,
    ))
}

// ─── Exportar ───────────────────────────────────────────────────────────────

const EXPORT: Tool = Tool {
    name: "mesh_export",
    title: "Export the mesh model to a file",
    description: "Writes the active mesh model to a file; the format comes from the extension: GLB/glTF, OBJ, STL, PLY, \
3MF, USDZ/USDA. STL/PLY/3MF are written in millimetres with Z up. GLB/glTF keep textures, skeleton, weights and \
animations; `simplify` (fraction of triangles to keep) and `draco` (compression) make them smaller. Will not replace \
an existing file without `overwrite: true`. For print plates use `print_plates`. \
Example: {\"path\": \"/home/me/model.glb\", \"draco\": true}.",
    input: || {
        object(&[
            ("path", string("Absolute output path; the extension picks the format"), true),
            ("overwrite", boolean("Replace the file if it exists (default false)"), false),
            ("include_rig", boolean("Skeleton, weights and animations if there are (default true)"), false),
            ("simplify", number_in("GLB/glTF: fraction of triangles to keep (0.01–1)", 0.01, 1.0), false),
            ("draco", boolean("GLB/glTF: Draco compression (default false)"), false),
            ("scale", number_in("Multiply the size by this (default 1)", 1e-6, 1e6), false),
        ])
    },
    effect: Effect::Write,
    run: export,
};

fn export(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let summary = need_model(ctx)?;
    let path = output_path(args)?;
    let ext = extension(&path);
    if !EXPORTS.contains(&ext.as_str()) {
        return Err(Fail::new(format!(".{ext} is not an export format")).hint(format!("use one of: {}", EXPORTS.join(", "))));
    }
    // Solo lo que hay: sin esqueleto o sin pesos, la exportación los pediría y fallaría
    let rig = arg_bool(args, "include_rig").unwrap_or(true);
    let rigged = rig && summary["rigged"] == true;
    let config = json!({
        "format": ext,
        "path": path,
        "include_skeleton": rig && !summary["skeleton"].is_null(),
        "include_weights": rigged,
        "export_animations": rigged,
        "simplify_ratio": args["simplify"].as_f64(),
        "draco": arg_bool(args, "draco"),
        "scale_factor": args["scale"].as_f64(),
    });
    let r = ctx.call("export_model", json!({ "config": config }))?;
    if r["success"] == false {
        return Err(Fail::new(r["message"].as_str().unwrap_or("export failed").to_string()));
    }
    let files: Vec<&str> = r["files_created"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let bytes = r["total_bytes"].as_u64().unwrap_or(0);
    let others = if files.len() > 1 { format!(" ({} files: {})", files.len(), files.join(", ")) } else { String::new() };
    Ok(Output::new(format!("Exported {} ({}){others}.", path.display(), size(bytes)), json!({ "path": path, "files": files, "bytes": bytes })))
}
