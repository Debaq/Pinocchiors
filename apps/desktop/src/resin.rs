//! Laminar para resina: las placas de Preparar pasan por el núcleo SLA de `libs/slicer`
//! (elevar, ventear, soportes, capas, revisión) y salen como `.ctb` (LD-002H).
//!
//! Ver `libs/pinocchio/print3d/PLAN_SLA.md`.

use crate::commands::{in_background, report, Progress};
use crate::print_prep::{plan_plates, plate_sources, PlateInput};
use crate::state::AppState;
use pinocchio_slicer::sla::{self, Layer, ResinPrinter, ResinSettings};
use pinocchio_slicer::sla_calibration::{self, CalibrationRow};
use pinocchio_slicer::sla_check::{Cavity, Island};
use pinocchio_slicer::sla_layers::{self, LayerParams};
use pinocchio_slicer::sla_hollow::HollowSettings;
use pinocchio_slicer::sla_orient::{self, Orientation, Turn};
use pinocchio_slicer::sla_prepare::{self, PrepareSettings, VentHole};
use pinocchio_slicer::sla_profiles::{self, ResinProfile};
use pinocchio_slicer::sla_supports::{SkipReason, SupportDensity, SupportPoint, SupportSettings};
use pinocchio_slicer::{ctb, cxdlp};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::AppHandle;

type Tri = [[f32; 3]; 3];

/// Solo una preparación a la vez.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Lo último laminado para resina: capas, piezas para el visor y el archivo listo.
static LAST: Mutex<Option<LastResin>> = Mutex::new(None);

struct LastResin {
    printer: &'static ResinPrinter,
    settings: ResinSettings,
    layers: Vec<Layer>,
    model: Vec<Tri>,
    supports: Vec<Tri>,
    /// Píxeles de la caja de todas las capas: `[x0, y0, x1, y1)`
    bbox: [u32; 4],
    /// El archivo para la impresora (si su formato ya se escribe)
    file: Option<Vec<u8>>,
}

#[derive(Serialize)]
pub struct ResinPrinterInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub resolution: [u32; 2],
    /// Pantalla y alto, mm
    pub bed: [f32; 3],
    /// Tamaño del píxel, µm
    pub pixel_um: [f32; 2],
    /// Extensión del archivo
    pub format: &'static str,
    /// Ya se puede escribir ese formato
    pub can_write: bool,
}

#[derive(Serialize)]
pub struct ResinInfo {
    pub printers: Vec<ResinPrinterInfo>,
    pub profiles: Vec<ResinProfile>,
    /// Soportes de fábrica: liviano, medio, pesado
    pub supports: Vec<(SupportDensity, SupportSettings)>,
    pub prepare: PrepareSettings,
    /// Vaciado de fábrica (pared, relleno)
    pub hollow: HollowSettings,
    /// Hileras de la pieza de calibración (de adelante hacia atrás)
    pub calibration: Vec<CalibrationRow>,
}

fn format_of(p: &ResinPrinter) -> (&'static str, bool) {
    match p.id {
        "ld-002h" => ("ctb", true),
        // CXDLP: se escribe cuando el usuario confirmó la unidad de la exposición (archivo de prueba)
        _ => ("cxdlp", true),
    }
}

/// El archivo de la impresora: `.ctb` para la LD-002H, `.cxdlp` para la Halot-Sky (si ya
/// se confirmó la unidad de la exposición; si no, `Ok(None)`).
fn printer_file(layers: &[sla::Layer], pr: &ResinPrinter, settings: &ResinSettings, parts: &[Vec<Tri>]) -> Result<Option<Vec<u8>>, String> {
    let preview = |(w, h): (u32, u32)| ctb::preview_image(parts, (w, h), [180, 190, 200]);
    match format_of(pr).0 {
        "ctb" => Ok(Some(ctb::write_ctb(layers, pr, settings, &preview(ctb::LARGE_PREVIEW), &preview(ctb::SMALL_PREVIEW)).map_err(|e| e.to_string())?)),
        _ => {
            let Some(unit) = settings.cxdlp.exposure_unit else { return Ok(None) };
            let previews = cxdlp::PREVIEWS.map(preview);
            Ok(Some(cxdlp::write_cxdlp(layers, pr, settings, unit, settings.cxdlp.mirror, &previews, None).map_err(|e| e.to_string())?))
        }
    }
}

/// Archivo de prueba para la Halot-Sky: una «F» de 1 mm con la exposición normal en el
/// valor crudo 35 (la pantalla dice 3,5 s si son décimas, 35 s si son segundos). Con el
/// espejo de `settings.cxdlp`. Se guarda en `path`.
#[tauri::command]
pub fn save_resin_probe(path: String, settings: ResinSettings) -> Result<u64, String> {
    let pr = &sla::HALOT_SKY;
    let piece = cxdlp::probe_piece(pr);
    let layers = sla::slice(&piece, pr, settings.layer_height).map_err(|e| e.to_string())?;
    let parts = vec![piece];
    let previews = cxdlp::PREVIEWS.map(|(w, h)| ctb::preview_image(&parts, (w, h), [180, 190, 200]));
    let bytes = cxdlp::write_cxdlp(&layers, pr, &settings, cxdlp::ExposureUnit::Tenths, settings.cxdlp.mirror, &previews, Some(cxdlp::PROBE_RAW_EXPOSURE))
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo guardar el archivo: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Impresoras de resina, perfiles y soportes de fábrica.
#[tauri::command]
pub fn resin_presets() -> ResinInfo {
    let printers = sla::printers()
        .into_iter()
        .map(|p| {
            let (pw, ph) = p.pixel_size();
            let (format, can_write) = format_of(p);
            ResinPrinterInfo {
                id: p.id,
                name: match p.id {
                    "halot-sky" => "Creality Halot-Sky (CL-89)",
                    _ => p.name,
                },
                resolution: [p.res_x, p.res_y],
                bed: [p.bed_x, p.bed_y, p.bed_z],
                pixel_um: [pw * 1000.0, ph * 1000.0],
                format,
                can_write,
            }
        })
        .collect();
    ResinInfo {
        printers,
        profiles: sla_profiles::profiles(),
        supports: [SupportDensity::Light, SupportDensity::Medium, SupportDensity::Heavy].into_iter().map(|d| (d, SupportSettings::preset(d))).collect(),
        prepare: PrepareSettings::default(),
        hollow: HollowSettings::default(),
        calibration: sla_calibration::rows(),
    }
}

/// Avisos sobre los parámetros de exposición (rangos razonables).
#[tauri::command]
pub fn resin_validate(printer: String, settings: ResinSettings) -> Result<Vec<String>, String> {
    let p = sla::printer(&printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
    Ok(sla_profiles::validate(&settings, p))
}

#[derive(Serialize)]
pub struct ResinResult {
    pub plate: usize,
    pub plates: usize,
    pub unplaced: Vec<String>,
    pub layers: usize,
    pub height_mm: f32,
    pub time_s: f32,
    pub volume_ml: f32,
    pub grams: f32,
    pub cost: f32,
    pub max_area_mm2: f32,
    pub max_area_layer: usize,
    /// Cuánto se subió la pieza sobre la placa (mm)
    pub lift: f32,
    pub islands: Vec<Island>,
    pub tiny_islands: usize,
    pub cavities: Vec<Cavity>,
    pub unvented: Vec<Cavity>,
    pub vents: Vec<VentHole>,
    pub points: Vec<SupportPoint>,
    pub skipped: Vec<(usize, SkipReason)>,
    /// Columnas hasta la balsa, puntas inclinadas y refuerzos
    pub columns: usize,
    pub tilted: usize,
    pub braces: usize,
    /// Resina ahorrada al vaciar (ml) y tapones de drenaje puestos en la placa
    pub hollowed_ml: f32,
    pub plugs: usize,
    /// El nombre que quedó en la balsa (vacío = ninguno)
    pub label: String,
    pub notes: Vec<String>,
    /// Avisos de los parámetros
    pub warnings: Vec<String>,
    /// Cama (mm) para dibujarla
    pub bed: [f32; 2],
    pub format: &'static str,
    /// Bytes del archivo, si el formato ya se escribe
    pub file_bytes: Option<u64>,
    /// z de la cara de arriba de cada capa (mm)
    pub layer_z: Vec<f32>,
    /// Área expuesta de cada capa (mm²), para el gráfico de despegue
    pub layer_area: Vec<f32>,
    /// Lo que hace la impresora en cada capa (exposición, subida...)
    pub layer_params: Vec<LayerParams>,
}

/// Prepara (venteos, soportes), lamina y revisa una placa para la impresora de resina.
/// Con `points` se usan esos puntos de soporte en vez de los automáticos; `tilt` gira la
/// placa por el centro antes de preparar (X, después Y, después Z).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn resin_slice(
    app: AppHandle,
    input: PlateInput,
    plate: usize,
    printer: String,
    settings: ResinSettings,
    prepare: PrepareSettings,
    points: Option<Vec<SupportPoint>>,
    tilt: Option<Turn>,
    on_progress: Channel<Progress>,
) -> Result<ResinResult, String> {
    in_background(app, move |state| resin_slice_impl(state, input, plate, &printer, &settings, &prepare, points.as_deref(), tilt, &on_progress)).await
}

/// El laminado de resina sin Tauri (también lo usa el conector MCP). Uno a la vez.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resin_slice_impl(
    state: &AppState,
    input: PlateInput,
    plate: usize,
    printer: &str,
    settings: &ResinSettings,
    prepare: &PrepareSettings,
    points: Option<&[SupportPoint]>,
    tilt: Option<Turn>,
    on_progress: &Channel<Progress>,
) -> Result<ResinResult, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Ya hay un laminado en curso".into());
    }
    let result = plate_sources(state, &input).and_then(|list| slice_plate(list, &input, plate, printer, settings, prepare, points, tilt, |stage, p, msg| report(on_progress, stage, p, msg)));
    RUNNING.store(false, Ordering::SeqCst);
    result
}

/// Todo el laminado de resina de una placa; deja el resultado en `LAST`.
#[allow(clippy::too_many_arguments)]
fn slice_plate(
    list: Vec<(String, pinocchio_mesh::Mesh)>,
    input: &PlateInput,
    plate: usize,
    printer: &str,
    settings: &ResinSettings,
    prepare: &PrepareSettings,
    points: Option<&[SupportPoint]>,
    tilt: Option<Turn>,
    progress: impl Fn(&str, u32, &str),
) -> Result<ResinResult, String> {
    let pr = sla::printer(printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
    progress("plates", 2, "Acomodando las piezas...");
    let (mut tris, plates, unplaced) = plate_triangles(list, input, pr, prepare, plate)?;
    if let Some(t) = tilt.filter(|t| !t.is_zero()) {
        tris = sla_orient::turn(&tris, t);
    }
    finish(tris, pr, settings, prepare, points, (plate, plates, unplaced), progress)
}

/// Prepara, lamina y escribe el archivo de estos triángulos; deja el resultado en `LAST`.
fn finish(
    tris: Vec<Tri>,
    pr: &'static ResinPrinter,
    settings: &ResinSettings,
    prepare: &PrepareSettings,
    points: Option<&[SupportPoint]>,
    (plate, plates, unplaced): (usize, usize, Vec<String>),
    progress: impl Fn(&str, u32, &str),
) -> Result<ResinResult, String> {
    progress("prepare", 15, "Venteos, soportes y capas...");
    let p = sla_prepare::prepare(&tris, pr, settings, prepare, points).map_err(|e| e.to_string())?;
    progress("file", 85, "Escribiendo el archivo...");
    let (format, _) = format_of(pr);
    let file = printer_file(&p.layers, pr, settings, &[p.model.clone(), p.supports.clone()])?;
    let volume = sla::volume_ml(&p.layers, pr, settings.layer_height);
    let layer_z: Vec<f32> = p.layers.iter().map(|l| l.z).collect();
    let layer_area = sla_layers::layer_areas(&p.layers, pr);
    let layer_params = sla_layers::layer_params(&layer_area, &layer_z, settings);
    let mut warnings = sla_profiles::validate(settings, pr);
    if file.is_none() {
        warnings.push(format!("Todavía no se puede guardar el archivo .{format}: falta confirmar en qué unidad lee la exposición la impresora (archivo de prueba en Impresora y resina)."));
    }
    let result = ResinResult {
        plate,
        plates,
        unplaced,
        layers: p.layers.len(),
        height_mm: p.layers.last().map_or(0.0, |l| l.z),
        time_s: sla_layers::total_time(&layer_params),
        volume_ml: volume,
        grams: volume * settings.density,
        cost: volume * settings.cost_per_ml,
        max_area_mm2: p.report.max_area_mm2,
        max_area_layer: p.report.max_area_layer,
        lift: p.lift,
        islands: p.report.islands.iter().take(200).cloned().collect(),
        tiny_islands: p.report.tiny_islands,
        cavities: p.report.cavities.clone(),
        unvented: p.unvented.clone(),
        vents: p.vents.clone(),
        points: p.points.clone(),
        skipped: p.skipped.clone(),
        columns: p.columns,
        tilted: p.tilted,
        braces: p.braces,
        hollowed_ml: p.hollowed_ml,
        plugs: p.plugs,
        label: p.label.clone(),
        notes: p.notes.clone(),
        warnings,
        bed: [pr.bed_x, pr.bed_y],
        format,
        file_bytes: file.as_ref().map(|f| f.len() as u64),
        layer_z,
        layer_area,
        layer_params,
    };
    let bbox = layers_bbox(&p.layers, pr);
    *LAST.lock().unwrap() = Some(LastResin { printer: pr, settings: settings.clone(), layers: p.layers, model: p.model, supports: p.supports, bbox, file });
    progress("done", 100, "Laminado listo");
    Ok(result)
}

/// Lamina la pieza de calibración (centrada, directo sobre la placa, sin soportes) para
/// verla y guardarla con varias exposiciones.
#[tauri::command]
pub async fn resin_calibration(app: AppHandle, printer: String, settings: ResinSettings) -> Result<ResinResult, String> {
    in_background(app, move |_| calibration(&printer, &settings)).await
}

pub(crate) fn calibration(printer: &str, settings: &ResinSettings) -> Result<ResinResult, String> {
    let pr = sla::printer(printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
    let [w, d, _] = sla_calibration::SIZE;
    let (dx, dy) = ((pr.bed_x - w) / 2.0, (pr.bed_y - d) / 2.0);
    let tris = sla_calibration::calibration_piece().into_iter().map(|t| t.map(|v| [v[0] + dx, v[1] + dy, v[2]])).collect();
    let prepare = PrepareSettings { supports: None, vents: false, ..Default::default() };
    finish(tris, pr, settings, &prepare, None, (0, 1, Vec::new()), |_, _, _| {})
}

/// Guarda la pieza de calibración laminada una vez por exposición en `dir`
/// (`calibracion-<impresora>-<exposición>s.<ext>`); devuelve las rutas.
#[tauri::command]
pub fn save_resin_calibration(dir: String, exposures: Vec<f32>) -> Result<Vec<String>, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    let (format, _) = format_of(last.printer);
    let parts = vec![last.model.clone()];
    let mut out = Vec::new();
    for e in exposures {
        let settings = ResinSettings { exposure: e, exposure_ranges: Vec::new(), ..last.settings.clone() };
        let bytes = printer_file(&last.layers, last.printer, &settings, &parts)?.ok_or("Todavía no se puede guardar el archivo de esta impresora: falta confirmar la unidad de la exposición")?;
        let name = format!("calibracion-{}-{}s.{format}", last.printer.id, format!("{e:.2}").trim_end_matches('0').trim_end_matches('.'));
        let path = std::path::Path::new(&dir).join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("No se pudo guardar el archivo: {e}"))?;
        out.push(path.display().to_string());
    }
    Ok(out)
}

/// Busca los mejores giros de la placa (área por capa, succión, voladizos, altura).
/// `overhang_angle`: el «Ángulo límite» de los soportes (45° si falta).
#[tauri::command]
pub async fn resin_tilts(
    app: AppHandle,
    input: PlateInput,
    plate: usize,
    printer: String,
    layer_height: f32,
    overhang_angle: Option<f32>,
) -> Result<Vec<Orientation>, String> {
    in_background(app, move |state| resin_tilts_impl(state, &input, plate, &printer, layer_height, overhang_angle)).await
}

pub(crate) fn resin_tilts_impl(
    state: &AppState,
    input: &PlateInput,
    plate: usize,
    printer: &str,
    layer_height: f32,
    overhang_angle: Option<f32>,
) -> Result<Vec<Orientation>, String> {
    let pr = sla::printer(printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
    let (tris, _, _) = plate_triangles(plate_sources(state, input)?, input, pr, &PrepareSettings::default(), plate)?;
    Ok(sla_orient::auto_orient(&tris, pr, layer_height, overhang_angle.unwrap_or(45.0), 5))
}

/// Piezas de la placa elegida como triángulos (mm, Z arriba), centradas en la pantalla.
/// La cama se achica lo que sobresale la balsa.
fn plate_triangles(
    list: Vec<(String, pinocchio_mesh::Mesh)>,
    input: &PlateInput,
    pr: &ResinPrinter,
    prepare: &PrepareSettings,
    plate: usize,
) -> Result<(Vec<Tri>, usize, Vec<String>), String> {
    let edge = prepare.supports.as_ref().map_or(0.5, |s| s.raft_margin + s.foot_diameter / 2.0 + 0.5) as f64;
    let mut inner = input.clone();
    inner.printer.build_volume = [pr.bed_x as f64 - 2.0 * edge, pr.bed_y as f64 - 2.0 * edge, pr.bed_z as f64];
    let plan = plan_plates(list, &inner)?;
    let plates = plan.plates.len();
    let chosen = plan.plates.get(plate).ok_or("Esa placa no existe")?;
    let mut tris: Vec<Tri> = chosen
        .meshes
        .iter()
        .flat_map(|(_, mesh)| (0..mesh.num_faces()).map(|f| mesh.get_face_positions(f).map(|p| [p.0.x as f32, p.0.y as f32, p.0.z as f32])))
        .collect();
    if tris.is_empty() {
        return Err("No hay piezas que laminar".into());
    }
    // Al centro de la pantalla
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for v in tris.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let shift = [pr.bed_x / 2.0 - (lo[0] + hi[0]) / 2.0, pr.bed_y / 2.0 - (lo[1] + hi[1]) / 2.0];
    for v in tris.iter_mut().flatten() {
        v[0] += shift[0];
        v[1] += shift[1];
    }
    Ok((tris, plates, plan.unplaced))
}

fn layers_bbox(layers: &[Layer], pr: &ResinPrinter) -> [u32; 4] {
    let mut b = [u32::MAX, u32::MAX, 0, 0];
    for s in layers.iter().flat_map(Layer::image_spans) {
        b[0] = b[0].min(s.x0);
        b[1] = b[1].min(s.y);
        b[2] = b[2].max(s.x1);
        b[3] = b[3].max(s.y + 1);
    }
    if b[0] >= b[2] { [0, 0, pr.res_x, pr.res_y] } else { b }
}

/// Máscara de la capa `index`, recortada a la caja de todas las capas y achicada a
/// `max_size` px como mucho: `[u32 w][u32 h][f32 x0 y0 (mm)][f32 celda x y (mm)][u8 gris…]`,
/// fila 0 = y más chica.
#[tauri::command]
pub fn resin_layer(index: usize, max_size: u32) -> Result<tauri::ipc::Response, String> {
    layer_image(index, max_size).map(tauri::ipc::Response::new)
}

fn layer_image(index: usize, max_size: u32) -> Result<Vec<u8>, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    let layer = last.layers.get(index).ok_or("Esa capa no existe")?;
    let [x0, y0, x1, y1] = last.bbox;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let scale = bw.max(bh).div_ceil(max_size.max(16)).max(1);
    let (w, h) = (bw.div_ceil(scale), bh.div_ceil(scale));
    let mut acc = vec![0u32; (w * h) as usize];
    for s in layer.image_spans() {
        if s.y < y0 || s.y >= y1 {
            continue;
        }
        let row = ((s.y - y0) / scale) as usize * w as usize;
        for x in s.x0.max(x0)..s.x1.min(x1) {
            acc[row + ((x - x0) / scale) as usize] += s.v as u32;
        }
    }
    let full = scale * scale * 255;
    let (pw, ph) = last.printer.pixel_size();
    let (ox, oy) = last.printer.pixel_to_mm(x0, y0);
    let mut out = Vec::with_capacity(24 + acc.len());
    for v in [w, h] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [ox - pw / 2.0, oy - ph / 2.0, pw * scale as f32, ph * scale as f32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend(acc.iter().map(|&a| (a as u64 * 255 / full as u64) as u8));
    Ok(out)
}

/// Pieza elevada y soportes para el visor: `[u32 n][u32 m][f32 9n pieza][f32 9m soportes]`
/// (mm, Z arriba, esquina de la pantalla en el origen).
#[tauri::command]
pub fn resin_view() -> Result<tauri::ipc::Response, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    let mut out = Vec::with_capacity(8 + 36 * (last.model.len() + last.supports.len()));
    out.extend_from_slice(&(last.model.len() as u32).to_le_bytes());
    out.extend_from_slice(&(last.supports.len() as u32).to_le_bytes());
    for c in last.model.iter().chain(&last.supports).flatten().flatten() {
        out.extend_from_slice(&c.to_le_bytes());
    }
    Ok(tauri::ipc::Response::new(out))
}

/// Guarda el archivo de la impresora de resina donde diga el usuario.
#[tauri::command]
pub fn save_resin(path: String) -> Result<u64, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    let file = last.file.as_ref().ok_or("Todavía no se puede guardar el archivo de esta impresora")?;
    std::fs::write(&path, file).map_err(|e| format!("No se pudo guardar el archivo: {e}"))?;
    Ok(file.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;
    use pinocchio_mesh::Mesh;
    use pinocchio_print3d::{MaterialProfile, PrinterProfile};

    fn cube(size: f64) -> Mesh {
        let p = |x: f64, y: f64, z: f64| Vector3::new(x * size, y * size, z * size);
        let v = vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        Mesh::from_triangles(&v, &f)
    }

    /// Los tests que pasan por `LAST` van de a uno
    static LAST_LOCK: Mutex<()> = Mutex::new(());

    fn input() -> PlateInput {
        PlateInput {
            printer: PrinterProfile::new("Creality LD-002H", [82.62, 130.56, 160.0], 0.15, 0.05),
            material: MaterialProfile::default(),
            format: "stl".into(),
            spacing: 5.0,
            orient: false,
            contour: false,
            hollow_wall: None,
            drain: None,
            vent: false,
            compensate: false,
            path: String::new(),
            objects: Vec::new(),
        }
    }

    #[test]
    fn laminado_completo_con_mascara() {
        let _lock = LAST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let list = vec![("a".to_string(), cube(10.0))];
        let r = slice_plate(list, &input(), 0, "ld-002h", &ResinSettings::default(), &PrepareSettings::default(), None, Some(Turn { x: 30.0, y: 0.0, z: 0.0 }), |_, _, _| {}).unwrap();
        assert!(r.islands.is_empty() && r.cavities.is_empty(), "{:?} {:?}", r.islands, r.cavities);
        assert!(r.file_bytes.unwrap() > 1000);
        assert!((r.volume_ml - 1.0).abs() < 0.6, "{}", r.volume_ml);
        let img = layer_image(r.layers / 2, 100).unwrap();
        let (w, h) = (u32::from_le_bytes(img[0..4].try_into().unwrap()), u32::from_le_bytes(img[4..8].try_into().unwrap()));
        assert!(w <= 100 && h <= 100 && img.len() == 24 + (w * h) as usize);
        assert!(img[24..].contains(&255));
        assert!(slice_plate(vec![], &input(), 0, "otra", &ResinSettings::default(), &PrepareSettings::default(), None, None, |_, _, _| {}).is_err());
    }

    /// Datos reales para probar la interfaz sin Tauri: `PINOCCHIO_RESIN_STL=pieza.stl
    /// PINOCCHIO_RESIN_OUT=carpeta cargo test -p pinocchio-app --lib datos_para_la_interfaz -- --ignored`
    #[test]
    #[ignore]
    fn datos_para_la_interfaz() {
        let _lock = LAST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (Ok(stl), Ok(out)) = (std::env::var("PINOCCHIO_RESIN_STL"), std::env::var("PINOCCHIO_RESIN_OUT")) else { return };
        let b = std::fs::read(stl).unwrap();
        let n = u32::from_le_bytes(b[80..84].try_into().unwrap()) as usize;
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as f64;
        let verts: Vec<Vector3> = (0..n * 3).map(|i| Vector3::new(f(84 + 50 * (i / 3) + 12 + 12 * (i % 3)), f(88 + 50 * (i / 3) + 12 + 12 * (i % 3)), f(92 + 50 * (i / 3) + 12 + 12 * (i % 3)))).collect();
        let faces: Vec<[usize; 3]> = (0..n).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
        let list = vec![("pieza".to_string(), Mesh::from_triangles(&verts, &faces))];
        // v3 con subida dinámica, para ver la curva de velocidad en el gráfico.
        let mut settings = ResinSettings { ctb_version: 3, ..Default::default() };
        settings.dynamic_lift.enabled = true;
        let r = slice_plate(list, &input(), 0, "ld-002h", &settings, &PrepareSettings::default(), None, None, |_, _, _| {}).unwrap();
        let dir = std::path::Path::new(&out);
        std::fs::write(dir.join("presets.json"), serde_json::to_string(&resin_presets()).unwrap()).unwrap();
        std::fs::write(dir.join("result.json"), serde_json::to_string(&r).unwrap()).unwrap();
        std::fs::write(dir.join("layer.bin"), layer_image(r.layers / 3, 360).unwrap()).unwrap();
    }

    #[test]
    fn calibracion_con_varias_exposiciones() {
        let _lock = LAST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let r = calibration("ld-002h", &ResinSettings::default()).unwrap();
        assert_eq!(r.layers, 130);
        assert!(r.islands.is_empty(), "{:?}", r.islands);
        let dir = std::env::temp_dir().join(format!("pinocchio-calibracion-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = save_resin_calibration(dir.display().to_string(), vec![2.5, 3.0, 3.75]).unwrap();
        assert!(files[2].ends_with("calibracion-ld-002h-3.75s.ctb"), "{files:?}");
        let b = std::fs::read(&files[0]).unwrap();
        let f = ctb::CtbFile::parse(&b).unwrap();
        assert_eq!(f.exposure, 2.5);
        assert_eq!(f.layers[20].1, 2.5);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dos_cubos_quedan_centrados_y_se_laminan() {
        let list = vec![("a".to_string(), cube(10.0)), ("b".to_string(), cube(8.0))];
        let (tris, plates, unplaced) = plate_triangles(list, &input(), &sla::LD_002H, &PrepareSettings::default(), 0).unwrap();
        assert_eq!((plates, unplaced.len()), (1, 0));
        let p = sla_prepare::prepare(&tris, &sla::LD_002H, &ResinSettings::default(), &PrepareSettings::default(), None).unwrap();
        assert!(p.report.islands.is_empty(), "{:?}", p.report.islands);
        let b = layers_bbox(&p.layers, &sla::LD_002H);
        let cx = (b[0] + b[2]) as f32 / 2.0;
        assert!((cx - 810.0).abs() < 40.0, "{b:?}");
        let file = ctb::write_ctb(&p.layers, &sla::LD_002H, &ResinSettings::default(), &ctb::preview_image(std::slice::from_ref(&tris), (32, 32), [200; 3]), &ctb::preview_image(&[tris], (16, 16), [200; 3])).unwrap();
        assert_eq!(ctb::CtbFile::parse(&file).unwrap().layers.len(), p.layers.len());
    }
}
