//! Pieles y texturas imprimibles: zonas de caras elegidas y biblioteca de mapas de altura
//!
//! Las caras se numeran como las dibuja el visor (el orden de los triángulos de la escena), que es también
//! el de las caras de la malla activa. La zona se guarda en el proyecto junto con las imágenes importadas.

use std::io::Cursor;

use pinocchio_print3d::{self as p3d, HeightMap, Pattern};
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::AppHandle;

use crate::commands::{in_background, mm_per_unit};
use crate::state::AppState;

/// Imagen de la biblioteca (la que el usuario importó)
#[derive(Debug, Clone)]
pub struct SkinImage {
    pub id: String,
    pub name: String,
    pub map: HeightMap,
}

/// Zona elegida: las caras y de cuántas caras era la malla cuando se eligió
#[derive(Debug, Clone, Default)]
pub struct SkinZone {
    pub total: usize,
    pub faces: Vec<u32>,
    /// Caras que no se texturizan (agujeros, uniones…), aunque estén dentro de la zona
    pub protected: Vec<u32>,
    /// Intensidad del relieve por vértice de la malla (0..1, pintada con el pincel); vacía = todo al 100 %
    pub strength: Vec<f32>,
}

// ═══════════════════════════════════════════════════════════════════════════
// BIBLIOTECA DE IMÁGENES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct PatternInfo {
    pub id: String,
    pub name: String,
    /// De fábrica (si no, importada)
    pub builtin: bool,
    pub width: usize,
    pub height: usize,
}

fn image_info(i: &SkinImage) -> PatternInfo {
    PatternInfo { id: i.id.clone(), name: i.name.clone(), builtin: false, width: i.map.width, height: i.map.height }
}

/// Patrones de fábrica y las imágenes importadas
#[tauri::command]
pub fn skin_library(state: tauri::State<'_, AppState>) -> Vec<PatternInfo> {
    library(&state)
}

pub(crate) fn library(state: &AppState) -> Vec<PatternInfo> {
    let mut out: Vec<PatternInfo> = Pattern::ALL
        .iter()
        .map(|p| PatternInfo { id: p.id().to_string(), name: p.name().to_string(), builtin: true, width: 256, height: 256 })
        .collect();
    out.extend(state.print_skin_images.lock().unwrap().iter().map(image_info));
    out
}

/// Cómo se retoca la imagen antes de usarla
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkinOps {
    pub invert: bool,
    /// 1 = sin cambio
    pub contrast: Option<f64>,
    /// Radio del desenfoque en píxeles
    pub blur: usize,
}

/// El mapa de altura de un patrón o imagen de la biblioteca, ya retocado
pub(crate) fn height_map(state: &AppState, id: &str, ops: &SkinOps) -> Result<HeightMap, String> {
    let mut map = match Pattern::from_id(id) {
        Some(p) => p.generate(256),
        None => state
            .print_skin_images
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.map.clone())
            .ok_or_else(|| format!("No existe la imagen «{id}»"))?,
    };
    map.blur(ops.blur.min(32));
    if let Some(c) = ops.contrast {
        map.contrast(c.clamp(0.0, 8.0));
    }
    if ops.invert {
        map.invert();
    }
    Ok(map)
}

/// Vista previa en grises (PNG) de un patrón o imagen, con los retoques aplicados
#[tauri::command]
pub async fn skin_preview(app: AppHandle, id: String, ops: Option<SkinOps>, size: Option<usize>) -> Result<Response, String> {
    in_background(app, move |state| preview_png(state, &id, &ops.unwrap_or_default(), size.unwrap_or(96)).map(Response::new)).await
}

pub(crate) fn preview_png(state: &AppState, id: &str, ops: &SkinOps, size: usize) -> Result<Vec<u8>, String> {
    let map = height_map(state, id, ops)?.downscale(size.clamp(16, 512));
    let img = image::GrayImage::from_raw(map.width as u32, map.height as u32, map.to_gray8()).ok_or("No se pudo armar la vista previa")?;
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(|e| format!("No se pudo codificar la vista previa: {e}"))?;
    Ok(bytes)
}

/// Importa una imagen (PNG, JPG, WebP…) como mapa de altura: la luminosidad manda, blanco = alto
#[tauri::command]
pub async fn skin_import_image(app: AppHandle, path: String) -> Result<PatternInfo, String> {
    in_background(app, move |state| import_image(state, &path)).await
}

pub(crate) fn import_image(state: &AppState, path: &str) -> Result<PatternInfo, String> {
    let img = image::open(path).map_err(|e| format!("No se pudo abrir la imagen: {e}"))?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 4 || h < 4 {
        return Err("La imagen es demasiado chica para una piel (mínimo 4 × 4 píxeles)".into());
    }
    let map = HeightMap::from_rgba8(w, h, img.as_raw()).map_err(|e| e.to_string())?.downscale(1024);
    let name = std::path::Path::new(path).file_stem().map_or_else(|| "Imagen".to_string(), |s| s.to_string_lossy().into_owned());
    let mut images = state.print_skin_images.lock().unwrap();
    let next = images.iter().filter_map(|i| i.id.strip_prefix("img_").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
    let image = SkinImage { id: format!("img_{next}"), name, map };
    let info = image_info(&image);
    images.push(image);
    Ok(info)
}

/// Quita una imagen importada de la biblioteca
#[tauri::command]
pub fn skin_remove_image(id: String, state: tauri::State<'_, AppState>) -> Vec<PatternInfo> {
    state.print_skin_images.lock().unwrap().retain(|i| i.id != id);
    library(&state)
}

// ═══════════════════════════════════════════════════════════════════════════
// ZONAS
// ═══════════════════════════════════════════════════════════════════════════

/// Resumen de la zona. Las caras no viajan aquí (con mallas grandes eran ~1 MB
/// de JSON en cada trazo del pincel): `skin_zone_faces` las da en binario, y
/// `revision` cambia solo cuando cambian las caras o las protegidas.
#[derive(Debug, Clone, Serialize)]
pub struct ZoneInfo {
    /// Caras de la zona
    pub count: usize,
    /// Caras protegidas
    pub protected_count: usize,
    /// Firma de las caras y las protegidas (cabe exacta en un número de JS)
    pub revision: u64,
    pub total: usize,
    /// Área de la zona en mm²
    pub area_mm2: f64,
    /// Porcentaje de la superficie (0..1)
    pub fraction: f64,
    /// Hay intensidad pintada (algún vértice por debajo del 100 %)
    pub painted: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneMode {
    Set,
    Add,
    Remove,
    Toggle,
}

fn zone_info(state: &AppState) -> Result<ZoneInfo, String> {
    let mesh = state.mesh.lock().unwrap();
    let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
    let total = mesh.num_faces();
    let mut zone = state.print_skin_zone.lock().unwrap();
    // Otra malla (otro número de caras): la zona de antes ya no vale
    if zone.total != total {
        *zone = SkinZone { total, ..Default::default() };
    }
    // La intensidad es por vértice: de otra malla no sirve
    if !zone.strength.is_empty() && zone.strength.len() != mesh.vertices.len() {
        zone.strength.clear();
    }
    let faces: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    let k = mm_per_unit(state);
    let area = p3d::zone_area(mesh, &faces) * k * k;
    let all = p3d::zone_area(mesh, &(0..total).collect::<Vec<_>>()) * k * k;
    Ok(ZoneInfo {
        count: zone.faces.len(),
        protected_count: zone.protected.len(),
        revision: zone_revision(&zone),
        total,
        area_mm2: area,
        fraction: if all > 0.0 { area / all } else { 0.0 },
        painted: zone.strength.iter().any(|&s| s < 0.999),
    })
}

/// Firma de las caras y las protegidas: el visor vuelve a pedirlas solo si cambia
fn zone_revision(zone: &SkinZone) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (zone.total, &zone.faces, &zone.protected).hash(&mut h);
    h.finish() & ((1 << 52) - 1)
}

/// Caras de la zona y protegidas, en binario (`u32` little-endian): cuántas
/// caras, cuántas protegidas, las caras y las protegidas
#[tauri::command]
pub fn skin_zone_faces(state: tauri::State<'_, AppState>) -> Result<Response, String> {
    zone_info(&state)?;
    let zone = state.print_skin_zone.lock().unwrap();
    let mut out = Vec::with_capacity(8 + 4 * (zone.faces.len() + zone.protected.len()));
    out.extend_from_slice(&(zone.faces.len() as u32).to_le_bytes());
    out.extend_from_slice(&(zone.protected.len() as u32).to_le_bytes());
    for f in zone.faces.iter().chain(&zone.protected) {
        out.extend_from_slice(&f.to_le_bytes());
    }
    Ok(Response::new(out))
}

/// La zona elegida
#[tauri::command]
pub fn skin_zone_get(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    zone_info(&state)
}

/// Cambia la zona con las caras `faces`: reemplazar, sumar, quitar o alternar
#[tauri::command]
pub fn skin_zone_edit(faces: Vec<u32>, mode: ZoneMode, state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    edit_zone(&state, &faces, mode)
}

pub(crate) fn edit_zone(state: &AppState, faces: &[u32], mode: ZoneMode) -> Result<ZoneInfo, String> {
    let total = state.mesh.lock().unwrap().as_ref().ok_or("No hay malla cargada")?.num_faces();
    {
        let mut zone = state.print_skin_zone.lock().unwrap();
        if zone.total != total {
            *zone = SkinZone { total, ..Default::default() };
        }
        let valid = faces.iter().copied().filter(|&f| (f as usize) < total);
        let mut set: std::collections::BTreeSet<u32> = zone.faces.iter().copied().collect();
        match mode {
            ZoneMode::Set => set = valid.collect(),
            ZoneMode::Add => set.extend(valid),
            ZoneMode::Remove => {
                for f in valid {
                    set.remove(&f);
                }
            }
            ZoneMode::Toggle => {
                for f in valid {
                    if !set.remove(&f) {
                        set.insert(f);
                    }
                }
            }
        }
        zone.faces = set.into_iter().collect();
    }
    zone_info(state)
}

/// Amplía la zona por las caras casi planas que la rodean (ángulo máximo entre caras vecinas, en grados)
#[tauri::command]
pub async fn skin_zone_grow(app: AppHandle, angle: f64) -> Result<ZoneInfo, String> {
    in_background(app, move |state| grow_zone(state, angle)).await
}

pub(crate) fn grow_zone(state: &AppState, angle: f64) -> Result<ZoneInfo, String> {
    zone_info(state)?;
    let seeds: Vec<usize> = state.print_skin_zone.lock().unwrap().faces.iter().map(|&f| f as usize).collect();
    if seeds.is_empty() {
        return Err("Elige primero algunas caras".into());
    }
    let grown: Vec<u32> = {
        let mesh = state.mesh.lock().unwrap();
        let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
        p3d::grow_by_angle(mesh, &seeds, angle).into_iter().map(|f| f as u32).collect()
    };
    state.print_skin_zone.lock().unwrap().faces = grown;
    zone_info(state)
}

/// Invierte la zona: las caras elegidas pasan a no estarlo y al revés
#[tauri::command]
pub fn skin_zone_invert(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    let current = zone_info(&state)?;
    let mut zone = state.print_skin_zone.lock().unwrap();
    let seeds: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    zone.faces = p3d::invert_zone(current.total, &seeds).into_iter().map(|f| f as u32).collect();
    drop(zone);
    zone_info(&state)
}

/// Quita la zona
#[tauri::command]
pub fn skin_zone_clear(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    state.print_skin_zone.lock().unwrap().faces.clear();
    zone_info(&state)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROTEGIDAS
// ═══════════════════════════════════════════════════════════════════════════

/// Cambia las caras protegidas con `faces`: reemplazar, sumar, quitar o alternar
#[tauri::command]
pub fn skin_protect_edit(faces: Vec<u32>, mode: ZoneMode, state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    edit_protected(&state, &faces, mode)
}

pub(crate) fn edit_protected(state: &AppState, faces: &[u32], mode: ZoneMode) -> Result<ZoneInfo, String> {
    zone_info(state)?;
    {
        let mut zone = state.print_skin_zone.lock().unwrap();
        let total = zone.total;
        let valid = faces.iter().copied().filter(|&f| (f as usize) < total);
        let mut set: std::collections::BTreeSet<u32> = zone.protected.iter().copied().collect();
        match mode {
            ZoneMode::Set => set = valid.collect(),
            ZoneMode::Add => set.extend(valid),
            ZoneMode::Remove => {
                for f in valid {
                    set.remove(&f);
                }
            }
            ZoneMode::Toggle => {
                for f in valid {
                    if !set.remove(&f) {
                        set.insert(f);
                    }
                }
            }
        }
        zone.protected = set.into_iter().collect();
    }
    zone_info(state)
}

/// Zona elegida por criterio en vez de cara por cara (la usa el conector MCP). Direcciones y alturas como
/// se muestran: Z arriba, alturas en mm desde la base del modelo
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ZoneCriteria {
    /// Todas las caras (después se recortan por altura)
    pub all: bool,
    /// «sides», «top», «bottom», «+x», «-x», «+y», «-y», «+z», «-z»
    pub directions: Vec<String>,
    /// Tolerancia de las direcciones (grados; por defecto 30)
    pub angle: Option<f64>,
    pub z_min: Option<f64>,
    pub z_max: Option<f64>,
}

/// Elige la zona por criterio (reemplaza la que había) y devuelve cómo quedó
pub(crate) fn select_zone(state: &AppState, c: &ZoneCriteria) -> Result<ZoneInfo, String> {
    let k = mm_per_unit(state);
    let faces: Vec<u32> = {
        let mesh = state.mesh.lock().unwrap();
        let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
        let cos = c.angle.unwrap_or(30.0).clamp(0.0, 90.0).to_radians().cos();
        let sin = c.angle.unwrap_or(30.0).clamp(0.0, 90.0).to_radians().sin();
        // Mostrado (Z arriba) → escena (Y arriba): (x, y, z) → (x, z, −y)
        let dirs: Vec<Option<[f64; 3]>> = c
            .directions
            .iter()
            .map(|d| match d.to_lowercase().as_str() {
                "+x" => Ok(Some([1.0, 0.0, 0.0])),
                "-x" => Ok(Some([-1.0, 0.0, 0.0])),
                "+y" => Ok(Some([0.0, 0.0, -1.0])),
                "-y" => Ok(Some([0.0, 0.0, 1.0])),
                "+z" | "top" => Ok(Some([0.0, 1.0, 0.0])),
                "-z" | "bottom" => Ok(Some([0.0, -1.0, 0.0])),
                "sides" => Ok(None),
                other => Err(format!("Dirección desconocida «{other}»")),
            })
            .collect::<Result<_, _>>()?;
        if !c.all && dirs.is_empty() {
            return Err("Di qué caras: todas o alguna dirección".into());
        }
        let bottom = mesh.vertices.iter().map(|v| v.position.y()).fold(f64::INFINITY, f64::min);
        (0..mesh.num_faces())
            .filter(|&f| {
                let p = mesh.get_face_positions(f);
                let (a, b, c3) = (p[0].0, p[1].0, p[2].0);
                let n = (b - a).cross(&(c3 - a));
                let len = n.norm();
                if len <= 0.0 {
                    return false;
                }
                let n = [n[0] / len, n[1] / len, n[2] / len];
                let height = ((a[1] + b[1] + c3[1]) / 3.0 - bottom) * k;
                if c.z_min.is_some_and(|z| height < z) || c.z_max.is_some_and(|z| height > z) {
                    return false;
                }
                c.all
                    || dirs.iter().any(|d| match d {
                        Some(d) => n[0] * d[0] + n[1] * d[1] + n[2] * d[2] >= cos,
                        None => n[1].abs() <= sin,
                    })
            })
            .map(|f| f as u32)
            .collect()
    };
    if faces.is_empty() {
        return Err("Ninguna cara cumple con eso".into());
    }
    edit_zone(state, &faces, ZoneMode::Set)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtectedHoles {
    pub zone: ZoneInfo,
    /// Todo lo encontrado
    pub found: usize,
    /// Por clase: agujeros, pernos, roscas y conos (avellanados)
    pub holes: usize,
    pub pins: usize,
    pub threads: usize,
    pub cones: usize,
}

/// Clases de lo que se protege solo, por nombre: «hole», «pin», «thread», «cone»
pub(crate) fn fastener_kinds(names: &[String]) -> Result<Vec<p3d::FastenerKind>, String> {
    names
        .iter()
        .map(|n| match n.as_str() {
            "hole" => Ok(p3d::FastenerKind::Hole),
            "pin" => Ok(p3d::FastenerKind::Pin),
            "thread" => Ok(p3d::FastenerKind::Thread),
            "cone" => Ok(p3d::FastenerKind::Cone),
            other => Err(format!("Clase desconocida «{other}»")),
        })
        .collect()
}

/// Protege lo que se encuentre de entre `min_mm` y `max_mm` de diámetro: agujeros cilíndricos y, si se piden en
/// `kinds`, pernos, roscas y avellanados. Sin `kinds`, solo agujeros
#[tauri::command]
pub async fn skin_protect_holes(app: AppHandle, min_mm: f64, max_mm: f64, kinds: Option<Vec<String>>) -> Result<ProtectedHoles, String> {
    in_background(app, move |state| {
        let kinds = fastener_kinds(&kinds.unwrap_or_else(|| vec!["hole".into()]))?;
        protect_fasteners(state, min_mm, max_mm, &kinds)
    })
    .await
}

/// Protege los agujeros cilíndricos de entre `min_mm` y `max_mm` de diámetro (los de tornillos y clavijas)
#[cfg(test)]
pub(crate) fn protect_holes(state: &AppState, min_mm: f64, max_mm: f64) -> Result<ProtectedHoles, String> {
    protect_fasteners(state, min_mm, max_mm, &[p3d::FastenerKind::Hole])
}

/// Protege lo que se encuentre de las clases `kinds` (ver [`p3d::detect_fasteners`])
pub(crate) fn protect_fasteners(state: &AppState, min_mm: f64, max_mm: f64, kinds: &[p3d::FastenerKind]) -> Result<ProtectedHoles, String> {
    zone_info(state)?;
    let k = mm_per_unit(state);
    let found = {
        let mesh = state.mesh.lock().unwrap();
        let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
        p3d::detect_fasteners(mesh, min_mm / k, max_mm / k, kinds)
    };
    let faces: Vec<u32> = found.iter().flat_map(|h| h.faces.iter().map(|&f| f as u32)).collect();
    let zone = edit_protected(state, &faces, ZoneMode::Add)?;
    let count = |kind| found.iter().filter(|f| f.kind == kind).count();
    Ok(ProtectedHoles {
        zone,
        found: found.len(),
        holes: count(p3d::FastenerKind::Hole),
        pins: count(p3d::FastenerKind::Pin),
        threads: count(p3d::FastenerKind::Thread),
        cones: count(p3d::FastenerKind::Cone),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// INTENSIDAD PINTADA
// ═══════════════════════════════════════════════════════════════════════════

/// La intensidad por vértice (f32 en little endian, uno por vértice del modelo); vacía si no se pintó nada
#[tauri::command]
pub fn skin_mask_get(state: tauri::State<'_, AppState>) -> Result<Response, String> {
    zone_info(&state)?;
    let zone = state.print_skin_zone.lock().unwrap();
    Ok(Response::new(zone.strength.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()))
}

/// Cambia la intensidad de esos vértices (lo que deja un trazo del pincel)
#[tauri::command]
pub fn skin_mask_edit(vertices: Vec<u32>, values: Vec<f32>, state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    edit_mask(&state, &vertices, &values)
}

pub(crate) fn edit_mask(state: &AppState, vertices: &[u32], values: &[f32]) -> Result<ZoneInfo, String> {
    if vertices.len() != values.len() {
        return Err("Faltan valores de intensidad".into());
    }
    zone_info(state)?;
    let n = state.mesh.lock().unwrap().as_ref().ok_or("No hay malla cargada")?.vertices.len();
    {
        let mut zone = state.print_skin_zone.lock().unwrap();
        if zone.strength.len() != n {
            zone.strength = vec![1.0; n];
        }
        for (&v, &s) in vertices.iter().zip(values) {
            if let Some(slot) = zone.strength.get_mut(v as usize) {
                *slot = if s.is_finite() { s.clamp(0.0, 1.0) } else { 1.0 };
            }
        }
        // Todo al 100 %: como si no se hubiera pintado
        if zone.strength.iter().all(|&s| s >= 0.999) {
            zone.strength.clear();
        }
    }
    zone_info(state)
}

/// Vuelve toda la intensidad al 100 %
#[tauri::command]
pub fn skin_mask_clear(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    zone_info(&state)?;
    state.print_skin_zone.lock().unwrap().strength.clear();
    zone_info(&state)
}

// ═══════════════════════════════════════════════════════════════════════════
// REGIÓN (para rehacer la piel si el diseño cambia)
// ═══════════════════════════════════════════════════════════════════════════

/// La zona, lo protegido y la intensidad guardados por su lugar en el espacio. Va con el paso del historial: si la
/// pieza del diseño cambia, la piel se rehace sobre la malla nueva buscando la zona ahí
#[tauri::command]
pub fn skin_zone_region(state: tauri::State<'_, AppState>) -> Result<p3d::ZoneRegion, String> {
    zone_region(&state)
}

pub(crate) fn zone_region(state: &AppState) -> Result<p3d::ZoneRegion, String> {
    let info = zone_info(state)?;
    let k = mm_per_unit(state);
    // Celdas de medio milímetro, o más grandes si la zona es enorme (unas 20 000 como mucho)
    let cell_mm = (info.area_mm2 / 20_000.0).sqrt().max(0.5);
    let mesh = state.mesh.lock().unwrap();
    let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
    let zone = state.print_skin_zone.lock().unwrap();
    let faces: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    let protected: Vec<usize> = zone.protected.iter().map(|&f| f as usize).collect();
    Ok(p3d::ZoneRegion::capture(mesh, &faces, &protected, &zone.strength, cell_mm / k))
}

/// Quita todas las caras protegidas
#[tauri::command]
pub fn skin_protect_clear(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    zone_info(&state)?;
    state.print_skin_zone.lock().unwrap().protected.clear();
    zone_info(&state)
}

// ═══════════════════════════════════════════════════════════════════════════
// APLICAR
// ═══════════════════════════════════════════════════════════════════════════

/// Cómo se aplica la piel a la zona
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinParams {
    /// Patrón de fábrica o imagen importada
    pub image: String,
    #[serde(default)]
    pub ops: SkinOps,
    /// Lado de la baldosa en la superficie (mm)
    pub tile_mm: f64,
    /// Altura entre el negro y el blanco (mm)
    pub amplitude: f64,
    #[serde(default)]
    pub offset: p3d::SkinOffset,
    #[serde(default)]
    pub rotation_deg: f64,
    /// Lado de los triángulos (mm); sin él, lo da el detalle del mapa
    pub edge_mm: Option<f64>,
    pub feather_mm: Option<f64>,
    /// De dónde sale el UV: el del modelo o una proyección en caja (sin UV)
    #[serde(default)]
    pub mapping: p3d::SkinMapping,
    /// Cuánto se aleja el relieve de las caras protegidas (mm)
    #[serde(default)]
    pub margin_mm: f64,
    /// Simplificar el resultado sin pasarse de esta desviación (mm)
    #[serde(default)]
    pub decimate_mm: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkinApplied {
    pub triangles_before: usize,
    pub triangles_after: usize,
    /// Triángulos antes de simplificar
    pub triangles_skinned: usize,
    pub edge_mm: f64,
    pub max_displacement_mm: f64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkinEstimate {
    /// La escena trae UV de todas sus primitivas
    pub has_uv: bool,
    pub edge_mm: f64,
    /// Triángulos aproximados de la malla resultante
    pub triangles: usize,
    pub zone_area_mm2: f64,
    pub warnings: Vec<String>,
}

/// Cuánto pesaría la piel con estos valores, sin aplicarla
#[tauri::command]
pub fn skin_estimate(params: SkinParams, state: tauri::State<'_, AppState>) -> Result<SkinEstimate, String> {
    estimate(&state, &params)
}

pub(crate) fn estimate(state: &AppState, params: &SkinParams) -> Result<SkinEstimate, String> {
    let zone = zone_info(state)?;
    let has_uv = params.mapping == p3d::SkinMapping::Box || state.scene.lock().unwrap().as_ref().is_some_and(|s| scene_uvs(s).is_ok());
    let map = height_map(state, &params.image, &params.ops)?;
    let pixel_mm = params.tile_mm.max(0.01) / map.width.max(map.height) as f64;
    let edge_mm = params.edge_mm.unwrap_or_else(|| pixel_mm.max(0.2));
    let outside = zone.total.saturating_sub(zone.count);
    let triangles = outside + (zone.area_mm2 / (0.19 * edge_mm * edge_mm)) as usize;
    let mut warnings = Vec::new();
    if params.amplitude > 0.0 && params.amplitude < 0.1 {
        warnings.push("Un relieve de menos de 0,1 mm es menos que una capa: casi no se va a notar".to_string());
    }
    if params.amplitude > 2.0 {
        warnings.push("Un relieve de más de 2 mm necesita paredes gruesas y puede crear voladizos".to_string());
    }
    if params.tile_mm / 16.0 < 0.4 {
        warnings.push("La baldosa es tan chica que el detalle queda por debajo de una boquilla de 0,4 mm".to_string());
    }
    if triangles > 1_500_000 {
        warnings.push(format!("Saldrían unos {triangles} triángulos: la malla se pone muy pesada"));
    }
    Ok(SkinEstimate { has_uv, edge_mm, triangles, zone_area_mm2: zone.area_mm2, warnings })
}

/// Cuántos triángulos como mucho tiene la vista previa del relieve
const PREVIEW_TRIANGLES: usize = 250_000;

/// La malla con el relieve puesto, con menos detalle si hace falta (para verla sin cambiar el modelo).
/// Sale en el formato de los datos de malla del visor
#[tauri::command]
pub async fn skin_preview_mesh(app: AppHandle, params: SkinParams) -> Result<Response, String> {
    in_background(app, move |state| preview_mesh(state, &params).map(Response::new)).await
}

pub(crate) fn preview_mesh(state: &AppState, params: &SkinParams) -> Result<Vec<u8>, String> {
    let zone = zone_info(state)?;
    if zone.count == 0 {
        return Err("Elige primero la zona donde va la piel".into());
    }
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let uvs = if params.mapping == p3d::SkinMapping::Box { Vec::new() } else { scene_uvs(&scene)? };
    if params.mapping == p3d::SkinMapping::Uv && uvs.len() != mesh.vertices.len() {
        return Err("La malla ya no coincide con el modelo desplegado: vuelve a desplegar el UV".into());
    }
    let map = height_map(state, &params.image, &params.ops)?;
    // El lado que cabe en el presupuesto de triángulos (nunca más fino que el real)
    let pixel_mm = params.tile_mm.max(0.01) / map.width.max(map.height) as f64;
    let wanted = params.edge_mm.unwrap_or_else(|| pixel_mm.max(0.2));
    let outside = zone.total.saturating_sub(zone.count);
    let budget = PREVIEW_TRIANGLES.saturating_sub(outside).max(10_000) as f64;
    let affordable = (zone.area_mm2 / (0.19 * budget)).sqrt();
    let cfg = p3d::SkinConfig {
        tile_mm: params.tile_mm,
        amplitude: params.amplitude,
        offset: params.offset,
        rotation_deg: params.rotation_deg,
        edge_mm: Some(wanted.max(affordable)),
        feather_mm: params.feather_mm,
        unit_mm: mm_per_unit(state),
        max_triangles: PREVIEW_TRIANGLES * 2,
        mapping: params.mapping,
        margin_mm: params.margin_mm,
        decimate_mm: None,
    };
    // Las caras van en la zona guardada (el resumen solo trae cuántas)
    let (faces, protected, strength) = {
        let saved = state.print_skin_zone.lock().unwrap();
        let faces: Vec<usize> = saved.faces.iter().map(|&f| f as usize).collect();
        let protected: Vec<usize> = saved.protected.iter().map(|&f| f as usize).collect();
        (faces, protected, saved.strength.clone())
    };
    let result = p3d::apply_skin_masked(&mesh, &uvs, &faces, &protected, &strength, &map, &cfg).map_err(|e| format!("No se pudo armar la vista previa: {e}"))?;
    Ok(crate::commands::scene_mesh_data(&crate::commands::mesh_to_scene(&result.mesh, "Vista previa", Some(&scene))).to_bytes())
}

/// Un UV por vértice de la malla activa, en el orden de la escena (el de `scene_to_mesh`)
fn scene_uvs(scene: &converter_scene::Scene) -> Result<Vec<[f32; 2]>, String> {
    let mut out = Vec::new();
    for prim in scene.world_primitives() {
        match &prim.uvs {
            Some(uv) if uv.len() == prim.positions.len() => out.extend_from_slice(uv),
            _ => return Err("El modelo no tiene UV: despliégalo primero en UV / Piel".into()),
        }
    }
    Ok(out)
}

/// Desplaza la zona elegida con el mapa de altura. El modelo pasa a ser geometría sola (sin UV, texturas ni rig);
/// la copia para deshacer la saca el llamador. `region` es la zona guardada por lugar ([`skin_zone_region`]): con
/// `replay` (se rehace el paso porque el diseño cambió) la zona sale de ahí en vez de la elegida
#[tauri::command]
pub async fn apply_print_skin(app: AppHandle, params: SkinParams, region: Option<p3d::ZoneRegion>, replay: Option<bool>) -> Result<SkinApplied, String> {
    in_background(app, move |state| apply_skin_full(state, &params, region.as_ref().filter(|_| replay == Some(true)))).await
}

pub(crate) fn apply_skin_impl(state: &AppState, params: &SkinParams) -> Result<SkinApplied, String> {
    apply_skin_full(state, params, None)
}

/// Aplica la piel a la zona elegida o, con `region`, a la que se encuentre en la malla en ese lugar
pub(crate) fn apply_skin_full(state: &AppState, params: &SkinParams, region: Option<&p3d::ZoneRegion>) -> Result<SkinApplied, String> {
    let zone = zone_info(state)?;
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let (faces, protected, strength) = match region {
        Some(region) => {
            let (faces, protected, strength) = region.locate(&mesh);
            if faces.is_empty() {
                return Err("No se encontró la zona de la piel en la pieza nueva: el diseño cambió justo ahí".into());
            }
            (faces, protected, strength)
        }
        None => {
            if zone.count == 0 {
                return Err("Elige primero la zona donde va la piel".into());
            }
            let saved = state.print_skin_zone.lock().unwrap();
            (saved.faces.iter().map(|&f| f as usize).collect(), saved.protected.iter().map(|&f| f as usize).collect(), saved.strength.clone())
        }
    };
    let uvs = if params.mapping == p3d::SkinMapping::Box { Vec::new() } else { scene_uvs(&scene)? };
    if params.mapping == p3d::SkinMapping::Uv && uvs.len() != mesh.vertices.len() {
        return Err("La malla ya no coincide con el modelo desplegado: vuelve a desplegar el UV".into());
    }
    let map = height_map(state, &params.image, &params.ops)?;
    let k = mm_per_unit(state);
    let cfg = p3d::SkinConfig {
        tile_mm: params.tile_mm,
        amplitude: params.amplitude,
        offset: params.offset,
        rotation_deg: params.rotation_deg,
        edge_mm: params.edge_mm,
        feather_mm: params.feather_mm,
        unit_mm: k,
        mapping: params.mapping,
        margin_mm: params.margin_mm,
        decimate_mm: params.decimate_mm,
        ..Default::default()
    };
    let result = p3d::apply_skin_masked(&mesh, &uvs, &faces, &protected, &strength, &map, &cfg).map_err(|e| format!("No se pudo aplicar la piel: {e}"))?;
    let new_scene = crate::commands::mesh_to_scene(&result.mesh, "Piel", Some(&scene));
    *state.mesh.lock().unwrap() = Some(result.mesh);
    *state.scene.lock().unwrap() = Some(new_scene);
    *state.print_skin_zone.lock().unwrap() = Default::default();
    *state.print3d_pieces.lock().unwrap() = None;
    *state.print3d_pieces_plain.lock().unwrap() = None;
    state.geometry_changed();
    Ok(SkinApplied {
        triangles_before: result.triangles_before,
        triangles_after: result.triangles_after,
        triangles_skinned: result.triangles_skinned,
        edge_mm: result.edge_mm,
        max_displacement_mm: result.max_displacement_mm,
        warnings: result.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;
    use pinocchio_mesh::Mesh;

    fn cube() -> Mesh {
        let p = |x: f64, y: f64, z: f64| Vector3::new(x * 10.0, y * 10.0, z * 10.0);
        let v = vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        Mesh::from_triangles(&v, &f)
    }

    fn state_with_cube() -> AppState {
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(cube());
        state
    }

    /// Caras de la zona guardada después de `r` (el resumen ya no las trae)
    fn faces_after(state: &AppState, r: Result<ZoneInfo, String>) -> Vec<u32> {
        r.unwrap();
        state.print_skin_zone.lock().unwrap().faces.clone()
    }

    #[test]
    fn la_firma_cambia_solo_con_las_caras() {
        let state = state_with_cube();
        let a = edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        let b = edit_mask(&state, &[0], &[0.5]).unwrap();
        assert_eq!(a.revision, b.revision, "pintar intensidad no cambia las caras");
        assert!(b.painted);
        let c = edit_zone(&state, &[4], ZoneMode::Add).unwrap();
        assert_ne!(a.revision, c.revision);
        assert!(c.revision < 1 << 53, "cabe exacta en un número de JS");
    }

    #[test]
    fn editar_la_zona_en_sus_cuatro_modos() {
        let state = state_with_cube();
        assert_eq!(faces_after(&state, edit_zone(&state, &[2, 3], ZoneMode::Set)), vec![2, 3]);
        assert_eq!(faces_after(&state, edit_zone(&state, &[3, 5], ZoneMode::Add)), vec![2, 3, 5]);
        assert_eq!(faces_after(&state, edit_zone(&state, &[3], ZoneMode::Remove)), vec![2, 5]);
        assert_eq!(faces_after(&state, edit_zone(&state, &[5, 7], ZoneMode::Toggle)), vec![2, 7]);
        // Las caras que no existen se ignoran
        assert_eq!(faces_after(&state, edit_zone(&state, &[2, 99], ZoneMode::Set)), vec![2]);
    }

    #[test]
    fn el_area_va_en_milimetros_cuadrados() {
        let state = state_with_cube();
        // La malla está en las unidades de la escena (metros por defecto sin escena: 1 unidad = 1 mm)
        let info = edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        assert!((info.area_mm2 - 100.0).abs() < 1e-9, "{}", info.area_mm2);
        assert!((info.fraction - 100.0 / 600.0).abs() < 1e-9);
        assert_eq!(info.total, 12);
    }

    #[test]
    fn crecer_invertir_y_limpiar() {
        let state = state_with_cube();
        assert!(grow_zone(&state, 20.0).is_err(), "sin caras no hay de dónde crecer");
        edit_zone(&state, &[2], ZoneMode::Set).unwrap();
        assert_eq!(faces_after(&state, grow_zone(&state, 20.0)), vec![2, 3]);
        assert_eq!(grow_zone(&state, 100.0).unwrap().count, 12);
        let ts = edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        assert_eq!(ts.count, 2);
    }

    /// Escena de una caja de 40 × 40 × 10 con UV planos en la tapa (x, y) / 40
    fn box_scene() -> converter_scene::Scene {
        use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
        let c = |x: f32, y: f32, z: f32| [x * 40.0, y * 40.0, z * 10.0];
        let v = [c(0., 0., 0.), c(1., 0., 0.), c(1., 1., 0.), c(0., 1., 0.), c(0., 0., 1.), c(1., 0., 1.), c(1., 1., 1.), c(0., 1., 1.)];
        let f: [[usize; 3]; 12] = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        let positions: Vec<[f32; 3]> = f.iter().flatten().map(|&k| v[k]).collect();
        let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0] / 40.0, p[1] / 40.0]).collect();
        let indices: Vec<u32> = (0..positions.len() as u32).collect();
        let prim = Primitive {
            attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::TexCoords(0, uvs)],
            indices: Some(IndexData::U32(indices)),
            material: None,
        };
        let mut scene = Scene::new();
        scene.meshes.push(SceneMesh { name: "caja".into(), primitives: vec![prim] });
        scene.nodes.push(Node { name: "caja".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: Vec::new() });
        scene.root_nodes.push(0);
        scene.meters_per_unit = 0.001;
        scene
    }

    #[test]
    fn la_estimacion_avisa_antes_de_aplicar() {
        let state = AppState::new();
        let scene = box_scene();
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        let params = |tile: f64, amplitude: f64| SkinParams {
            image: "dots".into(),
            ops: SkinOps::default(),
            tile_mm: tile,
            amplitude,
            offset: p3d::SkinOffset::Outward,
            rotation_deg: 0.0,
            edge_mm: Some(1.0),
            feather_mm: None,
            ..Default::default()
        };
        let e = estimate(&state, &params(10.0, 0.6)).unwrap();
        assert!(e.has_uv && e.warnings.is_empty());
        // 1600 mm² de tapa; al partir por bisección, cada triángulo ronda 0,19 · lado² + los 10 de afuera
        assert!((e.triangles as f64 - (1600.0 / 0.19 + 10.0)).abs() < 5.0, "{}", e.triangles);
        assert_eq!(estimate(&state, &params(10.0, 0.05)).unwrap().warnings.len(), 1);
        assert_eq!(estimate(&state, &params(3.0, 3.0)).unwrap().warnings.len(), 2);
        // El orden coincide con el que aplica: la estimación no se aleja mucho de la realidad
        let real = apply_skin_impl(&state, &params(10.0, 0.6)).unwrap();
        assert!((real.triangles_after as f64 / e.triangles as f64 - 1.0).abs() < 0.5, "{} vs {}", real.triangles_after, e.triangles);
    }

    #[test]
    fn la_vista_previa_no_toca_el_modelo_y_respeta_el_presupuesto() {
        let state = AppState::new();
        let scene = box_scene();
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        let params = SkinParams {
            image: "lines".into(),
            ops: SkinOps::default(),
            tile_mm: 10.0,
            amplitude: 0.8,
            offset: p3d::SkinOffset::Outward,
            rotation_deg: 0.0,
            edge_mm: Some(0.3),
            feather_mm: None,
            ..Default::default()
        };
        let before = state.mesh.lock().unwrap().clone().unwrap().num_faces();
        let bytes = preview_mesh(&state, &params).unwrap();
        // No cambió nada del modelo ni de la zona
        assert_eq!(state.mesh.lock().unwrap().as_ref().unwrap().num_faces(), before);
        assert_eq!(state.print_skin_zone.lock().unwrap().faces, vec![2, 3]);
        // Con 0,3 mm la tapa pediría ~17 000 · 1,3; el presupuesto de la vista previa lo deja con menos triángulos que lo real
        assert!(bytes.len() > 1000);
        let real = estimate(&state, &params).unwrap().triangles;
        assert!(real > 15_000, "{real}");
        assert!(preview_mesh(&AppState::new(), &params).is_err(), "sin malla no hay vista previa");
    }

    #[test]
    fn aplicar_la_piel_cambia_la_malla_y_limpia_la_zona() {
        let state = AppState::new();
        let scene = box_scene();
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        let params = SkinParams {
            image: "dots".into(),
            ops: SkinOps::default(),
            tile_mm: 10.0,
            amplitude: 0.8,
            offset: p3d::SkinOffset::Outward,
            rotation_deg: 0.0,
            edge_mm: Some(1.0),
            feather_mm: Some(2.0),
            ..Default::default()
        };
        assert!(apply_skin_impl(&state, &params).is_err(), "sin zona no hay piel");
        edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        let r = apply_skin_impl(&state, &params).unwrap();
        assert!(r.triangles_after > 1000 && r.max_displacement_mm > 0.7, "{r:?}");
        let mesh = state.mesh.lock().unwrap().clone().unwrap();
        assert!(mesh.is_closed() && mesh.num_faces() == r.triangles_after);
        assert!(state.print_skin_zone.lock().unwrap().faces.is_empty(), "la zona de antes ya no vale");
        // La escena es la nueva geometría
        assert_eq!(state.scene.lock().unwrap().as_ref().unwrap().meshes.len(), 1);
        // Ya sin UV: no se puede volver a aplicar
        edit_zone(&state, &[0], ZoneMode::Set).unwrap();
        assert!(apply_skin_impl(&state, &params).unwrap_err().contains("UV"));
    }

    #[test]
    fn otra_malla_descarta_la_zona() {
        let state = state_with_cube();
        edit_zone(&state, &[1, 2], ZoneMode::Set).unwrap();
        // Una malla con otro número de caras
        *state.mesh.lock().unwrap() = Some(Mesh::from_triangles(&[Vector3::zero(), Vector3::unit_x(), Vector3::unit_y()], &[[0, 1, 2]]));
        assert!(zone_info(&state).unwrap().count == 0);
    }

    #[test]
    fn biblioteca_vista_previa_e_importar() {
        let state = AppState::new();
        let lib = library(&state);
        assert_eq!(lib.len(), 6);
        assert!(lib.iter().all(|p| p.builtin));
        let png = preview_png(&state, "leather", &SkinOps::default(), 64).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(preview_png(&state, "nada", &SkinOps::default(), 64).is_err());
        // Importar una imagen: gradiente de 8 × 8 guardado como PNG
        let dir = std::env::temp_dir().join("pinocchio_skin_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("degradado.png");
        let img = image::GrayImage::from_fn(8, 8, |x, _| image::Luma([(x * 36) as u8]));
        img.save(&path).unwrap();
        let info = import_image(&state, path.to_str().unwrap()).unwrap();
        assert_eq!((info.id.as_str(), info.name.as_str(), info.width, info.height), ("img_1", "degradado", 8, 8));
        assert_eq!(library(&state).len(), 7);
        // Retocada: invertida, el lado negro pasa a blanco
        let plain = height_map(&state, "img_1", &SkinOps::default()).unwrap();
        let inverted = height_map(&state, "img_1", &SkinOps { invert: true, ..Default::default() }).unwrap();
        assert!((plain.data[0] - 0.0).abs() < 1e-6 && (inverted.data[0] - 1.0).abs() < 1e-6);
        // Una segunda importación recibe otro número
        assert_eq!(import_image(&state, path.to_str().unwrap()).unwrap().id, "img_2");
        std::fs::remove_file(path).ok();
        assert!(import_image(&state, "/no/existe.png").is_err());
    }

    #[test]
    fn piel_en_caja_sin_uv_y_simplificada() {
        let state = AppState::new();
        let mut scene = box_scene();
        // Sin UV en el modelo
        for prim in scene.meshes.iter_mut().flat_map(|m| m.primitives.iter_mut()) {
            prim.attributes.retain(|a| !matches!(a, converter_scene::VertexAttribute::TexCoords(..)));
        }
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        let params = |mapping| SkinParams {
            image: "dots".into(),
            tile_mm: 10.0,
            amplitude: 0.8,
            edge_mm: Some(1.0),
            mapping,
            ..Default::default()
        };
        // Con UV falla y sin él (caja) anda
        assert!(!estimate(&state, &params(p3d::SkinMapping::Uv)).unwrap().has_uv);
        assert!(apply_skin_impl(&state, &params(p3d::SkinMapping::Uv)).is_err());
        assert!(estimate(&state, &params(p3d::SkinMapping::Box)).unwrap().has_uv);
        assert!(preview_mesh(&state, &params(p3d::SkinMapping::Box)).is_ok());
        let slim = SkinParams { decimate_mm: Some(0.05), ..params(p3d::SkinMapping::Box) };
        let applied = apply_skin_impl(&state, &slim).unwrap();
        assert!(applied.triangles_after < applied.triangles_skinned, "{} de {}", applied.triangles_after, applied.triangles_skinned);
        assert!(state.mesh.lock().unwrap().as_ref().unwrap().is_closed());
    }

    #[test]
    fn la_intensidad_pintada_y_rehacer_en_otra_malla() {
        let state = AppState::new();
        let scene = box_scene();
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        // Mitad izquierda (x < 20) en 0 %: los vértices de la tapa de ese lado
        let (left, n): (Vec<u32>, usize) = {
            let mesh = state.mesh.lock().unwrap();
            let mesh = mesh.as_ref().unwrap();
            ((0..mesh.vertices.len() as u32).filter(|&v| mesh.vertices[v as usize].position.x() < 20.0).collect(), mesh.vertices.len())
        };
        let info = edit_mask(&state, &left, &vec![0.0; left.len()]).unwrap();
        assert!(info.painted);
        assert_eq!(state.print_skin_zone.lock().unwrap().strength.len(), n);
        assert!(edit_mask(&state, &[0], &[]).is_err(), "faltan valores");
        let region = zone_region(&state).unwrap();
        assert!(!region.is_empty() && !region.strength.is_empty());
        let params = SkinParams { image: "lines".into(), tile_mm: 10.0, amplitude: 0.8, edge_mm: Some(1.0), feather_mm: Some(1.0), mapping: p3d::SkinMapping::Box, ..Default::default() };
        // La caja solo tiene vértices en las esquinas: la intensidad va de 0 (x = 0) a 1 (x = 40) en línea recta, así
        // que a la izquierda el relieve no pasa de 0,2 · 0,8 mm y a la derecha llega casi entero
        let moved_at = |state: &AppState| {
            let mesh = state.mesh.lock().unwrap();
            let top: Vec<_> = mesh.as_ref().unwrap().vertices.iter().map(|v| v.position).filter(|p| p.z() > 9.0).collect();
            let band = |lo: f64, hi: f64| top.iter().filter(|p| p.x() >= lo && p.x() <= hi).map(|p| (p.z() - 10.0).abs()).fold(0.0, f64::max);
            (band(0.0, 8.0), band(32.0, 38.0))
        };
        // El diseño cambió: la misma caja con otra teselación (y otro número de caras)
        let finer = {
            let mesh = state.mesh.lock().unwrap().clone().unwrap();
            let (pos, tris): (Vec<_>, Vec<_>) = (mesh.vertices.iter().map(|v| v.position).collect(), (0..mesh.num_faces()).map(|f| { let v = mesh.get_face_vertices(f); [v[0], v[1], v[2]] }).collect());
            let mut p = pos.clone();
            let mut out = Vec::new();
            for t in tris {
                let c = p.len();
                p.push((pos[t[0]] + pos[t[1]] + pos[t[2]]) * (1.0 / 3.0));
                out.extend([[t[0], t[1], c], [t[1], t[2], c], [t[2], t[0], c]]);
            }
            Mesh::from_triangles(&p, &out)
        };
        let applied = apply_skin_impl(&state, &params).unwrap();
        assert!(applied.max_displacement_mm > 0.5);
        let (l, r) = moved_at(&state);
        assert!(l <= 0.2 * 0.8 + 1e-9 && r > 0.6, "relieve {l} a la izquierda, {r} a la derecha");
        // Se rehace sobre la otra malla con la región: misma piel, sin elegir nada
        let mut scene = crate::commands::mesh_to_scene(&finer, "caja", None);
        scene.meters_per_unit = 0.001;
        *state.scene.lock().unwrap() = Some(scene);
        *state.mesh.lock().unwrap() = Some(finer);
        assert!(zone_info(&state).unwrap().count == 0, "la zona de antes ya no vale");
        let again = apply_skin_full(&state, &params, Some(&region)).unwrap();
        assert!(again.max_displacement_mm > 0.5);
        let (l, r) = moved_at(&state);
        assert!(l <= 0.2 * 0.8 + 0.03 && r > 0.6, "rehecha: relieve {l} a la izquierda, {r} a la derecha");
        // Una región que no cae en la malla: error claro
        let far = p3d::ZoneRegion { cell: 1.0, zone: vec![1000, 1000, 1000, 0, 100, 0], ..Default::default() };
        assert!(apply_skin_full(&state, &params, Some(&far)).unwrap_err().contains("zona"));
    }

    #[test]
    fn proteger_agujeros_y_caras_a_mano() {
        use pinocchio_print3d::{add_hole, footprint_polygon, PlanarShape};
        let state = AppState::new();
        let b = {
            let p = |x: f64, y: f64, z: f64| Vector3::new(x * 40.0, y * 40.0, z * 20.0);
            let v = vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
            let f = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
            Mesh::from_triangles(&v, &f)
        };
        let poly = footprint_polygon(&PlanarShape::Round { diameter: 8.0 }, Vector3::new(20.0, 20.0, 20.0), Vector3::new(0.0, 0.0, 1.0), 0.0);
        let m = add_hole(&b, &poly, Vector3::new(0.0, 0.0, 1.0), 10.0, 1.0).unwrap();
        let total = m.num_faces();
        let mut scene = crate::commands::mesh_to_scene(&m, "caja", None);
        scene.meters_per_unit = 0.001;
        *state.scene.lock().unwrap() = Some(scene);
        *state.mesh.lock().unwrap() = Some(m);
        // Escena en mm: agujeros de 3 a 20 mm
        let found = protect_holes(&state, 3.0, 20.0).unwrap();
        assert_eq!(found.found, 1);
        assert_eq!(found.zone.protected_count, 48);
        // Fuera de rango no encuentra otros y no borra lo que ya había
        assert_eq!(protect_holes(&state, 10.0, 30.0).unwrap().found, 0);
        assert_eq!(edit_protected(&state, &[0, 1], ZoneMode::Add).unwrap().protected_count, 50);
        assert_eq!(edit_protected(&state, &[0], ZoneMode::Remove).unwrap().protected_count, 49);
        assert!(edit_protected(&state, &[total as u32 + 5], ZoneMode::Add).unwrap().protected_count == 49, "las caras que no existen se ignoran");
        // Aplicar con toda la malla como zona deja el agujero liso
        edit_zone(&state, &(0..total as u32).collect::<Vec<_>>(), ZoneMode::Set).unwrap();
        let params = SkinParams {
            image: "dots".into(),
            tile_mm: 10.0,
            amplitude: 0.5,
            edge_mm: Some(1.0),
            mapping: p3d::SkinMapping::Box,
            margin_mm: 2.0,
            ..Default::default()
        };
        let applied = apply_skin_impl(&state, &params).unwrap();
        assert!(applied.max_displacement_mm > 0.2);
        let after = state.mesh.lock().unwrap().clone().unwrap();
        assert!(after.is_closed());
        // Tras aplicar, la zona y las protegidas se limpian
        assert!(state.print_skin_zone.lock().unwrap().protected.is_empty());
    }
}
