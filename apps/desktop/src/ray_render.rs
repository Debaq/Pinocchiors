//! Render con GeppettoRay: trabajo en segundo plano con progreso, vista previa que se
//! afina por pasadas y cancelación. La interfaz pregunta por el estado cada rato y baja
//! la imagen (PNG) cuando cambia.

use crate::state::AppState;
use geppettoray as gr;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct RayRender {
    job: Mutex<Option<Job>>,
}

struct Job {
    cancel: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    #[default]
    Idle,
    /// Armando la escena (BVH, texturas)
    Preparing,
    Running,
    Done,
    Cancelled,
    Error,
}

#[derive(Default)]
struct Status {
    phase: Phase,
    pass: u32,
    passes: u32,
    samples: u32,
    total_samples: u32,
    elapsed_secs: f64,
    eta_secs: f64,
    /// Sube con cada imagen nueva: la interfaz solo la baja si cambió
    version: u64,
    png: Option<Vec<u8>>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    phase: Phase,
    pass: u32,
    passes: u32,
    samples: u32,
    total_samples: u32,
    elapsed_secs: f64,
    eta_secs: f64,
    version: u64,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDto {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    /// Campo de visión vertical de la imagen, en grados
    pub vfov_deg: f32,
}

/// Luz direccional: `direction` apunta hacia la luz; `color` es la irradiancia
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SunDto {
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RayRequest {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub passes: u32,
    pub max_depth: u32,
    pub denoise: bool,
    /// Fondo transparente (alfa 0 donde la cámara ve el fondo)
    pub transparent: bool,
    /// Color lineal del fondo visible; sin él se ve el entorno
    pub background: Option<[f32; 3]>,
    pub camera: CameraDto,
    pub suns: Vec<SunDto>,
    /// Radiancia lineal del entorno uniforme que ilumina
    pub ambient: [f32; 3],
}

fn encode_png(frame: &gr::Frame) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(&frame.to_srgb8_rgba(), frame.image.width, frame.image.height, image::ExtendedColorType::Rgba8)
    .map_err(|e| e.to_string())?;
    Ok(out)
}

fn settings_from(req: &RayRequest) -> gr::Settings {
    gr::Settings {
        width: req.width,
        height: req.height,
        samples: req.samples,
        max_depth: req.max_depth,
        environment: gr::Environment::uniform(Vec3::from(req.ambient)),
        lights: req
            .suns
            .iter()
            .map(|s| gr::Light::Sun(gr::Sun { direction: Vec3::from(s.direction), color: Vec3::from(s.color) }))
            .collect(),
        transparent_background: req.transparent,
        background: req.background.map(Vec3::from),
        ..Default::default()
    }
}

fn camera_from(req: &RayRequest) -> gr::Camera {
    gr::Camera::look_at(
        Vec3::from(req.camera.position),
        Vec3::from(req.camera.target),
        Vec3::from(req.camera.up),
        req.camera.vfov_deg,
        req.width as f32 / req.height.max(1) as f32,
    )
}

fn run(scene: converter_scene::Scene, req: RayRequest, cancel: Arc<AtomicBool>, status: Arc<Mutex<Status>>) {
    let set = |f: &mut dyn FnMut(&mut Status)| f(&mut status.lock().unwrap());
    let ray_scene = gr::RayScene::from_scene(&scene);
    drop(scene);
    if ray_scene.triangles.is_empty() {
        set(&mut |s| {
            s.phase = Phase::Error;
            s.error = Some("La escena no tiene geometría para renderizar".to_string());
        });
        return;
    }
    let settings = settings_from(&req);
    let camera = camera_from(&req);
    let options = gr::ProgressiveOptions {
        passes: req.passes,
        denoise: req.denoise.then(gr::DenoiseSettings::default),
        ..Default::default()
    };
    set(&mut |s| {
        s.phase = Phase::Running;
        s.passes = req.passes.clamp(1, req.samples.max(1));
        s.total_samples = req.samples.max(1);
    });
    let outcome = gr::render_progressive(&ray_scene, &camera, &settings, &options, &cancel, |info, frame| {
        let png = encode_png(frame);
        set(&mut |s| {
            s.pass = info.pass;
            s.passes = info.passes;
            s.samples = info.samples;
            s.total_samples = info.total_samples;
            s.elapsed_secs = info.elapsed_secs;
            s.eta_secs = info.eta_secs;
            match &png {
                Ok(bytes) => {
                    s.png = Some(bytes.clone());
                    s.version += 1;
                }
                Err(e) => s.error = Some(e.clone()),
            }
        });
    });
    let final_png = encode_png(&outcome.frame);
    set(&mut |s| {
        match &final_png {
            Ok(bytes) if outcome.samples > 0 => {
                s.png = Some(bytes.clone());
                s.version += 1;
            }
            Err(e) => s.error = Some(e.clone()),
            _ => {}
        }
        s.samples = outcome.samples;
        s.phase = if s.error.is_some() {
            Phase::Error
        } else if outcome.cancelled {
            Phase::Cancelled
        } else {
            Phase::Done
        };
    });
}

fn validate(req: &RayRequest) -> Result<(), String> {
    if req.width == 0 || req.height == 0 || req.width.max(req.height) > 16384 {
        return Err("El tamaño del render no es válido".to_string());
    }
    if req.samples == 0 {
        return Err("Hacen falta muestras para renderizar".to_string());
    }
    Ok(())
}

/// Empieza un render en segundo plano. Falla si ya hay uno corriendo.
#[tauri::command]
pub fn ray_render_start(app: AppHandle, jobs: State<'_, RayRender>, request: RayRequest) -> Result<(), String> {
    validate(&request)?;
    let mut slot = jobs.job.lock().unwrap();
    if let Some(job) = slot.as_ref() {
        if matches!(job.status.lock().unwrap().phase, Phase::Preparing | Phase::Running) {
            return Err("Ya hay un render en curso".to_string());
        }
    }
    let scene = app
        .state::<AppState>()
        .scene
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No hay ningún modelo cargado".to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(Status { phase: Phase::Preparing, ..Default::default() }));
    *slot = Some(Job { cancel: cancel.clone(), status: status.clone() });
    std::thread::spawn(move || {
        // Un fallo inesperado no debe dejar la interfaz esperando para siempre
        let guard = status.clone();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(scene, request, cancel, status))).is_err() {
            let mut s = guard.lock().unwrap();
            s.phase = Phase::Error;
            s.error = Some("El render falló de forma inesperada".to_string());
        }
    });
    Ok(())
}

#[tauri::command]
pub fn ray_render_status(jobs: State<'_, RayRender>) -> StatusDto {
    let slot = jobs.job.lock().unwrap();
    let Some(job) = slot.as_ref() else {
        return StatusDto {
            phase: Phase::Idle,
            pass: 0,
            passes: 0,
            samples: 0,
            total_samples: 0,
            elapsed_secs: 0.0,
            eta_secs: 0.0,
            version: 0,
            error: None,
        };
    };
    let s = job.status.lock().unwrap();
    StatusDto {
        phase: s.phase,
        pass: s.pass,
        passes: s.passes,
        samples: s.samples,
        total_samples: s.total_samples,
        elapsed_secs: s.elapsed_secs,
        eta_secs: s.eta_secs,
        version: s.version,
        error: s.error.clone(),
    }
}

/// La última imagen (PNG con alfa): una vista previa mientras corre, la final al terminar
#[tauri::command]
pub fn ray_render_image(jobs: State<'_, RayRender>) -> Result<tauri::ipc::Response, String> {
    let slot = jobs.job.lock().unwrap();
    let png = slot
        .as_ref()
        .and_then(|j| j.status.lock().unwrap().png.clone())
        .ok_or_else(|| "Todavía no hay imagen".to_string())?;
    Ok(tauri::ipc::Response::new(png))
}

#[tauri::command]
pub fn ray_render_cancel(jobs: State<'_, RayRender>) {
    if let Some(job) = jobs.job.lock().unwrap().as_ref() {
        job.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RayRequest {
        RayRequest {
            width: 32,
            height: 32,
            samples: 8,
            passes: 2,
            max_depth: 2,
            denoise: false,
            transparent: true,
            background: None,
            camera: CameraDto { position: [0.0, 0.0, 5.0], target: [0.0; 3], up: [0.0, 1.0, 0.0], vfov_deg: 30.0 },
            suns: vec![SunDto { direction: [0.3, 1.0, 0.5], color: [3.0; 3] }],
            ambient: [0.3; 3],
        }
    }

    fn cube_scene() -> converter_scene::Scene {
        use converter_scene::*;
        let mut s = Scene::new();
        let p = vec![
            [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0],
        ];
        let idx: Vec<u32> = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5];
        s.meshes.push(Mesh {
            name: "cubo".into(),
            primitives: vec![Primitive { attributes: vec![VertexAttribute::Positions(p)], indices: Some(IndexData::U32(idx)), material: None }],
        });
        s
    }

    #[test]
    fn corre_hasta_el_final_y_entrega_png() {
        let status = Arc::new(Mutex::new(Status { phase: Phase::Preparing, ..Default::default() }));
        run(cube_scene(), request(), Arc::new(AtomicBool::new(false)), status.clone());
        let s = status.lock().unwrap();
        assert!(s.phase == Phase::Done, "{:?}", s.error);
        assert_eq!((s.pass, s.samples, s.passes), (2, 8, 2));
        assert!(s.version >= 2);
        let png = s.png.as_ref().unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let img = image::load_from_memory(png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (32, 32));
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "la esquina es fondo transparente");
        assert_eq!(img.get_pixel(16, 16).0[3], 255, "el cubo es opaco");
    }

    #[test]
    fn cancelar_deja_el_estado_cancelado() {
        let status = Arc::new(Mutex::new(Status::default()));
        run(cube_scene(), request(), Arc::new(AtomicBool::new(true)), status.clone());
        assert!(status.lock().unwrap().phase == Phase::Cancelled);
    }

    #[test]
    fn escena_vacia_es_error() {
        let status = Arc::new(Mutex::new(Status::default()));
        run(converter_scene::Scene::new(), request(), Arc::new(AtomicBool::new(false)), status.clone());
        let s = status.lock().unwrap();
        assert!(s.phase == Phase::Error && s.error.is_some());
    }

    #[test]
    fn valida_el_tamano() {
        let mut r = request();
        r.width = 0;
        assert!(validate(&r).is_err());
        r.width = 32;
        r.samples = 0;
        assert!(validate(&r).is_err());
    }
}
