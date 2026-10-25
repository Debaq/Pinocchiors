//! Chat con proveedores compatibles con la API de OpenAI (DeepSeek, OpenAI,
//! OpenRouter, Ollama, LM Studio…): el agente vive en la app. Manda la
//! conversación con las herramientas del conector como funciones, ejecuta lo
//! que el modelo pide con el mismo servidor MCP (en vivo, por turnos, con su
//! deshacer) y repite hasta que el modelo contesta sin pedir herramientas.
//! A la ventana le llegan los mismos mensajes que con Claude Code
//! (`chat::translate`).
//!
//! Los proveedores se guardan en `<datos>/pinocchio/providers.json` (con la
//! clave; el archivo solo lo lee el usuario).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::chat::Image;

/// Vueltas máximas de herramientas por mensaje (una pieza compleja usa decenas)
const MAX_STEPS: usize = 60;

/// Lo que se le dice al modelo (además de las instrucciones del conector)
const SYSTEM: &str = "You are the assistant of the chat panel inside the Pinocchio 3D app. The user is looking at the \
open Pinocchio window: every change you make with the tools appears there live and can be undone with Ctrl+Z. Use the \
tools to do what the user asks (read `help` topics when unsure; start with `status` if you do not know what is loaded). \
When the user says \"this\", \"here\" or \"the selected one\", call `selection`. Check your work with `view` or \
`asm_check`. Reply briefly, in the user's language, and say what you changed.";

/// Un proveedor guardado
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// Dirección base de la API (…/v1), sin `/chat/completions`
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
    /// El modelo entiende imágenes (las de `view` y las adjuntas)
    #[serde(default)]
    pub vision: bool,
}

/// Lo que ve la ventana de un proveedor (sin la clave)
#[derive(Serialize)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub vision: bool,
    pub has_key: bool,
}

impl From<&Provider> for ProviderView {
    fn from(p: &Provider) -> Self {
        Self { id: p.id.clone(), name: p.name.clone(), base_url: p.base_url.clone(), model: p.model.clone(), vision: p.vision, has_key: !p.api_key.is_empty() }
    }
}

fn store() -> Result<PathBuf, String> {
    let file = super::live::live_file().ok_or("No se encontró la carpeta de datos")?;
    Ok(file.parent().ok_or("No se encontró la carpeta de datos")?.join("providers.json"))
}

pub fn load() -> Vec<Provider> {
    store().ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(list: &[Provider]) -> Result<(), String> {
    let path = store()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(list).map_err(|e| e.to_string())?).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))?;
    // Lleva claves: solo el usuario lo lee
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Lo que manda la ventana al guardar (sin clave = deja la que tenía)
#[derive(Deserialize)]
pub struct ProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default)]
    pub vision: bool,
}

pub fn upsert(input: ProviderInput) -> Result<Vec<Provider>, String> {
    let base = input.base_url.trim().trim_end_matches('/').to_string();
    if !(base.starts_with("https://") || base.starts_with("http://localhost") || base.starts_with("http://127.0.0.1")) {
        return Err("La dirección tiene que empezar con https:// (o http://localhost para un modelo local)".into());
    }
    if input.model.trim().is_empty() {
        return Err("Falta el modelo".into());
    }
    let mut list = load();
    let id = input.id.filter(|i| !i.is_empty()).unwrap_or_else(|| format!("p{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())));
    let old_key = list.iter().find(|p| p.id == id).map(|p| p.api_key.clone()).unwrap_or_default();
    let p = Provider {
        id: id.clone(),
        name: if input.name.trim().is_empty() { base.clone() } else { input.name.trim().to_string() },
        base_url: base,
        api_key: input.api_key.map(|k| k.trim().to_string()).unwrap_or(old_key),
        model: input.model.trim().to_string(),
        vision: input.vision,
    };
    match list.iter_mut().find(|x| x.id == id) {
        Some(x) => *x = p,
        None => list.push(p),
    }
    save(&list)?;
    Ok(list)
}

pub fn remove(id: &str) -> Result<Vec<Provider>, String> {
    let mut list = load();
    list.retain(|p| p.id != id);
    save(&list)?;
    Ok(list)
}

/// HTTPS con rustls y `ring` (se instala una vez)
fn client() -> Result<reqwest::blocking::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("No se pudo preparar la conexión: {e}"))
}

fn post(p: &Provider, body: &Value) -> Result<Value, String> {
    let mut req = client()?.post(format!("{}/chat/completions", p.base_url)).json(body);
    if !p.api_key.is_empty() {
        req = req.bearer_auth(&p.api_key);
    }
    let res = req.send().map_err(|e| format!("No se pudo conectar con {}: {e}", p.name))?;
    let status = res.status();
    let text = res.text().map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
    if !status.is_success() {
        let why = v["error"]["message"].as_str().or(v["error"].as_str()).or(v["message"].as_str()).map(str::to_string).unwrap_or_else(|| text.chars().take(300).collect());
        return Err(format!("{} respondió {}: {why}", p.name, status.as_u16()));
    }
    Ok(v)
}

/// Prueba un proveedor: un mensaje mínimo, sin herramientas
pub fn test(id: &str) -> Result<String, String> {
    let p = load().into_iter().find(|p| p.id == id).ok_or("Ese proveedor ya no está")?;
    let v = post(&p, &json!({ "model": p.model, "messages": [{ "role": "user", "content": "Reply with the single word: ok" }], "max_tokens": 5 }))?;
    let said = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    Ok(format!("{} contestó con {}: «{}»", p.name, v["model"].as_str().unwrap_or(&p.model), said.chars().take(40).collect::<String>()))
}

/// Las herramientas del conector como funciones de la API
fn functions() -> Vec<Value> {
    super::tools::all()
        .into_iter()
        .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": (t.input)() } }))
        .collect()
}

/// Conversación con un proveedor (la historia vive mientras la app está abierta)
#[derive(Default)]
pub struct Agent {
    history: Mutex<Vec<Value>>,
    busy: AtomicBool,
    stop: Arc<AtomicBool>,
    /// El conector de esta conversación (guarda la pestaña en la que trabaja la IA)
    server: Mutex<Option<Arc<super::server::Server>>>,
}

impl Agent {
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.stop();
        self.history.lock().unwrap().clear();
        *self.server.lock().unwrap() = None;
    }

    /// Manda un mensaje; las partes de la respuesta salen por `emit` como las de Claude Code
    pub fn send(self: &Arc<Self>, provider_id: &str, message: &str, images: &[Image], emit: impl Fn(Value) + Send + 'static) -> Result<(), String> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("Todavía está respondiendo el mensaje anterior".into());
        }
        let Some(p) = load().into_iter().find(|p| p.id == provider_id) else {
            self.busy.store(false, Ordering::SeqCst);
            return Err("Ese proveedor ya no está: elígelo de nuevo en el chat".into());
        };
        self.stop.store(false, Ordering::SeqCst);
        // El mensaje del usuario, con sus imágenes si el modelo las ve
        let user = if images.is_empty() {
            json!({ "role": "user", "content": message })
        } else if p.vision {
            let mut parts = vec![json!({ "type": "text", "text": message })];
            parts.extend(images.iter().map(|i| json!({ "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", i.mime, i.data) } })));
            json!({ "role": "user", "content": parts })
        } else {
            json!({ "role": "user", "content": format!("{message}\n\n(The user attached {} image(s), but this model cannot see images.)", images.len()) })
        };
        self.history.lock().unwrap().push(user);
        let agent = self.clone();
        std::thread::spawn(move || {
            let outcome = agent.run(&p, &emit);
            if let Err(e) = outcome {
                emit(json!({ "kind": "done", "ok": false, "error": e }));
            }
            agent.busy.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    fn run(&self, p: &Provider, emit: &impl Fn(Value)) -> Result<(), String> {
        use super::{backend::Auto, protocol, server::Server, session::Session};
        // Las herramientas corren con el conector (en vivo: por turnos y con el deshacer de la ventana)
        let server = self.server.lock().unwrap().get_or_insert_with(|| Arc::new(Server::new(Session::new(Box::new(Auto::new()))))).clone();
        let tools = functions();
        emit(json!({ "kind": "start", "model": p.model }));
        let (mut tokens_in, mut tokens_cache, mut tokens_out) = (0u64, 0u64, 0u64);
        for _ in 0..MAX_STEPS {
            if self.stop.load(Ordering::SeqCst) {
                return Err("Se detuvo".into());
            }
            let mut messages = vec![json!({ "role": "system", "content": format!("{SYSTEM}\n\n{}", super::guide::INSTRUCTIONS) })];
            messages.extend(self.history.lock().unwrap().iter().cloned());
            let v = post(p, &json!({ "model": p.model, "messages": messages, "tools": tools, "tool_choice": "auto" }))?;
            let u = &v["usage"];
            let cached = u["prompt_cache_hit_tokens"].as_u64().or(u["prompt_tokens_details"]["cached_tokens"].as_u64()).unwrap_or(0);
            tokens_cache += cached;
            tokens_in += u["prompt_tokens"].as_u64().unwrap_or(0).saturating_sub(cached);
            tokens_out += u["completion_tokens"].as_u64().unwrap_or(0);
            let msg = v["choices"][0]["message"].clone();
            if msg.is_null() {
                return Err(format!("{} no devolvió respuesta", p.name));
            }
            if let Some(text) = msg["content"].as_str().filter(|t| !t.trim().is_empty()) {
                emit(json!({ "kind": "text", "text": text }));
            }
            let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            // La respuesta tal cual (con sus pedidos de herramientas) queda en la historia
            let mut keep = json!({ "role": "assistant", "content": msg["content"].as_str().unwrap_or("") });
            if !calls.is_empty() {
                keep["tool_calls"] = json!(calls);
            }
            if let Some(r) = msg.get("reasoning_content") {
                keep["reasoning_content"] = r.clone();
            }
            self.history.lock().unwrap().push(keep);
            if calls.is_empty() {
                emit(json!({ "kind": "done", "ok": true, "tokens": { "input": tokens_in, "cache_read": tokens_cache, "cache_write": 0, "output": tokens_out } }));
                return Ok(());
            }
            let mut seen = Vec::new();
            for call in &calls {
                if self.stop.load(Ordering::SeqCst) {
                    return Err("Se detuvo".into());
                }
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = call["function"]["name"].as_str().unwrap_or("").to_string();
                let args: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}));
                emit(json!({ "kind": "tool", "id": id, "name": name, "input": args }));
                // Por el servidor MCP, como cualquier cliente
                let out = std::sync::Mutex::new(None);
                let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
                if let Ok(Some(msg)) = protocol::parse(&line) {
                    server.handle(msg, &|m| *out.lock().unwrap() = Some(m));
                }
                let reply = out.into_inner().unwrap().unwrap_or(json!({}));
                let result = &reply["result"];
                let error = result["isError"] == true || reply.get("error").is_some();
                let mut text: Vec<String> = Vec::new();
                for c in result["content"].as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("text") => text.push(c["text"].as_str().unwrap_or("").to_string()),
                        Some("image") => {
                            let (mime, data) = (c["mimeType"].as_str().unwrap_or("image/png"), c["data"].as_str().unwrap_or(""));
                            emit(json!({ "kind": "image", "mime": mime, "data": data }));
                            seen.push(Image { mime: mime.into(), data: data.into() });
                        }
                        _ => {}
                    }
                }
                if let Some(e) = reply["error"]["message"].as_str() {
                    text.push(format!("Error: {e}"));
                }
                let text = text.join("\n");
                emit(json!({ "kind": "result", "id": id, "error": error, "text": text.chars().take(600).collect::<String>() }));
                self.history.lock().unwrap().push(json!({ "role": "tool", "tool_call_id": id, "content": text }));
            }
            // Las imágenes de `view` no pueden ir en un mensaje de herramienta: van aparte si el modelo ve
            if p.vision && !seen.is_empty() {
                let mut parts = vec![json!({ "type": "text", "text": "Images returned by the tools above:" })];
                parts.extend(seen.iter().map(|i| json!({ "type": "image_url", "image_url": { "url": format!("data:{};base64,{}", i.mime, i.data) } })));
                self.history.lock().unwrap().push(json!({ "role": "user", "content": parts }));
            }
        }
        Err(format!("Se cortó tras {MAX_STEPS} pasos de herramientas sin terminar"))
    }
}

// ─── Comandos de la ventana ─────────────────────────────────────────────────

#[tauri::command]
pub fn ai_providers() -> Vec<ProviderView> {
    load().iter().map(ProviderView::from).collect()
}

#[tauri::command]
pub fn ai_provider_save(provider: ProviderInput) -> Result<Vec<ProviderView>, String> {
    Ok(upsert(provider)?.iter().map(ProviderView::from).collect())
}

#[tauri::command]
pub fn ai_provider_delete(id: String) -> Result<Vec<ProviderView>, String> {
    Ok(remove(&id)?.iter().map(ProviderView::from).collect())
}

/// Un mensaje mínimo para ver que la dirección, la clave y el modelo andan
#[tauri::command]
pub async fn ai_provider_test(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || test(&id)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    /// Servidor falso compatible con OpenAI: primero pide una caja, después contesta
    fn fake_api() -> (String, Arc<Mutex<Vec<Value>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/v1", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let req: Value = serde_json::from_slice(&body).unwrap();
                let n = { let mut l = log.lock().unwrap(); l.push(req); l.len() };
                let reply = if n == 1 {
                    json!({ "choices": [{ "message": { "role": "assistant", "content": "Hago la caja.", "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "cad_primitive", "arguments": "{\"shape\":\"box\",\"dimensions\":[10,20,30]}" } }] } }], "usage": { "prompt_tokens": 100, "completion_tokens": 10, "prompt_cache_hit_tokens": 40 } })
                } else {
                    json!({ "choices": [{ "message": { "role": "assistant", "content": "Listo" } }], "usage": { "prompt_tokens": 150, "completion_tokens": 5 } })
                };
                let text = reply.to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (url, seen)
    }

    #[test]
    fn the_agent_calls_tools_and_answers() {
        if !cad_model::occt::available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: esta prueba es la única del binario que usa la variable
        unsafe { std::env::set_var("PINOCCHIO_MCP_LIVE_FILE", dir.path().join("mcp-live.json")) };
        let (url, seen) = fake_api();
        let list = upsert(ProviderInput { id: None, name: "Falso".into(), base_url: url, api_key: Some("sk-x".into()), model: "falso-1".into(), vision: false }).unwrap();
        let id = list[0].id.clone();
        assert!(load()[0].api_key == "sk-x", "la clave queda guardada");
        assert_eq!(ProviderView::from(&load()[0]).has_key, true);

        let agent = Arc::new(Agent::default());
        let (tx, rx) = std::sync::mpsc::channel();
        agent.send(&id, "haz una caja", &[], move |m| tx.send(m).unwrap()).unwrap();
        let mut events = Vec::new();
        while let Ok(m) = rx.recv_timeout(Duration::from_secs(60)) {
            let done = m["kind"] == "done";
            events.push(m);
            if done {
                break;
            }
        }
        let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["start", "text", "tool", "result", "text", "done"], "{events:?}");
        assert_eq!(events[3]["error"], false, "{}", events[3]);
        assert!(events[3]["text"].as_str().unwrap().contains("Added"), "{}", events[3]);
        let done = &events[5];
        assert_eq!(done["ok"], true);
        assert_eq!(done["tokens"]["input"], 210);
        assert_eq!(done["tokens"]["cache_read"], 40);
        assert_eq!(done["tokens"]["output"], 15);
        // El segundo pedido lleva la herramienta y su resultado, con las funciones y la clave
        let second = &seen.lock().unwrap()[1];
        let msgs = second["messages"].as_array().unwrap();
        assert_eq!(msgs.last().unwrap()["role"], "tool");
        assert_eq!(msgs.last().unwrap()["tool_call_id"], "c1");
        assert!(second["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "cad_primitive"));
    }

    /// HTTPS de verdad (rustls + ring + certificados del sistema): con una clave
    /// falsa la API contesta «no autorizado». Usa la red: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn tls_reaches_a_real_api() {
        let p = Provider { id: "x".into(), name: "DeepSeek".into(), base_url: "https://api.deepseek.com".into(), api_key: "sk-falsa".into(), model: "deepseek-chat".into(), vision: false };
        let e = post(&p, &json!({ "model": p.model, "messages": [{ "role": "user", "content": "hi" }] })).unwrap_err();
        assert!(e.contains("401"), "{e}");
    }

    #[test]
    fn every_tool_is_a_valid_function() {
        for f in functions() {
            let name = f["function"]["name"].as_str().unwrap();
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && name.len() <= 64, "{name}");
            assert_eq!(f["function"]["parameters"]["type"], "object", "{name}");
        }
    }
}
