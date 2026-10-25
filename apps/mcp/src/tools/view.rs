//! Ver y medir: una imagen del diseño con los números de caras o aristas, y
//! medidas de caras, aristas y puntos.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::cad::{direction, num, open_design, tab_prop, vec3};
use super::{arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::render::{self, Camera, Labels, Options};
use crate::schema::{any_object, array, integer, number_in, object, one_of};

pub const TOOLS: &[Tool] = &[VIEW, MEASURE];

const VIEWS: &[&str] = &["iso", "iso_back", "iso_below", "top", "bottom", "front", "back", "left", "right"];

const VIEW: Tool = Tool {
    name: "view",
    title: "Look at the design (image)",
    description: "Returns a PNG image of a design tab so you can check your work: shaded solid, visible edges, each part \
in its own colour, an X/Y/Z axis triad, and the number of every visible face (or edge) printed on it — the same \
numbers `cad_geometry`, `cad_fillet`, `sketch_create` `on_face` etc. use. `highlight_faces` / `highlight_edges` paint \
some orange to confirm a pick before using it. Views: iso (front-right-top), iso_back, iso_below, top, bottom, front \
(looking along +Y), back, left, right; or `azimuth`/`elevation` in degrees. Example: {\"view\": \"iso\", \"labels\": \
\"faces\"}.",
    input: || {
        object(&[
            tab_prop(),
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

fn view(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let d = open_design(ctx, args)?;
    let result = ctx.call("cad_evaluate", json!({}))?;
    if result["body"].is_null() {
        return Err(Fail::new(format!("tab \"{}\" has no solid yet", d.tab_name)).hint("make one with `cad_extrude`, `cad_revolve` or `cad_primitive`"));
    }
    let mm = ctx.call("cad_mm_per_unit", json!({}))?.as_f64().unwrap_or(1.0) as f32;
    let mesh = render::decode_cad_mesh(&ctx.call_bytes("cad_mesh", json!({}))?, mm).map_err(Fail::new)?;
    // Pieza de cada cara (rango de caras de cada una)
    let ranges: Vec<(u64, u64)> = result["parts"].as_array().into_iter().flatten().map(|p| (p["faces"][0].as_u64().unwrap_or(0), p["faces"][1].as_u64().unwrap_or(0))).collect();
    let face_color = move |f: u32| {
        let k = ranges.iter().position(|r| (r.0..r.1).contains(&(f as u64))).unwrap_or(0);
        PALETTE[k % PALETTE.len()]
    };
    let name = arg_str(args, "view").unwrap_or("iso");
    let camera = match (args.get("azimuth").and_then(Value::as_f64), args.get("elevation").and_then(Value::as_f64)) {
        (Some(a), Some(e)) => Camera::orbit(a as f32, e as f32),
        (None, None) => Camera::named(name).unwrap_or_else(|| Camera::named("iso").unwrap()),
        _ => return Err(Fail::new("give both `azimuth` and `elevation`, or neither")),
    };
    let labels = match arg_str(args, "labels").unwrap_or("faces") {
        "edges" => Labels::Edges,
        "none" => Labels::None,
        _ => Labels::Faces,
    };
    let ints = |k: &str| args.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect::<Vec<_>>();
    let opts = Options {
        width: args.get("width").and_then(Value::as_u64).unwrap_or(800).clamp(160, 1600) as u32,
        height: args.get("height").and_then(Value::as_u64).unwrap_or(600).clamp(120, 1200) as u32,
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
    let view_name = if args.get("azimuth").is_some() { format!("azimuth {} / elevation {}", args["azimuth"], args["elevation"]) } else { name.to_string() };
    let mut out = Output::new(
        format!(
            "View {view_name} of tab \"{}\" ({}×{} px). {what} Parts have different colours; the triad shows X (red), Y (green), Z (blue). Small faces may have no number: use `cad_geometry` for the full list.",
            d.tab_name, opts.width, opts.height
        ),
        json!({ "tab": d.tab, "view": view_name, "labels": pic.labeled, "faces": mesh.triangle_face.iter().max().map_or(0, |m| m + 1), "edges": mesh.edges.len() }),
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
    let d = open_design(ctx, args)?;
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
    let mut lines = vec![format!("Tab \"{}\":", d.tab_name)];
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
    Ok(Output::new(lines.join("\n"), json!({ "tab": d.tab, "measurement": m })))
}
