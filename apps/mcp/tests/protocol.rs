//! El conector de punta a punta: saludo, lista, llamadas, errores con pista,
//! deshacer y proyectos; y el binario real por stdin/stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use pinocchio_mcp::backend::Local;
use pinocchio_mcp::protocol;
use pinocchio_mcp::server::Server;
use pinocchio_mcp::session::Session;
use serde_json::{json, Value};

fn server() -> Server {
    Server::new(Session::new(Box::new(Local::new())))
}

/// Manda un pedido y devuelve todo lo que salió (avisos y respuesta)
fn exchange(s: &Server, id: i64, method: &str, params: Value) -> Vec<Value> {
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    out.into_inner().unwrap()
}

fn request(s: &Server, method: &str, params: Value) -> Value {
    let out = exchange(s, 1, method, params);
    out.into_iter().find(|m| m.get("id").is_some()).expect("sin respuesta")
}

/// Llama una herramienta: (texto, datos, es error)
fn call(s: &Server, name: &str, args: Value) -> (String, Value, bool) {
    let r = request(s, "tools/call", json!({ "name": name, "arguments": args }));
    let result = &r["result"];
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["structuredContent"].clone(), result["isError"] == true)
}

#[test]
fn handshake_and_tool_list_are_complete() {
    let s = server();
    let init = request(&s, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["instructions"].as_str().unwrap().contains("millimetres"));
    assert_eq!(request(&s, "ping", json!({}))["result"], json!({}));

    let tools = request(&s, "tools/list", json!({}))["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in ["status", "help", "undo", "redo", "project_new", "project_open", "project_save"] {
        assert!(names.contains(&want), "falta {want}: {names:?}");
    }
    for t in &tools {
        let name = t["name"].as_str().unwrap();
        assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && name.len() <= 64, "{name}");
        assert!(t["description"].as_str().unwrap().len() > 60, "{name}: descripción muy corta");
        assert_eq!(t["inputSchema"]["type"], "object", "{name}");
        assert_eq!(t["inputSchema"]["additionalProperties"], false, "{name}");
        assert!(t["annotations"]["readOnlyHint"].is_boolean(), "{name}");
    }
    assert_eq!(request(&s, "nada/de/nada", json!({}))["error"]["code"], protocol::METHOD_NOT_FOUND);
}

#[test]
fn old_clients_get_data_as_text() {
    let s = server();
    let init = request(&s, "initialize", json!({ "protocolVersion": "2024-11-05" }));
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    let r = request(&s, "tools/call", json!({ "name": "status" }));
    assert!(r["result"].get("structuredContent").is_none());
    let content = r["result"]["content"].as_array().unwrap();
    let data: Value = serde_json::from_str(content[1]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data["mode"], "standalone");
}

#[test]
fn status_and_help_read_well() {
    let s = server();
    let (text, data, err) = call(&s, "status", json!({}));
    assert!(!err, "{text}");
    assert!(text.contains("Mode: standalone") && text.contains("Mesh model: none"), "{text}");
    assert_eq!(data["mode"], "standalone");
    let (text, _, err) = call(&s, "help", json!({ "topic": "conventions" }));
    assert!(!err && text.contains("millimetres") && text.contains("Other topics"), "{text}");
}

#[test]
fn bad_arguments_and_tools_explain_themselves() {
    let s = server();
    let (text, _, err) = call(&s, "help", json!({ "topic": "cooking" }));
    assert!(err && text.contains("not an option") && text.contains("Hint:"), "{text}");
    let (text, _, err) = call(&s, "project_open", json!({}));
    assert!(err && text.contains("missing \"path\""), "{text}");
    let (text, _, err) = call(&s, "project_open", json!({ "path": "relative.pinocchio" }));
    assert!(err && text.contains("absolute"), "{text}");
    let (text, _, err) = call(&s, "project_save", json!({}));
    assert!(err && text.contains("Hint: pass `path`"), "{text}");
    let r = request(&s, "tools/call", json!({ "name": "fly", "arguments": {} }));
    assert_eq!(r["error"]["code"], protocol::INVALID_PARAMS);
}

#[test]
fn save_open_and_undo_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pieza.pinocchio");
    let s = server();
    // Un diseño en el motor, como si lo hubiera hecho una herramienta de CAD
    s.session().backend.call("cad_new", json!({})).unwrap();
    let (text, _, err) = call(&s, "project_save", json!({ "path": file }));
    assert!(!err && text.starts_with("Saved"), "{text}");
    assert!(file.is_file());
    // Guardar encima de otro archivo pide permiso
    let other = dir.path().join("otro.pinocchio");
    std::fs::write(&other, b"x").unwrap();
    let (text, _, err) = call(&s, "project_save", json!({ "path": other }));
    assert!(err && text.contains("overwrite"), "{text}");

    let (_, _, err) = call(&s, "project_new", json!({}));
    assert!(!err);
    let (_, data, _) = call(&s, "status", json!({}));
    assert_eq!(data["workspace"]["design_tabs"].as_array().unwrap().len(), 0);

    let (text, data, err) = call(&s, "project_open", json!({ "path": file }));
    assert!(!err, "{text}");
    assert!(text.contains("CAD tabs: \""), "{text}");
    assert_eq!(data["workspace"]["design_tabs"].as_array().unwrap().len(), 1);

    // Deshacer abrir vuelve al proyecto vacío; rehacer, al abierto
    let (text, _, _) = call(&s, "undo", json!({}));
    assert!(text.contains("Open pieza.pinocchio"), "{text}");
    let (_, data, _) = call(&s, "status", json!({}));
    assert_eq!(data["workspace"]["design_tabs"].as_array().unwrap().len(), 0);
    assert!(data["project"]["path"].is_null());
    call(&s, "redo", json!({}));
    let (_, data, _) = call(&s, "status", json!({}));
    assert_eq!(data["workspace"]["design_tabs"].as_array().unwrap().len(), 1);
    assert_eq!(data["project"]["path"], json!(file));
}

#[test]
fn resources_and_prompts() {
    let s = server();
    let list = request(&s, "resources/list", json!({}));
    assert!(list["result"]["resources"].as_array().unwrap().iter().any(|r| r["uri"] == "pinocchio://guide"));
    let guide = request(&s, "resources/read", json!({ "uri": "pinocchio://guide/projects" }));
    assert!(guide["result"]["contents"][0]["text"].as_str().unwrap().contains("project_save"));
    assert_eq!(request(&s, "resources/read", json!({ "uri": "file:///etc/passwd" }))["error"]["code"], protocol::INVALID_PARAMS);
    let p = request(&s, "prompts/get", json!({ "name": "get_started" }));
    assert!(p["result"]["messages"][0]["content"]["text"].as_str().unwrap().contains("status"));
}

#[test]
fn binary_speaks_mcp_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pinocchio-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for msg in [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "status", "arguments": {} } }),
    ] {
        writeln!(stdin, "{msg}").unwrap();
    }
    writeln!(stdin, "esto no es json").unwrap();
    drop(stdin);
    let replies: Vec<Value> = BufReader::new(child.stdout.take().unwrap()).lines().map(|l| serde_json::from_str(&l.unwrap()).unwrap()).collect();
    child.wait().unwrap();
    let by_id = |id: i64| replies.iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("sin respuesta {id}: {replies:?}"));
    assert_eq!(by_id(1)["result"]["serverInfo"]["name"], "pinocchio");
    assert!(by_id(2)["result"]["content"][0]["text"].as_str().unwrap().contains("Mode: standalone"));
    assert!(replies.iter().any(|r| r["error"]["code"] == protocol::PARSE_ERROR));
    assert_eq!(replies.len(), 3, "el aviso no lleva respuesta: {replies:?}");
}
