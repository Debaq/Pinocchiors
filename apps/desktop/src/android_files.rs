//! Archivos en Android: cruzar entre el teléfono y la carpeta de la app.
//!
//! En Android el selector de archivos entrega `content://…`, que el backend no
//! puede abrir con rutas normales. Por eso todo se trabaja con copias dentro de
//! la app y solo se cruza en dos puntos:
//! - **traer** (`android_import_uri`): el archivo elegido se copia a
//!   `importados/<tanda>/` (o a `Proyectos/` si es un `.pinocchio`) y se devuelve
//!   la ruta local; desde ahí se importa como siempre;
//! - **sacar** (`android_export_begin`): el destino elegido con «Guardar en…» se
//!   recuerda, la app escribe en una ruta local de paso y, cuando el archivo
//!   termina de escribirse, se copia al destino (o se abre el menú de compartir).
//!
//! La parte que no depende de Android (nombres, tipo por el contenido, esperar a
//! que un archivo termine de escribirse) se prueba en escritorio.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Carpeta de los proyectos guardados dentro de la app
pub fn projects_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app_subdir(app, "Proyectos")
}

fn app_subdir(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Decodifica `%xx` de una URI (lo demás queda igual)
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extensión en minúsculas de un nombre (`pieza.STL` → `stl`)
fn extension_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > 10 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Nombre de archivo que trae la URI, si trae uno con extensión.
/// `content://…/document/primary%3ADownload%2Fpieza.stl` → `pieza.stl`;
/// `content://…/document/msf%3A1234` → nada.
fn name_from_uri(uri: &str) -> Option<String> {
    let decoded = percent_decode(uri.split(['?', '#']).next().unwrap_or(uri));
    let last = decoded.rsplit('/').next()?;
    let last = last.rsplit(':').next()?;
    extension_of(last).map(|_| last.to_string())
}

/// Nombre sin caracteres que estorban en una ruta
fn clean_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() { "archivo".into() } else { cleaned }
}

/// `dir/nombre.ext`, o `dir/nombre (2).ext`… si ya existe
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("siempre hay un nombre libre")
}

/// Tipo de archivo mirando el contenido (cuando la URI no trae el nombre)
fn sniff_extension(path: &Path) -> Option<&'static str> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut head = vec![0u8; 4096];
    let n = file.read(&mut head).ok()?;
    head.truncate(n);
    let starts = |magic: &[u8]| head.starts_with(magic);

    if starts(b"PK\x03\x04") {
        file.rewind().ok()?;
        let zip = zip::ZipArchive::new(file).ok()?;
        let has = |entry: &str| zip.file_names().any(|f| f.eq_ignore_ascii_case(entry));
        return if has("manifest.json") && has("state.msgpack") {
            Some("pinocchio")
        } else if has("3D/3dmodel.model") {
            Some("3mf")
        } else {
            None
        };
    }
    if starts(b"\x89PNG") {
        return Some("png");
    }
    if starts(b"\xFF\xD8\xFF") {
        return Some("jpg");
    }
    if head.len() >= 12 && starts(b"RIFF") && &head[8..12] == b"WEBP" {
        return Some("webp");
    }
    if starts(b"glTF") {
        return Some("glb");
    }
    if starts(b"#?RADIANCE") || starts(b"#?RGBE") {
        return Some("hdr");
    }
    if starts(b"ISO-10303-21") {
        return Some("step");
    }
    if starts(b"ply\n") || starts(b"ply\r\n") {
        return Some("ply");
    }
    // STL binario: 80 bytes de cabecera, cantidad de triángulos y 50 bytes por triángulo
    if head.len() >= 84 {
        let triangles = u32::from_le_bytes([head[80], head[81], head[82], head[83]]) as u64;
        if triangles > 0 && len == 84 + 50 * triangles {
            return Some("stl");
        }
    }
    let text = String::from_utf8_lossy(&head);
    let trimmed = text.trim_start();
    if trimmed.starts_with("solid") && text.contains("facet") {
        return Some("stl");
    }
    if text.contains("\"asset\"") && text.contains("\"version\"") && trimmed.starts_with('{') {
        return Some("gltf");
    }
    let obj_lines = text.lines().filter(|l| l.starts_with("v ") || l.starts_with("f ") || l.starts_with("vn ")).count();
    if obj_lines >= 3 {
        return Some("obj");
    }
    if text.lines().any(|l| l.starts_with("newmtl ")) {
        return Some("mtl");
    }
    if starts(b"BM") {
        return Some("bmp");
    }
    None
}

/// Elige el nombre del archivo traído: el de la URI, el de la ruta real del
/// descriptor o, si ninguno sirve, uno por el contenido. Revisa que el tipo sea
/// uno de los aceptados (`allowed` vacío = cualquiera).
fn choose_name(uri_name: Option<String>, fd_name: Option<String>, staged: &Path, allowed: &[String]) -> Result<String, String> {
    let ok = |name: &str| extension_of(name).is_some_and(|e| allowed.is_empty() || allowed.iter().any(|a| a.eq_ignore_ascii_case(&e)));
    for name in [uri_name.as_deref(), fd_name.as_deref()].into_iter().flatten() {
        if ok(name) {
            return Ok(clean_name(name));
        }
    }
    if let Some(ext) = sniff_extension(staged)
        && (allowed.is_empty() || allowed.iter().any(|a| a.eq_ignore_ascii_case(ext)))
    {
        let stem = uri_name.or(fd_name).and_then(|n| n.rsplit_once('.').map(|(s, _)| s.to_string())).unwrap_or_else(|| "archivo".into());
        return Ok(clean_name(&format!("{stem}.{ext}")));
    }
    let wanted = allowed.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ");
    Err(format!("Ese archivo no es de un tipo que se pueda abrir aquí ({wanted})"))
}

/// Espera a que `path` exista y deje de crecer (mismo tamaño y fecha durante
/// `quiet`). Falso si no apareció antes de `timeout`.
fn wait_written(path: &Path, quiet: Duration, timeout: Duration) -> bool {
    let start = Instant::now();
    let step = Duration::from_millis(250);
    let mut last: Option<(u64, Option<std::time::SystemTime>)> = None;
    let mut still_since = Instant::now();
    while start.elapsed() < timeout {
        let now = std::fs::metadata(path).ok().filter(|m| m.len() > 0).map(|m| (m.len(), m.modified().ok()));
        match (&now, &last) {
            (Some(a), Some(b)) if a == b => {
                if still_since.elapsed() >= quiet {
                    return true;
                }
            }
            _ => still_since = Instant::now(),
        }
        last = now;
        std::thread::sleep(step);
    }
    false
}

/// Copia por bloques grandes (archivos de cientos de MB sin pasar por el WebView)
fn copy_stream(mut from: impl Read, mut to: impl std::io::Write) -> std::io::Result<u64> {
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            break;
        }
        to.write_all(&buf[..n])?;
        total += n as u64;
    }
    to.flush()?;
    Ok(total)
}

#[cfg(target_os = "android")]
fn open_uri(app: &AppHandle, uri: &str, write: bool) -> Result<std::fs::File, String> {
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
    let url = tauri::Url::parse(uri).map_err(|e| format!("Dirección inválida: {e}"))?;
    let mut opts = OpenOptions::new();
    if write {
        opts.write(true).truncate(true).create(true).read(false);
    } else {
        opts.read(true);
    }
    app.fs().open(FilePath::Url(url), opts).map_err(|e| format!("No se pudo abrir el archivo del teléfono: {e}"))
}

#[cfg(not(target_os = "android"))]
fn open_uri(_app: &AppHandle, _uri: &str, _write: bool) -> Result<std::fs::File, String> {
    Err("Solo en Android".into())
}

/// Ruta real detrás de un descriptor (en muchos proveedores, `/storage/…/pieza.stl`)
fn fd_file_name(file: &std::fs::File) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let target = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()?;
        let name = target.file_name()?.to_string_lossy().into_owned();
        extension_of(&name).map(|_| name)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        None
    }
}

/// Trae un archivo del teléfono (`content://…`) a la app y devuelve su ruta local.
/// Los de una misma elección van a la misma carpeta (`batch`), así un OBJ
/// encuentra su .mtl y sus texturas. Un `.pinocchio` queda en Mis proyectos.
#[tauri::command]
pub async fn android_import_uri(app: AppHandle, uri: String, batch: String, allowed: Vec<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || import_uri(&app, &uri, &batch, &allowed))
        .await
        .map_err(|e| e.to_string())?
}

fn import_uri(app: &AppHandle, uri: &str, batch: &str, allowed: &[String]) -> Result<String, String> {
    let dir = app_subdir(app, "importados")?.join(clean_name(batch));
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let source = open_uri(app, uri, false)?;
    let fd_name = fd_file_name(&source);
    let staged = dir.join(".entrando");
    let copied = std::fs::File::create(&staged)
        .and_then(|out| copy_stream(std::io::BufReader::new(source), std::io::BufWriter::new(out)))
        .map_err(|e| format!("No se pudo copiar el archivo a la app: {e}"));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    let name = match choose_name(name_from_uri(uri), fd_name, &staged, allowed) {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }
    };
    let target_dir = if extension_of(&name).as_deref() == Some("pinocchio") { projects_dir(app)? } else { dir };
    let target = unique_path(&target_dir, &name);
    std::fs::rename(&staged, &target)
        .or_else(|_| std::fs::copy(&staged, &target).map(|_| ()).and_then(|_| std::fs::remove_file(&staged)))
        .map_err(|e| format!("No se pudo guardar {}: {e}", target.display()))?;
    Ok(target.to_string_lossy().into_owned())
}

#[derive(Clone, Serialize)]
struct ExportDone {
    name: String,
    ok: bool,
    /// Se abrió el menú de compartir (si no, se guardó con «Guardar en…»)
    shared: bool,
    error: Option<String>,
}

/// Carpeta nueva en la caché de la app (`exportar/<marca>`), borrando las de
/// más de un día: lo compartido tiene que seguir ahí un rato, hasta que la
/// otra app lo lea
fn fresh_cache_dir(app: &AppHandle, kind: &str) -> Result<PathBuf, String> {
    let base = app.path().app_cache_dir().map_err(|e| e.to_string())?.join(kind);
    if let Ok(entries) = std::fs::read_dir(&base) {
        let day = Duration::from_secs(24 * 3600);
        for entry in entries.flatten() {
            let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > day);
            if old {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = base.join(stamp.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Tipo MIME para el menú de compartir (los formatos 3D van como binarios:
/// así los aceptan WhatsApp, el correo o Drive)
fn mime_of(name: &str) -> &'static str {
    match extension_of(name).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("csv") => "text/csv",
        Some("json") => "application/json",
        Some("gcode" | "txt") => "text/plain",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

#[cfg(target_os = "android")]
fn open_share_menu(app: &AppHandle, path: &Path, title: &str) -> Result<(), String> {
    use tauri_plugin_pinocchio_android::PinocchioAndroidExt;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    app.pinocchio_android().share_file(&path.to_string_lossy(), mime_of(&name), title)
}

#[cfg(not(target_os = "android"))]
fn open_share_menu(_app: &AppHandle, _path: &Path, _title: &str) -> Result<(), String> {
    let _ = mime_of;
    Err("Compartir solo está en Android".into())
}

/// Exportar en el teléfono: devuelve una ruta local de paso donde la app
/// escribe como siempre; cuando el archivo termina de escribirse se copia a
/// `uri` (elegido con «Guardar en…») o, sin `uri`, se abre el menú de
/// compartir. Avisa con el evento `android-export`.
#[tauri::command]
pub fn android_export_begin(app: AppHandle, uri: Option<String>, name: String, share_title: Option<String>) -> Result<String, String> {
    let dir = fresh_cache_dir(&app, "exportar")?;
    let name = clean_name(&name);
    let local = dir.join(&name);
    let path = local.clone();
    std::thread::spawn(move || {
        // Si en media hora no se escribió nada, la exportación se canceló o falló
        if !wait_written(&path, Duration::from_millis(1500), Duration::from_secs(30 * 60)) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let shared = uri.is_none();
        let result = match &uri {
            Some(uri) => {
                let r = copy_to_uri(&app, &path, uri).map(|_| ());
                let _ = std::fs::remove_dir_all(&dir);
                r
            }
            // Lo compartido queda en la caché (se borra al día siguiente)
            None => open_share_menu(&app, &path, share_title.as_deref().unwrap_or("")),
        };
        use tauri::Emitter;
        let _ = app.emit("android-export", ExportDone { name, ok: result.is_ok(), shared, error: result.err() });
    });
    Ok(local.to_string_lossy().into_owned())
}

/// Comparte un archivo que ya está en la app (un proyecto guardado): se copia
/// a la caché, la única carpeta que se puede pasar a otras apps
#[tauri::command]
pub async fn android_share(app: AppHandle, path: String, title: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("Archivo sin nombre")?;
        let dir = fresh_cache_dir(&app, "compartir")?;
        let copy = dir.join(clean_name(&name));
        std::fs::copy(&source, &copy).map_err(|e| format!("No se pudo preparar {name} para compartir: {e}"))?;
        open_share_menu(&app, &copy, &title)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn copy_to_uri(app: &AppHandle, path: &Path, uri: &str) -> Result<u64, String> {
    let source = std::fs::File::open(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let target = open_uri(app, uri, true)?;
    copy_stream(std::io::BufReader::new(source), target).map_err(|e| format!("No se pudo escribir en el teléfono: {e}"))
}

/// Copia un archivo que ya está en la app (un proyecto guardado) al destino
/// elegido con «Guardar en…»
#[tauri::command]
pub async fn android_copy_to_uri(app: AppHandle, path: String, uri: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || copy_to_uri(&app, Path::new(&path), &uri)).await.map_err(|e| e.to_string())?
}

/// Carpeta de la app para lo que en escritorio pide una carpeta (cuadros de un
/// render, pruebas de exposición): en Android no se puede escribir en una
/// carpeta del teléfono elegida con el selector
#[tauri::command]
pub fn android_output_dir(app: AppHandle, name: String) -> Result<String, String> {
    let dir = app_subdir(&app, "exportados")?.join(clean_name(&name));
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(dir.to_string_lossy().into_owned())
}

#[derive(Serialize)]
pub struct ProjectEntry {
    pub path: String,
    pub name: String,
    /// Segundos desde 1970
    pub modified: u64,
    pub bytes: u64,
}

/// Los proyectos guardados dentro de la app, el más reciente primero
#[tauri::command]
pub fn android_projects_list(app: AppHandle) -> Result<Vec<ProjectEntry>, String> {
    list_projects(&projects_dir(&app)?)
}

fn list_projects(dir: &Path) -> Result<Vec<ProjectEntry>, String> {
    let mut out: Vec<ProjectEntry> = std::fs::read_dir(dir)
        .map_err(|e| format!("No se pudo leer {}: {e}", dir.display()))?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if extension_of(&path.file_name()?.to_string_lossy()).as_deref() != Some("pinocchio") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            Some(ProjectEntry {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                modified,
                bytes: meta.len(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// Ruta para guardar un proyecto nuevo dentro de la app con este nombre
/// (sin pisar otro: «Nombre (2)»)
#[tauri::command]
pub fn android_project_path(app: AppHandle, name: String) -> Result<String, String> {
    let name = clean_name(name.trim().trim_end_matches(".pinocchio"));
    Ok(unique_path(&projects_dir(&app)?, &format!("{name}.pinocchio")).to_string_lossy().into_owned())
}

/// Borra un proyecto guardado dentro de la app (solo de esa carpeta)
#[tauri::command]
pub fn android_project_delete(app: AppHandle, path: String) -> Result<(), String> {
    let dir = projects_dir(&app)?;
    let path = PathBuf::from(path);
    if path.parent() != Some(dir.as_path()) || extension_of(&path.to_string_lossy()).as_deref() != Some("pinocchio") {
        return Err("Solo se pueden borrar proyectos de la app".into());
    }
    std::fs::remove_file(&path).map_err(|e| format!("No se pudo borrar {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_desde_la_uri() {
        assert_eq!(
            name_from_uri("content://com.android.externalstorage.documents/document/primary%3ADownload%2Fpieza.STL").as_deref(),
            Some("pieza.STL")
        );
        assert_eq!(name_from_uri("content://com.android.providers.downloads.documents/document/msf%3A1000000123"), None);
        assert_eq!(name_from_uri("content://media/external/file/123?x=1"), None);
        assert_eq!(
            name_from_uri("content://com.whatsapp.provider.media/item/soporte%20brazo.step").as_deref(),
            Some("soporte brazo.step")
        );
        assert_eq!(percent_decode("a%2"), "a%2");
    }

    #[test]
    fn nombres_limpios_y_sin_pisar() {
        assert_eq!(clean_name("a/b:c.stl"), "a_b_c.stl");
        assert_eq!(clean_name("  "), "archivo");
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "p.stl"), dir.path().join("p.stl"));
        std::fs::write(dir.path().join("p.stl"), b"x").unwrap();
        assert_eq!(unique_path(dir.path(), "p.stl"), dir.path().join("p (2).stl"));
    }

    #[test]
    fn tipo_por_el_contenido() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, bytes: &[u8]| {
            let p = dir.path().join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let mut stl = vec![0u8; 80];
        stl.extend(2u32.to_le_bytes());
        stl.extend(vec![0u8; 100]);
        assert_eq!(sniff_extension(&file("a", &stl)), Some("stl"));
        assert_eq!(sniff_extension(&file("b", b"solid x\n facet normal 0 0 1\n")), Some("stl"));
        assert_eq!(sniff_extension(&file("c", b"ISO-10303-21;\nHEADER;")), Some("step"));
        assert_eq!(sniff_extension(&file("d", b"ply\nformat ascii 1.0\n")), Some("ply"));
        assert_eq!(sniff_extension(&file("e", b"# x\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")), Some("obj"));
        assert_eq!(sniff_extension(&file("f", b"\x89PNG\r\n")), Some("png"));
        assert_eq!(sniff_extension(&file("g", b"hola")), None);

        let zip_with = |name: &str, entries: &[&str]| {
            let p = dir.path().join(name);
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            for e in entries {
                zip.start_file(*e, zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut zip, b"x").unwrap();
            }
            zip.finish().unwrap();
            p
        };
        assert_eq!(sniff_extension(&zip_with("h", &["manifest.json", "state.msgpack", "ui.json"])), Some("pinocchio"));
        assert_eq!(sniff_extension(&zip_with("i", &["[Content_Types].xml", "3D/3dmodel.model"])), Some("3mf"));
    }

    #[test]
    fn elegir_nombre_y_revisar_tipo() {
        let dir = tempfile::tempdir().unwrap();
        let staged = dir.path().join(".entrando");
        std::fs::write(&staged, b"ISO-10303-21;\n").unwrap();
        let allowed = vec!["step".to_string(), "stp".to_string()];
        assert_eq!(choose_name(Some("x.STEP".into()), None, &staged, &allowed).unwrap(), "x.STEP");
        assert_eq!(choose_name(None, Some("y.stp".into()), &staged, &allowed).unwrap(), "y.stp");
        // Sin nombre: por el contenido
        assert_eq!(choose_name(None, None, &staged, &allowed).unwrap(), "archivo.step");
        // Nombre con otra extensión, pero el contenido es STEP
        assert_eq!(choose_name(Some("descarga.bin".into()), None, &staged, &allowed).unwrap(), "descarga.step");
        // Un STEP donde se pedía STL
        assert!(choose_name(Some("x.step".into()), None, &staged, &["stl".to_string()]).is_err());
        // Sin lista: cualquier cosa con nombre
        assert_eq!(choose_name(Some("notas.txt".into()), None, &staged, &[]).unwrap(), "notas.txt");
    }

    #[test]
    fn espera_a_que_termine_de_escribirse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("salida.stl");
        assert!(!wait_written(&path, Duration::from_millis(300), Duration::from_millis(600)));
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || {
                use std::io::Write;
                let mut f = std::fs::File::create(&path).unwrap();
                for _ in 0..4 {
                    f.write_all(&[1u8; 1000]).unwrap();
                    f.flush().unwrap();
                    std::thread::sleep(Duration::from_millis(200));
                }
            })
        };
        assert!(wait_written(&path, Duration::from_millis(700), Duration::from_secs(10)));
        writer.join().unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 4000);
    }

    #[test]
    fn tipos_para_compartir() {
        assert_eq!(mime_of("plano.PDF"), "application/pdf");
        assert_eq!(mime_of("pieza.stl"), "application/octet-stream");
        assert_eq!(mime_of("pieza.gcode"), "text/plain");
        assert_eq!(mime_of("sin_extension"), "application/octet-stream");
    }

    #[test]
    fn lista_de_proyectos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pinocchio"), b"1").unwrap();
        std::fs::write(dir.path().join("b.stl"), b"1").unwrap();
        let list = list_projects(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "a");
    }
}
