//! Chat dentro de la app: lo que escribe el usuario va al Claude Code que
//! tiene instalado (`claude -p`, con su cuenta), con el conector MCP de
//! Pinocchio en modo en vivo: los cambios se ven en la ventana. Los eventos de
//! Claude Code (`--output-format stream-json`) llegan a la ventana como
//! mensajes del chat (texto, herramientas que usa, imágenes, fin).
//!
//! Corre en una carpeta propia y solo con la configuración de ese proyecto
//! (`--setting-sources project`): no hereda hooks ni servidores MCP del
//! usuario. Las herramientas permitidas son solo las de Pinocchio.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};

use super::live::Bridge;

/// Evento a la ventana con cada parte de la respuesta
pub const EV_CHAT: &str = "ai-chat://event";

/// Lo que se le agrega a las instrucciones de Claude Code
const PROMPT: &str = "You are the assistant of the chat panel inside the Pinocchio 3D app. The user is looking at the \
open Pinocchio window: every change you make with the pinocchio tools appears there live and can be undone with Ctrl+Z. \
Use only the pinocchio tools (read `help` topics when unsure; start with `status` if you do not know what is loaded). \
When the user says \"this\", \"here\" or \"the selected one\", call `selection`. Check your work with `view` or \
`asm_check`. Reply briefly, in the user's language, and say what you changed.";

/// Alias de modelo que entiende Claude Code (también acepta el nombre completo `claude-…`)
pub const MODELS: &[&str] = &["fable", "opus", "sonnet", "haiku"];
/// Niveles de esfuerzo de Claude Code
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Prefijo de las herramientas del conector en Claude Code
const PREFIX: &str = "mcp__pinocchio__";

#[derive(Default)]
pub struct Chat {
    child: Mutex<Option<Child>>,
    /// Sesión de Claude Code de la conversación (para continuarla)
    session: Mutex<Option<String>>,
}

/// Dónde está `claude` (los programas de escritorio no siempre heredan el PATH de la terminal)
pub fn find_claude() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(h) = &home {
        dirs.extend([h.join(".local/bin"), h.join(".claude/local"), h.join(".npm-global/bin"), h.join("bin")]);
    }
    dirs.extend(["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Carpeta de trabajo del chat (con su `mcp.json`)
fn workdir() -> Result<PathBuf, String> {
    let file = super::live::live_file().ok_or("No se encontró la carpeta de datos")?;
    let dir = file.parent().ok_or("No se encontró la carpeta de datos")?.join("chat");
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let (cmd, args) = super::clients::command()?;
    let config = json!({ "mcpServers": { "pinocchio": { "command": cmd, "args": args } } });
    std::fs::write(dir.join("mcp.json"), config.to_string()).map_err(|e| format!("No se pudo escribir mcp.json: {e}"))?;
    Ok(dir)
}

/// Un evento de Claude Code → los mensajes del chat que salen de él
pub fn translate(event: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    match event["type"].as_str() {
        Some("system") if event["subtype"] == "init" => out.push(json!({ "kind": "start", "session": event["session_id"], "model": event["model"] })),
        Some("assistant") => {
            for c in event["message"]["content"].as_array().into_iter().flatten() {
                match c["type"].as_str() {
                    Some("text") if !c["text"].as_str().unwrap_or("").trim().is_empty() => out.push(json!({ "kind": "text", "text": c["text"] })),
                    Some("tool_use") => {
                        if let Some(name) = c["name"].as_str().and_then(|n| n.strip_prefix(PREFIX)) {
                            out.push(json!({ "kind": "tool", "id": c["id"], "name": name, "input": c["input"] }));
                        }
                    }
                    _ => {}
                }
            }
        }
        Some("user") => {
            for c in event["message"]["content"].as_array().into_iter().flatten().filter(|c| c["type"] == "tool_result") {
                // Las de búsqueda de herramientas no son de Pinocchio
                let blocks: Vec<&Value> = match &c["content"] {
                    Value::Array(a) => a.iter().collect(),
                    other => vec![other],
                };
                if blocks.iter().any(|b| b["type"] == "tool_reference") {
                    continue;
                }
                let text: String = blocks
                    .iter()
                    .filter_map(|b| b.as_str().or_else(|| b["text"].as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(json!({ "kind": "result", "id": c["tool_use_id"], "error": c["is_error"] == true, "text": text.chars().take(600).collect::<String>() }));
                for b in blocks.iter().filter(|b| b["type"] == "image") {
                    if let Some(data) = b["source"]["data"].as_str() {
                        out.push(json!({ "kind": "image", "data": data, "mime": b["source"]["media_type"] }));
                    }
                }
            }
        }
        Some("result") => out.push(json!({ "kind": "done", "ok": event["is_error"] != true, "session": event["session_id"], "cost": event["total_cost_usd"] })),
        _ => {}
    }
    out
}

impl Chat {
    pub fn busy(&self) -> bool {
        self.child.lock().unwrap().as_mut().is_some_and(|c| c.try_wait().ok().flatten().is_none())
    }

    /// Manda un mensaje; la respuesta llega por `emit` a medida que sale.
    /// `model` y `effort`: los de Claude Code (`None` = los suyos por defecto)
    pub fn send(self: &Arc<Self>, message: &str, model: Option<&str>, effort: Option<&str>, emit: impl Fn(Value) + Send + 'static) -> Result<(), String> {
        if self.busy() {
            return Err("Todavía está respondiendo el mensaje anterior".into());
        }
        let claude = find_claude().ok_or("No se encontró Claude Code (`claude`). Instálalo desde claude.com/claude-code e inicia sesión una vez en la terminal.")?;
        let dir = workdir()?;
        let mut cmd = Command::new(&claude);
        cmd.current_dir(&dir)
            .arg("-p")
            .arg(message)
            .args(["--output-format", "stream-json", "--verbose", "--strict-mcp-config", "--setting-sources", "project"])
            .arg("--mcp-config")
            .arg(dir.join("mcp.json"))
            .args(["--allowedTools", "mcp__pinocchio"])
            .args(["--append-system-prompt", PROMPT])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(s) = self.session.lock().unwrap().clone() {
            cmd.args(["--resume", &s]);
        }
        // Solo valores conocidos (van como argumentos de otro programa)
        if let Some(m) = model.filter(|m| MODELS.contains(m) || m.starts_with("claude-") && m.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')) {
            cmd.args(["--model", m]);
        }
        if let Some(e) = effort.filter(|e| EFFORTS.contains(e)) {
            cmd.args(["--effort", e]);
        }
        let mut child = cmd.spawn().map_err(|e| format!("No se pudo abrir Claude Code: {e}"))?;
        let stdout = child.stdout.take().ok_or("sin salida")?;
        let mut stderr = child.stderr.take().ok_or("sin salida de errores")?;
        *self.child.lock().unwrap() = Some(child);
        let chat = self.clone();
        std::thread::spawn(move || {
            let mut finished = false;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                for msg in translate(&event) {
                    if msg["kind"] == "start" || msg["kind"] == "done" {
                        if let Some(s) = msg["session"].as_str() {
                            *chat.session.lock().unwrap() = Some(s.to_string());
                        }
                    }
                    finished |= msg["kind"] == "done";
                    emit(msg);
                }
            }
            let mut err = String::new();
            let _ = stderr.read_to_string(&mut err);
            let code = chat.child.lock().unwrap().take().and_then(|mut c| c.wait().ok());
            if !finished {
                let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
                let message = match code {
                    Some(c) if c.success() => "Claude Code terminó sin respuesta".to_string(),
                    _ if why.is_empty() => "Se detuvo".to_string(),
                    _ => why,
                };
                emit(json!({ "kind": "done", "ok": false, "error": message }));
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    pub fn reset(&self) {
        self.stop();
        *self.session.lock().unwrap() = None;
    }
}

// ─── Comandos de la ventana ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ChatStatus {
    /// Ruta de `claude` si está instalado
    pub claude: Option<String>,
    pub busy: bool,
    /// Hay una conversación en curso (el próximo mensaje la continúa)
    pub ongoing: bool,
}

#[tauri::command]
pub fn ai_chat_status(chat: tauri::State<'_, Arc<Chat>>) -> ChatStatus {
    ChatStatus { claude: find_claude().map(|p| p.to_string_lossy().into_owned()), busy: chat.busy(), ongoing: chat.session.lock().unwrap().is_some() }
}

/// Manda un mensaje al chat; prende el modo en vivo para que los cambios se vean
#[tauri::command]
pub fn ai_chat_send(
    app: tauri::AppHandle,
    chat: tauri::State<'_, Arc<Chat>>,
    bridge: tauri::State<'_, Arc<Bridge>>,
    message: String,
    model: Option<String>,
    effort: Option<String>,
) -> Result<(), String> {
    super::live::start_for(&app, &bridge)?;
    let handle = app.clone();
    chat.send(&message, model.as_deref(), effort.as_deref(), move |msg| {
        use tauri::Emitter;
        let _ = handle.emit(EV_CHAT, msg);
    })
}

#[tauri::command]
pub fn ai_chat_stop(chat: tauri::State<'_, Arc<Chat>>) {
    chat.stop();
}

/// Conversación nueva (la próxima no continúa la anterior)
#[tauri::command]
pub fn ai_chat_reset(chat: tauri::State<'_, Arc<Chat>>) {
    chat.reset();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_events_become_chat_messages() {
        let init = json!({ "type": "system", "subtype": "init", "session_id": "s1", "model": "claude-opus-5-5" });
        assert_eq!(translate(&init), vec![json!({ "kind": "start", "session": "s1", "model": "claude-opus-5-5" })]);
        let assistant = json!({ "type": "assistant", "message": { "content": [
            { "type": "text", "text": "Voy a redondear." },
            { "type": "tool_use", "id": "t1", "name": "mcp__pinocchio__cad_fillet", "input": { "radius": 2 } },
            { "type": "tool_use", "id": "t0", "name": "ToolSearch", "input": {} }
        ] } });
        let out = translate(&assistant);
        assert_eq!(out.len(), 2, "ToolSearch no es de Pinocchio: {out:?}");
        assert_eq!(out[1]["name"], "cad_fillet");
        let result = json!({ "type": "user", "message": { "content": [
            { "type": "tool_result", "tool_use_id": "t1", "content": [{ "type": "text", "text": "Rounded 4 edge(s)." }, { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "AAAA" } }] },
            { "type": "tool_result", "tool_use_id": "t0", "content": [{ "type": "tool_reference", "tool_name": "x" }] }
        ] } });
        let out = translate(&result);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0]["text"], "Rounded 4 edge(s).");
        assert_eq!(out[1]["kind"], "image");
        let done = json!({ "type": "result", "is_error": false, "session_id": "s1", "total_cost_usd": 0.01 });
        assert_eq!(translate(&done)[0]["kind"], "done");
    }
}
