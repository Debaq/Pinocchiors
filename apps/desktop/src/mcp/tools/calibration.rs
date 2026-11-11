//! Poner a punto una impresora de filamento con la IA: qué está calibrado y qué falta, laminar (y
//! con permiso imprimir) una prueba, y guardar el valor elegido.
//!
//! La calibración es de la impresora física (el id de la impresora de red de la app) y del
//! filamento. Sin la app abierta no hay lista de impresoras: el id es la dirección que dio la IA.

use serde_json::{json, Value};

use super::cad::num;
use super::printers::{approve, connection, saved, PRINTER_HELP};
use super::slice::{find_by, fmt_time, key, names, short};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::calibrations::{self, CalTarget, CalValue, KEYS};
use crate::mcp::schema::{boolean, integer, number, object, one_of, string};

pub const TOOLS: &[Tool] = &[STATUS, TEST, SAVE];

/// Las pruebas que se pueden laminar (las de `slicer::calib` más las de medidas)
const TESTS: &[&str] = &[
    "first_layer",
    "temperature",
    "flow_coarse",
    "flow_fine",
    "pressure_advance",
    "pressure_advance_pattern",
    "retraction",
    "volumetric_speed",
    "vfa",
    "shaper_frequency",
    "shaper_damping",
    "cornering",
    "skew",
    "clearance",
    "block",
];

/// Los pasos en el orden recomendado: prueba, título, qué se guarda y qué sigue siendo válido de antes
struct Step {
    test: &'static str,
    title: &'static str,
    /// Claves que tienen que estar guardadas para darlo por hecho
    keys: &'static [&'static str],
    /// Depende del filamento (si cambia algo anterior del filamento conviene repetirlo)
    filament: bool,
}

const STEPS: &[Step] = &[
    Step { test: "first_layer", title: "Bed and first layer (Z offset)", keys: &["z_offset"], filament: false },
    Step { test: "temperature", title: "Temperature", keys: &["temperature"], filament: true },
    Step { test: "flow_coarse", title: "Flow, coarse pass", keys: &["flow_ratio"], filament: true },
    Step { test: "flow_fine", title: "Flow, fine pass", keys: &["flow_ratio"], filament: true },
    Step { test: "pressure_advance", title: "Pressure advance", keys: &["pressure_advance"], filament: true },
    Step { test: "retraction", title: "Retraction", keys: &["retraction_length"], filament: true },
    Step { test: "volumetric_speed", title: "Max volumetric speed", keys: &["max_volumetric_speed"], filament: true },
    Step { test: "clearance", title: "Fit clearance (clearance plate)", keys: &["fit_clearance"], filament: false },
    Step { test: "block", title: "Dimensions and holes (measured block)", keys: &["hole_comp"], filament: false },
    Step { test: "shaper_frequency", title: "Input shaping, frequency (Klipper)", keys: &["shaper_freq_x", "shaper_freq_y"], filament: false },
    Step { test: "shaper_damping", title: "Input shaping, damping (Klipper)", keys: &["shaper_damping"], filament: false },
    Step { test: "vfa", title: "VFA (fine wall ripples)", keys: &["vfa_speed"], filament: false },
    Step { test: "cornering", title: "Corners", keys: &["corner_speed"], filament: false },
    Step { test: "skew", title: "Axis squareness (skew, Klipper)", keys: &["skew_xy"], filament: false },
];

/// Pruebas que dan lo mismo que otra y no son un paso aparte (el patrón de pressure advance es otra forma de la torre)
const ALTERNATES: &[&str] = &["pressure_advance_pattern"];

/// Qué clave se guarda con el resultado de cada prueba
fn keys_of(test: &str) -> Vec<&'static str> {
    match test {
        "flow_coarse" | "flow_fine" => vec!["flow_ratio"],
        "pressure_advance_pattern" => vec!["pressure_advance"],
        "shaper_frequency" => vec!["shaper_freq_x", "shaper_freq_y"],
        t => KEYS.iter().filter(|k| k.test == t).map(|k| k.key).collect(),
    }
}

fn key_enum() -> Vec<&'static str> {
    KEYS.iter().map(|k| k.key).collect()
}

// ─── Quién y con qué ────────────────────────────────────────────────────────

/// La impresora física
struct Who {
    id: String,
    name: String,
    /// Modelo del laminador (`Creality K1`) de la impresora guardada en la app
    model: String,
    /// Boquilla instalada (mm), si la app la sabe
    nozzle: Option<f64>,
    /// Sin la app: solo se conoce la dirección
    address_only: bool,
}

fn who(ctx: &Ctx, args: &Value) -> Result<Who, Fail> {
    let want = arg_str(args, "printer").unwrap_or("").trim().to_string();
    if want.is_empty() {
        return Err(Fail::new("which printer?").hint("pass `printer`: a name from `printer_list` or an address"));
    }
    let list = saved(ctx);
    let w = want.to_lowercase();
    if let Some(p) = list.iter().find(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase() == w)).or_else(|| list.iter().find(|p| p["address"].as_str() == Some(want.as_str()))) {
        let name = p["name"].as_str().unwrap_or(&want).to_string();
        return Ok(Who { id: p["id"].as_str().filter(|i| !i.is_empty()).unwrap_or(&name).to_string(), name, model: p["model"].as_str().unwrap_or("").to_string(), nozzle: p["nozzle"].as_f64(), address_only: false });
    }
    if !want.chars().any(|c| c == '.' || c == ':') {
        let names: Vec<&str> = list.iter().filter_map(|p| p["name"].as_str()).collect();
        return Err(Fail::new(format!("no printer called \"{want}\"")).hint(if names.is_empty() {
            "give its address (IP), or open the app to use the saved printers".to_string()
        } else {
            format!("use one of: {}", names.join(", "))
        }));
    }
    Ok(Who { id: want.clone(), name: want, model: String::new(), nozzle: None, address_only: true })
}

/// El perfil de máquina del laminador de esa impresora, y una nota si hubo que suponerlo
fn machine(ctx: &Ctx, who: &Who, args: &Value, remembered: &str, assume: bool) -> Result<(Option<Value>, Option<String>), Fail> {
    let info = ctx.call("slicer_presets", json!({}))?;
    let printers = info["printers"].as_array().cloned().unwrap_or_default();
    // Cada boquilla tiene su perfil de máquina: la pedida, la instalada que sabe la app o la de 0,4
    let nozzle = args["nozzle"].as_f64().or(who.nozzle).unwrap_or(0.4);
    let same_model = |model: &str| -> Option<Value> {
        let all: Vec<&Value> = printers.iter().filter(|p| !model.is_empty() && p["model"].as_str().is_some_and(|n| key(n) == key(model))).collect();
        all.iter().find(|p| (p["nozzle"].as_f64().unwrap_or(0.4) - nozzle).abs() < 0.005).or(all.first()).map(|p| (*p).clone())
    };
    if let Some(m) = arg_str(args, "machine") {
        let p = find_by(&printers, "name", m).or_else(|| find_by(&printers, "model", m)).ok_or_else(|| Fail::new(format!("no filament printer \"{m}\"")).hint(format!("use one of: {}", names(&printers, "name"))))?;
        // Un modelo («Creality K1») o un perfil con otra boquilla que la pedida: el del mismo modelo con esa boquilla
        let exact_name = p["name"].as_str().is_some_and(|n| key(n) == key(m));
        if args["nozzle"].is_number() || !exact_name {
            if let Some(q) = same_model(p["model"].as_str().unwrap_or("")) {
                return Ok((Some(q), None));
            }
        }
        return Ok((Some(p.clone()), None));
    }
    let exact = |field: &str, want: &str| printers.iter().find(|p| !want.is_empty() && p[field].as_str().is_some_and(|n| key(n) == key(want))).cloned();
    if let Some(p) = same_model(&who.model).or_else(|| exact("name", remembered)) {
        return Ok((Some(p), None));
    }
    if !assume {
        return Ok((None, None));
    }
    let p = printers.first().cloned().ok_or_else(|| Fail::new("there are no filament printer profiles"))?;
    let note = format!(
        "Note: {} is not a saved printer in the app, so its slicer profile is not known; assumed \"{}\". Pass `machine` (see `print_machines`) if that is wrong.",
        who.name,
        p["name"].as_str().unwrap_or("")
    );
    Ok((Some(p), Some(note)))
}

/// El filamento: el pedido (un perfil de la máquina), el último que se calibró en esta impresora o el de la máquina
fn filament(args: &Value, who: &Who, machine: Option<&Value>) -> Result<String, Fail> {
    let list: Vec<&str> = machine.map(|m| m["filaments"].as_array().into_iter().flatten().filter_map(Value::as_str).collect()).unwrap_or_default();
    if let Some(w) = arg_str(args, "filament") {
        if list.is_empty() {
            return Ok(w.to_string());
        }
        let k = key(w);
        return list
            .iter()
            .find(|n| key(short(n)) == k || key(n) == k)
            .or_else(|| list.iter().find(|n| key(n).contains(&k)))
            .map(|n| n.to_string())
            .ok_or_else(|| Fail::new(format!("no filament \"{w}\" for this printer")).hint(format!("use one of: {}", list.iter().map(|n| short(n)).collect::<Vec<_>>().join(", "))));
    }
    if let Some(f) = calibrations::load()?.printers.iter().find(|p| p.id == who.id).and_then(|p| {
        p.filaments.iter().filter_map(|f| f.values.values().map(|v| v.date.as_str()).max().map(|d| (d.to_string(), f.filament.clone()))).max().map(|x| x.1)
    }) {
        return Ok(f);
    }
    Ok(machine.and_then(|m| m["default_filament"].as_str()).unwrap_or_default().to_string())
}

fn target(who: &Who, machine: Option<&Value>, filament: &str) -> CalTarget {
    CalTarget { printer_id: who.id.clone(), printer_name: who.name.clone(), machine: machine.and_then(|m| m["name"].as_str()).unwrap_or("").to_string(), filament: filament.to_string(), nozzle: None }
}

const PRINTER_HELP_SHORT: &str = "Which physical printer to calibrate (each one has its own calibration)";
const FILAMENT_HELP: &str = "Filament profile of the slicer for this printer (see `print_machines`), e.g. \"PETG\". Default: the one last calibrated on this printer, else the printer's default";
const NOZZLE_HELP: &str = "Nozzle diameter in mm (0.2–1.0). Calibrations are kept per nozzle: a value found with 0.4 is not used with 0.8. Default: the nozzle the printer has in the app, else 0.4";
const MACHINE_HELP: &str = "Only needed when the printer is not saved in the app (you passed an address): the slicer profile, from `print_machines`";

// ─── Estado ─────────────────────────────────────────────────────────────────

const STATUS: Tool = Tool {
    name: "calibration_status",
    title: "What is calibrated on a printer",
    description: "Shows what has been calibrated on one physical printer with one filament (values, date and the test they came from) \
and the steps of the tune-up in the recommended order, marking each as done, to repeat (something it depends on changed \
later) or pending, and which one comes next. Order: bed and first layer, temperature, flow (coarse then fine), pressure \
advance, retraction, max volumetric speed, dimensions and clearances, input shaping, VFA, corners, axis squareness (skew). \
Everything is kept per physical printer AND nozzle diameter (`nozzle`, else the printer's own, else 0.4): a calibration made with \
a 0.4 nozzle is not used with a 0.8 one. `calibrated` in the data says whether this filament has anything calibrated with \
this nozzle. Calibrated values are \
applied by themselves when slicing for that printer with that filament. Start here when the user asks to tune a printer, \
then run the next step with `calibration_test`.",
    input: || {
        object(&[
            ("printer", string(PRINTER_HELP), true),
            ("filament", string(FILAMENT_HELP), false),
            ("machine", string(MACHINE_HELP), false),
            ("nozzle", number(NOZZLE_HELP), false),
        ])
    },
    effect: Effect::Read,
    run: status,
};

fn when(date: &str) -> &str {
    date.split('T').next().unwrap_or(date)
}

fn show(v: &CalValue) -> String {
    let mut s = format!("{} ({}", num(v.value), when(&v.date));
    if !v.test.is_empty() && v.test != "manual" {
        s += &format!(", test {}", v.test);
    }
    s += ")";
    if !v.note.is_empty() {
        s += &format!(" — {}", v.note);
    }
    s
}

/// El estado de un paso: `done`, `repeat` (algo anterior del filamento cambió después) o `pending`
fn step_state(i: usize, values: &std::collections::BTreeMap<String, CalValue>) -> &'static str {
    let step = &STEPS[i];
    let done = step.keys.iter().all(|k| values.contains_key(*k)) && (step.test != "flow_fine" || values["flow_ratio"].test == "flow_fine");
    if !done {
        return "pending";
    }
    let mine = step.keys.iter().filter_map(|k| values.get(*k).map(|v| v.date.as_str())).max().unwrap_or("");
    let older_changed = step.filament
        && STEPS[..i].iter().filter(|s| s.filament && s.keys != step.keys).any(|s| s.keys.iter().filter_map(|k| values.get(*k)).any(|v| v.date.as_str() > mine));
    if older_changed { "repeat" } else { "done" }
}

fn status(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let who = who(ctx, args)?;
    let remembered = calibrations::load()?.printers.iter().find(|p| p.id == who.id).map(|p| p.machine.clone()).unwrap_or_default();
    let (machine, _) = machine(ctx, &who, args, &remembered, false)?;
    let filament = filament(args, &who, machine.as_ref())?;
    let t = target(&who, machine.as_ref(), &filament);
    let view = calibrations::view(&t)?;
    let mut lines = vec![format!(
        "{} — {}, nozzle {} mm{}.",
        who.name,
        if view.machine.is_empty() { "slicer profile unknown".to_string() } else { view.machine.clone() },
        num(view.nozzle),
        if filament.is_empty() { String::new() } else { format!(", filament {}", short(&filament)) }
    )];
    lines.push("Calibrations are kept per nozzle: temperature, flow, pressure advance, retraction, max speed, shrink, first layer (Z offset), clearance and hole compensation belong to the nozzle in use; input shaping, VFA, corners and skew belong to the printer.".into());
    if view.nozzles.iter().any(|n| (n - view.nozzle).abs() > 0.005) {
        lines.push(format!("Also calibrated on this printer with other nozzles: {} mm.", view.nozzles.iter().filter(|n| (*n - view.nozzle).abs() > 0.005).map(|n| num(*n)).collect::<Vec<_>>().join(", ")));
    }
    if who.address_only {
        lines.push(format!("Not a saved printer: calibrations are kept under the id \"{}\" (the address). With the app open and the printer saved in it, use its name so they stay together.", who.id));
    }
    if filament.is_empty() {
        lines.push("No filament chosen: pass `filament` to see the filament-dependent values (temperature, flow…).".into());
    }
    if view.values.is_empty() {
        lines.push("Nothing calibrated yet.".into());
    } else {
        lines.push("Calibrated:".into());
        for k in KEYS {
            if let Some(v) = view.values.get(k.key) {
                lines.push(format!("- {}: {}", k.key, show(v)));
            }
        }
    }
    if !view.filaments.is_empty() {
        lines.push(format!("Filaments calibrated on this printer with this nozzle: {}.", view.filaments.iter().map(|f| short(f)).collect::<Vec<_>>().join(", ")));
    }
    lines.push("Steps in the recommended order:".into());
    let mut next: Option<&str> = None;
    let mut steps = Vec::new();
    for (i, s) in STEPS.iter().enumerate() {
        let state = if s.filament && filament.is_empty() { "pending" } else { step_state(i, &view.values) };
        if next.is_none() && state != "done" {
            next = Some(s.test);
        }
        let mark = match state {
            "done" => "[done]",
            "repeat" => "[repeat: something before it changed]",
            _ => "[pending]",
        };
        lines.push(format!("{}. {} (`{}`) {mark}{}", i + 1, s.title, s.test, if next == Some(s.test) { " <- next" } else { "" }));
        steps.push(json!({ "test": s.test, "title": s.title, "state": state, "next": next == Some(s.test), "keys": s.keys }));
    }
    lines.push(match next {
        Some(n) => format!("Next: `calibration_test` {{\"printer\": \"{}\", \"test\": \"{n}\"}}. Optional steps (input shaping, VFA, corners, measured block) can be skipped for everyday printing.", who.name),
        None => "Everything is calibrated. Repeat a test only if something changed (nozzle, new spool brand, hardware).".into(),
    });
    Ok(Output::new(
        lines.join("\n"),
        json!({ "printer": { "id": who.id, "name": who.name, "machine": view.machine }, "nozzle": view.nozzle, "nozzles": view.nozzles, "calibrated": view.calibrated, "filament": filament, "values": view.values, "filaments": view.filaments, "steps": steps, "next": next }),
    ))
}

// ─── Prueba ─────────────────────────────────────────────────────────────────

const TEST: Tool = Tool {
    name: "calibration_test",
    title: "Slice (and print) a calibration test",
    description: "Slices one calibration test for a physical printer and filament, on top of what is already calibrated, and says how \
to read the printed result and what value each block or piece means. With `print: true` it also uploads the G-code and \
prints it, ONLY if the user asked for it: they approve in the app (or, without the app, you must have asked and pass \
`user_confirmed: true`); remind them to clear the bed. Without `print` it only slices (the G-code stays as the last \
slice of the app). Tests: first_layer (live Z offset), temperature, flow_coarse then flow_fine, pressure_advance, retraction, \
volumetric_speed, vfa, shaper_frequency then shaper_damping (Klipper), cornering, skew (squareness plate, Klipper), clearance (fit plate) and block (measured \
block). pressure_advance_pattern is the pattern version of pressure_advance (Klipper or Marlin 2; it may be unavailable in this build, then use the tower). `start`/`end`/`step` change the range (defaults depend on the filament type); `frequency` is the chosen shaper \
frequency for shaper_damping. After the user reports (or a photo shows) the best block, save it with `calibration_save`.",
    input: || {
        object(&[
            ("printer", string(PRINTER_HELP), true),
            ("test", one_of("The test to slice", TESTS), true),
            ("filament", string(FILAMENT_HELP), false),
            ("machine", string(MACHINE_HELP), false),
            ("nozzle", number(NOZZLE_HELP), false),
            ("start", number("First value of the range (default by filament type)"), false),
            ("end", number("Last value of the range"), false),
            ("step", number("Step between values"), false),
            ("frequency", number("shaper_damping only: the shaper frequency (Hz) already chosen"), false),
            ("print", boolean("Also upload and print it on the printer (default false: only slice)"), false),
            ("slot", integer("Flashforge AD5X: spool 1–4", 1), false),
            ("user_confirmed", boolean("Only without the app and with `print`: the user explicitly agreed to print"), false),
        ])
    },
    effect: Effect::Write,
    run: test,
};

fn read_blocks(unit: &str, blocks: &[Value]) -> Vec<String> {
    let mut lines = vec!["Blocks from the bottom up (the value is what the printer used there):".to_string()];
    for (i, b) in blocks.iter().enumerate() {
        lines.push(format!("- block {}: {} {unit} (height {}–{} mm)", i + 1, num(b["value"].as_f64().unwrap_or(0.0)), num(b["z0"].as_f64().unwrap_or(0.0)), num(b["z1"].as_f64().unwrap_or(0.0))));
    }
    lines
}

/// Cómo se lee lo impreso
fn reading_text(test: &str, reading: &Value, unit: &str) -> Vec<String> {
    let mut lines = match reading["kind"].as_str() {
        Some("blocks") => {
            let mut l = read_blocks(unit, reading["blocks"].as_array().map(Vec::as_slice).unwrap_or_default());
            l.push("Pick the block that looks best; its value is the result.".into());
            l
        }
        Some("pieces") => {
            let mut l = vec!["Separate pieces on the bed, each with its mark embossed (the value is what the printer used for it):".to_string()];
            for p in reading["pieces"].as_array().into_iter().flatten() {
                l.push(format!("- \"{}\" → {} at ({}, {}) mm", p["mark"].as_str().unwrap_or(""), num(p["value"].as_f64().unwrap_or(0.0)), num(p["x"].as_f64().unwrap_or(0.0)), num(p["y"].as_f64().unwrap_or(0.0))));
            }
            l.push("Pick the piece with the best surface (no gaps or ridges, smooth top); its value is the result.".into());
            l
        }
        Some("numbers") => {
            let values: Vec<String> = reading["values"].as_array().into_iter().flatten().filter_map(Value::as_f64).map(num).collect();
            vec![
                format!("A pattern of lines with corners, one set per pressure advance value: {}. The printer prints the numbers next to the lines.", values.join(", ")),
                "Pick the number where the corners are sharpest and the lines stay even (no bulge at the corner, no gap).".into(),
            ]
        }
        Some("skew") => {
            let (side, pin) = (reading["side_mm"].as_f64().unwrap_or(100.0), reading["pin_mm"].as_f64().unwrap_or(6.0));
            vec![format!(
                "A square frame with a round post (Ø{} mm) in each corner, {} mm between post centers. Corners: D = front-left (the origin, 1 dot), A = front-right (2 dots), C = back-left (3 dots), B = back-right (4 dots). With calipers, measure OUTSIDE edge to outside edge of the two posts: AC (diagonal), BD (the other diagonal) and AD (the front side, along X). Save with `calibration_save` key skew_xy passing `skew_ac`, `skew_bd`, `skew_ad` (the three measures: the post diameter is subtracted for you).",
                num(pin),
                num(side)
            )]
        }
        Some("height") => {
            let (h, a, b) = (reading["height_mm"].as_f64().unwrap_or(1.0), reading["start"].as_f64().unwrap_or(0.0), reading["end"].as_f64().unwrap_or(0.0));
            vec![format!(
                "One tall print whose value changes with height: {} {unit} at the bottom to {} {unit} at {} mm. Measure the height (mm from the bed) where it looks best: value = {} + height / {} × ({} − {}). Approximate; a ruler or calipers are enough.",
                num(a),
                num(b),
                num(h),
                num(a),
                num(h),
                num(b),
                num(a)
            )]
        }
        _ => vec!["The Z offset is adjusted live while the first layer prints: closer/farther with `printer_control` {\"kind\": \"z_offset\", \"delta\": ±0.02}; when the lines just touch without gaps or ridges, save the printer's total offset.".to_string()],
    };
    lines.push(
        match test {
            "temperature" => "Look for the block with the cleanest overhangs and bridges, no stringing and good layer bonding.",
            "flow_coarse" | "flow_fine" => "Look at the top surface of each piece: smooth and closed, without gaps (too little) or ridges (too much).",
            "pressure_advance" => "Look at the corners: sharp and even, with no bulge and no gap. Save the value at that block.",
            "pressure_advance_pattern" => "Look at the pattern: the number where the corners are sharpest and the lines stay even. Save it as pressure_advance.",
            "skew" => "A perfect square gives skew 0; the app computes the factor like Klipper's SET_SKEW. It is applied in the start G-code and needs a [skew_correction] section in printer.cfg; factory printers (K1 without root, AD5X) cannot apply it.",
            "retraction" => "Look for the block with the fewest strings between the towers.",
            "volumetric_speed" => "Look for where the surface starts to get rough or thin: the value just before that is the limit.",
            "vfa" => "Look for the speed band with the least fine ripples on the wall; the highest speed that is still clean is the result.",
            "shaper_frequency" => "Look for where ringing (ghost ripples after corners) is least; save the same frequency for X and Y unless the user measured each axis.",
            "shaper_damping" => "Look for where ringing is least at that frequency.",
            "cornering" => "Look for the highest value that still has sharp, bulge-free corners.",
            "clearance" => "Measure which plate pair of pieces slides together with a snug fit; its clearance is the result.",
            "block" => "Measure the printed block with calipers and compare with the nominal sizes: the difference gives the hole compensation and the shrink per axis.",
            _ => "",
        }
        .to_string(),
    );
    lines.retain(|l| !l.is_empty());
    let saves = keys_of(test);
    if !saves.is_empty() {
        lines.push(format!("Then save it with `calibration_save` (key: {}).", saves.join(" and ")));
    }
    lines
}

fn test(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let test = arg_str(args, "test").unwrap_or("");
    let print = arg_bool(args, "print") == Some(true);
    let who = who(ctx, args)?;
    let remembered = calibrations::load()?.printers.iter().find(|p| p.id == who.id).map(|p| p.machine.clone()).unwrap_or_default();
    let (machine, note) = machine(ctx, &who, args, &remembered, true)?;
    let machine = machine.ok_or_else(|| Fail::new("no slicer profile for this printer"))?;
    let filament = filament(args, &who, Some(&machine))?;
    let info = ctx.call("slicer_presets", json!({}))?;
    if info["available"] != true {
        return Err(Fail::new("the slicing engine (pinocchio-slicer) is missing").hint("it ships with the release builds; in development build it with scripts/build-orca-engine.sh"));
    }
    let t = target(&who, Some(&machine), &filament);
    let range = json!({ "start": args["start"], "end": args["end"], "step": args["step"], "frequency": args["frequency"] });
    let settings = json!({ "printer": machine["name"], "process": machine["default_process"], "filament": filament, "overrides": {} });
    let r = ctx.call("slice_calibration", json!({ "settings": settings, "calibration": t, "test": test, "range": range }))?;
    let res = &r["result"];
    let unit = r["unit"].as_str().unwrap_or("");
    let mut lines = vec![format!(
        "Sliced \"{test}\" for {} ({}, {}): {}, {} g of filament, {} layers. Range {}–{} {unit}, step {}.",
        who.name,
        machine["name"].as_str().unwrap_or(""),
        short(&filament),
        fmt_time(res["time_s"].as_f64().unwrap_or(0.0)),
        num(res["filament_g"].as_f64().unwrap_or(0.0)),
        res["layers"],
        num(r["start"].as_f64().unwrap_or(0.0)),
        num(r["end"].as_f64().unwrap_or(0.0)),
        num(r["step"].as_f64().unwrap_or(0.0)),
    )];
    if let Some(n) = note {
        lines.push(n);
    }
    for w in res["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        lines.push(format!("Note: {w}"));
    }
    lines.push("How to read it:".into());
    lines.extend(reading_text(test, &r["reading"], unit));

    let mut sent = Value::Null;
    if print {
        let (conn, name) = connection(ctx, args)?;
        let file = format!("pinocchio-calib-{test}.gcode");
        let path = std::env::temp_dir().join(&file);
        approve(
            ctx,
            args,
            &format!("IA: {name}"),
            &format!("The AI wants to upload the calibration test \"{test}\" ({file}, {}) to {name} and START PRINTING it. Is the bed clear?", fmt_time(res["time_s"].as_f64().unwrap_or(0.0))),
        )?;
        ctx.call("save_gcode", json!({ "path": path }))?;
        let s = ctx.call("printer_send_file", json!({ "conn": conn, "path": path, "start": true, "slot": args["slot"].as_u64() }))?;
        lines.push(if s["started"] == true {
            format!("Uploaded to {name} and printing. Follow it with `printer_status`; when it ends, ask the user which block/piece looks best (a photo helps) and save it.")
        } else {
            format!("Uploaded to {name} as {file}, but it did not start (the printer was busy).")
        });
        sent = s;
    } else {
        lines.push("Not sent to any printer (`print: true` uploads and prints it, with the user's OK).".into());
    }
    let mut data = r.clone();
    if let Some(o) = data["result"].as_object_mut() {
        o.remove("preview");
    }
    Ok(Output::new(lines.join("\n"), json!({ "printer": who.name, "printer_id": who.id, "filament": filament, "test": test, "slice": data, "sent": sent })))
}

// ─── Guardar ────────────────────────────────────────────────────────────────

const SAVE: Tool = Tool {
    name: "calibration_save",
    title: "Save a calibrated value",
    description: "Saves the value the user (or a photo of the printed test) settled on, for one physical printer (and filament, for the \
filament-dependent keys). It is kept with the date and the test it came from (previous values stay in the history) and \
applied by itself the next time something is sliced for that printer with that filament and nozzle. Per nozzle \
(`nozzle`; a value found with 0.4 is not used with 0.8): z_offset, fit_clearance, hole_comp (printer level) and temperature, \
flow_ratio (absolute, e.g. 0.98), pressure_advance, retraction_length, max_volumetric_speed, shrink_x/y/z (these need \
`filament`). Any nozzle: shaper_freq_x, shaper_freq_y, shaper_damping, vfa_speed, corner_speed, skew_xy (squareness factor, or \
pass skew_ac/skew_bd/skew_ad measured on the skew test and it is computed; applied as SET_SKEW in Klipper's start G-code, needs \
[skew_correction] in printer.cfg). Only save what the user confirmed. Does not touch the printer itself.",
    input: || {
        object(&[
            ("printer", string(PRINTER_HELP_SHORT), true),
            ("key", one_of("What was calibrated", &key_enum()), true),
            ("value", number("The value, in the unit of the key (°C, mm, mm/s, Hz, ratio…). For skew_xy it can be left out when skew_ac/skew_bd/skew_ad are given"), false),
            ("skew_ac", number("skew_xy only: measured AC diagonal in mm, outside edge to outside edge of the posts"), false),
            ("skew_bd", number("skew_xy only: measured BD diagonal in mm"), false),
            ("skew_ad", number("skew_xy only: measured AD side in mm"), false),
            ("filament", string(FILAMENT_HELP), false),
            ("machine", string(MACHINE_HELP), false),
            ("nozzle", number(NOZZLE_HELP), false),
            ("test", one_of("The test it came from (default the one of the key; `flow_coarse` for the first flow pass)", TESTS), false),
            ("note", string("A short note (e.g. what the user saw)"), false),
        ])
    },
    effect: Effect::Write,
    run: save,
};

fn save(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let key_name = arg_str(args, "key").unwrap_or("");
    let mut skew_note = String::new();
    let value = if key_name == "skew_xy" && args["value"].is_null() {
        let m = |k: &str| args[k].as_f64().ok_or_else(|| Fail::new(format!("skew_xy needs `value` (the factor) or all of skew_ac, skew_bd and skew_ad (missing {k})")));
        let (ac, bd, ad) = (m("skew_ac")?, m("skew_bd")?, m("skew_ad")?);
        let pin = pinocchio_slicer::calib::SKEW_PIN_MM;
        skew_note = format!("AC {} / BD {} / AD {} mm (outside edges)", num(ac), num(bd), num(ad));
        pinocchio_slicer::calib::skew_factor(ac - pin, bd - pin, ad - pin).ok_or_else(|| Fail::new("those measures do not make a square (check AC, BD and AD, in mm)"))?
    } else {
        args["value"].as_f64().ok_or_else(|| Fail::new("`value` must be a number"))?
    };
    let info = calibrations::key_info(key_name).ok_or_else(|| Fail::new(format!("nothing is calibrated as \"{key_name}\"")).hint(format!("use one of: {}", key_enum().join(", "))))?;
    let who = who(ctx, args)?;
    let remembered = calibrations::load()?.printers.iter().find(|p| p.id == who.id).map(|p| p.machine.clone()).unwrap_or_default();
    let (machine, _) = machine(ctx, &who, args, &remembered, false)?;
    let filament = if info.printer_level { String::new() } else { filament(args, &who, machine.as_ref())? };
    if !info.printer_level && filament.is_empty() {
        return Err(Fail::new(format!("\"{key_name}\" depends on the filament")).hint("pass `filament`"));
    }
    let t = target(&who, machine.as_ref(), &filament);
    let test = arg_str(args, "test").unwrap_or(info.test);
    let note = [arg_str(args, "note").unwrap_or("").to_string(), skew_note].into_iter().filter(|n| !n.is_empty()).collect::<Vec<_>>().join("; ");
    let view = calibrations::set_value(&t, key_name, value, test, &note)?;
    let mut text = format!(
        "Saved {key_name} = {} for {}{} (test {test}). It will be applied when slicing for this printer{}.",
        num(value),
        who.name,
        if info.printer_level { String::new() } else { format!(" with {}", short(&filament)) },
        if info.printer_level { "" } else { " with that filament" }
    );
    text += &format!(" Kept for the {} mm nozzle{}.", num(view.nozzle), if info.nozzle_level { " only" } else { " (and any other: it belongs to the printer)" });
    if key_name == "skew_xy" {
        text += " It goes in the start G-code as SET_SKEW (Klipper) and needs a [skew_correction] section in the printer's printer.cfg; the printer is not modified by this.";
    }
    if who.address_only {
        text += &format!(" Kept under the id \"{}\" (the address): the app does not know this printer.", who.id);
    }
    // El paso que sigue, con el filamento de siempre si el valor guardado era de la impresora
    let fil = if filament.is_empty() { self::filament(args, &who, machine.as_ref()).unwrap_or_default() } else { filament.clone() };
    let after = calibrations::view(&CalTarget { filament: fil.clone(), ..t.clone() })?;
    let pending: Vec<&str> = STEPS.iter().enumerate().filter(|(i, s)| !(s.filament && fil.is_empty()) && step_state(*i, &after.values) != "done").map(|(_, s)| s.test).take(1).collect();
    if let Some(n) = pending.first() {
        text += &format!(" Next step: `{n}` (see `calibration_status`).");
    }
    Ok(Output::new(text, json!({ "printer": who.name, "filament": filament, "key": key_name, "value": value, "values": view.values })))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::mcp::backend::Local;
    use crate::mcp::session::Session;

    fn run(tool: &Tool, args: Value) -> Result<Output, Fail> {
        let session = Session::new(Box::new(Local::new()));
        let cancel = AtomicBool::new(false);
        let progress = |_: f64, _: &str| {};
        (tool.run)(&Ctx { session: &session, progress: &progress, cancel: &cancel }, &args)
    }

    #[test]
    fn guardar_y_ver_el_estado_sin_ventana() {
        calibrations::tests::with_data_dir(|| {
            // Sin la app solo hay la dirección: es el id
            let st = run(&STATUS, json!({ "printer": "192.168.1.50", "filament": "PETG", "machine": "K1" })).unwrap_or_else(|e| panic!("{}", e.message));
            assert!(st.text.contains("Nothing calibrated yet") && st.text.contains("<- next"), "{}", st.text);
            assert_eq!(st.data["next"], "first_layer");

            let s = run(&SAVE, json!({ "printer": "192.168.1.50", "key": "z_offset", "value": -0.04, "machine": "K1" })).unwrap_or_else(|e| panic!("{}", e.message));
            assert!(s.text.contains("z_offset = -0.04") && s.text.contains("Next step: `temperature`"), "{}", s.text);
            // Dependiente del filamento
            run(&SAVE, json!({ "printer": "192.168.1.50", "key": "temperature", "value": 225, "filament": "PETG", "machine": "K1", "note": "sin hilos" })).unwrap_or_else(|e| panic!("{}", e.message));
            run(&SAVE, json!({ "printer": "192.168.1.50", "key": "flow_ratio", "value": 0.97, "filament": "PETG", "machine": "K1", "test": "flow_coarse" })).unwrap_or_else(|e| panic!("{}", e.message));

            let st = run(&STATUS, json!({ "printer": "192.168.1.50", "machine": "K1" })).unwrap_or_else(|e| panic!("{}", e.message));
            // Sin `filament` toma el último calibrado
            assert!(st.text.contains("- temperature: 225") && st.text.contains("sin hilos") && st.text.contains("- z_offset: -0.04"), "{}", st.text);
            let state = |t: &str| st.data["steps"].as_array().unwrap().iter().find(|s| s["test"] == t).unwrap()["state"].clone();
            assert_eq!(state("first_layer"), "done");
            assert_eq!(state("temperature"), "done");
            assert_eq!(state("flow_coarse"), "done");
            assert_eq!(state("flow_fine"), "pending");
            assert_eq!(st.data["next"], "flow_fine");

            // Errores claros
            assert!(run(&SAVE, json!({ "printer": "192.168.1.50", "key": "nada", "value": 200 })).is_err());
            assert!(run(&SAVE, json!({ "printer": "k1", "key": "z_offset", "value": 0 })).is_err());
        });
    }

    #[test]
    fn cada_boquilla_con_su_calibracion_y_la_escuadra_por_medidas() {
        calibrations::tests::with_data_dir(|| {
            let addr = "192.168.1.51";
            run(&SAVE, json!({ "printer": addr, "key": "temperature", "value": 225, "filament": "PETG", "machine": "K1", "nozzle": 0.8 })).unwrap_or_else(|e| panic!("{}", e.message));
            run(&SAVE, json!({ "printer": addr, "key": "shaper_freq_x", "value": 55, "machine": "K1", "nozzle": 0.8 })).unwrap_or_else(|e| panic!("{}", e.message));
            // Con 0,4 (la de siempre) la temperatura de 0,8 no está; el input shaping, de la impresora, sí
            let st = run(&STATUS, json!({ "printer": addr, "filament": "PETG", "machine": "K1" })).unwrap_or_else(|e| panic!("{}", e.message));
            assert_eq!(st.data["nozzle"], 0.4);
            assert_eq!(st.data["calibrated"], false);
            assert!(!st.text.contains("- temperature:") && st.text.contains("- shaper_freq_x: 55") && st.text.contains("other nozzles: 0.8"), "{}", st.text);
            let st = run(&STATUS, json!({ "printer": addr, "filament": "PETG", "machine": "K1", "nozzle": 0.8 })).unwrap_or_else(|e| panic!("{}", e.message));
            assert_eq!((st.data["nozzle"].clone(), st.data["calibrated"].clone()), (json!(0.8), json!(true)));
            assert!(st.text.contains("- temperature: 225"), "{}", st.text);

            // Escuadra por medidas: los postes de 6 mm se descuentan y el factor sale como el de Klipper
            let pin = pinocchio_slicer::calib::SKEW_PIN_MM;
            let s = run(&SAVE, json!({ "printer": addr, "key": "skew_xy", "skew_ac": 141.5 + pin, "skew_bd": 141.2 + pin, "skew_ad": 100.0 + pin, "machine": "K1" })).unwrap_or_else(|e| panic!("{}", e.message));
            let f = s.data["value"].as_f64().unwrap();
            assert!((f - pinocchio_slicer::calib::skew_factor(141.5, 141.2, 100.0).unwrap()).abs() < 1e-9 && s.text.contains("[skew_correction]"), "{}", s.text);
            assert!(run(&SAVE, json!({ "printer": addr, "key": "skew_xy", "skew_ac": 10.0, "machine": "K1" })).is_err());
        });
    }

    #[test]
    fn un_valor_anterior_mas_nuevo_pide_repetir() {
        let v = |value: f64, date: &str, test: &str| CalValue { value, date: date.into(), test: test.into(), note: String::new(), history: Vec::new() };
        let mut values = std::collections::BTreeMap::new();
        values.insert("temperature".to_string(), v(225.0, "2026-11-06T12:00Z", "temperature"));
        values.insert("flow_ratio".to_string(), v(0.97, "2026-11-06T12:10Z", "flow_coarse"));
        let at = |t: &str, values: &_| step_state(STEPS.iter().position(|s| s.test == t).unwrap(), values);
        assert_eq!((at("temperature", &values), at("flow_coarse", &values), at("flow_fine", &values)), ("done", "done", "pending"));
        values.insert("temperature".to_string(), v(230.0, "2026-11-06T12:30Z", "temperature"));
        assert_eq!((at("temperature", &values), at("flow_coarse", &values)), ("done", "repeat"));
    }

    #[test]
    fn los_pasos_cubren_todas_las_pruebas_y_claves() {
        for t in TESTS.iter().filter(|t| !ALTERNATES.contains(t)) {
            assert!(STEPS.iter().any(|s| s.test == *t), "falta el paso {t}");
        }
        for k in KEYS {
            let used = STEPS.iter().any(|s| s.keys.contains(&k.key));
            // Las contracciones por eje las da el bloque, pero no son un paso aparte
            assert!(used || k.key.starts_with("shrink_"), "{} no está en ningún paso", k.key);
        }
    }
}
