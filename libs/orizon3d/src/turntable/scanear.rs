//! Rig del equipo scanEar (<https://github.com/Debaq/scanEar>): Arduino por
//! USB serie a 115200 baudios, comandos de texto terminados en `\n`.
//!
//! - `PY:MOVE_Y:<grados>`: plato, 0 a 360
//! - `PY:MOVE_Z:<grados>`: arco de la cámara, 0 a 135
//! - `PY:GET_STATUS` → `STATUS:{"state":0,"y_angle":…,"z_angle":…,…}`, con
//!   `state` 0 = quieto, 1 = escaneando, 2 = en pausa, 3 = calibrando
//! - `PY:RESET`: vuelve al origen
//! - Respuestas `OK:{…}` y `ERROR:{"message":…}`
//!
//! El escaneo lo maneja el programa, no el Arduino. No se usa
//! `PY:START_SCAN`, porque ahí el Arduino avisa "fotografiar" sin esperar a la
//! cámara. Cada movimiento se da por terminado cuando `GET_STATUS` informa
//! quieto en el ángulo pedido. Después se espera a que se asiente la
//! vibración.
//!
//! El firmware actual no controla láseres ni luz. [`ScanEarRig`] envía
//! `PY:LASER:<n>:<0|1>` y `PY:LIGHT:<0-255>`, comandos propuestos para
//! agregar al firmware. Mientras no existan, el Arduino responde `ERROR`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::scan::Rig;

/// Lo que informa `PY:GET_STATUS`
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub state: u8,
    #[serde(default)]
    pub y_angle: f64,
    #[serde(default)]
    pub z_angle: f64,
}

/// Una línea recibida del Arduino
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok,
    Error(String),
    Status(Status),
    /// Mensajes sueltos del firmware ("fotografiar", depuración…)
    Other(String),
}

pub fn parse_reply(line: &str) -> Reply {
    let line = line.trim();
    if let Some(json) = line.strip_prefix("STATUS:") {
        return match serde_json::from_str(json) {
            Ok(s) => Reply::Status(s),
            Err(_) => Reply::Other(line.to_string()),
        };
    }
    if let Some(json) = line.strip_prefix("ERROR:") {
        let message = serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or_else(|| json.to_string());
        return Reply::Error(message);
    }
    if line.starts_with("OK:") || line == "OK" {
        return Reply::Ok;
    }
    Reply::Other(line.to_string())
}

/// Cómo esperar los movimientos
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Máximo para terminar un movimiento
    pub move_timeout: Duration,
    /// Pausa entre consultas de estado
    pub poll: Duration,
    /// Espera tras llegar, para que se calme la vibración
    pub settle: Duration,
    /// Diferencia aceptada entre el ángulo pedido y el informado (grados)
    pub tolerance: f64,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            move_timeout: Duration::from_secs(30),
            poll: Duration::from_millis(50),
            settle: Duration::from_millis(300),
            tolerance: 0.2,
        }
    }
}

/// Rig scanEar sobre cualquier canal de bytes (el puerto serie, o uno
/// simulado en las pruebas)
pub struct ScanEarRig<T: Read + Write> {
    port: BufReader<T>,
    pub timing: Timing,
}

impl ScanEarRig<Box<dyn serialport::SerialPort>> {
    /// Abre el puerto (normalmente `/dev/ttyACM0`). El Arduino se reinicia al
    /// abrirlo: se espera a que arranque y se descarta su saludo
    pub fn open(path: &str) -> io::Result<Self> {
        let port = serialport::new(path, 115_200).timeout(Duration::from_millis(200)).open()?;
        std::thread::sleep(Duration::from_secs(2));
        port.clear(serialport::ClearBuffer::Input)?;
        Ok(ScanEarRig::new(port))
    }
}

impl<T: Read + Write> ScanEarRig<T> {
    pub fn new(port: T) -> Self {
        ScanEarRig { port: BufReader::new(port), timing: Timing::default() }
    }

    fn send(&mut self, command: &str) -> io::Result<()> {
        let port = self.port.get_mut();
        port.write_all(command.as_bytes())?;
        port.write_all(b"\n")?;
        port.flush()
    }

    /// Siguiente línea; `None` si el puerto no trajo nada a tiempo
    fn read_reply(&mut self) -> io::Result<Option<Reply>> {
        let mut line = String::new();
        match self.port.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(parse_reply(&line))),
            Err(e) if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Envía un comando y espera `OK` o `ERROR`
    fn command(&mut self, command: &str) -> io::Result<()> {
        self.send(command)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.read_reply()? {
                Some(Reply::Ok) => return Ok(()),
                Some(Reply::Error(m)) => return Err(io::Error::other(format!("{command}: {m}"))),
                _ => {}
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, format!("{command}: sin respuesta")))
    }

    pub fn status(&mut self) -> io::Result<Status> {
        self.status_before(Instant::now() + Duration::from_secs(2))
    }

    /// Estado, esperando hasta `deadline`: un firmware que bloquea mientras
    /// mueve el motor recién contesta al terminar
    fn status_before(&mut self, deadline: Instant) -> io::Result<Status> {
        self.send("PY:GET_STATUS")?;
        while Instant::now() < deadline {
            match self.read_reply()? {
                Some(Reply::Status(s)) => return Ok(s),
                Some(Reply::Error(m)) => return Err(io::Error::other(m)),
                _ => {}
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "PY:GET_STATUS: sin respuesta"))
    }

    /// Espera a que el rig quede quieto, y si se dan, en esos ángulos
    fn wait_still(&mut self, plate: Option<f64>, arc: Option<f64>) -> io::Result<Status> {
        let t = self.timing;
        let deadline = Instant::now() + t.move_timeout;
        let close = |a: f64, b: f64| {
            let d = (a - b).rem_euclid(360.0);
            d.min(360.0 - d) <= t.tolerance
        };
        loop {
            let s = self.status_before(deadline)?;
            if s.state == 0 && plate.is_none_or(|p| close(s.y_angle, p)) && arc.is_none_or(|z| close(s.z_angle, z)) {
                std::thread::sleep(t.settle);
                return Ok(s);
            }
            if Instant::now() > deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, format!("el rig no llegó: {s:?}")));
            }
            std::thread::sleep(t.poll);
        }
    }
}

impl<T: Read + Write> Rig for ScanEarRig<T> {
    fn home(&mut self) -> io::Result<()> {
        self.send("PY:RESET")?;
        self.wait_still(Some(0.0), Some(0.0)).map(|_| ())
    }

    fn move_to(&mut self, plate_deg: f64, elevation_deg: f64) -> io::Result<()> {
        if !(0.0..=135.0).contains(&elevation_deg) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("el arco va de 0 a 135°, no {elevation_deg}°")));
        }
        let plate = plate_deg.rem_euclid(360.0);
        self.send(&format!("PY:MOVE_Y:{plate:.3}"))?;
        self.wait_still(Some(plate), None)?;
        self.send(&format!("PY:MOVE_Z:{elevation_deg:.3}"))?;
        self.wait_still(Some(plate), Some(elevation_deg)).map(|_| ())
    }

    fn set_laser(&mut self, index: usize, on: bool) -> io::Result<()> {
        self.command(&format!("PY:LASER:{index}:{}", on as u8))
    }

    fn set_light(&mut self, level: f64) -> io::Result<()> {
        self.command(&format!("PY:LIGHT:{}", (level.clamp(0.0, 1.0) * 255.0).round() as u8))
    }
}
