//! Nube de puntos del escáner en edición (Orizon3D): se toma del escaneo o
//! de un PLY, se limpia (ruido, fragmentos, mesa, selección) con deshacer y
//! rehacer, y se malla como modelo de trabajo. Las operaciones viven en
//! `orizon3d_core::edit`.

use std::path::PathBuf;
use std::sync::Mutex;

use orizon3d_core::{edit, mesh, PointCloud};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, State};

use crate::commands::{in_background, load_scene, report, MeshInfo, Progress};
use crate::scanner::{scan_to_scene, MeshSettingsDto, ScannerHandle};

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
}

impl Edited {
    fn new(cloud: PointCloud, source: String, version: u32) -> Edited {
        // Igual que la malla del escáner: X/Z centrados y apoyada en el piso
        let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for p in &cloud.points {
            let v = [p.x, -p.y, -p.z];
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let offset = if cloud.points.is_empty() { [0.0; 3] } else { [(min[0] + max[0]) * 0.5, min[1], (min[2] + max[2]) * 0.5] };
        let spacing = edit::point_spacing(&cloud);
        Edited { cloud, source, undo: Vec::new(), redo: Vec::new(), version, offset, spacing }
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
        }
    }
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
        for v in [n as u32, ed.version, ed.cloud.has_color as u32, 0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let o = ed.offset;
        for p in &ed.cloud.points {
            for v in [p.x - o[0], -p.y - o[1], -p.z - o[2]] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        for p in &ed.cloud.points {
            bytes.extend_from_slice(&p.rgb);
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
        if mesh.is_empty() {
            return Err("La malla salió vacía: la nube tiene muy pocos puntos o el detalle es muy fino".into());
        }
        let name = "Escaneo".to_string();
        load_scene(scan_to_scene(&mesh, &name), name, "Escáner".into(), &on_progress, state)
    })
    .await
}
