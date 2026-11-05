//! Laminar: las placas de Preparar pasan por el motor de OrcaSlicer (proceso aparte).
//!
//! Ver `libs/slicer` y `libs/pinocchio/print3d/PLAN_SLICER.md`.

use crate::commands::{in_background, report, Progress};
use crate::print_prep::{plan_plates, sources, Plate, PlateInput, PlatePlan};
use pinocchio_mesh::Mesh;
use pinocchio_slicer as slicer;
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::AppHandle;

/// Se activa con «Cancelar»; el laminado en curso mata el motor al verlo.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Solo un laminado a la vez.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Último laminado: su gcode y la vista previa ya leída.
static LAST: Mutex<Option<LastSlice>> = Mutex::new(None);

struct LastSlice {
    gcode: PathBuf,
    preview: slicer::Preview,
}

#[derive(Serialize)]
pub struct SlicerInfo {
    /// Hay motor instalado
    pub available: bool,
    pub engine: Option<String>,
    pub orca_version: &'static str,
    pub printers: Vec<slicer::PrinterEntry>,
}

/// Impresoras, procesos y filamentos del laminador, y si el motor está.
#[tauri::command]
pub async fn slicer_presets() -> SlicerInfo {
    let engine = slicer::find_engine();
    SlicerInfo {
        available: engine.is_some(),
        engine: engine.map(|p| p.display().to_string()),
        orca_version: slicer::ORCA_VERSION,
        printers: slicer::index().printers.clone(),
    }
}

/// Valores que quedan con estos perfiles (sin cambios del usuario): los muestra el panel.
#[tauri::command]
pub fn slicer_config(printer: String, process: String, filament: String) -> Result<Map<String, Value>, String> {
    slicer::build_config(&slicer::SliceSettings { printer, process, filament, overrides: Default::default() }).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SliceResult {
    pub plate: usize,
    pub plates: usize,
    pub time_s: f64,
    pub filament_mm: f64,
    pub filament_cm3: f64,
    pub filament_g: f64,
    pub layers: u32,
    pub warnings: Vec<String>,
    /// Filamento por tipo de extrusión
    pub roles: Vec<slicer::RoleUse>,
    /// Tamaño de la cama (mm) para dibujarla en la vista previa
    pub bed: [f64; 2],
    pub gcode_bytes: u64,
    /// Piezas que no entraron en ninguna placa
    pub unplaced: Vec<String>,
    /// Capas y largos por tipo (los segmentos van aparte, ver `slicer_preview`)
    pub preview: slicer::Preview,
}

/// Lamina una placa (las piezas acomodadas como en «Exportar para imprimir»).
#[tauri::command]
pub async fn slice_plate(
    app: AppHandle,
    input: PlateInput,
    plate: usize,
    settings: slicer::SliceSettings,
    on_progress: Channel<Progress>,
) -> Result<SliceResult, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Ya hay un laminado en curso".into());
    }
    CANCEL.store(false, Ordering::SeqCst);
    let result = in_background(app, move |state| {
        let engine = slicer::find_engine().ok_or_else(|| slicer::SlicerError::EngineMissing.to_string())?;
        let config = slicer::build_config(&settings).map_err(|e| e.to_string())?;
        report(&on_progress, "plates", 2, "Acomodando las piezas...");
        let plan = slicing_plan(sources(state)?, &input, &config)?;
        let plates = plan.plates.len();
        let chosen = plan.plates.get(plate).ok_or("Esa placa no existe")?;
        let parts = plate_parts(chosen);
        let work_dir = std::env::temp_dir().join("pinocchio-slicer").join(std::process::id().to_string());
        let job = slicer::SliceJob { config, parts, work_dir };
        let summary = slicer::run(
            &job,
            &engine,
            |event| match event {
                slicer::Event::Progress { percent, text } => {
                    // 5–95 % del total: antes va el acomodo y después la lectura del gcode
                    let p = 5 + (percent.clamp(0, 100) as u32 * 90) / 100;
                    report(&on_progress, "slicing", p, text);
                }
                slicer::Event::Warning { text } => report(&on_progress, "warning", 0, text),
            },
            &CANCEL,
        )
        .map_err(|e| e.to_string())?;
        report(&on_progress, "preview", 96, "Leyendo el gcode...");
        let text = std::fs::read_to_string(&summary.gcode).map_err(|e| format!("No se pudo leer el gcode: {e}"))?;
        let preview = slicer::parse_gcode(&text);
        let gcode_bytes = text.len() as u64;
        let result = SliceResult {
            plate,
            plates,
            time_s: summary.time_s,
            filament_mm: summary.filament_mm,
            filament_cm3: summary.filament_cm3,
            filament_g: summary.filament_g,
            layers: summary.layers.max(preview.layer_z.len() as u32),
            warnings: summary.warnings,
            roles: summary.roles,
            bed: [input.printer.build_volume[0], input.printer.build_volume[1]],
            gcode_bytes,
            unplaced: plan.unplaced,
            preview: preview.clone(),
        };
        *LAST.lock().unwrap() = Some(LastSlice { gcode: summary.gcode, preview });
        report(&on_progress, "done", 100, "Laminado listo");
        Ok(result)
    })
    .await;
    RUNNING.store(false, Ordering::SeqCst);
    result
}

/// Piezas de una placa como triángulos (mm, Z arriba, ya en su lugar de la cama).
/// Acomoda las piezas dejando libre el borde de la cama que piden el brim y
/// la falda; después cada placa va al centro de la cama (como en Orca).
fn slicing_plan(list: Vec<(String, Mesh)>, input: &PlateInput, config: &Map<String, Value>) -> Result<PlatePlan, String> {
    let bed = [input.printer.build_volume[0], input.printer.build_volume[1]];
    let edge = edge_margin(config).min(bed[0].min(bed[1]) / 4.0);
    let mut inner = input.clone();
    inner.printer.build_volume[0] -= 2.0 * edge;
    inner.printer.build_volume[1] -= 2.0 * edge;
    let mut plan = plan_plates(list, &inner)?;
    for plate in &mut plan.plates {
        center_on_bed(plate, bed);
    }
    Ok(plan)
}

/// Lo que el brim o la falda se salen de las piezas (mm), con 1 mm de holgura.
fn edge_margin(config: &Map<String, Value>) -> f64 {
    let num = |k: &str| {
        let v = config.get(k);
        let v = match v {
            Some(Value::Array(a)) => a.first(),
            other => other,
        };
        v.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).unwrap_or(0.0).max(0.0)
    };
    let brim = match config.get("brim_type").and_then(Value::as_str) {
        Some("no_brim") => 0.0,
        _ => num("brim_width") + num("brim_object_gap"),
    };
    let skirt = if num("skirt_loops") > 0.0 { num("skirt_distance") + num("skirt_loops") * 0.5 } else { 0.0 };
    brim.max(skirt) + 1.0
}

/// Lleva las piezas de la placa al centro de la cama (sin cambiar cómo quedaron entre sí).
fn center_on_bed(plate: &mut Plate, bed: [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for (_, mesh) in &plate.meshes {
        for v in &mesh.vertices {
            for k in 0..2 {
                let c = v.position[k];
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
        }
    }
    if !lo[0].is_finite() || !hi[0].is_finite() {
        return;
    }
    let shift = [(bed[0] - (lo[0] + hi[0])) / 2.0, (bed[1] - (lo[1] + hi[1])) / 2.0];
    for (_, mesh) in &mut plate.meshes {
        pinocchio_print3d::translate(mesh, pinocchio_math::Vector3::new(shift[0] as _, shift[1] as _, 0.0));
    }
}

fn plate_parts(plate: &Plate) -> Vec<Vec<[[f32; 3]; 3]>> {
    plate
        .meshes
        .iter()
        .map(|(_, mesh)| (0..mesh.num_faces()).map(|f| mesh.get_face_positions(f).map(|p| [p.0.x as f32, p.0.y as f32, p.0.z as f32])).collect())
        .collect()
}

/// Corta el laminado en curso.
#[tauri::command]
pub fn slicer_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Segmentos del último laminado en binario (ver `Preview::to_bytes`).
#[tauri::command]
pub fn slicer_preview() -> Result<tauri::ipc::Response, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    Ok(tauri::ipc::Response::new(last.preview.to_bytes()))
}

/// El gcode del último laminado (para mandarlo a la impresora).
pub(crate) fn last_gcode() -> Option<PathBuf> {
    LAST.lock().ok()?.as_ref().map(|l| l.gcode.clone())
}

/// Guarda el gcode del último laminado donde diga el usuario.
#[tauri::command]
pub fn save_gcode(path: String) -> Result<u64, String> {
    let last = LAST.lock().unwrap();
    let last = last.as_ref().ok_or("Todavía no hay nada laminado")?;
    std::fs::copy(&last.gcode, &path).map_err(|e| format!("No se pudo guardar el gcode: {e}"))
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

    /// Dos cubos acomodados en la cama de la K1 y laminados con el motor real (si está).
    #[test]
    fn lamina_una_placa_de_preparar() {
        let Some(engine) = slicer::find_engine() else { return };
        let input = PlateInput {
            printer: PrinterProfile::new("Creality K1", [220.0, 220.0, 250.0], 0.4, 0.2),
            material: MaterialProfile::default(),
            format: "stl".into(),
            spacing: 5.0,
            orient: true,
            contour: false,
            hollow_wall: None,
            drain: None,
            vent: false,
            compensate: false,
            path: String::new(),
        };
        let k1 = slicer::index().printers.iter().find(|p| p.model == "Creality K1").unwrap();
        let settings = slicer::SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
        let config = slicer::build_config(&settings).unwrap();
        let plan = slicing_plan(vec![("a".into(), cube(15.0)), ("b".into(), cube(10.0))], &input, &config).unwrap();
        assert_eq!(plan.plates.len(), 1);
        let parts = plate_parts(&plan.plates[0]);
        assert_eq!(parts.len(), 2);
        let job = slicer::SliceJob {
            config,
            parts,
            work_dir: std::env::temp_dir().join(format!("pinocchio-slicer-app-{}", std::process::id())),
        };
        let summary = slicer::run(&job, &engine, |_| {}, &AtomicBool::new(false)).unwrap();
        // El cubo más alto (15 mm a 0,2 mm) manda: ~75 capas
        assert!((72..=78).contains(&summary.layers), "capas {}", summary.layers);
        let gcode = std::fs::read_to_string(&summary.gcode).unwrap();
        assert_eq!(gcode.matches("EXCLUDE_OBJECT_DEFINE").count(), 2);
        // Con el brim, nada se extruye fuera de la cama
        let preview = slicer::parse_gcode(&gcode);
        let _ = std::fs::remove_dir_all(&job.work_dir);
        let (lo, hi) = extruded_bounds(&gcode);
        assert!(lo[0] >= 0.0 && lo[1] >= 0.0 && hi[0] <= 220.0 && hi[1] <= 220.0, "fuera de la cama: {lo:?} {hi:?}");
        assert!(!preview.layer_z.is_empty());
    }

    /// Una pieza que entra justo en la cama sin margen: se acomoda igual con el
    /// borde libre y queda centrada
    #[test]
    fn la_placa_queda_centrada_con_borde_libre() {
        let input = PlateInput {
            printer: PrinterProfile::new("Creality K1", [220.0, 220.0, 250.0], 0.4, 0.2),
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
        };
        let mut config = Map::new();
        config.insert("brim_type".into(), "auto_brim".into());
        config.insert("brim_width".into(), "5".into());
        assert!((edge_margin(&config) - 6.0).abs() < 1e-9);
        let plan = slicing_plan(vec![("a".into(), cube(40.0))], &input, &config).unwrap();
        let bb = pinocchio_print3d::compute_bounding_box(&plan.plates[0].meshes[0].1);
        assert!((bb.min[0] - 90.0).abs() < 1e-6 && (bb.min[1] - 90.0).abs() < 1e-6, "{:?}", bb.min);
    }

    /// Lamina mallas reales (las que diga `PINOCCHIO_SLICER_MESHES`: un
    /// archivo con una ruta por línea) por el mismo camino que la app, con los
    /// ajustes por defecto de la K1, y dice cuáles fallan y por qué. Con
    /// `PINOCCHIO_SLICER_ORIENT=1`, orientadas como «Orientar» de Fabricar.
    ///   PINOCCHIO_SLICER_MESHES=lista.txt cargo test -p pinocchio-app --lib mallas_reales -- --ignored --nocapture
    #[test]
    #[ignore]
    fn lamina_mallas_reales() {
        let Ok(list) = std::env::var("PINOCCHIO_SLICER_MESHES") else { return };
        let engine = slicer::find_engine().expect("motor");
        let orient = std::env::var("PINOCCHIO_SLICER_ORIENT").is_ok_and(|v| v == "1");
        let k1 = slicer::index().printers.iter().find(|p| p.model == "Creality K1").unwrap();
        let settings = slicer::SliceSettings { printer: k1.name.clone(), process: k1.default_process.clone(), filament: k1.default_filament.clone(), overrides: Default::default() };
        let mut failed = Vec::new();
        for path in std::fs::read_to_string(list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
            let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
            let t0 = std::time::Instant::now();
            let outcome = (|| -> Result<String, String> {
                let mut mesh = pinocchio_mesh::load_mesh(path).map_err(|e| format!("carga: {e}"))?;
                // En metros u otra unidad chica: a 100 mm (acá importa lo complejo, no el tamaño)
                let bb = pinocchio_print3d::compute_bounding_box(&mesh);
                let size = bb.dimensions().iter().cloned().fold(0.0, f64::max);
                if size < 5.0 {
                    for v in &mut mesh.vertices {
                        v.position *= 100.0 / size;
                    }
                }
                let input = PlateInput {
                    printer: PrinterProfile::new("Creality K1", [220.0, 220.0, 250.0], 0.4, 0.2),
                    material: MaterialProfile::default(),
                    format: "stl".into(),
                    spacing: 5.0,
                    orient,
                    contour: false,
                    hollow_wall: None,
                    drain: None,
                    vent: false,
                    compensate: false,
                    path: String::new(),
                };
                let config = slicer::build_config(&settings).map_err(|e| e.to_string())?;
                let plan = slicing_plan(vec![(name.clone(), mesh)], &input, &config)?;
                let plate = plan.plates.first().ok_or_else(|| format!("no cabe en la cama: {:?} {:?}", plan.unplaced, plan.too_tall))?;
                let job = slicer::SliceJob { config, parts: plate_parts(plate), work_dir: std::env::temp_dir().join(format!("pinocchio-slicer-real-{}", std::process::id())) };
                let summary = slicer::run(&job, &engine, |_| {}, &AtomicBool::new(false)).map_err(|e| e.to_string())?;
                let gcode = std::fs::read_to_string(&summary.gcode).map_err(|e| e.to_string())?;
                let _ = std::fs::remove_dir_all(&job.work_dir);
                let (lo, hi) = extruded_bounds(&gcode);
                if lo[0] < 0.0 || lo[1] < 0.0 || hi[0] > 220.0 || hi[1] > 220.0 {
                    return Err(format!("se sale de la cama: {lo:?} {hi:?}"));
                }
                Ok(format!("{} capas, {:.0} min, {:.0} g, avisos {:?}", summary.layers, summary.time_s / 60.0, summary.filament_g, summary.warnings))
            })();
            let secs = t0.elapsed().as_secs_f64();
            match outcome {
                Ok(m) => eprintln!("OK    {name} ({secs:.0} s): {m}"),
                Err(e) => {
                    eprintln!("FALLA {name} ({secs:.0} s): {e}");
                    failed.push(name);
                }
            }
        }
        assert!(failed.is_empty(), "fallaron: {failed:?}");
    }

    /// Caja XY de lo que se extruye (G1 con E positivo)
    fn extruded_bounds(gcode: &str) -> ([f64; 2], [f64; 2]) {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for line in gcode.lines().filter(|l| l.starts_with("G1 ")) {
            let word = |c: char| line.split_whitespace().find_map(|w| w.strip_prefix(c)?.parse::<f64>().ok());
            if word('E').is_some_and(|e| e > 0.0) {
                for (k, c) in ['X', 'Y'].into_iter().enumerate() {
                    if let Some(v) = word(c) {
                        lo[k] = lo[k].min(v);
                        hi[k] = hi[k].max(v);
                    }
                }
            }
        }
        (lo, hi)
    }
}
