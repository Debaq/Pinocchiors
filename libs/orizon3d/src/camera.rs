//! Captura directa por **V4L2** del escáner Revopoint, sin el SDK propietario.
//!
//! El POP 2/3 es un dispositivo UVC estándar: expone el mapa de profundidad como
//! `Y16` (16-bit, = Z16) y el color como `MJPG`. Leerlos por V4L2 NO requiere la
//! licencia del SDK (que bloqueaba `systemConnectCamera`). Aquí va el
//! descubrimiento de nodos, los tipos de frame y la decodificación; el bucle de
//! captura vive en `capture.rs`.
//!
//! Limitación actual: sin el SDK no tenemos los intrínsecos/extrínsecos reales
//! de fábrica, así que usamos una aproximación (ver `default_depth_intrinsics`).
//! La nube saldrá con forma correcta pero escala/registro aproximados hasta que
//! extraigamos la calibración real (camparam del dispositivo o Q.bin de RevoScan).

use std::io;

/// Información legible de la cámara.
#[derive(Debug, Clone, Default)]
pub struct CameraDescription {
    pub name: String,
    pub serial: String,
    /// V4L2 no expone la versión de firmware; se conserva por compatibilidad.
    #[allow(dead_code)]
    pub firmware: String,
}

/// Intrínsecos de un stream (misma forma que usaba el SDK, para que
/// `pointcloud.rs` no cambie). Solo usamos fx, fy, cx, cy y la resolución;
/// el resto son los ceros/uno de la matriz, conservados por compatibilidad.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Intrinsics {
    pub width: i16,
    pub height: i16,
    pub fx: f32,
    pub zero01: f32,
    pub cx: f32,
    pub zeor10: f32,
    pub fy: f32,
    pub cy: f32,
    pub zeor20: f32,
    pub zero21: f32,
    pub one22: f32,
}

/// Extrínsecos depth→RGB: rotación 3x3 (fila-mayor) + traslación (mm).
#[derive(Debug, Clone, Copy)]
pub struct Extrinsics {
    pub rotation: [f32; 9],
    pub translation: [f32; 3],
}

impl Default for Extrinsics {
    fn default() -> Self {
        // Identidad: sin datos de fábrica asumimos depth y RGB alineados.
        Extrinsics {
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            translation: [0.0, 0.0, 0.0],
        }
    }
}

/// Descripción breve de un stream para la UI.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub fourcc: [u8; 4],
}

impl StreamInfo {
    pub fn fourcc_str(&self) -> String {
        String::from_utf8_lossy(&self.fourcc).trim().to_string()
    }
}

/// Frame de profundidad copiado a memoria propia. `depth_mm = depth_scale·valor`.
pub struct DepthFrame {
    pub width: u32,
    pub height: u32,
    pub depth: Vec<u16>,
    #[allow(dead_code)]
    pub timestamp_ms: f64,
}

/// Frame RGB ya decodificado a RGB8 entrelazado.
pub struct RgbFrame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// Conjunto de frames que la captura entrega a la GUI.
pub struct Frames {
    pub depth: DepthFrame,
    pub rgb: Option<RgbFrame>,
}

/// Un nodo V4L2 elegido (ruta + formato negociado).
#[derive(Debug, Clone)]
pub struct NodeChoice {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub fourcc: [u8; 4],
}

/// Resultado del descubrimiento: qué nodos usar para profundidad y RGB.
pub struct Discovered {
    pub info: CameraDescription,
    pub depth: NodeChoice,
    pub rgb: Option<NodeChoice>,
}

const FOURCC_Y16: [u8; 4] = *b"Y16 ";
const FOURCC_MJPG: [u8; 4] = *b"MJPG";

/// Resolución preferida de profundidad (la que vimos: 640x400 Y16).
const DEPTH_W: u32 = 640;
const DEPTH_H: u32 = 400;
/// Resolución preferida de RGB (1280x800 MJPG).
const RGB_W: u32 = 1280;
const RGB_H: u32 = 800;

/// Recorre `/dev/video*` y localiza el escáner Revopoint: el nodo que ofrece
/// `Y16` es la profundidad; el que ofrece `MJPG` es el color.
#[cfg(target_os = "linux")]
pub fn discover() -> io::Result<Discovered> {
    use v4l::video::Capture as _;
    use v4l::Device;

    let mut depth: Option<NodeChoice> = None;
    let mut rgb: Option<NodeChoice> = None;
    let mut info = CameraDescription::default();

    for idx in 0..64 {
        let path = format!("/dev/video{idx}");
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        let Ok(dev) = Device::with_path(&path) else {
            continue;
        };
        let Ok(caps) = dev.query_caps() else { continue };
        // Solo nos interesan los nodos del Revopoint, no la webcam del portátil.
        let card = caps.card.to_lowercase();
        if !(card.contains("revo") || card.contains("depthcam") || card.contains("3dcamera")) {
            continue;
        }
        let Ok(formats) = dev.enum_formats() else { continue };
        let has = |fourcc: [u8; 4]| formats.iter().any(|f| f.fourcc.repr == fourcc);

        if depth.is_none() && has(FOURCC_Y16) {
            depth = Some(NodeChoice {
                path: path.clone(),
                width: DEPTH_W,
                height: DEPTH_H,
                fourcc: FOURCC_Y16,
            });
            if info.name.is_empty() {
                info = describe(&caps.card);
            }
        }
        if rgb.is_none() && has(FOURCC_MJPG) {
            rgb = Some(NodeChoice {
                path: path.clone(),
                width: RGB_W,
                height: RGB_H,
                fourcc: FOURCC_MJPG,
            });
            if info.name.is_empty() {
                info = describe(&caps.card);
            }
        }
    }

    match depth {
        Some(depth) => Ok(Discovered { info, depth, rgb }),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No se encontró un escáner Revopoint (stream de profundidad Y16). \
             ¿Está conectado por USB y tu usuario puede leer /dev/video*?",
        )),
    }
}

/// V4L2 solo existe en Linux.
#[cfg(not(target_os = "linux"))]
pub fn discover() -> io::Result<Discovered> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "La conexión con escáneres 3D por ahora solo funciona en Linux",
    ))
}

/// Extrae nombre/serial de la cadena de tarjeta V4L2, p. ej.
/// «REVO_PRODUCT: DepthCamA2262699206F00A54».
fn describe(card: &str) -> CameraDescription {
    let serial = card
        .rsplit(|c| c == ' ' || c == ':')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches("DepthCam")
        .to_string();
    CameraDescription {
        name: card.trim().to_string(),
        serial,
        firmware: String::new(),
    }
}

/// Convierte un buffer `Y16` (u16 little-endian) en `Vec<u16>`.
pub fn depth_from_y16(buf: &[u8], width: u32, height: u32) -> Vec<u16> {
    let n = (width as usize) * (height as usize);
    let mut out = Vec::with_capacity(n);
    let take = n.min(buf.len() / 2);
    for i in 0..take {
        out.push(u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]));
    }
    out.resize(n, 0);
    out
}

/// Decodifica un buffer MJPG/JPEG a `RgbFrame` (RGB8) con zune-jpeg.
pub fn decode_mjpg(bytes: &[u8]) -> Option<RgbFrame> {
    use zune_jpeg::JpegDecoder;
    let mut decoder = JpegDecoder::new(bytes);
    match decoder.decode() {
        Ok(pixels) => {
            let (w, h) = decoder.dimensions().unwrap_or((0, 0));
            if w == 0 || h == 0 || pixels.len() < w * h * 3 {
                return None;
            }
            Some(RgbFrame {
                width: w as u32,
                height: h as u32,
                rgb: pixels,
            })
        }
        Err(e) => {
            log::warn!("Fallo al decodificar MJPG del stream RGB: {e:?}");
            None
        }
    }
}

/// Escala de profundidad por defecto (mm por unidad). El SDK usaba 0.1 para la
/// serie POP; el análisis del stream Y16 (valores ~8000 a ~80 cm) lo confirma.
pub const DEFAULT_DEPTH_SCALE: f32 = 0.1;

/// FOV horizontal del sensor de profundidad del POP 2 (grados). Derivado de las
/// specs oficiales: área ~220 mm de ancho a 400 mm de distancia y sensor nativo
/// 1280×800 (aspecto 1.6) → HFOV ≈ 30.7°. No es la calibración de fábrica exacta
/// (esa vive tras el HID propietario), pero es físicamente realista; el ajuste
/// fino de la UI permite afinarlo con un objeto de tamaño conocido.
pub const DEPTH_HFOV_DEG: f32 = 30.7;

/// Focal en píxeles a partir del FOV horizontal y el ancho del frame, asumiendo
/// píxeles cuadrados (fx = fy).
fn focal_from_hfov(width: u32, hfov_deg: f32) -> f32 {
    let half = (hfov_deg * 0.5).to_radians();
    (width as f32 * 0.5) / half.tan()
}

fn intrinsics_from_hfov(width: u32, height: u32, hfov_deg: f32) -> Intrinsics {
    let f = focal_from_hfov(width, hfov_deg);
    Intrinsics {
        width: width as i16,
        height: height as i16,
        fx: f,
        fy: f,
        cx: width as f32 * 0.5,
        cy: height as f32 * 0.5,
        ..Default::default()
    }
}

/// Intrínsecos de profundidad por defecto, desde el FOV real del POP 2.
pub fn default_depth_intrinsics(width: u32, height: u32) -> Intrinsics {
    intrinsics_from_hfov(width, height, DEPTH_HFOV_DEG)
}

/// Intrínsecos RGB por defecto (FOV similar; solo para colorear de forma
/// orientativa mientras no tengamos extrínsecos reales).
pub fn default_rgb_intrinsics(width: u32, height: u32) -> Intrinsics {
    intrinsics_from_hfov(width, height, DEPTH_HFOV_DEG)
}
