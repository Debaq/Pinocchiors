//! La guía para la IA: lo que dice `initialize`, el recurso `pinocchio://guide`
//! y la herramienta `help`. En inglés, como las descripciones.

/// Lo que se manda en `initialize.instructions`: lo justo para empezar bien
pub const INSTRUCTIONS: &str = "\
Pinocchio is a 3D app: parametric CAD (parts, assemblies, 2D drawings), mesh import/conversion/repair/remeshing, \
3D-print preparation, skeleton rigging, posing, animation and rendering.
Start with `status` to see what is loaded and which mode you are in. Call `help` with a topic for workflows.
Conventions: lengths in millimetres, angles in degrees, Z is up. Every change can be reverted with `undo`; \
a tool that fails changes nothing. Nothing is written to disk unless you call a save/export tool with a path.";

/// Temas de la guía: nombre, título y texto
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "overview",
        "What Pinocchio does and how to work with it",
        "\
Pinocchio works on one project at a time. A project holds:
- Design tabs: parametric CAD parts (sketches + operations such as extrude, revolve, fillet).
- Assembly tabs: instances of parts from design tabs, positioned with mates.
- Scene objects: polygon meshes (imported models or meshed CAD parts), with optional skeleton, weights and animation.

Typical loop:
1. `status` — see what is loaded.
2. Make changes with the area tools (each result tells you the ids and names it created).
3. Check the result (tools return a short summary; `view` returns an image when available).
4. `undo` if something went wrong; `project_save` when done.

Modes: `standalone` runs its own engine without a window. `live` drives the open Pinocchio app (the user sees every \
change and can undo it there too). `status` tells you which one is active.",
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

/// El texto de un tema, o `None` si no existe
pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS.iter().find(|t| t.0 == name).map(|t| t.2)
}

/// Toda la guía en un texto (recurso `pinocchio://guide`)
pub fn full() -> String {
    let mut out = String::from("# Pinocchio guide\n\n");
    out.push_str(INSTRUCTIONS);
    for (_, title, text) in TOPICS {
        out.push_str(&format!("\n\n## {title}\n\n{text}"));
    }
    out
}
