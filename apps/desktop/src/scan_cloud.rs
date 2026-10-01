//! Nubes de puntos del escáner en edición (Orizon3D), repartidas en tomas:
//! cada escaneo o PLY es una toma que se limpia por separado (ruido,
//! fragmentos, mesa, selección) con su propio deshacer y rehacer. Las tomas
//! se alinean entre sí y se fusionan en una nube completa, que se malla como
//! modelo de trabajo. También se compara contra una nube de referencia.
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

/// Pasos que se pueden deshacer en cada toma (cada uno guarda la nube entera)
const MAX_UNDO: usize = 12;

/// Una toma: un escaneo o una nube abierta, con su historial
struct Take {
    cloud: PointCloud,
    /// Nombre a la vista ("Escaneo 2", el nombre del PLY, "Fusión")
    name: String,
    undo: Vec<(PointCloud, String)>,
    redo: Vec<(PointCloud, String)>,
    spacing: f32,
    /// Escaneos o nubes que junta esta toma
    parts: u32,
    /// Se ve en el visor (la activa se ve siempre)
    visible: bool,
}

impl Take {
    fn new(cloud: PointCloud, name: String, parts: u32) -> Take {
        let spacing = edit::point_spacing(&cloud);
        Take { cloud, name, undo: Vec::new(), redo: Vec::new(), spacing, parts, visible: true }
    }

    /// Reemplaza la nube guardando la anterior para deshacer
    fn commit(&mut self, cloud: PointCloud, label: &str) {
        let old = std::mem::replace(&mut self.cloud, cloud);
        self.undo.push((old, label.to_string()));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.spacing = edit::point_spacing(&self.cloud);
    }
}

/// Las tomas en edición y lo que comparten
struct Session {
    takes: Vec<Take>,
    /// Toma sobre la que actúan las herramientas
    active: usize,
    /// Cambia con cada edición o cambio de toma: una selección hecha sobre
    /// otra versión no vale
    version: u32,
    /// Traslado de la vista (mm, Y arriba): común a todas las tomas, así se
    /// ven en su lugar unas respecto de otras y no saltan al editarlas
    offset: [f32; 3],
    /// Escaneos tomados en la sesión, para numerarlos
    scans: u32,
    /// Nube contra la que se compara, con su nombre
    reference: Option<(PointCloud, String)>,
    /// Colores del mapa de desviaciones de la toma activa
    heat: Option<Vec<[u8; 3]>>,
}

impl Session {
    fn new(take: Take, version: u32) -> Session {
        let offset = view_offset(&take.cloud);
        Session { takes: vec![take], active: 0, version, offset, scans: 0, reference: None, heat: None }
    }

    fn cur(&self) -> &Take {
        &self.takes[self.active]
    }

    fn cur_mut(&mut self) -> &mut Take {
        &mut self.takes[self.active]
    }

    /// Cambia la nube de la toma activa (se deshace)
    fn commit(&mut self, cloud: PointCloud, label: &str) {
        self.cur_mut().commit(cloud, label);
        self.touch();
    }

    fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
        // El mapa de colores era de la nube anterior
        self.heat = None;
    }

    /// Suma una toma y la deja activa
    fn push(&mut self, take: Take) {
        self.takes.push(take);
        self.active = self.takes.len() - 1;
        self.touch();
    }

    /// Las demás tomas a la vista, juntas (sin la activa)
    fn others_visible(&self) -> Option<PointCloud> {
        let others: Vec<&Take> = self.takes.iter().enumerate().filter(|(i, t)| *i != self.active && t.visible).map(|(_, t)| t).collect();
        if others.is_empty() {
            return None;
        }
        let mut points = Vec::with_capacity(others.iter().map(|t| t.cloud.points.len()).sum());
        for t in &others {
            points.extend_from_slice(&t.cloud.points);
        }
        Some(PointCloud { points, has_color: others.iter().all(|t| t.cloud.has_color) })
    }

    fn info(&self) -> CloudInfo {
        let cur = self.cur();
        CloudInfo {
            points: cur.cloud.points.len(),
            has_color: cur.cloud.has_color,
            version: self.version,
            spacing_mm: cur.spacing,
            source: cur.name.clone(),
            undo: cur.undo.last().map(|(_, l)| l.clone()),
            redo: cur.redo.last().map(|(_, l)| l.clone()),
            parts: cur.parts,
            reference: self.reference.as_ref().map(|(c, name)| ReferenceInfo { name: name.clone(), points: c.points.len() }),
            comparing: self.heat.is_some(),
            takes: self
                .takes
                .iter()
                .map(|t| TakeInfo { name: t.name.clone(), points: t.cloud.points.len(), visible: t.visible, parts: t.parts })
                .collect(),
            active: self.active,
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

/// Tomas en edición
#[derive(Default)]
pub struct CloudEditor(Mutex<Option<Session>>);

#[derive(Debug, Clone, Serialize)]
pub struct CloudInfo {
    /// Puntos de la toma activa
    pub points: usize,
    pub has_color: bool,
    pub version: u32,
    /// Separación típica entre puntos vecinos de la toma activa (mm)
    pub spacing_mm: f32,
    /// Nombre de la toma activa
    pub source: String,
    /// Nombre del paso que se deshace / rehace en la toma activa, si hay
    pub undo: Option<String>,
    pub redo: Option<String>,
    /// Escaneos o nubes que junta la toma activa
    pub parts: u32,
    /// Nube de referencia para comparar, si hay
    pub reference: Option<ReferenceInfo>,
    /// Se está mostrando el mapa de desviaciones
    pub comparing: bool,
    /// Todas las tomas, en orden
    pub takes: Vec<TakeInfo>,
    /// Índice de la toma activa
    pub active: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TakeInfo {
    pub name: String,
    pub points: usize,
    pub visible: bool,
    pub parts: u32,
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
fn mask_of(s: &Session, op: &CloudOp) -> Result<Option<(Vec<bool>, Option<String>)>, String> {
    let cloud = &s.cur().cloud;
    let n = cloud.points.len();
    let check = |version: u32| {
        if version == s.version {
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

/// Corre `f` con las tomas en edición en un hilo de trabajo
async fn with_editor<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut Option<Session>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let editor = app.state::<CloudEditor>();
        let mut lock = editor.0.lock().unwrap();
        f(&mut lock)
    })
    .await
    .map_err(|e| format!("La tarea terminó inesperadamente: {e}"))?
}

fn next_version(current: &Option<Session>) -> u32 {
    current.as_ref().map_or(1, |s| s.version.wrapping_add(1))
}

/// Nube de otro origen: "scan" (la del escáner) o "ply" (un archivo). El
/// segundo valor dice si es un escaneo (para numerarlo) y el tercero es el
/// nombre del archivo
fn load_other(app: &AppHandle, from: &str, path: Option<&PathBuf>) -> Result<(PointCloud, bool, String), String> {
    match from {
        "scan" => {
            let scanner = app.state::<ScannerHandle>().get().ok_or("El escáner no está conectado")?;
            let scanned = scanner.status().points > 0;
            let cloud = scanner.cloud().ok_or("Todavía no llega ningún cuadro del escáner")?;
            if cloud.points.is_empty() {
                return Err("No hay puntos dentro del volumen de escaneo".into());
            }
            Ok((cloud, true, if scanned { "Escaneo" } else { "Cuadro" }.into()))
        }
        "ply" => {
            let path = path.ok_or("Falta el archivo PLY")?;
            let cloud = edit::load_ply(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
            if cloud.points.is_empty() {
                return Err(format!("{} no tiene puntos", path.display()));
            }
            Ok((cloud, false, path.file_name().map_or_else(|| "Nube".into(), |n| n.to_string_lossy().into_owned())))
        }
        _ => Err(format!("Origen desconocido: {from}")),
    }
}

fn align_mode(mode: &str) -> Result<AlignMode, String> {
    match mode {
        "auto" => Ok(AlignMode::Auto),
        "fine" => Ok(AlignMode::Fine),
        "none" => Ok(AlignMode::None),
        _ => Err(format!("Modo de alineación desconocido: {mode}")),
    }
}

/// Qué pasó al sumar o alinear una nube
#[derive(Debug, Clone, Serialize)]
pub struct AlignResult {
    /// Puntos de la nube nueva
    pub added: usize,
    /// Fracción de la nube nueva que calza sobre las demás (0 a 1); `None`
    /// si no hubo con qué alinear
    pub overlap: Option<f32>,
    /// Error de las parejas finales (mm)
    pub rmse: f32,
    /// Aviso, si la alineación no se pudo hacer
    pub note: Option<String>,
    pub info: CloudInfo,
}

/// Suma una toma nueva (del escáner, `from = "scan"`, o un PLY) y la deja
/// activa. Con `align` distinto de "none", la lleva sobre las tomas a la
/// vista; si no encaja, queda donde llegó, con un aviso. Las demás tomas no
/// cambian
#[tauri::command]
pub async fn scan_cloud_add(app: AppHandle, from: String, path: Option<PathBuf>, align: String) -> Result<AlignResult, String> {
    let mode = align_mode(&align)?;
    let (cloud, is_scan, base) = load_other(&app, &from, path.as_ref())?;
    with_editor(app, move |editor| {
        let added = cloud.points.len();
        let Some(s) = editor.as_mut() else {
            let mut s = Session::new(Take::new(cloud, String::new(), 1), next_version(editor));
            if is_scan {
                s.scans = 1;
                s.takes[0].name = format!("{base} 1");
            } else {
                s.takes[0].name = base;
            }
            let info = s.info();
            *editor = Some(s);
            return Ok(AlignResult { added, overlap: None, rmse: 0.0, note: None, info });
        };
        let name = if is_scan {
            s.scans += 1;
            format!("{base} {}", s.scans)
        } else {
            base
        };
        let (mut overlap, mut rmse, mut note) = (None, 0.0, None);
        let mut cloud = cloud;
        if mode != AlignMode::None {
            // Las demás a la vista, sin contar la nueva (todavía no está)
            let target = {
                let mut points = Vec::new();
                for t in s.takes.iter().filter(|t| t.visible) {
                    points.extend_from_slice(&t.cloud.points);
                }
                PointCloud { points, has_color: false }
            };
            if target.points.is_empty() {
                note = Some("No hay otras tomas a la vista: quedó sin alinear".into());
            } else {
                match register::align(&cloud, &target, mode) {
                    Some(a) => {
                        cloud = register::transform_cloud(&cloud, &a.transform);
                        overlap = Some(a.overlap);
                        rmse = a.rmse;
                    }
                    None => {
                        note = Some(
                            "No encajó sobre las demás tomas: quedó donde llegó. Límpiala (ruido, mesa) y usa «Alinear» en la lista de tomas"
                                .into(),
                        )
                    }
                }
            }
        }
        s.push(Take::new(cloud, name, 1));
        Ok(AlignResult { added, overlap, rmse, note, info: s.info() })
    })
    .await
}

/// Toma la nube del escáner (la del escaneo o, sin escaneo, la del cuadro
/// actual) como toma nueva, sin alinear
#[tauri::command]
pub async fn scan_cloud_take(app: AppHandle) -> Result<CloudInfo, String> {
    Ok(scan_cloud_add(app, "scan".into(), None, "none".into()).await?.info)
}

/// Abre una nube PLY (en mm, con Y hacia abajo como la del escáner) como
/// toma nueva, sin alinear
#[tauri::command]
pub async fn scan_cloud_import(app: AppHandle, path: PathBuf) -> Result<CloudInfo, String> {
    Ok(scan_cloud_add(app, "ply".into(), Some(path), "none".into()).await?.info)
}

/// Guarda la toma activa como PLY binario
#[tauri::command]
pub async fn scan_cloud_export(app: AppHandle, path: PathBuf) -> Result<(), String> {
    with_editor(app, move |editor| {
        let s = editor.as_ref().ok_or("No hay una nube en edición")?;
        edit::save_ply(&s.cur().cloud, &path).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))
    })
    .await
}

#[tauri::command]
pub fn scan_cloud_info(editor: State<'_, CloudEditor>) -> Option<CloudInfo> {
    editor.0.lock().unwrap().as_ref().map(Session::info)
}

/// Nubes para el visor, en binario: cabecera u32 × 4 (puntos de la toma
/// activa, versión, con color, puntos de las demás tomas a la vista), las
/// posiciones f32 × 3 de la activa (mm, Y arriba, centrada en X/Z y apoyada
/// en el piso), sus colores u8 × 3 (rellenos a múltiplo de 4) y al final las
/// posiciones de las demás, que se dibujan de fondo y no se seleccionan
#[tauri::command]
pub async fn scan_cloud_data(app: AppHandle) -> Result<Response, String> {
    with_editor(app, |editor| {
        let Some(s) = editor.as_ref() else { return Ok(Response::new(vec![0; 16])) };
        let cur = s.cur();
        let n = cur.cloud.points.len();
        let ghosts: usize = s.takes.iter().enumerate().filter(|(i, t)| *i != s.active && t.visible).map(|(_, t)| t.cloud.points.len()).sum();
        let mut bytes = Vec::with_capacity(16 + n * 15 + 4 + ghosts * 12);
        let heat = s.heat.as_ref().filter(|h| h.len() == n);
        for v in [n as u32, s.version, (cur.cloud.has_color || heat.is_some()) as u32, ghosts as u32] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let o = s.offset;
        let put = |cloud: &PointCloud, bytes: &mut Vec<u8>| {
            for p in &cloud.points {
                for v in [p.x - o[0], -p.y - o[1], -p.z - o[2]] {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
        };
        put(&cur.cloud, &mut bytes);
        match heat {
            Some(colors) => colors.iter().for_each(|c| bytes.extend_from_slice(c)),
            None => cur.cloud.points.iter().for_each(|p| bytes.extend_from_slice(&p.rgb)),
        }
        bytes.resize(bytes.len().div_ceil(4) * 4, 0);
        for (i, t) in s.takes.iter().enumerate() {
            if i != s.active && t.visible {
                put(&t.cloud, &mut bytes);
            }
        }
        Ok(Response::new(bytes))
    })
    .await
}

/// Aplica una edición a la toma activa. Con `select_only`, no cambia la
/// nube: devuelve los índices que la operación quitaría, para verlos y
/// decidir en el visor
#[tauri::command]
pub async fn scan_cloud_edit(app: AppHandle, op: CloudOp, select_only: Option<bool>) -> Result<CloudEditResult, String> {
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        let before = s.cur().cloud.points.len();
        if let Some((mask, note)) = mask_of(s, &op)? {
            let removed = mask.iter().filter(|k| !**k).count();
            if select_only.unwrap_or(false) {
                let selected = mask.iter().enumerate().filter(|(_, k)| !**k).map(|(i, _)| i as u32).collect();
                return Ok(CloudEditResult { removed, selected: Some(selected), note, info: s.info() });
            }
            if removed == before {
                return Err("La operación quitaría la nube entera; prueba con valores más suaves".into());
            }
            if removed > 0 {
                let cloud = edit::retain(&s.cur().cloud, &mask);
                s.commit(cloud, op.label());
            }
            return Ok(CloudEditResult { removed, selected: None, note, info: s.info() });
        }
        if select_only.unwrap_or(false) {
            return Err("Esta operación no quita puntos: no hay nada que seleccionar".into());
        }
        let cloud = match op {
            CloudOp::Downsample { voxel_mm } => edit::downsample(&s.cur().cloud, voxel_mm),
            CloudOp::Smooth { radius_mm, strength } => edit::smooth(&s.cur().cloud, radius_mm, strength),
            _ => unreachable!("las demás operaciones dan una máscara"),
        };
        let removed = before.saturating_sub(cloud.points.len());
        s.commit(cloud, op.label());
        Ok(CloudEditResult { removed, selected: None, note: None, info: s.info() })
    })
    .await
}

/// Deshace (`redo = false`) o rehace la última edición de la toma activa
#[tauri::command]
pub async fn scan_cloud_history(app: AppHandle, redo: bool) -> Result<CloudInfo, String> {
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        let t = s.cur_mut();
        let (from, to) = if redo { (&mut t.redo, &mut t.undo) } else { (&mut t.undo, &mut t.redo) };
        let (cloud, label) = from.pop().ok_or(if redo { "No hay nada que rehacer" } else { "No hay nada que deshacer" })?;
        let old = std::mem::replace(&mut t.cloud, cloud);
        to.push((old, label));
        t.spacing = edit::point_spacing(&t.cloud);
        s.touch();
        Ok(s.info())
    })
    .await
}

/// Suelta todas las tomas
#[tauri::command]
pub fn scan_cloud_discard(editor: State<'_, CloudEditor>) {
    editor.0.lock().unwrap().take();
}

/// Qué hacer con una toma: "select" (dejarla activa), "show", "hide",
/// "remove" (quitarla de la lista; no se deshace) o "rename" (con `name`).
/// Quitar la última toma cierra la edición (`None`)
#[tauri::command]
pub async fn scan_cloud_take_set(
    app: AppHandle,
    index: usize,
    action: String,
    name: Option<String>,
) -> Result<Option<CloudInfo>, String> {
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        if index >= s.takes.len() {
            return Err("Esa toma ya no existe".into());
        }
        match action.as_str() {
            "select" => {
                if s.active != index {
                    s.active = index;
                    s.takes[index].visible = true;
                    s.touch();
                }
            }
            "show" => s.takes[index].visible = true,
            "hide" => {
                if index == s.active {
                    return Err("La toma activa siempre se ve: elige otra antes de ocultarla".into());
                }
                s.takes[index].visible = false;
            }
            "remove" => {
                if s.takes.len() == 1 {
                    *editor = None;
                    return Ok(None);
                }
                s.takes.remove(index);
                if s.active > index || s.active == s.takes.len() {
                    s.active = s.active.saturating_sub(1);
                }
                s.takes[s.active].visible = true;
                s.touch();
            }
            "rename" => {
                let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).ok_or("Falta el nombre")?;
                s.takes[index].name = name;
            }
            _ => return Err(format!("Acción desconocida: {action}")),
        }
        Ok(Some(s.info()))
    })
    .await
}

/// Lleva la toma activa sobre las demás tomas a la vista (se deshace en la
/// toma)
#[tauri::command]
pub async fn scan_cloud_take_align(app: AppHandle, align: String) -> Result<AlignResult, String> {
    let mode = align_mode(&align)?;
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        let target = s.others_visible().ok_or("No hay otras tomas a la vista contra las que alinear")?;
        let a = register::align(&s.cur().cloud, &target, mode).ok_or(
            "No se encontró cómo encajar esta toma sobre las demás. Limpia las dos (ruido, mesa) para que compartan más \
             superficie, o prueba la alineación automática",
        )?;
        let moved = register::transform_cloud(&s.cur().cloud, &a.transform);
        let added = moved.points.len();
        s.commit(moved, "Alinear con las demás tomas");
        Ok(AlignResult { added, overlap: Some(a.overlap), rmse: a.rmse, note: None, info: s.info() })
    })
    .await
}

/// Cómo quedó una fusión
#[derive(Debug, Clone, Serialize)]
pub struct FuseResult {
    /// Tomas fusionadas
    pub fused: usize,
    /// Peor solape entre una toma y lo ya fusionado (0 a 1); `None` sin
    /// alinear
    pub worst_overlap: Option<f32>,
    /// Nombre de la toma con el peor solape
    pub worst: Option<String>,
    pub info: CloudInfo,
}

/// Fusiona las tomas a la vista en una toma nueva ("Fusión"), que queda
/// activa; las originales se ocultan pero se conservan. Con `align`
/// distinto de "none", cada toma se encaja sobre lo ya fusionado, empezando
/// por la activa. Con `fuse`, el solape se promedia para no dejar dos capas
#[tauri::command]
pub async fn scan_cloud_fuse(app: AppHandle, align: String, fuse: bool) -> Result<FuseResult, String> {
    let mode = align_mode(&align)?;
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        // La activa primero: es la que marca la posición del resultado
        let mut order: Vec<usize> = vec![s.active];
        order.extend((0..s.takes.len()).filter(|&i| i != s.active && s.takes[i].visible));
        if order.len() < 2 {
            return Err("Deja a la vista al menos dos tomas para fusionarlas".into());
        }
        let mut merged = copy(&s.takes[order[0]].cloud);
        let mut parts = s.takes[order[0]].parts;
        let mut worst: Option<(f32, String)> = None;
        for &i in &order[1..] {
            let take = &s.takes[i];
            let piece = if mode == AlignMode::None {
                copy(&take.cloud)
            } else {
                let a = register::align(&take.cloud, &merged, mode).ok_or(format!(
                    "«{}» no encajó sobre las demás. Límpiala, alinéala a mano (botón Alinear) y fusiona sin alinear, u ocúltala",
                    take.name
                ))?;
                if worst.as_ref().map_or(true, |(o, _)| a.overlap < *o) {
                    worst = Some((a.overlap, take.name.clone()));
                }
                register::transform_cloud(&take.cloud, &a.transform)
            };
            let voxel = if fuse { take.spacing.max(edit::point_spacing(&merged)) } else { 0.0 };
            merged = register::merge(&merged, &piece, voxel);
            parts += take.parts;
        }
        for &i in &order {
            s.takes[i].visible = false;
        }
        let fused = order.len();
        s.push(Take::new(merged, format!("Fusión de {fused} tomas"), parts));
        let (worst_overlap, worst) = worst.map_or((None, None), |(o, n)| (Some(o), Some(n)));
        Ok(FuseResult { fused, worst_overlap, worst, info: s.info() })
    })
    .await
}

/// Malla la toma activa como modelo de trabajo
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
        let s = lock.as_ref().ok_or("No hay una nube en edición")?;
        copy(&s.cur().cloud)
    };
    in_background(app, move |state| {
        report(&on_progress, "meshing", 10, "Reconstruyendo la malla...");
        let mesh = mesh::reconstruct(&cloud, settings.voxel_mm.max(1.0), settings.fill, settings.smooth);
        let mesh = finish_mesh(mesh, &settings, &on_progress)?;
        load_scan(&mesh, &on_progress, state)
    })
    .await
}

/// Fija la nube de referencia para comparar: "current" (una copia de la toma
/// activa tal como está ahora), "ply" (un archivo) o "clear" (ninguna)
#[tauri::command]
pub async fn scan_cloud_reference(app: AppHandle, action: String, path: Option<PathBuf>) -> Result<CloudInfo, String> {
    let loaded = if action == "ply" { Some(load_other(&app, "ply", path.as_ref())?) } else { None };
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        s.reference = match action.as_str() {
            "current" => Some((copy(&s.cur().cloud), format!("Copia de {}", s.cur().name.to_lowercase()))),
            "ply" => loaded.map(|(cloud, _, name)| (cloud, name)),
            "clear" => None,
            _ => return Err(format!("Acción desconocida: {action}")),
        };
        s.heat = None;
        Ok(s.info())
    })
    .await
}

/// Mueve la toma activa para que calce sobre la referencia (se deshace)
#[tauri::command]
pub async fn scan_cloud_align_reference(app: AppHandle, align: String) -> Result<AlignResult, String> {
    let mode = align_mode(&align)?;
    with_editor(app, move |editor| {
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        let (reference, _) = s.reference.as_ref().ok_or("Primero fija una nube de referencia")?;
        let a = register::align(&s.cur().cloud, reference, mode).ok_or("No se encontró cómo encajar la nube sobre la referencia")?;
        let moved = register::transform_cloud(&s.cur().cloud, &a.transform);
        let added = moved.points.len();
        s.commit(moved, "Alinear a la referencia");
        Ok(AlignResult { added, overlap: Some(a.overlap), rmse: a.rmse, note: None, info: s.info() })
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
        let s = editor.as_mut().ok_or("No hay una nube en edición")?;
        if !show {
            s.heat = None;
            return Ok(None);
        }
        let (reference, _) = s.reference.as_ref().ok_or("Primero fija una nube de referencia")?;
        let dist = register::distances(&s.cur().cloud, reference, reach_mm.max(tolerance_mm).max(0.1));
        let d = register::summarize(&dist, tolerance_mm);
        s.heat = Some(register::heat_colors(&dist, scale_mm.max(0.05)));
        Ok(Some(CompareResult {
            compared: d.compared,
            unmatched: d.unmatched,
            mean_mm: d.mean,
            rms_mm: d.rms,
            max_mm: d.max,
            p95_mm: d.p95,
            within_percent: d.within * 100.0,
            info: s.info(),
        }))
    })
    .await
}
