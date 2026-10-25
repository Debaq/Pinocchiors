//! Planos 2D: una lámina con las vistas normalizadas alineadas (líneas
//! ocultas exactas del núcleo CAD), cotas generales, cajetín y, en un
//! ensamble, lista de materiales con globos. Sale en PDF o SVG, con una vista
//! previa PNG para que la IA la mire.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::assembly::bom;
use super::cad::{find_tab, num};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, id_or_name, object, one_of, string};

pub const TOOLS: &[Tool] = &[EXPORT];

const EXPORT: Tool = Tool {
    name: "drawing_export",
    title: "Make a 2D drawing (PDF/SVG)",
    description: "Makes an engineering drawing sheet of a design or assembly tab and writes it as PDF or SVG: the \
standard views aligned (front, top, side and an isometric), exact visible and dashed hidden lines, overall \
dimensions, a standard scale chosen to fit, a title block with the projection symbol, and for assemblies a bill of \
materials with numbered balloons. Projection `first` (ISO, default) or `third` (ASME). Returns a PNG preview of the \
sheet so you can check it. Example: {\"path\": \"/home/me/bracket.pdf\", \"title\": \"Bracket\"}.",
    input: || {
        object(&[
            ("tab", id_or_name("Design or assembly tab (default: the active tab)"), false),
            ("path", string("Absolute path ending in .pdf or .svg"), true),
            ("title", string("Title in the title block (default: the tab name)"), false),
            ("author", string("\"Drawn by\" in the title block"), false),
            ("views", array("Which views (default all four)", one_of("view", &["front", "top", "side", "iso"])), false),
            ("projection", one_of("first = ISO/European (default), third = ASME/American", &["first", "third"]), false),
            ("sheet", one_of("Sheet size, landscape (default A4)", &["A4", "A3", "A2"]), false),
            ("scale", string("Force a scale like \"1:2\" or \"2:1\" (default: the largest standard scale that fits)"), false),
            ("hidden_lines", boolean("Draw hidden edges dashed (default true)"), false),
            ("dimensions", boolean("Overall dimensions (default true)"), false),
            ("overwrite", boolean("Replace the file if it exists (default false)"), false),
        ])
    },
    effect: Effect::Write,
    run: export,
};

type P3 = [f64; 3];

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Desde dónde se mira (hacia quien mira) y la derecha de la hoja; el arriba
/// de la hoja es eye × xdir (como en la app)
fn spec(view: &str, first: bool) -> (P3, P3) {
    let (s, t) = (std::f64::consts::FRAC_1_SQRT_2, 1.0 / 3f64.sqrt());
    match view {
        "front" => ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        "top" => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        // Primer ángulo: la vista desde la izquierda va a la derecha del frente
        "side" if first => ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        "side" => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        _ => ([t, -t, t], [s, s, 0.0]),
    }
}

/// Celda (columna, fila) de cada vista según la proyección
fn cell(view: &str, first: bool) -> (usize, usize) {
    match (view, first) {
        ("front", true) => (0, 0),
        ("side", true) => (1, 0),
        ("top", true) => (0, 1),
        ("iso", true) => (1, 1),
        ("top", false) => (0, 0),
        ("iso", false) => (1, 0),
        ("front", false) => (0, 1),
        _ => (1, 1),
    }
}

const SCALES: &[(f64, &str)] = &[
    (10.0, "10:1"), (5.0, "5:1"), (2.0, "2:1"), (1.0, "1:1"), (0.5, "1:2"), (0.2, "1:5"), (0.1, "1:10"),
    (0.05, "1:20"), (0.02, "1:50"), (0.01, "1:100"), (0.005, "1:200"),
];

fn parse_scale(s: &str) -> Option<f64> {
    let (a, b) = s.split_once(':')?;
    let (a, b): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0.0 && b > 0.0).then(|| a / b)
}

/// Fecha de hoy (UTC) como AAAA-MM-DD, sin depender de otra biblioteca
fn today() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Hinnant: días desde 1970 → fecha civil
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

struct Placed {
    name: String,
    eye: P3,
    xdir: P3,
    /// Caja de la vista en coordenadas de la vista (mm del modelo)
    lo: [f64; 2],
    hi: [f64; 2],
    /// Centro en la hoja (mm)
    at: [f64; 2],
}

impl Placed {
    fn up(&self) -> P3 {
        cross(self.eye, self.xdir)
    }
    /// Punto de la vista → hoja
    fn sheet(&self, p: [f64; 2], s: f64) -> [f64; 2] {
        let mid = [(self.lo[0] + self.hi[0]) / 2.0, (self.lo[1] + self.hi[1]) / 2.0];
        [self.at[0] + (p[0] - mid[0]) * s, self.at[1] - (p[1] - mid[1]) * s]
    }
    /// Punto del modelo → vista
    fn project(&self, p: P3) -> [f64; 2] {
        [dot(p, self.xdir), dot(p, self.up())]
    }
}

fn export(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = PathBuf::from(arg_str(args, "path").unwrap_or_default());
    if !path.is_absolute() {
        return Err(Fail::new("`path` must be absolute").hint("e.g. /home/me/part.pdf"));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "pdf" && ext != "svg" {
        return Err(Fail::new("the drawing is written as .pdf or .svg").hint("end `path` in .pdf or .svg"));
    }
    if path.exists() && arg_bool(args, "overwrite") != Some(true) {
        return Err(Fail::new(format!("{} already exists", path.display())).hint("pass `overwrite: true` to replace it"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.is_dir()) {
        return Err(Fail::new(format!("folder {} does not exist", dir.display())));
    }
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    super::cad::focus(ctx, tab)?;
    let assembly = kind == "assembly";
    let info = ctx.call("cad_drawing_info", json!({})).map_err(|e| e.hint("the tab needs a solid (or, for an assembly, instances) first"))?;
    let (lo3, hi3): (P3, P3) = (serde_json::from_value(info["min"].clone()).unwrap_or_default(), serde_json::from_value(info["max"].clone()).unwrap_or_default());
    let first = arg_str(args, "projection").unwrap_or("first") == "first";
    let wanted: Vec<String> = match args.get("views").and_then(Value::as_array) {
        Some(v) if !v.is_empty() => v.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => ["front", "top", "side", "iso"].map(String::from).to_vec(),
    };

    // Caja de cada vista: las 8 esquinas de la caja del modelo proyectadas
    let corners: Vec<P3> = (0..8).map(|k| [if k & 1 == 0 { lo3[0] } else { hi3[0] }, if k & 2 == 0 { lo3[1] } else { hi3[1] }, if k & 4 == 0 { lo3[2] } else { hi3[2] }]).collect();
    let mut views: Vec<Placed> = wanted
        .iter()
        .map(|name| {
            let (eye, xdir) = spec(name, first);
            let mut v = Placed { name: name.clone(), eye, xdir, lo: [f64::MAX; 2], hi: [f64::MIN; 2], at: [0.0; 2] };
            for c in &corners {
                let p = v.project(*c);
                v.lo = [v.lo[0].min(p[0]), v.lo[1].min(p[1])];
                v.hi = [v.hi[0].max(p[0]), v.hi[1].max(p[1])];
            }
            v
        })
        .collect();

    // Hoja y escala
    let sheet = arg_str(args, "sheet").unwrap_or("A4");
    let (sw, sh) = match sheet {
        "A2" => (594.0, 420.0),
        "A3" => (420.0, 297.0),
        _ => (297.0, 210.0),
    };
    let (margin, gap, title_h) = (10.0, 18.0, 30.0);
    let cols = views.iter().map(|v| cell(&v.name, first).0).max().unwrap_or(0) + 1;
    let rows = views.iter().map(|v| cell(&v.name, first).1).max().unwrap_or(0) + 1;
    let col_w: Vec<f64> = (0..cols).map(|c| views.iter().filter(|v| cell(&v.name, first).0 == c).map(|v| v.hi[0] - v.lo[0]).fold(0.0, f64::max)).collect();
    let row_h: Vec<f64> = (0..rows).map(|r| views.iter().filter(|v| cell(&v.name, first).1 == r).map(|v| v.hi[1] - v.lo[1]).fold(0.0, f64::max)).collect();
    // Entre vistas, `gap`; alrededor, lugar para las cotas
    let around = 12.0;
    let avail_w = sw - 2.0 * margin - gap * (cols as f64 - 1.0) - 2.0 * around;
    let avail_h = sh - 2.0 * margin - title_h - gap * (rows as f64 - 1.0) - 2.0 * around;
    let fit = (avail_w / col_w.iter().sum::<f64>().max(1e-6)).min(avail_h / row_h.iter().sum::<f64>().max(1e-6));
    let (scale, scale_name) = match arg_str(args, "scale") {
        Some(s) => (parse_scale(s).ok_or_else(|| Fail::new(format!("\"{s}\" is not a scale")).hint("write it like \"1:2\" or \"2:1\""))?, s.to_string()),
        None => SCALES.iter().find(|(s, _)| *s <= fit).map(|(s, n)| (*s, n.to_string())).unwrap_or((0.005, "1:200".into())),
    };
    // Centros de las celdas
    let used_w: f64 = col_w.iter().map(|w| w * scale).sum::<f64>() + gap * (cols as f64 - 1.0);
    let used_h: f64 = row_h.iter().map(|h| h * scale).sum::<f64>() + gap * (rows as f64 - 1.0);
    let x0 = margin + (sw - 2.0 * margin - used_w) / 2.0;
    let y0 = margin + (sh - 2.0 * margin - title_h - used_h) / 2.0;
    for v in &mut views {
        let (c, r) = cell(&v.name, first);
        let x = x0 + col_w[..c].iter().map(|w| w * scale + gap).sum::<f64>() + col_w[c] * scale / 2.0;
        let y = y0 + row_h[..r].iter().map(|h| h * scale + gap).sum::<f64>() + row_h[r] * scale / 2.0;
        v.at = [x, y];
    }

    // Líneas de cada vista (líneas ocultas exactas)
    let specs: Vec<Value> = views.iter().map(|v| json!({ "eye": v.eye, "xdir": v.xdir })).collect();
    let drawn = ctx.call("cad_drawing", json!({ "views": specs }))?;
    let hidden = arg_bool(args, "hidden_lines") != Some(false);

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{sw}mm" height="{sh}mm" viewBox="0 0 {sw} {sh}" font-family="DejaVu Sans, Arial, sans-serif"><rect x="0" y="0" width="{sw}" height="{sh}" fill="white"/><rect x="{m}" y="{m}" width="{iw}" height="{ih}" fill="none" stroke="black" stroke-width="0.5"/>"##,
        m = margin,
        iw = sw - 2.0 * margin,
        ih = sh - 2.0 * margin
    );
    for (v, res) in views.iter().zip(drawn.as_array().into_iter().flatten()) {
        let mut paths = [String::new(), String::new(), String::new()];
        for line in res["lines"].as_array().into_iter().flatten() {
            let k = match line["kind"].as_str() {
                Some("visible" | "outline") => 0,
                Some("hidden" | "hidden_outline") if hidden => 1,
                Some("smooth") => 2,
                _ => continue,
            };
            let pts: Vec<[f64; 2]> = line["points"].as_array().into_iter().flatten().filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?])).collect();
            for (i, p) in pts.iter().enumerate() {
                let q = v.sheet(*p, scale);
                let _ = write!(paths[k], "{}{:.3},{:.3} ", if i == 0 { "M" } else { "L" }, q[0], q[1]);
            }
        }
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="#999" stroke-width="0.18"/><path d="{}" fill="none" stroke="#444" stroke-width="0.25" stroke-dasharray="1.6 0.9"/><path d="{}" fill="none" stroke="black" stroke-width="0.4" stroke-linecap="round" stroke-linejoin="round"/>"##,
            paths[2], paths[1], paths[0]
        );
    }

    // Cotas generales: ancho y alto en el frente, fondo en la planta o el lateral
    let mut dims = Vec::new();
    if arg_bool(args, "dimensions") != Some(false) {
        let size = [hi3[0] - lo3[0], hi3[1] - lo3[1], hi3[2] - lo3[2]];
        if let Some(f) = views.iter().find(|v| v.name == "front") {
            dim_h(&mut svg, f.sheet([f.lo[0], f.lo[1]], scale), f.sheet([f.hi[0], f.lo[1]], scale), 8.0, size[0]);
            dim_v(&mut svg, f.sheet([f.lo[0], f.lo[1]], scale), f.sheet([f.lo[0], f.hi[1]], scale), -8.0, size[2]);
            dims.push(format!("width {} mm", num(size[0])));
            dims.push(format!("height {} mm", num(size[2])));
        }
        if let Some(t) = views.iter().find(|v| v.name == "top") {
            dim_v(&mut svg, t.sheet([t.hi[0], t.lo[1]], scale), t.sheet([t.hi[0], t.hi[1]], scale), 8.0, size[1]);
            dims.push(format!("depth {} mm", num(size[1])));
        } else if let Some(s) = views.iter().find(|v| v.name == "side") {
            dim_h(&mut svg, s.sheet([s.lo[0], s.lo[1]], scale), s.sheet([s.hi[0], s.lo[1]], scale), 8.0, size[1]);
            dims.push(format!("depth {} mm", num(size[1])));
        }
    }

    // Ensamble: lista de materiales y globos en la iso
    let mut rows_out = Vec::new();
    if assembly {
        let doc: cad_model::Document = serde_json::from_value(ctx.call("cad_get_document", json!({}))?).map_err(|e| Fail::new(e.to_string()))?;
        let asm = doc.assembly.unwrap_or_default();
        let rows = bom(ctx, &asm);
        // Número de ítem de cada instancia (por su pieza)
        let item_of = |inst: &cad_model::Instance| rows.iter().position(|r| r.holds(inst));
        let (bx, row_h, w) = (sw - margin - 110.0, 6.0, [12.0, 82.0, 16.0]);
        let by = sh - margin - title_h - row_h * (rows.len() as f64 + 1.0);
        let _ = write!(svg, r##"<g font-size="3" stroke="black" stroke-width="0.25">"##);
        let header = ["ITEM", "PART", "QTY"];
        for (r, cells) in std::iter::once(header.map(String::from).to_vec()).chain(rows.iter().enumerate().map(|(k, r)| vec![(k + 1).to_string(), format!("{}{}", r.name, r.configuration.as_ref().map_or(String::new(), |c| format!(" ({c})"))), r.quantity.to_string()])).enumerate() {
            let y = by + r as f64 * row_h;
            let mut x = bx;
            for (k, text) in cells.iter().enumerate() {
                let _ = write!(svg, r##"<rect x="{x:.2}" y="{y:.2}" width="{}" height="{row_h}" fill="{}"/><text x="{:.2}" y="{:.2}" stroke="none">{}</text>"##, w[k], if r == 0 { "#eee" } else { "white" }, x + 1.5, y + 4.2, esc(text));
                x += w[k];
            }
        }
        let _ = write!(svg, "</g>");
        rows_out = rows.iter().map(|r| r.json()).collect();
        if let Some(iso) = views.iter().find(|v| v.name == "iso") {
            // Un globo por ítem, en la primera instancia de esa pieza
            let mut marks: Vec<(usize, [f64; 2])> = Vec::new();
            for item in info["items"].as_array().into_iter().flatten() {
                let id = item["instance"].as_u64().unwrap_or(0) as u32;
                let Some(inst) = asm.instances.iter().find(|i| i.id == id) else { continue };
                let Some(k) = item_of(inst) else { continue };
                if marks.iter().any(|m| m.0 == k) {
                    continue;
                }
                let c: P3 = serde_json::from_value(item["center"].clone()).unwrap_or_default();
                marks.push((k, iso.sheet(iso.project(c), scale)));
            }
            // Ángulo de cada globo: hacia donde está su pieza desde el centro de la
            // vista (o repartidos si está en el centro), separados al menos 35°
            let mut angles: Vec<f64> = marks
                .iter()
                .enumerate()
                .map(|(n, (_, p))| {
                    let (dx, dy) = (p[0] - iso.at[0], p[1] - iso.at[1]);
                    if dx.hypot(dy) > 2.0 { dy.atan2(dx) } else { -2.4 + n as f64 * 0.9 }
                })
                .collect();
            let min = 35f64.to_radians();
            for _ in 0..20 {
                let mut order: Vec<usize> = (0..angles.len()).collect();
                order.sort_by(|a, b| angles[*a].total_cmp(&angles[*b]));
                for w in order.windows(2) {
                    let d = angles[w[1]] - angles[w[0]];
                    if d < min {
                        angles[w[0]] -= (min - d) / 2.0;
                        angles[w[1]] += (min - d) / 2.0;
                    }
                }
            }
            let reach = (iso.hi[0] - iso.lo[0]).max(iso.hi[1] - iso.lo[1]) * scale / 2.0 + 9.0;
            for ((k, p), a) in marks.iter().zip(&angles) {
                let b = [iso.at[0] + a.cos() * reach, iso.at[1] + a.sin() * reach];
                let _ = write!(
                    svg,
                    r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="0.25"/><circle cx="{:.2}" cy="{:.2}" r="0.7"/><circle cx="{:.2}" cy="{:.2}" r="4" fill="white" stroke="black" stroke-width="0.3"/><text x="{:.2}" y="{:.2}" font-size="3.5" text-anchor="middle">{}</text>"##,
                    p[0], p[1], b[0], b[1], p[0], p[1], b[0], b[1], b[0], b[1] + 1.25, k + 1
                );
            }
        }
    }

    // Cajetín
    let title = arg_str(args, "title").unwrap_or(&tab_name).to_string();
    let (tw, tx, ty) = (180.0, sw - margin - 180.0, sh - margin - title_h);
    let _ = write!(
        svg,
        r##"<g stroke="black" stroke-width="0.35" fill="none"><rect x="{tx}" y="{ty}" width="{tw}" height="{title_h}"/><line x1="{tx}" y1="{a}" x2="{x2}" y2="{a}"/><line x1="{c1}" y1="{a}" x2="{c1}" y2="{b}"/><line x1="{c2}" y1="{a}" x2="{c2}" y2="{b}"/><line x1="{c3}" y1="{a}" x2="{c3}" y2="{b}"/></g>"##,
        a = ty + 14.0,
        b = ty + title_h,
        x2 = tx + tw,
        c1 = tx + 50.0,
        c2 = tx + 85.0,
        c3 = tx + 135.0
    );
    let label = |svg: &mut String, x: f64, y: f64, caption: &str, value: &str| {
        let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="2.2" fill="#555">{}</text><text x="{:.2}" y="{:.2}" font-size="3.5">{}</text>"##, x + 1.5, y + 3.2, esc(caption), x + 1.5, y + 9.5, esc(value));
    };
    let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="6" font-weight="bold">{}</text><text x="{:.2}" y="{:.2}" font-size="2.6" fill="#555">{}</text>"##, tx + 3.0, ty + 8.0, esc(&title), tx + 3.0, ty + 12.0, esc(&format!("{} · tab \"{tab_name}\" · Pinocchio", if assembly { "Assembly" } else { "Part" })));
    label(&mut svg, tx, ty + 14.0, "DRAWN BY", arg_str(args, "author").unwrap_or("-"));
    label(&mut svg, tx + 50.0, ty + 14.0, "SCALE", &scale_name);
    label(&mut svg, tx + 85.0, ty + 14.0, "DATE · UNITS", &format!("{} · mm", today()));
    label(&mut svg, tx + 135.0, ty + 14.0, "PROJECTION · SHEET", &format!("{} · {sheet}", if first { "1st angle" } else { "3rd angle" }));
    projection_symbol(&mut svg, tx + 157.0, ty + 4.0, first);
    svg.push_str("</svg>");

    // Escribir y la vista previa
    let bytes = if ext == "pdf" {
        ctx.call("cad_write_pdf", json!({ "path": path, "svg": svg }))?.as_u64().unwrap_or(0)
    } else {
        std::fs::write(&path, &svg).map_err(|e| Fail::new(format!("could not write {}: {e}", path.display())))?;
        svg.len() as u64
    };
    let preview = rasterize(&svg, 1100).map_err(Fail::new)?;
    let mut lines = vec![format!(
        "Wrote the drawing of {} tab \"{tab_name}\" (id {tab}) to {} ({}, {bytes} bytes): views {}, scale {scale_name}, sheet {sheet} landscape, {} projection.",
        if assembly { "assembly" } else { "design" },
        path.display(),
        ext.to_uppercase(),
        wanted.join(", "),
        if first { "first-angle (ISO)" } else { "third-angle (ASME)" }
    )];
    if !dims.is_empty() {
        lines.push(format!("Overall dimensions: {}.", dims.join(", ")));
    }
    if !rows_out.is_empty() {
        lines.push(format!("Bill of materials with {} item(s) and balloons on the iso view.", rows_out.len()));
    }
    lines.push("The image is a preview of the sheet.".into());
    let mut out = Output::new(lines.join("\n"), json!({ "path": path, "bytes": bytes, "scale": scale_name, "sheet": sheet, "views": wanted, "bom": rows_out }));
    out.images.push(preview);
    Ok(out)
}

/// Flecha de cota en (x, y) apuntando hacia (dx, dy)
fn arrow(svg: &mut String, x: f64, y: f64, dx: f64, dy: f64) {
    let (l, w) = (2.5, 0.8);
    let (px, py) = (-dy, dx);
    let _ = write!(svg, r##"<path d="M{x:.2},{y:.2} L{:.2},{:.2} L{:.2},{:.2} Z" fill="black"/>"##, x - dx * l + px * w, y - dy * l + py * w, x - dx * l - px * w, y - dy * l - py * w);
}

/// Cota horizontal debajo de a–b (a la distancia `off`)
fn dim_h(svg: &mut String, a: [f64; 2], b: [f64; 2], off: f64, value: f64) {
    let y = a[1].max(b[1]) + off;
    let _ = write!(
        svg,
        r##"<g stroke="black" stroke-width="0.18"><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/><line x1="{:.2}" y1="{y:.2}" x2="{:.2}" y2="{y:.2}"/></g>"##,
        a[0], a[1] + 1.0, a[0], y + 1.5, b[0], b[1] + 1.0, b[0], y + 1.5, a[0], b[0]
    );
    arrow(svg, a[0], y, -1.0, 0.0);
    arrow(svg, b[0], y, 1.0, 0.0);
    let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="3.5" text-anchor="middle">{}</text>"##, (a[0] + b[0]) / 2.0, y - 1.0, num(value));
}

/// Cota vertical a la izquierda (off < 0) o derecha (off > 0) de a–b
fn dim_v(svg: &mut String, a: [f64; 2], b: [f64; 2], off: f64, value: f64) {
    let x = if off < 0.0 { a[0].min(b[0]) + off } else { a[0].max(b[0]) + off };
    let ext: f64 = if off < 0.0 { -1.5 } else { 1.5 };
    let _ = write!(
        svg,
        r##"<g stroke="black" stroke-width="0.18"><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/><line x1="{x:.2}" y1="{:.2}" x2="{x:.2}" y2="{:.2}"/></g>"##,
        a[0] + ext.signum() * 1.0, a[1], x + ext, a[1], b[0] + ext.signum() * 1.0, b[1], x + ext, b[1], a[1], b[1]
    );
    arrow(svg, x, a[1], 0.0, 1.0);
    arrow(svg, x, b[1], 0.0, -1.0);
    let (tx, ty) = (x - 1.0, (a[1] + b[1]) / 2.0);
    let _ = write!(svg, r##"<text x="{tx:.2}" y="{ty:.2}" font-size="3.5" text-anchor="middle" transform="rotate(-90 {tx:.2} {ty:.2})">{}</text>"##, num(value));
}

/// Símbolo de proyección (cono truncado y sus círculos)
fn projection_symbol(svg: &mut String, x: f64, y: f64, first: bool) {
    let (cone_x, circ_x) = if first { (x, x + 11.0) } else { (x + 6.0, x) };
    let _ = write!(
        svg,
        r##"<g stroke="black" stroke-width="0.25" fill="none"><path d="M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} Z"/><circle cx="{:.2}" cy="{:.2}" r="2.5"/><circle cx="{:.2}" cy="{:.2}" r="1.2"/></g>"##,
        cone_x, y + 1.0, cone_x + 4.5, y + 2.2, cone_x + 4.5, y + 3.8, cone_x, y + 5.0,
        circ_x + 2.5, y + 3.0, circ_x + 2.5, y + 3.0
    );
}

/// SVG → PNG de `width` píxeles de ancho
fn rasterize(svg: &str, width: u32) -> Result<Vec<u8>, String> {
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| format!("invalid SVG: {e}"))?;
    let size = tree.size();
    let k = width as f32 / size.width();
    let height = (size.height() * k).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("preview too large")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| e.to_string())
}
