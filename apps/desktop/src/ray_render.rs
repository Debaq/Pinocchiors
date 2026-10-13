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
    /// Animación: cuadro en curso (1 = el primero) y total; 0 en una imagen suelta
    frame: u32,
    frames: u32,
    /// Fracción de toda la secuencia
    fraction: f32,
    /// Cuadros que ya existían y se saltaron al reanudar
    skipped: u32,
    /// Video que se armó con la secuencia
    video: Option<String>,
    /// Aviso al terminar (p. ej. por qué no hay video)
    note: Option<String>,
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
    frame: u32,
    frames: u32,
    fraction: f32,
    skipped: u32,
    video: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CameraDto {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    /// Campo de visión vertical de la imagen, en grados
    pub vfov_deg: f32,
}

/// Luz direccional: `direction` apunta hacia la luz; `color` es la irradiancia
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SunDto {
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

#[derive(Deserialize, Clone)]
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
    /// Radio angular (radianes) del disco de los soles: 0 = sombras duras
    #[serde(default)]
    pub sun_angular_radius: f32,
    /// Curva de tono ACES como la del visor (sin ella la salida es lineal → sRGB)
    #[serde(default)]
    pub tone_map: bool,
    /// Suelo invisible que recibe la sombra, con esta opacidad máxima (None = sin suelo)
    #[serde(default)]
    pub shadow_floor: Option<f32>,
    /// Imagen HDR de entorno (un `builtin:id` de los incluidos o la ruta a un .hdr): reemplaza al cielo uniforme
    #[serde(default)]
    pub hdri: Option<HdriDto>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HdriDto {
    pub path: String,
    #[serde(default = "one")]
    pub intensity: f32,
    /// Giro alrededor del eje vertical, en grados
    #[serde(default)]
    pub rotation_deg: f32,
}

fn one() -> f32 {
    1.0
}

fn tone_of(req: &RayRequest) -> gr::ToneMap {
    if req.tone_map { gr::ToneMap::Aces { exposure: 1.0 } } else { gr::ToneMap::Linear }
}

/// Degradado vertical bajo la imagen, en sRGB de 8 bits (como el que pinta la interfaz)
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GradientDto {
    pub top: [u8; 3],
    pub bottom: [u8; 3],
}

fn encode_png(frame: &gr::Frame, tone: gr::ToneMap, gradient: Option<GradientDto>) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let (w, h) = (frame.image.width, frame.image.height);
    let mut rgba = frame.to_srgb8_rgba_with(tone);
    if let Some(g) = gradient {
        for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
            let y = (i / w as usize) as f32 / (h.max(2) - 1) as f32;
            let a = px[3] as f32 / 255.0;
            for c in 0..3 {
                let bg = g.top[c] as f32 * (1.0 - y) + g.bottom[c] as f32 * y;
                px[c] = (px[c] as f32 * a + bg * (1.0 - a) + 0.5) as u8;
            }
            px[3] = 255;
        }
    }
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
    .map_err(|e| e.to_string())?;
    Ok(out)
}

fn settings_from(req: &RayRequest, scene: &gr::RayScene) -> Result<gr::Settings, String> {
    let environment = match &req.hdri {
        Some(h) => {
            let bytes = crate::hdri::load(&h.path)?;
            let map = gr::EnvMap::from_bytes(&bytes, h.intensity, h.rotation_deg.to_radians())
                .ok_or_else(|| "La imagen de entorno no es válida (se espera un .hdr)".to_string())?;
            gr::Environment::hdri(map)
        }
        None => gr::Environment::uniform(Vec3::from(req.ambient)),
    };
    // Mismo suelo que el visor: bajo la caja de la escena, ancho 8 veces su diagonal
    let shadow_catcher = req.shadow_floor.map(|strength| {
        let b = scene.bvh.bounds();
        gr::ShadowCatcher {
            center: Vec3::new((b.min.x + b.max.x) * 0.5, b.min.y, (b.min.z + b.max.z) * 0.5),
            normal: Vec3::Y,
            radius: scene.extent * 4.0,
            strength: strength.clamp(0.0, 1.0),
        }
    });
    Ok(gr::Settings {
        width: req.width,
        height: req.height,
        samples: req.samples,
        max_depth: req.max_depth,
        environment,
        lights: req
            .suns
            .iter()
            .map(|s| {
                gr::Light::Sun(gr::Sun {
                    direction: Vec3::from(s.direction),
                    color: Vec3::from(s.color),
                    angular_radius: req.sun_angular_radius.clamp(0.0, 0.5),
                })
            })
            .collect(),
        transparent_background: req.transparent,
        background: req.background.map(Vec3::from),
        shadow_catcher,
        ..Default::default()
    })
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
    let settings = match settings_from(&req, &ray_scene) {
        Ok(s) => s,
        Err(e) => {
            set(&mut |s| {
                s.phase = Phase::Error;
                s.error = Some(e.clone());
            });
            return;
        }
    };
    let tone = tone_of(&req);
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
        let png = encode_png(frame, tone, None);
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
    let final_png = encode_png(&outcome.frame, tone, None);
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

/// Lo propio de una animación: qué clip, qué cuadros y dónde se guardan
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnimationRequest {
    /// Clips de la línea de tiempo ya horneados (como los de la exportación)
    pub clips: Vec<crate::animation::AnimationClip>,
    /// Cuál se renderiza
    pub clip: usize,
    pub first_frame: i64,
    pub last_frame: i64,
    pub fps: f32,
    /// Carpeta y nombre base: los cuadros salen como `nombre_0001.png`
    pub dir: String,
    pub name: String,
    /// Fracción del cuadro con el obturador abierto (0 = sin desenfoque de movimiento)
    #[serde(default)]
    pub shutter: f32,
    #[serde(default = "one_u32")]
    pub motion_samples: u32,
    /// Saltar los cuadros que ya están en la carpeta
    #[serde(default)]
    pub resume: bool,
    /// Armar un video con ffmpeg si está instalado
    #[serde(default)]
    pub video: bool,
    /// Degradado bajo la imagen (con fondo transparente)
    #[serde(default)]
    pub gradient: Option<GradientDto>,
    #[serde(default)]
    pub use_retopology: bool,
    #[serde(default)]
    pub non_deforming: Vec<usize>,
}

fn one_u32() -> u32 {
    1
}

fn frame_path(anim: &AnimationRequest, index: u32) -> std::path::PathBuf {
    std::path::Path::new(&anim.dir).join(format!("{}_{:04}.png", anim.name, index + 1))
}

/// Une los cuadros en un MP4 con ffmpeg. `Ok(None)` si ffmpeg no está instalado.
fn make_video(anim: &AnimationRequest) -> Result<Option<String>, String> {
    use std::process::{Command, Stdio};
    let found = Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !found.is_ok_and(|s| s.success()) {
        return Ok(None);
    }
    let out = std::path::Path::new(&anim.dir).join(format!("{}.mp4", anim.name));
    let input = std::path::Path::new(&anim.dir).join(format!("{}_%04d.png", anim.name));
    let result = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-framerate"])
        .arg(format!("{}", if anim.fps > 0.0 { anim.fps } else { 24.0 }))
        .arg("-i")
        .arg(&input)
        // yuv420p pide medidas pares
        .args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16"])
        .arg(&out)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().to_string());
    }
    Ok(Some(out.to_string_lossy().into_owned()))
}

fn run_animation(
    scene: converter_scene::Scene,
    req: RayRequest,
    anim: AnimationRequest,
    cancel: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
) {
    let set = |f: &mut dyn FnMut(&mut Status)| f(&mut status.lock().unwrap());
    let fail = |msg: String| {
        set(&mut |s| {
            s.phase = Phase::Error;
            s.error = Some(msg.clone());
        })
    };
    if let Err(e) = std::fs::create_dir_all(&anim.dir) {
        return fail(format!("No se pudo crear la carpeta {}: {e}", anim.dir));
    }
    // Para el tamaño de la escena (el suelo con sombra) y la comprobación de que hay geometría
    let probe = gr::RayScene::from_scene(&scene);
    if probe.triangles.is_empty() {
        return fail("La escena no tiene geometría para renderizar".to_string());
    }
    let settings = match settings_from(&req, &probe) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    drop(probe);
    let camera = camera_from(&req);
    let tone = tone_of(&req);
    let options = gr::ProgressiveOptions {
        passes: req.passes,
        denoise: req.denoise.then(gr::DenoiseSettings::default),
        ..Default::default()
    };
    let settings_a = gr::AnimationSettings {
        animation: Some(anim.clip),
        fps: if anim.fps > 0.0 { anim.fps } else { 24.0 },
        first_frame: anim.first_frame,
        last_frame: anim.last_frame,
        shutter: anim.shutter.clamp(0.0, 1.0),
        motion_samples: anim.motion_samples.clamp(1, 32),
    };
    let frames = settings_a.frame_count();
    set(&mut |s| {
        s.phase = Phase::Running;
        s.frames = frames;
        s.total_samples = req.samples.max(1);
    });
    let write_error: Mutex<Option<String>> = Mutex::new(None);
    let outcome = gr::render_animation(
        &scene,
        &camera,
        &settings,
        &options,
        &settings_a,
        &cancel,
        |i| anim.resume && std::fs::metadata(frame_path(&anim, i)).is_ok_and(|m| m.len() > 0),
        |p, frame| {
            let png = encode_png(frame, tone, None);
            set(&mut |s| {
                s.frame = p.frame + 1;
                s.frames = p.frames;
                s.pass = p.pass;
                s.passes = p.passes;
                s.samples = p.samples;
                s.total_samples = p.total_samples;
                s.fraction = p.fraction;
                s.elapsed_secs = p.elapsed_secs;
                s.eta_secs = p.eta_secs;
                if let Ok(bytes) = &png {
                    s.png = Some(bytes.clone());
                    s.version += 1;
                }
            });
        },
        |i, frame| {
            // El cuadro terminado va a disco; si falla se corta la secuencia
            let res = encode_png(frame, tone, anim.gradient).and_then(|bytes| {
                // Se escribe aparte y se renombra: un cuadro a medias no cuenta al reanudar
                let path = frame_path(&anim, i);
                let tmp = path.with_extension("png.part");
                std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
            });
            if let Err(e) = res {
                *write_error.lock().unwrap() = Some(format!("No se pudo guardar el cuadro {}: {e}", i + 1));
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    if let Some(e) = write_error.into_inner().unwrap() {
        return fail(e);
    }
    set(&mut |s| {
        s.skipped = outcome.skipped;
        s.fraction = if outcome.cancelled { s.fraction } else { 1.0 };
    });
    let mut video = None;
    let mut note = None;
    if anim.video && !outcome.cancelled {
        if req.transparent && anim.gradient.is_none() {
            note = Some("El video no guarda transparencia: elige un fondo de color o degradado. Quedó la secuencia de imágenes.".to_string());
        } else {
            match make_video(&anim) {
                Ok(Some(path)) => video = Some(path),
                Ok(None) => note = Some("No se encontró ffmpeg: quedó la secuencia de imágenes.".to_string()),
                Err(e) => note = Some(format!("ffmpeg no pudo armar el video ({e}): quedó la secuencia de imágenes.")),
            }
        }
    }
    set(&mut |s| {
        s.video = video.clone();
        s.note = note.clone();
        s.phase = if outcome.cancelled { Phase::Cancelled } else { Phase::Done };
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

/// Renderiza una animación cuadro a cuadro: el rig se arma como en la exportación, con los clips
/// que manda la interfaz, y cada cuadro se guarda como PNG en la carpeta indicada.
#[tauri::command]
pub fn ray_render_animation_start(
    app: AppHandle,
    jobs: State<'_, RayRender>,
    request: RayRequest,
    animation: AnimationRequest,
) -> Result<(), String> {
    validate(&request)?;
    if animation.clip >= animation.clips.len() {
        return Err("No existe la animación elegida".to_string());
    }
    if animation.last_frame < animation.first_frame || animation.last_frame - animation.first_frame >= 100_000 {
        return Err("El rango de cuadros no es válido".to_string());
    }
    if animation.dir.trim().is_empty() || animation.name.trim().is_empty() {
        return Err("Hace falta una carpeta y un nombre para los cuadros".to_string());
    }
    let mut slot = jobs.job.lock().unwrap();
    if let Some(job) = slot.as_ref() {
        if matches!(job.status.lock().unwrap().phase, Phase::Preparing | Phase::Running) {
            return Err("Ya hay un render en curso".to_string());
        }
    }
    let state = app.state::<AppState>();
    let mut config: crate::commands::ExportConfig =
        serde_json::from_value(serde_json::json!({ "format": "glb", "path": "", "include_weights": true, "include_skeleton": true }))
            .map_err(|e| e.to_string())?;
    config.use_retopology = Some(animation.use_retopology);
    config.animations = Some(animation.clips.clone());
    config.non_deforming = Some(animation.non_deforming.clone());
    // La misma escena con rig que se exporta: malla con skin y un nodo por articulación
    let scene = crate::commands::build_export_scene(&config, &state)?;
    if scene.skeletons.is_empty() {
        return Err("Hace falta el rig (esqueleto y pesos) para renderizar la animación".to_string());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(Status { phase: Phase::Preparing, ..Default::default() }));
    *slot = Some(Job { cancel: cancel.clone(), status: status.clone() });
    std::thread::spawn(move || {
        let guard = status.clone();
        let run = std::panic::AssertUnwindSafe(|| run_animation(scene, request, animation, cancel, status));
        if std::panic::catch_unwind(run).is_err() {
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
            frame: 0,
            frames: 0,
            fraction: 0.0,
            skipped: 0,
            video: None,
            note: None,
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
        frame: s.frame,
        frames: s.frames,
        fraction: s.fraction,
        skipped: s.skipped,
        video: s.video.clone(),
        note: s.note.clone(),
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
            sun_angular_radius: 0.0,
            tone_map: false,
            shadow_floor: None,
            hdri: None,
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

    fn run_png(req: RayRequest) -> (Phase, Option<String>, Option<image::RgbaImage>) {
        let status = Arc::new(Mutex::new(Status { phase: Phase::Preparing, ..Default::default() }));
        run(cube_scene(), req, Arc::new(AtomicBool::new(false)), status.clone());
        let s = status.lock().unwrap();
        (s.phase, s.error.clone(), s.png.as_ref().map(|p| image::load_from_memory(p).unwrap().to_rgba8()))
    }

    #[test]
    fn suelo_con_sombra_deja_alfa_parcial_bajo_el_objeto() {
        let mut r = request();
        r.width = 48;
        r.height = 48;
        r.samples = 16;
        r.camera = CameraDto { position: [0.0, 3.0, 6.0], target: [0.0; 3], up: [0.0, 1.0, 0.0], vfov_deg: 40.0 };
        r.suns = vec![SunDto { direction: [1.0, 1.0, 0.0], color: [3.0; 3] }];
        r.ambient = [0.0; 3];
        let partial = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[3] > 20 && p.0[3] < 235).count();
        // Sin suelo, el alfa parcial es solo el borde suavizado del cubo
        let (_, _, sin) = run_png(r.clone());
        let bordes = partial(&sin.unwrap());
        r.shadow_floor = Some(0.8);
        let (phase, err, con) = run_png(r);
        assert!(phase == Phase::Done, "{err:?}");
        assert!(partial(&con.unwrap()) > bordes + 10, "la sombra del cubo cae sobre el suelo invisible");
    }

    #[test]
    fn imagen_de_entorno_inexistente_es_error() {
        let mut r = request();
        r.hdri = Some(HdriDto { path: "/no/existe/cielo.hdr".into(), intensity: 1.0, rotation_deg: 0.0 });
        let (phase, err, _) = run_png(r);
        assert!(phase == Phase::Error);
        assert!(err.unwrap().contains("imagen de entorno"));
    }

    #[test]
    fn imagen_de_entorno_ilumina_el_objeto() {
        use image::codecs::hdr::HdrEncoder;
        let dir = std::env::temp_dir().join(format!("gr_hdri_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cielo.hdr");
        let px = vec![image::Rgb([2.0f32, 2.0, 2.0]); 16 * 8];
        HdrEncoder::new(std::fs::File::create(&path).unwrap()).encode(&px, 16, 8).unwrap();
        let mut r = request();
        r.suns.clear();
        r.ambient = [0.0; 3];
        r.transparent = false;
        r.hdri = Some(HdriDto { path: path.to_string_lossy().into_owned(), intensity: 1.0, rotation_deg: 0.0 });
        let (phase, err, img) = run_png(r);
        std::fs::remove_dir_all(&dir).ok();
        assert!(phase == Phase::Done, "{err:?}");
        let img = img.unwrap();
        // Sin soles el cubo solo recibe luz del HDRI, y el fondo es el HDRI visible
        assert!(img.get_pixel(16, 16).0[0] > 20, "el cubo se ilumina con el entorno");
        assert!(img.get_pixel(0, 0).0[0] > 200, "el fondo es el entorno");
    }

    /// El cubo en un nodo que se desplaza en X durante un segundo
    fn moving_cube() -> converter_scene::Scene {
        use converter_scene::*;
        let mut s = cube_scene();
        s.nodes.push(Node { name: "cubo".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        s.root_nodes = vec![0];
        s.animations.push(Animation {
            name: "mueve".into(),
            channels: vec![Channel {
                node: 0,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 1.0],
                values: KeyframeValues::Translation(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            }],
        });
        s
    }

    fn anim_request(dir: &std::path::Path) -> AnimationRequest {
        AnimationRequest {
            clips: vec![],
            clip: 0,
            first_frame: 0,
            last_frame: 2,
            fps: 2.0,
            dir: dir.to_string_lossy().into_owned(),
            name: "prueba".into(),
            shutter: 0.0,
            motion_samples: 1,
            resume: false,
            video: false,
            gradient: None,
            use_retopology: false,
            non_deforming: vec![],
        }
    }

    fn run_anim(req: RayRequest, anim: AnimationRequest) -> Status {
        let status = Arc::new(Mutex::new(Status { phase: Phase::Preparing, ..Default::default() }));
        run_animation(moving_cube(), req, anim, Arc::new(AtomicBool::new(false)), status.clone());
        let s = status.lock().unwrap();
        Status { phase: s.phase, frames: s.frames, frame: s.frame, skipped: s.skipped, error: s.error.clone(), note: s.note.clone(), fraction: s.fraction, ..Default::default() }
    }

    #[test]
    fn animacion_guarda_un_png_por_cuadro_y_reanuda() {
        let dir = std::env::temp_dir().join(format!("gr_anim_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut r = request();
        r.width = 24;
        r.height = 16;
        r.samples = 4;
        let s = run_anim(r.clone(), anim_request(&dir));
        assert!(s.phase == Phase::Done, "{:?}", s.error);
        assert_eq!((s.frames, s.skipped), (3, 0));
        assert!((s.fraction - 1.0).abs() < 1e-6);
        let first = image::open(dir.join("prueba_0001.png")).unwrap().to_rgba8();
        let last = image::open(dir.join("prueba_0003.png")).unwrap().to_rgba8();
        assert_eq!(first.dimensions(), (24, 16));
        // El cubo va de izquierda a derecha: la columna con más alfa se corre
        let centre = |img: &image::RgbaImage| {
            let cols: Vec<u32> = (0..24).filter(|&x| (0..16).any(|y| img.get_pixel(x, y).0[3] > 128)).collect();
            cols.iter().sum::<u32>() as f32 / cols.len().max(1) as f32
        };
        assert!(centre(&last) > centre(&first) + 3.0, "{} -> {}", centre(&first), centre(&last));
        // Reanudar: con todos los cuadros hechos se salta todo y no se toca ninguno
        let stamp = std::fs::metadata(dir.join("prueba_0002.png")).unwrap().modified().unwrap();
        let mut again = anim_request(&dir);
        again.resume = true;
        let s2 = run_anim(r.clone(), again);
        assert!(s2.phase == Phase::Done && s2.skipped == 3, "{:?} {}", s2.error, s2.skipped);
        assert_eq!(std::fs::metadata(dir.join("prueba_0002.png")).unwrap().modified().unwrap(), stamp);
        // Un cuadro borrado se rehace y los demás se saltan
        std::fs::remove_file(dir.join("prueba_0002.png")).unwrap();
        let mut partial = anim_request(&dir);
        partial.resume = true;
        let s3 = run_anim(r, partial);
        assert_eq!(s3.skipped, 2);
        assert!(dir.join("prueba_0002.png").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn animacion_con_degradado_sale_opaca_y_el_video_pide_fondo() {
        let dir = std::env::temp_dir().join(format!("gr_anim_grad_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut r = request();
        r.width = 16;
        r.height = 16;
        r.samples = 2;
        let mut a = anim_request(&dir);
        a.last_frame = 0;
        a.gradient = Some(GradientDto { top: [255, 0, 0], bottom: [0, 0, 255] });
        let s = run_anim(r.clone(), a);
        assert!(s.phase == Phase::Done, "{:?}", s.error);
        let img = image::open(dir.join("prueba_0001.png")).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(15, 0).0, [255, 0, 0, 255], "arriba, el color de arriba");
        assert_eq!(img.get_pixel(15, 15).0, [0, 0, 255, 255], "abajo, el de abajo");
        // Video con fondo transparente: no hay video y se avisa
        let mut v = anim_request(&dir);
        v.last_frame = 0;
        v.video = true;
        let s = run_anim(r, v);
        assert!(s.note.unwrap().contains("transparencia"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn los_entornos_incluidos_iluminan_y_se_ven_de_fondo() {
        for id in ["studio", "loft", "sky", "overcast", "sunset", "warehouse"] {
            let mut r = request();
            r.suns.clear();
            r.ambient = [0.0; 3];
            r.transparent = false;
            r.hdri = Some(HdriDto { path: format!("builtin:{id}"), intensity: 1.0, rotation_deg: 0.0 });
            let (phase, err, img) = run_png(r);
            assert!(phase == Phase::Done, "{id}: {err:?}");
            let img = img.unwrap();
            let lit = |x, y| img.get_pixel(x, y).0[..3].iter().map(|&c| c as u32).sum::<u32>();
            assert!(lit(16, 16) > 30, "{id}: el cubo recibe luz del entorno");
            // Fondo: un píxel puede caer en una zona oscura del entorno; el promedio de la fila de arriba no
            let top = (0..32).map(|x| lit(x, 0)).sum::<u32>() / 32;
            assert!(top > 6, "{id}: el entorno se ve de fondo ({top})");
        }
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
