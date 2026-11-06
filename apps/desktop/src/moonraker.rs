//! Impresoras con Moonraker (la API de Klipper): probar la conexión,
//! subir el gcode del último laminado (e imprimirlo), ver el estado y pausar,
//! reanudar o cancelar. Primera impresora: Creality K1 rooteada (puerto 7125).
//!
//! Ver S7 en `libs/pinocchio/print3d/PLAN_SLICER.md`.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Puerto de Moonraker si la dirección no trae uno
const DEFAULT_PORT: u16 = 7125;

/// `192.168.1.50` → `http://192.168.1.50:7125`; respeta esquema, puerto y quita la `/` final.
pub fn normalize_address(address: &str) -> Result<String, String> {
    let a = address.trim().trim_end_matches('/');
    if a.is_empty() {
        return Err("Falta la dirección de la impresora".into());
    }
    let (scheme, rest) = match a.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => ("http".to_string(), a),
    };
    if scheme != "http" && scheme != "https" {
        return Err(format!("Dirección de impresora no válida: {address}"));
    }
    // Solo el host (y puerto): lo que venga después de una «/» sobra
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(format!("Dirección de impresora no válida: {address}"));
    }
    // Con puerto: «host:1234» o «[ipv6]:1234»
    let has_port = match host.rfind(':') {
        Some(i) => host[i + 1..].chars().all(|c| c.is_ascii_digit()) && (!host.starts_with('[') || host[..i].ends_with(']')),
        None => false,
    };
    Ok(if has_port { format!("{scheme}://{host}") } else { format!("{scheme}://{host}:{DEFAULT_PORT}") })
}

/// Nombre de archivo seguro para la impresora (sin carpetas ni símbolos raros), terminado en `.gcode`.
pub fn safe_filename(name: &str) -> String {
    let base = name.trim().trim_end_matches(".gcode");
    let mut out: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    if out.is_empty() {
        out = "pinocchio".into();
    }
    out.truncate(out.char_indices().nth(100).map_or(out.len(), |(i, _)| i));
    format!("{out}.gcode")
}

pub(crate) struct Printer {
    base: String,
    api_key: Option<String>,
    client: reqwest::blocking::Client,
}

impl Printer {
    pub(crate) fn new(address: &str, api_key: Option<String>, timeout: Duration) -> Result<Self, String> {
        let base = normalize_address(address)?;
        // rustls con `ring` (como el chat del conector): hace falta aunque casi siempre sea http
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(timeout)
            .build()
            .map_err(|e| format!("No se pudo preparar la conexión: {e}"))?;
        Ok(Self { base, api_key: api_key.filter(|k| !k.trim().is_empty()), client })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        let req = self.client.request(method, format!("{}{path}", self.base));
        match &self.api_key {
            Some(k) => req.header("X-Api-Key", k.trim()),
            None => req,
        }
    }

    /// Manda el pedido y devuelve `result` de la respuesta de Moonraker.
    fn send(&self, req: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
        let resp = req.send().map_err(|e| self.connection_error(&e))?;
        let status = resp.status();
        let body: Value = resp.json().unwrap_or(Value::Null);
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err("La impresora pide una clave de API (Moonraker)".into());
        }
        if !status.is_success() {
            let msg = body.pointer("/error/message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| status.to_string());
            return Err(format!("La impresora respondió: {msg}"));
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    pub(crate) fn get(&self, path: &str) -> Result<Value, String> {
        self.send(self.request(reqwest::Method::GET, path))
    }

    pub(crate) fn post(&self, path: &str) -> Result<Value, String> {
        self.send(self.request(reqwest::Method::POST, path))
    }

    pub(crate) fn delete(&self, path: &str) -> Result<Value, String> {
        self.send(self.request(reqwest::Method::DELETE, path))
    }

    fn connection_error(&self, e: &reqwest::Error) -> String {
        if e.is_timeout() {
            format!("La impresora no respondió a tiempo ({})", self.base)
        } else {
            format!("No se pudo conectar con la impresora en {}: ¿está encendida y en la misma red?", self.base)
        }
    }
}


// ── Conexión ────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct PrinterInfo {
    /// Dirección completa que se usó (con esquema y puerto)
    pub url: String,
    pub hostname: String,
    /// ready, startup, shutdown, error o disconnected
    pub klippy_state: String,
    /// Lo que dice Klipper si no está listo
    pub message: String,
    pub moonraker_version: String,
    pub klipper_version: String,
}

pub(crate) fn info(p: &Printer) -> Result<PrinterInfo, String> {
    let server = p.get("/server/info")?;
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let klippy_state = s(&server, "klippy_state");
    // /printer/info solo responde con Klipper conectado
    let printer = if klippy_state == "ready" || klippy_state == "startup" { p.get("/printer/info").unwrap_or(Value::Null) } else { Value::Null };
    let mut message = s(&printer, "state_message");
    if message.is_empty() && klippy_state != "ready" {
        message = server.get("warnings").and_then(Value::as_array).and_then(|w| w.first()).and_then(Value::as_str).unwrap_or("").to_string();
    }
    Ok(PrinterInfo {
        url: p.base.clone(),
        hostname: s(&printer, "hostname"),
        klippy_state,
        message: message.trim().to_string(),
        moonraker_version: s(&server, "moonraker_version"),
        klipper_version: s(&printer, "software_version"),
    })
}


// ── Enviar ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SendResult {
    /// Nombre con el que quedó en la impresora
    pub filename: String,
    /// Empezó a imprimir (si se pidió); `false` si la impresora estaba ocupada
    pub started: bool,
}

/// Cuerpo multipart/form-data para `/server/files/upload`.
fn multipart(boundary: &str, filename: &str, gcode: &[u8], start: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(gcode.len() + 512);
    let field = |body: &mut Vec<u8>, name: &str, value: &str| {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    field(&mut body, "root", "gcodes");
    if start {
        field(&mut body, "print", "true");
    }
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(gcode);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

pub(crate) fn send_gcode(p: &Printer, filename: &str, gcode: &[u8], start: bool) -> Result<SendResult, String> {
    let boundary = format!("pinocchio{:016x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64));
    let req = p
        .request(reqwest::Method::POST, "/server/files/upload")
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(multipart(&boundary, filename, gcode, start));
    // La respuesta de la subida no viene envuelta en «result» en todas las versiones
    let resp = req.send().map_err(|e| p.connection_error(&e))?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("La impresora pide una clave de API (Moonraker)".into());
    }
    if !status.is_success() {
        let msg = body.pointer("/error/message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| status.to_string());
        return Err(format!("La impresora no aceptó el gcode: {msg}"));
    }
    let result = body.get("result").unwrap_or(&body);
    let filename = result.pointer("/item/path").and_then(Value::as_str).unwrap_or(filename).to_string();
    let started = start && result.get("print_started").and_then(Value::as_bool).unwrap_or(false);
    Ok(SendResult { filename, started })
}


/// Pausar, reanudar o cancelar la impresión en curso.
pub(crate) fn action(p: &Printer, action: &str) -> Result<(), String> {
    let path = match action {
        "pause" => "/printer/print/pause",
        "resume" => "/printer/print/resume",
        "cancel" => "/printer/print/cancel",
        other => return Err(format!("Acción desconocida: {other}")),
    };
    p.post(path).map(|_| ())
}

// ── Estado ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone, Default, PartialEq)]
pub struct PrinterStatus {
    /// Klipper: ready, startup, shutdown, error o disconnected
    pub klippy_state: String,
    /// Impresión: standby, printing, paused, complete, cancelled o error
    pub state: String,
    pub message: String,
    pub filename: String,
    /// 0–1 (por el avance en el archivo)
    pub progress: f64,
    /// Segundos imprimiendo (sin pausas) y en total
    pub print_duration: f64,
    pub total_duration: f64,
    /// Lo que falta según el laminador (o, sin eso, según el avance)
    pub remaining: Option<f64>,
    pub layer: Option<u32>,
    pub layers: Option<u32>,
    pub filament_mm: f64,
    pub extruder: [f64; 2],
    pub bed: [f64; 2],
    /// Carretes del cambiador (AD5X); vacío en las que tienen uno
    pub slots: Vec<crate::flashforge::Slot>,
    pub current_slot: Option<u32>,
}

/// Tiempo estimado por el laminador para cada archivo (no cambia mientras imprime).
static ESTIMATES: Mutex<Option<HashMap<String, Option<f64>>>> = Mutex::new(None);

fn estimate(p: &Printer, filename: &str) -> Option<f64> {
    if filename.is_empty() {
        return None;
    }
    let key = format!("{}|{filename}", p.base);
    if let Some(v) = ESTIMATES.lock().ok()?.as_ref().and_then(|m| m.get(&key)) {
        return *v;
    }
    let meta = p.get(&format!("/server/files/metadata?filename={}", urlencode(filename))).ok();
    let est = meta.and_then(|m| m.get("estimated_time").and_then(Value::as_f64)).filter(|t| *t > 0.0);
    ESTIMATES.lock().ok()?.get_or_insert_with(HashMap::new).insert(key, est);
    est
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Lo que falta: con la estimación del laminador, ella menos lo impreso; si no, por el avance.
fn remaining(estimated: Option<f64>, print_duration: f64, progress: f64) -> Option<f64> {
    match estimated {
        Some(e) => Some((e - print_duration).max(0.0)),
        None if progress > 0.01 => Some((print_duration / progress - print_duration).max(0.0)),
        None => None,
    }
}

fn parse_status(klippy_state: String, status: &Value) -> PrinterStatus {
    let f = |path: &str| status.pointer(path).and_then(Value::as_f64).unwrap_or(0.0);
    let s = |path: &str| status.pointer(path).and_then(Value::as_str).unwrap_or("").to_string();
    let u = |path: &str| status.pointer(path).and_then(Value::as_u64).map(|v| v as u32);
    PrinterStatus {
        klippy_state,
        state: s("/print_stats/state"),
        message: s("/print_stats/message"),
        filename: s("/print_stats/filename"),
        progress: status.pointer("/virtual_sdcard/progress").and_then(Value::as_f64).unwrap_or_else(|| f("/display_status/progress")).clamp(0.0, 1.0),
        print_duration: f("/print_stats/print_duration"),
        total_duration: f("/print_stats/total_duration"),
        remaining: None,
        layer: u("/print_stats/info/current_layer"),
        layers: u("/print_stats/info/total_layer"),
        filament_mm: f("/print_stats/filament_used"),
        extruder: [f("/extruder/temperature"), f("/extruder/target")],
        bed: [f("/heater_bed/temperature"), f("/heater_bed/target")],
        slots: Vec::new(),
        current_slot: None,
    }
}

pub(crate) fn status(p: &Printer) -> Result<PrinterStatus, String> {
    let server = p.get("/server/info")?;
    let klippy_state = server.get("klippy_state").and_then(Value::as_str).unwrap_or("").to_string();
    if klippy_state != "ready" {
        let message = server.get("warnings").and_then(Value::as_array).and_then(|w| w.first()).and_then(Value::as_str).unwrap_or("").to_string();
        return Ok(PrinterStatus { klippy_state, message, ..Default::default() });
    }
    let result = p.get("/printer/objects/query?webhooks&print_stats&virtual_sdcard&display_status&extruder&heater_bed")?;
    let mut st = parse_status(klippy_state, result.get("status").unwrap_or(&Value::Null));
    if matches!(st.state.as_str(), "printing" | "paused") {
        st.remaining = remaining(estimate(p, &st.filename), st.print_duration, st.progress);
    }
    Ok(st)
}



#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    #[test]
    fn direcciones() {
        assert_eq!(normalize_address("192.168.1.50").unwrap(), "http://192.168.1.50:7125");
        assert_eq!(normalize_address(" k1.local/ ").unwrap(), "http://k1.local:7125");
        assert_eq!(normalize_address("http://10.0.0.2:7125/server/info").unwrap(), "http://10.0.0.2:7125");
        assert_eq!(normalize_address("HTTPS://impresora:443").unwrap(), "https://impresora:443");
        assert_eq!(normalize_address("[fe80::1]").unwrap(), "http://[fe80::1]:7125");
        assert_eq!(normalize_address("[fe80::1]:80").unwrap(), "http://[fe80::1]:80");
        assert!(normalize_address("  ").is_err());
        assert!(normalize_address("ftp://x").is_err());
        assert!(normalize_address("mi impresora").is_err());
    }

    #[test]
    fn nombres_de_archivo() {
        assert_eq!(safe_filename("Pieza 1.gcode"), "Pieza 1.gcode");
        assert_eq!(safe_filename("../../etc/passwd"), "_.._etc_passwd.gcode");
        assert_eq!(safe_filename("tapa/caja:v2"), "tapa_caja_v2.gcode");
        assert_eq!(safe_filename("   "), "pinocchio.gcode");
        assert_eq!(safe_filename("engranaje"), "engranaje.gcode");
    }

    #[test]
    fn tiempo_que_falta() {
        assert_eq!(remaining(Some(3600.0), 600.0, 0.5), Some(3000.0));
        assert_eq!(remaining(Some(100.0), 600.0, 0.9), Some(0.0));
        assert_eq!(remaining(None, 600.0, 0.25), Some(1800.0));
        assert_eq!(remaining(None, 5.0, 0.0), None);
    }

    #[test]
    fn multipart_con_y_sin_imprimir() {
        let body = String::from_utf8(multipart("XYZ", "a.gcode", b"G28\n", true)).unwrap();
        assert!(body.starts_with("--XYZ\r\n"));
        assert!(body.contains("name=\"root\"\r\n\r\ngcodes\r\n"));
        assert!(body.contains("name=\"print\"\r\n\r\ntrue\r\n"));
        assert!(body.contains("filename=\"a.gcode\""));
        assert!(body.ends_with("G28\n\r\n--XYZ--\r\n"));
        let body = String::from_utf8(multipart("XYZ", "a.gcode", b"G28\n", false)).unwrap();
        assert!(!body.contains("name=\"print\""));
    }

    /// Lo que recibió el Moonraker falso
    #[derive(Default)]
    struct Seen {
        requests: Vec<(String, String, Option<String>, Vec<u8>)>,
    }

    /// Moonraker falso: responde según la ruta; guarda método, ruta, clave y cuerpo.
    fn fake(handler: impl Fn(&str, &str) -> (u16, Value) + Send + Sync + 'static) -> (String, Arc<Mutex<Seen>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.is_empty() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let (mut len, mut key) = (0usize, None);
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    let (k, v) = h.split_once(':').unwrap_or((h, ""));
                    match k.to_ascii_lowercase().as_str() {
                        "content-length" => len = v.trim().parse().unwrap_or(0),
                        "x-api-key" => key = Some(v.trim().to_string()),
                        _ => {}
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let (code, value) = handler(&method, &path);
                seen2.lock().unwrap().requests.push((method, path, key, body));
                let text = value.to_string();
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (addr, seen)
    }

    fn printer(addr: &str, key: Option<&str>) -> Printer {
        Printer::new(addr, key.map(str::to_string), Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn conexion_con_klipper_listo() {
        let (addr, seen) = fake(|_, path| match path {
            "/server/info" => (200, json!({"result": {"klippy_state": "ready", "moonraker_version": "v0.8.0"}})),
            "/printer/info" => (200, json!({"result": {"state": "ready", "state_message": "Printer is ready\n", "hostname": "K1-1234", "software_version": "v0.10.0"}})),
            _ => (404, json!({})),
        });
        let i = info(&printer(&addr, Some("clave"))).unwrap();
        assert_eq!(i.url, format!("http://{addr}"));
        assert_eq!((i.hostname.as_str(), i.klippy_state.as_str(), i.message.as_str()), ("K1-1234", "ready", "Printer is ready"));
        assert_eq!((i.moonraker_version.as_str(), i.klipper_version.as_str()), ("v0.8.0", "v0.10.0"));
        assert!(seen.lock().unwrap().requests.iter().all(|r| r.2.as_deref() == Some("clave")));
    }

    #[test]
    fn klipper_apagado_no_pregunta_por_la_impresora() {
        let (addr, seen) = fake(|_, path| match path {
            "/server/info" => (200, json!({"result": {"klippy_state": "shutdown", "warnings": ["Klipper reports: SHUTDOWN"]}})),
            _ => (500, json!({})),
        });
        let i = info(&printer(&addr, None)).unwrap();
        assert_eq!((i.klippy_state.as_str(), i.message.as_str()), ("shutdown", "Klipper reports: SHUTDOWN"));
        let s = status(&printer(&addr, None)).unwrap();
        assert_eq!(s.klippy_state, "shutdown");
        assert!(seen.lock().unwrap().requests.iter().all(|r| r.1 == "/server/info"));
    }

    #[test]
    fn sube_e_imprime() {
        let (addr, seen) = fake(|method, path| match (method, path) {
            ("POST", "/server/files/upload") => (201, json!({"result": {"item": {"path": "caja.gcode", "root": "gcodes"}, "print_started": true, "action": "create_file"}})),
            _ => (404, json!({})),
        });
        let r = send_gcode(&printer(&addr, None), "caja.gcode", b";gcode\nG28\n", true).unwrap();
        assert_eq!(r, SendResult { filename: "caja.gcode".into(), started: true });
        let seen = seen.lock().unwrap();
        let body = String::from_utf8_lossy(&seen.requests[0].3);
        assert!(body.contains(";gcode\nG28\n") && body.contains("name=\"print\""));
    }

    #[test]
    fn ocupada_sube_pero_no_empieza() {
        let (addr, _) = fake(|_, _| (201, json!({"item": {"path": "caja.gcode"}, "print_started": false})));
        let r = send_gcode(&printer(&addr, None), "caja.gcode", b"G28\n", true).unwrap();
        assert!(!r.started);
    }

    #[test]
    fn errores_de_moonraker_y_de_clave() {
        let (addr, _) = fake(|_, path| match path {
            "/printer/print/pause" => (400, json!({"error": {"code": 400, "message": "Print not in progress"}})),
            _ => (401, json!({"error": {"message": "Unauthorized"}})),
        });
        let p = printer(&addr, None);
        assert_eq!(p.post("/printer/print/pause").unwrap_err(), "La impresora respondió: Print not in progress");
        assert_eq!(info(&p).unwrap_err(), "La impresora pide una clave de API (Moonraker)");
    }

    #[test]
    fn sin_impresora_dice_que_no_conecta() {
        // Un puerto recién liberado: nadie escucha
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = info(&printer(&format!("127.0.0.1:{port}"), None)).unwrap_err();
        assert!(err.starts_with("No se pudo conectar con la impresora en http://127.0.0.1:"), "{err}");
    }

    /// Contra una impresora de verdad, solo consultas (no sube ni imprime nada):
    /// `PINOCCHIO_MOONRAKER=192.168.50.250 cargo test -p pinocchio-app --lib impresora_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn impresora_real() {
        let addr = std::env::var("PINOCCHIO_MOONRAKER").expect("PINOCCHIO_MOONRAKER=<ip>");
        let p = Printer::new(&addr, std::env::var("PINOCCHIO_MOONRAKER_KEY").ok(), Duration::from_secs(8)).unwrap();
        let i = info(&p).unwrap();
        println!("{i:?}");
        assert!(!i.klippy_state.is_empty());
        println!("{:?}", status(&p).unwrap());
    }

    /// Envío de verdad SIN imprimir: lamina un cubo, lo sube, revisa lo que Moonraker
    /// entiende del gcode (tiempo, capas, miniaturas) y lo borra.
    /// `PINOCCHIO_MOONRAKER=192.168.50.250 cargo test -p pinocchio-app --lib envio_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn envio_real_sin_imprimir() {
        use pinocchio_slicer as slicer;
        use std::sync::atomic::AtomicBool;
        let addr = std::env::var("PINOCCHIO_MOONRAKER").expect("PINOCCHIO_MOONRAKER=<ip>");
        let engine = slicer::find_engine().expect("falta el motor de laminado");
        let k1 = slicer::index().printers.iter().find(|p| p.name == "Creality K1 (0.4 nozzle)").unwrap();
        let settings = slicer::SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
        let (x, y, s) = (100.0f32, 100.0f32, 20.0f32);
        let p = |a: f32, b: f32, c: f32| [x + a * s, y + b * s, c * s];
        let v = [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        let cube: Vec<[[f32; 3]; 3]> = f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect();
        let work_dir = std::env::temp_dir().join(format!("pinocchio-envio-real-{}", std::process::id()));
        let job = slicer::SliceJob { config: slicer::build_config(&settings).unwrap(), parts: vec![cube], work_dir: work_dir.clone() };
        let summary = slicer::run(&job, &engine, |_| {}, &AtomicBool::new(false)).unwrap();
        let gcode = std::fs::read(&summary.gcode).unwrap();
        println!("laminado: {} capas, {:.0} s, {} bytes", summary.layers, summary.time_s, gcode.len());

        let p = Printer::new(&addr, std::env::var("PINOCCHIO_MOONRAKER_KEY").ok(), Duration::from_secs(60)).unwrap();
        let name = safe_filename("pinocchio-prueba-envio");
        let sent = send_gcode(&p, &name, &gcode, false).unwrap();
        println!("subido: {sent:?}");
        assert!(!sent.started, "no debía empezar a imprimir");
        let st = status(&p).unwrap();
        assert_ne!((st.state.as_str(), st.filename.as_str()), ("printing", name.as_str()), "¡está imprimiendo la prueba!");
        let meta = p.get(&format!("/server/files/metadata?filename={}", urlencode(&sent.filename)));
        // Borrar siempre, aunque falle lo de abajo
        let deleted = p.send(p.request(reqwest::Method::DELETE, &format!("/server/files/gcodes/{}", urlencode(&sent.filename))));
        let _ = std::fs::remove_dir_all(&work_dir);
        let meta = meta.unwrap();
        println!(
            "Moonraker entiende: slicer={:?} tiempo={:?} capas={:?} alto_capa={:?} filamento_g={:?} miniaturas={:?}",
            meta.get("slicer"),
            meta.get("estimated_time"),
            meta.get("layer_count"),
            meta.get("layer_height"),
            meta.get("filament_weight_total"),
            meta.get("thumbnails").and_then(Value::as_array).map(|t| t.iter().map(|x| format!("{}x{}", x["width"], x["height"])).collect::<Vec<_>>())
        );
        deleted.unwrap();
        assert!(meta.get("estimated_time").and_then(Value::as_f64).unwrap_or(0.0) > 0.0, "Moonraker no leyó el tiempo estimado");
        assert!(meta.get("thumbnails").and_then(Value::as_array).is_some_and(|t| !t.is_empty()), "Moonraker no leyó las miniaturas");
    }

    #[test]
    fn estado_imprimiendo_con_estimacion() {
        let (addr, _) = fake(|_, path| {
            if path == "/server/info" {
                (200, json!({"result": {"klippy_state": "ready"}}))
            } else if path.starts_with("/printer/objects/query") {
                (200, json!({"result": {"status": {
                    "print_stats": {"state": "printing", "filename": "caja 1.gcode", "print_duration": 1200.0, "total_duration": 1300.0,
                                    "filament_used": 1520.5, "message": "", "info": {"current_layer": 40, "total_layer": 100}},
                    "virtual_sdcard": {"progress": 0.4},
                    "extruder": {"temperature": 219.6, "target": 220.0},
                    "heater_bed": {"temperature": 59.9, "target": 60.0}
                }}}))
            } else if path == "/server/files/metadata?filename=caja%201.gcode" {
                (200, json!({"result": {"estimated_time": 3000.0}}))
            } else {
                (404, json!({}))
            }
        });
        let s = status(&printer(&addr, None)).unwrap();
        assert_eq!((s.state.as_str(), s.filename.as_str()), ("printing", "caja 1.gcode"));
        assert_eq!((s.layer, s.layers), (Some(40), Some(100)));
        assert!((s.progress - 0.4).abs() < 1e-9);
        assert_eq!(s.remaining, Some(1800.0));
        assert_eq!(s.extruder, [219.6, 220.0]);
        assert_eq!(s.bed, [59.9, 60.0]);
        assert_eq!(s.filament_mm, 1520.5);
    }
}
