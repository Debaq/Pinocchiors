//! Render de animaciones: un fotograma por instante de la animación de la escena (el
//! esqueleto y los nodos en esa pose), con desenfoque de movimiento opcional y la
//! posibilidad de saltar cuadros que ya existen para reanudar un render interrumpido.

use crate::camera::Camera;
use crate::image_out::Image;
use crate::progressive::{Frame, ProgressiveOptions, render_progressive};
use crate::render::Settings;
use crate::scene::AnimatedScene;
use crate::studio::Studio;
use converter_scene::Scene;
use glam::Vec3;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct AnimationSettings {
    /// Animación de la escena que se renderiza; `None` = la escena en reposo
    pub animation: Option<usize>,
    /// Cuadros por segundo: el cuadro `n` está en el segundo `n / fps`
    pub fps: f32,
    /// Primer y último cuadro (inclusive)
    pub first_frame: i64,
    pub last_frame: i64,
    /// Fracción del cuadro en que está abierto el obturador (0 = sin desenfoque; 0,5 = 180°)
    pub shutter: f32,
    /// Instantes dentro del obturador que se promedian (1 = sin desenfoque de movimiento)
    pub motion_samples: u32,
    /// Fondo de estudio que acompaña a todos los cuadros
    pub studio: Option<Studio>,
}

impl AnimationSettings {
    /// Cuántos cuadros tiene el rango
    pub fn frame_count(&self) -> u32 {
        (self.last_frame - self.first_frame + 1).clamp(0, u32::MAX as i64) as u32
    }
}

/// Avance dentro de la secuencia
#[derive(Debug, Clone, Copy)]
pub struct FrameProgress {
    /// Cuadro en curso (0 = el primero del rango) y total
    pub frame: u32,
    pub frames: u32,
    /// Pasada y muestras del cuadro (o del instante del obturador) en curso
    pub pass: u32,
    pub passes: u32,
    pub samples: u32,
    pub total_samples: u32,
    /// Fracción de toda la secuencia (los cuadros saltados cuentan como hechos)
    pub fraction: f32,
    pub elapsed_secs: f64,
    /// Tiempo que falta, con el ritmo de los cuadros renderizados hasta ahora
    pub eta_secs: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnimationOutcome {
    pub rendered: u32,
    pub skipped: u32,
    pub cancelled: bool,
}

/// Promedio de varios cuadros (instantes del obturador): el color con alfa se promedia
/// premultiplicado
fn average(frames: &[Frame]) -> Frame {
    let n = frames.len() as f32;
    let first = &frames[0];
    let len = first.image.pixels.len();
    let mut pixels = vec![Vec3::ZERO; len];
    let mut alpha = first.alpha.as_ref().map(|_| vec![0.0f32; len]);
    for f in frames {
        for i in 0..len {
            let a = f.alpha.as_ref().map_or(1.0, |a| a[i]);
            pixels[i] += f.image.pixels[i] * a;
            if let Some(acc) = alpha.as_mut() {
                acc[i] += a;
            }
        }
    }
    for i in 0..len {
        match alpha.as_mut() {
            Some(acc) => {
                let total = acc[i];
                if total > 1e-6 {
                    pixels[i] /= total;
                }
                acc[i] = total / n;
            }
            None => pixels[i] /= n,
        }
    }
    Frame { image: Image { width: first.image.width, height: first.image.height, pixels }, alpha }
}

/// Renderiza la secuencia. `skip(i)` dice si el cuadro `i` (0 = el primero) ya existe y se
/// salta; `on_progress` recibe la imagen que se va afinando (por pasada, o por instante del
/// obturador con desenfoque) y `on_frame` el cuadro terminado. Si `cancel` se activa se
/// descarta el cuadro en curso y se devuelve lo hecho.
pub fn render_animation(
    scene: &Scene,
    camera: &Camera,
    s: &Settings,
    o: &ProgressiveOptions,
    a: &AnimationSettings,
    cancel: &AtomicBool,
    skip: impl Fn(u32) -> bool,
    mut on_progress: impl FnMut(&FrameProgress, &Frame),
    mut on_frame: impl FnMut(u32, &Frame),
) -> AnimationOutcome {
    let frames = a.frame_count();
    let mut animated = AnimatedScene::new(scene);
    if let Some(studio) = a.studio {
        animated = animated.with_studio(studio);
    }
    let fps = if a.fps > 0.0 { a.fps } else { 24.0 };
    let blur = a.shutter > 0.0 && a.motion_samples > 1 && a.animation.is_some();
    let k = if blur { a.motion_samples } else { 1 };
    let sub_samples = s.samples.max(1).div_ceil(k).max(1);
    let todo = (0..frames).filter(|&i| !skip(i)).count().max(1) as f32;
    let start = Instant::now();
    let mut out = AnimationOutcome::default();
    let mut done_frames = 0.0f32;
    for i in 0..frames {
        if cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        if skip(i) {
            out.skipped += 1;
            continue;
        }
        let time = (a.first_frame + i as i64) as f32 / fps;
        let mut subframes: Vec<Frame> = Vec::with_capacity(k as usize);
        for j in 0..k {
            let offset = if blur { ((j as f32 + 0.5) / k as f32 - 0.5) * a.shutter / fps } else { 0.0 };
            let rs = animated.at(a.animation, time + offset);
            // Misma semilla en todos los cuadros: el ruido que queda no baila de un cuadro al
            // siguiente (mejor que un parpadeo nuevo cada vez); entre instantes del obturador cambia
            let ss = Settings { samples: sub_samples, seed: s.seed ^ (j as u64).wrapping_mul(0x9E37_79B9), ..s.clone() };
            let oo = ProgressiveOptions { passes: o.passes.min(sub_samples), ..*o };
            let outcome = render_progressive(&rs, camera, &ss, &oo, cancel, |info, frame| {
                let within = (j as f32 + info.samples as f32 / info.total_samples.max(1) as f32) / k as f32;
                let fraction_done = done_frames + within;
                let elapsed = start.elapsed().as_secs_f64();
                let eta = if fraction_done > 0.0 { elapsed / fraction_done as f64 * (todo - fraction_done) as f64 } else { 0.0 };
                let progress = FrameProgress {
                    frame: i,
                    frames,
                    pass: info.pass,
                    passes: info.passes,
                    samples: info.samples,
                    total_samples: info.total_samples,
                    fraction: (out.skipped as f32 + fraction_done) / (frames as f32).max(1.0),
                    elapsed_secs: elapsed,
                    eta_secs: eta.max(0.0),
                };
                if k == 1 {
                    on_progress(&progress, frame);
                }
            });
            if outcome.cancelled {
                out.cancelled = true;
                return out;
            }
            subframes.push(outcome.frame);
            if k > 1 {
                let elapsed = start.elapsed().as_secs_f64();
                let fraction_done = done_frames + (j + 1) as f32 / k as f32;
                let progress = FrameProgress {
                    frame: i,
                    frames,
                    pass: j + 1,
                    passes: k,
                    samples: sub_samples * (j + 1),
                    total_samples: sub_samples * k,
                    fraction: (out.skipped as f32 + fraction_done) / (frames as f32).max(1.0),
                    elapsed_secs: elapsed,
                    eta_secs: (elapsed / fraction_done as f64 * (todo - fraction_done) as f64).max(0.0),
                };
                on_progress(&progress, &average(&subframes));
            }
        }
        let frame = if subframes.len() == 1 { subframes.remove(0) } else { average(&subframes) };
        on_frame(i, &frame);
        out.rendered += 1;
        done_frames += 1.0;
    }
    out
}
