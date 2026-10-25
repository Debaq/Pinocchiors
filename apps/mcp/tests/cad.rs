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
