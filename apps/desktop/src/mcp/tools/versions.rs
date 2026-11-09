//! Versiones con nombre de un diseño (las de la sección «Versiones» del
//! panel): guardar cómo está antes de probar otra idea, ver qué cambió y
//! volver. Las versiones viven dentro del documento de la pestaña.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use cad_model::{Document, NamedVersion};
use serde_json::{json, Value};

use super::cad::{apply, describe_result, num, open_design, tab_prop, Design};
use super::{arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{find_id_or_name, id_or_name, object, one_of, string};

pub const TOOLS: &[Tool] = &[VERSIONS, VERSION];

const VERSIONS: Tool = Tool {
    name: "cad_versions",
    title: "List and compare design versions",
    description: "Lists the named versions of a design tab (snapshots kept inside the design; the user sees them in \
the Versions section) and, with `compare`, what changed since one of them: added and removed volume plus which \
operations and parameters differ. Read only. Example: {\"compare\": \"before thinner walls\"}.",
    input: || object(&[tab_prop(), ("compare", id_or_name("Version number (from the list) or name to compare with the design now"), false)]),
    effect: Effect::Read,
    run: versions,
};

const VERSION: Tool = Tool {
    name: "cad_version",
    title: "Save, restore or delete a design version",
    description: "Versions are branches of a design tab: `save` keeps how the design is now before trying an \
alternative; look at the difference with `cad_versions` {\"compare\": ...}; if the idea did not work, `restore` \
goes back to the version (all versions are kept, and `undo` reverts the restore — save the current state first if \
it may be worth keeping); `delete` removes one. Versions are picked by number (from `cad_versions`) or name. \
Example: {\"action\": \"save\", \"name\": \"before thinner walls\", \"note\": \"2 mm walls\"}.",
    input: || {
        object(&[
            ("action", one_of("What to do", &["save", "restore", "delete"]), true),
            tab_prop(),
            ("version", id_or_name("Version number or name: for restore and delete"), false),
            ("name", string("Name for `save` (default v1, v2…)"), false),
            ("note", string("Note for `save` (optional): what this version is or why it was kept"), false),
        ])
    },
    effect: Effect::Change,
    run: version,
};

fn versions(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let d = open_design(ctx, args)?;
    match args.get("compare") {
        Some(want) => compare(ctx, &d, want),
        None => Ok(list(&d)),
    }
}

fn version(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut d = open_design(ctx, args)?;
    match arg_str(args, "action").unwrap_or_default() {
        "save" => save(ctx, &mut d, args),
        "restore" => restore(ctx, &mut d, args.get("version")),
        "delete" => delete(ctx, &mut d, args.get("version")),
        other => Err(Fail::new(format!("unknown action \"{other}\"")).hint("use save, restore or delete; `cad_versions` lists and compares")),
    }
}

/// La versión pedida (número desde 1 o nombre): (índice, versión)
fn pick<'a>(d: &'a Design, want: Option<&Value>) -> Result<(usize, &'a NamedVersion), Fail> {
    let all = &d.doc.versions;
    if all.is_empty() {
        return Err(Fail::new(format!("tab \"{}\" has no saved versions", d.tab_name)).hint("save one with {\"action\": \"save\"}"));
    }
    let want = want.ok_or_else(|| Fail::new("which version?").hint(format!("versions: {}", names(&d.doc))))?;
    // Los números son los de `list` (desde 1)
    let numbered: Vec<(u64, &str)> = all.iter().enumerate().map(|(i, v)| (i as u64 + 1, v.name.as_str())).collect();
    find_id_or_name(&numbered, want, |v| Some(v.0), |v| v.1)
        .map(|i| (i, &all[i]))
        .ok_or_else(|| Fail::new(format!("no version {want}")).hint(format!("versions: {}", names(&d.doc))))
}

fn names(doc: &Document) -> String {
    doc.versions.iter().enumerate().map(|(i, v)| format!("{} \"{}\"", i + 1, v.name)).collect::<Vec<_>>().join(", ")
}

/// Primer «vN» libre (como `nextVersionName` de la app)
fn next_name(doc: &Document) -> String {
    let mut n = doc.versions.len() + 1;
    while doc.versions.iter().any(|v| v.name == format!("v{n}")) {
        n += 1;
    }
    format!("v{n}")
}

/// El documento sin sus versiones (lo que guarda una versión)
fn snapshot(doc: &Document) -> Document {
    let mut s = doc.clone();
    s.versions.clear();
    s
}

fn save(ctx: &Ctx, d: &mut Design, args: &Value) -> Result<Output, Fail> {
    let name = arg_str(args, "name").map(str::trim).filter(|s| !s.is_empty()).map_or_else(|| next_name(&d.doc), str::to_string);
    if d.doc.versions.iter().any(|v| v.name == name) {
        return Err(Fail::new(format!("there is already a version \"{name}\"")).hint("pick another name, or `delete` the old one first"));
    }
    let note = arg_str(args, "note").map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let document = snapshot(&d.doc);
    let features = document.features.len();
    d.doc.versions.push(NamedVersion { name: name.clone(), created: now_iso(), note, document });
    apply(ctx, d)?;
    let n = d.doc.versions.len();
    Ok(Output::new(
        format!("Saved version {n} \"{name}\" of tab \"{}\" ({features} operations). Try the alternative now; compare with `cad_versions` and `restore` if it does not work.", d.tab_name),
        json!({ "tab": d.tab, "version": n, "name": name }),
    ))
}

fn list(d: &Design) -> Output {
    if d.doc.versions.is_empty() {
        return Output::new(format!("Tab \"{}\" has no saved versions.", d.tab_name), json!({ "tab": d.tab, "versions": [] }));
    }
    let mut text = vec![format!("Versions of tab \"{}\":", d.tab_name)];
    let mut data = Vec::new();
    for (i, v) in d.doc.versions.iter().enumerate() {
        let changes = differences(&v.document, &d.doc);
        let same = changes.is_empty();
        text.push(format!(
            "{}. \"{}\" ({}, {} operations){}{}",
            i + 1,
            v.name,
            v.created.get(..16).unwrap_or(&v.created).replace('T', " "),
            v.document.features.len(),
            v.note.as_deref().map_or(String::new(), |n| format!(" — {n}")),
            if same { " · same as now" } else { "" },
        ));
        data.push(json!({ "version": i + 1, "name": v.name, "created": v.created, "note": v.note, "operations": v.document.features.len(), "same_as_now": same }));
    }
    Output::new(text.join("\n"), json!({ "tab": d.tab, "versions": data }))
}

fn compare(ctx: &Ctx, d: &Design, want: &Value) -> Result<Output, Fail> {
    let (i, v) = pick(d, Some(want))?;
    let mut text = vec![format!("Now vs version {} \"{}\":", i + 1, v.name)];
    let changes = differences(&v.document, &d.doc);
    if changes.is_empty() {
        text.push("- the design is the same as the version.".into());
    }
    text.extend(changes.iter().map(|c| format!("- {c}")));
    // Volumen que hay ahora y no en la versión (agregado) y al revés (quitado)
    let mut data = json!({ "tab": d.tab, "version": i + 1, "name": v.name, "changes": changes });
    match ctx.call_bytes("cad_compare", json!({ "document": v.document })) {
        Ok(bytes) if bytes.len() >= 16 => {
            let f = |k: usize| f64::from_le_bytes(bytes[k..k + 8].try_into().unwrap());
            let (added, removed) = (f(0), f(8));
            text.push(format!("- volume added since the version: {} mm³; removed: {} mm³", num(added), num(removed)));
            data["added_volume"] = json!(added);
            data["removed_volume"] = json!(removed);
        }
        Ok(_) => {}
        Err(e) => text.push(format!("- (volumes not compared: {})", e.message)),
    }
    Ok(Output::new(text.join("\n"), data))
}

fn restore(ctx: &Ctx, d: &mut Design, want: Option<&Value>) -> Result<Output, Fail> {
    let (i, v) = pick(d, want)?;
    let name = v.name.clone();
    let versions = std::mem::take(&mut d.doc.versions);
    d.doc = versions[i].document.clone();
    d.doc.versions = versions;
    let result = apply(ctx, d)?;
    Ok(Output::new(
        format!("Tab \"{}\" is back to version {} \"{name}\" (all versions kept; `undo` reverts this).\n{}", d.tab_name, i + 1, describe_result(&result)),
        json!({ "tab": d.tab, "version": i + 1, "name": name, "body": result["body"] }),
    ))
}

fn delete(ctx: &Ctx, d: &mut Design, want: Option<&Value>) -> Result<Output, Fail> {
    let (i, v) = pick(d, want)?;
    let name = v.name.clone();
    d.doc.versions.remove(i);
    apply(ctx, d)?;
    Ok(Output::new(format!("Deleted version {} \"{name}\" of tab \"{}\".", i + 1, d.tab_name), json!({ "tab": d.tab, "deleted": name })))
}

/// Qué cambió de `then` (la versión) a `now`: operaciones agregadas, quitadas
/// o cambiadas (por id) y parámetros
fn differences(then: &Document, now: &Document) -> Vec<String> {
    let mut out = Vec::new();
    fn as_map(d: &Document) -> BTreeMap<u32, &cad_model::Feature> {
        d.features.iter().map(|f| (f.id.0, f)).collect()
    }
    let (a, b) = (as_map(then), as_map(now));
    for (id, f) in &b {
        match a.get(id) {
            None => out.push(format!("added operation {id} \"{}\"", f.name)),
            Some(old) if serde_json::to_value(old).ok() != serde_json::to_value(f).ok() => {
                let renamed = if old.name != f.name { format!(" (was \"{}\")", old.name) } else { String::new() };
                out.push(format!("changed operation {id} \"{}\"{renamed}", f.name));
            }
            _ => {}
        }
    }
    for (id, f) in &a {
        if !b.contains_key(id) {
            out.push(format!("removed operation {id} \"{}\"", f.name));
        }
    }
    let params = |d: &Document| d.parameters.iter().map(|p| (p.name.clone(), p.expr.clone())).collect::<BTreeMap<_, _>>();
    let (pa, pb) = (params(then), params(now));
    for (name, expr) in &pb {
        match pa.get(name) {
            None => out.push(format!("added parameter {name} = {expr}")),
            Some(old) if old != expr => out.push(format!("parameter {name}: {old} → {expr}")),
            _ => {}
        }
    }
    for name in pa.keys().filter(|n| !pb.contains_key(*n)) {
        out.push(format!("removed parameter {name}"));
    }
    if then.bindings != now.bindings {
        out.push("formulas changed".into());
    }
    // Lo demás (configuraciones, planos, ensamble…) se compara entero
    let rest = |d: &Document| {
        let mut v = serde_json::to_value(snapshot(d)).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            for k in ["features", "parameters", "bindings"] {
                o.remove(k);
            }
        }
        v
    };
    if out.is_empty() && rest(then) != rest(now) {
        out.push("other settings changed (configurations, drawings or assembly)".into());
    }
    out
}

/// Fecha y hora UTC en ISO 8601 (como `new Date().toISOString()` de la app)
fn now_iso() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let (days, rest) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Días desde 1970 → fecha civil (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, milli) = (rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_date_has_the_shape_of_the_app() {
        let s = now_iso();
        assert_eq!(s.len(), 24, "{s}");
        assert!(s.starts_with("20") && s.ends_with('Z') && &s[10..11] == "T", "{s}");
    }
}
