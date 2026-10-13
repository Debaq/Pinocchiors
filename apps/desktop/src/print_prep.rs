//! Preparación para imprimir: verificar, orientar, uniones, material y placas
//!
//! La lógica vive en `pinocchio-print3d` (Z arriba, milímetros). Aquí se
//! traduce de la escena (Y arriba, unidades de la escena) a ese espacio y vuelta,
//! y se leen/guardan las piezas de `AppState::print3d_pieces`.

use crate::commands::{in_background, mesh_reoriented, mesh_to_scene, mm_per_unit};
use crate::state::AppState;
use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use pinocchio_print3d::{
    self as p3d, FabTolerances, Fit, HollowConfig, JointsReport, MaterialProfile, NestItem, OrientationCandidate, PlanarJointConfig, PrintabilityConfig, PrintabilityReport, PrinterProfile,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

// ═══════════════════════════════════════════════════════════════════════════
// ESPACIO DE IMPRESIÓN
// ═══════════════════════════════════════════════════════════════════════════

/// De la escena (Y arriba, `k` mm por unidad) a Z arriba en milímetros
pub(crate) fn to_print_space(mesh: &Mesh, k: f64) -> Mesh {
    let mut out = mesh_reoriented(mesh, true);
    for v in &mut out.vertices {
        v.position *= k;
    }
    out
}

/// De Z arriba en milímetros a la escena
pub(crate) fn from_print_space(mesh: &Mesh, k: f64) -> Mesh {
    let mut out = mesh.clone();
    for v in &mut out.vertices {
        v.position *= 1.0 / k;
    }
    mesh_reoriented(&out, false)
}

/// Dirección de impresión (Z arriba) a la de la escena (Y arriba)
fn dir_to_scene(d: [f64; 3]) -> [f64; 3] {
    [d[0], d[2], -d[1]]
}

fn point_to_scene(p: [f64; 3], k: f64) -> [f64; 3] {
    let s = dir_to_scene(p);
    [s[0] / k, s[1] / k, s[2] / k]
}

/// Mallas a preparar, en milímetros con Z arriba: las piezas si se dividió, o el modelo
fn sources(state: &AppState) -> Result<Vec<(String, Mesh)>, String> {
    let k = mm_per_unit(state);
    if let Some(pieces) = state.print3d_pieces.lock().unwrap().as_ref() {
        return Ok(pieces.iter().map(|p| (p.label.clone(), to_print_space(&p.mesh, k))).collect());
    }
    let mesh = state.mesh.lock().unwrap();
    let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
    Ok(vec![("modelo".to_string(), to_print_space(mesh, k))])
}

// ═══════════════════════════════════════════════════════════════════════════
// PERFILES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct PrintPresets {
    pub printers: Vec<PrinterProfile>,
    pub materials: Vec<MaterialProfile>,
}

/// Impresoras y materiales de fábrica
#[tauri::command]
pub fn get_print_presets() -> PrintPresets {
    PrintPresets { printers: PrinterProfile::presets(), materials: MaterialProfile::presets() }
}

#[derive(Debug, Clone, Serialize)]
pub struct FabInfo {
    #[serde(flatten)]
    pub tolerances: FabTolerances,
    /// Holgura total de cada tipo de ajuste (mm)
    pub tight: f64,
    pub normal: f64,
    pub loose: f64,
}

/// Holguras y compensaciones que salen de la impresora y el material elegidos
#[tauri::command]
pub fn get_fab_tolerances(printer: PrinterProfile, material: MaterialProfile) -> FabInfo {
    let t = FabTolerances::derive(&printer, &material);
    FabInfo { tight: t.fit_clearance(Fit::Tight), normal: t.fit_clearance(Fit::Normal), loose: t.fit_clearance(Fit::Loose), tolerances: t }
}

// ═══════════════════════════════════════════════════════════════════════════
// PIEZAS EN EL VISOR
// ═══════════════════════════════════════════════════════════════════════════

/// Color de la pieza `i` entre `n`: tonos repartidos por la proporción áurea
fn piece_color(i: usize) -> [f32; 4] {
    let h = (i as f32 * 0.618_034).fract();
    let (s, l) = (0.55f32, 0.58f32);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m, 1.0]
}

/// Las piezas del corte, cada una de su color y separadas del centro `explode` veces la distancia de su centro
/// al del modelo (0 = juntas). Sale en el formato de los datos de malla del visor
#[tauri::command]
pub async fn print_pieces_view(app: AppHandle, explode: f64) -> Result<tauri::ipc::Response, String> {
    in_background(app, move |state| pieces_view(state, explode).map(tauri::ipc::Response::new)).await
}

pub(crate) fn pieces_view(state: &AppState, explode: f64) -> Result<Vec<u8>, String> {
    let pieces = state.print3d_pieces.lock().unwrap();
    let pieces = pieces.as_ref().ok_or("Primero divide el modelo en piezas")?;
    let base = state.scene.lock().unwrap().clone();
    let boxes: Vec<(Vector3, Vector3)> = pieces
        .iter()
        .map(|p| {
            let (mut lo, mut hi) = (p.mesh.vertices[0].position, p.mesh.vertices[0].position);
            for v in &p.mesh.vertices {
                lo = lo.min(&v.position);
                hi = hi.max(&v.position);
            }
            (lo, hi)
        })
        .collect();
    let (lo, hi) = boxes.iter().fold((boxes[0].0, boxes[0].1), |(l, h), (a, b)| (l.min(a), h.max(b)));
    let model_center = (lo + hi) * 0.5;
    let mut scene = converter_scene::Scene::new();
    if let Some(b) = &base {
        scene.meters_per_unit = b.meters_per_unit;
        scene.y_up = b.y_up;
    }
    for (i, (piece, (a, b))) in pieces.iter().zip(&boxes).enumerate() {
        let offset = ((*a + *b) * 0.5 - model_center) * explode.max(0.0);
        let mut mesh = piece.mesh.clone();
        for v in &mut mesh.vertices {
            v.position += offset;
        }
        let mut part = mesh_to_scene(&mesh, &format!("pieza_{}", piece.label), None);
        for m in &mut part.meshes {
            for p in &mut m.primitives {
                p.material = Some(i);
            }
        }
        scene.materials.push(converter_scene::Material { name: format!("pieza_{}", piece.label), base_color_factor: piece_color(i), ..Default::default() });
        scene.merge(part);
    }
    Ok(crate::commands::scene_mesh_data(&scene).to_bytes())
}

// ═══════════════════════════════════════════════════════════════════════════
// CALIBRACIÓN
// ═══════════════════════════════════════════════════════════════════════════

/// Qué modelo de calibración se descarga
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalibrationModel {
    /// Placa con agujeros de holgura creciente y clavijas
    Clearance(p3d::ClearanceTest),
    /// Bloque de medidas conocidas con agujeros pasantes
    Block(p3d::CalibrationBlock),
}

/// Guarda un modelo de calibración listo para imprimir (STL, 3MF…), en milímetros con Z arriba
#[tauri::command]
pub async fn export_calibration_model(app: AppHandle, model: CalibrationModel, format: String, path: String) -> Result<u64, String> {
    in_background(app, move |_| export_calibration_impl(&model, &format, &path)).await
}

pub(crate) fn export_calibration_impl(model: &CalibrationModel, format: &str, path: &str) -> Result<u64, String> {
    let meshes: Vec<(String, Mesh)> = match model {
        CalibrationModel::Clearance(cfg) => p3d::clearance_test(cfg).map_err(|e| format!("No se pudo armar la prueba: {e}"))?,
        CalibrationModel::Block(cfg) => vec![("bloque".to_string(), p3d::calibration_block(cfg).map_err(|e| format!("No se pudo armar el bloque: {e}"))?)],
    };
    let p = std::path::Path::new(path);
    crate::cad::export_scene(&plate_scene(&Plate { meshes }), p, format)?;
    Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Las holguras que salen de lo medido en las piezas de calibración impresas
#[tauri::command]
pub fn solve_print_calibration(block: p3d::CalibrationBlock, measures: p3d::CalibrationMeasures) -> Result<p3d::CalibrationResult, String> {
    p3d::solve_calibration(&block, &measures).map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// VERIFICAR
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct PrintabilityInfo {
    #[serde(flatten)]
    pub report: PrintabilityReport,
    /// De qué se habla: "modelo" o las piezas
    pub subject: String,
}

/// Revisa si la malla (o cada pieza) se puede imprimir; junta los hallazgos
#[tauri::command]
pub async fn check_print_readiness(app: AppHandle, printer: PrinterProfile, material: MaterialProfile) -> Result<PrintabilityInfo, String> {
    in_background(app, move |state| check_print_readiness_impl(state, &printer, &material)).await
}

pub(crate) fn check_print_readiness_impl(state: &AppState, printer: &PrinterProfile, material: &MaterialProfile) -> Result<PrintabilityInfo, String> {
    let k = mm_per_unit(state);
    let list = sources(state)?;
    let single = list.len() == 1;
    let mut merged: Option<PrintabilityReport> = None;
    for (label, mesh) in &list {
        let mut report = p3d::check_printability(mesh, printer, material).map_err(|e| format!("No se pudo revisar: {e}"))?;
        report.thin_points = report.thin_points.iter().map(|&p| point_to_scene(p, k)).collect();
        if !single {
            for issue in &mut report.issues {
                issue.message = format!("Pieza {label}: {}", issue.message);
            }
        }
        merged = Some(match merged {
            None => report,
            Some(mut acc) => {
                acc.open_edges += report.open_edges;
                acc.non_manifold_edges += report.non_manifold_edges;
                acc.inconsistent_edges += report.inconsistent_edges;
                acc.degenerate_faces += report.degenerate_faces;
                acc.duplicate_faces += report.duplicate_faces;
                acc.components += report.components;
                acc.inverted |= report.inverted;
                acc.is_watertight &= report.is_watertight;
                acc.overhang_area += report.overhang_area;
                acc.support_volume_max += report.support_volume_max;
                acc.support_volume += report.support_volume;
                acc.bed_contact_area += report.bed_contact_area;
                acc.min_thickness = match (acc.min_thickness, report.min_thickness) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                acc.thin_fraction = acc.thin_fraction.max(report.thin_fraction);
                acc.thin_points.extend(report.thin_points);
                acc.thin_points.truncate(64);
                acc.fit.fits &= report.fit.fits;
                acc.fit.fits_rotated &= report.fit.fits_rotated;
                acc.issues.extend(report.issues);
                acc.printable &= report.printable;
                acc
            }
        });
    }
    let report = merged.ok_or("No hay nada que revisar")?;
    Ok(PrintabilityInfo { report, subject: if single { "modelo".into() } else { "piezas".into() } })
}

// ═══════════════════════════════════════════════════════════════════════════
// ORIENTACIÓN
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct OrientationInfo {
    /// Normal de la cara que queda abajo, en la escena (Y arriba)
    pub down: [f64; 3],
    pub overhang_area: f64,
    pub contact_area: f64,
    pub height: f64,
    pub support_volume_max: f64,
    pub support_volume: f64,
    pub score: f64,
}

fn orientation_info(c: &OrientationCandidate) -> OrientationInfo {
    OrientationInfo {
        down: dir_to_scene(c.down),
        overhang_area: c.overhang_area,
        contact_area: c.contact_area,
        height: c.height,
        support_volume_max: c.support_volume_max,
        support_volume: c.support_volume,
        score: c.score,
    }
}

/// Las mejores formas de apoyar el modelo en la cama (mínimos voladizos)
#[tauri::command]
pub async fn suggest_print_orientation(app: AppHandle, printer: PrinterProfile, material: MaterialProfile) -> Result<Vec<OrientationInfo>, String> {
    in_background(app, move |state| suggest_print_orientation_impl(state, &printer, &material)).await
}

pub(crate) fn suggest_print_orientation_impl(state: &AppState, printer: &PrinterProfile, material: &MaterialProfile) -> Result<Vec<OrientationInfo>, String> {
    let k = mm_per_unit(state);
    let mesh = state.mesh.lock().unwrap();
    let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
    let config = PrintabilityConfig::from_profiles(printer, material);
    let found = p3d::suggest_orientations(&to_print_space(mesh, k), &config, 6).map_err(|e| format!("No se pudo orientar: {e}"))?;
    Ok(found.iter().map(orientation_info).collect())
}

// ═══════════════════════════════════════════════════════════════════════════
// UNIONES
// ═══════════════════════════════════════════════════════════════════════════

/// Ajuste elegido: la holgura sale de la impresora y el material en vez de la del cuadro
#[derive(Debug, Clone, Deserialize)]
pub struct FitInput {
    pub printer: PrinterProfile,
    pub material: MaterialProfile,
    pub fit: Fit,
}

/// Pone pernos y huecos en las caras de corte de las piezas (parte de las piezas sin uniones)
#[tauri::command]
pub async fn add_print_joints(app: AppHandle, config: PlanarJointConfig, fit: Option<FitInput>) -> Result<JointsReport, String> {
    in_background(app, move |state| add_print_joints_impl(state, &config, fit.as_ref())).await
}

pub(crate) fn add_print_joints_impl(state: &AppState, config: &PlanarJointConfig, fit: Option<&FitInput>) -> Result<JointsReport, String> {
    let mut config = config.clone();
    if let Some(f) = fit {
        config.tolerance = FabTolerances::derive(&f.printer, &f.material).joint_tolerance(f.fit);
    }
    let config = &config;
    let k = mm_per_unit(state);
    let plain = state.print3d_pieces_plain.lock().unwrap().clone().ok_or("Primero divide el modelo en piezas")?;
    let mut work: Vec<p3d::LabeledPiece> = plain
        .iter()
        .map(|p| p3d::LabeledPiece { mesh: to_print_space(&p.mesh, k), ..p.clone() })
        .collect();
    let report = p3d::apply_joints(&mut work, config).map_err(|e| format!("No se pudieron poner las uniones: {e}"))?;
    let back: Vec<p3d::LabeledPiece> = work.into_iter().map(|p| p3d::LabeledPiece { mesh: from_print_space(&p.mesh, k), ..p }).collect();
    *state.print3d_pieces.lock().unwrap() = Some(back);
    Ok(report)
}

// ═══════════════════════════════════════════════════════════════════════════
// MATERIAL
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct MaterialInfo {
    pub volume_mm3: f64,
    pub mass_g: f64,
    pub filament_m: f64,
    /// Con vaciado: hueco total y fracción ahorrada
    pub hollow: Option<HollowInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HollowInfo {
    pub cavity_volume: f64,
    pub saved_fraction: f64,
    pub volume_mm3: f64,
    pub mass_g: f64,
    pub filament_m: f64,
}

/// Material que gasta imprimir el modelo (o las piezas), macizo y, si se pide, vaciado
#[tauri::command]
pub async fn estimate_print_material(app: AppHandle, material: MaterialProfile, infill: f64, hollow_wall: Option<f64>) -> Result<MaterialInfo, String> {
    in_background(app, move |state| estimate_print_material_impl(state, &material, infill, hollow_wall)).await
}

pub(crate) fn estimate_print_material_impl(state: &AppState, material: &MaterialProfile, infill: f64, hollow_wall: Option<f64>) -> Result<MaterialInfo, String> {
    let list = sources(state)?;
    let (mut solid, mut hol) = (p3d::MaterialEstimate { volume_mm3: 0.0, mass_g: 0.0, filament_m: 0.0 }, None::<(p3d::MaterialEstimate, f64)>);
    let add = |a: &mut p3d::MaterialEstimate, b: p3d::MaterialEstimate| {
        a.volume_mm3 += b.volume_mm3;
        a.mass_g += b.mass_g;
        a.filament_m += b.filament_m;
    };
    for (label, mesh) in &list {
        add(&mut solid, p3d::estimate_material(mesh, material, infill, 1.75));
        if let Some(wall) = hollow_wall {
            let r = p3d::hollow(mesh, &HollowConfig { wall, ..Default::default() }).map_err(|e| format!("Pieza {label}: {e}"))?;
            let e = p3d::estimate_material(&r.mesh, material, infill, 1.75);
            let (mut acc, cavity) = hol.take().unwrap_or((p3d::MaterialEstimate { volume_mm3: 0.0, mass_g: 0.0, filament_m: 0.0 }, 0.0));
            add(&mut acc, e);
            hol = Some((acc, cavity + r.cavity_volume));
        }
    }
    let hollow = hol.map(|(e, cavity)| HollowInfo {
        cavity_volume: cavity,
        saved_fraction: if solid.volume_mm3 > 0.0 { 1.0 - e.volume_mm3 / solid.volume_mm3 } else { 0.0 },
        volume_mm3: e.volume_mm3,
        mass_g: e.mass_g,
        filament_m: e.filament_m,
    });
    Ok(MaterialInfo { volume_mm3: solid.volume_mm3, mass_g: solid.mass_g, filament_m: solid.filament_m, hollow })
}

// ═══════════════════════════════════════════════════════════════════════════
// PLACAS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct PlateInput {
    pub printer: PrinterProfile,
    pub material: MaterialProfile,
    /// stl, 3mf, obj…
    pub format: String,
    /// Separación entre piezas (mm)
    pub spacing: f64,
    /// Girar cada pieza a su mejor posición de apoyo
    pub orient: bool,
    /// Acomodar por el contorno de cada pieza y no por su caja
    #[serde(default)]
    pub contour: bool,
    /// Vaciar con este espesor de pared (mm)
    pub hollow_wall: Option<f64>,
    /// Agujero de drenaje de este diámetro en la cara de abajo (mm), al vaciar
    #[serde(default)]
    pub drain: Option<f64>,
    /// Agrandar por eje lo que se encoge el material al enfriar
    #[serde(default)]
    pub compensate: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlateInfo {
    pub files: Vec<String>,
    /// Factor por eje por contracción del material (1 = sin compensar)
    pub shrink_scale: [f64; 3],
    pub plates: usize,
    pub pieces: usize,
    pub utilization: Vec<f64>,
    /// Piezas que no caben en la cama ni solas
    pub unplaced: Vec<String>,
    /// Piezas más altas que la impresora
    pub too_tall: Vec<String>,
    pub total_bytes: u64,
}

/// Una placa lista: sus piezas ya acomodadas (mm, Z arriba, esquina de la cama en el origen)
pub(crate) struct Plate {
    pub meshes: Vec<(String, Mesh)>,
}

pub(crate) struct PlatePlan {
    /// Factor por eje aplicado a las piezas por la contracción (1 si no se compensó)
    pub shrink_scale: [f64; 3],
    pub plates: Vec<Plate>,
    pub utilization: Vec<f64>,
    pub unplaced: Vec<String>,
    pub too_tall: Vec<String>,
}

/// Prepara las piezas (vaciar, orientar) y las reparte en camas
pub(crate) fn plan_plates(list: Vec<(String, Mesh)>, input: &PlateInput) -> Result<PlatePlan, String> {
    let config = PrintabilityConfig::from_profiles(&input.printer, &input.material);
    let tolerances = FabTolerances::derive(&input.printer, &input.material);
    let mut prepared: Vec<(String, Mesh)> = Vec::new();
    for (label, mut mesh) in list {
        // Primero se apoya: el drenaje va en la cara de abajo de esa posición
        if input.orient {
            p3d::orient_for_print(&mut mesh, &config).map_err(|e| format!("Pieza {label}: {e}"))?;
        } else {
            p3d::settle_on_bed(&mut mesh);
        }
        if let Some(wall) = input.hollow_wall {
            let hollow = HollowConfig { wall, drain_diameter: input.drain, ..Default::default() };
            mesh = p3d::hollow(&mesh, &hollow).map_err(|e| format!("Pieza {label}: {e}"))?.mesh;
        }
        if input.compensate {
            p3d::compensate_shrink(&mut mesh, &tolerances);
        }
        prepared.push((label, mesh));
    }
    let bed = [input.printer.build_volume[0], input.printer.build_volume[1]];
    let spacing = input.spacing.max(0.0);
    let result = if input.contour {
        let cell = (bed[0].max(bed[1]) / 200.0).max(0.5);
        let refs: Vec<&Mesh> = prepared.iter().map(|(_, m)| m).collect();
        p3d::nest_contour(&refs, bed, spacing, true, cell)
    } else {
        let items: Vec<NestItem> = prepared.iter().enumerate().map(|(id, (_, m))| NestItem { id, size: p3d::footprint(m) }).collect();
        p3d::nest(&items, bed, spacing, true)
    };
    let mut meshes: Vec<Mesh> = prepared.iter().map(|(_, m)| m.clone()).collect();
    p3d::apply_nesting(&mut meshes, &result);
    let too_tall = prepared
        .iter()
        .zip(&meshes)
        .enumerate()
        .filter(|(i, (_, m))| !result.unplaced.contains(i) && p3d::compute_bounding_box(m).dimensions()[2] > input.printer.build_volume[2] + 1e-6)
        .map(|(_, ((label, _), _))| label.clone())
        .collect();
    let mut plates: Vec<Plate> = (0..result.bed_count).map(|_| Plate { meshes: Vec::new() }).collect();
    for p in &result.placements {
        plates[p.bed].meshes.push((prepared[p.id].0.clone(), meshes[p.id].clone()));
    }
    let unplaced = result.unplaced.iter().map(|&i| prepared[i].0.clone()).collect();
    let shrink_scale = if input.compensate { tolerances.shrink_scale() } else { [1.0; 3] };
    Ok(PlatePlan { shrink_scale, plates, utilization: result.utilization, unplaced, too_tall })
}

/// Escena de una placa: las piezas como objetos, en mm (se exporta con Z arriba)
pub(crate) fn plate_scene(plate: &Plate) -> converter_scene::Scene {
    let mut scene = converter_scene::Scene { meters_per_unit: 0.001, y_up: true, ..Default::default() };
    for (label, mesh) in &plate.meshes {
        let part = mesh_to_scene(&from_print_space(mesh, 1.0), &format!("pieza_{label}"), None);
        let mesh_offset = scene.meshes.len();
        let node_offset = scene.nodes.len();
        scene.meshes.extend(part.meshes);
        for mut node in part.nodes {
            node.mesh = node.mesh.map(|m| m + mesh_offset);
            node.children = node.children.iter().map(|c| c + node_offset).collect();
            scene.nodes.push(node);
        }
        scene.root_nodes.extend(part.root_nodes.iter().map(|r| r + node_offset));
    }
    scene
}

fn plate_path(path: &str, index: usize, count: usize) -> String {
    if count <= 1 {
        return path.to_string();
    }
    let p = std::path::Path::new(path);
    let stem = p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let name = match p.extension() {
        Some(ext) => format!("{stem}_{}.{}", index + 1, ext.to_string_lossy()),
        None => format!("{stem}_{}", index + 1),
    };
    p.with_file_name(name).to_string_lossy().into_owned()
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewPiece {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatePreview {
    /// Ancho y fondo de la cama (mm)
    pub bed: [f64; 2],
    pub plates: Vec<Vec<PreviewPiece>>,
    pub utilization: Vec<f64>,
    pub unplaced: Vec<String>,
    pub too_tall: Vec<String>,
}

/// Cómo quedarían las piezas en las camas, sin exportar ni vaciar
#[tauri::command]
pub async fn preview_print_plates(app: AppHandle, input: PlateInput) -> Result<PlatePreview, String> {
    in_background(app, move |state| preview_print_plates_impl(state, input)).await
}

pub(crate) fn preview_print_plates_impl(state: &AppState, mut input: PlateInput) -> Result<PlatePreview, String> {
    // El vaciado no cambia la huella: se omite para que la vista sea rápida
    input.hollow_wall = None;
    let plan = plan_plates(sources(state)?, &input)?;
    let plates = plan
        .plates
        .iter()
        .map(|plate| {
            plate
                .meshes
                .iter()
                .map(|(label, mesh)| {
                    let bb = p3d::compute_bounding_box(mesh);
                    let d = bb.dimensions();
                    PreviewPiece { label: label.clone(), x: bb.min[0], y: bb.min[1], w: d[0], h: d[1], height: d[2] }
                })
                .collect()
        })
        .collect();
    Ok(PlatePreview {
        bed: [input.printer.build_volume[0], input.printer.build_volume[1]],
        plates,
        utilization: plan.utilization,
        unplaced: plan.unplaced,
        too_tall: plan.too_tall,
    })
}

/// Reparte las piezas en placas del tamaño de la cama y las exporta, una por archivo
#[tauri::command]
pub async fn export_print_plates(app: AppHandle, input: PlateInput) -> Result<PlateInfo, String> {
    in_background(app, move |state| export_print_plates_impl(state, input)).await
}

pub(crate) fn export_print_plates_impl(state: &AppState, input: PlateInput) -> Result<PlateInfo, String> {
    let list = sources(state)?;
    let pieces = list.len();
    let plan = plan_plates(list, &input)?;
    if plan.plates.is_empty() {
        return Err("Ninguna pieza cabe en la cama de la impresora".into());
    }
    let mut files = Vec::new();
    for (i, plate) in plan.plates.iter().enumerate() {
        let path = plate_path(&input.path, i, plan.plates.len());
        crate::cad::export_scene(&plate_scene(plate), std::path::Path::new(&path), &input.format)?;
        files.push(path);
    }
    let total_bytes = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    Ok(PlateInfo { shrink_scale: plan.shrink_scale, plates: plan.plates.len(), files, pieces, utilization: plan.utilization, unplaced: plan.unplaced, too_tall: plan.too_tall, total_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn cube(size: f64) -> Mesh {
        let p = |x: f64, y: f64, z: f64| Vector3::new(x * size, y * size, z * size);
        let v = vec![p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.), p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)];
        let f = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        Mesh::from_triangles(&v, &f)
    }

    fn input(spacing: f64) -> PlateInput {
        PlateInput {
            printer: PrinterProfile::new("t", [100.0, 100.0, 80.0], 0.4, 0.2),
            material: MaterialProfile::default(),
            format: "stl".into(),
            spacing,
            orient: true,
            contour: false,
            hollow_wall: None,
            drain: None,
            compensate: false,
            path: "x.stl".into(),
        }
    }

    #[test]
    fn el_espacio_de_impresion_es_reversible() {
        let m = cube(10.0);
        let back = from_print_space(&to_print_space(&m, 1000.0), 1000.0);
        for (a, b) in m.vertices.iter().zip(&back.vertices) {
            assert!((a.position - b.position).length() < 1e-9);
        }
    }

    #[test]
    fn y_arriba_pasa_a_z_arriba() {
        let m = cube(1.0);
        let z = to_print_space(&m, 10.0);
        // La altura (Y) del cubo de 1 m pasa a Z, en mm × 10
        let bb = p3d::compute_bounding_box(&z);
        assert!((bb.dimensions()[2] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn reparte_en_dos_placas() {
        let list: Vec<(String, Mesh)> = (0..3).map(|i| (format!("{i}"), cube(60.0))).collect();
        let plan = plan_plates(list, &input(2.0)).unwrap();
        assert!(plan.plates.len() >= 2, "{}", plan.plates.len());
        assert!(plan.unplaced.is_empty());
        let total: usize = plan.plates.iter().map(|p| p.meshes.len()).sum();
        assert_eq!(total, 3);
        for plate in &plan.plates {
            for (_, m) in &plate.meshes {
                let bb = p3d::compute_bounding_box(m);
                assert!(bb.max[0] <= 100.0 + 1e-6 && bb.max[1] <= 100.0 + 1e-6 && bb.min[2].abs() < 1e-9);
            }
        }
    }

    #[test]
    fn reparte_por_contorno() {
        let list: Vec<(String, Mesh)> = (0..3).map(|i| (format!("{i}"), cube(60.0))).collect();
        let mut inp = input(2.0);
        inp.contour = true;
        let plan = plan_plates(list, &inp).unwrap();
        assert!(plan.unplaced.is_empty());
        let total: usize = plan.plates.iter().map(|p| p.meshes.len()).sum();
        assert_eq!(total, 3);
        for plate in &plan.plates {
            for (_, m) in &plate.meshes {
                let bb = p3d::compute_bounding_box(m);
                assert!(bb.min[0] >= -1e-6 && bb.max[0] <= 100.0 + 1e-6 && bb.max[1] <= 100.0 + 1e-6);
            }
        }
    }

    #[test]
    fn la_contraccion_agranda_las_piezas_exportadas() {
        let abs = MaterialProfile::presets().into_iter().find(|m| m.name == "ABS").unwrap();
        let mut inp = input(2.0);
        inp.material = abs;
        inp.compensate = true;
        let plan = plan_plates(vec![("a".into(), cube(40.0))], &inp).unwrap();
        let bb = p3d::compute_bounding_box(&plan.plates[0].meshes[0].1);
        // Un cubo de 40 mm sale 40 / (1 − 0,008) = 40,32 mm
        assert!((bb.dimensions()[0] - 40.0 / 0.992).abs() < 1e-9);
        assert!((plan.shrink_scale[0] - 1.0 / 0.992).abs() < 1e-12);
        inp.compensate = false;
        let plain = plan_plates(vec![("a".into(), cube(40.0))], &inp).unwrap();
        assert!((p3d::compute_bounding_box(&plain.plates[0].meshes[0].1).dimensions()[0] - 40.0).abs() < 1e-9);
    }

    #[test]
    fn pieza_alta_y_pieza_enorme() {
        let list = vec![("alta".to_string(), cube(90.0)), ("enorme".to_string(), cube(150.0))];
        let plan = plan_plates(list, &input(0.0)).unwrap();
        assert_eq!(plan.unplaced, vec!["enorme".to_string()]);
        assert_eq!(plan.too_tall, vec!["alta".to_string()]);
    }

    #[test]
    fn la_escena_de_la_placa_exporta_en_mm() {
        let plan = plan_plates(vec![("a".into(), cube(20.0)), ("b".into(), cube(20.0))], &input(5.0)).unwrap();
        let scene = plate_scene(&plan.plates[0]);
        assert_eq!(scene.meshes.len(), 2);
        let dir = std::env::temp_dir().join("pinocchio_plate_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("placa.stl");
        crate::cad::export_scene(&scene, &path, "stl").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for t in 0..count {
            let base = 84 + t * 50 + 12;
            for v in 0..3 {
                for k in 0..3 {
                    let o = base + v * 12 + k * 4;
                    let c = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
                    lo[k] = lo[k].min(c);
                    hi[k] = hi[k].max(c);
                }
            }
        }
        // Dos cubos de 20 mm con 5 mm entre ellos, en Z arriba
        assert!((hi[2] - lo[2] - 20.0).abs() < 1e-3 && lo[2].abs() < 1e-3, "{lo:?} {hi:?}");
        assert!((hi[0] - lo[0]).max(hi[1] - lo[1]) >= 44.9, "{lo:?} {hi:?}");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn los_modelos_de_calibracion_se_exportan_en_mm() {
        let dir = std::env::temp_dir().join("pinocchio_calibration_test");
        std::fs::create_dir_all(&dir).unwrap();
        let bounds = |path: &std::path::Path| {
            let bytes = std::fs::read(path).unwrap();
            let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for t in 0..count {
                let base = 84 + t * 50 + 12;
                for v in 0..3 {
                    for k in 0..3 {
                        let o = base + v * 12 + k * 4;
                        let c = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
                        lo[k] = lo[k].min(c);
                        hi[k] = hi[k].max(c);
                    }
                }
            }
            (lo, hi, count)
        };
        let path = dir.join("bloque.stl");
        export_calibration_impl(&CalibrationModel::Block(p3d::CalibrationBlock::default()), "stl", path.to_str().unwrap()).unwrap();
        let (lo, hi, count) = bounds(&path);
        // 53 × 30 × 20, Z arriba y apoyado en la cama
        assert!((hi[0] - lo[0] - 53.0).abs() < 1e-3 && (hi[1] - lo[1] - 30.0).abs() < 1e-3 && (hi[2] - lo[2] - 20.0).abs() < 1e-3, "{lo:?} {hi:?}");
        assert!(lo[2].abs() < 1e-3 && count > 100);
        let path = dir.join("holgura.stl");
        export_calibration_impl(&CalibrationModel::Clearance(p3d::ClearanceTest::default()), "stl", path.to_str().unwrap()).unwrap();
        let (lo, hi, _) = bounds(&path);
        assert!(hi[2] - lo[2] > 13.0 && lo[2].abs() < 1e-3, "{lo:?} {hi:?}");
        assert!(export_calibration_impl(&CalibrationModel::Block(p3d::CalibrationBlock { holes: vec![], ..Default::default() }), "stl", path.to_str().unwrap()).is_err());
        std::fs::remove_file(dir.join("bloque.stl")).ok();
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn la_vista_de_piezas_separa_y_colorea() {
        let state = AppState::new();
        assert!(pieces_view(&state, 0.5).is_err(), "sin piezas no hay vista");
        // Dos cubos de 10 pegados por una cara
        let mk = |x: f64, label: &str, id: usize| {
            let mut m = cube(10.0);
            for v in &mut m.vertices {
                v.position += Vector3::new(x, 0.0, 0.0);
            }
            pinocchio_print3d::LabeledPiece { mesh: m, id, label: label.into(), ..Default::default() }
        };
        *state.print3d_pieces.lock().unwrap() = Some(vec![mk(0.0, "1", 0), mk(10.0, "2", 1)]);
        let joined = pieces_view(&state, 0.0).unwrap();
        let apart = pieces_view(&state, 1.0).unwrap();
        assert_eq!(joined.len(), apart.len(), "la misma cantidad de datos");
        assert_ne!(joined, apart, "separadas cambia las posiciones");
        assert_ne!(piece_color(0), piece_color(1));
        assert!(piece_color(5).iter().all(|c| (0.0..=1.0).contains(c)));
    }

    #[test]
    fn estirar_cada_eje_por_separado() {
        use crate::commands::{scale_mesh_for_print_impl, ScalePrintInput};
        // Caja de 10 × 20 × 30 en la escena (x, y, z con Y arriba): en la vista es 10 de ancho, 30 de fondo, 20 de alto
        let mut m = cube(10.0);
        for v in &mut m.vertices {
            v.position = Vector3::new(v.position.x(), v.position.y() * 2.0, v.position.z() * 3.0);
        }
        let mut scene = mesh_to_scene(&m, "caja", None);
        scene.meters_per_unit = 0.001;
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(crate::commands::scene_to_pinocchio_mesh(&scene).unwrap());
        *state.scene.lock().unwrap() = Some(scene);
        // Ancho ×2, fondo ×0,5, alto ×1,5 (ejes de la vista, Z arriba)
        let info = scale_mesh_for_print_impl(
            ScalePrintInput { mode: "axes".into(), factor: None, factors: Some([2.0, 0.5, 1.5]), target_size: None, target_volume: None },
            &state,
        )
        .unwrap();
        let [w, d, h] = info.dimensions;
        assert!((w - 20.0).abs() < 1e-6 && (d - 15.0).abs() < 1e-6 && (h - 30.0).abs() < 1e-6, "{:?}", info.dimensions);
        // La escena se escaló igual que la malla
        let scene_bb = {
            let scene = state.scene.lock().unwrap();
            let prims = scene.as_ref().unwrap().world_primitives();
            let ys: Vec<f32> = prims.iter().flat_map(|p| p.positions.iter().map(|q| q[1])).collect();
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!((scene_bb - 30.0).abs() < 1e-3, "{scene_bb}");
        // Factores imposibles
        let bad = ScalePrintInput { mode: "axes".into(), factor: None, factors: Some([0.0, 1.0, 1.0]), target_size: None, target_volume: None };
        assert!(scale_mesh_for_print_impl(bad, &state).is_err());
        let none = ScalePrintInput { mode: "axes".into(), factor: None, factors: None, target_size: None, target_volume: None };
        assert!(scale_mesh_for_print_impl(none, &state).is_err());
    }

    #[test]
    fn nombres_de_placa() {
        assert_eq!(plate_path("/a/b.stl", 0, 1), "/a/b.stl");
        assert_eq!(plate_path("/a/b.stl", 1, 3), "/a/b_2.stl");
    }
}
