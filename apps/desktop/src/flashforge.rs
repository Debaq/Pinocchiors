//! Impresoras Flashforge (AD5X, Adventurer 5M) con su firmware de fábrica, sin root:
//! - API local HTTP en el puerto 8898 (estado y subir+imprimir eligiendo carrete). Pide el
//!   número de serie y el código del modo LAN (`checkCode`).
//! - Protocolo clásico por TCP en el 8899 (comandos `~M…`, sin clave): número de serie,
//!   y cancelar/pausar/reanudar, que por ahí son inmediatos (desde la pantalla cancelar
//!   tarda muchísimo).
//!
//! Probado contra una AD5X con firmware 3.1.8 (ver `PLAN_IMPRESORAS.md`). Referencia:
//! `src/slic3r/Utils/Flashforge.cpp` de OrcaSlicer 2.4.2.

use crate::moonraker::{PrinterStatus, SendResult};
use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const HTTP_PORT: u16 = 8898;
const TCP_PORT: u16 = 8899;

/// Un carrete del IFS (estación de 4 materiales).
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Slot {
    /// Desde 1, como en la pantalla
    pub id: u32,
    pub has_filament: bool,
    /// «PLA», o vacío si la impresora no lo sabe («?»)
    pub material: String,
    pub color: String,
}

pub struct Flashforge {
    host: String,
    http_port: u16,
    tcp_port: u16,
    /// Se pregunta una vez (por el 8899) si no se dio
    serial: std::sync::Mutex<Option<String>>,
    check_code: Option<String>,
    timeout: Duration,
}

impl Flashforge {
    pub fn new(address: &str, serial: Option<String>, check_code: Option<String>, timeout: Duration) -> Result<Self, String> {
        let host = address.trim().trim_start_matches("http://").trim_start_matches("https://");
        let host = host.split(['/', ':']).next().unwrap_or("").trim().to_string();
        if host.is_empty() {
            return Err("Falta la dirección de la impresora".into());
        }
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Ok(Self { host, http_port: HTTP_PORT, tcp_port: TCP_PORT, serial: std::sync::Mutex::new(clean(serial)), check_code: clean(check_code), timeout })
    }

    #[cfg(test)]
    fn with_ports(mut self, http: u16, tcp: u16) -> Self {
        self.http_port = http;
        self.tcp_port = tcp;
        self
    }

    fn no_connect(&self) -> String {
        format!("No se pudo conectar con la impresora en {}: ¿está encendida y en la misma red?", self.host)
    }

    // ── Protocolo clásico (TCP 8899) ────────────────────────────────────────

    /// Manda comandos `~M…` dentro de una sesión (`~M601 S1` … `~M602`) y devuelve las respuestas.
    fn tcp(&self, commands: &[&str]) -> Result<Vec<String>, String> {
        let addr = (self.host.as_str(), self.tcp_port).to_socket_addrs().map_err(|_| self.no_connect())?.next().ok_or_else(|| self.no_connect())?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|_| self.no_connect())?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream);
        let mut send = |cmd: &str| -> Result<String, String> {
            writer.write_all(format!("{cmd}\r\n").as_bytes()).map_err(|_| self.no_connect())?;
            // La respuesta termina en una línea «ok»
            let mut out = String::new();
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let l = line.trim();
                        out.push_str(l);
                        out.push('\n');
                        if l == "ok" {
                            break;
                        }
                    }
                    Err(_) => return Err(format!("La impresora no respondió a tiempo ({})", self.host)),
                }
            }
            Ok(out)
        };
        send("~M601 S1")?;
        let mut replies = Vec::new();
        for c in commands {
            replies.push(send(c)?);
        }
        let _ = send("~M602");
        Ok(replies)
    }

    /// Número de serie (de `~M115`, sin clave) si no se dio.
    fn serial(&self) -> Result<String, String> {
        if let Some(s) = self.serial.lock().map_err(|e| e.to_string())?.clone() {
            return Ok(s);
        }
        let reply = self.tcp(&["~M115"])?;
        let sn = reply[0]
            .lines()
            .find_map(|l| l.strip_prefix("SN:").map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "La impresora no dio su número de serie".to_string())?;
        *self.serial.lock().map_err(|e| e.to_string())? = Some(sn.clone());
        Ok(sn)
    }

    // ── API local (HTTP 8898) ───────────────────────────────────────────────

    fn check_code(&self) -> Result<&str, String> {
        self.check_code.as_deref().ok_or_else(|| "Falta el código LAN de la impresora (lo muestra su pantalla en la configuración de red, modo LAN)".to_string())
    }

    fn client(&self) -> Result<reqwest::blocking::Client, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(self.timeout)
            .build()
            .map_err(|e| format!("No se pudo preparar la conexión: {e}"))
    }

    /// Revisa `code` (o `err`) de la respuesta: 0 es éxito.
    fn check_reply(&self, status: reqwest::StatusCode, body: &Value, what: &str) -> Result<(), String> {
        let code = body.get("code").or_else(|| body.get("err")).and_then(Value::as_i64);
        if status.is_success() && code == Some(0) {
            return Ok(());
        }
        let msg = body.get("message").or_else(|| body.get("msg")).and_then(Value::as_str).unwrap_or("").to_string();
        if msg.contains("SN is different") || msg.to_lowercase().contains("check") {
            return Err("La impresora rechazó el número de serie o el código LAN".into());
        }
        Err(format!("{what}: {}", if msg.is_empty() { status.to_string() } else { msg }))
    }

    fn detail(&self) -> Result<Value, String> {
        let body = serde_json::json!({ "serialNumber": self.serial()?, "checkCode": self.check_code()? });
        let resp = self
            .client()?
            .post(format!("http://{}:{}/detail", self.host, self.http_port))
            .json(&body)
            .send()
            .map_err(|e| if e.is_timeout() { format!("La impresora no respondió a tiempo ({})", self.host) } else { self.no_connect() })?;
        let status = resp.status();
        let body: Value = resp.json().unwrap_or(Value::Null);
        self.check_reply(status, &body, "La impresora respondió")?;
        Ok(body.get("detail").cloned().unwrap_or(Value::Null))
    }
}

// ── Lo que se muestra ───────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct FlashforgeInfo {
    pub name: String,
    pub serial: String,
    pub firmware: String,
    pub status: String,
    pub slots: Vec<Slot>,
}

fn slots(detail: &Value) -> (Vec<Slot>, Option<u32>) {
    let station = detail.get("matlStationInfo").or_else(|| detail.get("MatlStationInfo"));
    let has_station = detail.get("hasMatlStation").and_then(Value::as_bool).unwrap_or(station.is_some());
    let Some(station) = station.filter(|_| has_station) else { return (Vec::new(), None) };
    let list = station.get("slotInfos").or_else(|| station.get("SlotInfos")).and_then(Value::as_array).cloned().unwrap_or_default();
    let slots = list
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let material = s.get("materialName").and_then(Value::as_str).unwrap_or("").trim();
            Slot {
                id: s.get("slotId").and_then(Value::as_u64).map_or(i as u32 + 1, |v| v as u32),
                has_filament: s.get("hasFilament").and_then(Value::as_bool).unwrap_or(false),
                material: if material == "?" { String::new() } else { material.to_string() },
                color: s.get("materialColor").and_then(Value::as_str).unwrap_or("").to_string(),
            }
        })
        .collect();
    let current = station.get("currentSlot").and_then(Value::as_u64).filter(|&v| v > 0).map(|v| v as u32);
    (slots, current)
}

/// Estado de Flashforge con los mismos nombres que Moonraker (así la UI es una sola).
fn state_name(status: &str) -> &str {
    match status {
        "ready" | "idle" => "standby",
        "printing" | "busy" | "heating" | "pausing" | "canceling" => "printing",
        "paused" => "paused",
        "completed" | "complete" | "finished" => "complete",
        "cancel" | "cancelled" | "canceled" => "cancelled",
        "error" | "failed" => "error",
        other => other,
    }
}

pub fn parse_status(detail: &Value) -> PrinterStatus {
    let f = |k: &str| detail.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let s = |k: &str| detail.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let raw = s("status");
    let progress = f("printProgress").clamp(0.0, 1.0);
    let duration = f("printDuration");
    let printing = matches!(state_name(&raw), "printing" | "paused");
    let layers = detail.get("targetPrintLayer").and_then(Value::as_u64).filter(|&v| v > 0).map(|v| v as u32);
    let (slots, current_slot) = slots(detail);
    PrinterStatus {
        klippy_state: "ready".into(),
        state: state_name(&raw).to_string(),
        message: match raw.as_str() {
            "canceling" => "Cancelando…".into(),
            "pausing" => "Pausando…".into(),
            _ => s("errorCode"),
        },
        filename: s("printFileName"),
        progress,
        print_duration: duration,
        total_duration: duration,
        remaining: (printing && progress > 0.01).then(|| (duration / progress - duration).max(0.0)),
        layer: layers.map(|_| detail.get("printLayer").and_then(Value::as_u64).unwrap_or(0) as u32),
        layers,
        filament_mm: 0.0,
        extruder: [f("rightTemp"), f("rightTargetTemp")],
        bed: [f("platTemp"), f("platTargetTemp")],
        slots,
        current_slot,
    }
}

pub fn info(p: &Flashforge) -> Result<FlashforgeInfo, String> {
    let serial = p.serial()?;
    let detail = p.detail()?;
    let s = |k: &str| detail.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(FlashforgeInfo { name: s("name"), serial, firmware: s("firmwareVersion"), status: state_name(&s("status")).to_string(), slots: slots(&detail).0 })
}

pub fn status(p: &Flashforge) -> Result<PrinterStatus, String> {
    Ok(parse_status(&p.detail()?))
}

/// Cabeceras de `/uploadGcode` (como el diálogo de envío de Orca para Flashforge).
fn upload_headers(serial: &str, check_code: &str, size: usize, start: bool, slot: Option<u32>) -> Vec<(&'static str, String)> {
    // La pieza usa la herramienta T0 (ver `one_filament_per_tool` en el laminador): se asigna al carrete elegido
    let mappings = match slot {
        Some(id) => serde_json::json!([{
            "toolId": 0, "slotId": id, "materialName": "PLA",
            "toolMaterialColor": "#FFFFFF", "slotMaterialColor": "#FFFFFF"
        }]),
        None => serde_json::json!([]),
    };
    let b = |v: bool| if v { "true" } else { "false" }.to_string();
    vec![
        ("serialNumber", serial.to_string()),
        ("checkCode", check_code.to_string()),
        ("fileSize", size.to_string()),
        ("printNow", b(start)),
        ("levelingBeforePrint", b(false)),
        ("flowCalibration", b(false)),
        ("firstLayerInspection", b(false)),
        ("timeLapseVideo", b(false)),
        ("useMatlStation", b(slot.is_some())),
        ("gcodeToolCnt", if slot.is_some() { "1" } else { "0" }.to_string()),
        ("materialMappings", base64::engine::general_purpose::STANDARD.encode(mappings.to_string())),
    ]
}

pub fn send(p: &Flashforge, filename: &str, gcode: &[u8], start: bool, slot: Option<u32>) -> Result<SendResult, String> {
    let serial = p.serial()?;
    let code = p.check_code()?.to_string();
    let boundary = format!("pinocchio{:016x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64));
    let mut body = Vec::with_capacity(gcode.len() + 512);
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"gcodeFile\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(gcode);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut req = p
        .client()?
        .post(format!("http://{}:{}/uploadGcode", p.host, p.http_port))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"));
    for (k, v) in upload_headers(&serial, &code, gcode.len(), start, slot) {
        req = req.header(k, v);
    }
    let resp = req.body(body).send().map_err(|e| if e.is_timeout() { format!("La impresora no respondió a tiempo ({})", p.host) } else { p.no_connect() })?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    p.check_reply(status, &body, "La impresora no aceptó el gcode")?;
    Ok(SendResult { filename: filename.to_string(), started: start })
}

/// Cancelar (`~M26`, inmediato), pausar (`~M25`) o reanudar (`~M24`) por el protocolo clásico.
pub fn action(p: &Flashforge, action: &str) -> Result<(), String> {
    let cmd = match action {
        "cancel" => "~M26",
        "pause" => "~M25",
        "resume" => "~M24",
        other => return Err(format!("Acción desconocida: {other}")),
    };
    let reply = p.tcp(&[cmd])?;
    if reply[0].lines().any(|l| l.trim() == "ok") { Ok(()) } else { Err(format!("La impresora no confirmó: {}", reply[0].trim())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn detail_ad5x(status: &str) -> Value {
        json!({"code": 0, "message": "Success", "detail": {
            "name": "AD5X", "firmwareVersion": "3.1.8-1.1.1", "status": status,
            "printFileName": "cubo.gcode", "printProgress": 0.25, "printDuration": 300,
            "printLayer": 25, "targetPrintLayer": 100,
            "rightTemp": 209.6, "rightTargetTemp": 210.0, "platTemp": 45.1, "platTargetTemp": 45.0,
            "hasMatlStation": true,
            "matlStationInfo": {"currentSlot": 3, "currentLoadSlot": 3, "slotCnt": 4, "slotInfos": [
                {"hasFilament": false, "materialColor": "#FFFFFF", "materialName": "PLA", "slotId": 1},
                {"hasFilament": false, "materialColor": "", "materialName": "?", "slotId": 2},
                {"hasFilament": true, "materialColor": "", "materialName": "?", "slotId": 3},
                {"hasFilament": false, "materialColor": "", "materialName": "?", "slotId": 4}
            ]}
        }})
    }

    #[test]
    fn estado_de_la_ad5x() {
        let s = parse_status(&detail_ad5x("printing")["detail"]);
        assert_eq!((s.state.as_str(), s.filename.as_str()), ("printing", "cubo.gcode"));
        assert_eq!((s.layer, s.layers), (Some(25), Some(100)));
        assert_eq!(s.remaining, Some(900.0));
        assert_eq!(s.extruder, [209.6, 210.0]);
        assert_eq!(s.current_slot, Some(3));
        assert_eq!(s.slots.len(), 4);
        assert!(s.slots[2].has_filament && !s.slots[0].has_filament);
        assert_eq!((s.slots[0].material.as_str(), s.slots[1].material.as_str()), ("PLA", ""));
        assert_eq!(parse_status(&detail_ad5x("canceling")["detail"]).message, "Cancelando…");
        assert_eq!(parse_status(&detail_ad5x("cancel")["detail"]).state, "cancelled");
        assert_eq!(parse_status(&detail_ad5x("ready")["detail"]).remaining, None);
    }

    #[test]
    fn cabeceras_con_carrete() {
        let h = upload_headers("SN1", "abc", 1234, true, Some(3));
        let get = |k: &str| h.iter().find(|(n, _)| *n == k).unwrap().1.clone();
        assert_eq!((get("printNow"), get("useMatlStation"), get("gcodeToolCnt"), get("fileSize")), ("true".into(), "true".into(), "1".into(), "1234".into()));
        let map: Value = serde_json::from_slice(&base64::engine::general_purpose::STANDARD.decode(get("materialMappings")).unwrap()).unwrap();
        assert_eq!((map[0]["toolId"].as_i64(), map[0]["slotId"].as_i64()), (Some(0), Some(3)));
        let h = upload_headers("SN1", "abc", 1, false, None);
        let get = |k: &str| h.iter().find(|(n, _)| *n == k).unwrap().1.clone();
        assert_eq!((get("printNow"), get("useMatlStation"), get("gcodeToolCnt")), ("false".into(), "false".into(), "0".into()));
    }

    /// AD5X falsa: HTTP (detail y uploadGcode) y TCP (~M…). Guarda lo que recibió.
    #[derive(Default)]
    struct Seen {
        http: Vec<(String, Vec<(String, String)>, Vec<u8>)>,
        tcp: Vec<String>,
    }

    fn fake(check: &'static str) -> (u16, u16, Arc<Mutex<Seen>>) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let (hp, tp) = (http.local_addr().unwrap().port(), tcp.local_addr().unwrap().port());
        let s1 = seen.clone();
        std::thread::spawn(move || {
            for stream in http.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let (mut len, mut headers) = (0usize, Vec::new());
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    let (k, v) = h.split_once(':').unwrap_or((h, ""));
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.trim().parse().unwrap();
                    }
                    headers.push((k.to_string(), v.trim().to_string()));
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let ok = String::from_utf8_lossy(&body).contains(check) || headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("checkCode") && v == check);
                let reply = match (path.as_str(), ok) {
                    (_, false) => json!({"code": 1, "message": "SN is different"}),
                    ("/detail", true) => detail_ad5x("ready"),
                    (_, true) => json!({"code": 0, "message": "Success"}),
                };
                s1.lock().unwrap().http.push((path, headers, body));
                let text = reply.to_string();
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for stream in tcp.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut w = stream;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let cmd = line.trim().to_string();
                    let body = if cmd == "~M115" { "Machine Type: Flashforge AD5X\r\nSN: SNFALSO01\r\n".to_string() } else { String::new() };
                    let _ = write!(w, "CMD {} Received.\r\n{body}ok\r\n", cmd.trim_start_matches('~'));
                    s2.lock().unwrap().tcp.push(cmd);
                }
            }
        });
        (hp, tp, seen)
    }

    fn printer(hp: u16, tp: u16, code: Option<&str>) -> Flashforge {
        Flashforge::new("127.0.0.1", None, code.map(str::to_string), Duration::from_secs(5)).unwrap().with_ports(hp, tp)
    }

    #[test]
    fn info_pide_el_numero_de_serie_por_8899() {
        let (hp, tp, seen) = fake("clave");
        let i = info(&printer(hp, tp, Some("clave"))).unwrap();
        assert_eq!((i.name.as_str(), i.serial.as_str(), i.firmware.as_str(), i.status.as_str()), ("AD5X", "SNFALSO01", "3.1.8-1.1.1", "standby"));
        assert_eq!(i.slots.len(), 4);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.tcp, ["~M601 S1", "~M115", "~M602"]);
        assert!(String::from_utf8_lossy(&seen.http[0].2).contains("SNFALSO01"));
    }

    #[test]
    fn sin_codigo_o_con_codigo_malo() {
        let (hp, tp, _) = fake("clave");
        assert!(info(&printer(hp, tp, None)).unwrap_err().starts_with("Falta el código LAN"));
        assert_eq!(info(&printer(hp, tp, Some("otra"))).unwrap_err(), "La impresora rechazó el número de serie o el código LAN");
    }

    #[test]
    fn sube_e_imprime_en_el_carrete_3() {
        let (hp, tp, seen) = fake("clave");
        let r = send(&printer(hp, tp, Some("clave")), "cubo.gcode", b";cubo\nT0\nG28\n", true, Some(3)).unwrap();
        assert_eq!(r, SendResult { filename: "cubo.gcode".into(), started: true });
        let seen = seen.lock().unwrap();
        let (path, headers, body) = &seen.http[0];
        assert_eq!(path, "/uploadGcode");
        let h = |k: &str| headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone()).unwrap();
        assert_eq!((h("printNow"), h("useMatlStation"), h("serialNumber")), ("true".into(), "true".into(), "SNFALSO01".into()));
        let body = String::from_utf8_lossy(body);
        assert!(body.contains("name=\"gcodeFile\"; filename=\"cubo.gcode\"") && body.contains(";cubo\nT0\nG28\n"));
    }

    #[test]
    fn cancelar_es_m26_por_8899() {
        let (hp, tp, seen) = fake("clave");
        action(&printer(hp, tp, None), "cancel").unwrap();
        action(&printer(hp, tp, None), "pause").unwrap();
        assert!(action(&printer(hp, tp, None), "explotar").is_err());
        assert_eq!(seen.lock().unwrap().tcp, ["~M601 S1", "~M26", "~M602", "~M601 S1", "~M25", "~M602"]);
    }

    #[test]
    fn apagada_dice_que_no_conecta() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = action(&printer(port, port, None), "cancel").unwrap_err();
        assert!(err.starts_with("No se pudo conectar con la impresora en 127.0.0.1"), "{err}");
    }
}
