//! Lanza el motor `pinocchio-slicer` y lee sus eventos.

use crate::error::SlicerError;
use crate::stl::write_binary_stl;
use crate::thumbnail::{parse_color, render_thumbnail};
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
/// 2. El que va dentro de la app (release): se extrae a la caché la primera vez.
/// 3. Junto al ejecutable de la app: `pinocchio-slicer` o `<ejecutable>-slicer`.
/// 4. `libs/slicer/bin/` del repositorio (desarrollo, ver scripts/build-orca-engine.sh).
///
/// Un motor sin permiso de ejecución (descargado suelto) se marca ejecutable.
pub fn find_engine() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PINOCCHIO_SLICER")
        && !p.is_empty()
    {
        let p = PathBuf::from(p);
        return p.is_file().then(|| executable(p));
    }
    if let Some(p) = embedded::extract() {
        return Some(p);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join(ENGINE_NAME);
        if p.is_file() {
            return Some(executable(p));
        }
        if let Some(p) = exe.file_name().map(|n| dir.join(sibling_engine_name(&n.to_string_lossy())))
            && p.is_file()
        {
            return Some(executable(p));
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("bin").join(ENGINE_NAME);
    dev.is_file().then(|| executable(dev))
}

/// Si al archivo le falta el permiso de ejecución (un navegador no lo deja), se lo pone.
fn executable(path: PathBuf) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.permissions().mode() & 0o111 == 0
        {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&path, perms);
        }
    }
    path
}

/// El motor dentro del binario de la app (`PINOCCHIO_SLICER_EMBED` al compilar).
mod embedded {
    use std::path::PathBuf;

    #[cfg(embedded_engine)]
    include!(concat!(env!("OUT_DIR"), "/engine_embed.rs"));

    /// Lo deja en `<caché>/pinocchio/slicer/<id>/` (una vez por versión del motor).
    #[cfg(embedded_engine)]
    pub fn extract() -> Option<PathBuf> {
        use std::sync::Mutex;
        static DONE: Mutex<Option<PathBuf>> = Mutex::new(None);
        let mut done = DONE.lock().ok()?;
        if let Some(p) = done.as_ref().filter(|p| p.is_file()) {
            return Some(p.clone());
        }
        let dir = cache_dir()?.join("pinocchio").join("slicer").join(ENGINE_ID);
        let path = dir.join(super::ENGINE_NAME);
        if !path.is_file() {
            std::fs::create_dir_all(&dir).ok()?;
            // Se escribe aparte y se renombra: un corte a medias no deja un motor roto
            let tmp = dir.join(format!(".{}.{}", super::ENGINE_NAME, std::process::id()));
            let ok = (|| -> std::io::Result<()> {
                let mut out = std::fs::File::create(&tmp)?;
                std::io::copy(&mut flate2::read::GzDecoder::new(ENGINE_GZ), &mut out)?;
                out.sync_all()?;
                Ok(())
            })();
            if ok.is_err() {
                let _ = std::fs::remove_file(&tmp);
                return None;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).ok()?;
            }
            std::fs::rename(&tmp, &path).ok()?;
            // Las versiones viejas del motor sobran
            if let Ok(entries) = std::fs::read_dir(dir.parent()?) {
                for e in entries.flatten() {
                    if e.file_name() != ENGINE_ID {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        }
        *done = Some(path.clone());
        Some(path)
    }

    #[cfg(not(embedded_engine))]
    pub fn extract() -> Option<PathBuf> {
        None
    }

    #[cfg(embedded_engine)]
    fn cache_dir() -> Option<PathBuf> {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        if cfg!(windows) {
            env("LOCALAPPDATA")
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|h| h.join("Library").join("Caches"))
        } else {
            env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|h| h.join(".cache")))
        }
    }
}

/// `Pinocchio-pre.2026.11.04.abc1234-linux-x86_64` → `…-linux-x86_64-slicer` (el nombre
/// publicado tiene puntos: no se puede usar `file_stem`).
fn sibling_engine_name(exe_name: &str) -> String {
    match exe_name.strip_suffix(".exe") {
        Some(base) if cfg!(windows) => format!("{base}-slicer.exe"),
        _ => format!("{exe_name}-slicer"),
    }
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
    /// Filamento por tipo de extrusión (nombres de Orca: «Outer wall», «Sparse infill»…)
    #[serde(default)]
    pub roles: Vec<RoleUse>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleUse {
    pub role: String,
    /// Metros de filamento
    pub m: f64,
    /// Gramos
    pub g: f64,
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
enum Line {
    Progress { percent: i32, text: String },
    Warning { text: String },
    Done {
        gcode: PathBuf,
        time_s: f64,
        filament_mm: f64,
        filament_cm3: f64,
        filament_g: f64,
        layers: u32,
        #[serde(default)]
        roles: Vec<RoleUse>,
    },
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
    let thumbs = job.work_dir.join("miniaturas");
    write_thumbnails(&job.config, &job.parts, &thumbs)?;

    let mut child = Command::new(engine)
        .arg("--config")
        .arg(&config_path)
        .arg("--output")
        .arg(&output)
        .arg("--thumbnails")
        .arg(&thumbs)
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
                Ok(Line::Done { gcode, time_s, filament_mm, filament_cm3, filament_g, layers, roles }) => {
                    summary = Some(Summary { gcode, time_s, filament_mm, filament_cm3, filament_g, layers, warnings: Vec::new(), roles });
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

/// Tamaños de miniatura que pide el perfil (`thumbnails`: «96x96, 300x300» o una lista; a veces «300x300/PNG»).
pub(crate) fn thumbnail_sizes(config: &Map<String, Value>) -> Vec<(u32, u32)> {
    let items: Vec<String> = match config.get("thumbnails") {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::String(s)) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    items
        .iter()
        .filter_map(|it| {
            let size = it.trim().split('/').next()?;
            let (w, h) = size.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .filter(|&(w, h): &(u32, u32)| w > 0 && h > 0 && w <= 1024 && h <= 1024)
        .collect()
}

/// Dibuja las miniaturas que pide el perfil en `dir/thumb_WxH.rgba`.
fn write_thumbnails(config: &Map<String, Value>, parts: &[Vec<[[f32; 3]; 3]>], dir: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    let color = match config.get("filament_colour") {
        Some(Value::Array(a)) => a.first().and_then(Value::as_str).map(parse_color),
        Some(Value::String(s)) => Some(parse_color(s)),
        _ => None,
    }
    .unwrap_or([0xF2, 0x8C, 0x28]);
    for (w, h) in thumbnail_sizes(config) {
        let img = render_thumbnail(parts, w, h, color);
        std::fs::write(dir.join(format!("thumb_{w}x{h}.rgba")), img)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_del_motor_suelto() {
        assert_eq!(sibling_engine_name("Pinocchio-pre.2026.11.04.abc1234-linux-x86_64"), "Pinocchio-pre.2026.11.04.abc1234-linux-x86_64-slicer");
    }

    /// Solo con `PINOCCHIO_SLICER_EMBED` al compilar: el motor sale igual al original y ejecutable
    #[cfg(embedded_engine)]
    #[test]
    fn motor_embebido_se_extrae() {
        let p = embedded::extract().expect("extraer el motor");
        let original = std::fs::read(std::env::var("PINOCCHIO_SLICER_EMBED").unwrap()).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), original);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o111, 0);
        }
        assert_eq!(embedded::extract(), Some(p));
    }

    #[test]
    fn tamanos_de_miniatura() {
        let mut c = Map::new();
        c.insert("thumbnails".into(), serde_json::json!(["96x96", "300x300"]));
        assert_eq!(thumbnail_sizes(&c), vec![(96, 96), (300, 300)]);
        c.insert("thumbnails".into(), serde_json::json!("32x32/PNG, 400x300/PNG"));
        assert_eq!(thumbnail_sizes(&c), vec![(32, 32), (400, 300)]);
        c.remove("thumbnails");
        assert!(thumbnail_sizes(&c).is_empty());
    }
}
