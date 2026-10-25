//! Render del modelo con GeppettoRay (imagen con luz real, para mostrar) y
//! escaneos guardados (nubes de puntos PLY) convertidos en malla.

use serde_json::{json, Value};

use super::mesh::{absolute, model_line, need_model, output_path, size};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{boolean, integer, number_in, object, one_of, point, string};

pub const TOOLS: &[Tool] = &[RENDER, SCAN];

const VIEWS: &[(&str, f32, f32)] = &[
    ("iso", -45.0, 30.0),
    ("iso_back", 135.0, 30.0),
    ("front", -90.0, 5.0),
    ("back", 90.0, 5.0),
    ("left", 180.0, 5.0),
    ("right", 0.0, 5.0),
    ("top", -90.0, 89.0),
];

const HDRIS: &[&str] = &["studio", "loft", "sky", "overcast", "sunset"];

const RENDER: Tool = Tool {
    name: "render_image",
    title: "Render a realistic image of the model",
    description: "Renders the active mesh model with Pinocchio's path tracer (GeppettoRay): real light, soft shadows, \
its materials and textures. Unlike `view` (a quick technical picture with numbers) this is for showing the result. \
The camera frames the whole model from `view` (iso, iso_back, front, back, left, right, top) or `azimuth`/`elevation` \
(degrees, Z up). `background`: studio (seamless backdrop, default), white, transparent, or `color` [r, g, b] 0–1. \
`environment` lights it with an HDR sky instead of the studio lights. `quality` draft (fast) / normal / high. \
With `path` the PNG is also saved. Example: {\"view\": \"iso\", \"quality\": \"normal\", \"path\": \"/home/me/render.png\"}.",
    input: || {
        object(&[
            ("view", one_of("Camera direction (default iso)", &VIEWS.iter().map(|v| v.0).collect::<Vec<_>>()), false),
            ("azimuth", number_in("Custom: degrees around Z from +X toward +Y (with elevation)", -360.0, 360.0), false),
            ("elevation", number_in("Custom: degrees above the XY plane", -89.0, 89.0), false),
            ("width", integer("Pixels (default 1024, max 2048)", 64), false),
            ("height", integer("Pixels (default 768, max 2048)", 64), false),
            ("quality", one_of("Samples per pixel: draft 16, normal 64, high 256 (default normal)", &["draft", "normal", "high"]), false),
            ("background", one_of("Background (default studio)", &["studio", "white", "transparent", "color"]), false),
            ("color", point("With background color: linear RGB 0–1", 3), false),
            ("environment", one_of("HDR environment light instead of studio lights", HDRIS), false),
            ("path", string("Also save the PNG here (absolute path)"), false),
            ("overwrite", boolean("Replace the file if it exists (default false)"), false),
        ])
    },
    effect: Effect::Write,
    run: render,
};

fn render(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    need_model(ctx)?;
    let path = if args.get("path").is_some() { Some(output_path(args)?) } else { None };
    if let Some(p) = &path
        && p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("png")
    {
        return Err(Fail::new(format!("{} does not end in .png", p.display())));
    }
    let name = arg_str(args, "view").unwrap_or("iso");
    let (azimuth, elevation) = match (args["azimuth"].as_f64(), args["elevation"].as_f64()) {
        (Some(a), Some(e)) => (a as f32, e as f32),
        (None, None) => VIEWS.iter().find(|v| v.0 == name).map_or((-45.0, 30.0), |v| (v.1, v.2)),
        _ => return Err(Fail::new("give both `azimuth` and `elevation`, or neither")),
    };
    let width = args["width"].as_u64().unwrap_or(1024).clamp(64, 2048) as u32;
    let height = args["height"].as_u64().unwrap_or(768).clamp(64, 2048) as u32;
    let samples = match arg_str(args, "quality").unwrap_or("normal") {
        "draft" => 16,
        "high" => 256,
        _ => 64,
    };
    let background = arg_str(args, "background").unwrap_or("studio");
    if background == "color" && args.get("color").is_none() {
        return Err(Fail::new("background `color` needs `color` [r, g, b] (0–1)"));
    }
    let view = json!({
        "azimuth": azimuth,
        "elevation": elevation,
        "width": width,
        "height": height,
        "samples": samples,
        "background": background,
        "color": args.get("color"),
        "hdri": arg_str(args, "environment").map(|h| format!("builtin:{h}")),
    });
    let png = ctx.call_bytes("ray_render_view", json!({ "view": view }))?;
    let mut text = format!("Rendered {width}×{height} px, {samples} samples per pixel, from {}.", if args.get("azimuth").is_some() { format!("azimuth {azimuth}° / elevation {elevation}°") } else { name.to_string() });
    if let Some(p) = &path {
        std::fs::write(p, &png).map_err(|e| Fail::new(format!("could not write {}: {e}", p.display())))?;
        text.push_str(&format!(" Saved {} ({}).", p.display(), size(png.len() as u64)));
    }
    let mut out = Output::new(text, json!({ "width": width, "height": height, "samples": samples, "path": path }));
    out.images.push(png);
    Ok(out)
}

const SCAN: Tool = Tool {
    name: "scan_import",
    title: "Turn a saved 3D scan (point cloud) into a mesh",
    description: "Reads a point cloud saved as PLY (millimetres; e.g. from a Revopoint or Pinocchio's own scanner) and \
rebuilds a surface from it as a new object of the scene (the others stay), with the scan colour baked into a texture. \
`detail_mm` is the reconstruction cell (default: from the point spacing; smaller = finer and slower); \
`watertight: true` uses Poisson (closed, smooth surface); holes are patched (also the base the scanner did not see) \
unless `close_holes: false`. Controlling a physical scanner is not possible from here. Example: {\"path\": \"/home/me/scan.ply\"}.",
    input: || {
        object(&[
            ("path", string("Absolute path of the .ply point cloud"), true),
            ("detail_mm", number_in("Reconstruction cell in mm (default automatic)", 0.2, 100.0), false),
            ("smooth", integer("Smoothing passes (default 2)", 0), false),
            ("watertight", boolean("Poisson reconstruction: closed and smooth (default false)"), false),
            ("close_holes", boolean("Patch holes, also big ones like the base (default true)"), false),
            ("remove_pieces", boolean("Remove small loose pieces (default true)"), false),
        ])
    },
    effect: Effect::Replace,
    run: scan,
};

fn scan(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let path = absolute(arg_str(args, "path").unwrap_or_default())?;
    if !path.is_file() {
        return Err(Fail::new(format!("{} does not exist", path.display())));
    }
    if path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("ply") {
        return Err(Fail::new("scan_import reads .ply point clouds").hint("for a mesh file (STL, OBJ, GLB, PLY with faces) use `mesh_import`"));
    }
    let settings = json!({
        "voxel_mm": args["detail_mm"].as_f64().unwrap_or(0.0),
        "fill": 1,
        "smooth": args["smooth"].as_u64().unwrap_or(2),
        "close_holes": arg_bool(args, "close_holes").unwrap_or(true),
        "max_hole_mm": 0.0,
        "flat_patch": false,
        "remove_pieces": arg_bool(args, "remove_pieces").unwrap_or(true),
        "poisson": arg_bool(args, "watertight").unwrap_or(false),
    });
    // En vivo lo hace la ventana: objeto nuevo, como con el menú
    if let Some(answer) = ctx.session.backend.ask_with("scan", json!({ "path": path, "settings": settings })) {
        let r = answer.map_err(Fail::new)?;
        let (line, a) = model_line(ctx)?;
        return Ok(Output::new(format!("Rebuilt a mesh from {} points of {} in the app, as a new object: {line}", r["points"], path.display()), json!({ "path": path, "result": r, "analysis": a })));
    }
    let name = path.file_name().map_or_else(|| "Escaneo".to_string(), |n| n.to_string_lossy().into_owned());
    let r = super::mesh::new_object_with(ctx, &name, &format!("Scan {name}"), || ctx.call("scan_mesh_file", json!({ "path": path, "settings": settings })).map(|r| (r["info"].clone(), r)))?;
    let (line, a) = model_line(ctx)?;
    Ok(Output::new(format!("Rebuilt a mesh from {} points of {} as a new object: {line}", r["points"], path.display()), json!({ "path": path, "result": r, "analysis": a })))
}
