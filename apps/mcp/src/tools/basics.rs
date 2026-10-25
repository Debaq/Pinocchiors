//! Lo básico: en qué estado está todo, la guía y deshacer.

use serde_json::{json, Value};

use super::{arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::guide;
use crate::schema::{object, one_of};

pub const TOOLS: &[Tool] = &[STATUS, HELP, UNDO, REDO];

const STATUS: Tool = Tool {
    name: "status",
    title: "Show what is loaded",
    description: "Summary of the current work: mode (standalone or live app), project file and unsaved changes, \
design and assembly tabs (with their ids), the loaded mesh model, skeleton and rig, and what `undo`/`redo` would do. \
Call it first, and whenever you are unsure what state things are in. No arguments.",
    input: || object(&[]),
    effect: Effect::Read,
    run: status,
};

fn status(ctx: &Ctx, _: &Value) -> Result<Output, Fail> {
    let s = ctx.session;
    let summary = ctx.call("workspace_summary", json!({}))?;
    let cad = ctx.call("cad_status", json!({}))?;
    let project = s.project.lock().unwrap().clone();
    let (undo, redo) = s.history();

    let mut lines = vec![format!(
        "Mode: {}.",
        match s.backend.mode() {
            crate::backend::Mode::Standalone => "standalone (own engine, no window)",
            crate::backend::Mode::Live => "live (driving the open Pinocchio app)",
        }
    )];
    lines.push(match &project.path {
        Some(p) => format!("Project: {}{}.", p.display(), if project.dirty { " (unsaved changes)" } else { "" }),
        None => format!("Project: not saved yet{}.", if project.dirty { " (has changes)" } else { "" }),
    });
    lines.extend(describe_workspace(&summary, &cad));
    lines.push(match undo.first() {
        Some(last) => format!("Undo: {} step(s), last \"{last}\".", undo.len()),
        None => "Undo: nothing to undo.".into(),
    });
    if let Some(next) = redo.first() {
        lines.push(format!("Redo: \"{next}\"."));
    }
    let data = json!({
        "mode": s.backend.mode().name(),
        "version": env!("CARGO_PKG_VERSION"),
        "project": { "path": project.path, "unsaved_changes": project.dirty },
        "workspace": summary,
        "cad_engine": cad,
        "undo": undo,
        "redo": redo,
    });
    Ok(Output::new(lines.join("\n"), data))
}

/// El trabajo cargado en frases cortas (pestañas, malla, esqueleto), a partir
/// de `workspace_summary` y `cad_status`
pub(super) fn describe_workspace(summary: &Value, cad: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let active = summary["active_tab"].as_u64();
    let tabs: Vec<String> = summary["design_tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let (kind, unit) = if t["kind"] == "design" { ("design", "operations") } else { ("assembly", "instances") };
            format!(
                "\"{}\" (id {}, {kind}, {} {unit}){}",
                t["name"].as_str().unwrap_or(""),
                t["id"],
                t["items"],
                if t["id"].as_u64() == active { " [active]" } else { "" }
            )
        })
        .collect();
    lines.push(if tabs.is_empty() { "CAD tabs: none.".into() } else { format!("CAD tabs: {}.", tabs.join(", ")) });
    if cad["available"] == false {
        lines.push("CAD engine (OpenCASCADE) is not available in this build: CAD tools will fail.".into());
    }
    lines.push(match summary["model"].as_object() {
        Some(m) => format!(
            "Mesh model: {} — {} vertices, {} faces.",
            m.get("name").and_then(Value::as_str).unwrap_or("(unnamed)"),
            m["vertices"],
            m["faces"]
        ),
        None => "Mesh model: none.".into(),
    });
    if let Some(n) = summary["other_objects"].as_u64().filter(|n| *n > 0) {
        lines.push(format!("Other scene objects: {n}."));
    }
    if let Some(sk) = summary["skeleton"].as_str() {
        lines.push(format!("Skeleton: {sk}{}.", if summary["rigged"] == true { ", rigged (weights computed)" } else { "" }));
    }
    lines
}

const HELP: Tool = Tool {
    name: "help",
    title: "Read the guide",
    description: "Guide on how to use Pinocchio: workflows, units and conventions. Topics: \
`overview` (start here), `conventions` (units, axes, names, errors), `cad` (designing parts), `assembly` (assemblies and 2D drawings), `sketch` (every sketch \
constraint with an example), `features` (every operation type for `cad_add_feature` with an example), `projects` \
(open, save, undo). Without a topic it returns the overview and the list of topics. Example: {\"topic\": \"cad\"}.",
    input: || object(&[("topic", one_of("Guide topic", &topic_names()), false)]),
    effect: Effect::Read,
    run: help,
};

fn topic_names() -> Vec<&'static str> {
    guide::topic_list().into_iter().map(|t| t.0).collect()
}

fn help(_: &Ctx, args: &Value) -> Result<Output, Fail> {
    let name = arg_str(args, "topic").unwrap_or("overview");
    let text = guide::topic(name).ok_or_else(|| Fail::new(format!("no topic \"{name}\"")).hint(format!("topics: {}", topic_names().join(", "))))?;
    let topics: Vec<Value> = guide::topic_list().into_iter().map(|t| json!({ "topic": t.0, "title": t.1 })).collect();
    let others: Vec<String> = guide::topic_list().into_iter().filter(|t| t.0 != name).map(|t| format!("`{}` ({})", t.0, t.1)).collect();
    Ok(Output::new(format!("{text}\n\nOther topics: {}.", others.join(", ")), json!({ "topic": name, "topics": topics })))
}

const UNDO: Tool = Tool {
    name: "undo",
    title: "Undo the last change",
    description: "Reverts the last change made through these tools (up to 20 steps), including opening or creating a \
project. Returns what was undone. `status` shows what would be undone. No arguments.",
    input: || object(&[]),
    effect: Effect::History,
    run: undo,
};

fn undo(ctx: &Ctx, _: &Value) -> Result<Output, Fail> {
    match ctx.session.undo()? {
        Some(label) => Ok(Output::new(format!("Undid \"{label}\"."), json!({ "undone": label }))),
        None => Err(Fail::new("nothing to undo").hint("only changes made in this session through these tools can be undone")),
    }
}

const REDO: Tool = Tool {
    name: "redo",
    title: "Redo the last undone change",
    description: "Re-applies the last change reverted with `undo`. Making a new change clears what can be redone. No arguments.",
    input: || object(&[]),
    effect: Effect::History,
    run: redo,
};

fn redo(ctx: &Ctx, _: &Value) -> Result<Output, Fail> {
    match ctx.session.redo()? {
        Some(label) => Ok(Output::new(format!("Redid \"{label}\"."), json!({ "redone": label }))),
        None => Err(Fail::new("nothing to redo")),
    }
}
