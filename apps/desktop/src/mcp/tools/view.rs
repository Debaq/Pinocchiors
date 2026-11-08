//! Ver y medir: una imagen del diseño con los números de caras o aristas, y
//! medidas de caras, aristas y puntos.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::cad::{direction, find_tab, num, tab_prop, vec3};
use super::{arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::render::{self, Camera, Labels, Options};
use crate::mcp::schema::{any_object, array, boolean, integer, number_in, object, one_of};

pub const TOOLS: &[Tool] = &[VIEW, MEASURE, VIEWER];

const VIEWS: &[&str] = &["iso", "iso_back", "iso_below", "top", "bottom", "front", "back", "left", "right"];

const VIEW: Tool = Tool {
    name: "view",
    title: "Look at the design or the mesh (image)",
    description: "Returns a PNG image so you can check your work. By default a design tab: shaded solid, visible edges, each part \
in its own colour, an X/Y/Z axis triad, and the number of every visible face (or edge) printed on it — the same \
numbers `cad_geometry`, `cad_fillet`, `sketch_create` `on_face` etc. use. `highlight_faces` / `highlight_edges` paint \
some orange to confirm a pick before using it. `show: \"model\"` shows the active mesh model instead (sharp edges \
drawn, no numbers) and `show: \"pieces\"` the pieces from `print_cut`, each in its colour. Views: iso \
(front-right-top), iso_back, iso_below, top, bottom, front (looking along +Y), back, left, right; or \
`azimuth`/`elevation` in degrees. Example: {\"view\": \"iso\", \"labels\": \"faces\"}.",
    input: || {
        object(&[
            tab_prop(),
            ("show", one_of("What to look at: a design tab (default), the mesh model, or the cut pieces", &["design", "model", "pieces"]), false),
            ("view", one_of("Named view (default iso)", VIEWS), false),
            ("azimuth", number_in("Custom view: degrees around Z from +X toward +Y (use with elevation)", -360.0, 360.0), false),
            ("elevation", number_in("Custom view: degrees above the XY plane", -89.0, 89.0), false),
            ("labels", one_of("Numbers to print on the image (default faces)", &["faces", "edges", "none"]), false),
            ("highlight_faces", array("Face numbers to paint orange", integer("face", 0)), false),
            ("highlight_edges", array("Edge numbers to draw thick orange", integer("edge", 0)), false),
            ("width", integer("Image width in pixels (default 800, max 1600)", 160), false),
            ("height", integer("Image height in pixels (default 600, max 1200)", 120), false),
        ])
    },
    effect: Effect::Read,
    run: view,
};

/// Colores de pieza: suaves y bien distintos
const PALETTE: [[u8; 3]; 8] = [[150, 172, 205], [214, 176, 120], [140, 196, 150], [205, 145, 160], [175, 160, 214], [120, 190, 195], [210, 200, 130], [190, 150, 120]];

/// Activa la pestaña (diseño o ensamble): (id, nombre, es ensamble)
fn open_any(ctx: &Ctx, args: &Value) -> Result<(u32, String, bool), Fail> {
    let (tab, name, kind) = find_tab(ctx, args.get("tab"), false)?;
    super::cad::focus(ctx, tab)?;
    Ok((tab, name, kind == "assembly"))
}

fn view(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    if let Some(what @ ("model" | "pieces")) = arg_str(args, "show") {
        return view_mesh(ctx, args, what);
    }
    let (tab, tab_name, assembly) = open_any(ctx, args)?;
    let mm = ctx.call("cad_mm_per_unit", json!({}))?.as_f64().unwrap_or(1.0) as f32;
    // Pieza (o instancia) de cada cara: rangos de caras de cada una
    let (bytes, ranges): (Vec<u8>, Vec<(u64, u64)>) = if assembly {
        let faces = ctx.call("cad_assembly_faces", json!({}))?;
        let ranges = faces.as_array().into_iter().flatten().map(|r| (r[1].as_u64().unwrap_or(0), r[2].as_u64().unwrap_or(0))).collect();
        (ctx.call_bytes("cad_assembly_mesh", json!({}))?, ranges)
    } else {
        let result = ctx.call("cad_evaluate", json!({}))?;
        let ranges = result["parts"].as_array().into_iter().flatten().map(|p| (p["faces"][0].as_u64().unwrap_or(0), p["faces"][1].as_u64().unwrap_or(0))).collect();
        (ctx.call_bytes("cad_mesh", json!({}))?, ranges)
    };
    let mesh = render::decode_cad_mesh(&bytes, mm).map_err(Fail::new)?;
    if mesh.triangles.is_empty() {
        return Err(Fail::new(format!("tab \"{tab_name}\" has nothing to show yet")).hint(if assembly { "insert parts with `asm_insert`" } else { "make a solid with `cad_extrude`, `cad_revolve` or `cad_primitive`" }));
    }
    let face_color = move |f: u32| {
        let k = ranges.iter().position(|r| (r.0..r.1).contains(&(f as u64))).unwrap_or(0);
        PALETTE[k % PALETTE.len()]
    };
    let (camera, view_name) = camera(args)?;
    let labels = match arg_str(args, "labels").unwrap_or("faces") {
        "edges" => Labels::Edges,
        "none" => Labels::None,
        _ => Labels::Faces,
    };
    let ints = |k: &str| args.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect::<Vec<_>>();
    let (width, height) = image_size(args);
    let opts = Options {
        width,
        height,
        camera,
        labels,
        highlight_faces: ints("highlight_faces").into_iter().map(|f| f as u32).collect::<HashSet<_>>(),
        highlight_edges: ints("highlight_edges").into_iter().map(|e| e as usize).collect::<HashSet<_>>(),
        face_color: Box::new(face_color),
    };
    let pic = render::render(&mesh, &opts).map_err(Fail::new)?;
    let what = match labels {
        Labels::Faces => format!("Face numbers shown: {}.", list(&pic.labeled)),
        Labels::Edges => format!("Edge numbers shown: {}.", list(&pic.labeled)),
        Labels::None => "No numbers shown.".into(),
    };
    let mut out = Output::new(
        format!(
            "View {view_name} of tab \"{tab_name}\" ({}×{} px). {what} {} have different colours; the triad shows X (red), Y (green), Z (blue). Small faces may have no number: use `cad_geometry` for the full list.",
            opts.width,
            opts.height,
            if assembly { "Instances" } else { "Parts" }
        ),
        json!({ "tab": tab, "view": view_name, "labels": pic.labeled, "faces": mesh.triangle_face.iter().max().map_or(0, |m| m + 1), "edges": mesh.edges.len() }),
    );
    out.images.push(pic.png);
    Ok(out)
}

/// La cámara pedida y cómo se llama esa vista
fn camera(args: &Value) -> Result<(Camera, String), Fail> {
    let name = arg_str(args, "view").unwrap_or("iso");
    match (args.get("azimuth").and_then(Value::as_f64), args.get("elevation").and_then(Value::as_f64)) {
        (Some(a), Some(e)) => Ok((Camera::orbit(a as f32, e as f32), format!("azimuth {a} / elevation {e}"))),
        (None, None) => Ok((Camera::named(name).unwrap_or_else(|| Camera::named("iso").unwrap()), name.to_string())),
        _ => Err(Fail::new("give both `azimuth` and `elevation`, or neither")),
    }
}

fn image_size(args: &Value) -> (u32, u32) {
    (args.get("width").and_then(Value::as_u64).unwrap_or(800).clamp(160, 1600) as u32, args.get("height").and_then(Value::as_u64).unwrap_or(600).clamp(120, 1200) as u32)
}

/// El modelo de malla o las piezas del corte, sin números
fn view_mesh(ctx: &Ctx, args: &Value, what: &str) -> Result<Output, Fail> {
    super::mesh::need_model(ctx)?;
    let bytes = ctx.call_bytes("mcp_model_mesh", json!({ "what": what })).map_err(|e| if what == "pieces" { e.hint("cut the model first with `print_cut`") } else { e })?;
    let mesh = render::decode_cad_mesh(&bytes, 1.0).map_err(Fail::new)?;
    let (camera, view_name) = camera(args)?;
    let (width, height) = image_size(args);
    let pieces = mesh.triangle_face.iter().max().map_or(0, |m| m + 1);
    let opts = Options {
        width,
        height,
        camera,
        labels: Labels::None,
        highlight_faces: HashSet::new(),
        highlight_edges: HashSet::new(),
        face_color: Box::new(|f: u32| PALETTE[f as usize % PALETTE.len()]),
    };
    let pic = render::render(&mesh, &opts).map_err(Fail::new)?;
    let subject = if what == "pieces" { format!("the {pieces} cut pieces (one colour each)") } else { "the mesh model".into() };
    let mut out = Output::new(
        format!("View {view_name} of {subject} ({width}×{height} px); sharp edges drawn; the triad shows X (red), Y (green), Z (blue)."),
        json!({ "show": what, "view": view_name, "triangles": mesh.triangles.len(), "pieces": pieces }),
    );
    out.images.push(pic.png);
    Ok(out)
}

fn list(ids: &[u32]) -> String {
    if ids.is_empty() { "none".into() } else { ids.iter().map(u32::to_string).collect::<Vec<_>>().join(", ") }
}

const MEASURE: Tool = Tool {
    name: "measure",
    title: "Measure faces, edges and points",
    description: "Measures one or two things of the solid in a design tab. One: its type, area or length, radius, centre \
and direction. Two: also the minimum distance between them (with the closest points and the X/Y/Z difference), the \
distance between centres and the angle when both have a direction (planar faces, straight edges). Things are \
{\"face\": n}, {\"edge\": n} (numbers from `cad_geometry` or `view`) or {\"point\": [x, y, z]} in mm. \
Example: {\"items\": [{\"face\": 4}, {\"face\": 9}]}.",
    input: || object(&[tab_prop(), ("items", array("One or two things to measure", any_object("{\"face\": n} | {\"edge\": n} | {\"point\": [x, y, z]}")), true)]),
    effect: Effect::Read,
    run: measure,
};

fn measure(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, _) = open_any(ctx, args)?;
    let raw = args["items"].as_array().cloned().unwrap_or_default();
    if raw.is_empty() || raw.len() > 2 {
        return Err(Fail::new("give one or two items"));
    }
    let mut items = Vec::new();
    let mut names = Vec::new();
    for (k, it) in raw.iter().enumerate() {
        let (item, label) = if let Some(f) = it.get("face").and_then(Value::as_u64) {
            (json!({ "kind": "face", "index": f }), format!("face {f}"))
        } else if let Some(e) = it.get("edge").and_then(Value::as_u64) {
            (json!({ "kind": "edge", "index": e }), format!("edge {e}"))
        } else if let Some(p) = it.get("point").filter(|p| p.as_array().is_some_and(|a| a.len() == 3)) {
            (json!({ "kind": "vertex", "point": p }), format!("point {}", vec3(p)))
        } else {
            return Err(Fail::new(format!("items[{k}]: expected {{\"face\": n}}, {{\"edge\": n}} or {{\"point\": [x, y, z]}}, got {it}")));
        };
        items.push(item);
        names.push(label);
    }
    let m = ctx.call("cad_measure", json!({ "items": items })).map_err(|e| e.hint("face and edge numbers come from `cad_geometry` and change after every edit"))?;
    let mut lines = vec![format!("Tab \"{tab_name}\":")];
    for (it, name) in m["items"].as_array().into_iter().flatten().zip(&names) {
        let mut parts = vec![it["kind"].as_str().unwrap_or("").to_string()];
        if let Some(a) = it["area"].as_f64() {
            parts.push(format!("area {} mm²", num(a)));
        }
        if let Some(l) = it["length"].as_f64() {
            parts.push(format!("length {} mm", num(l)));
        }
        if let Some(r) = it["radius"].as_f64() {
            parts.push(format!("radius {} mm (diameter {})", num(r), num(2.0 * r)));
        }
        parts.push(format!("centre {}", vec3(&it["center"])));
        if !it["direction"].is_null() {
            parts.push(format!("direction {}", direction(&it["direction"])));
        }
        lines.push(format!("  {name}: {}", parts.join(", ")));
    }
    if let Some(dist) = m["distance"].as_object() {
        lines.push(format!(
            "Minimum distance: {} mm, from {} to {} (ΔX {}, ΔY {}, ΔZ {}).",
            num(dist["value"].as_f64().unwrap_or(0.0)),
            vec3(&dist["a"]),
            vec3(&dist["b"]),
            num(dist["delta"][0].as_f64().unwrap_or(0.0)),
            num(dist["delta"][1].as_f64().unwrap_or(0.0)),
            num(dist["delta"][2].as_f64().unwrap_or(0.0))
        ));
    }
    if let Some(c) = m["center_distance"].as_f64() {
        lines.push(format!("Distance between centres: {} mm.", num(c)));
    }
    if let Some(a) = m["angle"].as_f64() {
        lines.push(format!("Angle: {}°.", num(a)));
    }
    Ok(Output::new(lines.join("\n"), json!({ "tab": tab, "measurement": m })))
}

const VIEWER: Tool = Tool {
    name: "viewer",
    title: "Check the app's 3D viewer (performance)",
    description: "Live mode only: looks at the real 3D viewer of the open Pinocchio window, to test speed and drawing. \
`action`: `diagnostics` (which renderer draws — WebGL, or WebGPU / WebGL2 with the experimental GPU acceleration —, FPS, \
what is loaded, and how long each backend call and each heavy viewer action took since the last reset; `reset: true` \
starts counting from zero after reading), `capture` (PNG of the viewer exactly as the user sees it; `max_side` px, \
default 1024), `gpu` (turn the experimental GPU acceleration on or off with `on`; the window reloads, wait a few \
seconds and call `diagnostics`), `benchmark` (`kind`: `frame` drawing the view, `brush` paint-brush lookups, \
`influence` bone-influence preview in Rust vs TypeScript, `pose` rotating a weighted joint: GPU preview and final \
CPU pass; nothing is changed) or `explode` (assembly view open: set the exploded-view slider to `value` 0–2 and time it).",
    input: || {
        object(&[
            ("action", one_of("What to do", &["diagnostics", "capture", "gpu", "benchmark", "explode"]), true),
            ("reset", boolean("diagnostics: start counting from zero after reading"), false),
            ("max_side", integer("capture: largest side in pixels (default 1024)", 64), false),
            ("on", boolean("gpu: turn the GPU acceleration on (true) or off (false)"), false),
            ("kind", one_of("benchmark: what to measure", &["frame", "brush", "influence", "pose"]), false),
            ("value", number_in("explode: slider value, 0 = assembled", 0.0, 2.0), false),
        ])
    },
    effect: Effect::Read,
    run: viewer,
};

fn viewer(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    use base64::Engine;
    let action = arg_str(args, "action").unwrap_or("diagnostics");
    let Some(answer) = ctx.session.backend.ask_with("viewer", args.clone()) else {
        return Err(Fail::new("there is no open Pinocchio window to look at").hint(
            "viewer works in live mode: the user turns on \"Let an AI drive this window\" in Settings ▸ AI connector",
        ));
    };
    let v = answer?;
    match action {
        "capture" => {
            let png = base64::engine::general_purpose::STANDARD
                .decode(v["png"].as_str().unwrap_or(""))
                .map_err(|e| Fail::new(format!("the window sent a broken image: {e}")))?;
            let mut out = Output::new("The viewer as the user sees it.", json!({ "bytes": png.len() }));
            out.images.push(png);
            Ok(out)
        }
        "diagnostics" => {
            let viewer = &v["viewer"];
            let gpu = &v["gpu"];
            let mut lines = vec![format!(
                "Renderer: {} · {} FPS · canvas {} · {} vertices, {} triangles{}{}.",
                viewer["renderer"].as_str().unwrap_or("?"),
                viewer["fps"],
                viewer["canvas"],
                viewer["vertices"],
                viewer["triangles"],
                if viewer["weights"] == true { ", with weights" } else { "" },
                if viewer["posed"] == true { ", posed" } else { "" },
            )];
            lines.push(format!(
                "GPU acceleration: setting {}, in use {}, backend {}, WebGPU in this webview {}{}.",
                gpu["setting"], gpu["using"], gpu["backend"], gpu["navigatorGpu"],
                gpu["failure"].as_str().map_or(String::new(), |f| format!(", failed: {f}")),
            ));
            for (title, key) in [("Slowest backend calls (total)", "calls"), ("Viewer actions", "marks")] {
                let rows = v["perf"][key].as_array().cloned().unwrap_or_default();
                if rows.is_empty() {
                    continue;
                }
                lines.push(format!("{title}:"));
                for r in rows.iter().take(15) {
                    lines.push(format!(
                        "- {}: {}× avg {} ms, max {} ms, last {} ms",
                        r["name"].as_str().unwrap_or("?"),
                        r["count"],
                        r["avgMs"],
                        r["maxMs"],
                        r["lastMs"]
                    ));
                }
            }
            Ok(Output::new(lines.join("\n"), v))
        }
        "gpu" => {
            let text = if v["reloading"] == true {
                "GPU acceleration changed: the window is reloading. Wait a few seconds, then call `viewer` with `diagnostics`."
            } else {
                "GPU acceleration was already like that."
            };
            Ok(Output::new(text, v))
        }
        _ => {
            if let Some(e) = v["error"].as_str() {
                return Err(Fail::new(e.to_string()));
            }
            Ok(Output::new(serde_json::to_string_pretty(&v).unwrap_or_default(), v))
        }
    }
}
