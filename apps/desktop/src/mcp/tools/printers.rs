//! Las impresoras en la red, para la IA: verlas, leer todo de ellas y manejarlas.
//!
//! Leer no pide permiso. Todo lo que mueve, calienta, imprime, borra o cambia la
//! configuración de una impresora lo tiene que aprobar el usuario: con la app abierta (modo
//! en vivo) sale su diálogo de confirmación; sin la app, la IA tiene que haberle preguntado
//! y pasar `user_confirmed: true`. Nunca por iniciativa propia.

use serde_json::{json, Value};

use super::cad::num;
use super::mesh::absolute;
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{boolean, integer, object, string};

pub const TOOLS: &[Tool] = &[LIST, STATUS, CONTROL, SEND];

pub(super) const PRINTER_HELP: &str = "The printer: its name in the app (see `printer_list`) or its address (IP or host name)";

/// Las impresoras guardadas en la app (solo con la app abierta)
pub(super) fn saved(ctx: &Ctx) -> Vec<Value> {
    match ctx.session.backend.ask_with("printers", Value::Null) {
        Some(Ok(Value::Array(list))) => list,
        _ => Vec::new(),
    }
}

/// La conexión: una guardada en la app por nombre, o una dirección (se prueba Klipper y
/// después Creality de fábrica)
pub(super) fn connection(ctx: &Ctx, args: &Value) -> Result<(Value, String), Fail> {
    let want = arg_str(args, "printer").unwrap_or("").trim().to_string();
    if want.is_empty() {
        return Err(Fail::new("which printer?").hint("pass `printer`: a name from `printer_list` or an address"));
    }
    let list = saved(ctx);
    let w = want.to_lowercase();
    if let Some(p) = list.iter().find(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase() == w)).or_else(|| list.iter().find(|p| p["address"].as_str() == Some(want.as_str()))) {
        let conn = json!({ "kind": p["kind"], "address": p["address"], "apiKey": p["apiKey"], "checkCode": p["checkCode"], "serial": p["serial"] });
        return Ok((conn, p["name"].as_str().unwrap_or(&want).to_string()));
    }
    if !want.chars().any(|c| c == '.' || c == ':') {
        let names: Vec<&str> = list.iter().filter_map(|p| p["name"].as_str()).collect();
        return Err(Fail::new(format!("no printer called \"{want}\"")).hint(if names.is_empty() {
            "give its address (IP), or open the app to use the saved printers".to_string()
        } else {
            format!("use one of: {}", names.join(", "))
        }));
    }
    for kind in ["moonraker", "creality"] {
        let conn = json!({ "kind": kind, "address": want });
        if let Ok(info) = ctx.call("printer_info", json!({ "conn": conn })) {
            return Ok((conn, info["name"].as_str().unwrap_or(&want).to_string()));
        }
    }
    Err(Fail::new(format!("no Klipper (Moonraker) or stock Creality printer answers at {want}")).hint("check that it is on and on the same network"))
}

// ─── Lista ──────────────────────────────────────────────────────────────────

const LIST: Tool = Tool {
    name: "printer_list",
    title: "Printers on the network",
    description: "Lists the network printers saved in the app (name, kind, model, address): Klipper/Moonraker \
(rooted K1, CR-10 Max), stock Creality K1 and Flashforge. Needs the app open (live mode); without it, use an \
address directly in the other printer tools.",
    input: || object(&[]),
    effect: Effect::Read,
    run: list,
};

fn list(ctx: &Ctx, _args: &Value) -> Result<Output, Fail> {
    let list = saved(ctx);
    if list.is_empty() {
        return Ok(Output::new("No saved printers visible (open the app in live mode, or pass an address to the printer tools).", json!({ "printers": [] })));
    }
    let lines: Vec<String> = list
        .iter()
        .map(|p| format!("- {} — {} ({}) at {}", p["name"].as_str().unwrap_or(""), p["model"].as_str().unwrap_or(""), p["kind"].as_str().unwrap_or(""), p["address"].as_str().unwrap_or("")))
        .collect();
    let printers: Vec<Value> = list.iter().map(|p| json!({ "name": p["name"], "kind": p["kind"], "model": p["model"], "address": p["address"] })).collect();
    Ok(Output::new(lines.join("\n"), json!({ "printers": printers })))
}

// ─── Estado y detalles ──────────────────────────────────────────────────────

const STATUS: Tool = Tool {
    name: "printer_status",
    title: "Read a printer",
    description: "Reads everything from a network printer without touching it: what it is doing (file, progress, \
layer, time left), temperatures, nozzle Z offset, fans, speed/flow, the bed mesh (and its tilt), the objects of the \
current print, recent print history and the files stored on it. Use it to diagnose a print or before suggesting \
an action.",
    input: || object(&[("printer", string(PRINTER_HELP), true), ("files", boolean("Also list the stored files (default false)"), false)]),
    effect: Effect::Read,
    run: status,
};

fn status(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (conn, name) = connection(ctx, args)?;
    let s = ctx.call("printer_status", json!({ "conn": conn }))?;
    let d = ctx.call("printer_details", json!({ "conn": conn })).unwrap_or(Value::Null);
    let f = |v: &Value| v.as_f64().unwrap_or(0.0);
    let mut lines = vec![format!(
        "{name}: {}{}.",
        s["state"].as_str().unwrap_or("?"),
        if s["filename"].as_str().is_some_and(|x| !x.is_empty()) { format!(" — {}", s["filename"].as_str().unwrap()) } else { String::new() }
    )];
    if s["state"] == "printing" || s["state"] == "paused" {
        let mut l = format!("Progress {}%", num(f(&s["progress"]) * 100.0));
        if let (Some(a), Some(b)) = (s["layer"].as_u64(), s["layers"].as_u64()) {
            l += &format!(", layer {a}/{b}");
        }
        if let Some(r) = s["remaining"].as_f64() {
            l += &format!(", about {} min left", num((r / 60.0).round()));
        }
        lines.push(l + ".");
    }
    lines.push(format!(
        "Nozzle {} → {} °C, bed {} → {} °C.",
        num(f(&s["extruder"][0]).round()),
        num(f(&s["extruder"][1])),
        num(f(&s["bed"][0]).round()),
        num(f(&s["bed"][1]))
    ));
    if let Some(m) = s["message"].as_str().filter(|m| !m.is_empty()) {
        lines.push(format!("Message: {m}"));
    }
    if d.is_object() {
        let mut l = Vec::new();
        if let Some(z) = d["z_offset"].as_f64() {
            l.push(format!("Z offset {} mm", num(z)));
        }
        if let Some(v) = d["feed_rate"].as_f64() {
            l.push(format!("speed {}%", num(v)));
        }
        if let Some(v) = d["flow_rate"].as_f64() {
            l.push(format!("flow {}%", num(v)));
        }
        let fans: Vec<String> = d["fans"].as_array().into_iter().flatten().map(|x| format!("{} {}%", x[0].as_str().unwrap_or(""), num(f(&x[1])))).collect();
        if !fans.is_empty() {
            l.push(format!("fans {}", fans.join(", ")));
        }
        if !l.is_empty() {
            lines.push(l.join(", ") + ".");
        }
        if let Some(rows) = d["mesh"]["rows"].as_array() {
            let all: Vec<f64> = rows.iter().flat_map(|r| r.as_array().into_iter().flatten().filter_map(Value::as_f64)).collect();
            let (lo, hi) = (all.iter().copied().fold(f64::MAX, f64::min), all.iter().copied().fold(f64::MIN, f64::max));
            lines.push(format!("Bed mesh {}×{}: from {} to {} mm (tilt {} mm{}).", rows.len(), rows.first().and_then(|r| r.as_array()).map_or(0, Vec::len), num(lo), num(hi), num(hi - lo), if hi - lo > 0.5 { " — worth checking the bed" } else { "" }));
        }
        let objects: Vec<&str> = d["objects"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !objects.is_empty() {
            lines.push(format!("Objects in this print: {} (current {}).", objects.join(", "), d["current_object"].as_str().unwrap_or("?")));
        }
        let hist: Vec<String> = d["history"]
            .as_array()
            .into_iter()
            .flatten()
            .take(5)
            .map(|h| format!("{} ({} min, {})", h["name"].as_str().unwrap_or(""), num((f(&h["duration_s"]) / 60.0).round()), if h["running"] == true { "printing now" } else if h["finished"] == true { "finished" } else { "stopped" }))
            .collect();
        if !hist.is_empty() {
            lines.push(format!("Recent prints: {}.", hist.join("; ")));
        }
        let files = d["files"].as_array().map_or(0, Vec::len);
        if files > 0 {
            if arg_bool(args, "files") == Some(true) {
                let names: Vec<&str> = d["files"].as_array().into_iter().flatten().take(40).filter_map(|x| x["name"].as_str()).collect();
                lines.push(format!("{files} stored files, newest first: {}.", names.join(", ")));
            } else {
                lines.push(format!("{files} stored files (`files: true` lists them)."));
            }
        }
    }
    Ok(Output::new(lines.join("\n"), json!({ "printer": name, "status": s, "details": d })))
}

// ─── Órdenes ────────────────────────────────────────────────────────────────

/// Pide al usuario que apruebe una acción sobre la impresora
pub(super) fn approve(ctx: &Ctx, args: &Value, title: &str, message: &str) -> Result<(), Fail> {
    match ctx.session.backend.ask_with("confirm", json!({ "title": title, "message": message })) {
        Some(Ok(Value::Bool(true))) => Ok(()),
        Some(Ok(_)) => Err(Fail::new("the user said no").hint("do not retry unless the user asks again")),
        Some(Err(e)) => Err(Fail::new(format!("could not ask the user: {e}"))),
        // Sin la app: la IA tuvo que preguntarle antes
        None if arg_bool(args, "user_confirmed") == Some(true) => Ok(()),
        None => Err(Fail::new("this acts on a real printer and needs the user's OK").hint("ask the user first; if they agree, call again with `user_confirmed: true`")),
    }
}

const CONTROL: Tool = Tool {
    name: "printer_control",
    title: "Act on a printer",
    description: "Sends one command to a network printer. ONLY when the user asked for it: the user approves it in the \
app (or, without the app, you must have asked and pass `user_confirmed: true`). `control` is one of: \
{\"kind\": \"z_offset\", \"delta\": -0.05} (nozzle height live, mm), {\"kind\": \"nozzle_temp\", \"celsius\": 220}, \
{\"kind\": \"bed_temp\", \"celsius\": 60}, {\"kind\": \"fan\", \"fan\": \"model|aux|case\", \"percent\": 100}, \
{\"kind\": \"light\", \"on\": true}, {\"kind\": \"home\", \"axes\": \"X Y Z\"}, {\"kind\": \"jog\", \"axis\": \"Z\", \"mm\": 5}, \
{\"kind\": \"motors_off\"}, {\"kind\": \"heaters_off\"}, {\"kind\": \"auto_level\"}, {\"kind\": \"pid_nozzle\", \"celsius\": 220}, \
{\"kind\": \"pid_bed\", \"celsius\": 60}, {\"kind\": \"input_shaper\"}, {\"kind\": \"feed_rate\", \"percent\": 100}, \
{\"kind\": \"flow_rate\", \"percent\": 100}, {\"kind\": \"exclude_object\", \"name\": \"…\"}, {\"kind\": \"gcode\", \"script\": \"G28\"}, \
{\"kind\": \"reprint\", \"name\": \"file.gcode\"}, {\"kind\": \"delete_file\", \"name\": \"…\"}, {\"kind\": \"timelapse\", \"on\": true}, \
{\"kind\": \"restart_klipper\"}, {\"kind\": \"restart_firmware\"}, {\"kind\": \"clear_error\"}. Stock K1s run any G-code \
but do not return Klipper's reply; read the effect with `printer_status`.",
    input: || {
        object(&[
            ("printer", string(PRINTER_HELP), true),
            ("control", json!({ "type": "object", "description": "The command, e.g. {\"kind\": \"z_offset\", \"delta\": -0.05}" }), true),
            ("reason", string("Why, in the user's words (shown in the confirmation)"), false),
            ("user_confirmed", boolean("Only without the app: the user explicitly agreed to this"), false),
        ])
    },
    effect: Effect::Write,
    run: control,
};

fn describe(c: &Value) -> String {
    let k = c["kind"].as_str().unwrap_or("?");
    let f = |key: &str| num(c[key].as_f64().unwrap_or(0.0));
    match k {
        "z_offset" => format!("move the nozzle {} mm ({})", f("delta"), if c["delta"].as_f64().unwrap_or(0.0) < 0.0 { "closer to the bed" } else { "away from the bed" }),
        "nozzle_temp" => format!("set the nozzle to {} °C", f("celsius")),
        "bed_temp" => format!("set the bed to {} °C", f("celsius")),
        "gcode" => format!("run G-code: {}", c["script"].as_str().unwrap_or("")),
        "reprint" => format!("print {}", c["name"].as_str().unwrap_or("")),
        "delete_file" => format!("delete {}", c["name"].as_str().unwrap_or("")),
        "exclude_object" => format!("stop printing {}", c["name"].as_str().unwrap_or("")),
        other => other.replace('_', " "),
    }
}

fn control(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let c = args.get("control").filter(|c| c.is_object()).ok_or_else(|| Fail::new("missing `control`"))?;
    let (conn, name) = connection(ctx, args)?;
    let what = describe(c);
    let reason = arg_str(args, "reason").map(|r| format!(" ({r})")).unwrap_or_default();
    approve(ctx, args, &format!("IA: {name}"), &format!("The AI wants to {what} on {name}{reason}."))?;
    ctx.call("printer_control", json!({ "conn": conn, "control": c }))?;
    Ok(Output::new(format!("Done on {name}: {what}. Check the result with `printer_status`."), json!({ "printer": name, "control": c })))
}

const SEND: Tool = Tool {
    name: "printer_send",
    title: "Send G-code to a printer",
    description: "Uploads a G-code file (e.g. from `print_slice`) to a network printer and, with `start: true`, prints \
it if the printer is free. ONLY when the user asked for it: approved in the app, or `user_confirmed: true` without \
it. Remind the user to check the bed is clear.",
    input: || {
        object(&[
            ("printer", string(PRINTER_HELP), true),
            ("path", string("Absolute path of the .gcode"), true),
            ("start", boolean("Start printing after uploading (default false)"), false),
            ("slot", integer("Flashforge AD5X: spool 1–4", 1), false),
            ("user_confirmed", boolean("Only without the app: the user explicitly agreed to this"), false),
        ])
    },
    effect: Effect::Write,
    run: send,
};

fn send(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = absolute(arg_str(args, "path").unwrap_or_default())?;
    if !path.is_file() {
        return Err(Fail::new(format!("{} does not exist", path.display())));
    }
    let (conn, name) = connection(ctx, args)?;
    let start = arg_bool(args, "start") == Some(true);
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    approve(
        ctx,
        args,
        &format!("IA: {name}"),
        &if start { format!("The AI wants to upload {file} to {name} and START PRINTING it. Is the bed clear?") } else { format!("The AI wants to upload {file} to {name} (without printing).") },
    )?;
    let r = ctx.call("printer_send_file", json!({ "conn": conn, "path": path, "start": start, "slot": args["slot"].as_u64() }))?;
    let text = if r["started"] == true {
        format!("{file} uploaded to {name} and printing. Follow it with `printer_status`.")
    } else if start {
        format!("{file} uploaded to {name}, but it did not start (the printer was busy).")
    } else {
        format!("{file} uploaded to {name}.")
    };
    Ok(Output::new(text, r))
}
