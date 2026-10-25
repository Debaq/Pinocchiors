//! El servidor MCP: atiende cada mensaje y manda respuestas y avisos por
//! `send`. Las herramientas corren de a una y en el orden en que llegan (una
//! cola en `main`); `ping` y la cancelación se atienden mientras tanto.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde_json::{json, Value};

use crate::guide;
use crate::protocol::{self, Incoming};
use crate::schema;
use crate::session::Session;
use crate::tools::{self, Ctx, Effect, Fail, Output};

/// Por dónde salen respuestas y avisos
pub type Sender<'a> = dyn Fn(Value) + Send + Sync + 'a;

pub struct Server {
    session: Session,
    /// Versión del protocolo acordada en `initialize`
    version: Mutex<&'static str>,
    /// Una herramienta a la vez
    work: Mutex<()>,
    /// Pedidos en curso, para cancelarlos
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Server {
    pub fn new(session: Session) -> Self {
        Self { session, version: Mutex::new(protocol::VERSIONS[0]), work: Mutex::new(()), running: Mutex::new(HashMap::new()) }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Un pedido de herramienta entra a la cola: desde ya se puede cancelar
    pub fn queued(&self, msg: &Incoming) {
        if let Some(id) = &msg.id {
            self.running.lock().unwrap().entry(id.to_string()).or_insert_with(|| Arc::new(AtomicBool::new(false)));
        }
    }

    /// Atiende un mensaje
    pub fn handle(&self, msg: Incoming, send: &Sender<'_>) {
        let Incoming { id, method, params } = msg;
        let Some(id) = id else {
            // Avisos: no llevan respuesta
            if method == "notifications/cancelled"
                && let Some(req) = params.get("requestId")
                && let Some(flag) = self.running.lock().unwrap().get(&req.to_string())
            {
                flag.store(true, Ordering::SeqCst);
            }
            return;
        };
        let reply = match method.as_str() {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::all().iter().map(|t| t.listing()).collect::<Vec<_>>() })),
            "tools/call" => self.call_tool(&id, &params, send),
            "resources/list" => Ok(json!({ "resources": resources() })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "resources/read" => read_resource(&params),
            "prompts/list" => Ok(json!({ "prompts": PROMPTS.iter().map(|p| json!({ "name": p.0, "title": p.1, "description": p.2 })).collect::<Vec<_>>() })),
            "prompts/get" => get_prompt(&params),
            other => Err((protocol::METHOD_NOT_FOUND, format!("Unknown method: {other}"))),
        };
        send(match reply {
            Ok(result) => protocol::response(id, result),
            Err((code, message)) => protocol::error(id, code, &message),
        });
    }

    fn initialize(&self, params: &Value) -> Value {
        let version = protocol::negotiate(params.get("protocolVersion").and_then(Value::as_str));
        *self.version.lock().unwrap() = version;
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false },
                "prompts": { "listChanged": false },
            },
            "serverInfo": { "name": "pinocchio", "title": "Pinocchio 3D", "version": env!("CARGO_PKG_VERSION") },
            "instructions": guide::INSTRUCTIONS,
        })
    }

    fn call_tool(&self, id: &Value, params: &Value, send: &Sender<'_>) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let tool = tools::find(name).ok_or_else(|| (protocol::INVALID_PARAMS, format!("Unknown tool: {name}. Call tools/list to see them.")))?;
        let args = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(a) => a.clone(),
        };
        if let Err(e) = schema::check(&(tool.input)(), &args) {
            return Ok(self.result(Err(Fail::new(e).hint(format!("see the inputSchema of `{}` in tools/list", tool.name)))));
        }

        let token = params.pointer("/_meta/progressToken").cloned();
        let progress = |fraction: f64, message: &str| {
            if let Some(token) = &token {
                send(protocol::notification(
                    "notifications/progress",
                    json!({ "progressToken": token, "progress": (fraction.clamp(0.0, 1.0) * 100.0).round(), "total": 100, "message": message }),
                ));
            }
        };
        let cancel = self.running.lock().unwrap().entry(id.to_string()).or_insert_with(|| Arc::new(AtomicBool::new(false))).clone();
        if cancel.load(Ordering::SeqCst) {
            self.running.lock().unwrap().remove(&id.to_string());
            return Ok(self.result(Err(Fail::new("cancelled by the client before it started"))));
        }

        let outcome = {
            let _turn = self.work.lock().unwrap_or_else(|e| e.into_inner());
            let ctx = Ctx { session: &self.session, progress: &progress, cancel: &cancel };
            self.run(tool, &ctx, &args)
        };
        self.running.lock().unwrap().remove(&id.to_string());
        Ok(self.result(outcome))
    }

    /// Corre la herramienta. Un cambio que falla (o entra en pánico) vuelve
    /// el trabajo a como estaba; uno que sale bien queda para `undo`.
    fn run(&self, tool: &tools::Tool, ctx: &Ctx, args: &Value) -> Result<Output, Fail> {
        let before = match tool.effect {
            Effect::Change => Some(self.session.before_change()?),
            _ => None,
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| (tool.run)(ctx, args)))
            .unwrap_or_else(|_| Err(Fail::new(format!("internal error in `{}`", tool.name)).hint("the work was left as it was before this call; try again with other arguments")));
        if let Some(before) = before {
            match &outcome {
                Ok(_) => self.session.commit(tool.title, before),
                Err(_) => self.session.rollback(&before).map_err(|e| Fail::new(format!("could not restore the previous state: {e}")))?,
            }
        }
        outcome
    }

    /// Resultado de `tools/call`: resumen, imágenes y datos (o el error con su pista)
    fn result(&self, outcome: Result<Output, Fail>) -> Value {
        match outcome {
            Ok(out) => {
                let mut content = vec![json!({ "type": "text", "text": out.text })];
                for png in &out.images {
                    content.push(json!({ "type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(png) }));
                }
                let mut result = json!({ "content": content, "isError": false });
                if protocol::has_structured_content(*self.version.lock().unwrap()) {
                    if out.data.is_object() {
                        result["structuredContent"] = out.data;
                    }
                } else if !out.data.is_null() {
                    // Clientes viejos: los datos van como texto
                    result["content"].as_array_mut().unwrap().push(json!({ "type": "text", "text": out.data.to_string() }));
                }
                result
            }
            Err(fail) => {
                let text = match fail.hint {
                    Some(hint) => format!("Error: {}\nHint: {hint}", fail.message),
                    None => format!("Error: {}", fail.message),
                };
                json!({ "content": [{ "type": "text", "text": text }], "isError": true })
            }
        }
    }
}

/// La guía entera y un recurso por tema
fn resources() -> Vec<Value> {
    let mut list = vec![json!({
        "uri": "pinocchio://guide",
        "name": "guide",
        "title": "Pinocchio guide",
        "description": "How to use Pinocchio through these tools: workflows, units and conventions",
        "mimeType": "text/markdown",
    })];
    for (name, title) in guide::topic_list() {
        list.push(json!({ "uri": format!("pinocchio://guide/{name}"), "name": format!("guide-{name}"), "title": title, "mimeType": "text/markdown" }));
    }
    list
}

fn read_resource(params: &Value) -> Result<Value, (i64, String)> {
    let uri = params.get("uri").and_then(Value::as_str).unwrap_or_default();
    let text = match uri.strip_prefix("pinocchio://guide") {
        Some("") => guide::full(),
        Some(rest) => rest.strip_prefix('/').and_then(guide::topic).ok_or((protocol::INVALID_PARAMS, format!("Unknown resource: {uri}")))?,
        None => return Err((protocol::INVALID_PARAMS, format!("Unknown resource: {uri}"))),
    };
    Ok(json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": text }] }))
}

/// Pedidos armados: nombre, título, descripción y el mensaje
const PROMPTS: &[(&str, &str, &str, &str)] = &[(
    "get_started",
    "Get started with Pinocchio",
    "Look at the current work and explain what can be done next",
    "Call the `status` tool to see what is loaded in Pinocchio, then read `help` with topic `overview`. \
Summarise the current work for me in two or three sentences and suggest what I could do next.",
)];

fn get_prompt(params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let p = PROMPTS.iter().find(|p| p.0 == name).ok_or((protocol::INVALID_PARAMS, format!("Unknown prompt: {name}")))?;
    Ok(json!({ "description": p.2, "messages": [{ "role": "user", "content": { "type": "text", "text": p.3 } }] }))
}
