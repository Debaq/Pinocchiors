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

#[derive(Debug, Clone, Serialize)]
pub struct ZoneInfo {
    /// Caras de la zona, ordenadas
    pub faces: Vec<u32>,
    pub total: usize,
    /// Área de la zona en mm²
    pub area_mm2: f64,
    /// Porcentaje de la superficie (0..1)
    pub fraction: f64,
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
        *zone = SkinZone { total, faces: Vec::new() };
    }
    let faces: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    let k = mm_per_unit(state);
    let area = p3d::zone_area(mesh, &faces) * k * k;
    let all = p3d::zone_area(mesh, &(0..total).collect::<Vec<_>>()) * k * k;
    Ok(ZoneInfo { faces: zone.faces.clone(), total, area_mm2: area, fraction: if all > 0.0 { area / all } else { 0.0 } })
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
            *zone = SkinZone { total, faces: Vec::new() };
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
    let current = zone_info(state)?;
    if current.faces.is_empty() {
        return Err("Elige primero algunas caras".into());
    }
    let grown: Vec<u32> = {
        let mesh = state.mesh.lock().unwrap();
        let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
        let seeds: Vec<usize> = current.faces.iter().map(|&f| f as usize).collect();
        p3d::grow_by_angle(mesh, &seeds, angle).into_iter().map(|f| f as u32).collect()
    };
    state.print_skin_zone.lock().unwrap().faces = grown;
    zone_info(state)
}

/// Invierte la zona: las caras elegidas pasan a no estarlo y al revés
#[tauri::command]
pub fn skin_zone_invert(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    let current = zone_info(&state)?;
    let seeds: Vec<usize> = current.faces.iter().map(|&f| f as usize).collect();
    state.print_skin_zone.lock().unwrap().faces = p3d::invert_zone(current.total, &seeds).into_iter().map(|f| f as u32).collect();
    zone_info(&state)
}

/// Quita la zona
#[tauri::command]
pub fn skin_zone_clear(state: tauri::State<'_, AppState>) -> Result<ZoneInfo, String> {
    state.print_skin_zone.lock().unwrap().faces.clear();
    zone_info(&state)
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

    #[test]
    fn editar_la_zona_en_sus_cuatro_modos() {
        let state = state_with_cube();
        assert_eq!(edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap().faces, vec![2, 3]);
        assert_eq!(edit_zone(&state, &[3, 5], ZoneMode::Add).unwrap().faces, vec![2, 3, 5]);
        assert_eq!(edit_zone(&state, &[3], ZoneMode::Remove).unwrap().faces, vec![2, 5]);
        assert_eq!(edit_zone(&state, &[5, 7], ZoneMode::Toggle).unwrap().faces, vec![2, 7]);
        // Las caras que no existen se ignoran
        assert_eq!(edit_zone(&state, &[2, 99], ZoneMode::Set).unwrap().faces, vec![2]);
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
        assert_eq!(grow_zone(&state, 20.0).unwrap().faces, vec![2, 3]);
        assert_eq!(grow_zone(&state, 100.0).unwrap().faces.len(), 12);
        let ts = edit_zone(&state, &[2, 3], ZoneMode::Set).unwrap();
        assert_eq!(ts.faces.len(), 2);
    }

    #[test]
    fn otra_malla_descarta_la_zona() {
        let state = state_with_cube();
        edit_zone(&state, &[1, 2], ZoneMode::Set).unwrap();
        // Una malla con otro número de caras
        *state.mesh.lock().unwrap() = Some(Mesh::from_triangles(&[Vector3::zero(), Vector3::unit_x(), Vector3::unit_y()], &[[0, 1, 2]]));
        assert!(zone_info(&state).unwrap().faces.is_empty());
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
}
