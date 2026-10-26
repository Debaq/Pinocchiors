//! Las operaciones más comunes, con argumentos cómodos: extrusión, revolución,
//! primitivas, redondeo, chaflán y vaciado. Las demás van por `cad_add_feature`.

use cad_model::FeatureKind;
use serde_json::{json, Value};

use super::cad::{add_feature, find_feature, open_design, resolve_refs, tab_prop};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{array, boolean, id_or_name, integer, number_in, object, one_of, point, string, Prop};

pub const TOOLS: &[Tool] = &[EXTRUDE, REVOLVE, PRIMITIVE, FILLET, CHAMFER, SHELL];

fn op_prop() -> Prop {
    ("op", one_of("What to do with existing parts: join (add material, default), cut (remove), intersect (keep the overlap), new (separate part)", &["join", "cut", "intersect", "new"]), false)
}

fn name_prop() -> Prop {
    ("name", string("Name shown to the user (optional)"), false)
}

/// Arma la operación desde su JSON (con referencias por número) y la agrega
fn add_json(ctx: &Ctx, args: &Value, mut kind: Value, extra: &str) -> Result<Output, Fail> {
    let d = open_design(ctx, args)?;
    resolve_refs(ctx, &mut kind)?;
    let kind: FeatureKind = serde_json::from_value(kind).map_err(|e| Fail::new(format!("invalid arguments: {e}")))?;
    add_feature(ctx, d, kind, arg_str(args, "name"), extra)
}

/// Id de un sketch del diseño activo (por id o nombre)
fn sketch_id(ctx: &Ctx, args: &Value) -> Result<u32, Fail> {
    let d = open_design(ctx, args)?;
    let id = find_feature(&d.doc, &args["sketch"])?;
    match d.doc.get(id).map(|f| &f.kind) {
        Some(FeatureKind::Sketch { .. } | FeatureKind::SurfaceSketch { .. }) => Ok(id.0),
        _ => Err(Fail::new(format!("feature {} is not a sketch", id.0)).hint("pass the id or name of a sketch (see `cad_design`)")),
    }
}

fn regions(args: &Value) -> Value {
    match args.get("regions").and_then(Value::as_array) {
        Some(pts) if !pts.is_empty() => json!({ "type": "points", "points": pts }),
        _ => json!({ "type": "all" }),
    }
}

const EXTRUDE: Tool = Tool {
    name: "cad_extrude",
    title: "Extrude a sketch",
    description: "Pushes the closed regions of a sketch along the sketch normal to make (or cut) a solid. Give \
`distance` (mm), or `through_all: true`, or `to_face` (a face number from `cad_geometry`). `symmetric: true` splits \
the distance to both sides; `backward` adds a second distance the other way; `reverse: true` flips the direction. \
`op: \"cut\"` removes material (holes, pockets). `regions` picks which closed regions by a point inside each (sketch \
coordinates); default all. Example: {\"sketch\": 0, \"distance\": 10}; a pocket: {\"sketch\": 2, \"distance\": 3, \
\"op\": \"cut\", \"reverse\": true}.",
    input: || {
        object(&[
            tab_prop(),
            ("sketch", id_or_name("Sketch operation (id or name)"), true),
            ("distance", number_in("Length in mm", 0.0, 1e6), false),
            ("through_all", boolean("Go through the whole solid (cuts)"), false),
            ("to_face", integer("Stop at this face (number from `cad_geometry`)", 0), false),
            ("symmetric", boolean("Half the distance to each side of the sketch plane"), false),
            ("backward", number_in("Also extrude this many mm the opposite way", 0.0, 1e6), false),
            ("reverse", boolean("Extrude against the sketch normal"), false),
            op_prop(),
            ("regions", array("Points inside the regions to use, in sketch coordinates; default all", point("[x, y]", 2)), false),
            ("draft", number_in("Draft angle of the walls in degrees (positive narrows)", -89.0, 89.0), false),
            ("thin", number_in("Thin wall of this thickness following the outline instead of a filled solid (mm)", 0.0, 1e4), false),
            name_prop(),
        ])
    },
    effect: Effect::Change,
    run: extrude,
};

fn extrude(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let sketch = sketch_id(ctx, args)?;
    let distance = args.get("distance").and_then(Value::as_f64);
    let extent = if arg_bool(args, "through_all") == Some(true) {
        json!({ "type": "through_all" })
    } else if let Some(face) = args.get("to_face").and_then(Value::as_u64) {
        json!({ "type": "up_to_face", "face": { "$face": face } })
    } else {
        let d = distance.filter(|d| *d > 0.0).ok_or_else(|| Fail::new("give `distance` (mm), `through_all` or `to_face`"))?;
        match args.get("backward").and_then(Value::as_f64) {
            Some(b) => json!({ "type": "two_sides", "distance": d, "second": b }),
            None if arg_bool(args, "symmetric") == Some(true) => json!({ "type": "symmetric", "distance": d }),
            None => json!({ "type": "blind", "distance": d }),
        }
    };
    let mut kind = json!({
        "type": "extrude",
        "sketch": sketch,
        "regions": regions(args),
        "extent": extent,
        "reverse": arg_bool(args, "reverse").unwrap_or(false),
        "op": arg_str(args, "op").unwrap_or("join"),
    });
    if let Some(a) = args.get("draft").and_then(Value::as_f64) {
        kind["draft"] = json!(a);
    }
    if let Some(t) = args.get("thin").and_then(Value::as_f64) {
        kind["thin"] = json!(t);
    }
    add_json(ctx, args, kind, "")
}

const REVOLVE: Tool = Tool {
    name: "cad_revolve",
    title: "Revolve a sketch around an axis",
    description: "Spins the closed regions of a sketch around an axis to make (or cut) a solid of revolution — shafts, \
knobs, bottles. The axis is a world axis (`x`, `y`, `z`, through the origin), a line of the sketch (`axis_line`, its \
entity id; draw it as construction), or an edge (`axis_edge`, number from `cad_geometry`). Draw the profile on one \
side of the axis, e.g. on plane `xz` with sketch x ≥ 0 and revolve around `z`. `angle` in degrees (default 360). \
Example: {\"sketch\": 0, \"axis\": \"z\"}.",
    input: || {
        object(&[
            tab_prop(),
            ("sketch", id_or_name("Sketch operation (id or name)"), true),
            ("axis", one_of("World axis through the origin", &["x", "y", "z"]), false),
            ("axis_line", integer("Id of a line in this sketch to use as the axis", 0), false),
            ("axis_edge", integer("Edge number (from `cad_geometry`) to use as the axis", 0), false),
            ("angle", number_in("Degrees (default 360)", -360.0, 360.0), false),
            ("symmetric", boolean("Half the angle to each side of the sketch plane"), false),
            op_prop(),
            ("regions", array("Points inside the regions to use (sketch coordinates); default all", point("[x, y]", 2)), false),
            name_prop(),
        ])
    },
    effect: Effect::Change,
    run: revolve,
};

fn revolve(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let sketch = sketch_id(ctx, args)?;
    let axis = if let Some(line) = args.get("axis_line").and_then(Value::as_u64) {
        json!({ "type": "sketch_line", "sketch": sketch, "line": line })
    } else if let Some(edge) = args.get("axis_edge").and_then(Value::as_u64) {
        json!({ "type": "edge", "edge": { "$edge": edge } })
    } else {
        let a = arg_str(args, "axis").ok_or_else(|| Fail::new("give `axis`, `axis_line` or `axis_edge`"))?;
        json!({ "type": a })
    };
    let mut kind = json!({
        "type": "revolve",
        "sketch": sketch,
        "regions": regions(args),
        "axis": axis,
        "angle": args.get("angle").and_then(Value::as_f64).unwrap_or(360.0),
        "op": arg_str(args, "op").unwrap_or("join"),
    });
    if arg_bool(args, "symmetric") == Some(true) {
        kind["extent"] = json!({ "type": "symmetric" });
    }
    add_json(ctx, args, kind, "")
}

const PRIMITIVE: Tool = Tool {
    name: "cad_primitive",
    title: "Add a box, cylinder, sphere, cone, torus or standard hardware",
    description: "Adds a ready-made solid without a sketch. `box`: `dimensions` [x, y, z] (centred on `origin` in X and \
Y, base at `origin` unless `centered: true`); `cylinder`: `radius` or `diameter` and `height`; `cone`: `radius` (base), \
`top_radius`, `height`; `sphere`: `radius`; `torus`: `radius` (to the tube centre) and `tube_radius`; metric `bolt` \
(`thread` like \"M6\", `length`, `head` socket/hex/countersunk), `nut` and `washer` (`thread`). `origin` [x, y, z] \
(default 0,0,0) and `up` (the shape's Z direction, default [0,0,1]) place it. `op` joins, cuts… Examples: \
{\"shape\": \"box\", \"dimensions\": [40, 20, 10]}; a hole: {\"shape\": \"cylinder\", \"diameter\": 5, \"height\": 30, \
\"origin\": [10, 0, -5], \"op\": \"cut\"}.",
    input: || {
        object(&[
            tab_prop(),
            ("shape", one_of("The solid", &["box", "cylinder", "cone", "sphere", "torus", "bolt", "nut", "washer"]), true),
            ("dimensions", point("Box size [x, y, z] in mm", 3), false),
            ("centered", boolean("Box: centre also in Z (default: base on the origin)"), false),
            ("radius", number_in("mm", 0.0, 1e6), false),
            ("diameter", number_in("mm (instead of radius)", 0.0, 1e6), false),
            ("top_radius", number_in("Cone: radius at the top (0 = pointed)", 0.0, 1e6), false),
            ("tube_radius", number_in("Torus: radius of the tube", 0.0, 1e6), false),
            ("height", number_in("mm", 0.0, 1e6), false),
            ("thread", string("Bolt/nut/washer metric size, e.g. \"M6\""), false),
            ("length", number_in("Bolt length under the head (mm)", 0.0, 1e5), false),
            ("head", one_of("Bolt head", &["socket", "hex", "countersunk"]), false),
            ("origin", point("Where to put it [x, y, z] (default 0,0,0)", 3), false),
            ("up", point("Direction of the shape's Z axis (default [0, 0, 1])", 3), false),
            op_prop(),
            name_prop(),
        ])
    },
    effect: Effect::Change,
    run: primitive,
};

fn primitive(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let f = |k: &str| args.get(k).and_then(Value::as_f64);
    let need = |k: &str| f(k).ok_or_else(|| Fail::new(format!("`{}` needs `{k}`", arg_str(args, "shape").unwrap_or(""))));
    let radius = || f("radius").or(f("diameter").map(|d| d / 2.0)).ok_or_else(|| Fail::new("give `radius` or `diameter`"));
    let thread = || arg_str(args, "thread").map(str::to_string).ok_or_else(|| Fail::new("give `thread`, e.g. \"M6\""));
    let shape = match arg_str(args, "shape").unwrap_or("") {
        "box" => {
            let d = args.get("dimensions").and_then(Value::as_array).ok_or_else(|| Fail::new("a box needs `dimensions` [x, y, z]"))?;
            let c = arg_bool(args, "centered").unwrap_or(false);
            json!({ "type": "box", "dx": d[0], "dy": d[1], "dz": d[2], "centered": true, "centered_z": c })
        }
        "cylinder" => json!({ "type": "cylinder", "radius": radius()?, "height": need("height")? }),
        "cone" => json!({ "type": "cone", "r1": radius()?, "r2": f("top_radius").unwrap_or(0.0), "height": need("height")? }),
        "sphere" => json!({ "type": "sphere", "radius": radius()? }),
        "torus" => json!({ "type": "torus", "major": radius()?, "minor": need("tube_radius")? }),
        "bolt" => json!({ "type": "bolt", "size": thread()?, "length": need("length")?, "head": arg_str(args, "head").unwrap_or("socket") }),
        "nut" => json!({ "type": "nut", "size": thread()? }),
        "washer" => json!({ "type": "washer", "size": thread()? }),
        other => return Err(Fail::new(format!("unknown shape {other}"))),
    };
    let up = args.get("up").cloned().unwrap_or(json!([0.0, 0.0, 1.0]));
    // X de la primitiva: perpendicular a `up` (el mundo X, o Y si `up` es casi X)
    let u: Vec<f64> = (0..3).map(|k| up[k].as_f64().unwrap_or(0.0)).collect();
    let len = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if len < 1e-9 {
        return Err(Fail::new("`up` cannot be [0, 0, 0]"));
    }
    let x = if (u[0] / len).abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let kind = json!({
        "type": "primitive",
        "shape": shape,
        "origin": args.get("origin").cloned().unwrap_or(json!([0.0, 0.0, 0.0])),
        "z": up,
        "x": x,
        "op": arg_str(args, "op").unwrap_or("join"),
    });
    add_json(ctx, args, kind, "")
}

/// Aristas elegidas: las de `edges` más las del borde de cada cara de `faces`
fn picked_edges(ctx: &Ctx, args: &Value) -> Result<Vec<Value>, Fail> {
    open_design(ctx, args)?;
    let mut ids: Vec<u64> = args.get("edges").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect();
    for face in args.get("faces").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64) {
        let edges = ctx.call("cad_face_edges", json!({ "face": face }))?;
        ids.extend(edges.as_array().into_iter().flatten().filter_map(Value::as_u64));
    }
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(Fail::new("no edges picked").hint("pass `edges` and/or `faces` with numbers from `cad_geometry`"));
    }
    Ok(ids.into_iter().map(|e| json!({ "$edge": e })).collect())
}

fn edge_props() -> [Prop; 2] {
    [
        ("edges", array("Edge numbers from `cad_geometry`", integer("edge", 0)), false),
        ("faces", array("Face numbers: all the edges around each of these faces", integer("face", 0)), false),
    ]
}

const FILLET: Tool = Tool {
    name: "cad_fillet",
    title: "Round edges",
    description: "Rounds edges of the solid with a radius (mm). Pick `edges` and/or `faces` (all edges around a face) by \
their numbers from `cad_geometry`. Example: {\"faces\": [5], \"radius\": 2}.",
    input: || {
        let [e, f] = edge_props();
        object(&[tab_prop(), e, f, ("radius", number_in("mm", 0.0, 1e5), true), name_prop()])
    },
    effect: Effect::Change,
    run: fillet,
};

fn fillet(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let edges = picked_edges(ctx, args)?;
    let n = edges.len();
    add_json(ctx, args, json!({ "type": "fillet", "edges": edges, "radius": args["radius"] }), &format!("Rounded {n} edge(s)."))
}

const CHAMFER: Tool = Tool {
    name: "cad_chamfer",
    title: "Bevel edges",
    description: "Cuts edges at 45° (or asymmetric with `second_distance`). Pick `edges` and/or `faces` by number from \
`cad_geometry`. Example: {\"edges\": [3, 7], \"distance\": 1}.",
    input: || {
        let [e, f] = edge_props();
        object(&[
            tab_prop(),
            e,
            f,
            ("distance", number_in("mm along each face", 0.0, 1e5), true),
            ("second_distance", number_in("Different distance on the second face (mm)", 0.0, 1e5), false),
            name_prop(),
        ])
    },
    effect: Effect::Change,
    run: chamfer,
};

fn chamfer(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let edges = picked_edges(ctx, args)?;
    let n = edges.len();
    let mut kind = json!({ "type": "chamfer", "edges": edges, "distance": args["distance"] });
    if let Some(s) = args.get("second_distance").and_then(Value::as_f64) {
        kind["second"] = json!({ "type": "distance", "distance": s });
    }
    add_json(ctx, args, kind, &format!("Chamfered {n} edge(s)."))
}

const SHELL: Tool = Tool {
    name: "cad_shell",
    title: "Hollow out the solid",
    description: "Hollows the solid leaving walls of `thickness` mm, removing the `open_faces` (face numbers from \
`cad_geometry`) to make an opening — e.g. the top face for a box or cup. At least one face must be open (a closed \
hollow solid is not supported yet). Example: {\"open_faces\": [4], \"thickness\": 2}.",
    input: || {
        object(&[
            tab_prop(),
            ("open_faces", json!({ "type": "array", "description": "Faces to remove, at least one (numbers from `cad_geometry`)", "items": integer("face", 0), "minItems": 1 }), true),
            ("thickness", number_in("Wall thickness in mm", 0.0, 1e4), true),
            name_prop(),
        ])
    },
    effect: Effect::Change,
    run: shell,
};

fn shell(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let faces: Vec<Value> = args.get("open_faces").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|f| json!({ "$face": f })).collect();
    add_json(ctx, args, json!({ "type": "shell", "faces": faces, "thickness": args["thickness"] }), "")
}
