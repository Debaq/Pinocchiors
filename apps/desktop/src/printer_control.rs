//! Manejar una impresora en la red más allá de imprimir: ajustes en vivo, mando manual,
//! calibraciones, consola de gcode, historial, archivos guardados y malla de la cama.
//!
//! Los controles son los mismos para todas; cada conexión los traduce: la K1 de fábrica a
//! las órdenes de Creality (`creality.rs`), Klipper/Moonraker a gcode y a su API (aquí).
//! Ver P2 y P7 en `libs/pinocchio/print3d/PLAN_IMPRESORAS.md`.

use crate::moonraker::Printer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Ventiladores de las K1 (y de Klipper, por número de `M106 P`)
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FanKind {
    /// El de la pieza (capa)
    Model,
    /// El lateral grande
    Aux,
    /// El del gabinete
    Case,
}

impl FanKind {
    pub fn index(self) -> u8 {
        match self {
            FanKind::Model => 0,
            FanKind::Aux => 1,
            FanKind::Case => 2,
        }
    }
}

/// Una orden para la impresora.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Control {
    /// Subir (+) o bajar (−) la boquilla en vivo, mm
    ZOffset { delta: f64 },
    NozzleTemp { celsius: f64 },
    BedTemp { celsius: f64 },
    Fan { fan: FanKind, percent: f64 },
    Light { on: bool },
    /// Ejes a llevar al origen: «X», «Y», «Z» separados por espacios
    Home { axes: String },
    /// Mover un eje (relativo), mm
    Jog { axis: String, mm: f64 },
    MotorsOff,
    HeatersOff,
    /// Medir la cama de nuevo y guardar la malla
    AutoLevel,
    PidNozzle { celsius: f64 },
    PidBed { celsius: f64 },
    /// Medir resonancias (input shaper) y guardar
    InputShaper,
    FeedRate { percent: f64 },
    FlowRate { percent: f64 },
    ExcludeObject { name: String },
    /// Gcode cualquiera (una o varias líneas)
    Gcode { script: String },
    /// Imprimir un archivo que ya está en la impresora
    Reprint { name: String },
    DeleteFile { name: String },
    Timelapse { on: bool },
    RestartKlipper,
    RestartFirmware,
    ClearError,
}

impl Control {
    /// Valida lo que llega (de la interfaz o de la IA) antes de mandarlo
    pub fn check(&self) -> Result<(), String> {
        let temp = |c: f64, max: f64, what: &str| {
            if !(0.0..=max).contains(&c) { Err(format!("Temperatura de {what} fuera de 0–{max} °C")) } else { Ok(()) }
        };
        match self {
            Control::ZOffset { delta } if delta.abs() > 1.0 || *delta == 0.0 => Err("El ajuste de altura va de a poco: entre −1 y 1 mm".into()),
            Control::NozzleTemp { celsius } => temp(*celsius, 300.0, "la boquilla"),
            Control::BedTemp { celsius } => temp(*celsius, 110.0, "la cama"),
            Control::PidNozzle { celsius } if !(150.0..=300.0).contains(celsius) => Err("El PID de la boquilla se calibra entre 150 y 300 °C".into()),
            Control::PidBed { celsius } if !(40.0..=110.0).contains(celsius) => Err("El PID de la cama se calibra entre 40 y 110 °C".into()),
            Control::Fan { percent, .. } if !(0.0..=100.0).contains(percent) => Err("El ventilador va de 0 a 100 %".into()),
            Control::FeedRate { percent } | Control::FlowRate { percent } if !(10.0..=300.0).contains(percent) => Err("Entre 10 y 300 %".into()),
            Control::Home { axes } if axes.split_whitespace().any(|a| !["X", "Y", "Z"].contains(&a)) || axes.trim().is_empty() => {
                Err("Ejes: X, Y y/o Z".into())
            }
            Control::Jog { axis, mm } if !["X", "Y", "Z"].contains(&axis.as_str()) || mm.abs() > 100.0 || *mm == 0.0 => Err("Mover: eje X, Y o Z, hasta 100 mm".into()),
            Control::Gcode { script } if script.trim().is_empty() => Err("Falta el gcode".into()),
            Control::Reprint { name } | Control::DeleteFile { name } | Control::ExcludeObject { name } if name.trim().is_empty() || name.contains("..") => {
                Err("Nombre inválido".into())
            }
            _ => Ok(()),
        }
    }

    /// El gcode equivalente (Klipper), si lo hay
    pub fn gcode(&self) -> Option<String> {
        Some(match self {
            Control::ZOffset { delta } => format!("SET_GCODE_OFFSET Z_ADJUST={delta:.3} MOVE=1"),
            Control::NozzleTemp { celsius } => format!("M104 S{celsius:.0}"),
            Control::BedTemp { celsius } => format!("M140 S{celsius:.0}"),
            Control::Fan { fan, percent } => format!("M106 P{} S{:.0}", fan.index(), percent / 100.0 * 255.0),
            Control::Light { on } => format!("SET_PIN PIN=LED VALUE={}", u8::from(*on)),
            Control::Home { axes } => format!("G28 {}", axes.trim()),
            Control::Jog { axis, mm } => format!("G91\nG1 {axis}{mm:.2} F{}\nG90", if axis == "Z" { 600 } else { 3000 }),
            Control::MotorsOff => "M84".into(),
            Control::HeatersOff => "TURN_OFF_HEATERS".into(),
            Control::AutoLevel => "G28\nBED_MESH_CALIBRATE\nSAVE_CONFIG".into(),
            Control::PidNozzle { celsius } => format!("PID_CALIBRATE HEATER=extruder TARGET={celsius:.0}\nSAVE_CONFIG"),
            Control::PidBed { celsius } => format!("PID_CALIBRATE HEATER=heater_bed TARGET={celsius:.0}\nSAVE_CONFIG"),
            Control::InputShaper => "G28\nSHAPER_CALIBRATE\nSAVE_CONFIG".into(),
            Control::FeedRate { percent } => format!("M220 S{percent:.0}"),
            Control::FlowRate { percent } => format!("M221 S{percent:.0}"),
            Control::ExcludeObject { name } => format!("EXCLUDE_OBJECT NAME={name}"),
            Control::Gcode { script } => script.clone(),
            _ => return None,
        })
    }

    /// Mueve, calienta o cambia la configuración guardada (la interfaz pide confirmar las largas)
    pub fn is_calibration(&self) -> bool {
        matches!(self, Control::AutoLevel | Control::PidNozzle { .. } | Control::PidBed { .. } | Control::InputShaper)
    }
}

#[derive(Serialize, Debug, Clone, Default, PartialEq)]
pub struct HistoryEntry {
    pub name: String,
    /// Inicio (segundos desde 1970)
    pub started: f64,
    pub duration_s: f64,
    pub filament_mm: f64,
    pub finished: bool,
    /// Todavía se está imprimiendo
    #[serde(default)]
    pub running: bool,
}

#[derive(Serialize, Debug, Clone, Default, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    /// Segundos desde 1970
    pub modified: f64,
    /// Lo que dice el laminador, si se sabe
    pub print_time_s: Option<f64>,
    pub filament_g: Option<f64>,
}

/// Malla de la cama: alturas por filas (y crece hacia abajo en la lista), del mínimo al máximo
#[derive(Serialize, Debug, Clone, Default, PartialEq)]
pub struct BedMesh {
    pub rows: Vec<Vec<f64>>,
    pub min: [f64; 2],
    pub max: [f64; 2],
}

/// Todo lo que se puede leer de la impresora (lo que no se sabe queda vacío)
#[derive(Serialize, Debug, Clone, Default, PartialEq)]
pub struct PrinterDetails {
    pub z_offset: Option<f64>,
    /// Ventiladores (cuál, %)
    pub fans: Vec<(FanKind, f64)>,
    pub light: Option<bool>,
    pub feed_rate: Option<f64>,
    pub flow_rate: Option<f64>,
    pub timelapse: Option<bool>,
    pub total_jobs: Option<u64>,
    pub total_time_s: Option<f64>,
    pub history: Vec<HistoryEntry>,
    pub files: Vec<FileEntry>,
    pub mesh: Option<BedMesh>,
    pub objects: Vec<String>,
    pub excluded: Vec<String>,
    pub current_object: Option<String>,
    /// Los controles que entiende esta conexión (`kind` de [`Control`])
    pub supports: Vec<&'static str>,
}

pub const ALL_CONTROLS: &[&str] = &[
    "z_offset", "nozzle_temp", "bed_temp", "fan", "light", "home", "jog", "motors_off", "heaters_off", "auto_level", "pid_nozzle", "pid_bed",
    "input_shaper", "feed_rate", "flow_rate", "exclude_object", "gcode", "reprint", "delete_file", "timelapse", "restart_klipper",
    "restart_firmware", "clear_error",
];

// ─── Klipper con Moonraker ──────────────────────────────────────────────────

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub(crate) fn moonraker_control(p: &Printer, c: &Control) -> Result<(), String> {
    c.check()?;
    match c {
        Control::Reprint { name } => p.post(&format!("/printer/print/start?filename={}", encode(name))).map(|_| ()),
        Control::DeleteFile { name } => p.delete(&format!("/server/files/gcodes/{}", encode(name))).map(|_| ()),
        Control::RestartKlipper => p.post("/printer/restart").map(|_| ()),
        Control::RestartFirmware | Control::ClearError => p.post("/printer/firmware_restart").map(|_| ()),
        Control::Timelapse { .. } => Err("Esta impresora no tiene timelapse en Moonraker".into()),
        other => {
            let script = other.gcode().ok_or("Esta impresora no entiende esa orden")?;
            p.post(&format!("/printer/gcode/script?script={}", encode(&script))).map(|_| ())
        }
    }
}

pub(crate) fn moonraker_details(p: &Printer) -> Result<PrinterDetails, String> {
    let q = p.get("/printer/objects/query?gcode_move&fan&bed_mesh&exclude_object")?;
    let s = &q["status"];
    let num = |v: &Value| v.as_f64();
    let mut d = PrinterDetails {
        z_offset: num(&s["gcode_move"]["homing_origin"][2]),
        feed_rate: num(&s["gcode_move"]["speed_factor"]).map(|f| f * 100.0),
        flow_rate: num(&s["gcode_move"]["extrude_factor"]).map(|f| f * 100.0),
        fans: num(&s["fan"]["speed"]).map(|f| vec![(FanKind::Model, f * 100.0)]).unwrap_or_default(),
        supports: ALL_CONTROLS.iter().copied().filter(|c| *c != "timelapse").collect(),
        ..Default::default()
    };
    let m = &s["bed_mesh"];
    if let Some(rows) = m["probed_matrix"].as_array().filter(|r| !r.is_empty() && r[0].as_array().is_some_and(|x| !x.is_empty())) {
        let pair = |v: &Value| [num(&v[0]).unwrap_or(0.0), num(&v[1]).unwrap_or(0.0)];
        d.mesh = Some(BedMesh {
            rows: rows.iter().map(|r| r.as_array().into_iter().flatten().filter_map(num).collect()).collect(),
            min: pair(&m["mesh_min"]),
            max: pair(&m["mesh_max"]),
        });
    }
    let e = &s["exclude_object"];
    d.objects = e["objects"].as_array().into_iter().flatten().filter_map(|o| o["name"].as_str().map(str::to_string)).collect();
    d.excluded = e["excluded_objects"].as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::to_string)).collect();
    d.current_object = e["current_object"].as_str().map(str::to_string);
    if let Ok(h) = p.get("/server/history/list?limit=20&order=desc") {
        d.history = h["jobs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|j| HistoryEntry {
                name: j["filename"].as_str().unwrap_or("").to_string(),
                started: num(&j["start_time"]).unwrap_or(0.0),
                duration_s: num(&j["total_duration"]).unwrap_or(0.0),
                filament_mm: num(&j["filament_used"]).unwrap_or(0.0),
                finished: j["status"] == "completed",
                running: j["status"] == "in_progress",
            })
            .collect();
    }
    if let Ok(t) = p.get("/server/history/totals") {
        d.total_jobs = t["job_totals"]["total_jobs"].as_u64();
        d.total_time_s = num(&t["job_totals"]["total_time"]);
    }
    if let Ok(f) = p.get("/server/files/list?root=gcodes") {
        let mut files: Vec<FileEntry> = f
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| FileEntry {
                name: x["path"].as_str().unwrap_or("").to_string(),
                size: x["size"].as_u64().unwrap_or(0),
                modified: num(&x["modified"]).unwrap_or(0.0),
                print_time_s: None,
                filament_g: None,
            })
            .collect();
        files.sort_by(|a, b| b.modified.total_cmp(&a.modified));
        d.files = files;
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controles_en_json_y_validados() {
        let c: Control = serde_json::from_str(r#"{"kind":"z_offset","delta":-0.05}"#).unwrap();
        assert_eq!(c, Control::ZOffset { delta: -0.05 });
        assert_eq!(c.gcode().unwrap(), "SET_GCODE_OFFSET Z_ADJUST=-0.050 MOVE=1");
        let f: Control = serde_json::from_str(r#"{"kind":"fan","fan":"case","percent":50}"#).unwrap();
        assert_eq!(f.gcode().unwrap(), "M106 P2 S128");
        assert!(Control::NozzleTemp { celsius: 400.0 }.check().is_err());
        assert!(Control::ZOffset { delta: 2.0 }.check().is_err());
        assert!(Control::Home { axes: "X Q".into() }.check().is_err());
        assert!(Control::Home { axes: "X Y".into() }.check().is_ok());
        assert!(Control::DeleteFile { name: "../etc/passwd".into() }.check().is_err());
        assert!(Control::PidBed { celsius: 60.0 }.is_calibration());
        assert_eq!(Control::Jog { axis: "Z".into(), mm: -1.0 }.gcode().unwrap(), "G91\nG1 Z-1.00 F600\nG90");
        assert_eq!(encode("a b/ñ"), "a%20b%2F%C3%B1");
    }

    #[test]
    fn host_y_camara() {
        assert_eq!(host_of("http://192.168.50.49:7125/x"), "192.168.50.49");
        assert_eq!(host_of("k1.local"), "k1.local");
        assert_eq!(host_of("192.168.1.5:80"), "192.168.1.5");
        assert_eq!(host_of("[fe80::1]:7125"), "[fe80::1]");
        // La foto tiene que ser del mismo host que la impresora
        assert!(fetch_snapshot("192.168.50.49", "http://1.2.3.4:8080/?action=snapshot").is_err());
    }

    /// Baja una foto de un servidor falso en el PC (nunca una impresora real)
    #[test]
    fn baja_una_foto() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        // JPEG mínimo válido (magic FFD8 … FFD9), con relleno para pasar el mínimo de 100 bytes
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend(std::iter::repeat_n(0x20u8, 200));
        jpg.extend([0xFF, 0xD9]);
        let body = jpg.clone();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = server.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        let url = format!("http://127.0.0.1:{port}/?action=snapshot");
        let got = fetch_snapshot("127.0.0.1", &url).unwrap();
        assert_eq!(got, jpg);
        // Host distinto: rechazada antes de pedir nada
        assert!(fetch_snapshot("otra.cosa", &url).is_err());
    }
}

// ─── Cámara ─────────────────────────────────────────────────────────────────

use std::net::ToSocketAddrs;
use std::time::Duration;

/// Una cámara de la impresora (stream MJPEG y foto suelta).
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
pub struct Camera {
    pub name: String,
    /// Stream (MJPEG multipart) — por si el webview lo puede mostrar directo
    pub stream_url: String,
    /// Foto JPEG suelta — Pinocchio la pide seguido para mostrar el video
    pub snapshot_url: String,
    /// "mjpeg" (se puede mostrar leyendo fotos) u otro ("webrtc": cifrada/nube, no se puede)
    pub kind: String,
}

/// El host (sin esquema, puerto ni ruta): «http://192.168.1.5:7125/x» → «192.168.1.5».
pub fn host_of(address: &str) -> String {
    let a = address.trim().trim_start_matches("http://").trim_start_matches("https://");
    let a = a.split('/').next().unwrap_or("");
    // IPv6 entre corchetes se deja como está; si no, se corta el puerto
    if let Some(rest) = a.strip_prefix('[') {
        format!("[{}]", rest.split(']').next().unwrap_or(""))
    } else {
        a.split(':').next().unwrap_or("").to_string()
    }
}

/// Si un puerto TCP del host responde (para saber si hay stream local).
fn port_open(host: &str, port: u16) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
        .map(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
        .unwrap_or(false)
}

/// Cámara MJPEG en el puerto 8080 (K1 de fábrica/rooteada, Flashforge), si responde.
pub fn mjpeg_8080(address: &str) -> Vec<Camera> {
    let host = host_of(address);
    if host.is_empty() || !port_open(&host, 8080) {
        return Vec::new();
    }
    vec![Camera {
        name: "Cámara".into(),
        stream_url: format!("http://{host}:8080/?action=stream"),
        snapshot_url: format!("http://{host}:8080/?action=snapshot"),
        kind: "mjpeg".into(),
    }]
}

/// Cámaras configuradas en Moonraker (su API las da; pueden estar en otro puerto o ruta).
pub(crate) fn moonraker_cameras(p: &Printer) -> Result<Vec<Camera>, String> {
    let host = host_of(p.base());
    let resolve = |u: &str| -> String {
        let u = u.trim();
        if u.starts_with("http://") || u.starts_with("https://") {
            u.to_string()
        } else {
            // Relativa: la sirve el nginx de MainsailOS en el puerto 80
            format!("http://{host}/{}", u.trim_start_matches('/'))
        }
    };
    let list = match p.get("/server/webcams/list") {
        Ok(v) => v,
        // Sin el módulo de cámaras: se prueba el 8080 como en las K1
        Err(_) => return Ok(mjpeg_8080(p.base())),
    };
    let mut cams: Vec<Camera> = list["webcams"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| {
            let stream = w["stream_url"].as_str().filter(|s| !s.is_empty())?;
            let service = w["service"].as_str().unwrap_or("");
            // Solo las MJPEG se pueden leer por fotos; webrtc/hls no
            let kind = if service.contains("mjpeg") || service.contains("uv4l") { "mjpeg" } else { "webrtc" };
            Some(Camera {
                name: w["name"].as_str().unwrap_or("Cámara").to_string(),
                stream_url: resolve(stream),
                snapshot_url: resolve(w["snapshot_url"].as_str().filter(|s| !s.is_empty()).unwrap_or(stream)),
                kind: kind.to_string(),
            })
        })
        .collect();
    if cams.is_empty() {
        cams = mjpeg_8080(p.base());
    }
    Ok(cams)
}

/// Baja una foto JPEG de la cámara. `url` tiene que ser del mismo host que la impresora.
pub(crate) fn fetch_snapshot(host: &str, url: &str) -> Result<Vec<u8>, String> {
    if host_of(url) != host {
        return Err("La foto no es de esta impresora".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Error interno: {e}"))?;
    let r = client.get(url).send().map_err(|e| format!("No se pudo leer la cámara: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("La cámara respondió {}", r.status().as_u16()));
    }
    let bytes = r.bytes().map_err(|e| format!("No se pudo leer la foto: {e}"))?.to_vec();
    if bytes.len() < 100 {
        return Err("La cámara no mandó una imagen".into());
    }
    Ok(bytes)
}
