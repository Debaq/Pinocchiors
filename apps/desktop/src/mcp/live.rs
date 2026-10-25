//! Modo en vivo: la IA maneja la ventana abierta. Con el interruptor de
//! Configuración ▸ Conector para IA, la app escucha en 127.0.0.1 (puerto
//! libre, con clave) y deja `mcp-live.json` en su carpeta de datos; el
//! conector (`pinocchio --mcp`) lo lee y le manda las herramientas acá en vez
//! de usar su propio motor.
//!
//! Por turnos: cada herramienta pide turno a la ventana, que lo niega si el
//! usuario está editando (sketch o diálogo abiertos) o si pausó la IA.
//! Mientras dura el turno los comandos corren sobre el estado de la app; al
//! terminar, si la herramienta falló se vuelve a como estaba, y si no, la
//! ventana recibe qué pestañas cambiaron y cómo estaban antes (un paso de su
//! deshacer por pestaña).
//!
//! Mensajes: una línea JSON por pedido, `{"token", "op", ...}`; respuesta
//! `{"ok": true, "value" | "bytes"}` o `{"ok": false, "error"}`. Antes de la
//! respuesta, un comando largo manda su avance: `{"progress": 0..1, "message"}`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::cad::bridge::{try_dispatch_with, Reply};
use crate::AppState;

/// Eventos a la ventana
pub const EV_TURN: &str = "mcp-live://turn";
pub const EV_DONE: &str = "mcp-live://done";
pub const EV_ASK: &str = "mcp-live://ask";
pub const EV_PROGRESS: &str = "mcp-live://progress";

/// Cuánto se espera a que la ventana conteste
const ANSWER: Duration = Duration::from_secs(15);
/// Lo que espera a que la ventana importe un modelo (los grandes tardan)
const IMPORT: Duration = Duration::from_secs(600);

/// Dónde la app deja el puerto y la clave para el conector
pub fn live_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PINOCCHIO_MCP_LIVE_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local/share")))
    }?;
    Some(base.join("pinocchio/mcp-live.json"))
}

/// Lo que dice `mcp-live.json`
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LiveInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// Lo que el puente necesita de la app: su estado y avisar a la ventana
pub trait Host: Send + Sync + 'static {
    fn state(&self) -> &AppState;
    fn emit(&self, event: &str, payload: Value);
}

/// Una pestaña que la IA cambió y cómo estaba antes (lo que su deshacer necesita)
#[derive(Serialize, Clone, Debug)]
pub struct Change {
    pub tab: u32,
    pub before: cad_model::Document,
}

#[derive(Default)]
pub struct Bridge {
    running: Mutex<Option<Running>>,
    /// Preguntas a la ventana que esperan respuesta
    pending: Mutex<HashMap<u64, Sender<Value>>>,
    next: AtomicU64,
    paused: AtomicBool,
    /// Conexión que tiene el turno (una IA a la vez)
    turn: Mutex<Option<u64>>,
}

struct Running {
    port: u16,
    stop: Arc<AtomicBool>,
}

/// Clave al azar (sin otra biblioteca): dos hashes con semillas del sistema
fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for _ in 0..2 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

impl Bridge {
    pub fn is_on(&self) -> bool {
        self.running.lock().unwrap().is_some()
    }

    pub fn port(&self) -> Option<u16> {
        self.running.lock().unwrap().as_ref().map(|r| r.port)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Prende el puente (si no estaba) y escribe `mcp-live.json`
    pub fn start(self: &Arc<Self>, host: Arc<dyn Host>) -> Result<u16, String> {
        if let Some(port) = self.port() {
            return Ok(port);
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("No se pudo abrir el puerto local: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let token = token();
        let file = live_file().ok_or("No se encontró la carpeta de datos")?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
        }
        let info = LiveInfo { port, token: token.clone(), pid: std::process::id() };
        std::fs::write(&file, serde_json::to_string(&info).unwrap()).map_err(|e| format!("No se pudo escribir {}: {e}", file.display()))?;
        let stop = Arc::new(AtomicBool::new(false));
        *self.running.lock().unwrap() = Some(Running { port, stop: stop.clone() });
        let bridge = self.clone();
        std::thread::spawn(move || {
            let mut conn = 0u64;
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        conn += 1;
                        let (bridge, host, token, id) = (bridge.clone(), host.clone(), token.clone(), conn);
                        std::thread::spawn(move || bridge.serve(stream, host.as_ref(), &token, id));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        });
        Ok(port)
    }

    /// Apaga el puente y borra `mcp-live.json` (si es el de esta app)
    pub fn stop(&self) {
        if let Some(r) = self.running.lock().unwrap().take() {
            r.stop.store(true, Ordering::SeqCst);
        }
        if let Some(file) = live_file()
            && let Some(info) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<LiveInfo>(&t).ok())
            && info.pid == std::process::id()
        {
            let _ = std::fs::remove_file(file);
        }
    }

    /// La ventana contesta una pregunta
    pub fn answer(&self, id: u64, payload: Value) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
            let _ = tx.send(payload);
        }
    }

    /// Pregunta a la ventana y espera la respuesta
    fn ask(&self, host: &dyn Host, event: &str, payload: Value) -> Result<Value, String> {
        self.ask_for(host, event, payload, ANSWER)
    }

    fn ask_for(&self, host: &dyn Host, event: &str, mut payload: Value, wait: Duration) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id, tx);
        payload["id"] = json!(id);
        host.emit(event, payload);
        let out = rx.recv_timeout(wait);
        self.pending.lock().unwrap().remove(&id);
        out.map_err(|_| "Pinocchio did not answer (the window may be closed or frozen)".to_string())
    }

    /// Atiende una conexión del conector
    fn serve(&self, stream: TcpStream, host: &dyn Host, token: &str, conn: u64) {
        let _ = stream.set_nonblocking(false);
        let Ok(mut out) = stream.try_clone() else { return };
        let reader = BufReader::new(stream);
        // Lo de antes del turno (para volver si la herramienta falla, y para el deshacer)
        let mut before = Before::default();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(req) if req["token"] == token => {
                    // El avance va a la ventana (su indicador) y al conector
                    let tick = |fraction: f64, message: &str| {
                        host.emit(EV_PROGRESS, json!({ "fraction": fraction, "message": message }));
                        let _ = writeln!(&out, "{}", json!({ "progress": fraction, "message": message }));
                    };
                    self.handle(&req, host, conn, &mut before, &tick).unwrap_or_else(|e| json!({ "ok": false, "error": e }))
                }
                Ok(_) => json!({ "ok": false, "error": "wrong token" }),
                Err(e) => json!({ "ok": false, "error": format!("invalid JSON: {e}") }),
            };
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
        // Se cortó a mitad de un turno: se suelta
        let mut turn = self.turn.lock().unwrap();
        if *turn == Some(conn) {
            *turn = None;
            drop(turn);
            host.emit(EV_DONE, json!({ "ok": false, "changes": [], "title": "" }));
        }
    }

    fn handle(&self, req: &Value, host: &dyn Host, conn: u64, before: &mut Before, tick: &(dyn Fn(f64, &str) + Sync)) -> Result<Value, String> {
        let holds = || *self.turn.lock().unwrap() == Some(conn);
        // Apagado: las conexiones abiertas ya no sirven (el conector vuelve a su motor)
        if !self.is_on() {
            return Err("live mode was turned off in Pinocchio".into());
        }
        match req["op"].as_str().unwrap_or("") {
            "ping" => Ok(json!({ "ok": true, "value": { "paused": self.paused() } })),
            "begin" => {
                if self.paused() {
                    return Err("the user paused the AI in Pinocchio (Settings ▸ AI connector or the banner); ask them to resume".into());
                }
                {
                    let mut turn = self.turn.lock().unwrap();
                    if turn.is_some_and(|t| t != conn) {
                        return Err("another AI is working in Pinocchio right now; try again in a moment".into());
                    }
                    *turn = Some(conn);
                }
                let answer = self.ask(host, EV_TURN, json!({ "title": req["title"] }));
                match answer {
                    Ok(a) if a["ok"] == true => {
                        let state = host.state();
                        before.tabs = Some(state.cad_tabs.lock().unwrap().clone());
                        // Cambia la malla: copia del modelo para volver o para el deshacer de la ventana
                        before.model = (req["model"] == true).then(|| crate::project::take_snapshot_impl(state).id);
                        Ok(json!({ "ok": true }))
                    }
                    other => {
                        *self.turn.lock().unwrap() = None;
                        Err(match other {
                            Ok(a) => format!("the user is busy in Pinocchio ({}); wait a moment and try again", a["reason"].as_str().unwrap_or("editing")),
                            Err(e) => e,
                        })
                    }
                }
            }
            "call" => {
                if !holds() {
                    return Err("no turn: call begin first".into());
                }
                let cmd = req["cmd"].as_str().unwrap_or("");
                match try_dispatch_with(host.state(), cmd, &req["args"], tick) {
                    Some(Ok(Reply::Json(v))) => Ok(json!({ "ok": true, "value": v })),
                    Some(Ok(Reply::Bytes(b))) => Ok(json!({ "ok": true, "bytes": base64::engine::general_purpose::STANDARD.encode(b) })),
                    Some(Err(e)) => Err(e),
                    None => Err(format!("comando interno desconocido: {cmd}")),
                }
            }
            "end" => {
                if !holds() {
                    return Ok(json!({ "ok": true }));
                }
                let ok = req["ok"] == true;
                let state = host.state();
                let model = before.model.take().and_then(|id| {
                    if ok && req["changed"] == true {
                        return Some(id);
                    }
                    if !ok {
                        let _ = crate::project::swap_snapshot_impl(state, id);
                    }
                    state.undo_snapshots.lock().unwrap().remove(id);
                    None
                });
                let changes = match before.tabs.take() {
                    // Falló: todo vuelve a como estaba
                    Some(old) if !ok => {
                        state.set_cad_tabs(old);
                        Vec::new()
                    }
                    // Solo lo que cambia el trabajo va al deshacer (no mirar, ni deshacer)
                    Some(old) if req["changed"] == true => changes(&old, &state.cad_tabs.lock().unwrap()),
                    _ => Vec::new(),
                };
                *self.turn.lock().unwrap() = None;
                host.emit(EV_DONE, json!({ "ok": ok, "title": req["title"], "changes": changes, "model": model }));
                Ok(json!({ "ok": true }))
            }
            // Preguntas a la ventana: lo que el usuario tiene elegido, deshacer,
            // rehacer, importar un modelo o un escaneo (como objeto nuevo) o traer la malla
            // de una pieza del diseño: los hace la ventana, como el usuario
            "selection" | "undo" | "redo" | "import" | "scan" | "use_part" => {
                let model = matches!(req["op"].as_str(), Some("import" | "scan" | "use_part"));
                if model && !holds() {
                    return Err("no turn: call begin first".into());
                }
                let wait = if model { IMPORT } else { ANSWER };
                let a = self.ask_for(host, EV_ASK, json!({ "what": req["op"], "args": req["args"] }), wait)?;
                match a["error"].as_str() {
                    Some(e) => Err(e.to_string()),
                    None => Ok(json!({ "ok": true, "value": a["value"] })),
                }
            }
            other => Err(format!("unknown op {other}")),
        }
    }
}

/// Lo de antes de un turno de la IA
#[derive(Default)]
struct Before {
    /// Las pestañas (para volver si la herramienta falla, y para el deshacer)
    tabs: Option<cad_model::Tabs>,
    /// Copia del modelo de malla (`take_snapshot`), si la herramienta lo cambia
    model: Option<u64>,
}

/// Pestañas que cambiaron entre `old` y `new`, con su documento de antes
/// (un ensamble como su documento virtual: lo que guarda su deshacer)
pub fn changes(old: &cad_model::Tabs, new: &cad_model::Tabs) -> Vec<Change> {
    old.tabs
        .iter()
        .filter_map(|t| {
            let now = new.tabs.iter().find(|n| n.id == t.id)?;
            if now.kind == t.kind {
                return None;
            }
            let before = match &t.kind {
                cad_model::TabKind::Design { document } => (**document).clone(),
                cad_model::TabKind::Assembly { .. } => old.assembly_document(t.id, &[])?,
            };
            Some(Change { tab: t.id, before })
        })
        .collect()
}

// ─── Lado del conector: hablar con la app abierta ───────────────────────────

/// Conexión del conector con la app abierta
pub struct Client {
    info: LiveInfo,
    io: Mutex<(BufReader<TcpStream>, TcpStream)>,
}

impl Client {
    /// Se conecta si la app está abierta con el modo en vivo prendido
    pub fn connect() -> Option<Client> {
        let info: LiveInfo = serde_json::from_str(&std::fs::read_to_string(live_file()?).ok()?).ok()?;
        let stream = TcpStream::connect_timeout(&([127, 0, 0, 1], info.port).into(), Duration::from_millis(300)).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
        let client = Client { io: Mutex::new((BufReader::new(stream.try_clone().ok()?), stream)), info };
        client.request(json!({ "op": "ping" })).ok()?;
        Some(client)
    }

    /// Un pedido y su respuesta (`value` o `bytes`)
    pub fn request(&self, req: Value) -> Result<Value, String> {
        self.request_with(req, &|_, _| {})
    }

    /// Como [`request`](Self::request); el avance que manda la app va a `progress`
    pub fn request_with(&self, mut req: Value, progress: &(dyn Fn(f64, &str) + Sync)) -> Result<Value, String> {
        req["token"] = json!(self.info.token);
        let mut io = self.io.lock().unwrap();
        writeln!(io.1, "{req}").and_then(|_| io.1.flush()).map_err(|e| format!("Pinocchio closed the connection: {e}"))?;
        loop {
            let mut line = String::new();
            io.0.read_line(&mut line).map_err(|e| format!("Pinocchio closed the connection: {e}"))?;
            if line.is_empty() {
                return Err("Pinocchio closed the connection".into());
            }
            let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if let Some(fraction) = reply.get("progress").and_then(Value::as_f64) {
                progress(fraction, reply["message"].as_str().unwrap_or(""));
                continue;
            }
            return if reply["ok"] == true { Ok(reply) } else { Err(reply["error"].as_str().unwrap_or("error").to_string()) };
        }
    }
}

// ─── Comandos de la ventana ─────────────────────────────────────────────────

struct TauriHost(tauri::AppHandle);

impl Host for TauriHost {
    fn state(&self) -> &AppState {
        use tauri::Manager;
        self.0.state::<AppState>().inner()
    }
    fn emit(&self, event: &str, payload: Value) {
        use tauri::Emitter;
        let _ = self.0.emit(event, payload);
    }
}

#[derive(Serialize)]
pub struct LiveStatus {
    pub on: bool,
    pub paused: bool,
    pub port: Option<u16>,
}

fn live_status(bridge: &Bridge) -> LiveStatus {
    LiveStatus { on: bridge.is_on(), paused: bridge.paused(), port: bridge.port() }
}

/// Prende el puente de la ventana (si no estaba): lo usa el chat de la app
pub fn start_for(app: &tauri::AppHandle, bridge: &Arc<Bridge>) -> Result<(), String> {
    bridge.start(Arc::new(TauriHost(app.clone()))).map(|_| ())
}

/// Prende o apaga el modo en vivo
#[tauri::command]
pub fn mcp_live_set(app: tauri::AppHandle, bridge: tauri::State<'_, Arc<Bridge>>, on: bool) -> Result<LiveStatus, String> {
    if on {
        bridge.start(Arc::new(TauriHost(app)))?;
    } else {
        bridge.stop();
    }
    Ok(live_status(&bridge))
}

#[tauri::command]
pub fn mcp_live_pause(bridge: tauri::State<'_, Arc<Bridge>>, paused: bool) -> LiveStatus {
    bridge.set_paused(paused);
    live_status(&bridge)
}

#[tauri::command]
pub fn mcp_live_status(bridge: tauri::State<'_, Arc<Bridge>>) -> LiveStatus {
    live_status(&bridge)
}

/// La ventana contesta una pregunta del puente (turno, selección, deshacer)
#[tauri::command]
pub fn mcp_live_answer(bridge: tauri::State<'_, Arc<Bridge>>, id: u64, payload: Value) {
    bridge.answer(id, payload);
}
