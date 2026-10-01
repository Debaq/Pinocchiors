//! Escáneres 3D (Orizon3D): conectar, ver en vivo, escanear y traer la malla
//! como modelo de trabajo. La captura y el escaneo viven en `orizon3d-core`.

use std::sync::{Arc, Mutex};

use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
use orizon3d_core::{DepthControls, MeshSettings, ScanSettings, Scanner, ScannerState};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, State};

use crate::commands::{in_background, load_scene, report, MeshInfo, Progress};
use crate::state::AppState;

/// Escáner conectado (uno a la vez). En `Arc` para armar la malla sin
/// bloquear la vista previa ni el estado
#[derive(Default)]
pub struct ScannerHandle(Mutex<Option<Arc<Scanner>>>);

impl ScannerHandle {
    pub(crate) fn get(&self) -> Option<Arc<Scanner>> {
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
    /// Calibración: escala de la focal (achica X e Y al subir) y milímetros por
    /// unidad de profundidad. Opcionales para leer ajustes guardados antes
    #[serde(default = "unit")]
    pub fx_scale: f32,
    #[serde(default = "default_depth_scale")]
    pub depth_scale: f32,
}

fn unit() -> f32 {
    1.0
}

fn default_depth_scale() -> f32 {
    orizon3d_core::camera::DEFAULT_DEPTH_SCALE
}

impl ScanSettingsDto {
    fn apply(self) -> ScanSettings {
        ScanSettings {
            clip_min_mm: self.clip_min_mm,
            clip_max_mm: self.clip_max_mm.max(self.clip_min_mm + 10.0),
            box_mm: self.box_mm.max(0.0),
            clean_noise: self.clean_noise,
            isolate_object: self.isolate_object,
            edge_filter: self.edge_filter,
            temporal_frames: self.temporal_frames.clamp(1, 10),
            fx_scale: self.fx_scale.clamp(0.5, 2.0),
            depth_scale: self.depth_scale.clamp(0.05, 0.2),
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
    /// Cuadros guardados si hay una grabación en curso
    pub recorded: Option<u32>,
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
            recorded: None,
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
        recorded: scanner.recorded_frames(),
    }
}

/// Busca el escáner y empieza a transmitir (reemplaza la conexión anterior)
#[tauri::command]
pub fn scanner_connect(handle: State<'_, ScannerHandle>, settings: ScanSettingsDto) -> ScannerStatusDto {
    let mut lock = handle.0.lock().unwrap();
    // Soltar la conexión anterior libera los /dev/video* antes de reabrirlos
    lock.take();
    *lock = Some(Arc::new(Scanner::connect(settings.apply())));
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
        scanner.set_settings(settings.apply());
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

/// Empieza o termina la grabación de cuadros crudos ("start" / "stop"). Al
/// empezar devuelve la carpeta, dentro de Documentos/Pinocchio/grabaciones; al
/// terminar, la carpeta y cuántos cuadros quedaron
#[tauri::command]
pub fn scanner_record(app: AppHandle, handle: State<'_, ScannerHandle>, action: String) -> Result<RecordingDto, String> {
    let scanner = handle.get().ok_or("El escáner no está conectado")?;
    match action.as_str() {
        "start" => {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let dir = app
                .path()
                .document_dir()
                .map_err(|e| e.to_string())?
                .join("Pinocchio")
                .join("grabaciones")
                .join(format!("escaneo-{stamp}"));
            scanner.start_recording(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
            Ok(RecordingDto { path: dir.display().to_string(), frames: 0 })
        }
        "stop" => {
            let (dir, frames) = scanner.stop_recording().ok_or("No hay una grabación en curso")?;
            Ok(RecordingDto { path: dir.display().to_string(), frames })
        }
        _ => Err(format!("Acción desconocida: {action}")),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingDto {
    pub path: String,
    pub frames: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MeasurementDto {
    pub width_mm: f32,
    pub height_mm: f32,
    pub distance_mm: f32,
    pub points: usize,
}

/// Tamaño del objeto en el cuadro actual, para calibrar
#[tauri::command]
pub fn scanner_measure(handle: State<'_, ScannerHandle>) -> Result<MeasurementDto, String> {
    let scanner = handle.get().ok_or("El escáner no está conectado")?;
    let m = scanner.measure().ok_or("No hay un objeto a la vista dentro del volumen de escaneo")?;
    Ok(MeasurementDto { width_mm: m.width_mm, height_mm: m.height_mm, distance_mm: m.distance_mm, points: m.points })
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MeshSettingsDto {
    pub voxel_mm: f32,
    pub fill: u32,
    pub smooth: u32,
    /// Tapa los agujeros de la malla (también los grandes, como la base que
    /// el escáner no vio)
    #[serde(default)]
    pub close_holes: bool,
    /// Perímetro máximo de un agujero a tapar (mm); 0 = todos
    #[serde(default)]
    pub max_hole_mm: f32,
    /// Parche plano (una base, un corte) en vez de seguir la curvatura del borde
    #[serde(default)]
    pub flat_patch: bool,
    /// Quita las piezas sueltas chicas antes de tapar
    #[serde(default)]
    pub remove_pieces: bool,
}

/// Tapa los agujeros de la malla del escaneo según `settings` (triangulación
/// del borde, refinado a la densidad de la malla y, si no es plano, ajuste a
/// la curvatura). Devuelve la malla y un resumen para el usuario
pub(crate) fn close_holes(mesh: orizon3d_core::Mesh, settings: &MeshSettingsDto) -> Result<(orizon3d_core::Mesh, String), String> {
    use orizon3d_core::scan::VoxelIndex;
    use pinocchio_math::Vector3;

    let positions = mesh.vertices.iter().map(|v| Vector3::new(v[0] as f64, v[1] as f64, v[2] as f64)).collect();
    let triangles = mesh.tris.iter().map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
    let mut tri = pinocchio_repair::TriMesh::new(positions, triangles);
    // El perímetro se pasa a aristas con el largo típico de la malla (≈ el vóxel)
    let edge = settings.voxel_mm.max(0.5);
    let config = pinocchio_repair::RepairConfig {
        remove_small_components: settings.remove_pieces,
        small_component_ratio: 0.02,
        fill_holes: true,
        hole_fill_config: pinocchio_repair::HoleFillConfig {
            max_hole_edges: if settings.max_hole_mm > 0.0 { (settings.max_hole_mm / edge).ceil().max(3.0) as usize } else { 0 },
            refine: true,
            fair: !settings.flat_patch,
        },
        ..Default::default()
    };
    let summary = pinocchio_repair::repair_trimesh(&mut tri, &config).map_err(|e| format!("No se pudieron tapar los agujeros: {e}"))?;

    // Color de cada vértice: el del vértice original más cercano (los del
    // parche toman el del borde); si no hay ninguno cerca, el promedio
    let original: Vec<[f32; 3]> = mesh.vertices.clone();
    let has_color = mesh.colors.len() == mesh.vertices.len() && !mesh.colors.is_empty();
    let mean = if has_color {
        let n = mesh.colors.len() as f64;
        let s = mesh.colors.iter().fold([0.0f64; 3], |a, c| [a[0] + c[0] as f64, a[1] + c[1] as f64, a[2] + c[2] as f64]);
        [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
    } else {
        [200; 3]
    };
    let radii = [2.0 * edge, 8.0 * edge, 32.0 * edge];
    let indices: Vec<VoxelIndex> = if has_color { radii.iter().map(|&r| VoxelIndex::build(original.clone(), r)).collect() } else { Vec::new() };
    let vertices: Vec<[f32; 3]> = tri.positions.iter().map(|p| [p.x() as f32, p.y() as f32, p.z() as f32]).collect();
    let colors = if has_color {
        vertices
            .iter()
            .map(|&v| {
                indices.iter().zip(radii).find_map(|(index, r)| index.nearest(v, r)).map_or(mean, |(i, _)| mesh.colors[i])
            })
            .collect()
    } else {
        Vec::new()
    };
    let tris = tri.triangles.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
    let mut note = match summary.holes_filled {
        0 => "No había agujeros para tapar".to_string(),
        1 => "1 agujero tapado".to_string(),
        n => format!("{n} agujeros tapados"),
    };
    if summary.holes_skipped > 0 {
        note += &format!(", {} sin tapar (más grandes que el límite o con borde irregular)", summary.holes_skipped);
    }
    if summary.components_removed > 0 {
        note += &format!(", {} piezas sueltas quitadas", summary.components_removed);
    }
    Ok((orizon3d_core::Mesh { vertices, colors, tris }, note))
}

/// Malla de la nube con los ajustes del panel (reconstrucción y, si se
/// pidió, agujeros tapados), avisando el avance
pub(crate) fn finish_mesh(mesh: orizon3d_core::Mesh, settings: &MeshSettingsDto, on_progress: &Channel<Progress>) -> Result<orizon3d_core::Mesh, String> {
    if mesh.is_empty() {
        return Err("La malla salió vacía: la nube tiene muy pocos puntos o el detalle es muy fino".into());
    }
    if !settings.close_holes {
        return Ok(mesh);
    }
    report(on_progress, "meshing", 40, "Tapando agujeros...");
    let (mesh, note) = close_holes(mesh, settings)?;
    report(on_progress, "meshing", 60, note);
    Ok(mesh)
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
        let mesh = finish_mesh(mesh, &settings, &on_progress)?;
        load_scan(&mesh, &on_progress, state)
    })
    .await
}

/// Carga la malla del escaneo como modelo de trabajo, con su color como piel
pub(crate) fn load_scan(mesh: &orizon3d_core::Mesh, progress: &Channel<Progress>, state: &AppState) -> Result<MeshInfo, String> {
    let name = "Escaneo".to_string();
    report(progress, "texture", 30, "Desplegando UV y horneando el color del escaneo...");
    let scene = scan_to_skinned_scene(mesh, &name);
    load_scene(scene, name, "Escáner".into(), progress, state)
}

/// Lado de la textura con el color del escaneo (px)
const SCAN_TEXTURE_SIZE: u32 = 2048;

/// Como [`scan_to_scene`], con el color de los vértices horneado en una
/// textura sobre UV nuevas: el escaneo llega con su piel, lista para pintar y
/// editar como la de cualquier modelo importado (y la reparación y la
/// retopología la conservan). Sin color, o si no se puede desplegar, queda la
/// escena sin UV.
pub(crate) fn scan_to_skinned_scene(mesh: &orizon3d_core::Mesh, name: &str) -> Scene {
    let scene = scan_to_scene(mesh, name);
    if mesh.colors.len() != mesh.vertices.len() || mesh.tris.is_empty() {
        return scene;
    }
    let Ok(work) = crate::commands::scene_to_pinocchio_mesh(&scene) else { return scene };
    let positions: Vec<[f64; 3]> = work.vertices.iter().map(|v| [v.position.x(), v.position.y(), v.position.z()]).collect();
    let faces: Vec<[usize; 3]> = (0..work.num_faces()).map(|i| work.get_face_vertices(i)).collect();
    let options = uv_core::BakeOptions {
        texture_size: SCAN_TEXTURE_SIZE,
        // Islas legibles para pintar, como el valor de fábrica del paso UV
        unwrap: uv_core::UnwrapOptions {
            charts: uv_core::ChartOptions { max_angle: 55.0, ..Default::default() },
            layout: uv_core::Layout::Paintable,
            ..Default::default()
        },
    };
    let mut skin = uv_core::unwrapped_skin(&scene, None, &positions, &faces, &options);
    let Some(material) = skin.materials.first_mut().filter(|m| m.base_color_texture.is_some()) else { return scene };
    material.name = name.to_string();
    // La normal horneada de la misma malla es plana: no aporta y pesa
    if let Some(normal) = material.normal_texture.take() {
        skin.textures.remove(normal.texture_index);
        for m in &mut skin.materials {
            for r in [&mut m.base_color_texture, &mut m.metallic_roughness_texture, &mut m.occlusion_texture, &mut m.emissive_texture]
                .into_iter()
                .flatten()
            {
                if r.texture_index > normal.texture_index {
                    r.texture_index -= 1;
                }
            }
        }
    }
    for t in &mut skin.textures {
        t.name = format!("{name} color");
    }
    let (mut skinned, _) = uv_core::skin_scene(&positions, &faces, Some(&skin), &scene);
    for m in &mut skinned.meshes {
        m.name = name.to_string();
    }
    for n in &mut skinned.nodes {
        n.name = name.to_string();
    }
    skinned
}

/// Malla del escáner (cámara: X derecha, Y abajo, Z adelante, mm) a escena
/// con Y arriba, centrada y apoyada en el piso
pub(crate) fn scan_to_scene(mesh: &orizon3d_core::Mesh, name: &str) -> Scene {
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
        // La cámara da sRGB; los colores de vértice de la escena (glTF) son lineales
        let linear = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let colors = mesh.colors.iter().map(|c| [linear(c[0]), linear(c[1]), linear(c[2]), 1.0]).collect();
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

    /// Media esfera vista desde una cámara, como sale de un escaneo de un cuadro
    fn scanned_half_sphere() -> orizon3d_core::Mesh {
        use orizon3d_core::pointcloud::{Point, PointCloud};
        let (r, c) = (40.0f32, [0.0f32, 0.0, 300.0]);
        let mut points = Vec::new();
        let n = 120;
        for a in 0..n {
            let th = std::f32::consts::PI * (a as f32 + 0.5) / n as f32;
            let m = ((2.0 * n as f32 * th.sin()) as i32).max(1);
            for b in 0..m {
                let ph = 2.0 * std::f32::consts::PI * b as f32 / m as f32;
                let nrm = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
                if nrm[2] > 0.0 {
                    continue;
                }
                points.push(Point { x: c[0] + r * nrm[0], y: c[1] + r * nrm[1], z: c[2] + r * nrm[2], rgb: [180, 120, 90], view: [0.0; 3] });
            }
        }
        orizon3d_core::mesh::reconstruct(&PointCloud { points, has_color: true }, 2.0, 1, 2)
    }

    #[test]
    fn scan_mesh_can_be_repaired() {
        let mesh = scanned_half_sphere();
        assert!(!mesh.is_empty());
        let scene = scan_to_scene(&mesh, "Escaneo");
        let mut m = crate::commands::scene_to_pinocchio_mesh(&scene).unwrap();
        let _ = pinocchio_repair::analyze(&m, &pinocchio_repair::AnalysisConfig::default());
        pinocchio_repair::repair_all(&mut m, &pinocchio_repair::RepairConfig::default()).unwrap();
    }

    /// Escaneo sucio: esfera, mesa, ruido y restos sueltos
    fn scanned_messy() -> orizon3d_core::Mesh {
        use orizon3d_core::pointcloud::{Point, PointCloud};
        let mut seed = 12345u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 1_000_000) as f32 / 1_000_000.0
        };
        let mut points = Vec::new();
        let mut push = |x: f32, y: f32, z: f32| points.push(Point { x, y, z, rgb: [150, 140, 130], view: [0.0; 3] });
        for _ in 0..40000 {
            let u = rnd() * 2.0 - 1.0;
            let t = rnd() * std::f32::consts::TAU;
            let r = (1.0 - u * u).sqrt();
            let n = [r * t.cos(), u, r * t.sin()];
            if n[2] > 0.3 {
                continue;
            }
            let e = (rnd() - 0.5) * 1.5;
            push(n[0] * (40.0 + e), n[1] * (40.0 + e), 300.0 + n[2] * (40.0 + e));
        }
        for _ in 0..20000 {
            push(rnd() * 200.0 - 100.0, 40.0 + (rnd() - 0.5), 200.0 + rnd() * 200.0);
        }
        for _ in 0..800 {
            push(rnd() * 200.0 - 100.0, rnd() * 100.0 - 60.0, 200.0 + rnd() * 200.0);
        }
        orizon3d_core::mesh::reconstruct(&PointCloud { points, has_color: true }, 2.0, 1, 2)
    }

    #[test]
    fn messy_scan_can_be_repaired_and_retopologized() {
        let scene = scan_to_scene(&scanned_messy(), "Escaneo");
        let mut m = crate::commands::scene_to_pinocchio_mesh(&scene).unwrap();
        let config = pinocchio_repair::AnalysisConfig { check_self_intersections: true, ..Default::default() };
        let _ = pinocchio_repair::analyze(&m, &config);
        let q = quadriflow_core::RemeshConfig { target_faces: 3000, ..Default::default() };
        quadriflow_core::remesh(&m, &q).unwrap();
        let quads = quadriflow_core::remesh(&m, &q).unwrap();
        let _ = quadriflow_core::quality::analyze(&quads, Some(&m));
        pinocchio_repair::repair_all(&mut m, &app_repair_config()).unwrap();
        let quads = quadriflow_core::remesh(&m, &q).unwrap();
        let _ = quadriflow_core::quality::analyze(&quads, Some(&m));
    }

    /// La reparación como la pide el panel con sus valores de fábrica
    fn app_repair_config() -> pinocchio_repair::RepairConfig {
        pinocchio_repair::RepairConfig {
            remove_small_components: true,
            fill_holes: true,
            hole_fill_config: pinocchio_repair::HoleFillConfig { max_hole_edges: 0, refine: true, fair: true },
            ..Default::default()
        }
    }

    fn hole_settings(flat: bool) -> MeshSettingsDto {
        MeshSettingsDto { voxel_mm: 2.0, fill: 1, smooth: 2, close_holes: true, max_hole_mm: 0.0, flat_patch: flat, remove_pieces: true }
    }

    /// Aristas de borde (usadas por una sola cara)
    fn open_edges(mesh: &orizon3d_core::Mesh) -> usize {
        let mut count = std::collections::HashMap::new();
        for t in &mesh.tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        count.values().filter(|&&c| c == 1).count()
    }

    #[test]
    fn giant_hole_of_a_scan_gets_closed() {
        let mesh = scanned_half_sphere();
        assert!(open_edges(&mesh) > 20, "la media esfera debe venir abierta");
        for flat in [false, true] {
            let (closed, note) = close_holes(scanned_half_sphere(), &hole_settings(flat)).unwrap();
            assert!(note.contains("tapado"), "{note}");
            assert_eq!(open_edges(&closed), 0, "quedaron bordes abiertos ({note})");
            assert_eq!(closed.colors.len(), closed.vertices.len());
        }
        // Con un límite chico, la base queda abierta
        let small = MeshSettingsDto { max_hole_mm: 10.0, ..hole_settings(false) };
        let (open, _) = close_holes(mesh, &small).unwrap();
        assert!(open_edges(&open) > 0);
    }

    #[test]
    fn half_sphere_scan_with_app_repair() {
        let scene = scan_to_scene(&scanned_half_sphere(), "Escaneo");
        let mut m = crate::commands::scene_to_pinocchio_mesh(&scene).unwrap();
        pinocchio_repair::repair_all(&mut m, &app_repair_config()).unwrap();
    }

    #[test]
    fn scan_mesh_can_be_retopologized() {
        let scene = scan_to_scene(&scanned_half_sphere(), "Escaneo");
        let m = crate::commands::scene_to_pinocchio_mesh(&scene).unwrap();
        for rebuild in [quadriflow_core::Rebuild::Auto, quadriflow_core::Rebuild::Always] {
            let config = quadriflow_core::RemeshConfig { target_faces: 2000, rebuild, ..Default::default() };
            quadriflow_core::remesh(&m, &config).unwrap();
        }
    }

    #[test]
    fn scan_color_arrives_as_a_skin() {
        let scene = scan_to_skinned_scene(&scanned_half_sphere(), "Escaneo");
        assert_eq!(scene.materials.len(), 1);
        let material = &scene.materials[0];
        assert_eq!(material.name, "Escaneo");
        assert!(material.normal_texture.is_none());
        let texture = &scene.textures[material.base_color_texture.as_ref().unwrap().texture_index];
        assert_eq!(scene.textures.len(), 1);
        for prim in &scene.meshes[0].primitives {
            assert_eq!(prim.material, Some(0));
            assert!(prim.attributes.iter().any(|a| matches!(a, VertexAttribute::TexCoords(0, _))));
            // El color quedó en la textura: con colores de vértice se multiplicaría dos veces
            assert!(!prim.attributes.iter().any(|a| matches!(a, VertexAttribute::Colors(_))));
        }
        // La textura tiene el color del escaneo donde caen las caras
        let image = image::load_from_memory(&texture.data).unwrap().to_rgba8();
        let painted: Vec<_> = image.pixels().filter(|p| p.0[3] > 0 && p.0 != [255, 255, 255, 255]).collect();
        assert!(!painted.is_empty());
        let near = painted.iter().filter(|p| {
            let c = p.0;
            (c[0] as i32 - 180).abs() < 8 && (c[1] as i32 - 120).abs() < 8 && (c[2] as i32 - 90).abs() < 8
        });
        assert!(near.count() * 10 >= painted.len() * 9);
    }

    #[test]
    fn messy_scan_color_arrives_as_a_skin() {
        let scene = scan_to_skinned_scene(&scanned_messy(), "Escaneo");
        assert!(scene.materials[0].base_color_texture.is_some());
        assert!(scene.validate().is_ok());
    }

    #[test]
    fn scan_without_color_stays_without_skin() {
        let mut mesh = scanned_half_sphere();
        mesh.colors.clear();
        let scene = scan_to_skinned_scene(&mesh, "Escaneo");
        assert!(scene.materials.is_empty());
    }

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
