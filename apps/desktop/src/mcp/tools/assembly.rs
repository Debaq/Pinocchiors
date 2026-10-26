//! Ensambles: poner piezas de las pestañas de diseño, unirlas con relaciones
//! (mates), moverlas, revisar choques y la lista de materiales.

use std::collections::BTreeMap;

use cad_model::{Assembly, Document, Instance, Mate, MateKind, PartId, Relation};
use serde_json::{json, Value};

use super::cad::{find_part, find_tab, num, vec3};
use super::{arg_bool, arg_str, Ctx, Effect, Fail, Output, Tool};
use crate::mcp::schema::{any_object, boolean, find_id_or_name, id_or_name, integer, number, number_in, object, one_of, point, string, Prop};

pub const TOOLS: &[Tool] = &[INSERT, MATE, INSTANCE, MATE_EDIT, RELATION, CHECK];

/// Un ensamble abierto: su pestaña (ya activa) y su documento virtual
pub(super) struct Asm {
    pub tab: u32,
    pub tab_name: String,
    pub doc: Document,
}

impl Asm {
    pub fn assembly(&mut self) -> &mut Assembly {
        self.doc.assembly.get_or_insert_with(Assembly::default)
    }
}

pub(super) fn asm_tab_prop() -> Prop {
    ("tab", id_or_name("Assembly tab, by id or name. Default: the tab you worked on last (or the active one); it must be an assembly"), false)
}

/// Abre la pestaña de ensamble de `args.tab`
pub(super) fn open_assembly(ctx: &Ctx, args: &Value) -> Result<Asm, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    if kind != "assembly" {
        return Err(Fail::new(format!("tab \"{tab_name}\" is a design, not an assembly")).hint("create one with `cad_tab_create` {\"kind\": \"assembly\"} or pass an assembly tab"));
    }
    super::cad::focus(ctx, tab)?;
    let doc: Document = serde_json::from_value(ctx.call("cad_get_document", json!({}))?).map_err(|e| Fail::new(format!("could not read the assembly: {e}")))?;
    Ok(Asm { tab, tab_name, doc })
}

/// Guarda el ensamble (con sus planos tal cual) y lo recalcula
fn apply(ctx: &Ctx, a: &Asm) -> Result<Value, Fail> {
    ctx.call("cad_set_tab_document", json!({ "tab": a.tab, "document": a.doc }))
}

/// Estado del solver en palabras: (texto, resuelto)
pub(super) fn solution_text(result: &Value) -> (String, bool) {
    let sol = &result["assembly"]["solution"];
    if sol.is_null() {
        return ("No instances yet.".into(), true);
    }
    let ok = sol["converged"] == true && sol["residual"].as_f64().unwrap_or(1.0) < 1e-4;
    let dof = sol["dof"].as_u64().unwrap_or(0);
    (
        if ok {
            format!("Mates solved; {dof} degree(s) of freedom left{}.", if dof == 0 { " (fully positioned)" } else { " (parts can still move)" })
        } else {
            format!("Mates could NOT all be satisfied (residual {}).", num(sol["residual"].as_f64().unwrap_or(0.0)))
        },
        ok,
    )
}

/// Instancia por id o nombre
fn find_instance(asm: &Assembly, want: &Value) -> Result<u32, Fail> {
    let found = find_id_or_name(&asm.instances, want, |i| Some(i.id as u64), |i| &i.name).map(|i| &asm.instances[i]);
    found.map(|i| i.id).ok_or_else(|| {
        let list: Vec<String> = asm.instances.iter().map(|i| format!("{} \"{}\"", i.id, i.name)).collect();
        Fail::new(format!("no instance {want}")).hint(if list.is_empty() { "the assembly is empty: use `asm_insert`".into() } else { format!("instances: {}", list.join(", ")) })
    })
}

fn find_mate(asm: &Assembly, want: &Value) -> Result<u32, Fail> {
    let found = find_id_or_name(&asm.mates, want, |m| Some(m.id as u64), |m| &m.name).map(|i| &asm.mates[i]);
    found.map(|m| m.id).ok_or_else(|| {
        let list: Vec<String> = asm.mates.iter().map(|m| format!("{} \"{}\"", m.id, m.name)).collect();
        Fail::new(format!("no mate {want}")).hint(if list.is_empty() { "the assembly has no mates: use `asm_mate`".into() } else { format!("mates: {}", list.join(", ")) })
    })
}

/// Giro: {"axis": [x, y, z], "angle": grados} → vector de giro (radianes)
fn rotation(v: &Value) -> Result<[f64; 3], Fail> {
    let axis: Vec<f64> = v["axis"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let angle = v["angle"].as_f64().ok_or_else(|| Fail::new("`rotate` needs {\"axis\": [x, y, z], \"angle\": degrees}"))?;
    if axis.len() != 3 {
        return Err(Fail::new("`rotate.axis` must be [x, y, z]"));
    }
    let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if l < 1e-12 {
        return Err(Fail::new("`rotate.axis` cannot be [0, 0, 0]"));
    }
    let a = angle.to_radians() / l;
    Ok([axis[0] * a, axis[1] * a, axis[2] * a])
}

/// Vector de giro → «45° about (0, 0, 1)»
fn rotation_text(r: [f64; 3]) -> String {
    let th = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    if th < 1e-9 {
        return "not rotated".into();
    }
    format!("rotated {}° about {}", num(th.to_degrees()), vec3(&json!([r[0] / th, r[1] / th, r[2] / th])))
}

fn kind_name(k: MateKind) -> &'static str {
    match k {
        MateKind::Fastened => "fastened",
        MateKind::Revolute => "revolute",
        MateKind::Slider => "slider",
        MateKind::Cylindrical => "cylindrical",
        MateKind::Planar => "planar",
        MateKind::Spherical => "spherical",
        MateKind::Parallel => "parallel",
        MateKind::PinSlot => "pin_slot",
    }
}

/// Nombre de una pieza para la lista: el de su pestaña si el diseño tiene una
/// sola pieza; si no, «pestaña · pieza»
fn label(tab_name: &str, part_name: &str, parts_in_tab: usize) -> String {
    if parts_in_tab <= 1 { tab_name.to_string() } else { format!("{tab_name} · {part_name}") }
}

/// Nombre de la pieza de cada instancia (de su pestaña de diseño), con caché por pestaña
fn part_names(ctx: &Ctx, asm: &Assembly) -> BTreeMap<(u32, u32, u32), String> {
    let tab_names: BTreeMap<u64, String> = ctx
        .call("cad_tabs", json!({}))
        .unwrap_or(Value::Null)["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| Some((t["id"].as_u64()?, t["name"].as_str()?.to_string())))
        .collect();
    let mut names = BTreeMap::new();
    let mut tabs: BTreeMap<u32, Value> = BTreeMap::new();
    for inst in &asm.instances {
        let (tab, f) = cad_model::split_tab_feature(inst.part.feature);
        let home = if tab >= cad_model::VARIANT_BASE { inst.studio } else { tab };
        let list = tabs.entry(home).or_insert_with(|| ctx.call("cad_tab_parts", json!({ "id": home })).unwrap_or(Value::Null));
        let all = list.as_array().cloned().unwrap_or_default();
        let part = all.iter().find(|p| p["id"]["feature"] == f.0 && p["id"]["index"] == inst.part.index).and_then(|p| p["name"].as_str()).unwrap_or(&inst.name).to_string();
        let tab_name = tab_names.get(&(home as u64)).cloned().unwrap_or_else(|| format!("tab {home}"));
        names.insert((tab, f.0, inst.part.index), label(&tab_name, &part, all.len()));
    }
    names
}

/// Una fila de la lista de materiales
pub(super) struct BomRow {
    pub name: String,
    /// Pestaña de diseño de la pieza
    pub tab: u32,
    /// La pieza dentro de esa pestaña
    pub part: (u32, u32),
    pub configuration: Option<String>,
    pub quantity: usize,
}

impl BomRow {
    /// ¿Es la pieza de esta instancia?
    pub fn holds(&self, inst: &Instance) -> bool {
        let (t, f) = cad_model::split_tab_feature(inst.part.feature);
        let t = if t >= cad_model::VARIANT_BASE { inst.studio } else { t };
        t == self.tab && (f.0, inst.part.index) == self.part && inst.configuration == self.configuration
    }

    pub fn json(&self) -> Value {
        json!({ "part": self.name, "tab": self.tab, "configuration": self.configuration, "quantity": self.quantity })
    }

    pub fn text(&self) -> String {
        format!("{} × \"{}\" (design tab {}{})", self.quantity, self.name, self.tab, self.configuration.as_ref().map_or(String::new(), |c| format!(", configuration {c}")))
    }
}

/// Lista de materiales: una fila por pieza (y configuración), con la cantidad
pub(super) fn bom(ctx: &Ctx, asm: &Assembly) -> Vec<BomRow> {
    let names = part_names(ctx, asm);
    // (pestaña, operación, índice de pieza, configuración) → (nombre, cantidad)
    type Key = (u32, u32, u32, Option<String>);
    let mut rows: BTreeMap<Key, (String, usize)> = BTreeMap::new();
    for inst in &asm.instances {
        let (tab, f) = cad_model::split_tab_feature(inst.part.feature);
        let name = names.get(&(tab, f.0, inst.part.index)).cloned().unwrap_or_default();
        // Una variante se cuenta en la pestaña de su diseño
        let tab = if tab >= cad_model::VARIANT_BASE { inst.studio } else { tab };
        rows.entry((tab, f.0, inst.part.index, inst.configuration.clone())).or_insert((name, 0)).1 += 1;
    }
    rows.into_iter().map(|((tab, f, index, configuration), (name, quantity))| BomRow { name, tab, part: (f, index), configuration, quantity }).collect()
}

/// El ensamble en palabras (para `cad_design` de una pestaña de ensamble)
pub(super) fn describe(ctx: &Ctx, a: &Asm, result: &Value) -> Vec<String> {
    let asm = a.doc.assembly.clone().unwrap_or_default();
    let poses: BTreeMap<u64, (Value, Value)> = result["assembly"]["solution"]["poses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_u64()?, (p[1].clone(), p[2].clone()))))
        .collect();
    let mut lines = vec![format!("Assembly tab \"{}\" (id {}): {} instance(s), {} mate(s).", a.tab_name, a.tab, asm.instances.len(), asm.mates.len())];
    let names = part_names(ctx, &asm);
    for i in &asm.instances {
        let (tab, f) = cad_model::split_tab_feature(i.part.feature);
        let part = names.get(&(tab, f.0, i.part.index)).cloned().unwrap_or_default();
        let (pos, rot) = poses.get(&(i.id as u64)).cloned().unwrap_or((json!(i.position), json!(i.rotation)));
        let rot: [f64; 3] = serde_json::from_value(rot).unwrap_or_default();
        lines.push(format!(
            "  instance {} \"{}\": part \"{part}\" from tab {}{}; at {}, {}{}",
            i.id,
            i.name,
            if tab >= cad_model::VARIANT_BASE { i.studio } else { tab },
            i.configuration.as_ref().map_or(String::new(), |c| format!(" (configuration {c})")),
            vec3(&pos),
            rotation_text(rot),
            if i.fixed { ", FIXED" } else { "" }
        ));
    }
    for m in &asm.mates {
        let extra = [m.angle.map(|v| format!("angle {}°", num(v))), m.distance.map(|v| format!("distance {} mm", num(v))), m.flip.then(|| "flipped".to_string())]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        lines.push(format!(
            "  mate {} \"{}\": {} between instance {} and {}{}",
            m.id,
            m.name,
            kind_name(m.kind),
            m.a.instance,
            m.b.instance,
            if extra.is_empty() { String::new() } else { format!(" ({})", extra.join(", ")) }
        ));
    }
    for (k, r) in asm.relations.iter().enumerate() {
        lines.push(format!("  relation {k}: {}", relation_text(r)));
    }
    lines.push(solution_text(result).0);
    let rows = bom(ctx, &asm);
    if !rows.is_empty() {
        let list: Vec<String> = rows.iter().map(BomRow::text).collect();
        lines.push(format!("Bill of materials: {}.", list.join("; ")));
    }
    lines
}

// ─── Poner piezas ───────────────────────────────────────────────────────────

const INSERT: Tool = Tool {
    name: "asm_insert",
    title: "Put parts into an assembly",
    description: "Inserts parts from a design tab (`from`) into an assembly tab as instances. Without `part` it inserts \
every part of that design. Another assembly tab as `from` inserts it whole as one rigid subassembly. `at` [x, y, z] \
mm places it (default: to the right of what is already there); `rotate` {\"axis\": [x, y, z], \"angle\": degrees}; \
`count` inserts several copies side by side; `fixed: true` anchors it (otherwise the first instance stays put and the \
rest follow the mates). Returns the instance ids. Example: {\"tab\": \"Assembly 1\", \"from\": \"Shaft\", \"count\": 2}.",
    input: || {
        object(&[
            asm_tab_prop(),
            ("from", id_or_name("Design tab (or assembly tab, as a subassembly) the parts come from"), true),
            ("part", string("Only this part of that design: \"feature:index\" or its name (default: all parts)"), false),
            ("configuration", string("Configuration (variant) of that design, by name"), false),
            ("at", point("Position [x, y, z] in mm", 3), false),
            ("rotate", any_object("{\"axis\": [x, y, z], \"angle\": degrees}"), false),
            ("count", integer("How many copies (default 1)", 1), false),
            ("fixed", boolean("Anchor it so mates never move it"), false),
            ("name", string("Instance name (copies get a number)"), false),
        ])
    },
    effect: Effect::Change,
    run: insert,
};

fn insert(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (from, from_name, from_kind) = find_tab(ctx, args.get("from"), false)?;
    let mut a = open_assembly(ctx, args)?;
    if from == a.tab {
        return Err(Fail::new("an assembly cannot contain itself"));
    }
    // Qué piezas: (id local, nombre)
    let parts: Vec<(PartId, String)> = if from_kind == "assembly" {
        vec![(cad_model::SUBASSEMBLY_PART, from_name.clone())]
    } else {
        let list = ctx.call("cad_tab_parts", json!({ "id": from }))?;
        let mut parts: Vec<(PartId, String)> = list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((serde_json::from_value::<PartId>(p["id"].clone()).ok()?, p["name"].as_str().unwrap_or("").to_string())))
            .collect();
        let total = parts.len();
        if let Some(want) = arg_str(args, "part") {
            let id: PartId = serde_json::from_value(find_part(&list, want)?).map_err(|e| Fail::new(e.to_string()))?;
            parts.retain(|p| p.0 == id);
        }
        if parts.is_empty() {
            return Err(Fail::new(format!("design \"{from_name}\" has no parts yet")).hint("make a solid in it first"));
        }
        // Con una sola pieza, el nombre de la pestaña; con varias, «pestaña · pieza»
        parts.iter_mut().for_each(|p| p.1 = label(&from_name, &p.1, total));
        parts
    };
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1).min(100) as usize;
    let rot = match args.get("rotate") {
        Some(r) => rotation(r)?,
        None => [0.0; 3],
    };
    // Dónde: lo pedido, o a la derecha de lo que ya hay
    let mut at: [f64; 3] = match args.get("at") {
        Some(p) => serde_json::from_value(p.clone()).map_err(|e| Fail::new(e.to_string()))?,
        None if a.doc.assembly.as_ref().is_some_and(|x| !x.instances.is_empty()) => {
            let info = ctx.call("cad_drawing_info", json!({})).unwrap_or(Value::Null);
            [info["max"][0].as_f64().map_or(0.0, |x| x + 20.0), 0.0, 0.0]
        }
        None => [0.0; 3],
    };
    let step = 40.0;
    let fixed = arg_bool(args, "fixed").unwrap_or(false);
    let configuration = arg_str(args, "configuration").map(str::to_string);
    let mut created = Vec::new();
    {
        let asm = a.assembly();
        for _ in 0..count {
            for (part, pname) in &parts {
                let id = asm.next_id.max(asm.instances.iter().map(|i| i.id + 1).max().unwrap_or(0)).max(asm.mates.iter().map(|m| m.id + 1).max().unwrap_or(0));
                asm.next_id = id + 1;
                let base = arg_str(args, "name").map(str::to_string).unwrap_or_else(|| pname.clone());
                let same = asm.instances.iter().filter(|i| i.name.strip_prefix(&base).is_some_and(|r| r.trim().parse::<u32>().is_ok())).count();
                let name = format!("{base} {}", same + 1);
                asm.instances.push(Instance {
                    id,
                    studio: from,
                    configuration: configuration.clone(),
                    part: PartId { feature: cad_model::tab_feature(from, part.feature), index: part.index },
                    name: name.clone(),
                    position: at,
                    rotation: rot,
                    fixed,
                });
                created.push(json!({ "instance": id, "name": name }));
            }
            at[0] += step;
        }
    }
    let result = apply(ctx, &a)?;
    let list: Vec<String> = created.iter().map(|c| format!("{} \"{}\"", c["instance"], c["name"].as_str().unwrap_or(""))).collect();
    Ok(Output::new(
        format!(
            "Inserted into \"{}\" (tab {}): instance {}.\n{}\nNext: `view` to see it, `asm_mate` to connect instances (face numbers from `view` / `cad_geometry` on this tab).",
            a.tab_name,
            a.tab,
            list.join(", "),
            solution_text(&result).0
        ),
        json!({ "tab": a.tab, "instances": created, "solution": result["assembly"]["solution"] }),
    ))
}

// ─── Relaciones ─────────────────────────────────────────────────────────────

const KINDS: &[&str] = &["fastened", "revolute", "slider", "cylindrical", "planar", "spherical", "parallel", "pin_slot"];

const MATE: Tool = Tool {
    name: "asm_mate",
    title: "Connect two instances (mate)",
    description: "Connects two instances by picking a face on each (face numbers from `view` or `cad_geometry` on the \
assembly tab). A planar face gives its centre and normal; a cylindrical/conical face gives its axis. The second \
instance moves to satisfy it. Kinds: fastened (glued, 0 free), revolute (hinge about the common axis), slider (slides \
along it), cylindrical (slides and turns), planar (stays on the plane), spherical (ball joint), parallel, pin_slot. \
For faces that should touch face-to-face use `flip: true`. `angle`/`distance` drive revolute/slider/cylindrical \
mates. Example — a shaft in a hole: {\"kind\": \"cylindrical\", \"a\": 12, \"b\": 30}.",
    input: || {
        object(&[
            asm_tab_prop(),
            ("kind", one_of("Kind of joint", KINDS), true),
            ("a", integer("Face on the first instance (assembly face number)", 0), true),
            ("b", integer("Face on the second instance (it moves)", 0), true),
            ("flip", boolean("Reverse the second axis (faces facing each other)"), false),
            ("angle", number("Imposed angle in degrees (revolute, cylindrical)"), false),
            ("distance", number("Imposed distance along the axis in mm (slider, cylindrical)"), false),
            ("name", string("Mate name (optional)"), false),
        ])
    },
    effect: Effect::Change,
    run: mate,
};

fn mate(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut a = open_assembly(ctx, args)?;
    let connector = |face: &Value| -> Result<cad_model::Connector, Fail> {
        let c = ctx.call("cad_assembly_connector", json!({ "face": face })).map_err(|e| e.hint("face numbers come from `view` or `cad_geometry` on the assembly tab"))?;
        serde_json::from_value(c).map_err(|e| Fail::new(e.to_string()))
    };
    let (ca, cb) = (connector(&args["a"])?, connector(&args["b"])?);
    if ca.instance == cb.instance {
        return Err(Fail::new(format!("faces {} and {} are on the same instance ({})", args["a"], args["b"], ca.instance)).hint("pick one face on each of two different instances"));
    }
    let kind: MateKind = serde_json::from_value(args["kind"].clone()).map_err(|e| Fail::new(e.to_string()))?;
    let asm = a.assembly();
    let id = asm.next_id.max(asm.instances.iter().map(|i| i.id + 1).max().unwrap_or(0)).max(asm.mates.iter().map(|m| m.id + 1).max().unwrap_or(0));
    asm.next_id = id + 1;
    let n = asm.mates.iter().filter(|m| m.kind == kind).count() + 1;
    let name = arg_str(args, "name").map(str::to_string).unwrap_or_else(|| format!("{} {n}", kind_name(kind)));
    asm.mates.push(Mate {
        id,
        name: name.clone(),
        kind,
        a: ca,
        b: cb,
        flip: arg_bool(args, "flip").unwrap_or(false),
        angle: args.get("angle").and_then(Value::as_f64),
        distance: args.get("distance").and_then(Value::as_f64),
    });
    let result = apply(ctx, &a)?;
    let (text, ok) = solution_text(&result);
    if !ok {
        return Err(Fail::new(format!("the mate cannot be satisfied with the others: {text}"))
            .hint("nothing was changed; try `flip: true`, other faces, or a looser kind (e.g. cylindrical instead of fastened); check `fixed` instances"));
    }
    Ok(Output::new(
        format!("Added mate {id} \"{name}\" ({}) between instance {} and {}.\n{text}", kind_name(kind), ca.instance, cb.instance),
        json!({ "tab": a.tab, "mate": id, "name": name, "solution": result["assembly"]["solution"] }),
    ))
}

// ─── Mover, fijar, borrar ───────────────────────────────────────────────────

const INSTANCE: Tool = Tool {
    name: "asm_instance",
    title: "Move, fix, rename or remove an instance",
    description: "Changes one instance of an assembly: `at` [x, y, z] mm and/or `rotate` {\"axis\": [x, y, z], \
\"angle\": degrees} set its placement (mates still apply to instances that are not fixed), `fixed`, `name`, \
`configuration`, or `remove: true` (its mates go too). Example: {\"instance\": \"Shaft 1\", \"at\": [0, 0, 25]}.",
    input: || {
        object(&[
            asm_tab_prop(),
            ("instance", id_or_name("Instance id or name"), true),
            ("at", point("New position [x, y, z] in mm", 3), false),
            ("rotate", any_object("New rotation {\"axis\": [x, y, z], \"angle\": degrees} (absolute)"), false),
            ("fixed", boolean("Anchor it (true) or let mates move it (false)"), false),
            ("name", string("New name"), false),
            ("configuration", string("Configuration (variant) of its design; \"\" = the base one"), false),
            ("remove", boolean("Delete the instance and its mates"), false),
        ])
    },
    effect: Effect::Change,
    run: instance,
};

fn instance(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut a = open_assembly(ctx, args)?;
    let asm = a.assembly();
    let id = find_instance(asm, &args["instance"])?;
    let mut done = Vec::new();
    if arg_bool(args, "remove") == Some(true) {
        let before = asm.mates.len();
        asm.instances.retain(|i| i.id != id);
        asm.mates.retain(|m| m.a.instance != id && m.b.instance != id);
        done.push(format!("removed (and {} mate(s))", before - asm.mates.len()));
    } else {
        let rot = args.get("rotate").map(rotation).transpose()?;
        let at: Option<[f64; 3]> = args.get("at").map(|p| serde_json::from_value(p.clone())).transpose().map_err(|e| Fail::new(e.to_string()))?;
        let inst = asm.instances.iter_mut().find(|i| i.id == id).unwrap();
        if let Some(p) = at {
            inst.position = p;
            done.push(format!("moved to {}", vec3(&json!(p))));
        }
        if let Some(r) = rot {
            inst.rotation = r;
            done.push(rotation_text(r));
        }
        if let Some(f) = arg_bool(args, "fixed") {
            inst.fixed = f;
            done.push(if f { "fixed".into() } else { "free".into() });
        }
        if let Some(n) = arg_str(args, "name") {
            inst.name = n.trim().to_string();
            done.push(format!("renamed \"{}\"", inst.name));
        }
        if let Some(c) = arg_str(args, "configuration") {
            inst.configuration = (!c.trim().is_empty()).then(|| c.trim().to_string());
            done.push(format!("configuration {}", if c.trim().is_empty() { "base" } else { c.trim() }));
        }
    }
    if done.is_empty() {
        return Err(Fail::new("nothing to change").hint("pass `at`, `rotate`, `fixed`, `name`, `configuration` or `remove`"));
    }
    let result = apply(ctx, &a)?;
    let (text, _) = solution_text(&result);
    Ok(Output::new(format!("Instance {id}: {}.\n{text}", done.join(", ")), json!({ "tab": a.tab, "instance": id, "solution": result["assembly"]["solution"] })))
}

const MATE_EDIT: Tool = Tool {
    name: "asm_mate_edit",
    title: "Change or remove a mate",
    description: "Changes a mate: `angle` / `distance` (drive the joint; `null` frees it), `flip`, `kind`, `name`, or \
`remove: true`. Example: {\"mate\": \"revolute 1\", \"angle\": 90}.",
    input: || {
        object(&[
            asm_tab_prop(),
            ("mate", id_or_name("Mate id or name"), true),
            ("angle", json!({ "type": ["number", "null"], "description": "Imposed angle in degrees; null = free" }), false),
            ("distance", json!({ "type": ["number", "null"], "description": "Imposed distance in mm; null = free" }), false),
            ("flip", boolean("Reverse the second axis"), false),
            ("kind", one_of("New kind", KINDS), false),
            ("name", string("New name"), false),
            ("remove", boolean("Delete the mate"), false),
        ])
    },
    effect: Effect::Change,
    run: mate_edit,
};

fn mate_edit(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut a = open_assembly(ctx, args)?;
    let asm = a.assembly();
    let id = find_mate(asm, &args["mate"])?;
    let mut done = Vec::new();
    if arg_bool(args, "remove") == Some(true) {
        asm.mates.retain(|m| m.id != id);
        asm.relations.retain(|r| !r.mates().contains(&id));
        done.push("removed".to_string());
    } else {
        let m = asm.mates.iter_mut().find(|m| m.id == id).unwrap();
        if let Some(v) = args.get("angle") {
            m.angle = v.as_f64();
            done.push(m.angle.map_or("angle free".into(), |x| format!("angle {}°", num(x))));
        }
        if let Some(v) = args.get("distance") {
            m.distance = v.as_f64();
            done.push(m.distance.map_or("distance free".into(), |x| format!("distance {} mm", num(x))));
        }
        if let Some(f) = arg_bool(args, "flip") {
            m.flip = f;
            done.push(format!("flip {f}"));
        }
        if let Some(k) = args.get("kind") {
            m.kind = serde_json::from_value(k.clone()).map_err(|e| Fail::new(e.to_string()))?;
            done.push(format!("kind {}", kind_name(m.kind)));
        }
        if let Some(n) = arg_str(args, "name") {
            m.name = n.trim().to_string();
            done.push(format!("renamed \"{}\"", m.name));
        }
    }
    if done.is_empty() {
        return Err(Fail::new("nothing to change").hint("pass `angle`, `distance`, `flip`, `kind`, `name` or `remove`"));
    }
    let result = apply(ctx, &a)?;
    let (text, ok) = solution_text(&result);
    if !ok {
        return Err(Fail::new(format!("after that change the mates cannot be satisfied: {text}")).hint("nothing was changed; try another value or `flip`"));
    }
    Ok(Output::new(format!("Mate {id}: {}.\n{text}", done.join(", ")), json!({ "tab": a.tab, "mate": id, "solution": result["assembly"]["solution"] })))
}

// ─── Engranes, cremallera y tornillo ────────────────────────────────────────

fn relation_text(r: &Relation) -> String {
    match r {
        Relation::Gear { a, b, ratio, phase, carrier, carrier_ratio } => format!(
            "gear: mate {b} turns {} × mate {a}{}{}",
            num(*ratio),
            if *phase != 0.0 { format!(" + {}°", num(*phase)) } else { String::new() },
            carrier.map_or(String::new(), |c| format!(" + {} × carrier mate {c}", num(*carrier_ratio)))
        ),
        Relation::RackPinion { revolute, slider, per_turn, .. } => format!("rack and pinion: mate {slider} slides {} mm per turn of mate {revolute}", num(*per_turn)),
        Relation::Screw { cylindrical, pitch, .. } => format!("screw: mate {cylindrical} advances {} mm per turn", num(*pitch)),
    }
}

const RELATION: Tool = Tool {
    name: "asm_relation",
    title: "Couple joints: gears, rack and pinion, screw",
    description: "Couples the motion of existing mates so driving one moves the other. `gear`: two revolute mates, \
`ratio` = turns of `b` per turn of `a` (negative when they turn opposite ways, as external gears do: -z_a/z_b). \
`rack_pinion`: a revolute (`a`) and a slider (`b`), `per_turn` mm the rack moves per pinion turn (π × pitch \
diameter). `screw`: one cylindrical mate (`a`), `pitch` mm per turn. `auto: true` adds the couplings Pinocchio finds \
between meshing gear parts (from their teeth). `remove` deletes relation number n (from `cad_design`). Then \
`asm_mate_edit` with `angle` on one mate drives the rest. Example: {\"kind\": \"gear\", \"a\": \"revolute 1\", \"b\": \
\"revolute 2\", \"ratio\": -0.5}.",
    input: || {
        object(&[
            asm_tab_prop(),
            ("kind", one_of("What to couple", &["gear", "rack_pinion", "screw"]), false),
            ("a", id_or_name("Mate: the driving revolute (gear, rack_pinion) or the cylindrical (screw)"), false),
            ("b", id_or_name("Mate: the driven revolute (gear) or the slider (rack_pinion)"), false),
            ("ratio", number("gear: turns of b per turn of a (negative = opposite directions)"), false),
            ("per_turn", number("rack_pinion: mm per turn"), false),
            ("pitch", number("screw: mm per turn"), false),
            ("phase", number("Offset: degrees (gear) or mm (rack_pinion, screw) at angle 0 (default 0)"), false),
            ("auto", boolean("Add the couplings found between meshing gear parts"), false),
            ("remove", integer("Delete relation number n", 0), false),
        ])
    },
    effect: Effect::Change,
    run: relation,
};

fn relation(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let mut a = open_assembly(ctx, args)?;
    let mut done = Vec::new();
    if let Some(n) = args["remove"].as_u64() {
        let asm = a.assembly();
        if n as usize >= asm.relations.len() {
            return Err(Fail::new(format!("there is no relation {n} (the assembly has {})", asm.relations.len())));
        }
        let r = asm.relations.remove(n as usize);
        done.push(format!("removed {}", relation_text(&r)));
    }
    if arg_bool(args, "auto") == Some(true) {
        let found: Vec<Relation> = serde_json::from_value(ctx.call("cad_assembly_suggest", json!({}))?).map_err(|e| Fail::new(e.to_string()))?;
        if found.is_empty() {
            done.push("no new gear couplings found (the gears need revolute mates on their axes and must mesh)".into());
        }
        for r in found {
            done.push(format!("added {}", relation_text(&r)));
            a.assembly().relations.push(r);
        }
    }
    if let Some(kind) = arg_str(args, "kind") {
        let asm = a.assembly();
        let want = |k: &str| args.get(k).ok_or_else(|| Fail::new(format!("{kind} needs `{k}`")));
        let mate_of = |k: &str, need: &[MateKind]| -> Result<u32, Fail> {
            let id = find_mate(asm, want(k)?)?;
            let m = asm.mates.iter().find(|m| m.id == id).unwrap();
            if !need.contains(&m.kind) {
                let names: Vec<&str> = need.iter().map(|k| kind_name(*k)).collect();
                return Err(Fail::new(format!("mate {id} \"{}\" is {}, {kind} needs {}", m.name, kind_name(m.kind), names.join(" or "))).hint("make the joint with `asm_mate` first (a revolute on each gear axis, a slider for a rack, a cylindrical for a screw)"));
            }
            Ok(id)
        };
        let phase = args["phase"].as_f64().unwrap_or(0.0);
        let r = match kind {
            "gear" => {
                let (ma, mb) = (mate_of("a", &[MateKind::Revolute, MateKind::Cylindrical])?, mate_of("b", &[MateKind::Revolute, MateKind::Cylindrical])?);
                if ma == mb {
                    return Err(Fail::new("`a` and `b` are the same mate"));
                }
                let ratio = want("ratio")?.as_f64().filter(|r| *r != 0.0).ok_or_else(|| Fail::new("`ratio` must be a number other than 0"))?;
                Relation::Gear { a: ma, b: mb, ratio, phase, carrier: None, carrier_ratio: 0.0 }
            }
            "rack_pinion" => {
                let per_turn = want("per_turn")?.as_f64().filter(|r| *r != 0.0).ok_or_else(|| Fail::new("`per_turn` must be a number other than 0"))?;
                Relation::RackPinion { revolute: mate_of("a", &[MateKind::Revolute, MateKind::Cylindrical])?, slider: mate_of("b", &[MateKind::Slider, MateKind::Cylindrical])?, per_turn, phase }
            }
            _ => {
                let pitch = want("pitch")?.as_f64().filter(|r| *r != 0.0).ok_or_else(|| Fail::new("`pitch` must be a number other than 0"))?;
                Relation::Screw { cylindrical: mate_of("a", &[MateKind::Cylindrical])?, pitch, phase }
            }
        };
        done.push(format!("added {}", relation_text(&r)));
        asm.relations.push(r);
    }
    if done.is_empty() {
        return Err(Fail::new("nothing to do").hint("pass `kind` with its mates, `auto: true` or `remove`"));
    }
    let result = apply(ctx, &a)?;
    let (text, ok) = solution_text(&result);
    if !ok {
        return Err(Fail::new(format!("with that coupling the mates cannot be satisfied: {text}")).hint("nothing was changed; check the ratio sign, or free an imposed angle with `asm_mate_edit`"));
    }
    let warnings = ctx.call("cad_assembly_gear_warnings", json!({})).ok().and_then(|w| w.as_array().cloned()).unwrap_or_default();
    let mut lines = vec![format!("{}.", done.join("; ")), text];
    for w in warnings.iter().filter_map(Value::as_str) {
        lines.push(format!("Warning: {w}"));
    }
    Ok(Output::new(lines.join("\n"), json!({ "tab": a.tab, "relations": a.doc.assembly.as_ref().map(|x| x.relations.clone()), "solution": result["assembly"]["solution"] })))
}

// ─── Revisar ────────────────────────────────────────────────────────────────

const CHECK: Tool = Tool {
    name: "asm_check",
    title: "Check an assembly",
    description: "Checks an assembly: whether the mates are satisfied and how many degrees of freedom are left, which \
instances collide (shared volume in mm³), which are closer than `min_gap` mm (or the printer clearance of the \
design), gear warnings, and the bill of materials (parts and quantities). On a design tab it checks the parts of \
that design against each other instead (collisions and clearance; there are no mates). Example: {\"min_gap\": 0.2}.",
    input: || {
        object(&[
            ("tab", id_or_name("Tab to check, by id or name: an assembly, or a design with several parts. Default: the tab you worked on last (or the active one)"), false),
            ("min_gap", number_in("Report pairs closer than this (mm)", 0.0, 100.0), false),
        ])
    },
    effect: Effect::Read,
    run: check,
};

fn check(ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
    let (tab, tab_name, kind) = find_tab(ctx, args.get("tab"), false)?;
    if kind != "assembly" {
        return check_design(ctx, args, tab, &tab_name);
    }
    let a = open_assembly(ctx, args)?;
    let asm = a.doc.assembly.clone().unwrap_or_default();
    if asm.instances.is_empty() {
        return Err(Fail::new(format!("assembly \"{}\" is empty", a.tab_name)).hint("insert parts with `asm_insert`"));
    }
    let result = ctx.call("cad_evaluate", json!({}))?;
    let name = |id: u64| asm.instances.iter().find(|i| i.id as u64 == id).map_or(format!("{id}"), |i| format!("{id} \"{}\"", i.name));
    let mut lines = vec![format!("Assembly \"{}\" (tab {}): {} instance(s), {} mate(s).", a.tab_name, a.tab, asm.instances.len(), asm.mates.len()), solution_text(&result).0];
    let clashes = ctx.call("cad_assembly_interference", json!({}))?;
    let clash_list: Vec<String> = clashes.as_array().into_iter().flatten().map(|c| format!("{} with {} ({} mm³)", name(c[0].as_u64().unwrap_or(0)), name(c[1].as_u64().unwrap_or(0)), num(c[2].as_f64().unwrap_or(0.0)))).collect();
    lines.push(if clash_list.is_empty() { "No collisions.".into() } else { format!("COLLISIONS: {}.", clash_list.join("; ")) });
    let gaps = ctx.call("cad_assembly_clearance", json!({ "min_gap": args.get("min_gap") }));
    match gaps {
        Ok(g) => {
            let list: Vec<String> = g.as_array().into_iter().flatten().map(|c| format!("{} and {} ({} mm)", name(c[0].as_u64().unwrap_or(0)), name(c[1].as_u64().unwrap_or(0)), num(c[2].as_f64().unwrap_or(0.0)))).collect();
            lines.push(if list.is_empty() { "No pairs closer than the clearance.".into() } else { format!("Too close: {}.", list.join("; ")) });
        }
        Err(_) if args.get("min_gap").is_none() => lines.push("Clearance not checked (pass `min_gap`, or set a printer in the design).".into()),
        Err(e) => return Err(e),
    }
    let warnings = ctx.call("cad_assembly_gear_warnings", json!({})).unwrap_or(Value::Null);
    for w in warnings.as_array().into_iter().flatten().filter_map(Value::as_str) {
        lines.push(format!("Gear warning: {w}"));
    }
    let rows = bom(ctx, &asm);
    lines.push("Bill of materials:".into());
    for (k, row) in rows.iter().enumerate() {
        lines.push(format!("  {}. {}", k + 1, row.text()));
    }
    Ok(Output::new(
        lines.join("\n"),
        json!({
            "tab": a.tab,
            "solution": result["assembly"]["solution"],
            "collisions": clashes,
            "bom": rows.iter().map(BomRow::json).collect::<Vec<_>>(),
        }),
    ))
}

/// `asm_check` sobre una pestaña de diseño: sus piezas, una contra otra
fn check_design(ctx: &Ctx, args: &Value, tab: u32, tab_name: &str) -> Result<Output, Fail> {
    super::cad::focus(ctx, tab)?;
    let parts = ctx.call("cad_evaluate", json!({}))?["parts"].clone();
    let count = parts.as_array().map_or(0, Vec::len);
    let name = |id: &Value| {
        let found = parts.as_array().into_iter().flatten().find(|p| p["id"] == *id);
        let n = found.and_then(|p| p["name"].as_str()).unwrap_or("");
        format!("{}:{} \"{n}\"", id["feature"], id["index"])
    };
    let r = ctx.call("cad_part_interference", json!({ "min_gap": args.get("min_gap") }))?;
    let pairs = |key: &str, unit: &str| -> Vec<String> {
        r[key].as_array().into_iter().flatten().map(|c| format!("{} with {} ({} {unit})", name(&c[0]), name(&c[1]), num(c[2].as_f64().unwrap_or(0.0)))).collect()
    };
    let collisions = pairs("collisions", "mm³");
    let close = pairs("close", "mm");
    let mut lines = vec![format!("Design \"{tab_name}\" (tab {tab}): {count} part(s). This is a design, not an assembly: parts were checked against each other where they are modeled; there are no mates or degrees of freedom.")];
    lines.push(if collisions.is_empty() { "No collisions between parts.".into() } else { format!("COLLISIONS: {}.", collisions.join("; ")) });
    lines.push(match r["gap"].as_f64() {
        Some(g) if close.is_empty() => format!("No pairs closer than {} mm.", num(g)),
        Some(g) => format!("Closer than {} mm: {}.", num(g), close.join("; ")),
        None => "Clearance not checked (pass `min_gap`, or set a printer in the design).".into(),
    });
    if count > 1 {
        lines.push("To check motion (mates, degrees of freedom) make an assembly: `cad_tab_create` {\"kind\": \"assembly\"}, then `asm_insert` these parts.".into());
    }
    Ok(Output::new(lines.join("\n"), json!({ "tab": tab, "kind": "design", "parts": count, "collisions": r["collisions"], "close": r["close"], "gap": r["gap"] })))
}
