//! Hilo de captura V4L2: abre los nodos de profundidad (Y16) y color (MJPG) del
//! escáner Revopoint y entrega frames emparejados a la GUI por canales.
//!
//! El `Device` y su `Stream` de V4L2 se crean y usan dentro del propio hilo
//! (los `Stream` toman prestado el `Device`), así que viven como variables
//! locales del bucle y no hace falta compartirlos entre hilos.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

#[cfg(target_os = "linux")]
use v4l::{
    Device, FourCC,
    buffer::Type,
    control::{Control, Value},
    io::{mmap::Stream, traits::CaptureStream},
    video::Capture as _, // métodos format()/set_format() sobre Device
};

use crate::camera::{CameraDescription, Frames, StreamInfo};
#[cfg(target_os = "linux")]
use crate::camera::{self, NodeChoice};
use crate::pointcloud::CloudParams;

// IDs de controles V4L2 (UVC) que ajustamos en el sensor de profundidad.
#[cfg(target_os = "linux")]
const CID_EXPOSURE_AUTO: u32 = 0x009a0901; // 1=Manual, 3=Aperture priority (auto)
#[cfg(target_os = "linux")]
const CID_EXPOSURE_ABS: u32 = 0x009a0902;
#[cfg(target_os = "linux")]
const CID_GAIN: u32 = 0x00980913;

/// Ajustes de exposición/ganancia del stream de profundidad.
#[derive(Debug, Clone, Copy)]
pub struct DepthControls {
    pub auto_exposure: bool,
    pub exposure: i32,
    pub gain: i32,
}

/// Comandos del hilo principal hacia el hilo de captura.
enum CaptureCmd {
    Depth(DepthControls),
}

/// Estado del pipeline de captura, para mostrarlo en la UI.
#[derive(Debug, Clone)]
pub enum Status {
    Connecting,
    Streaming {
        info: CameraDescription,
        depth: StreamInfo,
        rgb: Option<StreamInfo>,
        params: CloudParams,
    },
    Error(String),
    Stopped,
}

/// Maneja el hilo de captura y expone los canales de frames y estado.
pub struct Capture {
    pub frames: Receiver<Frames>,
    pub status: Receiver<Status>,
    cmd: Sender<CaptureCmd>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Capture {
    /// Arranca el hilo de captura. `wake` se invoca con cada frame para
    /// refrescar la GUI.
    pub fn start<F>(wake: F) -> Self
    where
        F: Fn() + Send + 'static,
    {
        let (frame_tx, frame_rx) = bounded::<Frames>(2);
        let (status_tx, status_rx) = unbounded::<Status>();
        let (cmd_tx, cmd_rx) = unbounded::<CaptureCmd>();
        let stop = Arc::new(AtomicBool::new(false));

        let stop_thread = stop.clone();
        let join = std::thread::Builder::new()
            .name("orizon3d-capture".into())
            .spawn(move || capture_loop(stop_thread, frame_tx, status_tx, cmd_rx, wake))
            .expect("no se pudo crear el hilo de captura");

        Capture {
            frames: frame_rx,
            status: status_rx,
            cmd: cmd_tx,
            stop,
            join: Some(join),
        }
    }

    /// Aplica exposición/ganancia al stream de profundidad (en el hilo de captura).
    pub fn set_depth_controls(&self, c: DepthControls) {
        let _ = self.cmd.send(CaptureCmd::Depth(c));
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(target_os = "linux")]
/// Abre un nodo V4L2 y negocia el formato deseado.
fn open_node(node: &NodeChoice) -> std::io::Result<Device> {
    let dev = Device::with_path(&node.path)?;
    let mut fmt = dev.format()?;
    fmt.width = node.width;
    fmt.height = node.height;
    fmt.fourcc = FourCC::new(&node.fourcc);
    let fmt = dev.set_format(&fmt)?;
    log::info!(
        "V4L2 {} → {}x{} {}",
        node.path,
        fmt.width,
        fmt.height,
        std::str::from_utf8(&fmt.fourcc.repr).unwrap_or("?")
    );
    Ok(dev)
}

#[cfg(target_os = "linux")]
fn stream_info(node: &NodeChoice) -> StreamInfo {
    StreamInfo {
        width: node.width,
        height: node.height,
        fps: 0.0,
        fourcc: node.fourcc,
    }
}

#[cfg(target_os = "linux")]
/// Aplica los ajustes de exposición/ganancia al dispositivo de profundidad.
fn apply_depth_controls(dev: &Device, c: &DepthControls) {
    // Auto = "aperture priority" (3); manual = 1.
    let ae = if c.auto_exposure { 3 } else { 1 };
    let _ = dev.set_control(Control {
        id: CID_EXPOSURE_AUTO,
        value: Value::Integer(ae),
    });
    if !c.auto_exposure {
        let _ = dev.set_control(Control {
            id: CID_EXPOSURE_ABS,
            value: Value::Integer(c.exposure as i64),
        });
    }
    let _ = dev.set_control(Control {
        id: CID_GAIN,
        value: Value::Integer(c.gain as i64),
    });
}

#[cfg(not(target_os = "linux"))]
fn capture_loop<F>(
    _stop: Arc<AtomicBool>,
    _frame_tx: Sender<Frames>,
    status_tx: Sender<Status>,
    _cmd_rx: Receiver<CaptureCmd>,
    _wake: F,
) where
    F: Fn(),
{
    let msg = crate::camera::discover().err().map(|e| e.to_string()).unwrap_or_default();
    let _ = status_tx.send(Status::Error(msg));
}

#[cfg(target_os = "linux")]
fn capture_loop<F>(
    stop: Arc<AtomicBool>,
    frame_tx: Sender<Frames>,
    status_tx: Sender<Status>,
    cmd_rx: Receiver<CaptureCmd>,
    wake: F,
) where
    F: Fn(),
{
    let _ = status_tx.send(Status::Connecting);

    let found = match camera::discover() {
        Ok(d) => d,
        Err(e) => {
            let _ = status_tx.send(Status::Error(e.to_string()));
            return;
        }
    };

    // Abrir el nodo de profundidad (obligatorio).
    let depth_dev = match open_node(&found.depth) {
        Ok(d) => d,
        Err(e) => {
            let _ = status_tx.send(Status::Error(format!(
                "No se pudo abrir el stream de profundidad {}: {e}",
                found.depth.path
            )));
            return;
        }
    };
    let mut depth_stream = match Stream::with_buffers(&depth_dev, Type::VideoCapture, 4) {
        Ok(s) => s,
        Err(e) => {
            let _ = status_tx.send(Status::Error(format!("V4L2 (profundidad) falló: {e}")));
            return;
        }
    };

    // Abrir el nodo RGB (opcional).
    let mut rgb_stream = match &found.rgb {
        Some(node) => match open_node(node).and_then(|dev| {
            // El Device debe vivir tanto como su Stream: lo fugamos a propósito
            // (la sesión dura toda la vida del hilo).
            let dev: &'static Device = Box::leak(Box::new(dev));
            Stream::with_buffers(dev, Type::VideoCapture, 4)
        }) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("No se pudo arrancar el stream RGB; sigo solo con profundidad: {e}");
                None
            }
        },
        None => None,
    };

    let params = CloudParams {
        depth_intr: camera::default_depth_intrinsics(found.depth.width, found.depth.height),
        rgb_intr: found
            .rgb
            .as_ref()
            .map(|r| camera::default_rgb_intrinsics(r.width, r.height)),
        extrinsics: camera::Extrinsics::default(),
        depth_scale: camera::DEFAULT_DEPTH_SCALE,
        // El recorte efectivo (rango + caja) lo aplica la app (UI).
        clip_min_mm: 0.0,
        clip_max_mm: 0.0,
        roi: None,
        edge_filter: true,
    };
    let _ = status_tx.send(Status::Streaming {
        info: found.info.clone(),
        depth: stream_info(&found.depth),
        rgb: found.rgb.as_ref().map(stream_info),
        params,
    });

    let (dw, dh) = (found.depth.width, found.depth.height);

    while !stop.load(Ordering::SeqCst) {
        // Aplicar comandos pendientes (exposición/ganancia) al sensor.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                CaptureCmd::Depth(c) => apply_depth_controls(&depth_dev, &c),
            }
        }

        // El stream de profundidad marca el ritmo (bloquea hasta el frame).
        let depth = match depth_stream.next() {
            Ok((buf, _meta)) => camera::DepthFrame {
                width: dw,
                height: dh,
                depth: camera::depth_from_y16(buf, dw, dh),
                timestamp_ms: 0.0,
            },
            Err(e) => {
                let _ = status_tx.send(Status::Error(format!("lectura de profundidad falló: {e}")));
                break;
            }
        };

        // RGB: tomamos el frame disponible más reciente (no bloqueante de facto,
        // va a más fps que la profundidad) y lo decodificamos.
        let rgb = match &mut rgb_stream {
            Some(s) => match s.next() {
                Ok((buf, _meta)) => camera::decode_mjpg(buf),
                Err(e) => {
                    log::warn!("lectura RGB falló: {e}");
                    None
                }
            },
            None => None,
        };

        // Buffer acotado: si está lleno descartamos para no acumular latencia.
        let _ = frame_tx.try_send(Frames { depth, rgb });
        wake();
    }

    let _ = status_tx.send(Status::Stopped);
}
