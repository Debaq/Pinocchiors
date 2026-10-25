//! Conectar el conector MCP con los clientes de IA instalados (Claude Code,
//! Claude Desktop, Cursor, Gemini CLI, Codex) y los descargables para el
//! agente: la configuración, la guía y la skill de Claude Code.
//!
//! El comando que se registra es esta misma app con `--mcp` (el AppImage si
//! corre desde uno). Antes del primer cambio a la configuración de un cliente
//! se guarda una copia `<archivo>.pinocchio.bak`; un archivo que no se puede
//! leer no se toca.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};

/// Nombre del servidor en la configuración de los clientes
const SERVER: &str = "pinocchio";

/// El ejecutable que lanzan los clientes y sus argumentos
pub fn command() -> Result<(String, Vec<String>), String> {
    // Desde un AppImage, el ejecutable real está en un montaje temporal: se registra el AppImage
    let exe = match std::env::var_os("APPIMAGE") {
        Some(p) => PathBuf::from(p),
        None => std::env::current_exe().map_err(|e| format!("No se pudo saber dónde está la app: {e}"))?,
    };
    // El binario suelto de desarrollo ya es el conector; la app necesita `--mcp`
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let args = if stem == "pinocchio-mcp" { vec![] } else { vec![super::FLAG.to_string()] };
    Ok((exe.to_string_lossy().into_owned(), args))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `{"mcpServers": {...}}`
    Json,
    /// `[mcp_servers.<nombre>]` en TOML (Codex)
    CodexToml,
}

struct Client {
    id: &'static str,
    name: &'static str,
    /// Archivo de configuración
    config: PathBuf,
    /// Carpeta que dice que el cliente está instalado
    marker: PathBuf,
    format: Format,
}

fn known() -> Vec<Client> {
    let Some(h) = home() else { return Vec::new() };
    let claude_desktop = if cfg!(target_os = "macos") {
        h.join("Library/Application Support/Claude")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| h.join("AppData/Roaming")).join("Claude")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".config")).join("Claude")
    };
    vec![
        Client { id: "claude_code", name: "Claude Code", config: h.join(".claude.json"), marker: h.join(".claude"), format: Format::Json },
        Client { id: "claude_desktop", name: "Claude Desktop", config: claude_desktop.join("claude_desktop_config.json"), marker: claude_desktop, format: Format::Json },
        Client { id: "cursor", name: "Cursor", config: h.join(".cursor/mcp.json"), marker: h.join(".cursor"), format: Format::Json },
        Client { id: "gemini", name: "Gemini CLI", config: h.join(".gemini/settings.json"), marker: h.join(".gemini"), format: Format::Json },
        Client { id: "codex", name: "Codex CLI", config: h.join(".codex/config.toml"), marker: h.join(".codex"), format: Format::CodexToml },
    ]
}

/// Estado de un cliente para la Configuración
#[derive(Serialize)]
pub struct ClientStatus {
    pub id: &'static str,
    pub name: &'static str,
    /// Parece instalado (existe su carpeta)
    pub installed: bool,
    /// Tiene el conector registrado
    pub connected: bool,
    /// Registrado, pero con otra ruta (otra versión de la app)
    pub outdated: bool,
    pub config: String,
}

#[derive(Serialize)]
pub struct McpStatus {
    /// Lo que lanzan los clientes
    pub command: String,
    pub args: Vec<String>,
    pub clients: Vec<ClientStatus>,
    /// La skill de Claude Code está instalada
    pub skill: bool,
    pub skill_path: String,
}

pub fn status() -> Result<McpStatus, String> {
    let (cmd, args) = command()?;
    let clients = known()
        .into_iter()
        .map(|c| {
            let registered = registered_command(&c);
            ClientStatus {
                id: c.id,
                name: c.name,
                installed: c.marker.exists() || c.config.exists(),
                connected: registered.is_some(),
                outdated: registered.is_some_and(|r| r != cmd),
                config: c.config.to_string_lossy().into_owned(),
            }
        })
        .collect();
    let skill = skill_path();
    Ok(McpStatus { command: cmd, args, clients, skill: skill.as_ref().is_some_and(|p| p.exists()), skill_path: skill.map_or_else(String::new, |p| p.to_string_lossy().into_owned()) })
}

/// El comando que tiene registrado el cliente (si lo tiene)
fn registered_command(c: &Client) -> Option<String> {
    let text = std::fs::read_to_string(&c.config).ok()?;
    match c.format {
        Format::Json => serde_json::from_str::<Value>(&text).ok()?["mcpServers"][SERVER]["command"].as_str().map(str::to_string),
        Format::CodexToml => {
            let block = toml_block(&text)?;
            block.lines().find_map(|l| l.trim().strip_prefix("command").map(|r| r.trim_start_matches([' ', '=']).trim().trim_matches('"').replace("\\\\", "\\")))
        }
    }
}

/// Registra (o quita) el conector en el cliente `id`
pub fn connect(id: &str, on: bool) -> Result<McpStatus, String> {
    let c = known().into_iter().find(|c| c.id == id).ok_or_else(|| format!("Cliente desconocido: {id}"))?;
    let (cmd, args) = command()?;
    match c.format {
        Format::Json => edit_json(&c.config, |root| {
            let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
            let servers = servers.as_object_mut().ok_or("«mcpServers» no es un objeto")?;
            if on {
                servers.insert(SERVER.into(), json!({ "command": cmd, "args": args }));
            } else {
                servers.remove(SERVER);
            }
            Ok(())
        })?,
        Format::CodexToml => edit_codex(&c.config, on.then_some((cmd.as_str(), args.as_slice())))?,
    }
    status()
}

/// Copia de seguridad la primera vez que se cambia un archivo de otro programa
fn backup(path: &Path) -> Result<(), String> {
    let bak = PathBuf::from(format!("{}.pinocchio.bak", path.to_string_lossy()));
    if path.exists() && !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| format!("No se pudo respaldar {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Escribe por un archivo temporal y lo cambia de nombre (nunca queda a medias)
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.pinocchio.tmp", path.to_string_lossy()));
    std::fs::write(&tmp, text).map_err(|e| format!("No se pudo escribir {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("No se pudo reemplazar {}: {e}", path.display()))
}

fn edit_json(path: &Path, f: impl FnOnce(&mut Map<String, Value>) -> Result<(), String>) -> Result<(), String> {
    let mut root: Value = match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => json!({}),
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{} no es JSON válido ({e}); no se tocó", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(format!("No se pudo leer {}: {e}", path.display())),
    };
    let obj = root.as_object_mut().ok_or_else(|| format!("{} no es un objeto JSON; no se tocó", path.display()))?;
    f(obj)?;
    backup(path)?;
    write_atomic(path, &(serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n"))
}

const TOML_HEADER: &str = "[mcp_servers.pinocchio]";

/// El bloque `[mcp_servers.pinocchio]` de un config.toml (hasta la próxima sección)
fn toml_block(text: &str) -> Option<String> {
    let start = text.lines().position(|l| l.trim() == TOML_HEADER)?;
    Some(text.lines().skip(start + 1).take_while(|l| !l.trim_start().starts_with('[')).collect::<Vec<_>>().join("\n"))
}

fn edit_codex(path: &Path, register: Option<(&str, &[String])>) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("No se pudo leer {}: {e}", path.display())),
    };
    // Sin el bloque viejo
    let mut out = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.trim() == TOML_HEADER {
            skipping = true;
            continue;
        }
        if skipping && line.trim_start().starts_with('[') {
            skipping = false;
        }
        if !skipping {
            out.push(line.to_string());
        }
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    if let Some((cmd, args)) = register {
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push(TOML_HEADER.into());
        out.push(format!("command = {}", quote(cmd)));
        out.push(format!("args = [{}]", args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(", ")));
    }
    backup(path)?;
    write_atomic(path, &(out.join("\n") + "\n"))
}

/// Configuración para pegar en cualquier cliente (`mcp.json`)
pub fn config_json() -> Result<String, String> {
    let (cmd, args) = command()?;
    serde_json::to_string_pretty(&json!({ "mcpServers": { SERVER: { "command": cmd, "args": args } } })).map_err(|e| e.to_string())
}

/// Dónde va la skill de Claude Code
fn skill_path() -> Option<PathBuf> {
    home().map(|h| h.join(".claude/skills/pinocchio/SKILL.md"))
}

/// La skill de Claude Code: cuándo usar el conector y la guía entera
pub fn skill_markdown() -> String {
    format!(
        "---\nname: pinocchio\ndescription: Design 3D parts, assemblies and 2D engineering drawings with the Pinocchio MCP tools \
(sketches, extrusions, fillets, mates, PDF drawings, STEP/STL export). Use when the user asks to model, assemble or draw a \
part, or mentions Pinocchio.\n---\n\n{}",
        super::guide::agent_markdown()
    )
}

/// Instala (o quita) la skill en `~/.claude/skills/pinocchio`
pub fn install_skill(on: bool) -> Result<McpStatus, String> {
    let path = skill_path().ok_or("No se encontró la carpeta personal")?;
    if on {
        write_atomic(&path, &skill_markdown())?;
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("No se pudo borrar {}: {e}", path.display()))?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
    status()
}

// ─── Comandos de la Configuración ───────────────────────────────────────────

#[tauri::command]
pub fn mcp_status() -> Result<McpStatus, String> {
    status()
}

#[tauri::command]
pub fn mcp_connect(client: String, on: bool) -> Result<McpStatus, String> {
    connect(&client, on)
}

#[tauri::command]
pub fn mcp_skill(on: bool) -> Result<McpStatus, String> {
    install_skill(on)
}

/// Contenido de un descargable: "config" (mcp.json), "guide" (Markdown) o "skill" (SKILL.md)
#[tauri::command]
pub fn mcp_download(kind: String) -> Result<String, String> {
    match kind.as_str() {
        "config" => config_json(),
        "guide" => Ok(super::guide::agent_markdown()),
        "skill" => Ok(skill_markdown()),
        other => Err(format!("Descargable desconocido: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keeps_other_servers_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        std::fs::write(&path, r#"{"theme": "dark", "mcpServers": {"other": {"command": "x"}}}"#).unwrap();
        edit_json(&path, |root| {
            root["mcpServers"].as_object_mut().unwrap().insert(SERVER.into(), json!({ "command": "/app", "args": ["--mcp"] }));
            Ok(())
        })
        .unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["mcpServers"]["pinocchio"]["args"][0], "--mcp");
        let bak: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join("cfg.json.pinocchio.bak")).unwrap()).unwrap();
        assert!(bak["mcpServers"].get("pinocchio").is_none(), "la copia es la de antes");
    }

    #[test]
    fn broken_json_is_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        std::fs::write(&path, "{ esto no es json").unwrap();
        assert!(edit_json(&path, |_| Ok(())).unwrap_err().contains("no se tocó"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ esto no es json");
    }

    #[test]
    fn codex_block_is_added_replaced_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "model = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n").unwrap();
        let args = vec!["--mcp".to_string()];
        edit_codex(&path, Some(("/old/app", &args))).unwrap();
        edit_codex(&path, Some(("C:\\Apps\\Pinocchio.exe", &args))).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches(TOML_HEADER).count(), 1, "{text}");
        assert!(text.contains("command = \"C:\\\\Apps\\\\Pinocchio.exe\"") && text.contains("[mcp_servers.other]"), "{text}");
        let c = Client { id: "codex", name: "Codex", config: path.clone(), marker: dir.path().into(), format: Format::CodexToml };
        assert_eq!(registered_command(&c).as_deref(), Some("C:\\Apps\\Pinocchio.exe"));
        edit_codex(&path, None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("pinocchio") && text.contains("model = \"o4\"") && text.contains("[mcp_servers.other]"), "{text}");
    }

    #[test]
    fn guide_lists_every_tool() {
        let md = super::super::guide::agent_markdown();
        for t in crate::mcp::tools::all() {
            assert!(md.contains(&format!("### `{}`", t.name)), "{}", t.name);
        }
        assert!(skill_markdown().starts_with("---\nname: pinocchio\n"));
    }
}
