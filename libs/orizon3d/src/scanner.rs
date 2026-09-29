//! Sesión con el escáner: arranca la captura, guarda el último cuadro para la
//! vista previa, integra los cuadros al escaneo mientras está activo y arma la
//! malla. Es lo que en Orizon3D hacía la ventana egui (`app.rs`), sin la GUI.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};

use crate::camera::{self, CameraDescription, DepthFrame, Frames, StreamInfo};
use crate::capture::{Capture, DepthControls, Status};
use crate::mesh::{self, Mesh};
use crate::pointcloud::{self, CloudParams, PointCloud, Roi};
use crate::scan::{self, ScanSession, ScanStats};

/// Volumen de escaneo, limpieza y calibración fina.
#[derive(Debug, Clone, Copy)]
pub struct ScanSettings {
    /// Rango de profundidad (mm): lo que queda fuera es fondo
    pub clip_min_mm: f32,
    pub clip_max_mm: f32,
    /// Ancho de la caja lateral (mm) centrada en el eje de la cámara; 0 = sin límite
    pub box_mm: f32,
    /// Quita puntos sueltos
    pub clean_noise: bool,
    /// Conserva solo el grupo conexo más grande (el objeto)
    pub isolate_object: bool,
    /// Quita píxeles voladores en los bordes de profundidad
    pub edge_filter: bool,
    /// Mediana temporal de los últimos N mapas de profundidad (1 = sin suavizado)
    pub temporal_frames: usize,
    /// Ajuste fino de la focal (escala XY) sobre el FOV nominal
    pub fx_scale: f32,
    /// Milímetros por unidad del stream Y16
    pub depth_scale: f32,
}

impl Default for ScanSettings {
    fn default() -> Self {
        // Objeto cercano (~15–35 cm): corta la pared más allá de ~45 cm y los
        // lados a 30 cm, así el objeto es el grupo mayor y se aísla solo
        ScanSettings {
            clip_min_mm: 120.0,
            clip_max_mm: 450.0,
            box_mm: 300.0,
            clean_noise: true,
            isolate_object: true,
            edge_filter: true,
            temporal_frames: 3,
            fx_scale: 1.0,
            depth_scale: camera::DEFAULT_DEPTH_SCALE,
        }
    }
}

/// Parámetros de la reconstrucción de malla.
#[derive(Debug, Clone, Copy)]
pub struct MeshSettings {
    /// Tamaño de vóxel (mm): menor = más detalle
    pub voxel_mm: f32,
    /// Pasadas de relleno de huecos
    pub fill: u32,
    /// Pasadas de suavizado
    pub smooth: u32,
}

impl Default for MeshSettings {
    fn default() -> Self {
        MeshSettings { voxel_mm: 2.0, fill: 1, smooth: 2 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScannerState {
    Connecting,
    Streaming,
    Error(String),
    Stopped,
}

/// Lo que la interfaz muestra del escáner.
#[derive(Debug, Clone)]
pub struct ScannerStatus {
    pub state: ScannerState,
    pub device: Option<CameraDescription>,
    pub depth: Option<StreamInfo>,
    pub rgb: Option<StreamInfo>,
    pub fps: f32,
    /// Distancia en la ventana central del cuadro (cm); 0 = sin dato
    pub distance_cm: f32,
    /// Fracción de píxeles con profundidad válida
    pub coverage: f32,
    pub scanning: bool,
    pub stats: ScanStats,
    /// Puntos fusionados del escaneo
    pub points: usize,
    /// El último cuadro se alineó con el modelo
    pub tracking_ok: bool,
}

/// Imagen RGBA para la vista previa.
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

struct Shared {
    status: Mutex<ScannerStatus>,
    params: Mutex<Option<CloudParams>>,
    latest: Mutex<Option<Frames>>,
    history: Mutex<VecDeque<Vec<u16>>>,
    settings: Mutex<ScanSettings>,
    session: Mutex<Option<ScanSession>>,
    scanning: AtomicBool,
}

/// Escáner conectado. Al soltarlo se detiene la captura.
pub struct Scanner {
    capture: Option<Capture>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Scanner {
    /// Busca el escáner y arranca la captura en segundo plano. Los errores
    /// (sin escáner, sin permisos) llegan por [`Scanner::status`].
    pub fn connect(settings: ScanSettings) -> Self {
        let shared = Arc::new(Shared {
            status: Mutex::new(ScannerStatus {
                state: ScannerState::Connecting,
                device: None,
                depth: None,
                rgb: None,
                fps: 0.0,
                distance_cm: 0.0,
                coverage: 0.0,
                scanning: false,
                stats: ScanStats::default(),
                points: 0,
                tracking_ok: true,
            }),
            params: Mutex::new(None),
            latest: Mutex::new(None),
            history: Mutex::new(VecDeque::new()),
            settings: Mutex::new(settings),
            session: Mutex::new(None),
            scanning: AtomicBool::new(false),
        });
        let capture = Capture::start(|| {});
        let (frames, status) = (capture.frames.clone(), capture.status.clone());
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("orizon3d-scanner".into())
            .spawn(move || worker_loop(&worker_shared, &frames, &status))
            .expect("no se pudo crear el hilo del escáner");
        Scanner { capture: Some(capture), shared, worker: Some(worker) }
    }

    pub fn status(&self) -> ScannerStatus {
        let mut status = self.shared.status.lock().unwrap().clone();
        status.scanning = self.shared.scanning.load(Ordering::SeqCst);
        status
    }

    pub fn settings(&self) -> ScanSettings {
        *self.shared.settings.lock().unwrap()
    }

    pub fn set_settings(&self, settings: ScanSettings) {
        *self.shared.settings.lock().unwrap() = settings;
    }

    /// Exposición y ganancia del sensor de profundidad
    pub fn set_depth_controls(&self, controls: DepthControls) {
        if let Some(capture) = &self.capture {
            capture.set_depth_controls(controls);
        }
    }

    /// Mapa de profundidad coloreado: rojo cerca, azul lejos, negro sin dato
    pub fn preview_depth(&self) -> Option<Preview> {
        let latest = self.shared.latest.lock().unwrap();
        let depth = &latest.as_ref()?.depth;
        Some(colorize_depth(depth))
    }

    /// Imagen de color reducida a `max_width` de ancho como mucho
    pub fn preview_color(&self, max_width: u32) -> Option<Preview> {
        let latest = self.shared.latest.lock().unwrap();
        let rgb = latest.as_ref()?.rgb.as_ref()?;
        let step = rgb.width.div_ceil(max_width.max(1)).max(1);
        let (w, h) = (rgb.width / step, rgb.height / step);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let i = (((y * step) * rgb.width + x * step) * 3) as usize;
                rgba.extend_from_slice(&rgb.rgb[i..i + 3]);
                rgba.push(255);
            }
        }
        Some(Preview { width: w, height: h, rgba })
    }

    /// Empieza un escaneo nuevo: desde ahora cada cuadro se alinea y se fusiona
    pub fn start_scan(&self) {
        *self.shared.session.lock().unwrap() = Some(ScanSession::new());
        self.shared.scanning.store(true, Ordering::SeqCst);
    }

    /// Pausa el escaneo; lo fusionado se conserva para mallarlo
    pub fn stop_scan(&self) {
        self.shared.scanning.store(false, Ordering::SeqCst);
    }

    /// Descarta el escaneo: la malla vuelve a salir del cuadro actual
    pub fn reset_scan(&self) {
        self.shared.scanning.store(false, Ordering::SeqCst);
        *self.shared.session.lock().unwrap() = None;
        let mut status = self.shared.status.lock().unwrap();
        status.stats = ScanStats::default();
        status.points = 0;
        status.tracking_ok = true;
    }

    /// Nube limpia: la fusionada del escaneo si hay, o la del cuadro actual
    pub fn cloud(&self) -> Option<PointCloud> {
        let settings = self.settings();
        let fused = self.shared.session.lock().unwrap().as_ref().filter(|s| s.point_count() > 0).map(|s| s.fused_cloud());
        let base = match fused {
            Some(cloud) => cloud,
            None => {
                let params = effective_params(&self.shared, &settings)?;
                let latest = self.shared.latest.lock().unwrap();
                let frames = latest.as_ref()?;
                let smoothed = smoothed_depth(&self.shared, &frames.depth, &settings);
                PointCloud::generate(smoothed.as_ref().unwrap_or(&frames.depth), frames.rgb.as_ref(), &params)
            }
        };
        Some(clean(base, &settings))
    }

    /// Reconstruye la malla (coordenadas de cámara, mm) desde [`Scanner::cloud`]
    pub fn build_mesh(&self, settings: &MeshSettings) -> Result<Mesh, String> {
        let cloud = self.cloud().ok_or("Todavía no llega ningún cuadro del escáner")?;
        if cloud.points.is_empty() {
            return Err("No hay puntos dentro del volumen de escaneo".into());
        }
        let mesh = mesh::reconstruct(&cloud, settings.voxel_mm.max(1.0), settings.fill, settings.smooth);
        if mesh.is_empty() {
            return Err("La malla salió vacía: la nube tiene muy pocos puntos".into());
        }
        Ok(mesh)
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        // Detener la captura cierra el canal de cuadros y termina el hilo propio
        self.capture.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(shared: &Shared, frames: &Receiver<Frames>, status: &Receiver<Status>) {
    let mut fps_frames = 0u32;
    let mut fps_since = Instant::now();
    loop {
        for s in status.try_iter() {
            apply_status(shared, s);
        }
        let frame = match frames.recv_timeout(Duration::from_millis(100)) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                for s in status.try_iter() {
                    apply_status(shared, s);
                }
                let mut st = shared.status.lock().unwrap();
                if st.state == ScannerState::Streaming {
                    st.state = ScannerState::Stopped;
                }
                return;
            }
        };

        fps_frames += 1;
        let elapsed = fps_since.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            shared.status.lock().unwrap().fps = fps_frames as f32 / elapsed;
            fps_frames = 0;
            fps_since = Instant::now();
        }

        let settings = *shared.settings.lock().unwrap();
        push_history(shared, &frame.depth, &settings);
        let (distance_cm, coverage) = depth_stats(&frame.depth, settings.depth_scale);
        {
            let mut st = shared.status.lock().unwrap();
            st.distance_cm = distance_cm;
            st.coverage = coverage;
        }

        if shared.scanning.load(Ordering::SeqCst) {
            integrate(shared, &frame, &settings);
        }
        *shared.latest.lock().unwrap() = Some(frame);
    }
}

fn apply_status(shared: &Shared, status: Status) {
    let mut st = shared.status.lock().unwrap();
    match status {
        Status::Connecting => st.state = ScannerState::Connecting,
        Status::Streaming { info, depth, rgb, params } => {
            st.state = ScannerState::Streaming;
            st.device = Some(info);
            st.depth = Some(depth);
            st.rgb = rgb;
            *shared.params.lock().unwrap() = Some(params);
        }
        Status::Error(e) => st.state = ScannerState::Error(e),
        Status::Stopped => st.state = ScannerState::Stopped,
    }
}

/// Alinea el cuadro con lo escaneado y lo fusiona
fn integrate(shared: &Shared, frames: &Frames, settings: &ScanSettings) {
    let Some(params) = effective_params(shared, settings) else { return };
    let smoothed = smoothed_depth(shared, &frames.depth, settings);
    let cloud = PointCloud::generate(smoothed.as_ref().unwrap_or(&frames.depth), frames.rgb.as_ref(), &params);
    let cloud = clean(cloud, settings);
    let mut session = shared.session.lock().unwrap();
    let Some(session) = session.as_mut() else { return };
    let ok = session.integrate_frame(&cloud);
    let mut st = shared.status.lock().unwrap();
    st.tracking_ok = ok;
    st.stats = session.stats;
    st.points = session.point_count();
}

/// Parámetros del stream con la calibración fina y el volumen como caja
fn effective_params(shared: &Shared, settings: &ScanSettings) -> Option<CloudParams> {
    let mut p = (*shared.params.lock().unwrap())?;
    let s = settings.fx_scale.max(0.05);
    p.depth_intr.fx *= s;
    p.depth_intr.fy *= s;
    if let Some(ri) = &mut p.rgb_intr {
        ri.fx *= s;
        ri.fy *= s;
    }
    p.depth_scale = settings.depth_scale.max(1e-4);
    p.clip_min_mm = 0.0;
    p.clip_max_mm = 0.0;
    let half = if settings.box_mm > 0.0 { settings.box_mm * 0.5 } else { f32::INFINITY };
    p.roi = Some(Roi {
        min: [-half, -half, settings.clip_min_mm.max(0.0)],
        max: [half, half, settings.clip_max_mm.max(0.0)],
    });
    p.edge_filter = settings.edge_filter;
    Some(p)
}

fn clean(cloud: PointCloud, settings: &ScanSettings) -> PointCloud {
    if settings.clean_noise || settings.isolate_object {
        let min_pts = if settings.clean_noise { 3 } else { 1 };
        scan::clean_cloud(&cloud, 3.0, min_pts, settings.isolate_object)
    } else {
        cloud
    }
}

/// Guarda los últimos mapas para la mediana temporal
fn push_history(shared: &Shared, depth: &DepthFrame, settings: &ScanSettings) {
    let mut history = shared.history.lock().unwrap();
    if history.back().is_some_and(|last| last.len() != depth.depth.len()) {
        history.clear();
    }
    history.push_back(depth.depth.clone());
    while history.len() > settings.temporal_frames.max(1) {
        history.pop_front();
    }
}

/// Mediana temporal de los últimos cuadros (baja el ruido del sensor en reposo);
/// `None` si el suavizado está apagado o aún no hay historial
fn smoothed_depth(shared: &Shared, depth: &DepthFrame, settings: &ScanSettings) -> Option<DepthFrame> {
    let history = shared.history.lock().unwrap();
    if settings.temporal_frames < 2 || history.len() < 2 {
        return None;
    }
    let maps: Vec<&[u16]> = history.iter().map(|v| v.as_slice()).collect();
    // Un píxel sobrevive si es válido en al menos la mitad de los cuadros
    let min_valid = (maps.len() / 2).max(1);
    Some(DepthFrame {
        width: depth.width,
        height: depth.height,
        depth: pointcloud::temporal_median(&maps, depth.width as usize, depth.height as usize, min_valid),
        timestamp_ms: depth.timestamp_ms,
    })
}

/// Distancia (cm) en la ventana central del cuadro y cobertura
fn depth_stats(depth: &DepthFrame, scale: f32) -> (f32, f32) {
    let (w, h) = (depth.width as usize, depth.height as usize);
    let valid = depth.depth.iter().filter(|&&v| v != 0).count();
    let coverage = valid as f32 / depth.depth.len().max(1) as f32;
    if w == 0 || h == 0 || depth.depth.len() < w * h {
        return (0.0, coverage);
    }
    let (mut sum, mut count) = (0.0f64, 0usize);
    for y in h * 2 / 5..(h * 3 / 5).max(h * 2 / 5 + 1) {
        for x in w * 2 / 5..(w * 3 / 5).max(w * 2 / 5 + 1) {
            let d = depth.depth[y * w + x];
            if d != 0 {
                sum += d as f64;
                count += 1;
            }
        }
    }
    let distance = if count > 0 { (sum / count as f64) as f32 * scale.max(1e-4) / 10.0 } else { 0.0 };
    (distance, coverage)
}

fn colorize_depth(depth: &DepthFrame) -> Preview {
    let (mut min, mut max) = (u16::MAX, u16::MIN);
    for &v in depth.depth.iter().filter(|&&v| v != 0) {
        min = min.min(v);
        max = max.max(v);
    }
    let span = if max > min { (max - min) as f32 } else { 1.0 };
    let n = (depth.width * depth.height) as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    for &v in depth.depth.iter().take(n) {
        if v == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 255]);
        } else {
            // Rampa tipo jet: cerca rojo, lejos azul
            let t = ((v - min) as f32 / span).clamp(0.0, 1.0);
            let [r, g, b] = hsv_to_rgb(t * 240.0);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Preview { width: depth.width, height: depth.height, rgba }
}

/// Tono en grados con saturación y valor máximos
fn hsv_to_rgb(h: f32) -> [u8; 3] {
    let hp = h / 60.0;
    let x = 1.0 - (hp % 2.0 - 1.0).abs();
    let (r, g, b) = match hp as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(values: Vec<u16>, width: u32, height: u32) -> DepthFrame {
        DepthFrame { width, height, depth: values, timestamp_ms: 0.0 }
    }

    #[test]
    fn colorize_marks_missing_black_and_near_red() {
        let preview = colorize_depth(&frame(vec![0, 1000, 2000, 3000], 2, 2));
        assert_eq!(&preview.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&preview.rgba[4..8], &[255, 0, 0, 255]);
        assert_eq!(&preview.rgba[12..16], &[0, 0, 255, 255]);
    }

    #[test]
    fn depth_stats_uses_center_window() {
        let mut values = vec![0u16; 10 * 10];
        for y in 4..6 {
            for x in 4..6 {
                values[y * 10 + x] = 3000;
            }
        }
        let (distance, coverage) = depth_stats(&frame(values, 10, 10), 0.1);
        assert!((distance - 30.0).abs() < 1e-3);
        assert!((coverage - 0.04).abs() < 1e-6);
    }
}
