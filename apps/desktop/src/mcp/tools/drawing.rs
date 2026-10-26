//! Planos 2D: una lámina con las vistas normalizadas alineadas (líneas
//! ocultas exactas del núcleo CAD), corte A-A, cortes por una línea, detalles
//! ampliados, cotas generales y puntuales, cajetín y, en un ensamble, lista de
//! materiales con globos. Sale en PDF o SVG, con una vista previa PNG para que
//! la IA la mire.
//!
//! Las láminas de la app viven en el documento de la pestaña (`drawing.sheets`,
//! las arma la interfaz: `lib/drawingDoc.ts`). `drawing_sheet` las crea y las
//! cambia con los mismos datos (cotas por puntos de la vista, cortes por su
//! línea, detalles por su círculo), así el usuario las abre en «Planos 2D»;
//! `drawing_export` también dibuja una de ellas.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::assembly::bom;
use super::cad::{find_tab, num};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, find_id_or_name, id_or_name, object, one_of, string, Prop};

pub const TOOLS: &[Tool] = &[EXPORT, SHEET];

type P2 = [f64; 2];
type P3 = [f64; 3];

const VIEW_NAMES: &[&str] = &["front", "top", "bottom", "left", "right", "back", "iso"];

// ─── Lo que se pide ─────────────────────────────────────────────────────────

/// Una lámina: lo que la app guarda en `drawing.sheets[i]` y lo que dibuja el conector
#[derive(Clone, Debug)]
struct Spec {
    name: String,
    sheet: String,
    first: bool,
    scale: Option<f64>,
    hidden_lines: bool,
    overall: bool,
    /// Vistas normalizadas a la vista
    views: Vec<String>,
    title: String,
    author: String,
    /// Corte A-A del frente: `Some(None)` por el medio
    section: Option<Option<f64>>,
    cuts: Vec<Cut>,
    details: Vec<Detail>,
    dims: Vec<Dim>,
    bom: bool,
}

/// Corte por una línea dibujada en una vista (puntos en coordenadas de esa vista)
#[derive(Clone, Debug)]
struct Cut {
    label: String,
    view: String,
    points: Vec<P2>,
    flip: bool,
}

#[derive(Clone, Debug)]
struct Detail {
    label: String,
    view: String,
    center: P2,
    radius: f64,
    scale: Option<f64>,
}

/// Cota a mano: la vista, el tipo y puntos sobre las líneas (coordenadas de la vista)
#[derive(Clone, Debug)]
struct Dim {
    view: String,
    kind: String,
    refs: Vec<P2>,
}

impl Spec {
    fn new(name: &str, title: &str) -> Self {
        Spec {
            name: name.into(),
            sheet: "A4".into(),
            first: true,
            scale: None,
            hidden_lines: true,
            overall: true,
            views: vec!["front".into(), "top".into(), "left".into(), "iso".into()],
            title: title.into(),
            author: String::new(),
            section: None,
            cuts: Vec::new(),
            details: Vec::new(),
            dims: Vec::new(),
            bom: true,
        }
    }

    /// Una lámina de la app (lo que entiende el conector; lo demás se ignora al dibujar)
    fn from_app(s: &Value) -> Self {
        let f = |k: &str| s[k].as_f64();
        let first = s["projection"] != "third";
        let hidden: Vec<&str> = s["hiddenViews"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let pts = |v: &Value| -> Vec<P2> { v.as_array().into_iter().flatten().filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?])).collect() };
        Spec {
            name: s["name"].as_str().unwrap_or("Plano").into(),
            sheet: s["sheet"].as_str().unwrap_or("A4").into(),
            first,
            scale: f("scale"),
            hidden_lines: s["hiddenLines"] != false,
            overall: s["overall"] != false,
            views: VIEW_NAMES.iter().filter(|v| !hidden.contains(v)).map(|v| v.to_string()).collect(),
            title: s["title"].as_str().unwrap_or("").into(),
            author: s["author"].as_str().unwrap_or("").into(),
            section: s["section"].as_object().filter(|o| o.get("hidden") != Some(&json!(true))).map(|o| o.get("y").and_then(Value::as_f64)),
            cuts: s["sections"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["hidden"] != true)
                .map(|c| Cut { label: c["label"].as_str().unwrap_or("B").into(), view: c["view"].as_str().unwrap_or("front").into(), points: pts(&c["points"]), flip: c["flip"] == true })
                .collect(),
            details: s["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["hidden"] != true)
                .map(|d| Detail {
                    label: d["label"].as_str().unwrap_or("C").into(),
                    view: d["view"].as_str().unwrap_or("front").into(),
                    center: [d["center"][0].as_f64().unwrap_or(0.0), d["center"][1].as_f64().unwrap_or(0.0)],
                    radius: d["radius"].as_f64().unwrap_or(5.0),
                    scale: d["scale"].as_f64(),
                })
                .collect(),
            dims: s["dims"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["hidden"] != true)
                .map(|d| Dim { view: d["view"].as_str().unwrap_or("front").into(), kind: d["kind"].as_str().unwrap_or("length").into(), refs: pts(&d["refs"]) })
                .collect(),
            bom: s["bom"] != false,
        }
    }

    /// Escribe la lámina sobre `base` (la de la app, si ya existía: lo que el
    /// conector no maneja queda como estaba)
    fn to_app(&self, id: u64, base: Option<&Value>) -> Value {
        let mut s = base.cloned().unwrap_or_else(|| {
            json!({ "id": id, "smooth": false, "sixViews": true, "dims": [], "details": [], "sketch": null })
        });
        let hidden: Vec<&str> = VIEW_NAMES.iter().copied().filter(|v| !self.views.iter().any(|w| w == v)).collect();
        s["id"] = json!(id);
        s["name"] = json!(self.name);
        s["sheet"] = json!(self.sheet);
        s["projection"] = json!(if self.first { "first" } else { "third" });
        s["scale"] = json!(self.scale);
        s["hiddenLines"] = json!(self.hidden_lines);
        s["overall"] = json!(self.overall);
        s["hiddenViews"] = json!(hidden);
        s["sixViews"] = json!(true);
        s["title"] = json!(self.title);
        s["author"] = json!(self.author);
        s["section"] = match self.section {
            Some(y) => json!({ "y": y }),
            // Uno oculto en la app sigue ahí
            None if base.is_some_and(|b| b["section"]["hidden"] == true) => base.unwrap()["section"].clone(),
            None => Value::Null,
        };
        s["sections"] = json!(self.cuts.iter().map(|c| json!({ "label": c.label, "view": c.view, "points": c.points, "flip": c.flip })).collect::<Vec<_>>());
        s["details"] = json!(self.details.iter().map(|d| json!({ "label": d.label, "view": d.view, "center": d.center, "radius": d.radius, "scale": d.scale })).collect::<Vec<_>>());
        s["dims"] = json!(self.dims.iter().map(|d| json!({ "view": d.view, "kind": d.kind, "refs": d.refs })).collect::<Vec<_>>());
        s["bom"] = json!(self.bom);
        s
    }

    /// Letra libre para un corte o un detalle (la A es del corte A-A)
    fn next_label(&self) -> String {
        let used: Vec<&str> = self.details.iter().map(|d| d.label.as_str()).chain(self.cuts.iter().map(|c| c.label.as_str())).collect();
        "BCDEFGHJKLMNPQRSTUVWXYZ".chars().map(|c| c.to_string()).find(|l| !used.contains(&l.as_str())).unwrap_or_else(|| format!("{}", used.len() + 1))
    }
}

// ─── Geometría de las vistas ────────────────────────────────────────────────

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mul(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn unit(a: P3) -> P3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { mul(a, 1.0 / l) } else { a }
}

fn clean(a: P3) -> P3 {
    a.map(|x| if x.abs() < 1e-12 { 0.0 } else { x })
}

/// Desde dónde se mira (hacia quien mira) y la derecha de la hoja; el arriba
/// es eye × xdir (como `viewSpec` de la app)
#[derive(Clone, Copy, Debug)]
struct Dir {
    eye: P3,
    xdir: P3,
}

impl Dir {
    fn standard(name: &str) -> Option<Dir> {
        let (s, t) = (std::f64::consts::FRAC_1_SQRT_2, 1.0 / 3f64.sqrt());
        let (eye, xdir) = match name {
            "front" => ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
            "back" => ([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]),
            "top" => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            "bottom" => ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
            "left" => ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            "right" => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            "iso" => ([t, -t, t], [s, s, 0.0]),
            _ => return None,
        };
        Some(Dir { eye, xdir })
    }

    fn up(&self) -> P3 {
        unit(cross(self.eye, self.xdir))
    }

    fn project(&self, p: P3) -> P2 {
        [dot(p, self.xdir), dot(p, self.up())]
    }

    /// Punto de la vista (y una profundidad hacia quien mira) → modelo
    fn world(&self, p: P2, depth: f64) -> P3 {
        add(add(mul(self.xdir, p[0]), mul(self.up(), p[1])), mul(self.eye, depth))
    }
}

/// Hacia dónde miran las flechas de un corte: a la izquierda del primer tramo
fn cut_arrow(c: &Cut) -> P2 {
    let (a, b) = (c.points[0], c.points[1]);
    let t = [b[0] - a[0], b[1] - a[1]];
    let l = t[0].hypot(t[1]).max(1e-12);
    let n = [-t[1] / l, t[0] / l];
    if c.flip { [-n[0], -n[1]] } else { n }
}

/// Lo que quita un corte (como `sectionPrism` de la app)
fn cut_prism(c: &Cut, parent: Dir, reach: f64) -> Value {
    let pts = &c.points;
    let a = cut_arrow(c);
    let dir2 = |p: P2, q: P2| {
        let l = (q[0] - p[0]).hypot(q[1] - p[1]).max(1e-12);
        [(q[0] - p[0]) / l, (q[1] - p[1]) / l]
    };
    let n = pts.len();
    let (t0, tn) = (dir2(pts[0], pts[1]), dir2(pts[n - 2], pts[n - 1]));
    let first = [pts[0][0] - t0[0] * reach, pts[0][1] - t0[1] * reach];
    let last = [pts[n - 1][0] + tn[0] * reach, pts[n - 1][1] + tn[1] * reach];
    let back = |p: P2| [p[0] - a[0] * reach, p[1] - a[1] * reach];
    let mut poly = vec![first];
    poly.extend_from_slice(&pts[1..n - 1]);
    poly.extend([last, back(last), back(first)]);
    json!({ "points": poly.iter().map(|p| parent.world(*p, -reach)).collect::<Vec<_>>(), "dir": mul(parent.eye, 2.0 * reach) })
}

/// Desde dónde se mira la vista de un corte (como `sectionViewDir`)
fn cut_dir(c: &Cut, parent: Dir) -> Dir {
    let a2 = cut_arrow(c);
    let up = parent.up();
    let a = unit(add(mul(parent.xdir, a2[0]), mul(up, a2[1])));
    let eye = mul(a, -1.0);
    let u = if dot(up, a).abs() < 0.5 { up } else { parent.eye };
    let u = unit(add(u, mul(a, -dot(u, a))));
    Dir { eye: clean(eye), xdir: clean(unit(cross(u, eye))) }
}

fn cut_view_name(label: &str) -> String {
    format!("corte-{label}")
}

/// La dirección de una vista de la lámina (normalizada o de un corte)
fn dir_of(spec: &Spec, name: &str) -> Option<Dir> {
    if let Some(d) = Dir::standard(name) {
        return Some(d);
    }
    let c = spec.cuts.iter().find(|c| cut_view_name(&c.label) == name)?;
    Some(cut_dir(c, Dir::standard(&c.view)?))
}

// ─── Líneas: qué son ────────────────────────────────────────────────────────

#[derive(Clone)]
struct Line {
    solid: bool,
    kind: u8, // 0 visible, 1 oculta, 2 tangente
    points: Vec<P2>,
}

fn dist_to_polyline(p: P2, pts: &[P2]) -> f64 {
    let mut best = f64::INFINITY;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let l2 = d[0] * d[0] + d[1] * d[1];
        let t = if l2 < 1e-18 { 0.0 } else { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0) };
        best = best.min((p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]));
    }
    if pts.len() == 1 { (p[0] - pts[0][0]).hypot(p[1] - pts[0][1]) } else { best }
}

fn as_circle(pts: &[P2]) -> Option<(P2, f64)> {
    if pts.len() < 8 {
        return None;
    }
    let (f, z) = (pts[0], pts[pts.len() - 1]);
    let span = pts.iter().map(|p| (p[0] - f[0]).hypot(p[1] - f[1])).fold(0.0, f64::max);
    if (f[0] - z[0]).hypot(f[1] - z[1]) > span * 0.02 {
        return None;
    }
    let ring = &pts[..pts.len() - 1];
    let n = ring.len() as f64;
    let c = [ring.iter().map(|p| p[0]).sum::<f64>() / n, ring.iter().map(|p| p[1]).sum::<f64>() / n];
    let rs: Vec<f64> = ring.iter().map(|p| (p[0] - c[0]).hypot(p[1] - c[1])).collect();
    let r = rs.iter().sum::<f64>() / n;
    rs.iter().all(|x| (x - r).abs() < r * 0.03).then_some((c, r))
}

fn as_segment(pts: &[P2]) -> Option<(P2, P2)> {
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    if len < 1e-9 {
        return None;
    }
    pts.iter().all(|p| dist_to_polyline(*p, &[a, b]) <= len * 1e-3).then_some((a, b))
}

fn as_arc(pts: &[P2]) -> Option<(P2, f64, P2)> {
    if pts.len() < 4 || as_circle(pts).is_some() || as_segment(pts).is_some() {
        return None;
    }
    let (a, m, b) = (pts[0], pts[pts.len() / 2], pts[pts.len() - 1]);
    let d = 2.0 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
    if d.abs() < 1e-12 {
        return None;
    }
    let sq = |p: P2| p[0] * p[0] + p[1] * p[1];
    let c = [
        (sq(a) * (m[1] - b[1]) + sq(m) * (b[1] - a[1]) + sq(b) * (a[1] - m[1])) / d,
        (sq(a) * (b[0] - m[0]) + sq(m) * (a[0] - b[0]) + sq(b) * (m[0] - a[0])) / d,
    ];
    let r = (a[0] - c[0]).hypot(a[1] - c[1]);
    pts.iter().all(|p| ((p[0] - c[0]).hypot(p[1] - c[1]) - r).abs() < r * 0.02).then_some((c, r, m))
}

/// Línea visible más cercana a `p` que cumpla `accept`, a menos de `tol`
fn line_near<'a>(lines: &'a [Line], p: P2, tol: f64, accept: &dyn Fn(&[P2]) -> bool) -> Option<&'a Line> {
    lines
        .iter()
        .filter(|l| l.solid && accept(&l.points))
        .map(|l| (dist_to_polyline(p, &l.points), l))
        .filter(|(d, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| l)
}

fn bounds(lines: &[Line]) -> (P2, P2) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in lines.iter().flat_map(|l| &l.points) {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    if lo[0].is_finite() { (lo, hi) } else { ([0.0; 2], [0.0; 2]) }
}

/// Partes de una polilínea dentro del círculo
fn clip_to_circle(pts: &[P2], c: P2, r: f64) -> Vec<Vec<P2>> {
    let inside = |p: P2| (p[0] - c[0]).hypot(p[1] - c[1]) <= r;
    let mut out = Vec::new();
    let mut cur: Vec<P2> = Vec::new();
    let flush = |cur: &mut Vec<P2>, out: &mut Vec<Vec<P2>>| {
        if cur.len() > 1 {
            out.push(std::mem::take(cur));
        }
        cur.clear();
    };
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let f = [a[0] - c[0], a[1] - c[1]];
        let aa = d[0] * d[0] + d[1] * d[1];
        let bb = 2.0 * (f[0] * d[0] + f[1] * d[1]);
        let cc = f[0] * f[0] + f[1] * f[1] - r * r;
        let disc = bb * bb - 4.0 * aa * cc;
        let (mut t0, mut t1) = (0.0, 1.0);
        if aa < 1e-18 {
            if !inside(a) {
                continue;
            }
        } else if disc <= 0.0 {
            flush(&mut cur, &mut out);
            continue;
        } else {
            let sq = disc.sqrt();
            t0 = f64::max(0.0, (-bb - sq) / (2.0 * aa));
            t1 = f64::min(1.0, (-bb + sq) / (2.0 * aa));
            if t0 >= t1 {
                flush(&mut cur, &mut out);
                continue;
            }
        }
        let at = |t: f64| [a[0] + t * d[0], a[1] + t * d[1]];
        if t0 > 0.0 {
            flush(&mut cur, &mut out);
        }
        if cur.is_empty() {
            cur.push(at(t0));
        }
        cur.push(at(t1));
        if t1 < 1.0 {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

// ─── Hoja ───────────────────────────────────────────────────────────────────

const SCALES: &[f64] = &[10.0, 5.0, 2.0, 1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005];

fn scale_name(s: f64) -> String {
    if s >= 1.0 { format!("{}:1", num(s)) } else { format!("1:{}", (1.0 / s).round()) }
}

fn parse_scale(s: &str) -> Option<f64> {
    let (a, b) = s.split_once(':')?;
    let (a, b): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0.0 && b > 0.0).then(|| a / b)
}

fn sheet_size(name: &str) -> (f64, f64) {
    match name {
        "A2" => (594.0, 420.0),
        "A3" => (420.0, 297.0),
        "Carta" => (279.4, 215.9),
        _ => (297.0, 210.0),
    }
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

/// Celda de la grilla de cada vista normalizada (columna, fila) según el diedro:
/// columnas [a la izquierda del frente, frente, a la derecha, posterior], filas
/// [arriba, frente, abajo]; la isométrica arriba a la derecha
fn cell(view: &str, first: bool) -> (usize, usize) {
    let (above, below, left_of, right_of) = if first { ("bottom", "top", "right", "left") } else { ("top", "bottom", "left", "right") };
    match view {
        "front" => (1, 1),
        v if v == above => (1, 0),
        v if v == below => (1, 2),
        v if v == left_of => (0, 1),
        v if v == right_of => (2, 1),
        "back" => (3, 1),
        _ => (3, 0),
    }
}

/// Una vista puesta en la hoja
struct Placed {
    name: String,
    title: Option<String>,
    dir: Option<Dir>,
    lines: Vec<Line>,
    hatch: Vec<[P2; 3]>,
    lo: P2,
    hi: P2,
    /// Escala propia (con la de la hoja: los detalles la suya)
    scale: f64,
    /// Centro en la hoja (mm)
    at: P2,
    /// Detalle: círculo de recorte (coordenadas de la vista)
    clip: Option<(P2, f64)>,
}

impl Placed {
    fn sheet(&self, p: P2) -> P2 {
        let mid = [(self.lo[0] + self.hi[0]) / 2.0, (self.lo[1] + self.hi[1]) / 2.0];
        [self.at[0] + (p[0] - mid[0]) * self.scale, self.at[1] - (p[1] - mid[1]) * self.scale]
    }
}

/// Lo que salió de dibujar: el SVG y lo que se puso
struct Drawn {
    svg: String,
    scale: f64,
    views: Vec<String>,
    dims: Vec<String>,
    skipped: Vec<String>,
    bom: Vec<Value>,
}

/// Dibuja la lámina `spec` de la pestaña activa (ya enfocada)
fn render(ctx: &Ctx, spec: &Spec, tab_name: &str, assembly: bool) -> Result<Drawn, Fail> {
    let info = ctx.call("cad_drawing_info", json!({})).map_err(|e| e.hint("the tab needs a solid (or, for an assembly, instances) first"))?;
    let (lo3, hi3): (P3, P3) = (serde_json::from_value(info["min"].clone()).unwrap_or_default(), serde_json::from_value(info["max"].clone()).unwrap_or_default());
    let corners: Vec<P3> = (0..8).map(|k| [if k & 1 == 0 { lo3[0] } else { hi3[0] }, if k & 2 == 0 { lo3[1] } else { hi3[1] }, if k & 4 == 0 { lo3[2] } else { hi3[2] }]).collect();
    let reach = 2.0 * corners.iter().map(|c| dot(*c, *c).sqrt()).fold(1.0, f64::max);
    let section_y = spec.section.map(|y| y.unwrap_or((lo3[1] + hi3[1]) / 2.0));

    // Vistas a pedir al núcleo: las normalizadas y las de los cortes
    let mut names: Vec<String> = VIEW_NAMES.iter().filter(|v| spec.views.iter().any(|w| w == *v)).map(|v| v.to_string()).collect();
    let mut skipped = Vec::new();
    let cuts: Vec<&Cut> = spec
        .cuts
        .iter()
        .filter(|c| {
            let ok = c.points.len() >= 2 && Dir::standard(&c.view).is_some();
            if !ok {
                skipped.push(format!("section {}-{} (needs 2+ points on a standard view)", c.label, c.label));
            }
            ok
        })
        .collect();
    let mut specs: Vec<Value> = names
        .iter()
        .map(|n| {
            let d = Dir::standard(n).unwrap();
            match section_y {
                Some(y) if n == "front" => json!({ "eye": d.eye, "xdir": d.xdir, "section": { "origin": [0.0, y, 0.0], "normal": [0.0, 1.0, 0.0] } }),
                _ => json!({ "eye": d.eye, "xdir": d.xdir }),
            }
        })
        .collect();
    for c in &cuts {
        let parent = Dir::standard(&c.view).unwrap();
        let d = cut_dir(c, parent);
        specs.push(json!({ "eye": d.eye, "xdir": d.xdir, "cut": [cut_prism(c, parent, reach)] }));
        names.push(cut_view_name(&c.label));
    }
    let drawn = ctx.call("cad_drawing", json!({ "views": specs }))?;
    let mut views: Vec<Placed> = Vec::new();
    for (name, res) in names.iter().zip(drawn.as_array().into_iter().flatten()) {
        let lines: Vec<Line> = res["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| {
                let (kind, solid) = match l["kind"].as_str() {
                    Some("visible" | "outline") => (0, true),
                    Some("hidden" | "hidden_outline") if spec.hidden_lines && name != "iso" => (1, false),
                    Some("smooth") => (2, false),
                    _ => return None,
                };
                let points: Vec<P2> = l["points"].as_array().into_iter().flatten().filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?])).collect();
                (points.len() > 1).then_some(Line { solid, kind, points })
            })
            .collect();
        let hatch: Vec<[P2; 3]> = res["hatch"].as_array().into_iter().flatten().filter_map(|t| serde_json::from_value(t.clone()).ok()).collect();
        let (lo, hi) = bounds(&lines);
        let title = if let Some(label) = name.strip_prefix("corte-") {
            Some(format!("SECTION {label}-{label}"))
        } else if name == "front" && section_y.is_some() {
            Some("SECTION A-A".into())
        } else {
            None
        };
        views.push(Placed { name: name.clone(), title, dir: dir_of(spec, name), lines, hatch, lo, hi, scale: 1.0, at: [0.0; 2], clip: None });
    }
    // Detalles: las líneas de su vista recortadas al círculo
    let mut details = Vec::new();
    for d in &spec.details {
        let Some(parent) = views.iter().find(|v| v.name == d.view) else {
            skipped.push(format!("detail {} (view {} is not on the sheet)", d.label, d.view));
            continue;
        };
        let lines: Vec<Line> = parent.lines.iter().flat_map(|l| clip_to_circle(&l.points, d.center, d.radius).into_iter().map(|points| Line { points, ..l.clone() })).collect();
        let r = d.radius;
        details.push((d.scale, Placed {
            name: format!("detalle-{}", d.label),
            title: None,
            dir: parent.dir,
            lines,
            hatch: parent.hatch.clone(),
            lo: [d.center[0] - r, d.center[1] - r],
            hi: [d.center[0] + r, d.center[1] + r],
            scale: 1.0,
            at: [0.0; 2],
            clip: Some((d.center, r)),
        }));
    }

    // Hoja, grilla y escala. Las vistas extra (cortes y detalles) en columnas a la derecha
    let (sw, sh) = sheet_size(&spec.sheet);
    let (margin, gap, title_h, around) = (10.0, 16.0, 30.0, 12.0);
    let extra_cells = views.iter().filter(|v| v.name.starts_with("corte-")).count() + details.len();
    let extra_cols = extra_cells.div_ceil(3);
    let mut cells: Vec<(usize, usize)> = views.iter().map(|v| if v.name.starts_with("corte-") { (0, 0) } else { cell(&v.name, spec.first) }).collect();
    let mut k = 0;
    for (i, v) in views.iter().enumerate() {
        if v.name.starts_with("corte-") {
            cells[i] = (4 + k / 3, k % 3);
            k += 1;
        }
    }
    let detail_cells: Vec<(usize, usize)> = (0..details.len()).map(|j| (4 + (k + j) / 3, (k + j) % 3)).collect();
    let ncols = 4 + extra_cols;
    // Ancho y alto de cada celda a una escala `s`
    let detail_scale = |own: Option<f64>, s: f64| own.unwrap_or_else(|| SCALES.iter().copied().filter(|x| *x >= 2.0 * s - 1e-9).last().unwrap_or(2.0 * s));
    let sizes = |s: f64| -> (Vec<f64>, Vec<f64>) {
        let (mut cw, mut rh) = (vec![0.0f64; ncols], vec![0.0f64; 3]);
        for (v, (c, r)) in views.iter().zip(&cells) {
            cw[*c] = cw[*c].max((v.hi[0] - v.lo[0]) * s);
            rh[*r] = rh[*r].max((v.hi[1] - v.lo[1]) * s + if v.title.is_some() { 8.0 } else { 0.0 });
        }
        for ((own, d), (c, r)) in details.iter().zip(&detail_cells) {
            let k = detail_scale(*own, s);
            cw[*c] = cw[*c].max((d.hi[0] - d.lo[0]) * k);
            rh[*r] = rh[*r].max((d.hi[1] - d.lo[1]) * k + 8.0);
        }
        (cw, rh)
    };
    let fits = |s: f64| {
        let (cw, rh) = sizes(s);
        let w: f64 = cw.iter().sum::<f64>() + gap * (cw.iter().filter(|x| **x > 0.0).count().max(1) - 1) as f64;
        let h: f64 = rh.iter().sum::<f64>() + gap * (rh.iter().filter(|x| **x > 0.0).count().max(1) - 1) as f64;
        w <= sw - 2.0 * margin - 2.0 * around && h <= sh - 2.0 * margin - title_h - 2.0 * around
    };
    let scale = spec.scale.unwrap_or_else(|| SCALES.iter().copied().find(|s| fits(*s)).unwrap_or(0.005));
    let (cw, rh) = sizes(scale);
    let used_w: f64 = cw.iter().sum::<f64>() + gap * (cw.iter().filter(|x| **x > 0.0).count().max(1) - 1) as f64;
    let used_h: f64 = rh.iter().sum::<f64>() + gap * (rh.iter().filter(|x| **x > 0.0).count().max(1) - 1) as f64;
    let x0 = margin + ((sw - 2.0 * margin - used_w) / 2.0).max(around);
    let y0 = margin + ((sh - 2.0 * margin - title_h - used_h) / 2.0).max(around);
    let center_of = |c: usize, r: usize, title: bool| {
        let x = x0 + (0..c).map(|i| if cw[i] > 0.0 { cw[i] + gap } else { 0.0 }).sum::<f64>() + cw[c] / 2.0;
        let y = y0 + (0..r).map(|i| if rh[i] > 0.0 { rh[i] + gap } else { 0.0 }).sum::<f64>() + (rh[r] - if title { 8.0 } else { 0.0 }) / 2.0;
        [x, y]
    };
    for (v, (c, r)) in views.iter_mut().zip(&cells) {
        v.scale = scale;
        v.at = center_of(*c, *r, v.title.is_some());
    }
    for ((own, d), (c, r)) in details.iter_mut().zip(&detail_cells) {
        d.scale = detail_scale(*own, scale);
        d.at = center_of(*c, *r, true);
        d.title = Some(format!("DETAIL {} ({})", d.name.trim_start_matches("detalle-"), scale_name(d.scale)));
    }

    // SVG
    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{sw}mm" height="{sh}mm" viewBox="0 0 {sw} {sh}" font-family="DejaVu Sans, Arial, sans-serif"><defs><pattern id="hatch" width="2" height="2" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="2" stroke="black" stroke-width="0.18"/></pattern></defs><rect x="0" y="0" width="{sw}" height="{sh}" fill="white"/><rect x="{m}" y="{m}" width="{iw}" height="{ih}" fill="none" stroke="black" stroke-width="0.5"/>"##,
        m = margin,
        iw = sw - 2.0 * margin,
        ih = sh - 2.0 * margin
    );
    let mut clip_id = 0;
    for v in views.iter().chain(details.iter().map(|(_, d)| d)) {
        let mut group_open = String::new();
        if let Some((c, r)) = v.clip {
            clip_id += 1;
            let p = v.sheet(c);
            let _ = write!(svg, r##"<clipPath id="clip{clip_id}"><circle cx="{:.3}" cy="{:.3}" r="{:.3}"/></clipPath><circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="none" stroke="black" stroke-width="0.35"/>"##, p[0], p[1], r * v.scale, p[0], p[1], r * v.scale);
            group_open = format!(r##" clip-path="url(#clip{clip_id})""##);
        }
        let _ = write!(svg, "<g{group_open}>");
        if !v.hatch.is_empty() {
            let mut d = String::new();
            for t in &v.hatch {
                let (a, b, c) = (v.sheet(t[0]), v.sheet(t[1]), v.sheet(t[2]));
                let _ = write!(d, "M{:.3},{:.3} L{:.3},{:.3} L{:.3},{:.3} Z ", a[0], a[1], b[0], b[1], c[0], c[1]);
            }
            let _ = write!(svg, r##"<path d="{d}" fill="url(#hatch)" stroke="url(#hatch)" stroke-width="0.05"/>"##);
        }
        let mut paths = [String::new(), String::new(), String::new()];
        for l in &v.lines {
            for (i, p) in l.points.iter().enumerate() {
                let q = v.sheet(*p);
                let _ = write!(paths[l.kind as usize], "{}{:.3},{:.3} ", if i == 0 { "M" } else { "L" }, q[0], q[1]);
            }
        }
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="#999" stroke-width="0.18"/><path d="{}" fill="none" stroke="#444" stroke-width="0.25" stroke-dasharray="1.6 0.9"/><path d="{}" fill="none" stroke="black" stroke-width="0.4" stroke-linecap="round" stroke-linejoin="round"/></g>"##,
            paths[2], paths[1], paths[0]
        );
        if let Some(t) = &v.title {
            let half_h = (v.hi[1] - v.lo[1]) * v.scale / 2.0;
            let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="3.5" text-anchor="middle" font-weight="bold">{}</text>"##, v.at[0], v.at[1] + half_h + 6.5, esc(t));
        }
    }

    // Marcas en las vistas madre: línea de corte con letras y flechas, círculo de detalle
    let mut marks = |label: &str, view: &str, pts: Vec<P2>, arrow: P2| {
        let Some(v) = views.iter().find(|x| x.name == view) else { return };
        let q: Vec<P2> = pts.iter().map(|p| v.sheet(*p)).collect();
        let mut d = String::new();
        for (i, p) in q.iter().enumerate() {
            let _ = write!(d, "{}{:.2},{:.2} ", if i == 0 { "M" } else { "L" }, p[0], p[1]);
        }
        let _ = write!(svg, r##"<path d="{d}" fill="none" stroke="black" stroke-width="0.3" stroke-dasharray="6 1.2 1 1.2"/>"##);
        // En la hoja Y va hacia abajo
        let a = [arrow[0], -arrow[1]];
        for p in [q[0], q[q.len() - 1]] {
            let tip = [p[0] + a[0] * 6.0, p[1] + a[1] * 6.0];
            let _ = write!(svg, r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="0.35"/>"##, p[0], p[1], tip[0], tip[1]);
            arrow_head(&mut svg, tip[0], tip[1], a[0], a[1]);
            let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="4" font-weight="bold" text-anchor="middle">{}</text>"##, tip[0] + a[0] * 3.0, tip[1] + a[1] * 3.0 + 1.4, esc(label));
        }
    };
    if let Some(y) = section_y
        && let Some(top) = views.iter().find(|v| v.name == "top")
    {
        let (x_lo, x_hi) = (top.lo[0] - 4.0 / scale, top.hi[0] + 4.0 / scale);
        // Desde arriba la Y del modelo es el arriba de la vista; se mira hacia +Y (el frente cortado)
        marks("A", "top", vec![[x_lo, y], [x_hi, y]], [0.0, 1.0]);
    }
    for c in &cuts {
        marks(&c.label, &c.view, c.points.clone(), cut_arrow(c));
    }
    for d in &spec.details {
        if let Some(v) = views.iter().find(|x| x.name == d.view) {
            let p = v.sheet(d.center);
            let r = d.radius * v.scale;
            let _ = write!(
                svg,
                r##"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="none" stroke="black" stroke-width="0.3" stroke-dasharray="6 1.2 1 1.2"/><text x="{:.2}" y="{:.2}" font-size="4" font-weight="bold">{}</text>"##,
                p[0], p[1], r, p[0] + r * 0.75 + 1.0, p[1] - r * 0.75 - 1.0, esc(&d.label)
            );
        }
    }

    // Cotas generales: ancho y alto en el frente, fondo en la planta o en una lateral
    let mut dims = Vec::new();
    if spec.overall {
        if let Some(f) = views.iter().find(|v| v.name == "front") {
            let (w, h) = (f.hi[0] - f.lo[0], f.hi[1] - f.lo[1]);
            dim_aligned(&mut svg, f.sheet([f.lo[0], f.lo[1]]), f.sheet([f.hi[0], f.lo[1]]), [0.0, 8.0], &num(w));
            dim_aligned(&mut svg, f.sheet([f.lo[0], f.lo[1]]), f.sheet([f.lo[0], f.hi[1]]), [-8.0, 0.0], &num(h));
            dims.push(format!("width {} mm", num(w)));
            dims.push(format!("height {} mm", num(h)));
        }
        if let Some(t) = views.iter().find(|v| v.name == "top") {
            let d = t.hi[1] - t.lo[1];
            dim_aligned(&mut svg, t.sheet([t.hi[0], t.lo[1]]), t.sheet([t.hi[0], t.hi[1]]), [8.0, 0.0], &num(d));
            dims.push(format!("depth {} mm", num(d)));
        } else if let Some(s) = views.iter().find(|v| v.name == "left" || v.name == "right") {
            let d = s.hi[0] - s.lo[0];
            dim_aligned(&mut svg, s.sheet([s.lo[0], s.lo[1]]), s.sheet([s.hi[0], s.lo[1]]), [0.0, 8.0], &num(d));
            dims.push(format!("depth {} mm", num(d)));
        }
    }

    // Cotas a mano: como `userDimensions` de la app
    for (k, d) in spec.dims.iter().enumerate() {
        let Some(v) = views.iter().find(|x| x.name == d.view) else {
            skipped.push(format!("dimension {} (view {} is not on the sheet)", k + 1, d.view));
            continue;
        };
        if d.view == "iso" {
            skipped.push(format!("dimension {} (the iso view is foreshortened: dimension a standard view)", k + 1));
            continue;
        }
        let tol = 3.0 / v.scale;
        let far = (v.hi[0] - v.lo[0]).hypot(v.hi[1] - v.lo[1]) * 0.15;
        let shape: &dyn Fn(&[P2]) -> bool = match d.kind.as_str() {
            "diameter" => &|p| as_circle(p).is_some(),
            "radius" => &|p| as_arc(p).is_some() || as_circle(p).is_some(),
            _ => &|p| as_segment(p).is_some(),
        };
        let found: Vec<Option<&Line>> = d.refs.iter().map(|r| line_near(&v.lines, *r, tol, shape).or_else(|| line_near(&v.lines, *r, far, shape))).collect();
        let need = if matches!(d.kind.as_str(), "distance" | "angle") { 2 } else { 1 };
        if found.len() < need || found.iter().take(need).any(Option::is_none) {
            skipped.push(format!("dimension {} ({} on {}: no matching line near the given point)", k + 1, d.kind, d.view));
            continue;
        }
        let center = [(v.lo[0] + v.hi[0]) / 2.0, (v.lo[1] + v.hi[1]) / 2.0];
        let l0 = &found[0].unwrap().points;
        match d.kind.as_str() {
            "diameter" => {
                let (c, r) = as_circle(l0).unwrap();
                let (a, b) = (v.sheet([c[0] - r, c[1]]), v.sheet([c[0] + r, c[1]]));
                dim_aligned(&mut svg, a, b, [0.0, 0.0], &format!("Ø{}", num(2.0 * r)));
                dims.push(format!("Ø{} mm ({})", num(2.0 * r), d.view));
            }
            "radius" => {
                let (c, r, mid) = as_arc(l0).or_else(|| as_circle(l0).map(|(c, r)| (c, r, [c[0] + r * 0.7071, c[1] + r * 0.7071]))).unwrap();
                let l = (mid[0] - c[0]).hypot(mid[1] - c[1]).max(1e-12);
                let p = [c[0] + (mid[0] - c[0]) * r / l, c[1] + (mid[1] - c[1]) * r / l];
                let (cs, ps) = (v.sheet(c), v.sheet(p));
                let _ = write!(svg, r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="0.18"/>"##, cs[0], cs[1], ps[0], ps[1]);
                let u = [(ps[0] - cs[0]) / (ps[0] - cs[0]).hypot(ps[1] - cs[1]).max(1e-12), (ps[1] - cs[1]) / (ps[0] - cs[0]).hypot(ps[1] - cs[1]).max(1e-12)];
                arrow_head(&mut svg, ps[0], ps[1], u[0], u[1]);
                let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="3.5">R{}</text>"##, ps[0] + u[0] * 2.0, ps[1] + u[1] * 2.0, num(r));
                dims.push(format!("R{} mm ({})", num(r), d.view));
            }
            "length" => {
                let (p, q) = as_segment(l0).unwrap();
                let len = (q[0] - p[0]).hypot(q[1] - p[1]);
                let mut n = [-(q[1] - p[1]) / len, (q[0] - p[0]) / len];
                let mid = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
                if (mid[0] - center[0]) * n[0] + (mid[1] - center[1]) * n[1] < 0.0 {
                    n = [-n[0], -n[1]];
                }
                dim_aligned(&mut svg, v.sheet(p), v.sheet(q), [n[0] * 7.0, -n[1] * 7.0], &num(len));
                dims.push(format!("{} mm ({})", num(len), d.view));
            }
            "distance" => {
                let (s1, s2) = (as_segment(l0).unwrap(), as_segment(&found[1].unwrap().points).unwrap());
                let (p, q) = s1;
                let len = (q[0] - p[0]).hypot(q[1] - p[1]);
                let u = [(q[0] - p[0]) / len, (q[1] - p[1]) / len];
                let n = [-u[1], u[0]];
                let m2 = [(s2.0[0] + s2.1[0]) / 2.0, (s2.0[1] + s2.1[1]) / 2.0];
                let off = (m2[0] - p[0]) * n[0] + (m2[1] - p[1]) * n[1];
                let along = (m2[0] - p[0]) * u[0] + (m2[1] - p[1]) * u[1];
                let foot = [p[0] + u[0] * along, p[1] + u[1] * along];
                let d2 = [s2.1[0] - s2.0[0], s2.1[1] - s2.0[1]];
                if (d2[0] * u[1] - d2[1] * u[0]).abs() / d2[0].hypot(d2[1]).max(1e-12) > 0.02 {
                    skipped.push(format!("dimension {} (distance needs two parallel lines)", k + 1));
                    continue;
                }
                let side = if (foot[0] - center[0]) * u[0] + (foot[1] - center[1]) * u[1] >= 0.0 { 1.0 } else { -1.0 };
                dim_aligned(&mut svg, v.sheet(foot), v.sheet(m2), [u[0] * 7.0 * side, -u[1] * 7.0 * side], &num(off.abs()));
                dims.push(format!("{} mm ({})", num(off.abs()), d.view));
            }
            "angle" => {
                let (s1, s2) = (as_segment(l0).unwrap(), as_segment(&found[1].unwrap().points).unwrap());
                let Some((x, t0, t1, deg, reach)) = angle_between(s1, s2, d.refs[0], d.refs[1]) else {
                    skipped.push(format!("dimension {} (angle between parallel lines)", k + 1));
                    continue;
                };
                let c = v.sheet(x);
                let r = (reach * v.scale).max(8.0);
                // En la hoja Y va hacia abajo: los ángulos cambian de signo
                let (a0, a1) = (-t0, -t1);
                let p0 = [c[0] + r * a0.cos(), c[1] + r * a0.sin()];
                let p1 = [c[0] + r * a1.cos(), c[1] + r * a1.sin()];
                let sweep = if (a1 - a0).sin() > 0.0 { 1 } else { 0 };
                let _ = write!(svg, r##"<path d="M{:.2},{:.2} A{r:.2},{r:.2} 0 0 {sweep} {:.2},{:.2}" fill="none" stroke="black" stroke-width="0.18"/>"##, p0[0], p0[1], p1[0], p1[1]);
                let mid = (a0 + a1) / 2.0 + if (a1 - a0).abs() > std::f64::consts::PI { std::f64::consts::PI } else { 0.0 };
                let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="3.5" text-anchor="middle">{}°</text>"##, c[0] + (r + 3.5) * mid.cos(), c[1] + (r + 3.5) * mid.sin() + 1.2, num(deg));
                dims.push(format!("{}° ({})", num(deg), d.view));
            }
            other => skipped.push(format!("dimension {} (unknown kind {other})", k + 1)),
        }
    }

    // Ensamble: lista de materiales y globos en la iso
    let mut rows_out = Vec::new();
    if assembly && spec.bom {
        let doc: cad_model::Document = serde_json::from_value(ctx.call("cad_get_document", json!({}))?).map_err(|e| Fail::new(e.to_string()))?;
        let asm = doc.assembly.unwrap_or_default();
        let rows = bom(ctx, &asm);
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
            let dir = iso.dir.unwrap();
            let mut marks: Vec<(usize, P2)> = Vec::new();
            for item in info["items"].as_array().into_iter().flatten() {
                let id = item["instance"].as_u64().unwrap_or(0) as u32;
                let Some(inst) = asm.instances.iter().find(|i| i.id == id) else { continue };
                let Some(k) = item_of(inst) else { continue };
                if marks.iter().any(|m| m.0 == k) {
                    continue;
                }
                let c: P3 = serde_json::from_value(item["center"].clone()).unwrap_or_default();
                marks.push((k, iso.sheet(dir.project(c))));
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
    let title = if spec.title.is_empty() { tab_name } else { &spec.title };
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
    let _ = write!(svg, r##"<text x="{:.2}" y="{:.2}" font-size="6" font-weight="bold">{}</text><text x="{:.2}" y="{:.2}" font-size="2.6" fill="#555">{}</text>"##, tx + 3.0, ty + 8.0, esc(title), tx + 3.0, ty + 12.0, esc(&format!("{} · tab \"{tab_name}\" · Pinocchio", if assembly { "Assembly" } else { "Part" })));
    label(&mut svg, tx, ty + 14.0, "DRAWN BY", if spec.author.is_empty() { "-" } else { &spec.author });
    label(&mut svg, tx + 50.0, ty + 14.0, "SCALE", &scale_name(scale));
    label(&mut svg, tx + 85.0, ty + 14.0, "DATE · UNITS", &format!("{} · mm", today()));
    label(&mut svg, tx + 135.0, ty + 14.0, "PROJECTION · SHEET", &format!("{} · {}", if spec.first { "1st angle" } else { "3rd angle" }, spec.sheet));
    projection_symbol(&mut svg, tx + 157.0, ty + 4.0, spec.first);
    svg.push_str("</svg>");
    let mut placed: Vec<String> = views.iter().map(|v| v.name.clone()).collect();
    placed.extend(details.iter().map(|(_, d)| d.name.clone()));
    Ok(Drawn { svg, scale, views: placed, dims, skipped, bom: rows_out })
}

/// Ángulo entre dos rectas (como `angleBetween` de la app): vértice, ángulo de
/// cada rayo hacia su clic, grados y radio del arco
fn angle_between(s1: (P2, P2), s2: (P2, P2), r1: P2, r2: P2) -> Option<(P2, f64, f64, f64, f64)> {
    let d1 = [s1.1[0] - s1.0[0], s1.1[1] - s1.0[1]];
    let d2 = [s2.1[0] - s2.0[0], s2.1[1] - s2.0[1]];
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    if den.abs() < 1e-9 * d1[0].hypot(d1[1]) * d2[0].hypot(d2[1]) {
        return None;
    }
    let t = ((s2.0[0] - s1.0[0]) * d2[1] - (s2.0[1] - s1.0[1]) * d2[0]) / den;
    let x = [s1.0[0] + t * d1[0], s1.0[1] + t * d1[1]];
    let ray = |s: (P2, P2), r: P2| {
        let l = (s.1[0] - s.0[0]).hypot(s.1[1] - s.0[1]);
        let d = [(s.1[0] - s.0[0]) / l, (s.1[1] - s.0[1]) / l];
        let mut side = (r[0] - x[0]) * d[0] + (r[1] - x[1]) * d[1];
        if side.abs() < 1e-9 {
            side = ((s.0[0] + s.1[0]) / 2.0 - x[0]) * d[0] + ((s.0[1] + s.1[1]) / 2.0 - x[1]) * d[1];
        }
        if side >= 0.0 { d } else { [-d[0], -d[1]] }
    };
    let (u1, u2) = (ray(s1, r1), ray(s2, r2));
    let deg = (u1[0] * u2[0] + u1[1] * u2[1]).clamp(-1.0, 1.0).acos().to_degrees();
    let reach = ((r1[0] - x[0]).hypot(r1[1] - x[1]) + (r2[0] - x[0]).hypot(r2[1] - x[1])) / 2.0;
    Some((x, u1[1].atan2(u1[0]), u2[1].atan2(u2[0]), deg, reach))
}

/// Punta de flecha en (x, y) apuntando hacia (dx, dy)
fn arrow_head(svg: &mut String, x: f64, y: f64, dx: f64, dy: f64) {
    let (l, w) = (2.5, 0.8);
    let (px, py) = (-dy, dx);
    let _ = write!(svg, r##"<path d="M{x:.2},{y:.2} L{:.2},{:.2} L{:.2},{:.2} Z" fill="black"/>"##, x - dx * l + px * w, y - dy * l + py * w, x - dx * l - px * w, y - dy * l - py * w);
}

/// Cota de a a b (hoja) corrida `off` (con `off` cero, la cota va sobre a–b, como un diámetro)
fn dim_aligned(svg: &mut String, a: P2, b: P2, off: P2, text: &str) {
    let (a2, b2) = ([a[0] + off[0], a[1] + off[1]], [b[0] + off[0], b[1] + off[1]]);
    let ol = off[0].hypot(off[1]);
    if ol > 0.0 {
        let e = [off[0] / ol * 1.5, off[1] / ol * 1.5];
        let s = [off[0] / ol * 1.0, off[1] / ol * 1.0];
        let _ = write!(
            svg,
            r##"<g stroke="black" stroke-width="0.18"><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/><line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/></g>"##,
            a[0] + s[0], a[1] + s[1], a2[0] + e[0], a2[1] + e[1], b[0] + s[0], b[1] + s[1], b2[0] + e[0], b2[1] + e[1]
        );
    }
    let _ = write!(svg, r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="0.18"/>"##, a2[0], a2[1], b2[0], b2[1]);
    let l = (b2[0] - a2[0]).hypot(b2[1] - a2[1]).max(1e-9);
    let u = [(b2[0] - a2[0]) / l, (b2[1] - a2[1]) / l];
    arrow_head(svg, a2[0], a2[1], -u[0], -u[1]);
    arrow_head(svg, b2[0], b2[1], u[0], u[1]);
    // Texto sobre la línea, legible (de izquierda a derecha o de abajo arriba)
    let mut ang = u[1].atan2(u[0]).to_degrees();
    if ang > 90.0 {
        ang -= 180.0;
    } else if ang <= -90.0 {
        ang += 180.0;
    }
    let n = [ang.to_radians().sin(), -ang.to_radians().cos()];
    let (tx, ty) = ((a2[0] + b2[0]) / 2.0 + n[0] * 1.2, (a2[1] + b2[1]) / 2.0 + n[1] * 1.2);
    let _ = write!(svg, r##"<text x="{tx:.2}" y="{ty:.2}" font-size="3.5" text-anchor="middle" transform="rotate({ang:.2} {tx:.2} {ty:.2})">{}</text>"##, esc(text));
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

// ─── Argumentos comunes ─────────────────────────────────────────────────────

fn sheet_props() -> Vec<Prop> {
    let point3 = json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 });
    vec![
        ("tab", id_or_name("Design or assembly tab (default: the active tab)"), false),
        ("title", string("Title in the title block (default: the tab name)"), false),
        ("author", string("\"Drawn by\" in the title block"), false),
        ("views", array("Standard views on the sheet (default front, top, left, iso; \"side\" = the side view next to the front)", one_of("view", &["front", "top", "bottom", "left", "right", "back", "iso", "side"])), false),
        ("projection", one_of("first = ISO/European (default), third = ASME/American", &["first", "third"]), false),
        ("sheet", one_of("Sheet size, landscape (default A4)", &["A4", "A3", "A2", "Carta"]), false),
        ("scale", string("Force a scale like \"1:2\" or \"2:1\" (default: the largest standard scale that fits)"), false),
        ("hidden_lines", boolean("Draw hidden edges dashed (default true)"), false),
        ("dimensions", boolean("Overall dimensions (default true)"), false),
        ("section", json!({ "type": ["boolean", "number"], "description": "Section A-A: the front view cut by a plane parallel to it, at this Y in mm (true = through the middle)" }), false),
        (
            "cuts",
            array(
                "Section views cut along a line drawn on a standard view (each gets the next letter B, C…)",
                object(&[
                    ("view", one_of("View the cutting line is drawn on", &["front", "top", "bottom", "left", "right", "back"]), true),
                    ("points", array("Two or more model points [x, y, z] mm the line goes through (more = stepped section)", point3.clone()), true),
                    ("flip", boolean("Look the other way (default: to the left of the first segment as seen in that view)"), false),
                ]),
            ),
            false,
        ),
        (
            "details",
            array(
                "Enlarged details: a circle on a view drawn again bigger",
                object(&[
                    ("view", one_of("View the circle is on", &["front", "top", "bottom", "left", "right", "back"]), true),
                    ("center", point3.clone(), true),
                    ("radius", json!({ "type": "number", "minimum": 0.01, "description": "Circle radius in model mm" }), true),
                    ("scale", string("Detail scale like \"5:1\" (default: twice the sheet scale)"), false),
                ]),
            ),
            false,
        ),
        (
            "dims",
            array(
                "Dimensions of specific edges on a view: length (a straight edge), diameter (a circle seen round), radius (an arc), distance (two parallel edges), angle (two edges). Give each edge by `at` (a model point on it) or `edges` (numbers from cad_geometry)",
                object(&[
                    ("view", string("front, top, bottom, left, right, back, or \"corte-B\", \"corte-C\"… for a section view from `cuts` (not iso: it is foreshortened)"), true),
                    ("kind", one_of("What to dimension", &["length", "diameter", "radius", "distance", "angle"]), true),
                    ("at", json!({ "type": "array", "description": "Model point [x, y, z] mm on the edge, or a list of two such points for distance/angle" }), false),
                    ("edges", array("Edge numbers from cad_geometry instead of `at`", json!({ "type": "integer", "minimum": 0 })), false),
                ]),
            ),
            false,
        ),
    ]
}

/// Aplica los argumentos a `spec` (los que no vienen quedan como estaban)
fn apply_args(ctx: &Ctx, spec: &mut Spec, args: &Value) -> Result<(), Fail> {
    if let Some(t) = arg_str(args, "title") {
        spec.title = t.into();
    }
    if let Some(a) = arg_str(args, "author") {
        spec.author = a.into();
    }
    if let Some(p) = arg_str(args, "projection") {
        spec.first = p == "first";
    }
    if let Some(s) = arg_str(args, "sheet") {
        spec.sheet = s.into();
    }
    if let Some(s) = arg_str(args, "scale") {
        spec.scale = Some(parse_scale(s).ok_or_else(|| Fail::new(format!("\"{s}\" is not a scale")).hint("write it like \"1:2\" or \"2:1\""))?);
    }
    if let Some(v) = arg_bool(args, "hidden_lines") {
        spec.hidden_lines = v;
    }
    if let Some(v) = arg_bool(args, "dimensions") {
        spec.overall = v;
    }
    if let Some(list) = args.get("views").and_then(Value::as_array).filter(|l| !l.is_empty()) {
        let side = if spec.first { "left" } else { "right" };
        spec.views = list.iter().filter_map(Value::as_str).map(|v| if v == "side" { side } else { v }).map(String::from).collect();
    }
    match args.get("section") {
        Some(Value::Bool(true)) => spec.section = Some(None),
        Some(Value::Bool(false)) => spec.section = None,
        Some(v) if v.is_number() => spec.section = Some(v.as_f64()),
        _ => {}
    }
    let p3 = |v: &Value| -> Option<P3> { Some([v[0].as_f64()?, v[1].as_f64()?, v[2].as_f64()?]) };
    for c in args["cuts"].as_array().into_iter().flatten() {
        let view = c["view"].as_str().unwrap_or("front").to_string();
        let dir = Dir::standard(&view).ok_or_else(|| Fail::new(format!("cuts: no view \"{view}\"")))?;
        let points: Vec<P2> = c["points"].as_array().into_iter().flatten().filter_map(p3).map(|p| dir.project(p)).collect();
        if points.len() < 2 || points.windows(2).any(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) < 1e-6) {
            return Err(Fail::new(format!("cuts: the line on the {view} view needs two or more distinct points as seen in that view")).hint("e.g. on the top view, two points with different X or Y"));
        }
        let label = spec.next_label();
        spec.cuts.push(Cut { label, view, points, flip: c["flip"] == true });
    }
    for d in args["details"].as_array().into_iter().flatten() {
        let view = d["view"].as_str().unwrap_or("front").to_string();
        let dir = Dir::standard(&view).ok_or_else(|| Fail::new(format!("details: no view \"{view}\"")))?;
        let center = dir.project(p3(&d["center"]).ok_or_else(|| Fail::new("details: `center` must be [x, y, z]"))?);
        let scale = match d["scale"].as_str() {
            Some(s) => Some(parse_scale(s).ok_or_else(|| Fail::new(format!("details: \"{s}\" is not a scale")))?),
            None => None,
        };
        let label = spec.next_label();
        spec.details.push(Detail { label, view, center, radius: d["radius"].as_f64().unwrap_or(5.0), scale });
    }
    for (k, d) in args["dims"].as_array().into_iter().flatten().enumerate() {
        let view = d["view"].as_str().unwrap_or("front").to_string();
        let kind = d["kind"].as_str().unwrap_or("length").to_string();
        let dir = dir_of(spec, &view).ok_or_else(|| Fail::new(format!("dims[{k}]: no view \"{view}\"")).hint("standard views, or \"corte-B\"… for a section defined in `cuts`"))?;
        // Un punto [x, y, z] o una lista de puntos
        let mut points: Vec<P3> = if d["at"][0].is_number() { p3(&d["at"]).into_iter().collect() } else { d["at"].as_array().into_iter().flatten().filter_map(p3).collect() };
        if d.get("at").is_some() && points.is_empty() {
            return Err(Fail::new(format!("dims[{k}]: `at` must be [x, y, z] or [[x, y, z], [x, y, z]]")));
        }
        for e in d["edges"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            let m = ctx.call("cad_measure", json!({ "items": [{ "kind": "edge", "index": e }] })).map_err(|f| f.hint("edge numbers come from `cad_geometry` and change after every edit"))?;
            let it = &m["items"][0];
            let c = p3(&it["center"]).unwrap_or_default();
            // Una recta: su medio está sobre ella. Un círculo: un punto del borde, en la vista
            points.push(match it["radius"].as_f64() {
                Some(r) if it["kind"] != "line" => add(c, add(mul(dir.xdir, r * 0.7071), mul(dir.up(), r * 0.7071))),
                _ => c,
            });
        }
        let need = if matches!(kind.as_str(), "distance" | "angle") { 2 } else { 1 };
        if points.len() < need {
            return Err(Fail::new(format!("dims[{k}]: {kind} needs {need} edge(s)")).hint("give `at` with model points on the edges, or `edges`"));
        }
        spec.dims.push(Dim { view, kind, refs: points.iter().map(|p| dir.project(*p)).collect() });
    }
    Ok(())
}

// ─── Exportar ───────────────────────────────────────────────────────────────

const EXPORT: Tool = Tool {
    name: "drawing_export",
    title: "Make a 2D drawing (PDF/SVG)",
    description: "Makes an engineering drawing sheet of a design or assembly tab and writes it as PDF or SVG: \
standard views aligned by the projection (front, top, bottom, left, right, back, iso), exact visible and dashed \
hidden lines, overall dimensions, a standard scale chosen to fit, a title block with the projection symbol, and for \
assemblies a bill of materials with numbered balloons. Also: `section` (A-A, the front cut through), `cuts` (section \
views cut along a line drawn on a view, hatched), `details` (a circled area drawn bigger) and `dims` (dimensions of \
specific edges). With `drawing` it draws one of the app's drawing sheets instead (see `drawing_sheet`). Returns a \
PNG preview of the sheet. Example: {\"path\": \"/home/me/bracket.pdf\", \"title\": \"Bracket\", \"dims\": \
[{\"view\": \"top\", \"kind\": \"diameter\", \"at\": [13, 0, 10]}]}.",
    input: || {
        let mut props = sheet_props();
        props.push(("path", string("Absolute path ending in .pdf or .svg"), true));
        props.push(("drawing", id_or_name("Draw this drawing sheet of the tab (made in the app or with `drawing_sheet`), by name or id; other options then change only this export"), false));
        props.push(("overwrite", boolean("Replace the file if it exists (default false)"), false));
        object(&props)
    },
    effect: Effect::Write,
    run: export,
};

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
    let mut spec = match args.get("drawing") {
        Some(want) => {
            let doc = ctx.call("cad_get_document", json!({}))?;
            let (_, s) = find_sheet(&doc, want)?;
            Spec::from_app(&s)
        }
        None => Spec::new("", &tab_name),
    };
    apply_args(ctx, &mut spec, args)?;
    let drawn = render(ctx, &spec, &tab_name, assembly)?;
    let bytes = if ext == "pdf" {
        ctx.call("cad_write_pdf", json!({ "path": path, "svg": drawn.svg }))?.as_u64().unwrap_or(0)
    } else {
        std::fs::write(&path, &drawn.svg).map_err(|e| Fail::new(format!("could not write {}: {e}", path.display())))?;
        drawn.svg.len() as u64
    };
    let preview = rasterize(&drawn.svg, 1100).map_err(Fail::new)?;
    let mut lines = vec![format!(
        "Wrote the drawing of {} tab \"{tab_name}\" (id {tab}) to {} ({}, {bytes} bytes): views {}, scale {}, sheet {} landscape, {} projection.",
        if assembly { "assembly" } else { "design" },
        path.display(),
        ext.to_uppercase(),
        drawn.views.join(", "),
        scale_name(drawn.scale),
        spec.sheet,
        if spec.first { "first-angle (ISO)" } else { "third-angle (ASME)" }
    )];
    lines.extend(summary(&drawn));
    lines.push("The image is a preview of the sheet.".into());
    let mut out = Output::new(lines.join("\n"), json!({ "path": path, "bytes": bytes, "scale": scale_name(drawn.scale), "sheet": spec.sheet, "views": drawn.views, "dimensions": drawn.dims, "skipped": drawn.skipped, "bom": drawn.bom }));
    out.images.push(preview);
    Ok(out)
}

fn summary(d: &Drawn) -> Vec<String> {
    let mut lines = Vec::new();
    if !d.dims.is_empty() {
        lines.push(format!("Dimensions: {}.", d.dims.join(", ")));
    }
    if !d.bom.is_empty() {
        lines.push(format!("Bill of materials with {} item(s) and balloons on the iso view.", d.bom.len()));
    }
    if !d.skipped.is_empty() {
        lines.push(format!("Left out: {}.", d.skipped.join("; ")));
    }
    lines
}

// ─── Láminas de la app ──────────────────────────────────────────────────────

/// Las láminas guardadas en el documento (la forma vieja de un solo plano, como «Plano 1»)
fn sheets_of(doc: &Value) -> Vec<Value> {
    let d = &doc["drawing"];
    if let Some(list) = d["sheets"].as_array() {
        return list.clone();
    }
    let has_old = d["dims"].as_array().is_some_and(|l| !l.is_empty()) || d["details"].as_array().is_some_and(|l| !l.is_empty()) || !d["sketch"].is_null();
    if has_old {
        return vec![json!({ "id": 1, "name": "Plano 1", "sheet": "A4", "projection": "first", "hiddenLines": true, "overall": true, "hiddenViews": [], "dims": d["dims"], "details": d["details"], "sketch": d["sketch"], "title": "", "author": "" })];
    }
    Vec::new()
}

fn find_sheet(doc: &Value, want: &Value) -> Result<(usize, Value), Fail> {
    let list = sheets_of(doc);
    let found = find_id_or_name(&list, want, |s| s["id"].as_u64(), |s| s["name"].as_str().unwrap_or(""));
    found.map(|i| (i, list[i].clone())).ok_or_else(|| {
        let names: Vec<String> = list.iter().map(|s| format!("{} \"{}\"", s["id"], s["name"].as_str().unwrap_or(""))).collect();
        Fail::new(format!("no drawing sheet {want}")).hint(if names.is_empty() { "this tab has no drawing sheets yet: make one with `drawing_sheet`".into() } else { format!("sheets: {}", names.join(", ")) })
    })
}

const SHEET: Tool = Tool {
    name: "drawing_sheet",
    title: "Make or change a drawing sheet in the app",
    description: "Creates or edits one of the tab's drawing sheets — the ones the user opens in Pinocchio under \"2D \
drawings\" (Planos 2D) and keep following the model. Same options as `drawing_export` (views, projection, sheet, \
scale, title, section A-A, `cuts`, `details`, `dims`); cuts, details and dims are added to the ones the sheet already \
has (`clear` removes them first). `name` picks the sheet (a new one is made if there is none with that name); \
`remove: true` deletes it. Returns a preview. Example: {\"name\": \"Plano 1\", \"sheet\": \"A3\", \"section\": true, \
\"dims\": [{\"view\": \"front\", \"kind\": \"length\", \"at\": [0, 0, 10]}]}.",
    input: || {
        let mut props = sheet_props();
        props.push(("name", string("Sheet name (default: the first sheet, or a new \"Plano 1\")"), false));
        props.push(("rename", string("New name for the sheet"), false));
        props.push(("clear", array("Remove these before adding", one_of("what", &["cuts", "details", "dims", "section"])), false));
        props.push(("remove", boolean("Delete the sheet"), false));
        props.push(("bom", boolean("Assemblies: bill of materials and balloons (default true)"), false));
        object(&props)
    },
    effect: Effect::Change,
    run: sheet,
};

fn sheet(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    super::cad::focus(ctx, tab)?;
    let assembly = kind == "assembly";
    let mut doc = ctx.call("cad_get_document", json!({}))?;
    let mut list = sheets_of(&doc);
    let next_id = list.iter().filter_map(|s| s["id"].as_u64()).max().unwrap_or(0).max(doc["drawing"]["next_id"].as_u64().unwrap_or(1).saturating_sub(1)) + 1;
    let found = match arg_str(args, "name") {
        Some(n) => list.iter().position(|s| s["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n.trim()))),
        None => (!list.is_empty()).then_some(0),
    };
    if arg_bool(args, "remove") == Some(true) {
        let i = found.ok_or_else(|| Fail::new("there is no such sheet to remove"))?;
        let gone = list.remove(i);
        doc["drawing"] = json!({ "sheets": list, "next_id": next_id });
        ctx.call("cad_set_tab_document", json!({ "tab": tab, "document": doc }))?;
        return Ok(Output::new(format!("Removed drawing sheet \"{}\" of tab \"{tab_name}\".", gone["name"].as_str().unwrap_or("")), json!({ "tab": tab, "removed": gone["id"] })));
    }
    let (id, base, created) = match found {
        Some(i) => (list[i]["id"].as_u64().unwrap_or(next_id), Some(list[i].clone()), false),
        None => (next_id, None, true),
    };
    let mut spec = match &base {
        Some(s) => Spec::from_app(s),
        None => Spec::new(arg_str(args, "name").unwrap_or(&format!("Plano {id}")), &tab_name),
    };
    // Lo que `from_app` saltea (ocultos) se conserva: se escribe sobre la lámina de antes
    for what in args["clear"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        match what {
            "cuts" => spec.cuts.clear(),
            "details" => spec.details.clear(),
            "dims" => spec.dims.clear(),
            _ => spec.section = None,
        }
    }
    if let Some(n) = arg_str(args, "rename") {
        spec.name = n.trim().into();
    }
    if let Some(b) = arg_bool(args, "bom") {
        spec.bom = b;
    }
    apply_args(ctx, &mut spec, args)?;
    let drawn = render(ctx, &spec, &tab_name, assembly)?;
    let mut stored = spec.to_app(id, base.as_ref());
    // Las cotas, cortes y detalles ocultos de antes siguen en la lámina
    if let Some(b) = &base {
        for key in ["dims", "details", "sections"] {
            let hidden: Vec<Value> = b[key].as_array().into_iter().flatten().filter(|x| x["hidden"] == true).cloned().collect();
            if !hidden.is_empty()
                && let Some(l) = stored[key].as_array_mut()
            {
                l.extend(hidden);
            }
        }
    }
    match found {
        Some(i) => list[i] = stored,
        None => list.push(stored),
    }
    doc["drawing"] = json!({ "sheets": list, "next_id": next_id.max(id + 1) });
    ctx.call("cad_set_tab_document", json!({ "tab": tab, "document": doc }))?;
    let preview = rasterize(&drawn.svg, 1100).map_err(Fail::new)?;
    let mut lines = vec![format!(
        "{} drawing sheet \"{}\" (id {id}) of tab \"{tab_name}\": views {}, scale {}{}, sheet {}, {} projection{}{}.",
        if created { "Made" } else { "Updated" },
        spec.name,
        drawn.views.join(", "),
        scale_name(drawn.scale),
        if spec.scale.is_none() { " (automatic)" } else { "" },
        spec.sheet,
        if spec.first { "first-angle" } else { "third-angle" },
        if spec.section.is_some() { ", section A-A" } else { "" },
        if spec.cuts.is_empty() { String::new() } else { format!(", sections {}", spec.cuts.iter().map(|c| format!("{0}-{0} on {1}", c.label, c.view)).collect::<Vec<_>>().join(", ")) }
    )];
    lines.extend(summary(&drawn));
    lines.push("The user sees it in the app under 2D drawings; the image is a preview (the app lays the views out its own way). `drawing_export` with `drawing` writes it to PDF.".into());
    let mut out = Output::new(lines.join("\n"), json!({ "tab": tab, "drawing": id, "name": spec.name, "views": drawn.views, "dimensions": drawn.dims, "skipped": drawn.skipped }));
    out.images.push(preview);
    Ok(out)
}
