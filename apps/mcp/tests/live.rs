//! Modo en vivo de punta a punta: el puente de la app con una «ventana» de
//! prueba (concede o niega turnos, contesta la selección) y el conector que
//! la encuentra por `mcp-live.json`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use pinocchio_app_lib::mcp::backend::Auto;
use pinocchio_app_lib::mcp::backend::{Backend, Local};
use pinocchio_app_lib::mcp::live::{Bridge, Host, EV_ASK, EV_DONE, EV_PROGRESS, EV_TURN};
use pinocchio_app_lib::AppState;
use pinocchio_mcp::{protocol, server::Server, session::Session};
use serde_json::{json, Value};

struct Window {
    state: AppState,
    bridge: OnceLock<Arc<Bridge>>,
    /// El usuario está editando: niega los turnos
    busy: AtomicBool,
    done: Mutex<Vec<Value>>,
    /// Avance que vio el indicador de la ventana
    progress: Mutex<Vec<Value>>,
}

impl Host for Window {
    fn state(&self) -> &AppState {
        &self.state
    }
    fn emit(&self, event: &str, payload: Value) {
        let bridge = self.bridge.get().unwrap();
        let id = payload["id"].as_u64().unwrap_or(0);
        match event {
            EV_TURN if self.busy.load(Ordering::SeqCst) => bridge.answer(id, json!({ "ok": false, "reason": "editing a sketch" })),
            EV_TURN => bridge.answer(id, json!({ "ok": true })),
            EV_ASK if payload["what"] == "selection" => bridge.answer(id, json!({ "value": { "tab": { "id": 0, "name": "Pieza" }, "picks": [{ "kind": "edge", "edge": 3 }, { "kind": "face", "face": 5 }] } })),
            EV_ASK => bridge.answer(id, json!({ "value": null })),
            EV_DONE => self.done.lock().unwrap().push(payload),
            EV_PROGRESS => self.progress.lock().unwrap().push(payload),
            _ => {}
        }
    }
}

fn call(s: &Server, name: &str, args: Value) -> (String, bool) {
    let out = Mutex::new(Vec::new());
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    s.handle(protocol::parse(&line).unwrap().unwrap(), &|m| out.lock().unwrap().push(m));
    let r = out.into_inner().unwrap().pop().unwrap();
    (r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string(), r["result"]["isError"] == true)
}

fn features(w: &Window) -> usize {
    w.state.cad_doc().map_or(0, |d| d.features.len())
}

#[test]
fn the_ai_drives_the_open_window_by_turns() {
    if !cad_model::occt::available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mcp-live.json");
    // SAFETY: solo esta prueba usa la variable (las demás del binario no usan el modo en vivo)
    unsafe { std::env::set_var("PINOCCHIO_MCP_LIVE_FILE", &file) };

    let window = Arc::new(Window { state: AppState::new(), bridge: OnceLock::new(), busy: AtomicBool::new(false), done: Mutex::new(Vec::new()), progress: Mutex::new(Vec::new()) });
    let bridge = Arc::new(Bridge::default());
    window.bridge.set(bridge.clone()).ok();
    bridge.start(window.clone()).unwrap();
    assert!(file.is_file(), "deja dónde encontrarla");

    let s = Server::new(Session::new(Box::new(Auto::new())));
    let (text, err) = call(&s, "status", json!({}));
    assert!(!err && text.contains("Mode: live"), "{text}");

    // Lo que hace la IA queda en la app abierta, y la ventana se entera
    let (text, err) = call(&s, "cad_primitive", json!({ "shape": "box", "dimensions": [40, 20, 10] }));
    assert!(!err, "{text}");
    assert_eq!(features(&window), 1);
    let (_, err) = call(&s, "cad_primitive", json!({ "shape": "cylinder", "diameter": 6, "height": 30, "origin": [0, 0, -5], "op": "cut" }));
    assert!(!err);
    let last = window.done.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last["ok"], true);
    let changes = last["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "una pestaña cambió: {last}");
    assert_eq!(changes[0]["before"]["features"].as_array().unwrap().len(), 1, "con el documento de antes, para el deshacer");

    // Si el usuario está editando, la IA espera
    window.busy.store(true, Ordering::SeqCst);
    let (text, err) = call(&s, "cad_primitive", json!({ "shape": "sphere", "radius": 3 }));
    assert!(err && text.contains("busy") && text.contains("editing a sketch"), "{text}");
    assert_eq!(features(&window), 2, "no tocó nada");
    window.busy.store(false, Ordering::SeqCst);

    // Pausada, tampoco
    bridge.set_paused(true);
    let (text, err) = call(&s, "status", json!({}));
    assert!(err && text.contains("paused"), "{text}");
    bridge.set_paused(false);

    // Una herramienta que falla deja la app como estaba
    let (text, err) = call(&s, "cad_fillet", json!({ "edges": [0], "radius": 99 }));
    assert!(err, "{text}");
    assert_eq!(features(&window), 2);
    assert_eq!(window.done.lock().unwrap().last().unwrap()["ok"], false);

    // Lo que el usuario tiene elegido con el mouse
    let (text, err) = call(&s, "selection", json!({}));
    assert!(!err && text.contains("edge 3") && text.contains("face 5") && text.contains("\"Pieza\""), "{text}");

    // Los archivos los maneja el usuario
    let (text, err) = call(&s, "project_new", json!({}));
    assert!(err && text.contains("live mode"), "{text}");
    assert_eq!(features(&window), 2);

    // Un comando largo avisa su avance al conector y al indicador de la ventana
    let stl = stl_file(dir.path());
    let seen = Mutex::new(Vec::new());
    s.session().backend.begin("import").unwrap();
    s.session().backend.call_with("import_model", json!({ "path": stl }), &|f, m| seen.lock().unwrap().push((f, m.to_string()))).unwrap();
    s.session().backend.end("import", true, false);
    let seen = seen.into_inner().unwrap();
    assert!(seen.len() >= 2 && seen.iter().all(|(f, _)| (0.0..=1.0).contains(f)) && seen.iter().any(|(_, m)| !m.is_empty()), "{seen:?}");
    assert_eq!(window.progress.lock().unwrap().len(), seen.len());

    // Se apaga: el conector vuelve a su propio motor
    bridge.stop();
    assert!(!file.exists());
    let (text, err) = call(&s, "status", json!({}));
    assert!(!err && text.contains("Mode: standalone"), "{text}");
}

/// Un triángulo en STL de texto
fn stl_file(dir: &std::path::Path) -> String {
    let path = dir.join("tri.stl");
    std::fs::write(&path, "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 10 0\nendloop\nendfacet\nendsolid t\n").unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn long_commands_report_progress_without_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Mutex::new(Vec::new());
    let info = Local::new().call_with("import_model", json!({ "path": stl_file(dir.path()) }), &|f, m| seen.lock().unwrap().push((f, m.to_string()))).unwrap().json();
    assert_eq!(info["num_faces"], 1, "{info}");
    let seen = seen.into_inner().unwrap();
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "avanza: {seen:?}");
}
