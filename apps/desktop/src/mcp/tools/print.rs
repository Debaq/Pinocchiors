//! Preparar para imprimir el modelo activo: verificar, escalar, orientar,
//! cortar en piezas que entren en la cama, uniones entre piezas y exportar las
//! placas (con vaciado), o todo en un paso.

use serde_json::{json, Value};

use super::cad::num;
use super::mesh::{dims, model_line, need_model, output_path, size};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, integer, number_in, object, one_of, point, Prop};

pub const TOOLS: &[Tool] = &[CHECK, SCALE, ORIENT, CUT, JOINTS, PLATES, PREPARE];

const PRINTER_HELP: &str = "Printer: a preset name (\"Creality Ender 3\", \"Creality K1\", \"Prusa MK4\", \"Prusa MINI\", \
\"Bambu Lab X1 / P1\", \"Voron 2.4 (350)\"; default Ender 3) or {\"build_volume\": [x, y, z], \"nozzle\": 0.4, \"layer_height\": 0.2}";
const MATERIAL_HELP: &str = "Material: PLA (default), PETG, ABS, ASA, TPU, or {\"name\", \"density\" g/cm³, \"max_overhang_deg\"}";

fn printer_prop() -> Prop {
    ("printer", json!({ "type": ["string", "object"], "description": PRINTER_HELP }), false)
}

fn material_prop() -> Prop {
    ("material", json!({ "type": ["string", "object"], "description": MATERIAL_HELP }), false)
}

/// La impresora pedida: por nombre de los de fábrica o con sus medidas
fn printer(ctx: &Ctx, args: &Value) -> Result<Value, Fail> {
    let presets = ctx.call("get_print_presets", json!({}))?;
    let list = presets["printers"].as_array().cloned().unwrap_or_default();
    match args.get("printer") {
        None => Ok(list.first().cloned().unwrap_or(Value::Null)),
        Some(Value::String(want)) => {
            let w = want.to_lowercase();
            list.iter().find(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase().contains(&w))).cloned().ok_or_else(|| {
                let names: Vec<&str> = list.iter().filter_map(|p| p["name"].as_str()).collect();
                Fail::new(format!("no printer preset \"{want}\"")).hint(format!("use one of {} or give {{\"build_volume\": [x, y, z]}}", names.join(", ")))
            })
        }
        Some(custom) => {
            let bv = custom["build_volume"].as_array().filter(|b| b.len() == 3 && b.iter().all(|x| x.as_f64().is_some_and(|x| x > 0.0)));
            let Some(bv) = bv else {
                return Err(Fail::new("custom printer needs \"build_volume\": [x, y, z] in mm"));
            };
            let nozzle = custom["nozzle"].as_f64().unwrap_or(0.4);
            Ok(json!({
                "name": custom["name"].as_str().unwrap_or("Custom"),
                "build_volume": bv,
                "nozzle": nozzle,
                "layer_height": custom["layer_height"].as_f64().unwrap_or(0.2),
                "fit_clearance": nozzle,
                "hole_comp": nozzle * 0.25,
            }))
        }
    }
}

fn material(ctx: &Ctx, args: &Value) -> Result<Value, Fail> {
    let presets = ctx.call("get_print_presets", json!({}))?;
    let list = presets["materials"].as_array().cloned().unwrap_or_default();
    match args.get("material") {
        None => Ok(list.first().cloned().unwrap_or(Value::Null)),
        Some(Value::String(want)) => list.iter().find(|m| m["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(want.trim()))).cloned().ok_or_else(|| {
            let names: Vec<&str> = list.iter().filter_map(|m| m["name"].as_str()).collect();
            Fail::new(format!("no material preset \"{want}\"")).hint(format!("use one of {}", names.join(", ")))
        }),
        Some(custom) => Ok(json!({
            "name": custom["name"].as_str().unwrap_or("Custom"),
            "density": custom["density"].as_f64().unwrap_or(1.24),
            "max_overhang_deg": custom["max_overhang_deg"].as_f64().unwrap_or(45.0),
        })),
    }
}

/// Dirección de la escena (Y arriba) como se muestra (Z arriba)
fn shown(v: &Value) -> [f64; 3] {
    let c = |k: usize| v[k].as_f64().unwrap_or(0.0);
    [c(0), -c(2), c(1)]
}

fn shown_dir(v: &Value) -> String {
    let d = shown(v);
    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
        if (d[k].abs() - 1.0).abs() < 1e-3 {
            return format!("{}{axis}", if d[k] > 0.0 { "+" } else { "-" });
        }
    }
    format!("({}, {}, {})", num(d[0]), num(d[1]), num(d[2]))
}

// ─── Verificar ──────────────────────────────────────────────────────────────

const CHECK: Tool = Tool {
    name: "print_check",
    title: "Check if the model can be printed",
    description: "Checks the active mesh model (or its pieces, after `print_cut`) for FDM printing on a printer and \
material: watertight, walls too thin for the nozzle, overhangs that need support (area and support volume), whether \
it fits the bed (or how many pieces it needs), and the material it takes (grams and metres of filament at `infill`). \
Lists every problem with its severity. Example: {\"printer\": \"Prusa MK4\", \"material\": \"PETG\"}.",
    input: || object(&[printer_prop(), material_prop(), ("infill", number_in("Infill 0–1 for the material estimate (default 0.2)", 0.0, 1.0), false)]),
    effect: Effect::Read,
    run: check,
};

fn check(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let (p, m) = (printer(ctx, args)?, material(ctx, args)?);
    let r = ctx.call("check_print_readiness", json!({ "printer": p, "material": m }))?;
    let est = ctx.call("estimate_print_material", json!({ "material": m, "infill": args["infill"].as_f64().unwrap_or(0.2), "hollowWall": null }))?;
    let (line, analysis) = model_line(ctx)?;
    let pieces = r["subject"] != "modelo";
    let mut lines = vec![format!(
        "{} on {} with {}: {}.",
        if pieces { "The pieces" } else { "The model" },
        p["name"].as_str().unwrap_or(""),
        m["name"].as_str().unwrap_or(""),
        if r["printable"] == true { "printable" } else { "NOT printable as it is" }
    )];
    if !pieces {
        lines.push(format!("Model: {line}"));
    }
    let fit = &r["fit"];
    lines.push(if fit["fits"] == true {
        "Fits the bed.".into()
    } else if fit["fits_rotated"] == true {
        "Fits the bed turned 90° on Z.".into()
    } else {
        let n = fit["pieces_needed"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(1)).product::<u64>()).unwrap_or(1);
        format!("Too big for the bed (by {} mm): about {n} pieces with `print_cut`, or `print_scale`.", dims(&fit["excess"]))
    });
    lines.push(format!(
        "Overhangs: {} cm² need support (about {} cm³ of support); minimum wall {}.",
        num(r["overhang_area"].as_f64().unwrap_or(0.0) / 100.0),
        num(r["support_volume"].as_f64().unwrap_or(0.0) / 1000.0),
        r["min_thickness"].as_f64().map_or_else(|| "not measured".into(), |t| format!("{} mm", num(t)))
    ));
    lines.push(format!("Material: {} g, {} m of 1.75 mm filament.", num(est["mass_g"].as_f64().unwrap_or(0.0)), num(est["filament_m"].as_f64().unwrap_or(0.0))));
    for issue in r["issues"].as_array().into_iter().flatten() {
        lines.push(format!("  [{}] {}", issue["severity"].as_str().unwrap_or(""), issue["message"].as_str().unwrap_or("")));
    }
    if r["is_watertight"] == false {
        lines.push("It is not watertight: `mesh_repair` first.".into());
    }
    let mut report = r.clone();
    if let Some(o) = report.as_object_mut() {
        o.remove("thin_points");
    }
    Ok(Output::new(lines.join("\n"), json!({ "printer": p, "material": m, "report": report, "estimate": est, "analysis": analysis })))
}

// ─── Escalar ────────────────────────────────────────────────────────────────

const SCALE: Tool = Tool {
    name: "print_scale",
    title: "Scale the model",
    description: "Changes the size of the active mesh model. One of: `factor` (uniform, 2 = twice as big), `size` \
[x, y, z] in mm to fit inside (keeps proportions; 0 = any), `factors` [fx, fy, fz] per axis (stretches), or \
`volume_cm3`. Z is up. Example: {\"size\": [0, 0, 120]} makes it 120 mm tall.",
    input: || {
        object(&[
            ("factor", number_in("Uniform factor", 1e-4, 1e4), false),
            ("size", point("Fit inside this box [x, y, z] mm (0 = no limit on that axis)", 3), false),
            ("factors", point("Per-axis factors [fx, fy, fz]", 3), false),
            ("volume_cm3", number_in("Target volume in cm³ (closed models)", 1e-6, 1e9), false),
        ])
    },
    effect: Effect::Model,
    run: scale,
};

fn scale(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let given: Vec<&str> = ["factor", "size", "factors", "volume_cm3"].into_iter().filter(|k| args.get(*k).is_some()).collect();
    if given.len() != 1 {
        return Err(Fail::new("give exactly one of `factor`, `size`, `factors`, `volume_cm3`"));
    }
    let params = match given[0] {
        "factor" => json!({ "mode": "uniform", "factor": args["factor"] }),
        "factors" => json!({ "mode": "axes", "factors": args["factors"] }),
        "volume_cm3" => json!({ "mode": "volume", "target_volume": args["volume_cm3"].as_f64().unwrap_or(0.0) * 1000.0 }),
        _ => {
            // Factor uniforme para entrar en la caja (agranda o achica); 0 = sin límite en ese eje
            let now = ctx.call("analyze_print3d", json!({}))?;
            let factor = (0..3)
                .filter_map(|k| {
                    let want = args["size"][k].as_f64().unwrap_or(0.0);
                    let have = now["dimensions"][k].as_f64().unwrap_or(0.0);
                    (want > 0.0 && have > 1e-9).then(|| want / have)
                })
                .fold(f64::INFINITY, f64::min);
            if !factor.is_finite() {
                return Err(Fail::new("`size` needs at least one axis above 0"));
            }
            json!({ "mode": "uniform", "factor": factor })
        }
    };
    let before = ctx.call("analyze_print3d", json!({}))?;
    let after = ctx.call("scale_mesh_for_print", json!({ "params": params }))?;
    Ok(Output::new(format!("Scaled: {} → {}.", dims(&before["dimensions"]), dims(&after["dimensions"])), json!({ "before": before, "after": after })))
}

// ─── Orientar ───────────────────────────────────────────────────────────────

const ORIENT: Tool = Tool {
    name: "print_orient",
    title: "Lay the model on its best face for printing",
    description: "Finds the ways of resting the active mesh model on the bed with the least overhang and support, \
and lays it on the chosen one (default the best, `choice` 0): rotates it so that face is down, sits it on Z = 0 and \
centres it. Returns all the candidates with their support volume, contact area and height. \
Example: {} or {\"choice\": 1, \"material\": \"PETG\"}.",
    input: || object(&[printer_prop(), material_prop(), ("choice", integer("Candidate to use, 0 = best (default)", 0), false)]),
    effect: Effect::Model,
    run: orient,
};

fn orient(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let (p, m) = (printer(ctx, args)?, material(ctx, args)?);
    let found = ctx.call("suggest_print_orientation", json!({ "printer": p, "material": m }))?;
    let list = found.as_array().cloned().unwrap_or_default();
    let k = args["choice"].as_u64().unwrap_or(0) as usize;
    let chosen = list.get(k).ok_or_else(|| Fail::new(format!("there are {} candidates; `choice` {k} does not exist", list.len())))?;
    ctx.call("placement_floor", json!({ "down": chosen["down"] }))?;
    let mut lines = vec![format!(
        "Laid on the face that pointed {}: support {} cm³, contact {} cm², height {} mm.",
        shown_dir(&chosen["down"]),
        num(chosen["support_volume"].as_f64().unwrap_or(0.0) / 1000.0),
        num(chosen["contact_area"].as_f64().unwrap_or(0.0) / 100.0),
        num(chosen["height"].as_f64().unwrap_or(0.0))
    )];
    lines.push("Candidates:".into());
    let candidates: Vec<Value> = list
        .iter()
        .enumerate()
        .map(|(i, c)| {
            lines.push(format!(
                "  {i}: down {}, support {} cm³, contact {} cm², height {} mm{}",
                shown_dir(&c["down"]),
                num(c["support_volume"].as_f64().unwrap_or(0.0) / 1000.0),
                num(c["contact_area"].as_f64().unwrap_or(0.0) / 100.0),
                num(c["height"].as_f64().unwrap_or(0.0)),
                if i == k { " [used]" } else { "" }
            ));
            json!({ "down": shown(&c["down"]), "support_volume": c["support_volume"], "contact_area": c["contact_area"], "height": c["height"], "overhang_area": c["overhang_area"] })
        })
        .collect();
    Ok(Output::new(lines.join("\n"), json!({ "choice": k, "candidates": candidates })))
}

// ─── Cortar ─────────────────────────────────────────────────────────────────

const CUT: Tool = Tool {
    name: "print_cut",
    title: "Cut the model into pieces that fit the bed",
    description: "Splits the active mesh model into closed pieces that fit the printer bed, with flat cut faces \
(labelled 1A, 1B…). `strategy`: `grid` (even cuts on each axis; default), `optimal` (fewest pieces), `zlayers` \
(horizontal slices only), or `manual` with `planes` [{\"axis\": \"z\", \"at_mm\": 60}] measured from the model's \
lowest point on that axis (Z up), optionally tilted. Then `print_joints` adds pegs, `print_plates` exports. Running \
it again replaces the pieces. Example: {\"printer\": \"Prusa MINI\"}.",
    input: || {
        object(&[
            printer_prop(),
            ("strategy", one_of("How to split (default grid)", &["grid", "optimal", "zlayers", "manual"]), false),
            (
                "planes",
                array(
                    "manual: cutting planes",
                    object(&[
                        ("axis", one_of("Axis the plane is perpendicular to", &["x", "y", "z"]), true),
                        ("at_mm", number_in("Distance from the model's lowest point on that axis (mm)", 0.0, 1e6), true),
                        ("tilt_deg", number_in("Tilt of the plane (degrees, default 0)", -80.0, 80.0), false),
                    ]),
                ),
                false,
            ),
            ("margin", number_in("Space kept free at the bed edges (mm, default 5)", 0.0, 100.0), false),
        ])
    },
    effect: Effect::Model,
    run: cut,
};

fn cut(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let p = printer(ctx, args)?;
    let strategy = arg_str(args, "strategy").unwrap_or("grid");
    let planes: Vec<Value> = args["planes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|pl| {
            let axis = ["x", "y", "z"].iter().position(|a| pl["axis"] == *a).unwrap_or(2);
            json!({ "axis": axis, "position": pl["at_mm"], "tilt_deg": pl["tilt_deg"].as_f64().unwrap_or(0.0) })
        })
        .collect();
    if strategy == "manual" && planes.is_empty() {
        return Err(Fail::new("`manual` needs `planes`").hint("e.g. {\"strategy\": \"manual\", \"planes\": [{\"axis\": \"z\", \"at_mm\": 60}]}"));
    }
    let config = json!({ "build_volume": p["build_volume"], "strategy": strategy, "margin": args["margin"].as_f64(), "planes": planes });
    let r = ctx.call("subdivide_mesh", json!({ "config": config }))?;
    let pieces = r["pieces"].as_array().cloned().unwrap_or_default();
    let mut lines = vec![format!("Cut into {} piece(s) for {} ({}):", pieces.len(), p["name"].as_str().unwrap_or(""), dims(&p["build_volume"]))];
    for piece in &pieces {
        lines.push(format!("  {}: {}, {} triangles", piece["label"].as_str().unwrap_or(""), dims(&piece["dimensions"]), piece["face_count"]));
    }
    lines.push("Next: `print_joints` to add pegs between pieces, `view` with `show: \"pieces\"` to look, `print_plates` to export.".into());
    Ok(Output::new(lines.join("\n"), r))
}

// ─── Uniones ────────────────────────────────────────────────────────────────

const JOINTS: Tool = Tool {
    name: "print_joints",
    title: "Add pegs between the cut pieces",
    description: "Puts connectors on the flat cut faces between the pieces from `print_cut`: a peg on one piece and a \
matching hole on the other, with the clearance for the `fit` (tight, normal, loose) on the printer and material. \
`shape`: round (default), square, dovetail, bayonet (quarter-turn lock), rail (sliding dovetail), terrace, puzzle. \
`size` is the peg width in mm (default 6), `depth` the hole depth (default 10), `count` per cut face (default 2). \
Running it again replaces the joints. Example: {\"shape\": \"round\", \"size\": 8, \"fit\": \"tight\"}.",
    input: || {
        object(&[
            ("shape", one_of("Connector shape (default round)", &["round", "square", "dovetail", "bayonet", "rail", "terrace", "puzzle"]), false),
            ("size", number_in("Peg width in mm (default 6)", 1.0, 200.0), false),
            ("depth", number_in("Hole depth in mm (default 10)", 1.0, 200.0), false),
            ("count", integer("Connectors per cut face (default 2)", 1), false),
            ("fit", one_of("Clearance (default normal)", &["tight", "normal", "loose"]), false),
            printer_prop(),
            material_prop(),
        ])
    },
    effect: Effect::Model,
    run: joints,
};

fn joints(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let s = args["size"].as_f64().unwrap_or(6.0);
    let shape = match arg_str(args, "shape").unwrap_or("round") {
        "square" => json!({ "kind": "square", "size": s }),
        "dovetail" => json!({ "kind": "dovetail", "width_base": s, "width_top": s * 0.6, "length": s * 1.5 }),
        "bayonet" => json!({ "kind": "bayonet", "length": s * 1.6, "width": s * 0.5 }),
        "rail" => json!({ "kind": "rail", "width": s }),
        "terrace" => json!({ "kind": "terrace", "diameter": s }),
        "puzzle" => json!({ "kind": "puzzle", "size": s }),
        _ => json!({ "kind": "round", "diameter": s }),
    };
    let config = json!({
        "shape": shape,
        "depth": args["depth"].as_f64().unwrap_or(10.0),
        "tolerance": -0.4,
        "count": args["count"].as_u64().unwrap_or(2),
        "edge_margin": 2.0,
        "min_depth": 3.0,
        "min_contact_area": 50.0,
        "rotation_deg": 0.0,
    });
    let fit = json!({ "printer": printer(ctx, args)?, "material": material(ctx, args)?, "fit": arg_str(args, "fit").unwrap_or("normal") });
    let r = ctx.call("add_print_joints", json!({ "config": config, "fit": fit })).map_err(|e| e.hint("cut the model first with `print_cut`"))?;
    let mut lines = vec![format!("{} connector(s) placed.", r["joints_placed"])];
    for c in r["connections"].as_array().into_iter().flatten() {
        let notes: Vec<&str> = c["notes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        lines.push(format!(
            "  piece {} ↔ {}: {}/{} (contact {} cm²){}",
            c["piece_a"].as_u64().unwrap_or(0) + 1,
            c["piece_b"].as_u64().unwrap_or(0) + 1,
            c["placed"],
            c["requested"],
            num(c["contact_area"].as_f64().unwrap_or(0.0) / 100.0),
            if notes.is_empty() { String::new() } else { format!(" — {}", notes.join("; ")) }
        ));
    }
    Ok(Output::new(lines.join("\n"), r))
}

// ─── Placas ─────────────────────────────────────────────────────────────────

fn plate_props() -> Vec<Prop> {
    vec![
        ("path", crate::mcp::schema::string("Absolute output path; .stl, .3mf or .obj. With several plates a number is added (_1, _2…)"), true),
        ("overwrite", boolean("Replace existing files (default false)"), false),
        printer_prop(),
        material_prop(),
        ("spacing", number_in("Gap between pieces on the bed in mm (default 5)", 0.0, 100.0), false),
        ("orient", boolean("Turn each piece to its best resting face (default true)"), false),
        ("hollow_wall", number_in("Hollow each piece leaving this wall thickness in mm (saves material)", 0.4, 50.0), false),
        ("drain_mm", number_in("With hollow: drain hole of this diameter in the bottom (mm)", 0.5, 30.0), false),
        ("compensate_shrink", boolean("Enlarge by the material's cooling shrinkage (default false)"), false),
    ]
}

fn plate_input(ctx: &Ctx, args: &Value, path: &std::path::Path) -> Result<Value, Fail> {
    let format = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !["stl", "3mf", "obj"].contains(&format.as_str()) {
        return Err(Fail::new(format!(".{format} is not a plate format")).hint("use .stl, .3mf or .obj"));
    }
    Ok(json!({
        "printer": printer(ctx, args)?,
        "material": material(ctx, args)?,
        "format": format,
        "spacing": args["spacing"].as_f64().unwrap_or(5.0),
        "orient": arg_bool(args, "orient").unwrap_or(true),
        "contour": true,
        "hollow_wall": args["hollow_wall"].as_f64(),
        "drain": args["drain_mm"].as_f64(),
        "vent": args.get("drain_mm").is_some(),
        "compensate": arg_bool(args, "compensate_shrink").unwrap_or(false),
        "path": path,
    }))
}

fn describe_plates(info: &Value) -> Vec<String> {
    let files: Vec<&str> = info["files"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut lines = vec![format!(
        "{} piece(s) on {} plate(s), {}: {}.",
        info["pieces"],
        info["plates"],
        size(info["total_bytes"].as_u64().unwrap_or(0)),
        files.join(", ")
    )];
    let names = |k: &str| info[k].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
    if !names("unplaced").is_empty() {
        lines.push(format!("Do not fit the bed even alone: {} — `print_cut` them.", names("unplaced")));
    }
    if !names("too_tall").is_empty() {
        lines.push(format!("Taller than the printer: {}.", names("too_tall")));
    }
    for n in info["notes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        lines.push(format!("Note: {n}"));
    }
    lines
}

const PLATES: Tool = Tool {
    name: "print_plates",
    title: "Export print plates",
    description: "Arranges the pieces (from `print_cut`, or the whole model) on as many beds as needed — each turned \
to its best face unless `orient: false` — and writes one file per plate (STL, 3MF or OBJ, mm, Z up). Optional \
hollowing with `hollow_wall` (and a `drain_mm` hole), and shrink compensation. Will not replace files without \
`overwrite: true`. Example: {\"path\": \"/home/me/print/plate.3mf\", \"printer\": \"Bambu Lab X1 / P1\"}.",
    input: || object(&plate_props()),
    effect: Effect::Write,
    run: plates,
};

fn plates(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let path = output_path(args)?;
    let input = plate_input(ctx, args, &path)?;
    let info = ctx.call("export_print_plates", json!({ "input": input }))?;
    Ok(Output::new(describe_plates(&info).join("\n"), info))
}

// ─── Todo en un paso ────────────────────────────────────────────────────────

const PREPARE: Tool = Tool {
    name: "print_prepare",
    title: "Prepare for printing in one step",
    description: "Does the whole print preparation of the active mesh model in order and reports each step: checks it \
(stops if it is not watertight — `mesh_repair` first), cuts it if it does not fit the bed (`strategy`, or \
`force_split`), adds pegs between the pieces if `joints` is given, hollows if `hollow_wall` is given, and writes the \
plates to `path`. Example: {\"path\": \"/home/me/print/dragon.3mf\", \"printer\": \"Prusa MK4\", \"joints\": \
{\"shape\": \"round\", \"size\": 6}}.",
    input: || {
        let mut props = plate_props();
        props.push(("strategy", one_of("How to split if it does not fit (default grid)", &["grid", "optimal", "zlayers"]), false));
        props.push(("force_split", boolean("Split even if it fits (default false)"), false));
        props.push((
            "joints",
            object(&[
                ("shape", one_of("Connector shape", &["round", "square", "dovetail", "bayonet", "rail", "terrace", "puzzle"]), false),
                ("size", number_in("Peg width in mm (default 6)", 1.0, 200.0), false),
                ("fit", one_of("Clearance (default normal)", &["tight", "normal", "loose"]), false),
            ]),
            false,
        ));
        object(&props)
    },
    effect: Effect::Model,
    run: prepare,
};

fn prepare(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let path = output_path(args)?;
    let plate = plate_input(ctx, args, &path)?;
    let joints = args.get("joints").map(|j| {
        let s = j["size"].as_f64().unwrap_or(6.0);
        let shape = match j["shape"].as_str().unwrap_or("round") {
            "square" => json!({ "kind": "square", "size": s }),
            "dovetail" => json!({ "kind": "dovetail", "width_base": s, "width_top": s * 0.6, "length": s * 1.5 }),
            "bayonet" => json!({ "kind": "bayonet", "length": s * 1.6, "width": s * 0.5 }),
            "rail" => json!({ "kind": "rail", "width": s }),
            "terrace" => json!({ "kind": "terrace", "diameter": s }),
            "puzzle" => json!({ "kind": "puzzle", "size": s }),
            _ => json!({ "kind": "round", "diameter": s }),
        };
        json!({ "shape": shape, "depth": 10.0, "tolerance": -0.4, "count": 2, "edge_margin": 2.0, "min_depth": 3.0, "min_contact_area": 50.0, "rotation_deg": 0.0 })
    });
    let fit = args.get("joints").map(|j| json!({ "printer": plate["printer"], "material": plate["material"], "fit": j["fit"].as_str().unwrap_or("normal") }));
    let input = json!({
        "plate": plate,
        "strategy": arg_str(args, "strategy"),
        "joints": joints,
        "fit": fit,
        "force_split": arg_bool(args, "force_split").unwrap_or(false),
    });
    let r = ctx.call("prepare_for_printing", json!({ "input": input }))?;
    let mut lines = Vec::new();
    for step in r["steps"].as_array().into_iter().flatten() {
        lines.push(format!("[{}] {}: {}", step["status"].as_str().unwrap_or(""), step["id"].as_str().unwrap_or(""), step["detail"].as_str().unwrap_or("")));
        for n in step["notes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            lines.push(format!("    {n}"));
        }
    }
    if let Some(info) = r["plates"].as_object() {
        lines.extend(describe_plates(&Value::Object(info.clone())));
    }
    if r["ok"] != true {
        return Err(Fail::new(lines.join("\n")).hint("fix what the failed step says (often `mesh_repair`) and call it again"));
    }
    Ok(Output::new(lines.join("\n"), r))
}
