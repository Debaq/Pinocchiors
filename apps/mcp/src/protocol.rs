//! JSON-RPC 2.0 y lo mínimo de MCP: mensajes, errores y versiones.

use serde_json::{json, Value};

/// Versiones del protocolo MCP que se entienden, de la más nueva a la más vieja
pub const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Versión que se usa con un cliente que pide `requested`: la misma si se
/// conoce; si no, la más nueva (el cliente decide si sigue)
pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested.and_then(|r| VERSIONS.iter().find(|v| **v == r)).copied().unwrap_or(VERSIONS[0])
}

/// La versión trae `structuredContent` en los resultados de herramientas
pub fn has_structured_content(version: &str) -> bool {
    version >= "2025-06-18"
}

// Códigos de error de JSON-RPC
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// Mensaje que llega: pedido (con `id`) o aviso (sin `id`)
#[derive(Debug, Clone)]
pub struct Incoming {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

/// Lee un mensaje. Las respuestas que manda el cliente (sin `method`) y los
/// lotes se ignoran o se rechazan: este servidor no le pide nada al cliente.
pub fn parse(line: &str) -> Result<Option<Incoming>, Value> {
    let v: Value = serde_json::from_str(line).map_err(|e| error(Value::Null, PARSE_ERROR, &format!("Invalid JSON: {e}")))?;
    let Some(obj) = v.as_object() else {
        return Err(error(Value::Null, INVALID_REQUEST, "Expected a JSON-RPC object (batches are not supported)"));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        // Respuesta del cliente a algo que no se le pidió
        return match id {
            Some(_) if obj.contains_key("result") || obj.contains_key("error") => Ok(None),
            _ => Err(error(id.unwrap_or(Value::Null), INVALID_REQUEST, "Missing \"method\"")),
        };
    };
    Ok(Some(Incoming { id, method: method.to_string(), params: obj.get("params").cloned().unwrap_or(json!({})) }))
}

pub fn response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_known_or_newest() {
        assert_eq!(negotiate(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate(Some("1999-01-01")), VERSIONS[0]);
        assert_eq!(negotiate(None), VERSIONS[0]);
    }

    #[test]
    fn parses_requests_notifications_and_rejects_batches() {
        let r = parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap().unwrap();
        assert_eq!((r.id, r.method.as_str()), (Some(json!(1)), "ping"));
        let n = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap().unwrap();
        assert!(n.id.is_none());
        assert_eq!(parse("[]").unwrap_err()["error"]["code"], INVALID_REQUEST);
        assert_eq!(parse("{").unwrap_err()["error"]["code"], PARSE_ERROR);
        assert!(parse(r#"{"jsonrpc":"2.0","id":7,"result":{}}"#).unwrap().is_none());
    }
}
