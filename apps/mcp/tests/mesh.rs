//! Mallas, impresión, render y escaneos por el conector, como lo haría una IA.

use std::path::Path;
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
fn call_full(s: &Server, name: &str, args: Value) -> (String, Value, bool, usize) {
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    let r = out.into_inner().unwrap().pop().unwrap();
    let result = &r["result"];
    let images = result["content"].as_array().map_or(0, |c| c.iter().filter(|x| x["type"] == "image").count());
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["structuredContent"].clone(), result["isError"] == true, images)
}

fn call(s: &Server, name: &str, args: Value) -> (String, Value, bool) {
    let (t, d, e, _) = call_full(s, name, args);
    (t, d, e)
}

fn ok(s: &Server, name: &str, args: Value) -> (String, Value) {
    let (text, data, err) = call(s, name, args);
    assert!(!err, "{name} falló:\n{text}");
    (text, data)
}

/// Cubo de `size` mm en ASCII STL; sin la cara de arriba si `open`
fn cube_stl(dir: &Path, name: &str, size: f64, open: bool) -> String {
    let s = size;
    let p = |x: f64, y: f64, z: f64| [x * s, y * s, z * s];
    let v = [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
    let mut faces = vec![[0, 2, 1], [0, 3, 2], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    if !open {
        faces.extend([[4, 5, 6], [4, 6, 7]]);
    }
    let mut text = String::from("solid c\n");
    for f in faces {
        text.push_str("facet normal 0 0 0\nouter loop\n");
        for i in f {
            text.push_str(&format!("vertex {} {} {}\n", v[i][0], v[i][1], v[i][2]));
        }
        text.push_str("endloop\nendfacet\n");
    }
    text.push_str("endsolid c\n");
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

fn dims(data: &Value) -> Vec<f64> {
    (0..3).map(|k| data["analysis"]["dimensions"][k].as_f64().unwrap_or(0.0)).collect()
}

#[test]
fn import_check_cut_and_export_plates() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    let (text, _, err) = call(&s, "print_check", json!({}));
    assert!(err && text.contains("mesh_import"), "sin modelo dice cómo tener uno: {text}");

    let (text, data) = ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "cube.stl", 40.0, false) }));
    assert!(text.contains("closed"), "{text}");
    let d = dims(&data);
    assert!(d.iter().all(|x| (x - 40.0).abs() < 1e-6), "40 mm por lado: {d:?}");

    let (text, data) = ok(&s, "print_check", json!({ "printer": "Prusa MINI", "material": "PETG" }));
    assert!(text.contains("printable") && text.contains("Fits the bed"), "{text}");
    assert!(data["estimate"]["mass_g"].as_f64().unwrap() > 0.0);

    // En una cama chica no cabe: piezas, uniones y placas
    let tiny = json!({ "build_volume": [30, 30, 30] });
    let (text, _) = ok(&s, "print_check", json!({ "printer": tiny }));
    assert!(text.contains("Too big"), "{text}");
    let (text, data) = ok(&s, "print_cut", json!({ "printer": tiny, "margin": 0 }));
    let pieces = data["piece_count"].as_u64().unwrap();
    assert!(pieces >= 2, "{text}");
    let (text, data) = ok(&s, "print_joints", json!({ "size": 5, "depth": 5, "count": 1 }));
    assert!(data["joints_placed"].as_u64().unwrap() > 0, "{text}");
    let (_, _, err, images) = call_full(&s, "view", json!({ "show": "pieces", "width": 320, "height": 240 }));
    assert!(!err && images == 1);

    let plate = dir.path().join("plate.stl");
    let (text, data) = ok(&s, "print_plates", json!({ "path": plate, "printer": tiny, "spacing": 2 }));
    let files = data["files"].as_array().unwrap();
    assert!(!files.is_empty() && files.iter().all(|f| Path::new(f.as_str().unwrap()).is_file()), "{text}");
    // No pisa sin permiso
    let first = files[0].as_str().unwrap();
    let (text, _, err) = call(&s, "print_plates", json!({ "path": first, "printer": tiny }));
    assert!(err && text.contains("overwrite"), "{text}");

    // Deshacer vuelve al cubo entero (sin piezas)
    ok(&s, "undo", json!({}));
    ok(&s, "undo", json!({}));
    let (_, _, err) = call(&s, "view", json!({ "show": "pieces" }));
    assert!(err, "sin piezas tras deshacer el corte");
}

#[test]
fn repair_an_open_mesh_and_remesh() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "open.stl", 20.0, true) }));
    let (text, data) = ok(&s, "mesh_info", json!({ "diagnose": true }));
    assert!(text.contains("hole"), "{text}");
    assert_eq!(data["diagnostics"]["boundary_loops"], 1);
    let (text, _) = ok(&s, "print_check", json!({}));
    assert!(text.contains("not watertight"), "{text}");

    let (text, data) = ok(&s, "mesh_repair", json!({}));
    assert!(text.contains("holes filled") && data["analysis"]["is_closed"] == true, "{text}");

    let (text, data) = ok(&s, "mesh_remesh", json!({ "mode": "isotropic", "edge_mm": 4 }));
    assert!(data["remesh"]["stats"]["after"]["triangles"].as_u64().unwrap() > 12, "{text}");
    let (text, _, err) = call(&s, "mesh_remesh", json!({ "mode": "simplify" }));
    assert!(err && text.contains("ratio"), "{text}");
    let (_, _, err, images) = call_full(&s, "view", json!({ "show": "model", "view": "top", "width": 200, "height": 160 }));
    assert!(!err && images == 1);

    // Exportar en dos formatos, sin pisar
    for name in ["out.stl", "out.glb"] {
        let path = dir.path().join(name);
        let (text, _) = ok(&s, "mesh_export", json!({ "path": path }));
        assert!(path.is_file(), "{text}");
        let (text, _, err) = call(&s, "mesh_export", json!({ "path": path }));
        assert!(err && text.contains("overwrite"), "{text}");
    }
    let (text, _, err) = call(&s, "mesh_export", json!({ "path": dir.path().join("x.fbx") }));
    assert!(err && text.contains("glb"), "{text}");
}

#[test]
fn scale_and_orient() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "c.stl", 10.0, false) }));
    let (text, data) = ok(&s, "print_scale", json!({ "size": [0, 0, 25] }));
    let z = data["after"]["dimensions"][2].as_f64().unwrap();
    assert!((z - 25.0).abs() < 1e-3, "{text}");
    let (text, _, err) = call(&s, "print_scale", json!({ "factor": 2, "size": [1, 1, 1] }));
    assert!(err && text.contains("exactly one"), "{text}");
    let (text, data) = ok(&s, "print_orient", json!({}));
    assert!(!data["candidates"].as_array().unwrap().is_empty(), "{text}");
    let (_, data) = ok(&s, "mesh_info", json!({}));
    assert!(dims(&data).iter().all(|x| (x - 25.0).abs() < 1e-3), "un cubo sigue igual de grande al apoyarlo: {:?}", dims(&data));
}

#[test]
fn every_import_is_a_new_object_and_undo_takes_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "a.stl", 10.0, false) }));
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "b.stl", 30.0, false) }));
    let ui = s.session().ui();
    let names: Vec<&str> = ui["objects"].as_array().unwrap().iter().map(|o| o["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["a.stl", "b.stl"], "{ui}");
    assert_eq!(ui["activeObject"], ui["objects"][1]["id"]);
    let (text, data) = ok(&s, "mesh_info", json!({}));
    assert!(text.contains("a.stl") && (dims(&data)[0] - 30.0).abs() < 1e-6, "{text}");
    let (_, status) = ok(&s, "status", json!({}));
    assert_eq!(status["workspace"]["other_objects"], 1);

    ok(&s, "undo", json!({}));
    let ui = s.session().ui();
    assert_eq!(ui["objects"].as_array().unwrap().len(), 1, "{ui}");
    let (_, data) = ok(&s, "mesh_info", json!({}));
    assert!((dims(&data)[0] - 10.0).abs() < 1e-6);

    // Un archivo que no existe o de otro formato no deja nada
    let (text, _, err) = call(&s, "mesh_import", json!({ "path": dir.path().join("nada.stl") }));
    assert!(err && text.contains("does not exist"), "{text}");
    std::fs::write(dir.path().join("x.step"), "ISO").unwrap();
    let (text, _, err) = call(&s, "mesh_import", json!({ "path": dir.path().join("x.step") }));
    assert!(err && text.contains("CAD"), "{text}");
}

#[test]
fn saved_project_keeps_the_objects() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "a.stl", 10.0, false) }));
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "b.stl", 20.0, false) }));
    let file = dir.path().join("p.pinocchio");
    ok(&s, "project_save", json!({ "path": file }));
    let t = server();
    ok(&t, "project_open", json!({ "path": file }));
    let ui = t.session().ui();
    assert_eq!(ui["objects"].as_array().unwrap().len(), 2, "{ui}");
    let (_, data) = ok(&t, "mesh_info", json!({}));
    assert!((dims(&data)[0] - 20.0).abs() < 1e-6);
}

#[test]
fn render_an_image() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    ok(&s, "mesh_import", json!({ "path": cube_stl(dir.path(), "c.stl", 10.0, false) }));
    let png = dir.path().join("r.png");
    let (text, _, err, images) = call_full(&s, "render_image", json!({ "quality": "draft", "width": 96, "height": 64, "path": png }));
    assert!(!err && images == 1, "{text}");
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"), "{text}");
    let (text, _, err) = call(&s, "render_image", json!({ "background": "color" }));
    assert!(err && text.contains("color"), "{text}");
}

#[test]
fn scan_a_point_cloud() {
    let dir = tempfile::tempdir().unwrap();
    let s = server();
    // Esfera de 20 mm de radio en puntos (espiral de Fibonacci)
    let n = 4000;
    let mut ply = format!("ply\nformat ascii 1.0\nelement vertex {n}\nproperty float x\nproperty float y\nproperty float z\nend_header\n");
    for i in 0..n {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
        let r = (1.0 - y * y).sqrt();
        let a = i as f64 * 2.399_963;
        ply.push_str(&format!("{} {} {}\n", 20.0 * r * a.cos(), 20.0 * y, 20.0 * r * a.sin()));
    }
    let path = dir.path().join("ball.ply");
    std::fs::write(&path, ply).unwrap();
    let (text, data) = ok(&s, "scan_import", json!({ "path": path, "watertight": true }));
    let d = dims(&data);
    assert!(d.iter().all(|x| (30.0..50.0).contains(x)), "unos 40 mm de diámetro: {d:?} — {text}");
}

#[test]
fn a_cad_part_becomes_a_mesh_object() {
    if !cad_model::occt::available() {
        return;
    }
    let s = server();
    ok(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [30, 20, 10], "name": "Bloque" }));
    let (text, data) = ok(&s, "mesh_from_part", json!({}));
    let d = dims(&data);
    assert!((d[0] - 30.0).abs() < 1e-3 && (d[1] - 20.0).abs() < 1e-3 && (d[2] - 10.0).abs() < 1e-3, "{d:?} — {text}");
    let ui = s.session().ui();
    assert_eq!(ui["objects"][0]["source"]["kind"], "cad", "{ui}");
    // Otra vez: el mismo objeto, no uno nuevo
    ok(&s, "mesh_from_part", json!({}));
    assert_eq!(s.session().ui()["objects"].as_array().unwrap().len(), 1);
    let (text, _) = ok(&s, "print_check", json!({}));
    assert!(text.contains("printable"), "{text}");
}
