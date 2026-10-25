//! Diseñar una pieza de punta a punta por el conector, como lo haría una IA.

use std::sync::Mutex;

use pinocchio_mcp::backend::Local;
use pinocchio_mcp::guide;
use pinocchio_mcp::protocol;
use pinocchio_mcp::server::Server;
use pinocchio_mcp::session::Session;
use serde_json::{json, Value};

fn server() -> Server {
    Server::new(Session::new(Box::new(Local::new())))
}

/// (texto, datos, es error)
fn call(s: &Server, name: &str, args: Value) -> (String, Value, bool) {
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    let r = out.into_inner().unwrap().pop().unwrap();
    let result = &r["result"];
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["structuredContent"].clone(), result["isError"] == true)
}

fn ok(s: &Server, name: &str, args: Value) -> (String, Value) {
    let (text, data, err) = call(s, name, args);
    assert!(!err, "{name} falló:\n{text}");
    (text, data)
}

fn volume(data: &Value) -> f64 {
    data["body"]["volume"].as_f64().unwrap_or(0.0)
}

#[test]
fn design_a_plate_with_a_hole() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    // Placa de 40 × 20 centrada, con un agujero de Ø6 a 10 mm del centro, acotada
    let (text, data) = ok(
        &s,
        "sketch_create",
        json!({
            "plane": "xy",
            "name": "Base",
            "shapes": [
                { "type": "rectangle", "center": [0, 0], "width": 40, "height": 20 },
                { "type": "circle", "center": [10, 0], "diameter": 6 }
            ],
            "constraints": [
                { "type": "length", "line": "s0.bottom", "value": 40 },
                { "type": "length", "line": "s0.left", "value": 20 },
                { "type": "horizontal_distance", "a": "s0.bottom_left", "b": "origin", "value": 20 },
                { "type": "vertical_distance", "a": "s0.bottom_left", "b": "origin", "value": 10 },
                { "type": "diameter", "entity": "s1", "value": 6 },
                { "type": "horizontal_distance", "a": "origin", "b": "s1.center", "value": 10 },
                { "type": "vertical_distance", "a": "origin", "b": "s1.center", "value": 0 }
            ]
        }),
    );
    assert!(text.contains("s0.top=") && text.contains("s1.center="), "{text}");
    assert!(text.contains("Profile used by default (1 region(s))") && text.contains("1 hole"), "{text}");
    assert!(text.contains("fully defined"), "{text}");
    // Las cotas dicen su índice real (el rectángulo pone antes sus 4 restricciones)
    assert!(text.contains("[4] length = 40 → formula path \"0.sketch.constraints.4.value\""), "{text}");
    let sketch = data["feature"].as_u64().unwrap();

    // Extrusión de 10: 40·20·10 − π·3²·10
    let (_, data) = ok(&s, "cad_extrude", json!({ "sketch": "Base", "distance": 10 }));
    let expect = 8000.0 - std::f64::consts::PI * 9.0 * 10.0;
    assert!((volume(&data) - expect).abs() < 0.5, "volumen {} (esperado {expect})", volume(&data));
    let extrude = data["feature"].as_u64().unwrap();

    // La cara de arriba, por su orientación
    let (text, geo) = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "+Z" }));
    let faces = geo["faces"].as_array().unwrap();
    assert_eq!(faces.len(), 1, "{text}");
    assert!(text.contains("end"), "el origen de la tapa se nombra: {text}");
    let top = faces[0]["index"].as_u64().unwrap();

    // Redondeo del borde de arriba (incluye el del agujero): quita algo de volumen
    let before = volume(&data);
    let (_, data) = ok(&s, "cad_fillet", json!({ "faces": [top], "radius": 1 }));
    assert!(volume(&data) < before);

    // Un redondeo imposible falla y no deja nada
    let (text, _, err) = call(&s, "cad_fillet", json!({ "faces": [top], "radius": 50 }));
    assert!(err && text.contains("Hint:"), "{text}");
    let (text, _) = ok(&s, "cad_design", json!({}));
    assert_eq!(text.matches("fillet").count(), 1, "solo el primer redondeo:\n{text}");

    // Parámetro y fórmula: la extrusión pasa a 15
    let (text, _) = ok(&s, "cad_parameters", json!({ "set": { "h": "15" }, "formulas": { format!("{extrude}.extent.distance"): "h" } }));
    assert!(text.contains("h = 15"), "{text}");
    let (text, data) = ok(&s, "cad_design", json!({}));
    assert!(text.contains("Formulas:"), "{text}");
    assert!(data["body"]["bbox_max"][2].as_f64().unwrap() > 14.9, "{text}");

    // Un cilindro que corta: agujero pasante de Ø4
    let (_, data2) = ok(&s, "cad_primitive", json!({ "shape": "cylinder", "diameter": 4, "height": 40, "origin": [-10, 0, -5], "op": "cut" }));
    assert!(volume(&data2) < volume(&data));

    // Borrar el sketch pide borrar lo que lo usa
    let (text, _, err) = call(&s, "cad_delete_feature", json!({ "feature": sketch }));
    assert!(err && text.contains("with_dependents"), "{text}");

    // Cambiar una operación por su JSON
    let (text, _, err) = call(&s, "cad_edit_feature", json!({ "feature": extrude, "changes": { "reverse": true } }));
    assert!(!err, "{text}");
    let (_, data) = ok(&s, "cad_design", json!({}));
    assert!(data["body"]["bbox_min"][2].as_f64().unwrap() < -14.0, "la extrusión va hacia abajo");
    ok(&s, "undo", json!({}));

    // Exportar
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("placa.step");
    let (text, _) = ok(&s, "cad_export", json!({ "path": file }));
    assert!(file.metadata().unwrap().len() > 1000, "{text}");
    let (text, _, err) = call(&s, "cad_export", json!({ "path": file }));
    assert!(err && text.contains("overwrite"), "{text}");
}

#[test]
fn raw_features_and_references() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [30, 30, 10] }));
    // Simetría por JSON crudo, y una caja corrida para que se note
    ok(&s, "cad_add_feature", json!({ "feature": { "type": "primitive", "shape": { "type": "box", "dx": 10, "dy": 10, "dz": 10, "centered": true }, "origin": [30, 0, 0] } }));
    let (text, data) = ok(&s, "cad_add_feature", json!({ "feature": { "type": "mirror", "features": [], "plane": { "type": "yz" } }, "name": "Espejo" }));
    assert!(text.contains("\"Espejo\""), "{text}");
    assert!(data["body"]["bbox_min"][0].as_f64().unwrap() < -34.0, "{text}");

    // Vaciado abriendo la cara de arriba de la primera caja, elegida por número
    let (_, geo) = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "+Z" }));
    let top = geo["faces"][0]["index"].as_u64().unwrap();
    let (text, _, err) = call(&s, "cad_add_feature", json!({ "feature": { "type": "shell", "faces": [{ "$face": top }], "thickness": 1 } }));
    assert!(!err, "{text}");

    // Referencias que no existen y JSON inválido explican qué pasó
    let (text, _, err) = call(&s, "cad_add_feature", json!({ "feature": { "type": "fillet", "edges": [{ "$edge": 99999 }], "radius": 1 } }));
    assert!(err && text.contains("cad_geometry"), "{text}");
    let (text, _, err) = call(&s, "cad_add_feature", json!({ "feature": { "type": "extrudir" } }));
    assert!(err && text.contains("features"), "{text}");
    let (text, _, err) = call(&s, "sketch_create", json!({ "shapes": [{ "type": "circle", "center": [0, 0] }] }));
    assert!(err && text.contains("radius"), "{text}");
    let (text, _, err) = call(&s, "sketch_create", json!({ "shapes": [{ "type": "line", "from": [0, 0], "to": [5, 0] }], "constraints": [{ "type": "length", "line": "s9", "value": 3 }] }));
    assert!(err && text.contains("s0"), "nombra los que hay: {text}");
}

#[test]
fn tabs_by_name() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Tapa" }));
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Caja" }));
    ok(&s, "cad_primitive", json!({ "tab": "tapa", "shape": "sphere", "radius": 5 }));
    let (text, _) = ok(&s, "status", json!({}));
    assert!(text.contains("\"Tapa\"") && text.contains("1 operations"), "{text}");
    let (text, _, err) = call(&s, "cad_design", json!({ "tab": "Nada" }));
    assert!(err && text.contains("Tapa") && text.contains("Caja"), "{text}");
    ok(&s, "cad_tab_rename", json!({ "tab": "Caja", "name": "Base" }));
    ok(&s, "cad_tab_delete", json!({ "tab": "Base" }));
    let (_, data) = ok(&s, "status", json!({}));
    assert_eq!(data["workspace"]["design_tabs"].as_array().unwrap().len(), 1);
}

/// Cambia `$face`/`$edge` por referencias de mentira, para validar los ejemplos sin sólido
fn fake_refs(v: &mut Value) {
    match v {
        Value::Object(m) if m.contains_key("$face") => *v = json!({ "point": [0, 0, 0], "normal": [0, 0, 1] }),
        Value::Object(m) if m.contains_key("$edge") => *v = json!({ "point": [0, 0, 0], "direction": [1, 0, 0] }),
        Value::Object(m) => m.values_mut().for_each(fake_refs),
        Value::Array(a) => a.iter_mut().for_each(fake_refs),
        _ => {}
    }
}

#[test]
fn guide_examples_are_valid() {
    for (ty, _, json) in guide::FEATURE_EXAMPLES {
        let mut v: Value = serde_json::from_str(json).unwrap_or_else(|e| panic!("{ty}: {e}"));
        fake_refs(&mut v);
        serde_json::from_value::<cad_model::FeatureKind>(v).unwrap_or_else(|e| panic!("ejemplo {ty}: {e}"));
    }
    for (ty, _, json) in guide::CONSTRAINT_EXAMPLES {
        serde_json::from_str::<cad_model::SketchConstraint>(json).unwrap_or_else(|e| panic!("ejemplo {ty}: {e}"));
    }
    for (name, _) in guide::topic_list() {
        assert!(guide::topic(name).is_some_and(|t| t.len() > 100), "{name}");
    }
}

#[test]
fn view_and_measure_use_the_same_numbers() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [40, 20, 10] }));
    let (_, geo) = ok(&s, "cad_geometry", json!({}));
    let top = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "+Z" })).1["faces"][0]["index"].as_u64().unwrap();
    let bottom = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "-Z" })).1["faces"][0]["index"].as_u64().unwrap();

    // La imagen: PNG, con el número de la cara de arriba en la vista de arriba
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "view", "arguments": { "view": "top" } } }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    let r = out.into_inner().unwrap().pop().unwrap();
    let content = r["result"]["content"].as_array().unwrap();
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    let data = &r["result"]["structuredContent"];
    assert_eq!(data["labels"], json!([top]), "{data}");
    // Las aristas de la imagen son las de `cad_geometry`
    assert_eq!(data["edges"].as_u64().unwrap() as usize, geo["edges"].as_array().unwrap().len());
    assert_eq!(data["faces"].as_u64().unwrap() as usize, geo["faces"].as_array().unwrap().len());

    let (text, data) = ok(&s, "measure", json!({ "items": [{ "face": top }, { "face": bottom }] }));
    assert!(text.contains("Minimum distance: 10 mm") && text.contains("area 800 mm²"), "{text}");
    assert_eq!(data["measurement"]["angle"], json!(0.0));
    let (text, _) = ok(&s, "measure", json!({ "items": [{ "point": [0, 0, 30] }, { "face": top }] }));
    assert!(text.contains("Minimum distance: 20 mm"), "{text}");
    let (text, _, err) = call(&s, "view", json!({ "azimuth": 30 }));
    assert!(err && text.contains("elevation"), "{text}");
}

/// La cara cilíndrica de una instancia del ensamble (de `cad_geometry`)
fn cylinder_of(s: &Server, instance: u64) -> u64 {
    let (_, geo) = ok(s, "cad_geometry", json!({ "kind": "faces" }));
    geo["faces"].as_array().unwrap().iter().find(|f| f["surface"] == "cylinder" && f["part"]["feature"] == instance).unwrap()["index"].as_u64().unwrap()
}

#[test]
fn assemble_a_shaft_in_a_plate_and_draw_it() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Base" }));
    ok(&s, "sketch_create", json!({ "shapes": [{ "type": "rectangle", "center": [0, 0], "width": 60, "height": 40 }, { "type": "circle", "center": [0, 0], "diameter": 10 }] }));
    ok(&s, "cad_extrude", json!({ "sketch": 0, "distance": 10 }));
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Eje" }));
    ok(&s, "cad_primitive", json!({ "shape": "cylinder", "diameter": 10, "height": 40 }));
    ok(&s, "cad_tab_create", json!({ "kind": "assembly", "name": "Conjunto" }));

    // Una pestaña de diseño no sirve para insertar en ella
    let (text, _, err) = call(&s, "asm_insert", json!({ "tab": "Base", "from": "Eje" }));
    assert!(err && text.contains("not an assembly"), "{text}");

    let (text, data) = ok(&s, "asm_insert", json!({ "from": "Base", "fixed": true }));
    assert!(text.contains("\"Base 1\""), "{text}");
    let base = data["instances"][0]["instance"].as_u64().unwrap();
    let (_, data) = ok(&s, "asm_insert", json!({ "from": "Eje" }));
    let shaft = data["instances"][0]["instance"].as_u64().unwrap();

    // Eje en el agujero: cilíndrica (gira y desliza), luego centrado (solo gira)
    let (text, data) = ok(&s, "asm_mate", json!({ "kind": "cylindrical", "a": cylinder_of(&s, base), "b": cylinder_of(&s, shaft) }));
    assert!(text.contains("2 degree(s) of freedom"), "{text}");
    let mate = data["mate"].as_u64().unwrap();
    let (text, _) = ok(&s, "asm_mate_edit", json!({ "mate": mate, "distance": 0 }));
    assert!(text.contains("1 degree(s) of freedom"), "{text}");
    // Dos caras de la misma instancia no se pueden unir
    let c = cylinder_of(&s, base);
    let (text, _, err) = call(&s, "asm_mate", json!({ "kind": "fastened", "a": c, "b": c }));
    assert!(err && text.contains("same instance"), "{text}");

    let (text, data) = ok(&s, "asm_check", json!({}));
    assert!(text.contains("No collisions") && text.contains("1 × \"Base\"") && text.contains("1 × \"Eje\""), "{text}");
    assert_eq!(data["bom"].as_array().unwrap().len(), 2);
    let (text, _) = ok(&s, "cad_design", json!({}));
    assert!(text.contains("instance") && text.contains("cylindrical"), "{text}");

    // Medir dentro del ensamble: el eje (Ø10) en el agujero (Ø10) los toca
    let (text, _) = ok(&s, "measure", json!({ "items": [{ "face": cylinder_of(&s, base) }, { "face": cylinder_of(&s, shaft) }] }));
    assert!(text.contains("Minimum distance: 0 mm"), "{text}");

    // Mover y quitar
    let (text, _) = ok(&s, "asm_instance", json!({ "instance": "Eje 1", "fixed": true }));
    assert!(text.contains("fixed"), "{text}");
    ok(&s, "asm_insert", json!({ "from": "Eje", "count": 2 }));
    let (text, _) = ok(&s, "asm_check", json!({}));
    assert!(text.contains("3 × \"Eje\""), "{text}");
    let (text, _) = ok(&s, "asm_instance", json!({ "instance": "Eje 3", "remove": true }));
    assert!(text.contains("removed"), "{text}");

    // Planos: PDF del ensamble con lista de materiales, SVG de una pieza
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("conjunto.pdf");
    let (text, data) = ok(&s, "drawing_export", json!({ "tab": "Conjunto", "path": pdf, "title": "Conjunto" }));
    assert!(text.contains("assembly tab \"Conjunto\"") && text.contains("Bill of materials"), "{text}");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    assert_eq!(data["bom"].as_array().unwrap().len(), 2);
    let svg = dir.path().join("base.svg");
    let (text, data) = ok(&s, "drawing_export", json!({ "tab": "Base", "path": svg, "projection": "third", "scale": "2:1" }));
    assert!(text.contains("scale 2:1") && text.contains("width 60 mm") && text.contains("depth 40 mm"), "{text}");
    assert_eq!(data["scale"], "2:1");
    let body = std::fs::read_to_string(&svg).unwrap();
    assert!(body.starts_with("<svg") && body.contains("stroke-dasharray"), "líneas ocultas a trazos");
    let (text, _, err) = call(&s, "drawing_export", json!({ "tab": "Base", "path": svg }));
    assert!(err && text.contains("overwrite"), "{text}");
}

#[test]
fn the_ai_keeps_working_on_its_own_tab_when_the_active_one_changes() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Uno" }));
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [10, 10, 10] }));
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Dos" }));
    // El usuario (o la ventana) activa otra pestaña entre dos pasos de la IA
    let (_, data) = ok(&s, "status", json!({}));
    let uno = data["workspace"]["design_tabs"][0]["id"].clone();
    s.session().backend.call("cad_tab_activate", json!({ "id": uno })).unwrap();
    // Sin `tab`, la IA sigue en la suya
    let (text, _) = ok(&s, "cad_primitive", json!({ "shape": "sphere", "radius": 4 }));
    assert!(text.contains("tab \"Dos\""), "{text}");
    let (_, data) = ok(&s, "status", json!({}));
    let tabs = data["workspace"]["design_tabs"].as_array().unwrap();
    assert_eq!(tabs[0]["items"], 1, "«Uno» sigue con su caja sola");
    assert_eq!(tabs[1]["items"], 1, "la esfera fue a «Dos»");
    // Nombrar una pestaña que no sirve no mueve el foco
    let (_, _, err) = call(&s, "asm_insert", json!({ "tab": "Uno", "from": "Dos" }));
    assert!(err);
    let (text, _) = ok(&s, "cad_primitive", json!({ "shape": "sphere", "radius": 2, "origin": [20, 0, 0] }));
    assert!(text.contains("tab \"Dos\""), "{text}");
}

#[test]
fn asm_check_on_a_design_checks_its_parts() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Caja de cambios" }));
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [20, 20, 10], "name": "Placa" }));
    // Un eje que atraviesa la placa (choca) y otro a 0,1 mm del borde
    ok(&s, "cad_primitive", json!({ "shape": "cylinder", "diameter": 6, "height": 30, "origin": [0, 0, -10], "op": "new", "name": "Eje" }));
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [5, 5, 5], "origin": [12.6, 0, 0], "op": "new", "name": "Taco" }));

    let (text, data) = ok(&s, "asm_check", json!({ "min_gap": 0.2 }));
    assert_eq!(data["kind"], "design", "{text}");
    assert_eq!(data["parts"], 3, "{text}");
    let collisions = data["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 1, "{text}");
    assert!((collisions[0][2].as_f64().unwrap() - std::f64::consts::PI * 9.0 * 10.0).abs() < 0.5, "{text}");
    let close = data["close"].as_array().unwrap();
    assert_eq!(close.len(), 1, "{text}");
    assert!((close[0][2].as_f64().unwrap() - 0.1).abs() < 1e-6, "{text}");
    assert!(text.contains("COLLISIONS") && text.contains("\"Pieza 2\"") && text.contains("asm_insert"), "{text}");
}
