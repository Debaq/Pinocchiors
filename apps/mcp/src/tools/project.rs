//! Proyectos: nuevo, abrir y guardar archivos `.pinocchio`.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::basics::describe_workspace;
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::schema::{boolean, object, string};

pub const TOOLS: &[Tool] = &[NEW, OPEN, SAVE];

const NEW: Tool = Tool {
    name: "project_new",
    title: "Start an empty project",
    description: "Discards the current work (designs, mesh, skeleton, animation) and starts an empty project. \
Unsaved changes are not lost for good: `undo` brings the previous project back. No arguments.",
    input: || object(&[]),
    effect: Effect::Replace,
    run: new,
};

fn new(ctx: &Ctx, _: &Value) -> Result<Output, Fail> {
    let before = ctx.session.before_change()?;
    ctx.call("new_project", json!({}))?;
    ctx.session.replaced("New project", before, None, "{}".into());
    Ok(Output::new("Started an empty project.", json!({ "project": null })))
}

const OPEN: Tool = Tool {
    name: "project_open",
    title: "Open a .pinocchio project",
    description: "Opens a project file saved by Pinocchio (or by `project_save`), replacing the current work. \
`undo` returns to the previous work. Returns what the project contains (tabs with ids, mesh model, skeleton). \
Example: {\"path\": \"/home/me/bracket.pinocchio\"}.",
    input: || object(&[("path", string("Absolute path of the .pinocchio file"), true)]),
    effect: Effect::Replace,
    run: open,
};

fn open(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = absolute(arg_str(args, "path").unwrap_or_default())?;
    if !path.is_file() {
        return Err(Fail::new(format!("{} does not exist", path.display())).hint("pass the absolute path of an existing .pinocchio file"));
    }
    let before = ctx.session.before_change()?;
    let opened = ctx.call("open_project", json!({ "path": path }))?;
    let ui = opened["ui"].as_str().unwrap_or("{}").to_string();
    let label = format!("Open {}", file_name(&path));
    ctx.session.replaced(&label, before, Some(path.clone()), ui);
    let summary = ctx.call("workspace_summary", json!({}))?;
    let cad = ctx.call("cad_status", json!({}))?;
    let mut lines = vec![format!("Opened {}.", path.display())];
    lines.extend(describe_workspace(&summary, &cad));
    Ok(Output::new(lines.join("\n"), json!({ "path": path, "workspace": summary })))
}

const SAVE: Tool = Tool {
    name: "project_save",
    title: "Save the project",
    description: "Writes the project to a .pinocchio file that the Pinocchio app can open. Without `path` it saves to \
the file the project was opened from or last saved to. Writing over a different existing file needs \
`overwrite: true`. Example: {\"path\": \"/home/me/bracket.pinocchio\"}.",
    input: || {
        object(&[
            ("path", string("Absolute path ending in .pinocchio; omit to save to the project's own file"), false),
            ("overwrite", boolean("Allow replacing an existing file that is not this project's own file (default false)"), false),
        ])
    },
    effect: Effect::Write,
    run: save,
};

fn save(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let own = ctx.session.project.lock().unwrap().path.clone();
    let path = match arg_str(args, "path") {
        Some(p) => absolute(p)?,
        None => own.clone().ok_or_else(|| Fail::new("the project has no file yet").hint("pass `path`, e.g. {\"path\": \"/home/me/part.pinocchio\"}"))?,
    };
    if path.extension().and_then(|e| e.to_str()) != Some("pinocchio") {
        return Err(Fail::new(format!("{} does not end in .pinocchio", path.display())).hint("project files use the .pinocchio extension"));
    }
    if path.exists() && own.as_ref() != Some(&path) && arg_bool(args, "overwrite") != Some(true) {
        return Err(Fail::new(format!("{} already exists", path.display())).hint("pass `overwrite: true` to replace it, or choose another path"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.is_dir()) {
        return Err(Fail::new(format!("folder {} does not exist", dir.display())));
    }
    let ui = ctx.session.project.lock().unwrap().ui.clone();
    let saved = ctx.call("save_project", json!({ "path": path, "ui": ui }))?;
    {
        let mut project = ctx.session.project.lock().unwrap();
        project.path = Some(path.clone());
        project.dirty = false;
    }
    let bytes = saved["bytes"].as_u64().unwrap_or(0);
    Ok(Output::new(format!("Saved {} ({}).", path.display(), size(bytes)), json!({ "path": path, "bytes": bytes })))
}

/// Ruta absoluta o un error que lo pide
fn absolute(path: &str) -> Result<PathBuf, Fail> {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        Ok(p)
    } else {
        Err(Fail::new(format!("\"{path}\" is not an absolute path")).hint("use a full path such as /home/me/part.pinocchio"))
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Tamaño legible
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / (1 << 10) as f64),
        b => format!("{b} bytes"),
    }
}
