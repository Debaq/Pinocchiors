//! Quién ejecuta los comandos: el motor propio del conector (sin ventana) o,
//! en el modo en vivo, la app abierta. Las herramientas no saben cuál es.

use crate::cad::bridge::{try_dispatch_with, Reply as AppReply};
use crate::AppState;
use std::sync::Mutex;

use serde_json::Value;

/// Respuesta de un comando
pub enum Reply {
    Json(Value),
    Bytes(Vec<u8>),
}

impl Reply {
    /// El JSON de la respuesta (`null` si fueron bytes)
    pub fn json(self) -> Value {
        match self {
            Reply::Json(v) => v,
            Reply::Bytes(_) => Value::Null,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Motor propio, sin ventana
    Standalone,
    /// La app abierta: los cambios se ven en la ventana
    Live,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Standalone => "standalone",
            Mode::Live => "live",
        }
    }
}

pub trait Backend: Send + Sync {
    fn mode(&self) -> Mode;
    /// Ejecuta un comando de la app por nombre, con los argumentos como los
    /// manda el frontend
    fn call(&self, cmd: &str, args: Value) -> Result<Reply, String> {
        self.call_with(cmd, args, &|_, _| {})
    }
    /// Como [`call`](Self::call), con el avance de los comandos largos
    /// (fracción de 0 a 1 y qué está haciendo)
    fn call_with(&self, cmd: &str, args: Value, progress: &(dyn Fn(f64, &str) + Sync)) -> Result<Reply, String>;
    /// Copia de todo el estado, para deshacer
    fn snapshot(&self) -> Result<Vec<u8>, String>;
    fn restore(&self, snapshot: &[u8]) -> Result<(), String>;
    /// Empieza una herramienta (en vivo: pide turno a la ventana)
    fn begin(&self, _title: &str) -> Result<(), String> {
        Ok(())
    }
    /// Termina la herramienta; `ok = false` deshace lo que hizo (en vivo).
    /// `changed`: la herramienta cambia el trabajo (su cambio va al deshacer de la app)
    fn end(&self, _title: &str, _ok: bool, _changed: bool) {}
    /// Pregunta a la ventana ("selection", "undo", "redo"); sin ventana, `None`
    fn ask(&self, _what: &str) -> Option<Result<Value, String>> {
        None
    }
}

/// Motor propio: el mismo backend de la app, sin ventana
pub struct Local {
    pub state: AppState,
}

impl Local {
    pub fn new() -> Self {
        Self { state: AppState::new() }
    }
}

impl Default for Local {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Local {
    fn mode(&self) -> Mode {
        Mode::Standalone
    }

    fn call_with(&self, cmd: &str, args: Value, progress: &(dyn Fn(f64, &str) + Sync)) -> Result<Reply, String> {
        match try_dispatch_with(&self.state, cmd, &args, progress) {
            Some(Ok(AppReply::Json(v))) => Ok(Reply::Json(v)),
            Some(Ok(AppReply::Bytes(b))) => Ok(Reply::Bytes(b)),
            Some(Err(e)) => Err(e),
            None => Err(format!("comando interno desconocido: {cmd}")),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, String> {
        crate::project::snapshot_state(&self.state)
    }

    fn restore(&self, snapshot: &[u8]) -> Result<(), String> {
        crate::project::restore_state(&self.state, snapshot)
    }
}

/// El que se usa: la app abierta si tiene el modo en vivo prendido, si no el
/// motor propio. Se decide al empezar cada herramienta (`begin`).
pub struct Auto {
    local: Local,
    live: Mutex<Option<crate::mcp::live::Client>>,
    current: Mutex<Mode>,
}

impl Auto {
    pub fn new() -> Self {
        Self { local: Local::new(), live: Mutex::new(None), current: Mutex::new(Mode::Standalone) }
    }

    /// La conexión con la app (se reintenta si se cortó)
    fn live_request(&self, req: Value) -> Result<Value, String> {
        self.live_request_with(req, &|_, _| {})
    }

    fn live_request_with(&self, req: Value, progress: &(dyn Fn(f64, &str) + Sync)) -> Result<Value, String> {
        let mut live = self.live.lock().unwrap();
        let client = live.as_ref().ok_or("Pinocchio closed the connection")?;
        let out = client.request_with(req, progress);
        if out.as_ref().is_err_and(|e| e.contains("closed")) {
            *live = None;
        }
        out
    }

    fn going_live(&self) -> bool {
        let mut live = self.live.lock().unwrap();
        if live.as_ref().is_some_and(|c| c.request(serde_json::json!({ "op": "ping" })).is_ok()) {
            return true;
        }
        *live = crate::mcp::live::Client::connect();
        live.is_some()
    }
}

impl Default for Auto {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Auto {
    fn mode(&self) -> Mode {
        *self.current.lock().unwrap()
    }

    fn call_with(&self, cmd: &str, args: Value, progress: &(dyn Fn(f64, &str) + Sync)) -> Result<Reply, String> {
        if self.mode() == Mode::Standalone {
            return self.local.call_with(cmd, args, progress);
        }
        let r = self.live_request_with(serde_json::json!({ "op": "call", "cmd": cmd, "args": args }), progress)?;
        match r.get("bytes").and_then(Value::as_str) {
            Some(b) => {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(b).map(Reply::Bytes).map_err(|e| e.to_string())
            }
            None => Ok(Reply::Json(r.get("value").cloned().unwrap_or(Value::Null))),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, String> {
        // En vivo el deshacer es el de la app
        if self.mode() == Mode::Live { Ok(Vec::new()) } else { self.local.snapshot() }
    }

    fn restore(&self, snapshot: &[u8]) -> Result<(), String> {
        if self.mode() == Mode::Live { Ok(()) } else { self.local.restore(snapshot) }
    }

    fn begin(&self, title: &str) -> Result<(), String> {
        let live = self.going_live();
        *self.current.lock().unwrap() = if live { Mode::Live } else { Mode::Standalone };
        if live {
            self.live_request(serde_json::json!({ "op": "begin", "title": title }))?;
        }
        Ok(())
    }

    fn end(&self, title: &str, ok: bool, changed: bool) {
        if self.mode() == Mode::Live {
            let _ = self.live_request(serde_json::json!({ "op": "end", "title": title, "ok": ok, "changed": changed }));
        }
    }

    fn ask(&self, what: &str) -> Option<Result<Value, String>> {
        (self.mode() == Mode::Live).then(|| self.live_request(serde_json::json!({ "op": what })).map(|r| r.get("value").cloned().unwrap_or(Value::Null)))
    }
}
