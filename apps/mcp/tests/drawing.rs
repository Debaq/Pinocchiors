//! Planos 2D completos (cortes, detalles, cotas) y láminas de la app; engranes
//! acoplados en un ensamble.

use std::sync::Mutex;

use pinocchio_mcp::backend::Local;
use pinocchio_mcp::protocol;
use pinocchio_mcp::server::Server;
use pinocchio_mcp::session::Session;
use serde_json::{json, Value};

fn server() -> Server {
    Server::new(Session::new(Box::new(Local::new())))
}

/// (texto, datos, es error, imágenes)
fn call(s: &Server, name: &str, args: Value) -> (String, Value, bool, usize) {
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    let r = out.into_inner().unwrap().pop().unwrap();
    let result = &r["result"];
    let images = result["content"].as_array().map_or(0, |c| c.iter().filter(|x| x["type"] == "image").count());
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["structuredContent"].clone(), result["isError"] == true, images)
}

fn ok(s: &Server, name: &str, args: Value) -> (String, Value) {
    let (text, data, err, _) = call(s, name, args);
    assert!(!err, "{name} falló:\n{text}");
    (text, data)
}

/// El documento de la pestaña activa, como lo guarda la app
fn document(s: &Server) -> Value {
    s.session().backend.call("cad_get_document", json!({})).unwrap().json()
}

/// Placa de 60 × 40 × 10 centrada, con un agujero de Ø10 en el medio
fn plate(s: &Server) {
    ok(s, "cad_tab_create", json!({ "kind": "design", "name": "Placa" }));
    ok(s, "sketch_create", json!({ "shapes": [{ "type": "rectangle", "center": [0, 0], "width": 60, "height": 40 }, { "type": "circle", "center": [0, 0], "diameter": 10 }] }));
    ok(s, "cad_extrude", json!({ "sketch": 0, "distance": 10 }));
}

#[test]
fn a_full_drawing_with_sections_details_and_dimensions() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    plate(&s);
    let (_, geo) = ok(&s, "cad_geometry", json!({ "kind": "edges" }));
    let circle = geo["edges"].as_array().unwrap().iter().find(|e| e["curve"] == "circle").unwrap()["index"].as_u64().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let svg = dir.path().join("placa.svg");
    let (text, data, err, images) = call(
        &s,
        "drawing_export",
        json!({
            "path": svg,
            "section": true,
            "cuts": [{ "view": "top", "points": [[0, -30, 5], [0, 30, 5]] }],
            "details": [{ "view": "front", "center": [30, 0, 10], "radius": 4 }],
            "dims": [
                { "view": "top", "kind": "diameter", "at": [5, 0, 10] },
                { "view": "top", "kind": "diameter", "edges": [circle] },
                // El frente va cortado por el agujero: el borde de arriba son dos tramos de 25
                { "view": "front", "kind": "length", "at": [-20, -20, 10] },
                { "view": "top", "kind": "distance", "at": [[-30, 0, 10], [30, 0, 10]] },
                { "view": "iso", "kind": "length", "at": [0, 0, 0] }
            ]
        }),
    );
    assert!(!err && images == 1, "{text}");
    assert!(text.contains("Ø10 mm (top)") && text.contains("25 mm (front)") && text.contains("60 mm (top)"), "{text}");
    assert!(text.contains("Left out") && text.contains("iso view"), "la cota en la iso se omite y se avisa: {text}");
    let views: Vec<&str> = data["views"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(views.contains(&"corte-B") && views.contains(&"detalle-C"), "{views:?}");
    let body = std::fs::read_to_string(&svg).unwrap();
    for want in ["SECTION A-A", "SECTION B-B", "DETAIL C", "url(#hatch)", "clip-path"] {
        assert!(body.contains(want), "falta {want}");
    }

    // Una línea de corte que en la vista es un punto no sirve
    let (text, _, err, _) = call(&s, "drawing_export", json!({ "path": dir.path().join("x.svg"), "cuts": [{ "view": "top", "points": [[0, 0, 0], [0, 0, 9]] }] }));
    assert!(err && text.contains("distinct points"), "{text}");
}

#[test]
fn drawing_sheets_live_in_the_document_like_the_app_ones() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    plate(&s);
    let (text, data, err, images) = call(&s, "drawing_sheet", json!({ "name": "Taller", "sheet": "A3", "section": 5, "dims": [{ "view": "top", "kind": "diameter", "at": [5, 0, 10] }] }));
    assert!(!err && images == 1 && text.contains("Made drawing sheet \"Taller\""), "{text}");
    let id = data["drawing"].as_u64().unwrap();

    // Queda en el documento con la forma de la app
    let doc = document(&s);
    let sheet = &doc["drawing"]["sheets"][0];
    assert_eq!(sheet["name"], "Taller", "{doc}");
    assert_eq!(sheet["sheet"], "A3");
    assert_eq!(sheet["section"]["y"], 5.0);
    assert_eq!(sheet["dims"][0]["kind"], "diameter");
    assert_eq!(sheet["dims"][0]["refs"][0], json!([5.0, 0.0]), "refs en coordenadas de la vista superior");
    assert!(sheet["hiddenViews"].as_array().unwrap().contains(&json!("back")));

    // Cambiarla suma cotas; otra lámina es otra entrada; se dibuja con drawing_export
    let (text, _) = ok(&s, "drawing_sheet", json!({ "name": "Taller", "dims": [{ "view": "front", "kind": "length", "at": [0, -20, 10] }] }));
    assert!(text.contains("Updated") && text.contains("Ø10") && text.contains("60 mm"), "{text}");
    ok(&s, "drawing_sheet", json!({ "name": "Montaje", "views": ["front", "iso"] }));
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("taller.pdf");
    let (text, _) = ok(&s, "drawing_export", json!({ "path": pdf, "drawing": id }));
    assert!(text.contains("Ø10") && text.contains("sheet A3"), "{text}");
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));

    // Borrar y deshacer
    ok(&s, "drawing_sheet", json!({ "name": "Montaje", "remove": true }));
    assert_eq!(document(&s)["drawing"]["sheets"].as_array().unwrap().len(), 1);
    ok(&s, "undo", json!({}));
    assert_eq!(document(&s)["drawing"]["sheets"].as_array().unwrap().len(), 2);
    let (text, _, err, _) = call(&s, "drawing_export", json!({ "path": dir.path().join("y.pdf"), "drawing": "Nada" }));
    assert!(err && text.contains("Taller"), "{text}");
}

#[test]
fn gears_turn_together() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Rueda" }));
    ok(&s, "cad_primitive", json!({ "shape": "cylinder", "diameter": 20, "height": 5 }));
    ok(&s, "cad_tab_create", json!({ "kind": "design", "name": "Base" }));
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [80, 40, 4], "origin": [0, 0, -4] }));
    ok(&s, "cad_tab_create", json!({ "kind": "assembly", "name": "Tren" }));
    ok(&s, "asm_insert", json!({ "from": "Base", "fixed": true }));
    let (_, a) = ok(&s, "asm_insert", json!({ "from": "Rueda", "at": [-15, 0, 0] }));
    let (_, b) = ok(&s, "asm_insert", json!({ "from": "Rueda", "at": [15, 0, 0] }));
    let (ia, ib) = (a["instances"][0]["instance"].as_u64().unwrap(), b["instances"][0]["instance"].as_u64().unwrap());
    // Cada rueda gira sobre su eje: revoluta entre la cara de abajo de la rueda y la de arriba de la base
    let (_, geo) = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "-Z" }));
    let bottom = |inst: u64| geo["faces"].as_array().unwrap().iter().find(|f| f["part"]["feature"] == inst).unwrap()["index"].as_u64().unwrap();
    let (_, top) = ok(&s, "cad_geometry", json!({ "kind": "faces", "facing": "+Z" }));
    let base_top = top["faces"].as_array().unwrap().iter().find(|f| f["part"]["feature"] != ia && f["part"]["feature"] != ib).unwrap()["index"].as_u64().unwrap();
    let (_, m1) = ok(&s, "asm_mate", json!({ "kind": "revolute", "a": base_top, "b": bottom(ia) }));
    let (_, m2) = ok(&s, "asm_mate", json!({ "kind": "revolute", "a": base_top, "b": bottom(ib) }));
    let (r1, r2) = (m1["mate"].as_u64().unwrap(), m2["mate"].as_u64().unwrap());

    let (text, _, err, _) = call(&s, "asm_relation", json!({ "kind": "screw", "a": r1, "pitch": 1 }));
    assert!(err && text.contains("cylindrical"), "un tornillo pide una cilíndrica: {text}");
    let (text, data) = ok(&s, "asm_relation", json!({ "kind": "gear", "a": r1, "b": r2, "ratio": -1 }));
    assert!(text.contains("gear"), "{text}");
    assert_eq!(data["relations"].as_array().unwrap().len(), 1);
    let (text, _) = ok(&s, "cad_design", json!({}));
    assert!(text.contains("relation 0: gear"), "{text}");
    // Girar una gira la otra al revés: queda un solo grado libre
    let (text, _) = ok(&s, "asm_mate_edit", json!({ "mate": r1, "angle": 30 }));
    assert!(text.contains("0 degree(s)"), "{text}");
    let (text, _) = ok(&s, "asm_relation", json!({ "remove": 0 }));
    assert!(text.contains("removed"), "{text}");
}
