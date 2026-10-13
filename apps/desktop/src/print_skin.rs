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

// ═══════════════════════════════════════════════════════════════════════════
// APLICAR
// ═══════════════════════════════════════════════════════════════════════════

/// Cómo se aplica la piel a la zona
#[derive(Debug, Clone, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize)]
pub struct SkinApplied {
    pub triangles_before: usize,
    pub triangles_after: usize,
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
    let has_uv = state.scene.lock().unwrap().as_ref().is_some_and(|s| scene_uvs(s).is_ok());
    let map = height_map(state, &params.image, &params.ops)?;
    let pixel_mm = params.tile_mm.max(0.01) / map.width.max(map.height) as f64;
    let edge_mm = params.edge_mm.unwrap_or_else(|| pixel_mm.max(0.2));
    let outside = zone.total.saturating_sub(zone.faces.len());
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
    if zone.faces.is_empty() {
        return Err("Elige primero la zona donde va la piel".into());
    }
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let uvs = scene_uvs(&scene)?;
    if uvs.len() != mesh.vertices.len() {
        return Err("La malla ya no coincide con el modelo desplegado: vuelve a desplegar el UV".into());
    }
    let map = height_map(state, &params.image, &params.ops)?;
    // El lado que cabe en el presupuesto de triángulos (nunca más fino que el real)
    let pixel_mm = params.tile_mm.max(0.01) / map.width.max(map.height) as f64;
    let wanted = params.edge_mm.unwrap_or_else(|| pixel_mm.max(0.2));
    let outside = zone.total.saturating_sub(zone.faces.len());
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
    };
    let faces: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    let result = p3d::apply_skin(&mesh, &uvs, &faces, &map, &cfg).map_err(|e| format!("No se pudo armar la vista previa: {e}"))?;
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
/// la copia para deshacer la saca el llamador
#[tauri::command]
pub async fn apply_print_skin(app: AppHandle, params: SkinParams) -> Result<SkinApplied, String> {
    in_background(app, move |state| apply_skin_impl(state, &params)).await
}

pub(crate) fn apply_skin_impl(state: &AppState, params: &SkinParams) -> Result<SkinApplied, String> {
    let zone = zone_info(state)?;
    if zone.faces.is_empty() {
        return Err("Elige primero la zona donde va la piel".into());
    }
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let uvs = scene_uvs(&scene)?;
    if uvs.len() != mesh.vertices.len() {
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
        ..Default::default()
    };
    let faces: Vec<usize> = zone.faces.iter().map(|&f| f as usize).collect();
    let result = p3d::apply_skin(&mesh, &uvs, &faces, &map, &cfg).map_err(|e| format!("No se pudo aplicar la piel: {e}"))?;
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
