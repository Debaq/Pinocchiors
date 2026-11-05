//! Impresoras en la red, sea cual sea la marca: Klipper con Moonraker (la K1 rooteada) o
//! Flashforge de fábrica (AD5X). La interfaz habla solo con estos comandos.
//!
//! Ver `libs/pinocchio/print3d/PLAN_IMPRESORAS.md`.

use crate::flashforge::{self, Flashforge, Slot};
use crate::moonraker::{self, PrinterStatus, SendResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cómo llegar a una impresora (se guarda en los ajustes de la app).
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrinterConn {
    /// "moonraker" o "flashforge"
    pub kind: String,
    pub address: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Flashforge: código del modo LAN
    #[serde(default)]
    pub check_code: Option<String>,
    /// Flashforge: número de serie (si falta se le pregunta a la impresora)
    #[serde(default)]
    pub serial: Option<String>,
}

enum Conn {
    Moonraker(moonraker::Printer),
    Flashforge(Flashforge),
}

fn open(c: &PrinterConn, timeout: Duration) -> Result<Conn, String> {
    match c.kind.as_str() {
        "moonraker" => Ok(Conn::Moonraker(moonraker::Printer::new(&c.address, c.api_key.clone(), timeout)?)),
        "flashforge" => Ok(Conn::Flashforge(Flashforge::new(&c.address, c.serial.clone(), c.check_code.clone(), timeout)?)),
        other => Err(format!("Tipo de impresora desconocido: {other}")),
    }
}

/// Corre el trabajo de red fuera del hilo de la interfaz.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Error interno: {e}"))?
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct NetPrinterInfo {
    pub kind: String,
    /// Nombre que da la impresora (K1-436A, AD5X)
    pub name: String,
    /// Lista para recibir trabajos
    pub ready: bool,
    /// Klipper: ready, startup, shutdown… · Flashforge: standby, printing…
    pub state: String,
    pub message: String,
    pub firmware: String,
    /// Flashforge: para guardarlo y no preguntarlo cada vez
    pub serial: Option<String>,
    pub slots: Vec<Slot>,
}

fn info(c: &PrinterConn) -> Result<NetPrinterInfo, String> {
    match open(c, Duration::from_secs(10))? {
        Conn::Moonraker(p) => {
            let i = moonraker::info(&p)?;
            Ok(NetPrinterInfo {
                kind: c.kind.clone(),
                name: if i.hostname.is_empty() { i.url } else { i.hostname },
                ready: i.klippy_state == "ready",
                state: i.klippy_state,
                message: i.message,
                firmware: i.klipper_version,
                serial: None,
                slots: Vec::new(),
            })
        }
        Conn::Flashforge(p) => {
            let i = flashforge::info(&p)?;
            Ok(NetPrinterInfo {
                kind: c.kind.clone(),
                name: i.name,
                ready: true,
                state: i.status,
                message: String::new(),
                firmware: i.firmware,
                serial: Some(i.serial),
                slots: i.slots,
            })
        }
    }
}

/// Prueba la conexión y dice qué impresora es (y sus carretes).
#[tauri::command]
pub async fn printer_info(conn: PrinterConn) -> Result<NetPrinterInfo, String> {
    blocking(move || info(&conn)).await
}

/// Estado de la impresora y de la impresión en curso.
#[tauri::command]
pub async fn printer_status(conn: PrinterConn) -> Result<PrinterStatus, String> {
    blocking(move || match open(&conn, Duration::from_secs(10))? {
        Conn::Moonraker(p) => moonraker::status(&p),
        Conn::Flashforge(p) => flashforge::status(&p),
    })
    .await
}

/// Sube el gcode del último laminado y, con `start`, lo empieza. `slot` (desde 1) elige el
/// carrete en las que tienen cambiador (AD5X).
#[tauri::command]
pub async fn printer_send(conn: PrinterConn, filename: String, start: bool, slot: Option<u32>) -> Result<SendResult, String> {
    let gcode = crate::slicer::last_gcode().ok_or("Todavía no hay nada laminado")?;
    blocking(move || {
        let bytes = std::fs::read(&gcode).map_err(|e| format!("No se pudo leer el gcode: {e}"))?;
        let name = moonraker::safe_filename(&filename);
        match open(&conn, Duration::from_secs(300))? {
            Conn::Moonraker(p) => moonraker::send_gcode(&p, &name, &bytes, start),
            Conn::Flashforge(p) => flashforge::send(&p, &name, &bytes, start, slot),
        }
    })
    .await
}

/// Pausar, reanudar o cancelar la impresión en curso.
#[tauri::command]
pub async fn printer_action(conn: PrinterConn, action: String) -> Result<(), String> {
    blocking(move || match open(&conn, Duration::from_secs(15))? {
        Conn::Moonraker(p) => moonraker::action(&p, &action),
        Conn::Flashforge(p) => flashforge::action(&p, &action),
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipo_desconocido() {
        let c = PrinterConn { kind: "octoprint".into(), address: "1.2.3.4".into(), api_key: None, check_code: None, serial: None };
        assert!(matches!(open(&c, Duration::from_secs(1)), Err(e) if e == "Tipo de impresora desconocido: octoprint"));
    }
}
