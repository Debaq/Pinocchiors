//! Lanza el motor `pinocchio-slicer` y lee sus eventos.

use crate::error::SlicerError;
use crate::stl::write_binary_stl;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const ENGINE_NAME: &str = if cfg!(windows) { "pinocchio-slicer.exe" } else { "pinocchio-slicer" };

/// Busca el binario del motor:
/// 1. `PINOCCHIO_SLICER` (ruta explícita).
/// 2. Junto al ejecutable de la app (sidecar empaquetado).
/// 3. `libs/slicer/bin/` del repositorio (desarrollo, ver scripts/build-orca-engine.sh).
pub fn find_engine() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PINOCCHIO_SLICER")
        && !p.is_empty()
    {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join(ENGINE_NAME);
        if p.is_file() {
            return Some(p);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("bin").join(ENGINE_NAME);
    dev.is_file().then_some(dev)
}

/// Un laminado: configuración ya unida y las piezas en coordenadas de la cama.
pub struct SliceJob {
    pub config: Map<String, Value>,
    /// Cada pieza como triángulos en mm, Z arriba, apoyada en Z = 0.
    pub parts: Vec<Vec<[[f32; 3]; 3]>>,
    /// Carpeta donde quedan los STL, el perfil y el gcode.
    pub work_dir: PathBuf,
}

/// Lo que va contando el motor.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    Progress { percent: i32, text: String },
    Warning { text: String },
}

/// Resultado de un laminado.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub gcode: PathBuf,
    pub time_s: f64,
    pub filament_mm: f64,
    pub filament_cm3: f64,
    pub filament_g: f64,
    pub layers: u32,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
enum Line {
    Progress { percent: i32, text: String },
    Warning { text: String },
    Done { gcode: PathBuf, time_s: f64, filament_mm: f64, filament_cm3: f64, filament_g: f64, layers: u32 },
    Error { text: String },
    #[serde(other)]
    Other,
}

/// Lamina `job` con el motor en `engine`. `cancel` se revisa varias veces por
/// segundo: al activarse se mata el proceso.
pub fn run(job: &SliceJob, engine: &Path, mut on_event: impl FnMut(Event), cancel: &AtomicBool) -> Result<Summary, SlicerError> {
    if job.parts.iter().all(|p| p.is_empty()) {
        return Err(SlicerError::Empty);
    }
    std::fs::create_dir_all(&job.work_dir)?;
    let config_path = job.work_dir.join("perfil.json");
    std::fs::write(&config_path, serde_json::to_vec_pretty(&Value::Object(job.config.clone()))?)?;
    let mut models = Vec::new();
    for (i, part) in job.parts.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
        let path = job.work_dir.join(format!("pieza_{}.stl", i + 1));
        write_binary_stl(&path, part)?;
        models.push(path);
    }
    let output = job.work_dir.join("placa.gcode");
    let _ = std::fs::remove_file(&output);

    let mut child = Command::new(engine)
        .arg("--config")
        .arg(&config_path)
        .arg("--output")
        .arg(&output)
        .args(&models)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let (tx, rx) = mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let mut summary: Option<Summary> = None;
    let mut error: Option<String> = None;
    let mut warnings = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SlicerError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => match serde_json::from_str::<Line>(&line) {
                Ok(Line::Progress { percent, text }) => on_event(Event::Progress { percent, text }),
                Ok(Line::Warning { text }) => {
                    warnings.push(text.clone());
                    on_event(Event::Warning { text });
                }
                Ok(Line::Done { gcode, time_s, filament_mm, filament_cm3, filament_g, layers }) => {
                    summary = Some(Summary { gcode, time_s, filament_mm, filament_cm3, filament_g, layers, warnings: Vec::new() });
                }
                Ok(Line::Error { text }) => error = Some(text),
                Ok(Line::Other) | Err(_) => {}
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait()?;
    let _ = reader.join();
    let stderr_text = err_reader.join().unwrap_or_default();

    if let Some(e) = error {
        return Err(SlicerError::Engine(e));
    }
    match summary {
        Some(mut s) if status.success() => {
            s.warnings = warnings;
            Ok(s)
        }
        _ => {
            let tail: String = stderr_text.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            Err(SlicerError::Engine(if tail.is_empty() { format!("terminó con {status}") } else { tail }))
        }
    }
}
