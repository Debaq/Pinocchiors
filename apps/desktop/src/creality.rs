//! Creality K1 / K1C / K1 Max con su firmware de fábrica (sin root): el protocolo de
//! Creality Print (P2 de `libs/pinocchio/print3d/PLAN_IMPRESORAS.md`).
//!
//! - `GET http://<ip>/info` → modelo y MAC.
//! - Subir: `POST http://<ip>/upload/<archivo>` multipart con `path` (vacío) y `file`.
//! - WebSocket `ws://<ip>:9999`: la impresora empuja su estado en JSON (temperaturas,
//!   avance, capa, archivo…); las órdenes van como `{"method":"set","params":{…}}`:
//!   `opGcodeFile: "printprt:/usr/data/printer_data/gcodes/<archivo>"` imprime,
//!   `pause: 1/0` pausa y reanuda, `stop: 1` cancela. Así lo hace Orca 2.4.2
//!   (`CrealityPrint.cpp`); la K1 cierra el WebSocket apenas acepta la orden de imprimir.
//!
//! El WebSocket es un cliente mínimo propio (texto, sin extensiones): no hace falta más.

use crate::moonraker::{PrinterStatus, SendResult};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Carpeta de los gcode en las K1
const GCODES: &str = "/usr/data/printer_data/gcodes/";
const WS_PORT: u16 = 9999;

pub struct Creality {
    host: String,
    /// Puerto del WebSocket (9999; otro solo en las pruebas)
    ws_port: u16,
    timeout: Duration,
}

impl Creality {
    pub fn new(address: &str, timeout: Duration) -> Result<Self, String> {
        let a = address.trim().trim_start_matches("http://").trim_start_matches("https://");
        let host = a.split(['/', ':']).next().unwrap_or("").to_string();
        if host.is_empty() {
            return Err("Falta la dirección de la impresora".into());
        }
        Ok(Self { host, ws_port: WS_PORT, timeout })
    }

    fn http(&self) -> Result<reqwest::blocking::Client, String> {
        // rustls con `ring` (como Moonraker y Flashforge): hace falta aunque sea http
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::blocking::Client::builder().timeout(self.timeout).connect_timeout(Duration::from_secs(5)).build().map_err(|e| format!("Error interno: {e}"))
    }

    fn unreachable(&self, e: impl std::fmt::Display) -> String {
        format!("No se pudo conectar a la impresora en {}: {e}. ¿Está encendida y en la misma red?", self.host)
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct CrealityInfo {
    /// Nombre de red (K1-34E2)
    pub name: String,
    pub model: String,
    pub firmware: String,
    /// Estado de la impresión (standby, printing…)
    pub status: String,
}

/// Modelo según `/info` (K1, K1C, K1 Max)
fn model(p: &Creality) -> Result<String, String> {
    let r = p.http()?.get(format!("http://{}/info", p.host)).send().map_err(|e| p.unreachable(e))?;
    let v: Value = r.json().map_err(|_| format!("{} no respondió como una impresora Creality de fábrica", p.host))?;
    Ok(v["model"].as_str().unwrap_or("").to_string())
}

pub fn info(p: &Creality) -> Result<CrealityInfo, String> {
    let model = model(p)?;
    let s = read_state(p, Duration::from_secs(3))?;
    let status = parse_status(&s);
    let firmware = s
        .get("modelVersion")
        .and_then(Value::as_str)
        .and_then(|v| v.split(';').find_map(|part| part.strip_prefix("DWIN sw ver:")))
        .unwrap_or("")
        .to_string();
    Ok(CrealityInfo { name: s.get("hostname").and_then(Value::as_str).unwrap_or(&model).to_string(), model, firmware, status: status.state })
}

/// Junta lo que empuja la impresora por el WebSocket durante `wait` (o hasta tener lo básico)
fn read_state(p: &Creality, wait: Duration) -> Result<Map<String, Value>, String> {
    let mut ws = Ws::connect(&p.host, p.ws_port, p.timeout).map_err(|e| p.unreachable(e))?;
    let end = Instant::now() + wait;
    let mut state = Map::new();
    while Instant::now() < end {
        match ws.read_text(end) {
            Ok(Some(text)) => {
                if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&text) {
                    state.extend(o);
                }
                if ["state", "printProgress", "nozzleTemp", "bedTemp0", "hostname"].iter().all(|k| state.contains_key(*k)) {
                    break;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    if state.is_empty() {
        return Err(format!("{} no mandó su estado por el puerto {WS_PORT}", p.host));
    }
    Ok(state)
}

/// Nombre del estado de la impresión. Los números salen de la K1 (firmware 1.3.x): 0 sin
/// trabajo, 1 imprimiendo, 2 terminada, 3 falló, 4 cancelada, 5 en pausa.
fn state_name(state: i64) -> &'static str {
    match state {
        1 => "printing",
        2 => "complete",
        3 => "error",
        4 => "cancelled",
        5 => "paused",
        _ => "standby",
    }
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).unwrap_or(0.0)
}

pub fn parse_status(s: &Map<String, Value>) -> PrinterStatus {
    let state = state_name(num(s.get("state")) as i64).to_string();
    let progress = (num(s.get("printProgress")) / 100.0).clamp(0.0, 1.0);
    let layers = num(s.get("TotalLayer")) as u32;
    let filename = s.get("printFileName").and_then(Value::as_str).unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
    let left = num(s.get("printLeftTime"));
    PrinterStatus {
        klippy_state: "ready".into(),
        message: s.get("err").and_then(|e| e["value"].as_str()).unwrap_or("").to_string(),
        filename,
        progress,
        print_duration: num(s.get("printJobTime")),
        total_duration: num(s.get("printJobTime")),
        remaining: (state == "printing" || state == "paused").then_some(left),
        layer: (layers > 0).then(|| num(s.get("layer")) as u32),
        layers: (layers > 0).then_some(layers),
        filament_mm: num(s.get("usedMaterialLength")),
        extruder: [num(s.get("nozzleTemp")), num(s.get("targetNozzleTemp"))],
        bed: [num(s.get("bedTemp0")), num(s.get("targetBedTemp0"))],
        slots: Vec::new(),
        current_slot: None,
        state,
    }
}

pub fn status(p: &Creality) -> Result<PrinterStatus, String> {
    Ok(parse_status(&read_state(p, Duration::from_secs(3))?))
}

/// Cuerpo multipart: `path` vacío (como Orca) y el archivo
fn multipart(boundary: &str, filename: &str, gcode: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(gcode.len() + 512);
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"path\"\r\n\r\n\r\n").as_bytes());
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(gcode);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Sube el gcode y, con `start`, lo imprime (si la impresora no está ocupada).
pub fn send(p: &Creality, filename: &str, gcode: &[u8], start: bool) -> Result<SendResult, String> {
    let busy = if start {
        let s = status(p)?;
        s.state == "printing" || s.state == "paused"
    } else {
        false
    };
    let boundary = format!("----pinocchio{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let r = p
        .http()?
        .post(format!("http://{}/upload/{}", p.host, url_encode(filename)))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(multipart(&boundary, filename, gcode))
        .send()
        .map_err(|e| p.unreachable(e))?;
    if !r.status().is_success() {
        return Err(format!("La impresora rechazó el archivo (HTTP {})", r.status().as_u16()));
    }
    let started = start && !busy;
    if started {
        command(p, json!({ "opGcodeFile": format!("printprt:{GCODES}{filename}") }))?;
    }
    Ok(SendResult { filename: filename.to_string(), started })
}

/// Manda una orden `set` por el WebSocket (la impresora puede cerrar enseguida: no es error)
fn command(p: &Creality, params: Value) -> Result<(), String> {
    let mut ws = Ws::connect(&p.host, p.ws_port, p.timeout).map_err(|e| p.unreachable(e))?;
    ws.send_text(&json!({ "method": "set", "params": params }).to_string()).map_err(|e| p.unreachable(e))?;
    let _ = ws.read_text(Instant::now() + Duration::from_millis(500));
    Ok(())
}

/// Pausar, reanudar o cancelar la impresión en curso.
pub fn action(p: &Creality, action: &str) -> Result<(), String> {
    let params = match action {
        "pause" => json!({ "pause": 1 }),
        "resume" => json!({ "pause": 0 }),
        "cancel" => json!({ "stop": 1 }),
        other => return Err(format!("Acción desconocida: {other}")),
    };
    command(p, params)
}

// ─── Controles y detalles ───────────────────────────────────────────────────

use crate::printer_control::{BedMesh, Control, FanKind, FileEntry, HistoryEntry, PrinterDetails, ALL_CONTROLS};

/// Manda un control con las órdenes de Creality (las mismas que usa la web de la impresora).
pub fn control(p: &Creality, c: &Control) -> Result<(), String> {
    c.check()?;
    let params = match c {
        Control::ZOffset { delta } => json!({ "setZOffset": format!("{}{:.3}", if *delta >= 0.0 { "+" } else { "-" }, delta.abs()) }),
        Control::NozzleTemp { celsius } => json!({ "nozzleTempControl": celsius.round() as i64 }),
        Control::BedTemp { celsius } => json!({ "bedTempControl": { "num": 0, "val": celsius.round() as i64 } }),
        Control::Light { on } => json!({ "lightSw": u8::from(*on) }),
        Control::Home { axes } => json!({ "autohome": axes.split_whitespace().collect::<Vec<_>>().join(" ") }),
        Control::Jog { axis, mm } => json!({ "setPosition": format!("{axis}{mm:.2} F{}", if axis == "Z" { 600 } else { 3000 }) }),
        Control::AutoLevel => json!({ "autoLevel": 1 }),
        Control::FeedRate { percent } => json!({ "setFeedratePct": percent.round() as i64 }),
        Control::FlowRate { percent } => json!({ "setFlowratePct": percent.round() as i64 }),
        Control::ExcludeObject { name } => json!({ "excludeObjects": [name] }),
        Control::Reprint { name } => {
            let s = status(p)?;
            if s.state == "printing" || s.state == "paused" {
                return Err("La impresora está ocupada".into());
            }
            json!({ "opGcodeFile": format!("printprt:{GCODES}{name}") })
        }
        Control::DeleteFile { name } => json!({ "opGcodeFile": format!("deleteprt:{GCODES}{name}") }),
        Control::Timelapse { on } => json!({ "videoElapse": u8::from(*on) }),
        Control::RestartKlipper => json!({ "restartKlipper": 1 }),
        Control::RestartFirmware => json!({ "restartFirmware": 1 }),
        Control::ClearError => json!({ "cleanErr": 1 }),
        // El resto como gcode (ventiladores con M106 P0/P1/P2, como la web de Creality)
        other => json!({ "gcodeCmd": other.gcode().ok_or("La impresora no entiende esa orden")? }),
    };
    command(p, params)
}

/// Todo lo que se puede leer: estado, historial, archivos, malla, z offset y objetos.
pub fn details(p: &Creality) -> Result<PrinterDetails, String> {
    let mut ws = Ws::connect(&p.host, p.ws_port, p.timeout).map_err(|e| p.unreachable(e))?;
    for what in ["reqHistory", "reqGcodeFile", "reqProbedMatrix", "ReqPrinterPara", "reqPrintObjects"] {
        ws.send_text(&json!({ "method": "get", "params": { what: 1 } }).to_string()).map_err(|e| p.unreachable(e))?;
    }
    let end = Instant::now() + Duration::from_secs(4);
    let mut state = Map::new();
    let mut files: Vec<Value> = Vec::new();
    while let Ok(Some(text)) = ws.read_text(end) {
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&text) else { continue };
        for (k, v) in o {
            if k == "retGcodeFileInfo2" || k == "retGcodeFileInfo" {
                files.extend(v.as_array().cloned().unwrap_or_default());
            } else {
                state.insert(k, v);
            }
        }
        if ["historyList", "probedMatrix", "curZOffset", "objects"].iter().all(|k| state.contains_key(*k)) && !files.is_empty() {
            break;
        }
    }
    Ok(parse_details(&state, &files))
}

pub fn parse_details(s: &Map<String, Value>, files: &[Value]) -> PrinterDetails {
    let num = |v: Option<&Value>| v.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()));
    let names = |k: &str| -> Vec<String> {
        match s.get(k) {
            Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
            // Creality las manda como texto: "[ \"a\", \"b\" ]"
            Some(Value::String(t)) => serde_json::from_str::<Vec<String>>(t).unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    let mut seen = std::collections::HashSet::new();
    let files: Vec<FileEntry> = files
        .iter()
        .filter(|f| seen.insert(f["name"].as_str().unwrap_or("").to_string()))
        .map(|f| FileEntry {
            name: f["name"].as_str().unwrap_or("").to_string(),
            size: f["file_size"].as_u64().unwrap_or(0),
            modified: num(f.get("create_time")).unwrap_or(0.0),
            print_time_s: num(f.get("timeCost")).filter(|t| *t > 0.0),
            filament_g: num(f.get("filamentWeight")).filter(|g| *g > 0.0),
        })
        .collect();
    let mut files = files;
    files.sort_by(|a, b| b.modified.total_cmp(&a.modified));
    PrinterDetails {
        z_offset: num(s.get("curZOffset")),
        fans: [(FanKind::Model, "modelFanPct"), (FanKind::Aux, "auxiliaryFanPct"), (FanKind::Case, "caseFanPct")]
            .into_iter()
            .filter_map(|(f, k)| num(s.get(k)).map(|v| (f, v)))
            .collect(),
        light: num(s.get("lightSw")).map(|v| v != 0.0),
        feed_rate: num(s.get("curFeedratePct")),
        flow_rate: num(s.get("curFlowratePct")),
        timelapse: num(s.get("videoElapse")).map(|v| v != 0.0),
        total_jobs: num(s.get("totalJob")).map(|v| v as u64),
        total_time_s: num(s.get("totalUsageTime")),
        history: s
            .get("historyList")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|h| HistoryEntry {
                name: h["filename"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string(),
                started: num(h.get("starttime")).unwrap_or(0.0),
                duration_s: num(h.get("usagetime")).unwrap_or(0.0),
                filament_mm: num(h.get("usagematerial")).unwrap_or(0.0),
                finished: num(h.get("printfinish")) == Some(1.0),
            })
            .collect(),
        files,
        mesh: s.get("probedMatrix").and_then(parse_mesh),
        objects: names("objects"),
        excluded: names("excluded_objects"),
        current_object: s.get("current_object").and_then(Value::as_str).filter(|o| !o.is_empty()).map(str::to_string),
        supports: ALL_CONTROLS.to_vec(),
    }
}

/// La malla que manda Creality viene mal armada (corta el texto de printer.cfg por comas y
/// mezcla filas): se juntan todos los números de las z, en orden, y se toman `num`.
fn parse_mesh(m: &Value) -> Option<BedMesh> {
    let val = m["val"].as_array()?;
    let count = m["num"].as_u64()? as usize;
    let side = (count as f64).sqrt().round() as usize;
    if side < 2 || side * side != count {
        return None;
    }
    let mut z = Vec::new();
    for p in val {
        let text = p["z"].as_str().unwrap_or("");
        let mut cur = String::new();
        for ch in text.chars().chain(std::iter::once(' ')) {
            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && cur.is_empty()) {
                cur.push(ch);
            } else {
                if cur.contains('.') && cur.chars().any(|c| c.is_ascii_digit()) && !cur.starts_with('.') {
                    z.push(cur.parse::<f64>().ok()?);
                }
                cur.clear();
            }
        }
    }
    if z.len() < count {
        return None;
    }
    let xs: Vec<f64> = val.iter().filter_map(|p| p["x"].as_str()?.parse().ok()).collect();
    let ys: Vec<f64> = val.iter().filter_map(|p| p["y"].as_str()?.parse().ok()).collect();
    let (min, max) = (|v: &[f64]| v.iter().copied().fold(f64::MAX, f64::min), |v: &[f64]| v.iter().copied().fold(f64::MIN, f64::max));
    Some(BedMesh { rows: z[..count].chunks(side).map(<[f64]>::to_vec).collect(), min: [min(&xs), min(&ys)], max: [max(&xs), max(&ys)] })
}

// ─── WebSocket mínimo ───────────────────────────────────────────────────────

struct Ws {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Ws {
    fn connect(host: &str, port: u16, timeout: Duration) -> std::io::Result<Self> {
        let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other("dirección inválida"))?;
        let mut stream = TcpStream::connect_timeout(&addr, timeout.min(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_read_timeout(Some(timeout.min(Duration::from_secs(5))))?;
        let key = base64(&std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(7).to_le_bytes());
        write!(stream, "GET / HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n")?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let end = loop {
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                return Err(std::io::Error::other("la impresora cerró la conexión"));
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        if !buf.starts_with(b"HTTP/1.1 101") {
            return Err(std::io::Error::other("el puerto 9999 no aceptó el WebSocket"));
        }
        let rest = buf.split_off(end);
        Ok(Self { stream, buf: rest })
    }

    fn fill(&mut self, n: usize, deadline: Instant) -> std::io::Result<()> {
        let mut chunk = [0u8; 16384];
        while self.buf.len() < n {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            self.stream.set_read_timeout(Some(left.max(Duration::from_millis(10))))?;
            let got = self.stream.read(&mut chunk)?;
            if got == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            self.buf.extend_from_slice(&chunk[..got]);
        }
        Ok(())
    }

    fn take(&mut self, n: usize, deadline: Instant) -> std::io::Result<Vec<u8>> {
        self.fill(n, deadline)?;
        let rest = self.buf.split_off(n);
        Ok(std::mem::replace(&mut self.buf, rest))
    }

    /// El siguiente mensaje de texto; `None` si la impresora cerró. Contesta los ping.
    fn read_text(&mut self, deadline: Instant) -> std::io::Result<Option<String>> {
        let mut message = Vec::new();
        loop {
            let head = self.take(2, deadline)?;
            let (fin, op, masked) = (head[0] & 0x80 != 0, head[0] & 0x0F, head[1] & 0x80 != 0);
            let len = match head[1] & 0x7F {
                126 => u16::from_be_bytes(self.take(2, deadline)?.try_into().unwrap()) as usize,
                127 => u64::from_be_bytes(self.take(8, deadline)?.try_into().unwrap()) as usize,
                n => n as usize,
            };
            let mask = if masked { Some(self.take(4, deadline)?) } else { None };
            let mut data = self.take(len, deadline)?;
            if let Some(m) = mask {
                data.iter_mut().enumerate().for_each(|(i, b)| *b ^= m[i % 4]);
            }
            match op {
                0x8 => return Ok(None),
                0x9 => self.frame(0xA, &data)?,
                0x1 | 0x0 => {
                    message.extend_from_slice(&data);
                    if fin {
                        return Ok(Some(String::from_utf8_lossy(&message).into_owned()));
                    }
                }
                _ => {}
            }
        }
    }

    fn send_text(&mut self, text: &str) -> std::io::Result<()> {
        self.frame(0x1, text.as_bytes())
    }

    /// Un cuadro del cliente (con máscara, como pide el protocolo)
    fn frame(&mut self, op: u8, data: &[u8]) -> std::io::Result<()> {
        let mut out = vec![0x80 | op];
        match data.len() {
            n if n < 126 => out.push(0x80 | n as u8),
            n if n < 65536 => {
                out.push(0x80 | 126);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                out.push(0x80 | 127);
                out.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        let mask = (Instant::now().elapsed().as_nanos() as u32 ^ 0x5A3C_96E1).to_le_bytes();
        out.extend_from_slice(&mask);
        out.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&out)
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Una K1 de fábrica de mentira: HTTP en un puerto (info y upload) y el WebSocket en otro.
    /// Guarda lo que recibe.
    struct FakeK1 {
        http_port: u16,
        received: Arc<Mutex<Vec<String>>>,
    }

    fn server_frame(text: &str) -> Vec<u8> {
        let mut out = vec![0x81];
        let n = text.len();
        if n < 126 {
            out.push(n as u8);
        } else {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        out.extend_from_slice(text.as_bytes());
        out
    }

    fn fake(state: Value) -> (FakeK1, u16) {
        let received = Arc::new(Mutex::new(Vec::new()));
        let ws = TcpListener::bind("127.0.0.1:0").unwrap();
        let ws_port = ws.local_addr().unwrap().port();
        let rec = received.clone();
        std::thread::spawn(move || {
            for stream in ws.incoming().flatten() {
                let rec = rec.clone();
                let state = state.clone();
                std::thread::spawn(move || {
                    let mut s = stream;
                    let mut buf = [0u8; 2048];
                    let n = s.read(&mut buf).unwrap_or(0);
                    if !String::from_utf8_lossy(&buf[..n]).contains("Upgrade: websocket") {
                        return;
                    }
                    s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n").unwrap();
                    // El estado llega en dos mensajes, como en la real
                    let o = state.as_object().unwrap();
                    let (a, b): (Map<String, Value>, Map<String, Value>) = o.iter().enumerate().fold((Map::new(), Map::new()), |(mut a, mut b), (i, (k, v))| {
                        if i % 2 == 0 { a.insert(k.clone(), v.clone()) } else { b.insert(k.clone(), v.clone()) };
                        (a, b)
                    });
                    let _ = s.write_all(&server_frame(&Value::Object(a).to_string()));
                    let _ = s.write_all(&server_frame(&Value::Object(b).to_string()));
                    // Lee una orden del cliente (con máscara) si llega
                    s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
                    let mut head = [0u8; 2];
                    if s.read_exact(&mut head).is_ok() {
                        let len = (head[1] & 0x7F) as usize;
                        let mut rest = vec![0u8; 4 + len];
                        if s.read_exact(&mut rest).is_ok() {
                            let (mask, data) = rest.split_at(4);
                            let text: Vec<u8> = data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
                            rec.lock().unwrap().push(String::from_utf8_lossy(&text).into_owned());
                        }
                    }
                });
            }
        });
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let http_port = http.local_addr().unwrap().port();
        let rec = received.clone();
        std::thread::spawn(move || {
            for stream in http.incoming().flatten() {
                let mut s = stream;
                let mut data = Vec::new();
                let mut buf = [0u8; 65536];
                s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
                while let Ok(n) = s.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data);
                    if text.starts_with("GET") && text.contains("\r\n\r\n") || text.contains("--\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&data).into_owned();
                let body = if text.starts_with("GET /info") { r#"{"model":"K1","mac":"FCEE280334E2"}"#.to_string() } else { r#"{"code":0}"#.to_string() };
                rec.lock().unwrap().push(text.lines().next().unwrap_or("").to_string());
                if text.contains("filename=") {
                    rec.lock().unwrap().push(text);
                }
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (FakeK1 { http_port, received }, ws_port)
    }

    /// Creality apuntando a la falsa (el HTTP y el WebSocket en puertos de prueba)
    fn printer(f: &FakeK1, ws_port: u16) -> TestK1 {
        TestK1 { http: format!("127.0.0.1:{}", f.http_port), ws_port }
    }

    struct TestK1 {
        http: String,
        ws_port: u16,
    }

    fn idle() -> Value {
        json!({ "hostname": "K1-34E2", "model": "K1", "state": 4, "printProgress": 0, "nozzleTemp": "102.98", "targetNozzleTemp": 0,
                "bedTemp0": "56.54", "targetBedTemp0": 0, "printFileName": "/usr/data/printer_data/gcodes/aa_pla.gcode", "TotalLayer": 0, "layer": 0,
                "modelVersion": "printer hw ver:;printer sw ver:;DWIN hw ver:CR4CU220812S11;DWIN sw ver:1.3.5.22;", "err": { "errcode": 0, "value": "" } })
    }

    #[test]
    fn estado_de_una_k1_quieta() {
        let (f, ws_port) = fake(idle());
        let t = printer(&f, ws_port);
        let mut ws = Ws::connect("127.0.0.1", t.ws_port, Duration::from_secs(2)).unwrap();
        let mut state = Map::new();
        let end = Instant::now() + Duration::from_secs(2);
        for _ in 0..2 {
            if let Ok(Some(text)) = ws.read_text(end)
                && let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&text)
            {
                state.extend(o);
            }
        }
        let s = parse_status(&state);
        assert_eq!(s.state, "cancelled");
        assert_eq!(s.filename, "aa_pla.gcode");
        assert!((s.extruder[0] - 102.98).abs() < 1e-9 && (s.bed[0] - 56.54).abs() < 1e-9);
        assert_eq!(s.layers, None);
        assert_eq!(s.remaining, None);
    }

    #[test]
    fn imprimiendo() {
        let mut v = idle();
        v["state"] = json!(1);
        v["printProgress"] = json!(42);
        v["TotalLayer"] = json!(200);
        v["layer"] = json!(84);
        v["printLeftTime"] = json!(1800);
        let s = parse_status(v.as_object().unwrap());
        assert_eq!(s.state, "printing");
        assert!((s.progress - 0.42).abs() < 1e-9);
        assert_eq!((s.layer, s.layers, s.remaining), (Some(84), Some(200), Some(1800.0)));
    }

    #[test]
    fn subir_e_imprimir_por_el_protocolo_de_creality() {
        let (f, ws_port) = fake(idle());
        let t = printer(&f, ws_port);
        // La subida va al puerto HTTP de prueba; el WebSocket al de prueba
        let p = Creality { host: t.http.clone(), ws_port: WS_PORT, timeout: Duration::from_secs(3) };
        let r = p.http().unwrap().get(format!("http://{}/info", p.host)).send().unwrap();
        assert_eq!(r.json::<Value>().unwrap()["model"], "K1");
        let boundary = "b0undary";
        let body = multipart(boundary, "cubo 1.gcode", b"G28\n");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"path\"\r\n\r\n\r\n") && text.contains("filename=\"cubo 1.gcode\"") && text.contains("G28"));
        assert_eq!(url_encode("cubo 1.gcode"), "cubo%201.gcode");
        let up = p.http().unwrap().post(format!("http://{}/upload/{}", p.host, url_encode("cubo 1.gcode"))).header("Content-Type", format!("multipart/form-data; boundary={boundary}")).body(body).send().unwrap();
        assert!(up.status().is_success());
        // La orden de imprimir por el WebSocket
        let mut ws = Ws::connect("127.0.0.1", t.ws_port, Duration::from_secs(2)).unwrap();
        ws.send_text(&json!({ "method": "set", "params": { "opGcodeFile": format!("printprt:{GCODES}cubo 1.gcode") } }).to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        let got = f.received.lock().unwrap().clone();
        assert!(got.iter().any(|l| l.starts_with("POST /upload/cubo%201.gcode")), "{got:?}");
        assert!(got.iter().any(|l| l.contains("printprt:/usr/data/printer_data/gcodes/cubo 1.gcode")), "{got:?}");
    }

    /// La malla tal como la manda la K1-34E2 (mal armada por Creality)
    #[test]
    fn malla_de_la_k1_de_fabrica() {
        let zs = ["*# \t0.685105", "0.388844", "0.141594", "-0.091813", "-0.271313\n#*# \t0.506125", "0.282562", "0.108375", "-0.116281",
            "-0.238438\n#*# \t0.523063", "0.299969", "0.050000", "-0.097625", "-0.290094\n#*# \t0.544906", "0.281187", "0.114031", "-0.033031",
            "-0.169615\n#*# \t0.622375", "0.390156", "0.228781", "0.094062", "-0.009813", "0.009813", ".009813", "009813", "09813"];
        let val: Vec<Value> = zs.iter().enumerate().map(|(i, z)| json!({ "x": format!("{:.6}", 5.0 + 52.5 * (i % 5) as f64), "y": format!("{:.6}", 5.0 + 52.5 * (i / 5) as f64), "z": z })).collect();
        let m = parse_mesh(&json!({ "num": 25, "val": val })).unwrap();
        assert_eq!(m.rows.len(), 5);
        assert_eq!(m.rows[0], vec![0.685105, 0.388844, 0.141594, -0.091813, -0.271313]);
        assert_eq!(m.rows[1][0], 0.506125);
        assert_eq!(m.rows[4][4], -0.009813);
        assert_eq!((m.min, m.max), ([5.0, 5.0], [215.0, 215.0]));
    }

    #[test]
    fn detalles_de_la_k1_de_fabrica() {
        let s = json!({ "curZOffset": "0.000", "modelFanPct": 100, "auxiliaryFanPct": 0, "caseFanPct": 0, "lightSw": 1, "curFeedratePct": 100,
            "totalJob": 50, "totalUsageTime": 236172, "objects": "[ \"PIEZA_1\", \"PIEZA_2\" ]", "excluded_objects": "[ ]", "current_object": "PIEZA_1",
            "historyList": [{ "filename": "/usr/data/printer_data/gcodes/cubito.gcode", "starttime": 1793966486, "usagetime": 905, "usagematerial": 1233.07, "printfinish": 1 }] });
        let files = vec![json!({ "name": "a.gcode", "file_size": 10, "create_time": 1, "timeCost": 998, "filamentWeight": "2.51" }), json!({ "name": "a.gcode" })];
        let d = parse_details(s.as_object().unwrap(), &files);
        assert_eq!(d.z_offset, Some(0.0));
        assert_eq!(d.fans.len(), 3);
        assert_eq!(d.objects, vec!["PIEZA_1", "PIEZA_2"]);
        assert_eq!(d.history[0].name, "cubito.gcode");
        assert!(d.history[0].finished);
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].filament_g, Some(2.51));
        assert_eq!(d.total_jobs, Some(50));
    }

    /// Las órdenes llegan por el WebSocket con el formato de la web de Creality
    #[test]
    fn ordenes_de_creality() {
        let (f, ws_port) = fake(idle());
        let p = Creality { host: "127.0.0.1".into(), ws_port, timeout: Duration::from_secs(2) };
        control(&p, &Control::ZOffset { delta: -0.05 }).unwrap();
        control(&p, &Control::BedTemp { celsius: 60.0 }).unwrap();
        control(&p, &Control::Fan { fan: FanKind::Aux, percent: 100.0 }).unwrap();
        control(&p, &Control::Home { axes: "X Y".into() }).unwrap();
        assert!(control(&p, &Control::NozzleTemp { celsius: 500.0 }).is_err());
        std::thread::sleep(Duration::from_millis(500));
        let got = f.received.lock().unwrap().clone();
        for want in [r#""setZOffset":"-0.050""#, r#""bedTempControl":{"num":0,"val":60}"#, r#""gcodeCmd":"M106 P1 S255""#, r#""autohome":"X Y""#] {
            assert!(got.iter().any(|g| g.contains(want)), "falta {want} en {got:?}");
        }
    }

    #[test]
    fn direcciones() {
        assert_eq!(Creality::new("http://192.168.50.49/", Duration::from_secs(1)).unwrap().host, "192.168.50.49");
        assert_eq!(Creality::new("k1.local:80", Duration::from_secs(1)).unwrap().host, "k1.local");
        assert!(Creality::new("  ", Duration::from_secs(1)).is_err());
        assert_eq!(base64(b"hola"), "aG9sYQ==");
    }

    /// Manda un gcode a la K1 de fábrica real (con `PINOCCHIO_START=1` también lo imprime) y
    /// pide la lista de archivos. Solo con el usuario al lado: `PINOCCHIO_CREALITY=ip
    /// PINOCCHIO_GCODE=archivo cargo test -p pinocchio-app --lib creality_enviar_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn creality_enviar_real() {
        let (Ok(addr), Ok(path)) = (std::env::var("PINOCCHIO_CREALITY"), std::env::var("PINOCCHIO_GCODE")) else { return };
        let start = std::env::var("PINOCCHIO_START").is_ok_and(|v| v == "1");
        let p = Creality::new(&addr, Duration::from_secs(60)).unwrap();
        println!("antes: {:?}", status(&p).unwrap().state);
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
        let r = send(&p, &name, &std::fs::read(&path).unwrap(), start).unwrap();
        println!("enviado: {r:?}");
        let mut ws = Ws::connect(&p.host, p.ws_port, Duration::from_secs(5)).unwrap();
        ws.send_text(&json!({ "method": "get", "params": { "reqGcodeFile": 1 } }).to_string()).unwrap();
        let end = Instant::now() + Duration::from_secs(4);
        let mut found = false;
        while let Ok(Some(text)) = ws.read_text(end) {
            if text.contains(&name) {
                found = true;
            }
        }
        println!("en la lista de archivos: {found}");
        std::thread::sleep(Duration::from_secs(3));
        println!("después: {:?}", status(&p).unwrap());
    }

    /// Solo lee la K1 de fábrica real: `PINOCCHIO_CREALITY=192.168.50.49 cargo test -p pinocchio-app
    /// --lib creality_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn creality_real() {
        let Ok(addr) = std::env::var("PINOCCHIO_CREALITY") else { return };
        let p = Creality::new(&addr, Duration::from_secs(10)).unwrap();
        println!("{:?}", info(&p).unwrap());
        println!("{:?}", status(&p).unwrap());
        let d = details(&p).unwrap();
        println!(
            "z_offset {:?} · ventiladores {:?} · luz {:?} · velocidad {:?} · trabajos {:?} · historial {} · archivos {} · malla {:?} · objetos {:?}",
            d.z_offset, d.fans, d.light, d.feed_rate, d.total_jobs, d.history.len(), d.files.len(), d.mesh.as_ref().map(|m| m.rows.len()), d.objects
        );
        println!("último: {:?}", d.history.first());
        println!("malla: {:?}", d.mesh.map(|m| m.rows));
    }

    /// Solo lee una Klipper con Moonraker: `PINOCCHIO_MOONRAKER=ip cargo test -p pinocchio-app --lib
    /// moonraker_detalles_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn moonraker_detalles_real() {
        let Ok(addr) = std::env::var("PINOCCHIO_MOONRAKER") else { return };
        let p = crate::moonraker::Printer::new(&addr, None, Duration::from_secs(10)).unwrap();
        let d = crate::printer_control::moonraker_details(&p).unwrap();
        println!(
            "z_offset {:?} · ventiladores {:?} · velocidad {:?}/{:?} · trabajos {:?} · historial {} · archivos {} · malla {:?} · objetos {:?}",
            d.z_offset, d.fans, d.feed_rate, d.flow_rate, d.total_jobs, d.history.len(), d.files.len(), d.mesh.as_ref().map(|m| (m.rows.len(), m.rows[0].len())), d.objects
        );
    }
}
