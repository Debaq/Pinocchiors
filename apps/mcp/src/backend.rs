//! Quién ejecuta los comandos: el motor propio del conector (sin ventana) o,
//! en el modo en vivo, la app abierta. Las herramientas no saben cuál es.

use pinocchio_app_lib::cad::bridge::{try_dispatch, Reply as AppReply};
use pinocchio_app_lib::AppState;
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
    fn call(&self, cmd: &str, args: Value) -> Result<Reply, String>;
    /// Copia de todo el estado, para deshacer
    fn snapshot(&self) -> Result<Vec<u8>, String>;
    fn restore(&self, snapshot: &[u8]) -> Result<(), String>;
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

    fn call(&self, cmd: &str, args: Value) -> Result<Reply, String> {
        match try_dispatch(&self.state, cmd, &args) {
            Some(Ok(AppReply::Json(v))) => Ok(Reply::Json(v)),
            Some(Ok(AppReply::Bytes(b))) => Ok(Reply::Bytes(b)),
            Some(Err(e)) => Err(e),
            None => Err(format!("comando interno desconocido: {cmd}")),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, String> {
        pinocchio_app_lib::project::snapshot_state(&self.state)
    }

    fn restore(&self, snapshot: &[u8]) -> Result<(), String> {
        pinocchio_app_lib::project::restore_state(&self.state, snapshot)
    }
}
