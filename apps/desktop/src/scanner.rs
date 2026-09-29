//! Escáneres 3D (Orizon3D): conectar, ver en vivo, escanear y traer la malla
//! como modelo de trabajo. La captura y el escaneo viven en `orizon3d-core`.

use std::sync::{Arc, Mutex};

use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
use orizon3d_core::{DepthControls, MeshSettings, ScanSettings, Scanner, ScannerState};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, State};

use crate::commands::{in_background, load_scene, report, MeshInfo, Progress};

/// Escáner conectado (uno a la vez). En `Arc` para armar la malla sin
/// bloquear la vista previa ni el estado
#[derive(Default)]
pub struct ScannerHandle(Mutex<Option<Arc<Scanner>>>);

impl ScannerHandle {
    fn get(&self) -> Option<Arc<Scanner>> {
        self.0.lock().unwrap().clone()
    }
}

/// Volumen de escaneo y limpieza que elige la interfaz
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScanSettingsDto {
    pub clip_min_mm: f32,
    pub clip_max_mm: f32,
    pub box_mm: f32,
    pub clean_noise: bool,
    pub isolate_object: bool,
    pub edge_filter: bool,
    pub temporal_frames: usize,
}

impl ScanSettingsDto {
    fn apply(self, base: ScanSettings) -> ScanSettings {
        ScanSettings {
            clip_min_mm: self.clip_min_mm,
            clip_max_mm: self.clip_max_mm.max(self.clip_min_mm + 10.0),
            box_mm: self.box_mm.max(0.0),
            clean_noise: self.clean_noise,
            isolate_object: self.isolate_object,
            edge_filter: self.edge_filter,
            temporal_frames: self.temporal_frames.clamp(1, 10),
            ..base
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScannerStatusDto {
    /// "off", "connecting", "streaming", "error" o "stopped"
    pub state: &'static str,
    pub message: Option<String>,
    pub device: Option<String>,
    pub serial: Option<String>,
    /// Resolución y formato de cada stream, p. ej. "640×400 Y16"
    pub depth_stream: Option<String>,
    pub color_stream: Option<String>,
    pub fps: f32,
    pub distance_cm: f32,
    pub coverage: f32,
    pub scanning: bool,
    pub frames: u32,
    pub registered: u32,
    pub dropped: u32,
    pub points: usize,
    pub tracking_ok: bool,
}

impl ScannerStatusDto {
    fn off() -> Self {
        ScannerStatusDto {
            state: "off",
            message: None,
            device: None,
            serial: None,
            depth_stream: None,
            color_stream: None,
            fps: 0.0,
            distance_cm: 0.0,
            coverage: 0.0,
            scanning: false,
            frames: 0,
            registered: 0,
            dropped: 0,
            points: 0,
            tracking_ok: true,
        }
    }
}

fn status_of(scanner: Option<&Scanner>) -> ScannerStatusDto {
    let Some(scanner) = scanner else { return ScannerStatusDto::off() };
    let s = scanner.status();
    let (state, message) = match s.state {
        ScannerState::Connecting => ("connecting", None),
        ScannerState::Streaming => ("streaming", None),
        ScannerState::Error(e) => ("error", Some(e)),
        ScannerState::Stopped => ("stopped", None),
    };
    let stream = |i: &orizon3d_core::StreamInfo| format!("{}×{} {}", i.width, i.height, i.fourcc_str());
    ScannerStatusDto {
        state,
        message,
        device: s.device.as_ref().map(|d| d.name.clone()),
        serial: s.device.as_ref().map(|d| d.serial.clone()).filter(|x| !x.is_empty()),
        depth_stream: s.depth.as_ref().map(stream),
        color_stream: s.rgb.as_ref().map(stream),
        fps: s.fps,
        distance_cm: s.distance_cm,
        coverage: s.coverage,
        scanning: s.scanning,
        frames: s.stats.frames,
        registered: s.stats.registered,
        dropped: s.stats.dropped,
        points: s.points,
        tracking_ok: s.tracking_ok,
    }
}

/// Busca el escáner y empieza a transmitir (reemplaza la conexión anterior)
#[tauri::command]
pub fn scanner_connect(handle: State<'_, ScannerHandle>, settings: ScanSettingsDto) -> ScannerStatusDto {
    let mut lock = handle.0.lock().unwrap();
    // Soltar la conexión anterior libera los /dev/video* antes de reabrirlos
    lock.take();
    *lock = Some(Arc::new(Scanner::connect(settings.apply(ScanSettings::default()))));
    status_of(lock.as_deref())
}

#[tauri::command]
pub fn scanner_disconnect(handle: State<'_, ScannerHandle>) {
    handle.0.lock().unwrap().take();
}

#[tauri::command]
pub fn scanner_status(handle: State<'_, ScannerHandle>) -> ScannerStatusDto {
    status_of(handle.get().as_deref())
}

#[tauri::command]
pub fn scanner_set_settings(handle: State<'_, ScannerHandle>, settings: ScanSettingsDto) {
    if let Some(scanner) = handle.get() {
        scanner.set_settings(settings.apply(scanner.settings()));
    }
}

/// Ganancia del sensor de profundidad. El IR de la serie POP solo admite
/// auto-exposición por V4L2, así que la ganancia es la única palanca
#[tauri::command]
pub fn scanner_set_gain(handle: State<'_, ScannerHandle>, gain: i32) {
    if let Some(scanner) = handle.get() {
        scanner.set_depth_controls(DepthControls { auto_exposure: true, exposure: 8000, gain: gain.clamp(1, 16) });
    }
}

/// Último cuadro: cabecera u32 × 2 (ancho, alto) y RGBA. Vacío (0 × 0) si aún no hay
#[tauri::command]
pub fn scanner_preview(handle: State<'_, ScannerHandle>, kind: String) -> Response {
    let preview = handle.get().and_then(|s| match kind.as_str() {
        "color" => s.preview_color(640),
        _ => s.preview_depth(),
    });
    let mut bytes = Vec::with_capacity(8 + preview.as_ref().map_or(0, |p| p.rgba.len()));
    match preview {
        Some(p) => {
            bytes.extend_from_slice(&p.width.to_le_bytes());
            bytes.extend_from_slice(&p.height.to_le_bytes());
            bytes.extend_from_slice(&p.rgba);
        }
        None => bytes.extend_from_slice(&[0; 8]),
    }
    Response::new(bytes)
}

/// "start" empieza un escaneo nuevo, "stop" lo pausa, "reset" lo descarta
#[tauri::command]
pub fn scanner_scan(handle: State<'_, ScannerHandle>, action: String) -> Result<(), String> {
    let scanner = handle.get().ok_or("El escáner no está conectado")?;
    match action.as_str() {
        "start" => scanner.start_scan(),
        "stop" => scanner.stop_scan(),
        "reset" => scanner.reset_scan(),
        _ => return Err(format!("Acción desconocida: {action}")),
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MeshSettingsDto {
    pub voxel_mm: f32,
    pub fill: u32,
    pub smooth: u32,
}

/// Malla del escaneo (o del cuadro actual) como modelo de trabajo
#[tauri::command]
pub async fn scanner_create_model(
    app: AppHandle,
    settings: MeshSettingsDto,
    on_progress: Channel<Progress>,
) -> Result<MeshInfo, String> {
    let scanner = app.state::<ScannerHandle>().get().ok_or("El escáner no está conectado")?;
    in_background(app, move |state| {
        report(&on_progress, "meshing", 10, "Reconstruyendo la malla...");
        let mesh = scanner.build_mesh(&MeshSettings { voxel_mm: settings.voxel_mm, fill: settings.fill, smooth: settings.smooth })?;
        let name = "Escaneo".to_string();
        load_scene(scan_to_scene(&mesh, &name), name, "Escáner".into(), &on_progress, state)
    })
    .await
}

/// Malla del escáner (cámara: X derecha, Y abajo, Z adelante, mm) a escena
/// con Y arriba, centrada y apoyada en el piso
fn scan_to_scene(mesh: &orizon3d_core::Mesh, name: &str) -> Scene {
    // Girar 180° sobre X: Y abajo → arriba y el objeto queda mirando a la
    // cámara del visor. Es una rotación, así que el sentido de las caras se conserva
    let mut positions: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| [v[0], -v[1], -v[2]]).collect();
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &positions {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    let offset = [(min[0] + max[0]) * 0.5, min[1], (min[2] + max[2]) * 0.5];
    for p in &mut positions {
        for k in 0..3 {
            p[k] -= offset[k];
        }
    }

    let mut attributes = vec![VertexAttribute::Positions(positions)];
    if mesh.colors.len() == mesh.vertices.len() {
        let colors = mesh.colors.iter().map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]).collect();
        attributes.push(VertexAttribute::Colors(colors));
    }

    let mut scene = Scene::new();
    scene.meshes.push(SceneMesh {
        name: name.to_string(),
        primitives: vec![Primitive {
            attributes,
            indices: Some(IndexData::U32(mesh.tris.iter().flatten().copied().collect())),
            material: None,
        }],
    });
    scene.nodes.push(Node { name: name.to_string(), transform: Transform::identity(), mesh: Some(0), skin: None, children: Vec::new() });
    scene.root_nodes.push(0);
    // El escáner mide en milímetros
    scene.meters_per_unit = 0.001;
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_mesh_becomes_y_up_scene_on_the_floor() {
        let mesh = orizon3d_core::Mesh {
            // Triángulo frente a la cámara, con el vértice de arriba en Y negativa
            vertices: vec![[-10.0, 10.0, 300.0], [10.0, 10.0, 300.0], [0.0, -10.0, 310.0]],
            colors: vec![[255, 0, 0]; 3],
            tris: vec![[0, 1, 2]],
        };
        let scene = scan_to_scene(&mesh, "prueba");
        let prim = &scene.meshes[0].primitives[0];
        let VertexAttribute::Positions(p) = &prim.attributes[0] else { panic!() };
        // El vértice de arriba en la cámara queda arriba en la escena, y la base en el piso
        assert_eq!(p[2][1], 20.0);
        assert_eq!(p[0][1], 0.0);
        assert!(p.iter().all(|v| v[2].abs() <= 5.0));
        assert!(matches!(prim.attributes[1], VertexAttribute::Colors(_)));
        assert_eq!(scene.meters_per_unit, 0.001);
    }
}
