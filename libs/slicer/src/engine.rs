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

    /// Lo deja en `<caché>/pinocchio/slicer/<id>/` (una vez por versión del motor), con
    /// lo que vaya al lado (las DLL en Windows).
    #[cfg(embedded_engine)]
    pub fn extract() -> Option<PathBuf> {
        use std::sync::Mutex;
        static DONE: Mutex<Option<PathBuf>> = Mutex::new(None);
        let mut done = DONE.lock().ok()?;
        if let Some(p) = done.as_ref().filter(|p| p.is_file()) {
            return Some(p.clone());
        }
        let root = cache_dir()?.join("pinocchio").join("slicer");
        let dir = root.join(ENGINE_ID);
        let path = dir.join(super::ENGINE_NAME);
        if !path.is_file() {
            std::fs::create_dir_all(&root).ok()?;
            // Se extrae en otra carpeta y se renombra: un corte a medias no deja un motor roto
            let tmp = root.join(format!(".{ENGINE_ID}.{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
            if unpack(&tmp).is_err() {
                let _ = std::fs::remove_dir_all(&tmp);
                return None;
            }
            let _ = std::fs::remove_dir_all(&dir);
            if std::fs::rename(&tmp, &dir).is_err() {
                // Otra instancia de la app lo extrajo al mismo tiempo
                let _ = std::fs::remove_dir_all(&tmp);
                if !path.is_file() {
                    return None;
                }
            }
            // Las versiones viejas del motor sobran (las carpetas con punto son extracciones en curso)
            if let Ok(entries) = std::fs::read_dir(&root) {
                for e in entries.flatten() {
                    let name = e.file_name();
                    if name != ENGINE_ID && !name.to_string_lossy().starts_with('.') {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        }
        *done = Some(path.clone());
        Some(path)
    }

    /// Escribe los archivos de `ENGINE_GZ` en `dir` (formato en build.rs).
    #[cfg(embedded_engine)]
    fn unpack(dir: &std::path::Path) -> std::io::Result<()> {
        use std::io::{Error, Read, Write};
        std::fs::create_dir_all(dir)?;
        let mut gz = flate2::read::GzDecoder::new(ENGINE_GZ);
        let mut found = false;
        loop {
            let mut len = [0u8; 2];
            match gz.read_exact(&mut len) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
            let mut name = vec![0u8; u16::from_le_bytes(len) as usize];
            gz.read_exact(&mut name)?;
            let name = String::from_utf8(name).map_err(Error::other)?;
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Err(Error::other(format!("nombre inválido en el motor embebido: {name}")));
            }
            let mut size = [0u8; 8];
            gz.read_exact(&mut size)?;
            let size = u64::from_le_bytes(size);
            let file = dir.join(&name);
            let mut out = std::fs::File::create(&file)?;
            if std::io::copy(&mut (&mut gz).take(size), &mut out)? != size {
                return Err(Error::other("motor embebido cortado"));
            }
            out.flush()?;
            out.sync_all()?;
            if name == super::ENGINE_NAME {
                found = true;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
        if found { Ok(()) } else { Err(Error::other("el motor embebido no trae el ejecutable")) }
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
///
/// Los soportes no se le dejan al usuario: si están apagados y Orca ve que la
/// pieza los necesita (zonas en el aire, voladizos, primera capa vacía), se
/// corta ese laminado y se repite con soportes (los automáticos solo van donde
/// hacen falta), con un aviso que lo dice.
pub fn run(job: &SliceJob, engine: &Path, mut on_event: impl FnMut(Event), cancel: &AtomicBool) -> Result<Summary, SlicerError> {
    let supports_on = matches!(job.config.get("enable_support"), Some(Value::String(v)) if v == "1") || job.config.get("enable_support") == Some(&Value::Bool(true));
    match run_once(job, engine, &mut on_event, cancel, !supports_on) {
        Err(SlicerError::NeedsSupport(why)) => {
            let mut config = job.config.clone();
            config.insert("enable_support".into(), Value::String("1".into()));
            let with = SliceJob { config, parts: job.parts.clone(), work_dir: job.work_dir.clone() };
            let note = format!("Se pusieron soportes: {why}");
            on_event(Event::Warning { text: note.clone() });
            let mut summary = run_once(&with, engine, &mut on_event, cancel, false)?;
            summary.warnings.insert(0, note);
            Ok(summary)
        }
        other => other,
    }
}

/// Si el aviso o error de Orca dice que sin soportes no se puede imprimir
fn needs_support(text: &str) -> bool {
    text == "Errors"
        || text.starts_with("One object has an empty first layer")
        || (text.starts_with("It seems object ") && text.ends_with("Please re-orient the object or enable support generation."))
}

/// Por qué hicieron falta los soportes, para el aviso
fn support_reason(text: &str) -> String {
    let es = orca_text(text);
    match es.split_once(": reoriéntala") {
        Some((what, _)) => what.replacen("La pieza", "la pieza", 1),
        None => "la pieza apenas toca la cama".to_string(),
    }
}

/// Un laminado. Con `watch_support`, se corta apenas Orca dice que hacen falta soportes.
fn run_once(job: &SliceJob, engine: &Path, on_event: &mut impl FnMut(Event), cancel: &AtomicBool, watch_support: bool) -> Result<Summary, SlicerError> {
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

    let mut cmd = Command::new(engine);
    #[cfg(windows)]
    {
        // Sin abrir una ventana de consola al laminar
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
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
                Ok(Line::Warning { text }) if watch_support && needs_support(&text) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SlicerError::NeedsSupport(support_reason(&text)));
                }
                Ok(Line::Warning { text }) => {
                    let text = orca_text(&text);
                    warnings.push(text.clone());
                    on_event(Event::Warning { text });
                }
                Ok(Line::Done { gcode, time_s, filament_mm, filament_cm3, filament_g, layers, roles }) => {
                    summary = Some(Summary { gcode, time_s, filament_mm, filament_cm3, filament_g, layers, warnings: Vec::new(), roles });
                }
                Ok(Line::Error { text }) if watch_support && text.lines().any(needs_support) => {
                    let _ = child.wait();
                    return Err(SlicerError::NeedsSupport(support_reason(text.lines().find(|l| needs_support(l)).unwrap_or(&text))));
                }
                Ok(Line::Error { text }) => error = Some(text.lines().map(orca_text).collect::<Vec<_>>().join("\n")),
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

/// Los mensajes de Orca que más se ven, en español (la interfaz los lleva al
/// idioma activo con `tBackend`); los demás quedan como vienen.
pub(crate) fn orca_text(text: &str) -> String {
    const EMPTY_FIRST: &str = "La primera capa de la pieza queda vacía (apenas toca la cama): apóyala sobre una cara plana, córtale la base o activa los soportes";
    // Motores sin el arreglo: `SlicingErrors` sin su contenido. En Orca 2.4.2
    // solo la lanza la primera capa vacía
    if text == "Errors" || text.starts_with("One object has an empty first layer") {
        return EMPTY_FIRST.to_string();
    }
    if text.starts_with("No object can be printed. Maybe too small") {
        return "No hay nada que imprimir: la pieza es demasiado chica".to_string();
    }
    if let Some(rest) = text.strip_prefix("It seems object ")
        && let Some((name, what)) = rest.split_once(" has ")
        && let Some(what) = what.strip_suffix(". Please re-orient the object or enable support generation.")
    {
        let what = match what {
            "floating regions" => "zonas en el aire",
            "floating cantilever" => "un voladizo en el aire",
            "large overhangs" => "voladizos grandes",
            other => other,
        };
        let name = name.strip_prefix("pieza_").and_then(|n| n.strip_suffix(".stl")).unwrap_or(name);
        return format!("La pieza {name} tiene {what}: reoriéntala o activa los soportes");
    }
    text.to_string()
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

    #[test]
    fn orca_messages_come_out_in_spanish() {
        let empty = super::orca_text("Errors");
        assert!(empty.starts_with("La primera capa"), "{empty}");
        assert_eq!(super::orca_text("One object has an empty first layer and can't be printed. Please Cut the bottom or enable supports."), empty);
        assert_eq!(
            super::orca_text("It seems object pieza_2.stl has floating regions. Please re-orient the object or enable support generation."),
            "La pieza 2 tiene zonas en el aire: reoriéntala o activa los soportes"
        );
        assert_eq!(super::orca_text("algo que no se conoce"), "algo que no se conoce");
        assert_eq!(super::support_reason("Errors"), "la pieza apenas toca la cama");
        assert_eq!(
            super::support_reason("It seems object pieza_1.stl has large overhangs. Please re-orient the object or enable support generation."),
            "la pieza 1 tiene voladizos grandes"
        );
        assert!(!super::needs_support("Detect overhangs for auto-lift"));
    }
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
        let src = PathBuf::from(std::env::var("PINOCCHIO_SLICER_EMBED").unwrap());
        let src = if src.is_dir() { src.join(ENGINE_NAME) } else { src };
        let original = std::fs::read(src).unwrap();
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
