//! La guía para la IA: lo que dice `initialize`, el recurso `pinocchio://guide`
//! y la herramienta `help`. En inglés, como las descripciones.

/// Lo que se manda en `initialize.instructions`: lo justo para empezar bien
pub const INSTRUCTIONS: &str = "\
Pinocchio is a 3D app. Through these tools you do parametric CAD: parts (sketches, extrusions, fillets…), \
assemblies with mates, 2D engineering drawings and STEP/STL export; and you work on mesh models: import, repair, \
remesh, UV-unwrap, prepare for 3D printing (check, orient, cut into pieces, pegs, plates), render realistic images \
and rebuild scans. (Rigging and animation tools are not available through this connector yet.)
Start with `status` to see what is loaded and which mode you are in. Call `help` with a topic for workflows.
Use `view` to look at your work: it returns an image with the numbers of faces or edges you can pick.
Conventions: lengths in millimetres, angles in degrees, Z is up. Every change can be reverted with `undo`; \
a tool that fails changes nothing. Nothing is written to disk unless you call a save/export tool with a path.";

/// Temas de la guía: nombre, título y texto
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "overview",
        "What Pinocchio does and how to work with it",
        "\
Pinocchio works on one project at a time. Through these tools a project holds:
- Design tabs: parametric CAD parts (sketches + operations such as extrude, revolve, fillet).
- Assembly tabs: instances of parts from design tabs, positioned with mates.
- Scene objects: mesh models (imported files, scans, or CAD parts turned into meshes). One is active; the mesh and
  print tools work on it (`help` topics `mesh` and `print`).
(Projects made in the app can also hold skeletons and animation; `status` reports them.)

Typical loop:
1. `status` — see what is loaded.
2. Make changes with the area tools (each result tells you the ids and names it created).
3. Check the result (tools return a short summary; `view` returns an image when available).
4. `undo` if something went wrong; `project_save` when done.

Modes: `standalone` runs its own engine without a window. `live` drives the open Pinocchio app (when the user turned \
on \"Let an AI drive this window\"). `status` tells you which one is active.

Live mode — you share the window with the user:
- The user has priority. A tool may answer that the user is busy (editing a sketch, a dialog open) or paused you: wait \
a few seconds and try again; do not loop quickly.
- When the user says \"this\", \"here\" or \"the one I selected\", call `selection`: it returns the faces/edges they
  clicked, with the numbers other tools use.
- Each of your changes is one undo step in the app; `undo` only reverts your own last step.
- Files are the user's: `project_new`, `project_open` and `project_save` are disabled (exports still work).
- The user may change things between your calls: read the state again (`cad_design`, `cad_geometry`) before picking.",
    ),
    (
        "conventions",
        "Units, axes, names and errors",
        "\
- Lengths: millimetres. Angles: degrees. Mass: grams. Volumes: cubic millimetres unless the field says otherwise.
- Axes: right-handed, Z up, X right, Y away from the viewer (front view looks along +Y).
- Things are referred to by id and also by name; every tool result lists the ids/names it created or changed.
- Errors come back with a hint of how to fix them. A failed tool leaves the project exactly as it was.
- Paths must be absolute. Tools never overwrite an existing file unless you pass `overwrite: true`.
- Texts shown to the user (tab, part and feature names) may be in Spanish; that is expected.",
    ),
    (
        "cad",
        "Designing parts: sketch, operations, references",
        "\
A design tab is a list of operations computed in order. Typical part:
1. `sketch_create` on a plane with shapes (rectangle, circle, slot…), optionally constrained.
2. `cad_extrude` (or `cad_revolve`) the sketch: op `join` adds material, `cut` removes it.
3. More sketches on faces (`sketch_create` with `on_face`) for holes and bosses; or `cad_primitive` with op `cut`.
4. `cad_fillet` / `cad_chamfer` / `cad_shell` using face and edge numbers from `view` or `cad_geometry`.
5. Check with `view` (an image) and `cad_design` (operations, errors, size, volume); `cad_export` to STEP/STL.

- `view` prints the number of every visible face (or edge, with `labels: \"edges\"`) on the image: look at it to
  pick the right one, and confirm a pick with `highlight_faces` / `highlight_edges` before cutting or rounding.
- `measure` gives sizes, distances and angles between faces, edges and points.

- Planes: xy = top (sketch x→X, y→Y, normal +Z); xz = front (x→X, y→Z, normal −Y); yz = right (x→Y, y→Z, normal +X).
- Face and edge numbers change after every change: list them again with `cad_geometry` before picking.
- `cad_geometry` `facing` filters planar faces by direction, e.g. \"+Z\" for top faces.
- Parameters (`cad_parameters`) make designs editable: `width = 40`, then formulas bind fields to them.
- Anything without its own tool: `cad_add_feature` with the JSON from `help` topic `features`.
- Edit an operation with `cad_edit_feature` (see its JSON with `cad_design` {\"detail\": true}); delete with \
`cad_delete_feature`.",
    ),
    (
        "assembly",
        "Assemblies and 2D drawings",
        "\
Assemblies put parts from design tabs together and connect them with mates (like Onshape).
1. `cad_tab_create` {\"kind\": \"assembly\"} — the new tab becomes active.
2. `asm_insert` {\"from\": \"<design tab>\"} for each part (`fixed: true` on the base; `count` for copies).
3. `view` the assembly tab: faces carry assembly-wide numbers (instance by instance).
4. `asm_mate` {\"kind\": ..., \"a\": face, \"b\": face}: a planar face gives its centre and normal, a cylindrical one its
   axis; the second instance moves. Shaft in a hole: cylindrical between the two cylinders. Faces touching each other:
   fastened or planar with `flip: true`.
5. `asm_mate_edit` with `distance` / `angle` positions or drives a joint; `asm_instance` moves, fixes or removes.
6. `asm_check`: unsatisfied mates, degrees of freedom, collisions, clearance and the bill of materials. On a design tab with several parts it checks those parts against each other (collisions, clearance).
7. `drawing_export` {\"tab\": ..., \"path\": \"/abs/file.pdf\"} makes a drawing sheet of a design or an assembly (with
   the bill of materials and balloons); look at the returned preview.
- `cad_design`, `cad_geometry`, `view` and `measure` also work on assembly tabs (the \"part\" of a face is its instance).
- Tools without `tab` use the active tab, and most tools activate the tab they work on: pass `tab` when switching.",
    ),
    (
        "mesh",
        "Mesh models: import, repair, remesh, UV, render, scans",
        "\
Mesh tools work on the active scene object (a triangle mesh, in mm with Z up).
1. Get a model: `mesh_import` (GLB/glTF, OBJ, STL, PLY), `scan_import` (a PLY point cloud → surface) or
   `mesh_from_part` (a part of a design tab). Each one is a new scene object; the others stay. `undo` takes it back.
2. `mesh_info` — size, triangles, closed or not; `diagnose: true` counts holes, non-manifold edges, flipped faces.
3. `mesh_repair` — fills holes, fixes normals and non-manifold geometry (needed before printing an open scan).
4. `mesh_remesh` — `simplify` (fewer triangles), `smooth`, `isotropic` (even triangles), `voxel` (rebuild as a closed
   skin: fixes anything, loses fine detail), `quads`.
5. `mesh_uv` — unwrap UVs and bake the textures onto a new map.
6. `view` {\"show\": \"model\"} — a quick picture; `render_image` — a realistic one (path tracer) to show the result.
7. `mesh_export` — GLB/glTF, OBJ, STL, PLY, 3MF, USDZ.
- Mesh tools change the model of the active object; every step can be undone.
- In live mode imports and scans appear in the app as new objects in the Outliner, like when the user imports.",
    ),
    (
        "print",
        "Preparing a model for 3D printing (FDM)",
        "\
Works on the active mesh model (import one, or `mesh_from_part` for a CAD part).
1. `print_check` {\"printer\": ..., \"material\": ...} — watertight? thin walls? overhangs and support? fits the bed?
   grams of filament. Printers: Creality Ender 3 (default), Creality K1, Prusa MK4, Prusa MINI, Bambu Lab X1 / P1,
   Voron 2.4 (350), or {\"build_volume\": [x, y, z]}. Materials: PLA (default), PETG, ABS, ASA, TPU.
2. Not watertight → `mesh_repair`. Wrong size → `print_scale`.
3. `print_orient` — lays it on the face that needs the least support.
4. Too big → `print_cut` (pieces that fit, flat cuts) and `print_joints` (pegs and holes between the pieces, with the
   clearance for the printer and material). Look at them with `view` {\"show\": \"pieces\"}.
5. `print_plates` {\"path\": \"/abs/plate.3mf\"} — arranges the pieces on beds and writes one file per plate (optional
   hollowing with `hollow_wall` and a drain hole).
Or all at once: `print_prepare` (check → cut if needed → joints → hollow → plates), which reports each step.",
    ),
    (
        "projects",
        "Opening, saving and undoing",
        "\
- `project_new` starts an empty project. `project_open` opens a `.pinocchio` file. Both can be undone.
- `project_save` writes the project (to its own file, or to `path`). Saving to a new path needs `overwrite: true` \
if the file exists.
- `undo` / `redo` step through the last 20 changes made in this session. `status` lists them.
- The saved file opens in the Pinocchio app exactly like one saved from the app.",
    ),
];

/// Ejemplos de operaciones para `cad_add_feature`: (tipo, para qué, JSON).
/// Una prueba verifica que cada uno es una operación válida.
pub const FEATURE_EXAMPLES: &[(&str, &str, &str)] = &[
    ("extrude", "push a sketch into a solid (or cut with op cut)", r#"{"type":"extrude","sketch":0,"extent":{"type":"blind","distance":10},"op":"join"}"#),
    ("revolve", "spin a sketch around an axis", r#"{"type":"revolve","sketch":0,"axis":{"type":"z"},"angle":360}"#),
    ("primitive", "box, cylinder, cone, sphere, torus, bolt, nut, washer", r#"{"type":"primitive","shape":{"type":"box","dx":40,"dy":20,"dz":10,"centered":true},"origin":[0,0,0]}"#),
    ("fillet", "round edges", r#"{"type":"fillet","edges":[{"$edge":3}],"radius":2}"#),
    ("chamfer", "bevel edges", r#"{"type":"chamfer","edges":[{"$edge":3}],"distance":1}"#),
    ("shell", "hollow out, opening some faces", r#"{"type":"shell","faces":[{"$face":4}],"thickness":2}"#),
    ("hole", "drilled holes at the points (or circle centres) of a sketch; style simple, counterbore, countersink", r#"{"type":"hole","sketch":5,"diameter":6.6,"depth":{"type":"through_all"},"style":{"type":"counterbore","diameter":11,"depth":6.5}}"#),
    ("pattern (linear)", "repeat operations along a direction", r#"{"type":"pattern","features":[3],"pattern":{"type":"linear","direction":[1,0,0],"count":4,"spacing":15}}"#),
    ("pattern (circular)", "repeat operations around an axis", r#"{"type":"pattern","features":[3],"pattern":{"type":"circular","axis":{"type":"z"},"count":6,"angle":360}}"#),
    ("mirror", "mirror operations (empty list = the whole solid) across a plane", r#"{"type":"mirror","features":[],"plane":{"type":"yz"}}"#),
    ("draft", "tilt faces for moulding", r#"{"type":"draft","faces":[{"$face":2}],"neutral":{"type":"xy"},"angle":3}"#),
    ("split", "cut the solid with a plane, keep the side the normal points to", r#"{"type":"split","plane":{"type":"xy"},"flip":false}"#),
    ("transform", "move or rotate parts (empty list = all), optionally as a copy", r#"{"type":"transform","parts":[],"motion":{"type":"translate","vector":[0,0,10]},"copy":false}"#),
    ("boolean", "union, subtract or intersect parts (ids from cad_design)", r#"{"type":"boolean","op":"subtract","targets":[{"feature":1,"index":0}],"tools":[{"feature":4,"index":0}]}"#),
    ("move_face", "push or pull planar faces", r#"{"type":"move_face","faces":[{"$face":4}],"distance":2}"#),
    ("delete_face", "remove faces and heal (holes, fillets)", r#"{"type":"delete_face","faces":[{"$face":7}]}"#),
    ("plane", "reference plane: offset, angle, midplane, three points", r#"{"type":"plane","def":{"type":"offset","base":{"type":"xy"},"distance":25}}"#),
    ("axis", "reference axis", r#"{"type":"axis","def":{"type":"two_points","a":{"type":"at","point":[0,0,0]},"b":{"type":"at","point":[0,0,10]}}}"#),
    ("point", "reference point", r#"{"type":"point","def":{"type":"at","point":[10,0,5]}}"#),
    ("helix", "helical curve (path for springs and threads)", r#"{"type":"helix","axis":{"type":"z"},"radius":10,"pitch":3,"turns":5}"#),
    ("sweep", "move a profile sketch along a path sketch (or a helix: path {type: curve, feature: id})", r#"{"type":"sweep","sketch":2,"path":{"type":"sketch","sketch":1,"entities":[]}}"#),
    ("loft", "blend between profile sketches on different planes", r#"{"type":"loft","sections":[{"sketch":0},{"sketch":2}]}"#),
];

/// Ejemplos de restricciones del sketch: (tipo, qué hace, JSON). Validados en una prueba.
pub const CONSTRAINT_EXAMPLES: &[(&str, &str, &str)] = &[
    ("coincident", "two points at the same place", r#"{"type":"coincident","a":1,"b":2}"#),
    ("fixed", "a point at fixed sketch coordinates", r#"{"type":"fixed","point":1,"x":0,"y":0}"#),
    ("horizontal / vertical", "a line parallel to sketch x / y", r#"{"type":"horizontal","line":5}"#),
    ("horizontal_points / vertical_points", "two points at the same y / x", r#"{"type":"vertical_points","a":1,"b":2}"#),
    ("parallel / perpendicular / collinear", "between two lines", r#"{"type":"perpendicular","a":5,"b":6}"#),
    ("equal", "same length (lines) or same radius (circles, arcs)", r#"{"type":"equal","a":5,"b":7}"#),
    ("tangent", "line–circle/arc or circle–circle", r#"{"type":"tangent","a":5,"b":9}"#),
    ("concentric", "same centre", r#"{"type":"concentric","a":9,"b":10}"#),
    ("point_on_line / point_on_circle / point_on_curve", "a point lies on an entity", r#"{"type":"point_on_line","point":1,"line":5}"#),
    ("midpoint", "a point at the middle of a line", r#"{"type":"midpoint","point":1,"line":5}"#),
    ("symmetric", "two points mirrored across a line", r#"{"type":"symmetric","a":1,"b":2,"line":5}"#),
    ("distance", "distance between two points (mm)", r#"{"type":"distance","a":1,"b":2,"value":25}"#),
    ("horizontal_distance / vertical_distance", "b.x − a.x (or b.y − a.y) = value", r#"{"type":"horizontal_distance","a":1,"b":2,"value":25}"#),
    ("length", "length of a line", r#"{"type":"length","line":5,"value":40}"#),
    ("radius / diameter", "size of a circle or arc", r#"{"type":"diameter","entity":9,"value":8}"#),
    ("angle", "angle from line a to line b in degrees (counter-clockwise positive)", r#"{"type":"angle","a":5,"b":6,"degrees":30}"#),
    ("point_line_distance", "distance from a point to a line", r#"{"type":"point_line_distance","point":1,"line":5,"value":12}"#),
    ("lock", "freeze an entity where it is", r#"{"type":"lock","entity":5}"#),
];

/// Temas que se arman con datos (los ejemplos de arriba)
fn generated(name: &str) -> Option<String> {
    match name {
        "features" => {
            let mut out = String::from(
                "Operations for `cad_add_feature` (and the JSON `cad_design` {\"detail\": true} shows). Lengths mm, angles degrees. \
Faces/edges: {\"$face\": n} / {\"$edge\": n} with numbers from `cad_geometry`. Planes: {\"type\":\"xy\"|\"xz\"|\"yz\"}, \
{\"type\":\"face\",\"face\":{\"$face\":n}}, {\"type\":\"reference\",\"feature\":id}. Axes: {\"type\":\"x\"|\"y\"|\"z\"}, \
{\"type\":\"edge\",\"edge\":{\"$edge\":n}}, {\"type\":\"sketch_line\",\"sketch\":id,\"line\":entity}. Part ids: {\"feature\":id,\"index\":n}. \
`op`: join | cut | intersect | new.\n",
            );
            for (ty, what, json) in FEATURE_EXAMPLES {
                out.push_str(&format!("\n- {ty} — {what}:\n  {json}"));
            }
            Some(out)
        }
        "sketch" => {
            let mut out = String::from(
                "Sketch constraints for `sketch_create` / `sketch_add`. Ids are sketch point or entity ids (they share one \
numbering); in the same call you can use names like \"s0.top\", \"s1\", \"s1.center\" and \"origin\" instead. \
Distances mm, angles degrees. A dimension with \"reference\": true only measures (does not constrain).\n\
Tips: shapes drawn at exact coordinates already have their size; add constraints when the sketch must keep its \
shape when edited, e.g. tie a corner to \"origin\" and give length/diameter dimensions until it is fully defined. \
Conflicting constraints are reported with their indices; remove them with `sketch_add` {\"remove_constraints\": [i]}.\n\
Shapes add their own constraints first (a rectangle adds 4: horizontal/vertical), so your dimensions do not start at \
index 0: the result of `sketch_create` / `sketch_add` lists every dimension with its index and the formula path.\n",
            );
            for (ty, what, json) in CONSTRAINT_EXAMPLES {
                out.push_str(&format!("\n- {ty} — {what}: {json}"));
            }
            Some(out)
        }
        _ => None,
    }
}

/// Nombres y títulos de todos los temas
pub fn topic_list() -> Vec<(&'static str, &'static str)> {
    let mut list: Vec<(&str, &str)> = TOPICS.iter().map(|t| (t.0, t.1)).collect();
    list.push(("sketch", "Sketch constraints, with an example of each"));
    list.push(("features", "Every operation type for cad_add_feature, with an example of each"));
    list
}

/// El texto de un tema, o `None` si no existe
pub fn topic(name: &str) -> Option<String> {
    TOPICS.iter().find(|t| t.0 == name).map(|t| t.2.to_string()).or_else(|| generated(name))
}

/// Toda la guía en un texto (recurso `pinocchio://guide`)
pub fn full() -> String {
    let mut out = String::from("# Pinocchio guide\n\n");
    out.push_str(INSTRUCTIONS);
    for (name, title) in topic_list() {
        out.push_str(&format!("\n\n## {title}\n\n{}", topic(name).unwrap_or_default()));
    }
    out
}

/// La guía para un agente, en Markdown: convenciones, temas y cada herramienta
/// con sus argumentos (`pinocchio --mcp --guide`, y el descargable de Configuración)
pub fn agent_markdown() -> String {
    let mut out = full();
    out.push_str("\n\n## Tools\n");
    for tool in crate::mcp::tools::all() {
        out.push_str(&format!("\n### `{}` — {}\n\n{}\n", tool.name, tool.title, tool.description));
        let schema = (tool.input)();
        let required: Vec<&str> = schema["required"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).collect();
        let props = schema["properties"].as_object().cloned().unwrap_or_default();
        if props.is_empty() {
            out.push_str("\nNo arguments.\n");
            continue;
        }
        out.push_str("\nArguments:\n");
        for (name, p) in &props {
            let ty = match &p["type"] {
                serde_json::Value::String(t) => t.clone(),
                serde_json::Value::Array(ts) => ts.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join(" | "),
                _ => "value".into(),
            };
            let options = p["enum"].as_array().map(|o| format!(" One of: {}.", o.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))).unwrap_or_default();
            out.push_str(&format!(
                "- `{name}` ({ty}{}): {}{options}\n",
                if required.contains(&name.as_str()) { ", required" } else { "" },
                p["description"].as_str().unwrap_or("")
            ));
        }
    }
    out
}
