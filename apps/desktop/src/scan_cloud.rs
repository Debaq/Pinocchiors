//! Nube de puntos del escáner en edición (Orizon3D): se toma del escaneo o
//! de un PLY, se limpia (ruido, fragmentos, mesa, selección) con deshacer y
//! rehacer, y se malla como modelo de trabajo. Se le pueden sumar otros
//! escaneos o nubes (alineados solos) y comparar con una nube de referencia.
//! Las operaciones viven en `orizon3d_core::edit` y `orizon3d_core::register`.

use std::path::PathBuf;
use std::sync::Mutex;

use orizon3d_core::register::{self, AlignMode};
use orizon3d_core::{edit, mesh, PointCloud};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, State};

use crate::commands::{in_background, report, MeshInfo, Progress};
use crate::scanner::{finish_mesh, load_scan, MeshSettingsDto, ScannerHandle};

/// Pasos que se pueden deshacer (cada uno guarda la nube entera)
const MAX_UNDO: usize = 12;

struct Edited {
    cloud: PointCloud,
    /// De dónde salió ("Escaneo", "Cuadro actual" o el nombre del PLY)
    source: String,
    undo: Vec<(PointCloud, String)>,
    redo: Vec<(PointCloud, String)>,
    /// Cambia con cada edición: una selección hecha sobre otra versión no vale
    version: u32,
    /// Traslado de la vista (mm, Y arriba): fijo mientras se edita, así la nube
    /// no salta al quitar puntos
    offset: [f32; 3],
    spacing: f32,
    /// Escaneos o nubes sumados en esta nube
    parts: u32,
    /// Nube contra la que se compara, con su nombre
    reference: Option<(PointCloud, String)>,
    /// Colores del mapa de desviaciones mientras se muestra la comparación
    heat: Option<Vec<[u8; 3]>>,
}

impl Edited {
    fn new(cloud: PointCloud, source: String, version: u32) -> Edited {
        let offset = view_offset(&cloud);
        let spacing = edit::point_spacing(&cloud);
        Edited { cloud, source, undo: Vec::new(), redo: Vec::new(), version, offset, spacing, parts: 1, reference: None, heat: None }
    }

    /// Reemplaza la nube guardando la anterior para deshacer
    fn commit(&mut self, cloud: PointCloud, label: &str) {
        let old = std::mem::replace(&mut self.cloud, cloud);
        self.undo.push((old, label.to_string()));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.touch();
    }

    fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.spacing = edit::point_spacing(&self.cloud);
        // El mapa de colores era de la nube anterior
        self.heat = None;
    }

    fn info(&self) -> CloudInfo {
        CloudInfo {
            points: self.cloud.points.len(),
            has_color: self.cloud.has_color,
            version: self.version,
            spacing_mm: self.spacing,
            source: self.source.clone(),
            undo: self.undo.last().map(|(_, l)| l.clone()),
            redo: self.redo.last().map(|(_, l)| l.clone()),
            parts: self.parts,
            reference: self.reference.as_ref().map(|(c, name)| ReferenceInfo { name: name.clone(), points: c.points.len() }),
            comparing: self.heat.is_some(),
        }
    }
}

/// Traslado de la vista: como la malla del escáner, X/Z centrados y apoyada
/// en el piso (Y arriba)
fn view_offset(cloud: &PointCloud) -> [f32; 3] {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &cloud.points {
        let v = [p.x, -p.y, -p.z];
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    if cloud.points.is_empty() {
        [0.0; 3]
    } else {
        [(min[0] + max[0]) * 0.5, min[1], (min[2] + max[2]) * 0.5]
    }
}

fn copy(cloud: &PointCloud) -> PointCloud {
    PointCloud { points: cloud.points.clone(), has_color: cloud.has_color }
}

/// Nube en edición (una a la vez)
#[derive(Default)]
pub struct CloudEditor(Mutex<Option<Edited>>);

#[derive(Debug, Clone, Serialize)]
pub struct CloudInfo {
    pub points: usize,
    pub has_color: bool,
    pub version: u32,
    /// Separación típica entre puntos vecinos (mm)
    pub spacing_mm: f32,
    pub source: String,
    /// Nombre del paso que se deshace / rehace, si hay
    pub undo: Option<String>,
    pub redo: Option<String>,
    /// Escaneos o nubes sumados
    pub parts: u32,
    /// Nube de referencia para comparar, si hay
    pub reference: Option<ReferenceInfo>,
    /// Se está mostrando el mapa de desviaciones
    pub comparing: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceInfo {
    pub name: String,
    pub points: usize,
}

/// Una edición de la nube
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudOp {
    /// Borra los puntos elegidos en el visor
    Delete { indices: Vec<u32>, version: u32 },
    /// Conserva solo los puntos elegidos (recortar)
    Keep { indices: Vec<u32>, version: u32 },
    /// Ruido estadístico (SOR)
    Statistical { neighbors: usize, std_ratio: f32 },
    /// Puntos con pocos vecinos a `radius_mm`
    Radius { radius_mm: f32, min_neighbors: usize },
    /// Fragmentos sueltos con menos del `min_percent` % de puntos que el más
    /// grande (100 = solo el más grande)
    Fragments { gap_mm: f32, min_percent: f32 },
    /// Plano dominante (la mesa) y, con `below`, lo que queda debajo
    Plane { threshold_mm: f32, below: bool },
    /// Un punto por celda de `voxel_mm`
    Downsample { voxel_mm: f32 },
    /// Proyección sobre el plano local de los vecinos
    Smooth { radius_mm: f32, strength: f32 },
}

impl CloudOp {
    fn label(&self) -> &'static str {
        match self {
            CloudOp::Delete { .. } => "Borrar selección",
            CloudOp::Keep { .. } => "Recortar a la selección",
            CloudOp::Statistical { .. } => "Quitar ruido",
            CloudOp::Radius { .. } => "Quitar puntos sueltos",
            CloudOp::Fragments { .. } => "Quitar fragmentos",
            CloudOp::Plane { .. } => "Quitar la mesa",
            CloudOp::Downsample { .. } => "Simplificar",
            CloudOp::Smooth { .. } => "Suavizar",
        }
    }
}

/// Qué hizo una edición
#[derive(Debug, Clone, Serialize)]
pub struct CloudEditResult {
    /// Puntos quitados (o que se quitarían, al solo seleccionar)
    pub removed: usize,
    /// Al pedir solo la selección: los índices que la operación quitaría
    pub selected: Option<Vec<u32>>,
    pub note: Option<String>,
    pub info: CloudInfo,
}

/// Máscara de lo que se conserva, para las operaciones que solo quitan puntos
fn mask_of(ed: &Edited, op: &CloudOp) -> Result<Option<(Vec<bool>, Option<String>)>, String> {
    let cloud = &ed.cloud;
    let n = cloud.points.len();
    let check = |version: u32| {
        if version == ed.version {
            Ok(())
        } else {
            Err("La nube cambió desde que se hizo la selección: vuelve a seleccionar".to_string())
        }
    };
    Ok(Some(match op {
        CloudOp::Delete { indices, version } => {
            check(*version)?;
            (edit::mask_without(n, indices), None)
        }
        CloudOp::Keep { indices, version } => {
            check(*version)?;
            (edit::mask_only(n, indices), None)
        }
        CloudOp::Statistical { neighbors, std_ratio } => (edit::statistical_outliers(cloud, *neighbors, *std_ratio), None),
        CloudOp::Radius { radius_mm, min_neighbors } => (edit::radius_outliers(cloud, *radius_mm, *min_neighbors), None),
        CloudOp::Fragments { gap_mm, min_percent } => {
            let (_, sizes) = edit::clusters(cloud, *gap_mm);
            let mask = edit::small_clusters(cloud, *gap_mm, *min_percent / 100.0);
            let note = format!("{} fragmento{} en la nube", sizes.len(), if sizes.len() == 1 { "" } else { "s" });
            (mask, Some(note))
        }
        CloudOp::Plane { threshold_mm, below } => {
            let (mask, plane) = edit::remove_plane(cloud, *threshold_mm, *below)
                .ok_or("No se encontró un plano: ninguna superficie plana junta al menos el 5 % de los puntos")?;
            (mask, Some(format!("Plano de {} puntos", plane.inliers)))
        }
        CloudOp::Downsample { .. } | CloudOp::Smooth { .. } => return Ok(None),
    }))
}

/// Corre `f` con la nube en edición en un hilo de trabajo
async fn with_editor<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut Option<Edited>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let editor = app.state::<CloudEditor>();
        let mut lock = editor.0.lock().unwrap();
        f(&mut lock)
    })
    .await
    .map_err(|e| format!("La tarea terminó inesperadamente: {e}"))?
}

fn next_version(current: &Option<Edited>) -> u32 {
    current.as_ref().map_or(1, |e| e.version.wrapping_add(1))
}

/// Toma la nube del escáner (la fusionada del escaneo o, sin escaneo, la del
/// cuadro actual) para editarla. Reemplaza la que se estuviera editando
#[tauri::command]
pub async fn scan_cloud_take(app: AppHandle) -> Result<CloudInfo, String> {
    let scanner = app.state::<ScannerHandle>().get().ok_or("El escáner no está conectado")?;
    with_editor(app, move |editor| {
        let scanned = scanner.status().points > 0;
        let cloud = scanner.cloud().ok_or("Todavía no llega ningún cuadro del escáner")?;
        if cloud.points.is_empty() {
            return Err("No hay puntos dentro del volumen de escaneo".into());
        }
        let source = if scanned { "Escaneo" } else { "Cuadro actual" };
        let ed = Edited::new(cloud, source.into(), next_version(editor));
        let info = ed.info();
        *editor = Some(ed);
        Ok(info)
    })
    .await
}

/// Abre una nube PLY (en mm, con Y hacia abajo como la del escáner)
#[tauri::command]
pub async fn scan_cloud_import(app: AppHandle, path: PathBuf) -> Result<CloudInfo, String> {
    with_editor(app, move |editor| {
        let cloud = edit::load_ply(&path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(|| "Nube".into(), |n| n.to_string_lossy().into_owned());
        let ed = Edited::new(cloud, name, next_version(editor));
        let info = ed.info();
        *editor = Some(ed);
        Ok(info)
    })
    .await
}

/// Guarda la nube en edición como PLY binario
#[tauri::command]
pub async fn scan_cloud_export(app: AppHandle, path: PathBuf) -> Result<(), String> {
    with_editor(app, move |editor| {
        let ed = editor.as_ref().ok_or("No hay una nube en edición")?;
        edit::save_ply(&ed.cloud, &path).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))
    })
    .await
}

#[tauri::command]
pub fn scan_cloud_info(editor: State<'_, CloudEditor>) -> Option<CloudInfo> {
    editor.0.lock().unwrap().as_ref().map(Edited::info)
}

/// Nube para el visor, en binario: cabecera u32 × 4 (puntos, versión, con
/// color, reservado), posiciones f32 × 3 (mm, Y arriba, centrada en X/Z y
/// apoyada en el piso) y colores u8 × 3 (rellenos a múltiplo de 4)
#[tauri::command]
pub async fn scan_cloud_data(app: AppHandle) -> Result<Response, String> {
    with_editor(app, |editor| {
        let Some(ed) = editor.as_ref() else { return Ok(Response::new(vec![0; 16])) };
        let n = ed.cloud.points.len();
        let mut bytes = Vec::with_capacity(16 + n * 15 + 4);
        let heat = ed.heat.as_ref().filter(|h| h.len() == n);
        for v in [n as u32, ed.version, (ed.cloud.has_color || heat.is_some()) as u32, 0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let o = ed.offset;
        for p in &ed.cloud.points {
            for v in [p.x - o[0], -p.y - o[1], -p.z - o[2]] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        match heat {
            Some(colors) => colors.iter().for_each(|c| bytes.extend_from_slice(c)),
            None => ed.cloud.points.iter().for_each(|p| bytes.extend_from_slice(&p.rgb)),
        }
        bytes.resize(bytes.len().div_ceil(4) * 4, 0);
        Ok(Response::new(bytes))
    })
    .await
}

/// Aplica una edición. Con `select_only`, no cambia la nube: devuelve los
/// índices que la operación quitaría, para verlos y decidir en el visor
#[tauri::command]
pub async fn scan_cloud_edit(app: AppHandle, op: CloudOp, select_only: Option<bool>) -> Result<CloudEditResult, String> {
    with_editor(app, move |editor| {
        let ed = editor.as_mut().ok_or("No hay una nube en edición")?;
        let before = ed.cloud.points.len();
        if let Some((mask, note)) = mask_of(ed, &op)? {
            let removed = mask.iter().filter(|k| !**k).count();
            if select_only.unwrap_or(false) {
                let selected = mask.iter().enumerate().filter(|(_, k)| !**k).map(|(i, _)| i as u32).collect();
                return Ok(CloudEditResult { removed, selected: Some(selected), note, info: ed.info() });
            }
            if removed == before {
                return Err("La operación quitaría la nube entera; prueba con valores más suaves".into());
            }
            if removed > 0 {
                let cloud = edit::retain(&ed.cloud, &mask);
                ed.commit(cloud, op.label());
            }
            return Ok(CloudEditResult { removed, selected: None, note, info: ed.info() });
        }
        if select_only.unwrap_or(false) {
            return Err("Esta operación no quita puntos: no hay nada que seleccionar".into());
        }
        let cloud = match op {
            CloudOp::Downsample { voxel_mm } => edit::downsample(&ed.cloud, voxel_mm),
            CloudOp::Smooth { radius_mm, strength } => edit::smooth(&ed.cloud, radius_mm, strength),
            _ => unreachable!("las demás operaciones dan una máscara"),
        };
        let removed = before.saturating_sub(cloud.points.len());
        ed.commit(cloud, op.label());
        Ok(CloudEditResult { removed, selected: None, note: None, info: ed.info() })
    })
    .await
}

/// Deshace (`redo = false`) o rehace la última edición de la nube
#[tauri::command]
pub async fn scan_cloud_history(app: AppHandle, redo: bool) -> Result<CloudInfo, String> {
    with_editor(app, move |editor| {
        let ed = editor.as_mut().ok_or("No hay una nube en edición")?;
        let (from, to) = if redo { (&mut ed.redo, &mut ed.undo) } else { (&mut ed.undo, &mut ed.redo) };
        let (cloud, label) = from.pop().ok_or(if redo { "No hay nada que rehacer" } else { "No hay nada que deshacer" })?;
        let old = std::mem::replace(&mut ed.cloud, cloud);
        to.push((old, label));
        ed.touch();
        Ok(ed.info())
    })
    .await
}

/// Suelta la nube en edición
#[tauri::command]
pub fn scan_cloud_discard(editor: State<'_, CloudEditor>) {
    editor.0.lock().unwrap().take();
}

/// Malla la nube en edición como modelo de trabajo
#[tauri::command]
pub async fn scan_cloud_create_model(
    app: AppHandle,
    settings: MeshSettingsDto,
    on_progress: Channel<Progress>,
) -> Result<MeshInfo, String> {
    // Copia de la nube: la malla tarda y no debe bloquear la edición
    let cloud = {
        let editor = app.state::<CloudEditor>();
        let lock = editor.0.lock().unwrap();
        let ed = lock.as_ref().ok_or("No hay una nube en edición")?;
        PointCloud { points: ed.cloud.points.clone(), has_color: ed.cloud.has_color }
    };
    in_background(app, move |state| {
        report(&on_progress, "meshing", 10, "Reconstruyendo la malla...");
        let mesh = mesh::reconstruct(&cloud, settings.voxel_mm.max(1.0), settings.fill, settings.smooth);
        let mesh = finish_mesh(mesh, &settings, &on_progress)?;
        load_scan(&mesh, &on_progress, state)
    })
    .await
}

fn align_mode(mode: &str) -> Result<AlignMode, String> {
    match mode {
        "auto" => Ok(AlignMode::Auto),
        "fine" => Ok(AlignMode::Fine),
        "none" => Ok(AlignMode::None),
        _ => Err(format!("Modo de alineación desconocido: {mode}")),
    }
}

/// Nube de otro origen: "scan" (la del escáner) o "ply" (un archivo)
fn load_other(app: &AppHandle, from: &str, path: Option<&PathBuf>) -> Result<(PointCloud, String), String> {
    match from {
        "scan" => {
            let scanner = app.state::<ScannerHandle>().get().ok_or("El escáner no está conectado")?;
            let scanned = scanner.status().points > 0;
            let cloud = scanner.cloud().ok_or("Todavía no llega ningún cuadro del escáner")?;
            Ok((cloud, if scanned { "Escaneo" } else { "Cuadro actual" }.into()))
        }
        "ply" => {
            let path = path.ok_or("Falta el archivo PLY")?;
            let cloud = edit::load_ply(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
            Ok((cloud, path.file_name().map_or_else(|| "Nube".into(), |n| n.to_string_lossy().into_owned())))
        }
        _ => Err(format!("Origen desconocido: {from}")),
    }
}

/// Qué pasó al sumar o alinear una nube
#[derive(Debug, Clone, Serialize)]
pub struct AlignResult {
    /// Puntos de la nube nueva
    pub added: usize,
    /// Fracción de la nube nueva que calza sobre la anterior (0 a 1)
    pub overlap: f32,
    /// Error de las parejas finales (mm)
    pub rmse: f32,
    pub info: CloudInfo,
}

/// Suma a la nube en edición otra nube (del escáner o un PLY), alineada
/// sobre ella según `align` ("auto", "fine" o "none"). Con `fuse`, el solape
/// se promedia para no dejar dos capas. Sin nube en edición, la toma sin más.
/// Se deshace como cualquier edición: el escaneo anterior nunca se pierde
#[tauri::command]
pub async fn scan_cloud_merge(
    app: AppHandle,
    from: String,
    path: Option<PathBuf>,
    align: String,
    fuse: bool,
) -> Result<AlignResult, String> {
    let mode = align_mode(&align)?;
    let (other, name) = load_other(&app, &from, path.as_ref())?;
    if other.points.is_empty() {
        return Err("La nube nueva no tiene puntos".into());
    }
    with_editor(app, move |editor| {
        let added = other.points.len();
        let Some(ed) = editor.as_mut() else {
            let ed = Edited::new(other, name, next_version(editor));
            let info = ed.info();
            *editor = Some(ed);
            return Ok(AlignResult { added, overlap: 1.0, rmse: 0.0, info });
        };
        let a = register::align(&other, &ed.cloud, mode).ok_or(
            "No se encontró cómo encajar la nube nueva sobre la anterior. Limpia las dos (ruido, mesa) para que \
             compartan más superficie, o súmala sin alinear",
        )?;
        let moved = register::transform_cloud(&other, &a.transform);
        let fuse_voxel = if fuse { ed.spacing.max(edit::point_spacing(&moved)) } else { 0.0 };
        let merged = register::merge(&ed.cloud, &moved, fuse_voxel);
        ed.parts += 1;
        let label = format!("Sumar {} ({})", name.to_lowercase(), ed.parts);
        ed.commit(merged, &label);
        ed.offset = view_offset(&ed.cloud);
        Ok(AlignResult { added, overlap: a.overlap, rmse: a.rmse, info: ed.info() })
    })
    .await
}

/// Fija la nube de referencia para comparar: "current" (una copia de la nube
/// tal como está ahora), "ply" (un archivo) o "clear" (ninguna)
#[tauri::command]
pub async fn scan_cloud_reference(app: AppHandle, action: String, path: Option<PathBuf>) -> Result<CloudInfo, String> {
    let loaded = if action == "ply" { Some(load_other(&app, "ply", path.as_ref())?) } else { None };
    with_editor(app, move |editor| {
        let ed = editor.as_mut().ok_or("No hay una nube en edición")?;
        ed.reference = match action.as_str() {
            "current" => Some((copy(&ed.cloud), format!("Copia de {}", ed.source.to_lowercase()))),
            "ply" => loaded,
            "clear" => None,
            _ => return Err(format!("Acción desconocida: {action}")),
        };
        ed.heat = None;
        Ok(ed.info())
    })
    .await
}

/// Mueve la nube en edición para que calce sobre la referencia (se deshace)
#[tauri::command]
pub async fn scan_cloud_align_reference(app: AppHandle, align: String) -> Result<AlignResult, String> {
    let mode = align_mode(&align)?;
    with_editor(app, move |editor| {
        let ed = editor.as_mut().ok_or("No hay una nube en edición")?;
        let (reference, _) = ed.reference.as_ref().ok_or("Primero fija una nube de referencia")?;
        let a = register::align(&ed.cloud, reference, mode).ok_or("No se encontró cómo encajar la nube sobre la referencia")?;
        let moved = register::transform_cloud(&ed.cloud, &a.transform);
        let added = moved.points.len();
        ed.commit(moved, "Alinear a la referencia");
        Ok(AlignResult { added, overlap: a.overlap, rmse: a.rmse, info: ed.info() })
    })
    .await
}

/// Resultado de comparar con la referencia (mm)
#[derive(Debug, Clone, Serialize)]
pub struct CompareResult {
    pub compared: usize,
    pub unmatched: usize,
    pub mean_mm: f32,
    pub rms_mm: f32,
    pub max_mm: f32,
    pub p95_mm: f32,
    /// Porcentaje dentro de la tolerancia
    pub within_percent: f32,
    pub info: CloudInfo,
}

/// Distancia de cada punto a la referencia: estadísticas y, con `show`, el
/// mapa de colores en el visor (azul = calza, rojo = `scale_mm` o más, gris =
/// sin pareja a menos de `reach_mm`). Sin `show`, vuelve a los colores reales
#[tauri::command]
pub async fn scan_cloud_compare(
    app: AppHandle,
    tolerance_mm: f32,
    scale_mm: f32,
    reach_mm: f32,
    show: bool,
) -> Result<Option<CompareResult>, String> {
    with_editor(app, move |editor| {
        let ed = editor.as_mut().ok_or("No hay una nube en edición")?;
        if !show {
            ed.heat = None;
            return Ok(None);
        }
        let (reference, _) = ed.reference.as_ref().ok_or("Primero fija una nube de referencia")?;
        let dist = register::distances(&ed.cloud, reference, reach_mm.max(tolerance_mm).max(0.1));
        let d = register::summarize(&dist, tolerance_mm);
        ed.heat = Some(register::heat_colors(&dist, scale_mm.max(0.05)));
        Ok(Some(CompareResult {
            compared: d.compared,
            unmatched: d.unmatched,
            mean_mm: d.mean,
            rms_mm: d.rms,
            max_mm: d.max,
            p95_mm: d.p95,
            within_percent: d.within * 100.0,
            info: ed.info(),
        }))
    })
    .await
}
