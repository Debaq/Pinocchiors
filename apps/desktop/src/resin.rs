//! Laminar para resina: las placas de Preparar pasan por el núcleo SLA de `libs/slicer`
//! (elevar, ventear, soportes, capas, revisión) y salen como `.ctb` (LD-002H).
//!
//! Ver `libs/pinocchio/print3d/PLAN_SLA.md`.

use crate::commands::{in_background, report, Progress};
use crate::print_prep::{plan_plates, sources, PlateInput};
use pinocchio_slicer::sla::{self, Layer, ResinPrinter, ResinSettings};
use pinocchio_slicer::sla_check::{Cavity, Island};
use pinocchio_slicer::sla_prepare::{self, Axis, PrepareSettings, TiltOption, VentHole};
use pinocchio_slicer::sla_profiles::{self, ResinProfile};
use pinocchio_slicer::sla_supports::{SkipReason, SupportDensity, SupportPoint, SupportSettings};
use pinocchio_slicer::ctb;
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
}

fn format_of(p: &ResinPrinter) -> (&'static str, bool) {
    match p.id {
        "ld-002h" => ("ctb", true),
        // CXDLP descifrado (SLA0) pero falta confirmar la unidad de exposición
        _ => ("cxdlp", false),
    }
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
                name: p.name,
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
}

/// Prepara (venteos, soportes), lamina y revisa una placa para la impresora de resina.
/// Con `points` se usan esos puntos de soporte en vez de los automáticos; `tilt` gira la
/// placa por el centro antes de preparar.
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
    tilt: Option<(Axis, f32)>,
    on_progress: Channel<Progress>,
) -> Result<ResinResult, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Ya hay un laminado en curso".into());
    }
    let result = in_background(app, move |state| {
        let list = sources(state)?;
        slice_plate(list, &input, plate, &printer, &settings, &prepare, points.as_deref(), tilt, |stage, p, msg| report(&on_progress, stage, p, msg))
    })
    .await;
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
    tilt: Option<(Axis, f32)>,
    progress: impl Fn(&str, u32, &str),
) -> Result<ResinResult, String> {
    let pr = sla::printer(printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
    progress("plates", 2, "Acomodando las piezas...");
    let (mut tris, plates, unplaced) = plate_triangles(list, input, pr, prepare, plate)?;
    if let Some((axis, deg)) = tilt.filter(|t| t.1 != 0.0) {
        tris = sla_prepare::rotate(&tris, axis, deg);
    }
    progress("prepare", 15, "Venteos, soportes y capas...");
    let p = sla_prepare::prepare(&tris, pr, settings, prepare, points).map_err(|e| e.to_string())?;
    progress("file", 85, "Escribiendo el archivo...");
    let (format, can_write) = format_of(pr);
    let file = if can_write {
        let parts = vec![p.model.clone(), p.supports.clone()];
        let preview = |(w, h): (u32, u32)| ctb::preview_image(&parts, (w, h), [180, 190, 200]);
        Some(ctb::write_ctb(&p.layers, pr, settings, &preview(ctb::LARGE_PREVIEW), &preview(ctb::SMALL_PREVIEW)).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let volume = sla::volume_ml(&p.layers, pr, settings.layer_height);
    let mut warnings = sla_profiles::validate(settings, pr);
    if !can_write {
        warnings.push(format!("Todavía no se puede guardar el archivo .{format} de esta impresora (falta confirmar la unidad de exposición)."));
    }
    let result = ResinResult {
        plate,
        plates,
        unplaced,
        layers: p.layers.len(),
        height_mm: p.layers.last().map_or(0.0, |l| l.z),
        time_s: sla::print_time(p.layers.len(), settings),
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
        notes: p.notes.clone(),
        warnings,
        bed: [pr.bed_x, pr.bed_y],
        format,
        file_bytes: file.as_ref().map(|f| f.len() as u64),
        layer_z: p.layers.iter().map(|l| l.z).collect(),
    };
    let bbox = layers_bbox(&p.layers, pr);
    *LAST.lock().unwrap() = Some(LastResin { printer: pr, layers: p.layers, model: p.model, supports: p.supports, bbox, file });
    progress("done", 100, "Laminado listo");
    Ok(result)
}

/// Compara inclinaciones de la placa (área por capa, succión, altura).
#[tauri::command]
pub async fn resin_tilts(app: AppHandle, input: PlateInput, plate: usize, printer: String, layer_height: f32) -> Result<Vec<TiltOption>, String> {
    in_background(app, move |state| {
        let pr = sla::printer(&printer).ok_or_else(|| format!("No existe la impresora de resina «{printer}»"))?;
        let (tris, _, _) = plate_triangles(sources(state)?, &input, pr, &PrepareSettings::default(), plate)?;
        Ok(sla_prepare::tilt_options(&tris, pr, layer_height, &[15.0, 30.0, 45.0]))
    })
    .await
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
    for s in layers.iter().flat_map(|l| &l.spans) {
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
    for s in &layer.spans {
        if s.y < y0 || s.y >= y1 {
            continue;
        }
        let row = ((s.y - y0) / scale) as usize * w as usize;
        for x in s.x0.max(x0)..s.x1.min(x1) {
            acc[row + ((x - x0) / scale) as usize] += 1;
        }
    }
    let full = scale * scale;
    let (pw, ph) = last.printer.pixel_size();
    let (ox, oy) = last.printer.pixel_to_mm(x0, y0);
    let mut out = Vec::with_capacity(24 + acc.len());
    for v in [w, h] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [ox - pw / 2.0, oy - ph / 2.0, pw * scale as f32, ph * scale as f32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend(acc.iter().map(|&a| (a * 255 / full) as u8));
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
        }
    }

    #[test]
    fn laminado_completo_con_mascara() {
        let list = vec![("a".to_string(), cube(10.0))];
        let r = slice_plate(list, &input(), 0, "ld-002h", &ResinSettings::default(), &PrepareSettings::default(), None, Some((Axis::X, 30.0)), |_, _, _| {}).unwrap();
        assert!(r.islands.is_empty() && r.cavities.is_empty(), "{:?} {:?}", r.islands, r.cavities);
        assert!(r.file_bytes.unwrap() > 1000);
        assert!((r.volume_ml - 1.0).abs() < 0.6, "{}", r.volume_ml);
        let img = layer_image(r.layers / 2, 100).unwrap();
        let (w, h) = (u32::from_le_bytes(img[0..4].try_into().unwrap()), u32::from_le_bytes(img[4..8].try_into().unwrap()));
        assert!(w <= 100 && h <= 100 && img.len() == 24 + (w * h) as usize);
        assert!(img[24..].iter().any(|&g| g == 255));
        assert!(slice_plate(vec![], &input(), 0, "otra", &ResinSettings::default(), &PrepareSettings::default(), None, None, |_, _, _| {}).is_err());
    }

    /// Datos reales para probar la interfaz sin Tauri: `PINOCCHIO_RESIN_STL=pieza.stl
    /// PINOCCHIO_RESIN_OUT=carpeta cargo test -p pinocchio-app --lib datos_para_la_interfaz -- --ignored`
    #[test]
    #[ignore]
    fn datos_para_la_interfaz() {
        let (Ok(stl), Ok(out)) = (std::env::var("PINOCCHIO_RESIN_STL"), std::env::var("PINOCCHIO_RESIN_OUT")) else { return };
        let b = std::fs::read(stl).unwrap();
        let n = u32::from_le_bytes(b[80..84].try_into().unwrap()) as usize;
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as f64;
        let verts: Vec<Vector3> = (0..n * 3).map(|i| Vector3::new(f(84 + 50 * (i / 3) + 12 + 12 * (i % 3)), f(88 + 50 * (i / 3) + 12 + 12 * (i % 3)), f(92 + 50 * (i / 3) + 12 + 12 * (i % 3)))).collect();
        let faces: Vec<[usize; 3]> = (0..n).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
        let list = vec![("pieza".to_string(), Mesh::from_triangles(&verts, &faces))];
        let r = slice_plate(list, &input(), 0, "ld-002h", &ResinSettings::default(), &PrepareSettings::default(), None, None, |_, _, _| {}).unwrap();
        let dir = std::path::Path::new(&out);
        std::fs::write(dir.join("presets.json"), serde_json::to_string(&resin_presets()).unwrap()).unwrap();
        std::fs::write(dir.join("result.json"), serde_json::to_string(&r).unwrap()).unwrap();
        std::fs::write(dir.join("layer.bin"), layer_image(r.layers / 3, 360).unwrap()).unwrap();
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
