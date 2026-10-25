//! Errores que la IA repite: cada herramienta que falla por lo que pidió la
//! IA (argumentos que no cumplen el esquema, o un pedido que la herramienta no
//! pudo hacer) queda anotada en `mcp-errors.json`, junto a `mcp-live.json`,
//! agrupada por herramienta y mensaje. Los que se repiten muestran una
//! descripción poco clara o una herramienta que debería aceptar más casos
//! (Configuración ▸ Conector para IA los lista). No se anota lo que no es
//! culpa de la IA: el usuario ocupado o en pausa, o una cancelación.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Grupos que se guardan (se olvidan los vistos hace más tiempo)
const MAX: usize = 200;
/// Largo máximo del mensaje y de los argumentos guardados
const TEXT: usize = 400;

/// Un error repetido
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub tool: String,
    /// El mensaje con los números cambiados por `#` (lo que agrupa)
    pub message: String,
    pub count: u32,
    /// Segundos desde 1970
    pub first: u64,
    pub last: u64,
    /// El último mensaje tal cual, con su pista
    pub example: String,
    /// Los últimos argumentos (JSON, recortado)
    pub args: String,
}

/// Dónde se guardan (junto a `mcp-live.json`, así las pruebas que lo cambian
/// tampoco tocan el del usuario)
pub fn file() -> Option<PathBuf> {
    Some(super::live::live_file()?.with_file_name("mcp-errors.json"))
}

/// El mensaje que agrupa: sin números (ids, medidas), sin lo que va entre
/// comillas (nombres y valores que pidió la IA) y sin la pista
fn key(message: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    let mut quoted = false;
    for c in message.chars() {
        if quoted {
            if c == '"' {
                quoted = false;
                out.push_str("…\"");
            }
        } else if c == '"' {
            quoted = true;
            digits = false;
            out.push(c);
        } else if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else if digits && c == '.' {
            // 2.5 → # (el punto queda dentro del número)
        } else {
            digits = false;
            out.push(c);
        }
    }
    cut(&out)
}

fn cut(s: &str) -> String {
    match s.char_indices().nth(TEXT) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Lo anotado, el más repetido primero
pub fn read(path: &Path) -> Vec<Entry> {
    let mut list: Vec<Entry> = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    list.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
    list
}

/// Anota un error de `tool`; si no se puede escribir, sigue sin él
pub fn record(path: &Path, tool: &str, message: &str, hint: Option<&str>, args: &Value) {
    let mut list = read(path);
    let message_key = key(message);
    let example = cut(&match hint {
        Some(h) => format!("{message} — {h}"),
        None => message.to_string(),
    });
    let args = cut(&args.to_string());
    let t = now();
    match list.iter_mut().find(|e| e.tool == tool && e.message == message_key) {
        Some(e) => {
            e.count += 1;
            e.last = t;
            e.example = example;
            e.args = args;
        }
        None => list.push(Entry { tool: tool.into(), message: message_key, count: 1, first: t, last: t, example, args }),
    }
    if list.len() > MAX {
        list.sort_by(|a, b| b.last.cmp(&a.last));
        list.truncate(MAX);
    }
    let Ok(json) = serde_json::to_vec_pretty(&list) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Escritura atómica: otro conector puede estar leyéndolo
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Los errores anotados, para Configuración
#[tauri::command]
pub fn mcp_errors() -> Vec<Entry> {
    file().map(|p| read(&p)).unwrap_or_default()
}

/// Borra lo anotado (después de arreglar las herramientas)
#[tauri::command]
pub fn mcp_errors_clear() -> Result<(), String> {
    match file() {
        Some(p) if p.exists() => std::fs::remove_file(&p).map_err(|e| format!("No se pudo borrar {}: {e}", p.display())),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agrupa_por_herramienta_y_mensaje_sin_numeros() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mcp-errors.json");
        record(&p, "cad_fillet", "edge 12 is not on the solid", Some("see `view`"), &json!({ "edges": [12] }));
        record(&p, "cad_fillet", "edge 7 is not on the solid", None, &json!({ "edges": [7] }));
        record(&p, "cad_fillet", "radius 2.5 is too big", None, &json!({}));
        record(&p, "measure", "edge 7 is not on the solid", None, &json!({}));
        let list = read(&p);
        assert_eq!(list.len(), 3);
        assert_eq!((list[0].tool.as_str(), list[0].message.as_str(), list[0].count), ("cad_fillet", "edge # is not on the solid", 2));
        assert_eq!(list[0].example, "edge 7 is not on the solid");
        assert_eq!(list[0].args, r#"{"edges":[7]}"#);
        assert!(list.iter().any(|e| e.message == "radius # is too big"));
        assert_eq!(key(r#"no tab "Lid" (3 tabs)"#), r#"no tab "…" (# tabs)"#);
    }

    #[test]
    fn archivo_roto_vuelve_a_empezar_y_se_recorta() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/mcp-errors.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{roto").unwrap();
        for i in 0..MAX + 5 {
            record(&p, &format!("tool_{}", "x".repeat(i % 3 + 1)), &format!("problem {}{}", "a".repeat(i), "b"), None, &json!({}));
        }
        assert_eq!(read(&p).len(), MAX);
        let long = "z".repeat(TEXT * 2);
        record(&p, "t", &long, None, &json!({}));
        assert!(read(&p).iter().any(|e| e.message.chars().count() == TEXT + 1));
    }
}
