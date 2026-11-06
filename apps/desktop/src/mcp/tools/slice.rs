//! Laminar el modelo activo y guardar el archivo para la impresora: filamento con el motor
//! de OrcaSlicer (gcode) y resina con el laminador propio (soportes, venteos, `.ctb`),
//! más inclinaciones y la pieza de calibración de exposición. Nunca manda nada a una
//! impresora: solo escribe archivos.

use serde_json::{json, Map, Value};

use super::cad::num;
use super::mesh::{absolute, need_model, output_path};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, integer, number_in, object, one_of, point, string, Prop};

pub const TOOLS: &[Tool] = &[MACHINES, SLICE, RESIN_SLICE, RESIN_TILTS, RESIN_CALIBRATION];

fn fmt_time(seconds: f64) -> String {
    let s = seconds.round() as u64;
    if s >= 3600 { format!("{} h {} min", s / 3600, (s % 3600) / 60) } else { format!("{} min {} s", s / 60, s % 60) }
}

/// Lo que se busca por nombre: sin mayúsculas ni espacios ni guiones
fn key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '.').collect()
}

fn find_by<'a>(list: &'a [Value], field: &str, want: &str) -> Option<&'a Value> {
    let w = key(want);
    list.iter().find(|v| v[field].as_str().is_some_and(|n| key(n) == w)).or_else(|| list.iter().find(|v| v[field].as_str().is_some_and(|n| key(n).contains(&w))))
}

fn names(list: &[Value], field: &str) -> String {
    list.iter().filter_map(|v| v[field].as_str()).collect::<Vec<_>>().join(", ")
}

/// Nombre corto de un perfil de Orca: «0.20mm Standard @Creality K1» → «0.20mm Standard»
fn short(name: &str) -> &str {
    name.split('@').next().unwrap_or(name).trim()
}

/// Opciones de acomodo de la placa, comunes a filamento y resina
fn placement_props() -> Vec<Prop> {
    vec![
        ("plate", integer("Which plate to slice when the pieces need several beds (0 = first, default)", 0), false),
        ("orient", boolean("Turn each piece to its best resting face first (default false: as it is in the scene)"), false),
        ("spacing", number_in("Gap between pieces on the bed in mm (default 5)", 0.0, 100.0), false),
        ("hollow_wall", number_in("Hollow each piece leaving this wall thickness in mm", 0.4, 50.0), false),
        ("drain_mm", number_in("With hollow: drain hole of this diameter in the bottom (mm)", 0.5, 30.0), false),
    ]
}

/// Entrada de placas con la cama de la máquina y el material de Fabricar
fn plate_input(ctx: &Ctx, args: &Value, name: &str, bed: &Value, nozzle: f64, layer: f64, material: &str) -> Result<Value, Fail> {
    let presets = ctx.call("get_print_presets", json!({}))?;
    let mats = presets["materials"].as_array().cloned().unwrap_or_default();
    let mat = mats.iter().find(|m| m["name"].as_str() == Some(material)).or(mats.first()).cloned().unwrap_or(Value::Null);
    Ok(json!({
        "printer": { "name": name, "build_volume": bed, "nozzle": nozzle, "layer_height": layer, "fit_clearance": nozzle, "hole_comp": nozzle * 0.25 },
        "material": mat,
        "format": "stl",
        "spacing": args["spacing"].as_f64().unwrap_or(5.0),
        "orient": arg_bool(args, "orient").unwrap_or(false),
        "contour": true,
        "hollow_wall": args["hollow_wall"].as_f64(),
        "drain": args["drain_mm"].as_f64(),
        "vent": args.get("drain_mm").is_some(),
        "compensate": false,
        "path": "",
    }))
}

/// Material de Fabricar por el nombre del filamento
fn material_of(filament: &str) -> &'static str {
    let f = filament.to_uppercase();
    if f.contains("TPU") {
        "TPU"
    } else if f.contains("PETG") || f.contains("PET-") {
        "PETG"
    } else if f.contains("ASA") {
        "ASA"
    } else if f.contains("ABS") {
        "ABS"
    } else {
        "PLA"
    }
}

// ─── Máquinas ───────────────────────────────────────────────────────────────

const MACHINES: Tool = Tool {
    name: "print_machines",
    title: "Printers that can be sliced",
    description: "Lists the lab printers that `print_slice` (filament, OrcaSlicer engine) and `resin_slice` (resin) can \
slice for: bed size, qualities and filaments of each filament printer, and screen, file format and resin profiles of \
each resin printer, plus the support densities. Nothing is sent to any printer: slicing only writes files.",
    input: || object(&[]),
    effect: Effect::Read,
    run: machines,
};

fn machines(ctx: &Ctx, _args: &Value) -> Result<Output, Fail> {
    let fdm = ctx.call("slicer_presets", json!({}))?;
    let resin = ctx.call("resin_presets", json!({}))?;
    let mut lines = vec![format!(
        "Filament (OrcaSlicer {}{}):",
        fdm["orca_version"].as_str().unwrap_or(""),
        if fdm["available"] == true { "" } else { ", ENGINE MISSING: `print_slice` will not work" }
    )];
    for p in fdm["printers"].as_array().into_iter().flatten() {
        let list = |k: &str| p[k].as_array().into_iter().flatten().filter_map(Value::as_str).map(short).collect::<Vec<_>>().join(", ");
        lines.push(format!(
            "- {} — bed {} mm, {} spool(s). Qualities: {}. Filaments: {}.",
            p["name"].as_str().unwrap_or(""),
            p["bed"].as_array().map(|b| b.iter().map(|x| num(x.as_f64().unwrap_or(0.0))).collect::<Vec<_>>().join(" × ")).unwrap_or_default(),
            p["tools"].as_u64().unwrap_or(1),
            list("processes"),
            list("filaments"),
        ));
    }
    lines.push("Resin:".into());
    let profiles = resin["profiles"].as_array().cloned().unwrap_or_default();
    for p in resin["printers"].as_array().into_iter().flatten() {
        let id = p["id"].as_str().unwrap_or("");
        let profs: Vec<String> = profiles.iter().filter(|x| x["printer"] == id).map(|x| format!("{} ({})", x["id"].as_str().unwrap_or(""), x["name"].as_str().unwrap_or(""))).collect();
        lines.push(format!(
            "- {} [{}] — screen {} px, {} µm, {} mm; .{}{}. Profiles: {}.",
            p["name"].as_str().unwrap_or(""),
            id,
            p["resolution"].as_array().map(|r| r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" × ")).unwrap_or_default(),
            num(p["pixel_um"][0].as_f64().unwrap_or(0.0)),
            p["bed"].as_array().map(|b| b.iter().map(|x| num(x.as_f64().unwrap_or(0.0))).collect::<Vec<_>>().join(" × ")).unwrap_or_default(),
            p["format"].as_str().unwrap_or(""),
            if p["can_write"] == true { "" } else { " (file not writable yet: preview only)" },
            profs.join(", "),
        ));
    }
    lines.push("Support densities: light, medium (default), heavy, or none.".into());
    Ok(Output::new(lines.join("\n"), json!({ "filament": fdm, "resin": resin })))
}

// ─── Filamento ──────────────────────────────────────────────────────────────

const SLICE: Tool = Tool {
    name: "print_slice",
    title: "Slice for a filament printer (G-code)",
    description: "Slices the active mesh model (or its pieces from `print_cut`) for one of the lab's filament printers \
with the OrcaSlicer engine and saves the G-code to `path`. The pieces are arranged on the bed like `print_plates`; \
with several plates choose one with `plate`. `settings` changes any OrcaSlicer key of the profile (values as text or \
numbers): e.g. {\"sparse_infill_density\": \"20%\", \"wall_loops\": 3, \"enable_support\": 1}. Supports are added by \
themselves if the engine says they are needed. Does NOT send anything to the printer. Example: {\"path\": \
\"/home/me/print/part.gcode\", \"machine\": \"K1\", \"quality\": \"0.20mm Standard\", \"filament\": \"PETG\"}. See \
`print_machines` for the names.",
    input: || {
        let mut props = vec![
            ("path", string("Absolute path of the .gcode to write"), true),
            ("overwrite", boolean("Replace an existing file (default false)"), false),
            ("machine", string("Filament printer (default the first: K1). See `print_machines`"), false),
            ("quality", string("Process / quality profile (default the printer's)"), false),
            ("filament", string("Filament profile (default the printer's)"), false),
            ("settings", json!({ "type": "object", "description": "OrcaSlicer keys to change, e.g. {\"sparse_infill_density\": \"20%\"}" }), false),
        ];
        props.extend(placement_props());
        object(&props)
    },
    effect: Effect::Write,
    run: slice,
};

fn slice(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let path = output_path(args)?;
    if path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("gcode") {
        return Err(Fail::new("the path must end in .gcode"));
    }
    let info = ctx.call("slicer_presets", json!({}))?;
    if info["available"] != true {
        return Err(Fail::new("the slicing engine (pinocchio-slicer) is missing").hint("it ships with the release builds; in development build it with scripts/build-orca-engine.sh"));
    }
    let printers = info["printers"].as_array().cloned().unwrap_or_default();
    let p = match arg_str(args, "machine") {
        Some(m) => find_by(&printers, "name", m).ok_or_else(|| Fail::new(format!("no filament printer \"{m}\"")).hint(format!("use one of: {}", names(&printers, "name"))))?,
        None => printers.first().ok_or_else(|| Fail::new("there are no filament printer profiles"))?,
    };
    let pick = |field: &str, default: &str, want: Option<&str>| -> Result<String, Fail> {
        let list: Vec<&str> = p[field].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        match want {
            None => Ok(p[default].as_str().unwrap_or("").to_string()),
            Some(w) => {
                let k = key(w);
                list.iter()
                    .find(|n| key(short(n)) == k)
                    .or_else(|| list.iter().find(|n| key(n).contains(&k)))
                    .map(|n| n.to_string())
                    .ok_or_else(|| Fail::new(format!("no {field} \"{w}\" for {}", p["name"].as_str().unwrap_or(""))).hint(format!("use one of: {}", list.iter().map(|n| short(n)).collect::<Vec<_>>().join(", "))))
            }
        }
    };
    let process = pick("processes", "default_process", arg_str(args, "quality"))?;
    let filament = pick("filaments", "default_filament", arg_str(args, "filament"))?;
    let mut overrides = Map::new();
    for (k, v) in args["settings"].as_object().into_iter().flatten() {
        overrides.insert(k.clone(), Value::String(v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())));
    }
    let bed = p["bed"].clone();
    if !bed.is_array() {
        return Err(Fail::new("this printer profile has no bed size"));
    }
    let input = plate_input(ctx, args, p["model"].as_str().unwrap_or(""), &bed, p["nozzle"].as_f64().unwrap_or(0.4), 0.2, material_of(&filament))?;
    let settings = json!({ "printer": p["name"], "process": process, "filament": filament, "overrides": overrides });
    let r = ctx.call("slice_plate", json!({ "input": input, "plate": args["plate"].as_u64().unwrap_or(0), "settings": settings }))?;
    ctx.call("save_gcode", json!({ "path": path }))?;
    let mut lines = vec![
        format!(
            "Sliced for {} ({}, {}): {}, {} m / {} g of filament, {} layers. Saved {}.",
            p["name"].as_str().unwrap_or(""),
            short(&process),
            short(&filament),
            fmt_time(r["time_s"].as_f64().unwrap_or(0.0)),
            num(r["filament_mm"].as_f64().unwrap_or(0.0) / 1000.0),
            num(r["filament_g"].as_f64().unwrap_or(0.0)),
            r["layers"],
            path.display()
        ),
    ];
    if r["plates"].as_u64().unwrap_or(1) > 1 {
        lines.push(format!("Plate {} of {} (choose another with `plate`).", r["plate"].as_u64().unwrap_or(0) + 1, r["plates"]));
    }
    if let Some(list) = r["unplaced"].as_array().filter(|a| !a.is_empty()) {
        lines.push(format!("Do not fit the bed: {} — `print_cut` them.", list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
    }
    for w in r["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        lines.push(format!("Note: {w}"));
    }
    let mut data = r;
    if let Some(o) = data.as_object_mut() {
        o.remove("preview");
        o.insert("path".into(), json!(path));
    }
    Ok(Output::new(lines.join("\n"), data))
}

// ─── Resina ─────────────────────────────────────────────────────────────────

/// Impresora de resina, perfil y ajustes (perfil + cambios)
fn resin_choice(ctx: &Ctx, args: &Value) -> Result<(Value, Value, Value), Fail> {
    let info = ctx.call("resin_presets", json!({}))?;
    let printers = info["printers"].as_array().cloned().unwrap_or_default();
    let p = match arg_str(args, "printer") {
        Some(w) => find_by(&printers, "id", w)
            .or_else(|| find_by(&printers, "name", w))
            .cloned()
            .ok_or_else(|| Fail::new(format!("no resin printer \"{w}\"")).hint(format!("use one of: {}", names(&printers, "id"))))?,
        None => printers.first().cloned().ok_or_else(|| Fail::new("there are no resin printers"))?,
    };
    let id = p["id"].as_str().unwrap_or("").to_string();
    let profiles: Vec<Value> = info["profiles"].as_array().into_iter().flatten().filter(|x| x["printer"] == id.as_str()).cloned().collect();
    let prof = match arg_str(args, "profile") {
        Some(w) => find_by(&profiles, "id", w)
            .or_else(|| find_by(&profiles, "name", w))
            .cloned()
            .ok_or_else(|| Fail::new(format!("no resin profile \"{w}\" for {id}")).hint(format!("use one of: {}", names(&profiles, "id"))))?,
        None => profiles.first().cloned().ok_or_else(|| Fail::new("this printer has no resin profiles"))?,
    };
    let mut settings = prof["settings"].clone();
    if let (Some(s), Some(changes)) = (settings.as_object_mut(), args["settings"].as_object()) {
        for (k, v) in changes {
            if !s.contains_key(k) {
                return Err(Fail::new(format!("unknown resin setting \"{k}\"")).hint(format!("use: {}", s.keys().cloned().collect::<Vec<_>>().join(", "))));
            }
            s.insert(k.clone(), v.clone());
        }
    }
    Ok((p, prof, settings))
}

fn resin_settings_props() -> Vec<Prop> {
    vec![
        ("printer", string("Resin printer id or name (default ld-002h). See `print_machines`"), false),
        ("profile", string("Resin profile id or name (default standard)"), false),
        (
            "settings",
            json!({ "type": "object", "description": "Changes to the profile: layer_height (mm), exposure and bottom_exposure (s), bottom_layers, light_off (s), lift (mm), lift_speed (mm/min), bottom_lift, bottom_lift_speed, retract_speed, density (g/ml), cost_per_ml, antialias (1 = off, 2, 4 or 8: grey edges)" }),
            false,
        ),
    ]
}

const RESIN_SLICE: Tool = Tool {
    name: "resin_slice",
    title: "Slice for a resin printer",
    description: "Prepares and slices the active mesh model (or its pieces) for a lab resin printer: raises it on a raft \
with automatic supports (on downward faces flatter than the overhang angle and on every island), drills a vent hole \
under every enclosed cavity (suction cup / trapped resin), optionally tilts it, slices it into screen masks and checks \
the result (islands left without support, cavities). With `path` it writes the printer file (.ctb for the LD-002H). \
`points` replaces the automatic support points with your own (printer coordinates as returned in `points` of a \
previous call: mm, bed corner at the origin, Z up). Does NOT send anything to the printer. Example: {\"path\": \
\"/home/me/print/mini.ctb\", \"supports\": \"medium\", \"tilt\": {\"axis\": \"x\", \"degrees\": 30}}.",
    input: || {
        let mut props = vec![
            ("path", string("Absolute path of the printer file (.ctb for the LD-002H); omit to only check"), false),
            ("overwrite", boolean("Replace an existing file (default false)"), false),
        ];
        props.extend(resin_settings_props());
        props.push(("supports", one_of("Support density, or none to print straight on the plate (default medium)", &["light", "medium", "heavy", "none"]), false));
        props.push((
            "support_settings",
            json!({ "type": "object", "description": "Changes to the supports: spacing, overhang_angle (°), tip_diameter, pillar_diameter, lift (mm above the raft), raft_thickness, on_model (bool)" }),
            false,
        ));
        props.push(("vents", boolean("Vent enclosed cavities with a hole (default true)"), false));
        props.push(("vent_diameter", number_in("Vent hole diameter in mm (default 2.5)", 0.5, 10.0), false));
        props.push(("tilt", object(&[("axis", one_of("Axis", &["x", "y"]), true), ("degrees", number_in("Angle", -90.0, 90.0), true)]), false));
        props.push(("points", array("Your own support points [x, y, z] in printer mm (replace the automatic ones)", point("Point", 3)), false));
        props.extend(placement_props());
        object(&props)
    },
    effect: Effect::Write,
    run: resin_slice,
};

fn prepare_settings(ctx: &Ctx, args: &Value) -> Result<Value, Fail> {
    let info = ctx.call("resin_presets", json!({}))?;
    let mut prep = info["prepare"].clone();
    let density = arg_str(args, "supports").unwrap_or("medium");
    if density == "none" {
        prep["supports"] = Value::Null;
    } else {
        let base = info["supports"].as_array().into_iter().flatten().find(|d| d[0] == density).map(|d| d[1].clone());
        let mut s = base.ok_or_else(|| Fail::new(format!("no support density \"{density}\"")).hint("use light, medium, heavy or none"))?;
        if let (Some(o), Some(changes)) = (s.as_object_mut(), args["support_settings"].as_object()) {
            for (k, v) in changes {
                if !o.contains_key(k) {
                    return Err(Fail::new(format!("unknown support setting \"{k}\"")).hint(format!("use: {}", o.keys().cloned().collect::<Vec<_>>().join(", "))));
                }
                o.insert(k.clone(), v.clone());
            }
        }
        prep["supports"] = s;
    }
    if let Some(v) = arg_bool(args, "vents") {
        prep["vents"] = json!(v);
    }
    if let Some(d) = args["vent_diameter"].as_f64() {
        prep["vent_diameter"] = json!(d);
    }
    Ok(prep)
}

fn resin_input(ctx: &Ctx, args: &Value, p: &Value, settings: &Value) -> Result<Value, Fail> {
    plate_input(ctx, args, p["name"].as_str().unwrap_or(""), &p["bed"], 0.15, settings["layer_height"].as_f64().unwrap_or(0.05), "Resina")
}

fn resin_slice(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let (p, prof, settings) = resin_choice(ctx, args)?;
    let path = if args.get("path").is_some() {
        if p["can_write"] != true {
            return Err(Fail::new(format!("the .{} file of {} cannot be written yet", p["format"].as_str().unwrap_or(""), p["name"].as_str().unwrap_or(""))).hint("omit `path` to only check, or use the LD-002H"));
        }
        let path = output_path(args)?;
        let ext = format!(".{}", p["format"].as_str().unwrap_or(""));
        if !path.to_string_lossy().to_lowercase().ends_with(&ext) {
            return Err(Fail::new(format!("the path must end in {ext}")));
        }
        Some(path)
    } else {
        None
    };
    let prepare = prepare_settings(ctx, args)?;
    let input = resin_input(ctx, args, &p, &settings)?;
    let tilt = args.get("tilt").map(|t| json!([t["axis"].as_str().unwrap_or("x"), t["degrees"].as_f64().unwrap_or(0.0)]));
    let points = args["points"].as_array().map(|list| list.iter().map(|q| json!({ "pos": q, "kind": "manual" })).collect::<Vec<_>>());
    let r = ctx.call(
        "resin_slice",
        json!({ "input": input, "plate": args["plate"].as_u64().unwrap_or(0), "printer": p["id"], "settings": settings, "prepare": prepare, "points": points, "tilt": tilt }),
    )?;
    if let Some(path) = &path {
        ctx.call("save_resin", json!({ "path": path }))?;
    }
    let f = |k: &str| r[k].as_f64().unwrap_or(0.0);
    let count = |k: &str| r[k].as_array().map_or(0, Vec::len);
    let mut lines = vec![format!(
        "{} with {}: {}, {} ml ({} g) of resin, {} layers ({} mm), largest layer {} mm².",
        p["name"].as_str().unwrap_or(""),
        prof["name"].as_str().unwrap_or(""),
        fmt_time(f("time_s")),
        num(f("volume_ml")),
        num(f("grams")),
        r["layers"],
        num(f("height_mm")),
        num(f("max_area_mm2")),
    )];
    lines.push(format!(
        "Supports: {} placed ({} points, {} skipped), raised {} mm; vents: {}.",
        count("points") - count("skipped"),
        count("points"),
        count("skipped"),
        num(f("lift")),
        count("vents")
    ));
    if count("islands") == 0 {
        lines.push("No islands left without support.".into());
    } else {
        let first: Vec<String> = r["islands"].as_array().into_iter().flatten().take(5).map(|i| format!("layer {} at ({})", i["layer"], i["pos"].as_array().map(|p| p.iter().map(|x| num(x.as_f64().unwrap_or(0.0))).collect::<Vec<_>>().join(", ")).unwrap_or_default())).collect();
        lines.push(format!("{} island(s) left without support: {} — add `points` there or use denser supports.", count("islands"), first.join("; ")));
    }
    if count("cavities") > 0 {
        lines.push(format!("{} cavity(ies) still with suction or trapped resin.", count("cavities")));
    }
    for n in r["notes"].as_array().into_iter().flatten().chain(r["warnings"].as_array().into_iter().flatten()).filter_map(Value::as_str) {
        lines.push(format!("Note: {n}"));
    }
    match &path {
        Some(path) => lines.push(format!("Saved {} ({}).", path.display(), super::mesh::size(r["file_bytes"].as_u64().unwrap_or(0)))),
        None => lines.push("Not saved (no `path`).".into()),
    }
    let mut data = r;
    if let Some(o) = data.as_object_mut() {
        o.remove("layer_z");
        o.insert("path".into(), json!(path));
    }
    Ok(Output::new(lines.join("\n"), data))
}

const RESIN_TILTS: Tool = Tool {
    name: "resin_tilts",
    title: "Compare resin tilts",
    description: "Compares tilting the active model 0/15/30/45° around X and Y for a resin printer: the largest layer \
area (peel force), the suction of enclosed cavities and the height. Best first; use it with `tilt` in `resin_slice`.",
    input: || {
        let mut props = vec![("printer", string("Resin printer id or name (default ld-002h)"), false)];
        props.extend(placement_props());
        object(&props)
    },
    effect: Effect::Read,
    run: resin_tilts,
};

fn resin_tilts(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let (p, _, settings) = resin_choice(ctx, args)?;
    let input = resin_input(ctx, args, &p, &settings)?;
    let r = ctx.call(
        "resin_tilts",
        json!({ "input": input, "plate": args["plate"].as_u64().unwrap_or(0), "printer": p["id"], "layerHeight": settings["layer_height"] }),
    )?;
    let mut lines = vec!["Best first:".to_string()];
    for o in r.as_array().into_iter().flatten() {
        lines.push(format!(
            "- {}: largest layer {} mm², suction {} mm², height {} mm{}",
            if o["degrees"].as_f64() == Some(0.0) { "not tilted".into() } else { format!("{}° around {}", num(o["degrees"].as_f64().unwrap_or(0.0)), o["axis"].as_str().unwrap_or("").to_uppercase()) },
            num(o["max_area_mm2"].as_f64().unwrap_or(0.0)),
            num(o["suction_mm2"].as_f64().unwrap_or(0.0)),
            num(o["height_mm"].as_f64().unwrap_or(0.0)),
            if o["fits"] == true { "" } else { " — does not fit" }
        ));
    }
    Ok(Output::new(lines.join("\n"), json!({ "options": r })))
}

const RESIN_CALIBRATION: Tool = Tool {
    name: "resin_calibration",
    title: "Resin exposure test files",
    description: "Writes the exposure calibration piece (46 × 24 mm, straight on the plate: rows of pins, holes, slots \
and thin walls from 0.1 to 2.5 mm) once per exposure into `dir`, e.g. calibracion-ld-002h-3.5s.ctb. Print each one \
and keep the lowest exposure where the thin pins and walls stand and the holes and slots stay open. Default \
exposures: the profile's ±1 and ±2 steps of `step` seconds.",
    input: || {
        let mut props = vec![
            ("dir", string("Absolute path of an existing folder"), true),
            ("exposures", array("Exposures in seconds", json!({ "type": "number", "minimum": 0.2, "maximum": 60 })), false),
            ("step", number_in("Step between the 5 default exposures in s (default 0.5)", 0.05, 10.0), false),
        ];
        props.extend(resin_settings_props());
        object(&props)
    },
    effect: Effect::Write,
    run: resin_calibration,
};

fn resin_calibration(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let dir = absolute(arg_str(args, "dir").unwrap_or_default())?;
    if !dir.is_dir() {
        return Err(Fail::new(format!("folder {} does not exist", dir.display())));
    }
    let (p, _, settings) = resin_choice(ctx, args)?;
    if p["can_write"] != true {
        return Err(Fail::new(format!("the .{} file of {} cannot be written yet", p["format"].as_str().unwrap_or(""), p["name"].as_str().unwrap_or(""))));
    }
    let exposures: Vec<f64> = match args["exposures"].as_array() {
        Some(list) => list.iter().filter_map(Value::as_f64).collect(),
        None => {
            let e = settings["exposure"].as_f64().unwrap_or(3.0);
            let step = args["step"].as_f64().unwrap_or(0.5);
            (-2..=2).map(|k| ((e + k as f64 * step) * 100.0).round() / 100.0).filter(|x| *x > 0.2).collect()
        }
    };
    ctx.call("resin_calibration", json!({ "printer": p["id"], "settings": settings }))?;
    let files = ctx.call("save_resin_calibration", json!({ "dir": dir, "exposures": exposures }))?;
    let list: Vec<&str> = files.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    Ok(Output::new(
        format!(
            "Wrote {} test file(s) for {}: {}.\nPrint each one; keep the lowest exposure where the thin pins and walls stand and the holes and slots stay open, then pass it as {{\"settings\": {{\"exposure\": …}}}} to `resin_slice`.",
            list.len(),
            p["name"].as_str().unwrap_or(""),
            list.join(", ")
        ),
        json!({ "files": list, "exposures": exposures }),
    ))
}
