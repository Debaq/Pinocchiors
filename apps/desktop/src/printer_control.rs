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

// ═══════════════════════════════════════════════════════════════════════════
// PUESTA A PUNTO DE LA MÁQUINA (Klipper con Moonraker)
//
// Para una impresora recién armada o recién pasada a Klipper: revisar temperaturas, finales de
// carrera, motores y ventiladores; calibrar el extrusor, la sonda y los tornillos de la cama.
// Ver P7 en `libs/pinocchio/print3d/PLAN_IMPRESORAS.md`. Solo Klipper: la K1 de fábrica no
// devuelve respuestas ni deja leer su configuración.
// ═══════════════════════════════════════════════════════════════════════════

use crate::printers::PrinterConn;

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupHeater {
    /// Nombre en Klipper («extruder», «heater_bed», «heater_generic caja»)
    pub name: String,
    pub temp: f64,
    pub target: f64,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupFan {
    /// Nombre del objeto en Klipper («fan», «fan_generic aux», «heater_fan hotend»)
    pub name: String,
    /// 0–100
    pub percent: f64,
    /// Se puede encender/apagar a mano (los de la boquilla o la electrónica se manejan solos)
    pub manual: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupProbe {
    /// «probe» o «bltouch»
    pub kind: String,
    pub z_offset: Option<f64>,
}

/// Un tornillo de la cama con lo que hay que girarle
#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupScrew {
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// El tornillo de referencia: no se toca
    pub base: bool,
    pub z: Option<f64>,
    /// «CW» (a la derecha) o «CCW» (a la izquierda); vacío en el de referencia
    pub dir: String,
    /// Vuelta y minutos del reloj: «01:15» = una vuelta y cuarto
    pub turn: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupMesh {
    pub rows: Vec<Vec<f64>>,
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub profile: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupInfo {
    /// Esta conexión permite la puesta a punto completa (Klipper con Moonraker)
    pub supported: bool,
    /// Si no, por qué (en palabras simples)
    pub reason: String,
    pub heaters: Vec<SetupHeater>,
    /// Motores que se pueden mover para ver su sentido
    pub steppers: Vec<String>,
    pub fans: Vec<SetupFan>,
    pub has_endstops: bool,
    pub probe: Option<SetupProbe>,
    pub has_screws_tilt: bool,
    pub has_bed_screws: bool,
    /// Último resultado guardado por Klipper de SCREWS_TILT_CALCULATE
    pub screws: Vec<SetupScrew>,
    pub rotation_distance: Option<f64>,
    pub mesh: Option<SetupMesh>,
    /// Ejes con origen hecho («xyz», «», …)
    pub homed: String,
}

/// Qué se puede hacer en la puesta a punto (todo lo demás son los `Control` de siempre)
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupAction {
    QueryEndstops,
    StepperBuzz { stepper: String },
    SetFan { name: String, percent: f64 },
    ProbeAccuracy,
    ProbeCalibrate,
    TestZ { delta: f64 },
    Accept,
    Abort,
    Adjusted,
    SaveConfig,
    ScrewsTilt,
    BedScrewsAdjust,
    /// Calienta, mete 100 mm de filamento a 5 mm/s y apaga la boquilla
    ExtrudeTest { celsius: f64 },
    /// Cambia `rotation_distance` de [extruder] en printer.cfg (con copia de respaldo) y reinicia
    SaveRotationDistance { value: f64 },
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct SetupResult {
    pub ok: bool,
    /// Lo que respondió Klipper y lo que hizo Pinocchio, línea por línea
    pub lines: Vec<String>,
    /// Datos para dibujar (finales de carrera, desvío de la sonda, tornillos…)
    pub data: Value,
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 48 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

impl SetupAction {
    pub fn check(&self) -> Result<(), String> {
        match self {
            SetupAction::StepperBuzz { stepper } if !ident_ok(stepper) => Err("Nombre de motor no válido".into()),
            SetupAction::SetFan { name, .. } if name != "fan" && !ident_ok(name.trim_start_matches("fan_generic ")) => Err("Nombre de ventilador no válido".into()),
            SetupAction::SetFan { percent, .. } if !(0.0..=100.0).contains(percent) => Err("El ventilador va de 0 a 100 %".into()),
            SetupAction::TestZ { delta } if delta.abs() > 1.0 || *delta == 0.0 => Err("El ajuste de la sonda va de a poco: entre −1 y 1 mm".into()),
            SetupAction::ExtrudeTest { celsius } if !(170.0..=300.0).contains(celsius) => Err("Para extruir, la boquilla tiene que estar entre 170 y 300 °C".into()),
            SetupAction::SaveRotationDistance { value } if !(1.0..=200.0).contains(value) || !value.is_finite() => Err("El valor de rotation_distance no parece válido (entre 1 y 200)".into()),
            _ => Ok(()),
        }
    }

    /// El gcode de la acción (la que edita el archivo de configuración no tiene)
    pub fn gcode(&self) -> Option<String> {
        Some(match self {
            SetupAction::QueryEndstops => "QUERY_ENDSTOPS".into(),
            SetupAction::StepperBuzz { stepper } => format!("STEPPER_BUZZ STEPPER={stepper}"),
            SetupAction::SetFan { name, percent } => match name.strip_prefix("fan_generic ") {
                Some(n) => format!("SET_FAN_SPEED FAN={n} SPEED={:.2}", percent / 100.0),
                None => format!("M106 S{:.0}", percent / 100.0 * 255.0),
            },
            SetupAction::ProbeAccuracy => "G28\nPROBE_ACCURACY".into(),
            SetupAction::ProbeCalibrate => "G28\nPROBE_CALIBRATE".into(),
            SetupAction::TestZ { delta } => format!("TESTZ Z={delta:+.3}"),
            SetupAction::Accept => "ACCEPT".into(),
            SetupAction::Abort => "ABORT".into(),
            SetupAction::Adjusted => "ADJUSTED".into(),
            SetupAction::SaveConfig => "SAVE_CONFIG".into(),
            SetupAction::ScrewsTilt => "G28\nSCREWS_TILT_CALCULATE".into(),
            SetupAction::BedScrewsAdjust => "G28\nBED_SCREWS_ADJUST".into(),
            // Klipper no deja extruir más de 50 mm de una vez (`max_extrude_only_distance`): dos de 50
            SetupAction::ExtrudeTest { celsius } => format!("M109 S{celsius:.0}\nM83\nG1 E50 F300\nG1 E50 F300\nM82\nM104 S0"),
            SetupAction::SaveRotationDistance { .. } => return None,
        })
    }
}

fn num_of(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Arma lo que se sabe de la máquina con la lista de objetos de Klipper y su estado.
pub(crate) fn parse_setup(objects: &[String], status: &Value) -> SetupInfo {
    let has = |n: &str| objects.iter().any(|o| o == n);
    let st = |n: &str| &status[n];
    let mut info = SetupInfo { supported: true, ..Default::default() };
    for o in objects {
        let is_heater = o == "extruder" || o == "heater_bed" || o.starts_with("heater_generic ") || (o.starts_with("extruder") && o[8..].chars().all(|c| c.is_ascii_digit()));
        if is_heater {
            info.heaters.push(SetupHeater { name: o.clone(), temp: num_of(&st(o)["temperature"]).unwrap_or(0.0), target: num_of(&st(o)["target"]).unwrap_or(0.0) });
        }
        let manual = o == "fan" || o.starts_with("fan_generic ");
        if manual || o.starts_with("heater_fan ") || o.starts_with("controller_fan ") || o.starts_with("temperature_fan ") {
            info.fans.push(SetupFan { name: o.clone(), percent: num_of(&st(o)["speed"]).unwrap_or(0.0) * 100.0, manual });
        }
    }
    if let Some(m) = st("stepper_enable")["steppers"].as_object() {
        info.steppers = m.keys().cloned().collect();
        info.steppers.sort();
    }
    info.has_endstops = has("query_endstops");
    let settings = &st("configfile")["settings"];
    for kind in ["probe", "bltouch"] {
        if has(kind) || !settings[kind].is_null() {
            info.probe = Some(SetupProbe { kind: kind.into(), z_offset: num_of(&settings[kind]["z_offset"]) });
            break;
        }
    }
    info.has_screws_tilt = has("screws_tilt_adjust") || !settings["screws_tilt_adjust"].is_null();
    info.has_bed_screws = has("bed_screws") || !settings["bed_screws"].is_null();
    info.rotation_distance = num_of(&settings["extruder"]["rotation_distance"]);
    info.homed = st("toolhead")["homed_axes"].as_str().unwrap_or("").to_string();
    if info.has_screws_tilt {
        info.screws = screws_from_status(&st("screws_tilt_adjust")["results"], &settings["screws_tilt_adjust"]);
    }
    let m = st("bed_mesh");
    let matrix = ["probed_matrix", "mesh_matrix"]
        .iter()
        .filter_map(|k| m[*k].as_array())
        .find(|r| !r.is_empty() && r[0].as_array().is_some_and(|x| !x.is_empty()));
    if let Some(rows) = matrix {
        let pair = |v: &Value| [num_of(&v[0]).unwrap_or(0.0), num_of(&v[1]).unwrap_or(0.0)];
        info.mesh = Some(SetupMesh {
            rows: rows.iter().map(|r| r.as_array().into_iter().flatten().filter_map(num_of).collect()).collect(),
            min: pair(&m["mesh_min"]),
            max: pair(&m["mesh_max"]),
            profile: m["profile_name"].as_str().unwrap_or("").to_string(),
        });
    }
    info
}

/// `screws_tilt_adjust.results` de Klipper + los nombres y posiciones de `printer.cfg`.
fn screws_from_status(results: &Value, cfg: &Value) -> Vec<SetupScrew> {
    let Some(r) = results.as_object() else { return Vec::new() };
    let mut keys: Vec<&String> = r.keys().collect();
    keys.sort();
    keys.into_iter()
        .map(|k| {
            let v = &r[k];
            let xy = cfg[k.as_str()].as_str().map(|s| s.split(',').filter_map(|p| p.trim().parse::<f64>().ok()).collect::<Vec<_>>()).unwrap_or_default();
            SetupScrew {
                name: cfg[format!("{k}_name")].as_str().map(str::to_string).unwrap_or_else(|| k.clone()),
                x: xy.first().copied().unwrap_or(0.0),
                y: xy.get(1).copied().unwrap_or(0.0),
                base: v["is_base"].as_bool().unwrap_or(false),
                z: num_of(&v["z"]),
                dir: v["sign"].as_str().unwrap_or("").to_string(),
                turn: v["adjust"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect()
}

/// Lee una línea de SCREWS_TILT_CALCULATE de Klipper viejo:
/// `front_right : x=155.0, y=30.0, z=2.36000 : adjust CW 01:15` (el de referencia dice `(base)`).
pub(crate) fn parse_screw_line(line: &str) -> Option<SetupScrew> {
    let line = line.trim().trim_start_matches("//").trim();
    let mut parts = line.split(" : ");
    let head = parts.next()?.trim();
    let coords = parts.next()?;
    let base = head.contains("(base)");
    let name = head.replace("(base)", "").trim().to_string();
    let get = |key: &str| -> Option<f64> {
        let i = coords.find(&format!("{key}="))? + key.len() + 1;
        let end = coords[i..].find(|c: char| c == ',' || c.is_whitespace()).map_or(coords.len(), |e| i + e);
        coords[i..end].parse().ok()
    };
    let (x, y) = (get("x")?, get("y")?);
    let (mut dir, mut turn) = (String::new(), String::new());
    if let Some(adj) = parts.next().and_then(|a| a.trim().strip_prefix("adjust ")) {
        let mut w = adj.split_whitespace();
        dir = w.next().unwrap_or("").to_string();
        turn = w.next().unwrap_or("").to_string();
    }
    Some(SetupScrew { name, x, y, base, z: get("z"), dir, turn })
}

/// Lee el resultado de PROBE_ACCURACY:
/// `probe accuracy results: maximum 1.0, minimum 0.99, range 0.01, average 1.0, median 1.0, standard deviation 0.003`
pub(crate) fn parse_probe_accuracy(lines: &[String]) -> Option<Value> {
    let line = lines.iter().find(|l| l.contains("probe accuracy results"))?;
    let rest = line.split_once("results:")?.1;
    let mut out = serde_json::Map::new();
    for (label, key) in [("maximum", "max"), ("minimum", "min"), ("range", "range"), ("average", "average"), ("median", "median"), ("standard deviation", "stddev")] {
        if let Some(i) = rest.find(label) {
            let v: String = rest[i + label.len()..].trim_start().chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-')).collect();
            if let Ok(n) = v.parse::<f64>() {
                out.insert(key.into(), Value::from(n));
            }
        }
    }
    (!out.is_empty()).then(|| Value::Object(out))
}

/// Cambia solo `rotation_distance` dentro de `[extruder]` de un `printer.cfg`.
/// Devuelve el texto nuevo y el valor que había. Si el valor no está en este archivo (viene
/// de un `[include]` o del bloque `#*#` que Klipper guarda solo), no toca nada y dice por qué.
pub(crate) fn set_rotation_distance(cfg: &str, value: f64) -> Result<(String, String), String> {
    let new_value = format!("{value:.6}").trim_end_matches('0').trim_end_matches('.').to_string();
    let mut out = String::with_capacity(cfg.len() + 16);
    let mut section = String::new();
    let mut in_save_block = false;
    let (mut done, mut old, mut has_include, mut has_extruder) = (false, String::new(), false, false);
    let mut saved_override = false;
    let mut save_section = String::new();
    for line in cfg.split_inclusive('\n') {
        if let Some(auto) = line.strip_prefix("#*#") {
            in_save_block = true;
            let a = auto.trim();
            if a.starts_with('[') {
                save_section = a.trim_matches(|c| c == '[' || c == ']').trim().to_lowercase();
            } else if save_section == "extruder" && a.split([':', '=']).next().is_some_and(|k| k.trim() == "rotation_distance") {
                saved_override = true;
            }
            out.push_str(line);
            continue;
        }
        let first = line.chars().next().unwrap_or('\n');
        if !in_save_block && first == '[' {
            if let Some(end) = line.find(']') {
                section = line[1..end].trim().to_lowercase();
                has_include |= section.starts_with("include ");
                has_extruder |= section == "extruder";
            }
        } else if !in_save_block && section == "extruder" && !done && !first.is_whitespace() && first != '#' && first != ';' {
            if let Some(i) = line.find([':', '=']) {
                if line[..i].trim().eq_ignore_ascii_case("rotation_distance") {
                    let rest = &line[i + 1..];
                    let lead: String = rest.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                    let body = &rest[lead.len()..];
                    // Comentario al final: empieza con un espacio y «#» o «;»
                    let cut = body.find([' ', '\t']).and_then(|_| body.find(" #").or_else(|| body.find(" ;")).or_else(|| body.find("\t#")));
                    let (val, comment) = body.split_at(cut.unwrap_or(body.trim_end().len()));
                    old = val.trim().to_string();
                    let eol = if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" };
                    let comment = comment.trim_end_matches(['\r', '\n']);
                    out.push_str(&format!("{}{}{}{}{}", &line[..=i], lead, new_value, comment, eol));
                    done = true;
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    if saved_override {
        return Err("Klipper lo guardó solo al final del archivo (bloque «#*#»); cámbialo ahí o bórralo de ese bloque".into());
    }
    if done {
        return Ok((out, old));
    }
    if has_include {
        return Err("el valor no está en printer.cfg: parece estar en otro archivo que se incluye con [include]".into());
    }
    if has_extruder {
        Err("la sección [extruder] de printer.cfg no tiene rotation_distance".into())
    } else {
        Err("printer.cfg no tiene una sección [extruder]".into())
    }
}

/// «20261111-134501» (hora universal) desde segundos desde 1970, para el nombre de la copia.
pub(crate) fn stamp(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // Fecha civil desde días (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn moonraker_only(conn: &PrinterConn, timeout: Duration) -> Result<Printer, String> {
    if conn.kind != "moonraker" {
        return Err("Esta impresora no es Klipper con Moonraker: la puesta a punto no está disponible".into());
    }
    Printer::new(&conn.address, conn.api_key.clone(), timeout)
}

fn setup_info(p: &Printer) -> Result<SetupInfo, String> {
    let list = p.get("/printer/objects/list")?;
    let objects: Vec<String> = list["objects"].as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::to_string)).collect();
    let wanted = |o: &&String| {
        matches!(o.as_str(), "configfile" | "toolhead" | "bed_mesh" | "stepper_enable" | "screws_tilt_adjust" | "probe" | "bltouch" | "fan" | "extruder" | "heater_bed")
            || o.starts_with("heater_generic ")
            || o.starts_with("extruder")
            || o.contains("fan")
    };
    let query = objects.iter().filter(wanted).map(|o| encode(o)).collect::<Vec<_>>().join("&");
    let status = p.get(&format!("/printer/objects/query?{query}"))?;
    Ok(parse_setup(&objects, &status["status"]))
}

/// Las respuestas de Klipper (gcode_store) que llegaron después de `since`.
fn responses_since(p: &Printer, since: f64) -> Vec<String> {
    let Ok(r) = p.get("/server/gcode_store?count=80") else { return Vec::new() };
    r["gcode_store"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["type"] == "response" && num_of(&e["time"]).unwrap_or(0.0) > since)
        .filter_map(|e| e["message"].as_str())
        .flat_map(|m| m.lines())
        .map(|l| l.trim().trim_start_matches("// ").to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn last_store_time(p: &Printer) -> f64 {
    p.get("/server/gcode_store?count=1").ok().and_then(|r| r["gcode_store"].as_array().and_then(|a| a.last().and_then(|e| num_of(&e["time"])))).unwrap_or(0.0)
}

fn setup_run(p: &Printer, action: &SetupAction) -> Result<SetupResult, String> {
    let script = action.gcode().ok_or("Esta acción no tiene gcode")?;
    let since = last_store_time(p);
    p.post(&format!("/printer/gcode/script?script={}", encode(&script)))?;
    let lines = responses_since(p, since);
    let mut res = SetupResult { ok: !lines.iter().any(|l| l.starts_with("!!")), lines, data: Value::Null };
    match action {
        SetupAction::QueryEndstops => {
            let q = p.get("/printer/objects/query?query_endstops")?;
            res.data = q["status"]["query_endstops"]["last_query"].clone();
        }
        SetupAction::ProbeAccuracy => {
            res.data = parse_probe_accuracy(&res.lines).unwrap_or(Value::Null);
            if res.data.is_null() {
                res.ok = false;
                res.lines.push("No llegó el resultado de la prueba de precisión.".into());
            }
        }
        SetupAction::ScrewsTilt | SetupAction::BedScrewsAdjust => {
            let mut screws: Vec<SetupScrew> = res.lines.iter().filter_map(|l| parse_screw_line(l)).collect();
            if screws.is_empty() {
                // Klipper nuevo: el resultado queda en el estado
                let q = p.get("/printer/objects/query?screws_tilt_adjust&configfile")?;
                screws = screws_from_status(&q["status"]["screws_tilt_adjust"]["results"], &q["status"]["configfile"]["settings"]["screws_tilt_adjust"]);
            }
            res.data = serde_json::to_value(&screws).unwrap_or(Value::Null);
        }
        _ => {}
    }
    Ok(res)
}

fn save_rotation_distance(p: &Printer, value: f64) -> Result<SetupResult, String> {
    let by_hand = |why: String| SetupResult {
        ok: false,
        lines: vec![format!("No se cambió el archivo: {why}."), format!("Ponlo a mano en [extruder]:  rotation_distance: {value:.4}")],
        data: serde_json::json!({ "value": value }),
    };
    let cfg = p.get_text("/server/files/config/printer.cfg")?;
    let (new, old) = match set_rotation_distance(&cfg, value) {
        Ok(x) => x,
        Err(why) => return Ok(by_hand(why)),
    };
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let backup = format!("printer.cfg.pinocchio-{}.bak", stamp(secs));
    p.post(&format!("/server/files/copy?source={}&dest={}", encode("config/printer.cfg"), encode(&format!("config/{backup}"))))
        .map_err(|e| format!("No se pudo hacer la copia de respaldo, así que no se cambió nada. {e}"))?;
    p.upload("config", "printer.cfg", new.as_bytes())?;
    p.post("/printer/firmware_restart")?;
    Ok(SetupResult {
        ok: true,
        lines: vec![format!("Copia de respaldo: {backup}"), format!("rotation_distance: {old} → {value:.4}"), "Klipper se está reiniciando con el valor nuevo.".into()],
        data: serde_json::json!({ "backup": backup, "old": old, "value": value }),
    })
}

/// Lo que se puede revisar y calibrar de la máquina; solo Klipper con Moonraker.
#[tauri::command]
pub async fn printer_setup_info(conn: PrinterConn) -> Result<SetupInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if conn.kind != "moonraker" {
            let reason = if conn.kind == "creality" {
                "Esta K1 de fábrica no deja leer su configuración ni devuelve las respuestas de Klipper; solo se pueden hacer el nivelado, los PID y las resonancias."
            } else {
                "Esta impresora no usa Klipper con Moonraker, así que solo se pueden hacer los ajustes de arriba."
            };
            return Ok(SetupInfo { supported: false, reason: reason.into(), ..Default::default() });
        }
        setup_info(&moonraker_only(&conn, Duration::from_secs(10))?)
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))?
}

/// Una acción de la puesta a punto. Mueve, calienta o cambia la configuración: la interfaz
/// (o la IA) la confirma antes, y aquí se rechaza si se está imprimiendo.
#[tauri::command]
pub async fn printer_setup_action(conn: PrinterConn, action: SetupAction) -> Result<SetupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        action.check()?;
        let p = moonraker_only(&conn, Duration::from_secs(600))?;
        let state = p.get("/printer/objects/query?print_stats").ok().and_then(|q| q["status"]["print_stats"]["state"].as_str().map(str::to_string)).unwrap_or_default();
        if state == "printing" || state == "paused" {
            return Err("No se puede hacer esto mientras se imprime".into());
        }
        match action {
            SetupAction::SaveRotationDistance { value } => save_rotation_distance(&p, value),
            _ => setup_run(&p, &action),
        }
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))?
}

#[cfg(test)]
mod setup_tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    const CFG: &str = "[include mainsail.cfg]\n[stepper_x]\nrotation_distance: 40\n\n[extruder]\nstep_pin: PB4\nrotation_distance: 22.6789 # medido\nnozzle_diameter: 0.4\n\n[extruder_stepper x]\nrotation_distance: 5\n\n#*# <---------------------- SAVE_CONFIG ---------------------->\n#*# DO NOT EDIT THIS BLOCK OR BELOW.\n#*#\n#*# [heater_bed]\n#*# control = pid\n";

    #[test]
    fn edita_solo_el_extrusor() {
        let (new, old) = set_rotation_distance(CFG, 23.5).unwrap();
        assert_eq!(old, "22.6789");
        assert!(new.contains("rotation_distance: 23.5 # medido\n"), "{new}");
        // Los otros rotation_distance no cambian
        assert!(new.contains("[stepper_x]\nrotation_distance: 40\n") && new.contains("[extruder_stepper x]\nrotation_distance: 5\n"));
        assert_eq!(new.lines().count(), CFG.lines().count());
        assert!(new.ends_with("#*# control = pid\n"));
    }

    #[test]
    fn con_igual_sin_comentario_y_crlf() {
        let (new, old) = set_rotation_distance("[extruder]\r\nrotation_distance = 33.333\r\nstep_pin: x\r\n", 31.25).unwrap();
        assert_eq!(old, "33.333");
        assert_eq!(new, "[extruder]\r\nrotation_distance = 31.25\r\nstep_pin: x\r\n");
    }

    #[test]
    fn comentarios_no_se_tocan() {
        let cfg = "[extruder]\n# rotation_distance: 1\n; rotation_distance: 2\nrotation_distance: 22\n";
        let (new, _) = set_rotation_distance(cfg, 21.0).unwrap();
        assert_eq!(new, "[extruder]\n# rotation_distance: 1\n; rotation_distance: 2\nrotation_distance: 21\n");
    }

    #[test]
    fn en_otro_archivo_o_en_el_bloque_guardado_no_toca() {
        // Con [include] y sin el valor en [extruder]
        let e = set_rotation_distance("[include extruder.cfg]\n[extruder]\nstep_pin: PB4\n", 20.0).unwrap_err();
        assert!(e.contains("[include]"), "{e}");
        // Sin sección
        assert!(set_rotation_distance("[stepper_x]\nrotation_distance: 40\n", 20.0).unwrap_err().contains("sección [extruder]"));
        // En el bloque SAVE_CONFIG
        let cfg = "[extruder]\nrotation_distance: 22\n#*# <--- SAVE_CONFIG --->\n#*# [extruder]\n#*# rotation_distance = 22.5\n";
        assert!(set_rotation_distance(cfg, 20.0).unwrap_err().contains("#*#"));
        // [extruder] sin la clave
        assert!(set_rotation_distance("[extruder]\nstep_pin: PB4\n", 20.0).unwrap_err().contains("no tiene rotation_distance"));
    }

    #[test]
    fn fecha_de_la_copia() {
        assert_eq!(stamp(1_700_000_000), "20231114-221320");
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn lee_tornillos_y_precision() {
        let s = parse_screw_line("// front_right : x=155.0, y=30.0, z=2.36000 : adjust CW 01:15").unwrap();
        assert_eq!((s.name.as_str(), s.dir.as_str(), s.turn.as_str(), s.base), ("front_right", "CW", "01:15", false));
        assert_eq!((s.x, s.y, s.z), (155.0, 30.0, Some(2.36)));
        let b = parse_screw_line("front_left (base) : x=-5.0, y=30.0, z=2.48750").unwrap();
        assert!(b.base && b.dir.is_empty() && b.name == "front_left" && b.x == -5.0);
        assert!(parse_screw_line("Send ACCEPT or ABORT").is_none());
        let l = vec!["probe accuracy at x:10 y:10 z:5 (samples=10)".to_string(), "probe accuracy results: maximum 1.0125, minimum 1.0, range 0.0125, average 1.005, median 1.0, standard deviation 0.0043".to_string()];
        let a = parse_probe_accuracy(&l).unwrap();
        assert_eq!((a["range"].as_f64(), a["stddev"].as_f64(), a["max"].as_f64()), (Some(0.0125), Some(0.0043), Some(1.0125)));
    }

    #[test]
    fn lee_la_maquina() {
        let objects: Vec<String> = ["configfile", "toolhead", "extruder", "heater_bed", "fan", "heater_fan hotend", "fan_generic caja", "bltouch", "screws_tilt_adjust", "query_endstops", "bed_mesh", "stepper_enable"].iter().map(|s| s.to_string()).collect();
        let st = json!({
            "extruder": {"temperature": 24.5, "target": 0.0}, "heater_bed": {"temperature": 23.0, "target": 0.0},
            "fan": {"speed": 0.5}, "heater_fan hotend": {"speed": 1.0}, "fan_generic caja": {"speed": 0.0},
            "stepper_enable": {"steppers": {"stepper_y": false, "stepper_x": false, "extruder": false}},
            "toolhead": {"homed_axes": "xy"},
            "configfile": {"settings": {"extruder": {"rotation_distance": 22.68}, "bltouch": {"z_offset": 2.45},
                "screws_tilt_adjust": {"screw1": "30, 30", "screw1_name": "frente izq", "screw2": "230, 30"}}},
            "screws_tilt_adjust": {"results": {"screw1": {"z": 2.5, "sign": "CW", "adjust": "00:00", "is_base": true}, "screw2": {"z": 2.4, "sign": "CCW", "adjust": "00:15", "is_base": false}}},
            "bed_mesh": {"profile_name": "default", "probed_matrix": [], "mesh_matrix": [[0.1, 0.2], [0.0, -0.1]], "mesh_min": [10, 10], "mesh_max": [200, 200]}
        });
        let i = parse_setup(&objects, &st);
        assert_eq!(i.heaters.len(), 2);
        assert_eq!(i.steppers, vec!["extruder", "stepper_x", "stepper_y"]);
        assert_eq!(i.fans.iter().map(|f| (f.name.as_str(), f.manual)).collect::<Vec<_>>(), vec![("fan", true), ("heater_fan hotend", false), ("fan_generic caja", true)]);
        assert_eq!(i.probe, Some(SetupProbe { kind: "bltouch".into(), z_offset: Some(2.45) }));
        assert_eq!(i.rotation_distance, Some(22.68));
        assert!(i.has_endstops && i.has_screws_tilt && !i.has_bed_screws);
        assert_eq!(i.screws[0].name, "frente izq");
        assert_eq!(i.screws[1].dir, "CCW");
        assert_eq!(i.mesh.unwrap().rows, vec![vec![0.1, 0.2], vec![0.0, -0.1]]);
        assert_eq!(i.homed, "xy");
        // Sin sonda ni sección de tornillos
        let i = parse_setup(&["extruder".to_string()], &json!({}));
        assert!(i.probe.is_none() && !i.has_screws_tilt && i.mesh.is_none());
    }

    #[test]
    fn valida_acciones() {
        let a: SetupAction = serde_json::from_str(r#"{"kind":"stepper_buzz","stepper":"stepper_x"}"#).unwrap();
        assert_eq!(a.gcode().unwrap(), "STEPPER_BUZZ STEPPER=stepper_x");
        assert!(SetupAction::StepperBuzz { stepper: "x\nM112".into() }.check().is_err());
        assert!(SetupAction::TestZ { delta: 0.025 }.check().is_ok());
        assert_eq!(SetupAction::TestZ { delta: -0.1 }.gcode().unwrap(), "TESTZ Z=-0.100");
        assert!(SetupAction::ExtrudeTest { celsius: 50.0 }.check().is_err());
        assert!(SetupAction::SaveRotationDistance { value: 0.0 }.check().is_err());
        assert_eq!(SetupAction::SetFan { name: "fan_generic caja".into(), percent: 50.0 }.gcode().unwrap(), "SET_FAN_SPEED FAN=caja SPEED=0.50");
        assert_eq!(SetupAction::SetFan { name: "fan".into(), percent: 0.0 }.gcode().unwrap(), "M106 S0");
    }

    /// Moonraker falso en el PC: guarda (método, ruta, cuerpo)
    fn fake(handler: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (String, Arc<Mutex<Vec<(String, String, String)>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.is_empty() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let mut len = 0usize;
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = h.split_once(':') {
                        if k.eq_ignore_ascii_case("content-length") {
                            len = v.trim().parse().unwrap_or(0);
                        }
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let (code, text) = handler(&method, &path);
                seen2.lock().unwrap().push((method, path, String::from_utf8_lossy(&body).to_string()));
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (addr, seen)
    }

    #[test]
    fn guarda_el_extrusor_con_copia_y_reinicio() {
        let (addr, seen) = fake(|m, path| match (m, path) {
            ("GET", "/server/files/config/printer.cfg") => (200, CFG.to_string()),
            _ => (200, json!({"result": "ok"}).to_string()),
        });
        let p = Printer::new(&addr, None, Duration::from_secs(5)).unwrap();
        let r = save_rotation_distance(&p, 23.5).unwrap();
        assert!(r.ok, "{r:?}");
        let seen = seen.lock().unwrap();
        let paths: Vec<&str> = seen.iter().map(|s| s.1.as_str()).collect();
        assert!(paths[1].starts_with("/server/files/copy?source=config%2Fprinter.cfg&dest=config%2Fprinter.cfg.pinocchio-") && paths[1].ends_with(".bak"), "{paths:?}");
        assert_eq!(paths[2], "/server/files/upload");
        assert!(seen[2].2.contains("name=\"root\"\r\n\r\nconfig") && seen[2].2.contains("rotation_distance: 23.5 # medido"));
        assert_eq!(paths[3], "/printer/firmware_restart");
    }

    #[test]
    fn si_esta_en_un_include_no_sube_nada() {
        let (addr, seen) = fake(|_, _| (200, "[include x.cfg]\n[extruder]\nstep_pin: PB4\n".to_string()));
        let p = Printer::new(&addr, None, Duration::from_secs(5)).unwrap();
        let r = save_rotation_distance(&p, 23.5).unwrap();
        assert!(!r.ok && r.lines[1].contains("rotation_distance: 23.5000"), "{r:?}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn precision_de_la_sonda_lee_la_consola() {
        let (addr, _) = fake(|m, path| match (m, path) {
            ("GET", "/server/gcode_store?count=1") => (200, json!({"result": {"gcode_store": [{"message": "G28", "time": 100.0, "type": "command"}]}}).to_string()),
            ("GET", p) if p.starts_with("/server/gcode_store") => (
                200,
                json!({"result": {"gcode_store": [
                    {"message": "old", "time": 90.0, "type": "response"},
                    {"message": "// probe accuracy results: maximum 1.0, minimum 0.99, range 0.01, average 1.0, median 1.0, standard deviation 0.003", "time": 120.0, "type": "response"}
                ]}})
                .to_string(),
            ),
            _ => (200, json!({"result": "ok"}).to_string()),
        });
        let p = Printer::new(&addr, None, Duration::from_secs(5)).unwrap();
        let r = setup_run(&p, &SetupAction::ProbeAccuracy).unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.lines.len(), 1);
        assert_eq!(r.data["range"], json!(0.01));
    }
}
