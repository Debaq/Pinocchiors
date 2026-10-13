//! Preparar para imprimir en un paso: verificar → dividir si no cabe → uniones → vaciar → exportar las placas
//!
//! Encadena lo que el resto de la sección hace por separado, con el mismo código, y deja un informe por paso. Se
//! detiene en el primer paso que falla (un modelo que no es estanco no se divide ni se exporta).

use pinocchio_print3d::{JointsReport, PlanarJointConfig, Severity};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::{in_background, subdivide_mesh_impl, CutPlaneInput, SubdivideConfigInput, SubdivideResultInfo};
use crate::print_prep::{add_print_joints_impl, check_print_readiness_impl, export_print_plates_impl, FitInput, PlateInfo, PlateInput};
use crate::state::AppState;

/// Qué se pide en el paso único
#[derive(Debug, Clone, Deserialize)]
pub struct PrepareInput {
    /// Impresora, material, vaciado, formato y archivo de las placas
    pub plate: PlateInput,
    /// Cómo se divide si no cabe: grid, optimal, zlayers o manual
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub margin: Option<f64>,
    /// Con la estrategia `manual`: los planos de corte
    #[serde(default)]
    pub planes: Vec<CutPlaneInput>,
    /// Uniones entre las piezas; sin esto no se ponen
    #[serde(default)]
    pub joints: Option<PlanarJointConfig>,
    #[serde(default)]
    pub fit: Option<FitInput>,
    /// Dividir aunque el modelo quepa en la cama
    #[serde(default)]
    pub force_split: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Hecho sin problemas
    Done,
    /// No hizo falta o no se pidió
    Skipped,
    /// Hecho, pero hay algo que mirar
    Warning,
    /// No se pudo: el resto no se hace
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareStep {
    /// verify, split, joints, hollow, export
    pub id: String,
    pub label: String,
    pub status: StepStatus,
    pub detail: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareReport {
    pub steps: Vec<PrepareStep>,
    /// Ningún paso falló
    pub ok: bool,
    /// Las placas exportadas
    pub plates: Option<PlateInfo>,
    /// Las piezas, si este paso dividió el modelo
    pub split: Option<SubdivideResultInfo>,
    /// Las uniones puestas, si se pusieron
    pub joints: Option<JointsReport>,
}

fn build_report(steps: Vec<PrepareStep>, plates: Option<PlateInfo>, split: Option<SubdivideResultInfo>, joints: Option<JointsReport>) -> PrepareReport {
    let ok = steps.iter().all(|s| s.status != StepStatus::Failed);
    PrepareReport { steps, ok, plates, split, joints }
}

fn step(id: &str, label: &str, status: StepStatus, detail: impl Into<String>, notes: Vec<String>) -> PrepareStep {
    PrepareStep { id: id.into(), label: label.into(), status, detail: detail.into(), notes }
}

/// Verifica, divide si hace falta, pone uniones, vacía y exporta las placas
#[tauri::command]
pub async fn prepare_for_printing(app: AppHandle, input: PrepareInput) -> Result<PrepareReport, String> {
    in_background(app, move |state| prepare_for_printing_impl(state, input)).await
}

pub(crate) fn prepare_for_printing_impl(state: &AppState, input: PrepareInput) -> Result<PrepareReport, String> {
    let mut steps: Vec<PrepareStep> = Vec::new();
    let mut split: Option<SubdivideResultInfo> = None;
    let mut joints: Option<JointsReport> = None;
    let (printer, material) = (&input.plate.printer, &input.plate.material);

    // 1. Verificar
    let info = check_print_readiness_impl(state, printer, material)?;
    let report = &info.report;
    let issues = |sev: Severity| -> Vec<String> { report.issues.iter().filter(|i| i.severity == sev).map(|i| i.message.clone()).collect() };
    let (errors, warnings) = (issues(Severity::Error), issues(Severity::Warning));
    if !report.printable || !report.is_watertight {
        let mut notes = errors;
        if notes.is_empty() {
            notes.push("El modelo no es estanco: repáralo antes de imprimir".to_string());
        }
        steps.push(step("verify", "Verificar el modelo", StepStatus::Failed, "No se puede imprimir así", notes));
        return Ok(build_report(steps, None, split, joints));
    }
    let status = if warnings.is_empty() { StepStatus::Done } else { StepStatus::Warning };
    steps.push(step("verify", "Verificar el modelo", status, if warnings.is_empty() { "Estanco y sin problemas" } else { "Se puede imprimir, con avisos" }, warnings));

    // 2. Dividir si no cabe
    let existing = state.print3d_pieces.lock().unwrap().as_ref().map(Vec::len);
    let fits = report.fit.fits || report.fit.fits_rotated;
    match existing {
        Some(n) if !input.force_split => steps.push(step("split", "Dividir en piezas", StepStatus::Skipped, format!("Ya hay {n} piezas"), Vec::new())),
        _ if fits && !input.force_split => steps.push(step("split", "Dividir en piezas", StepStatus::Skipped, "Cabe en la cama sin dividir", Vec::new())),
        _ => {
            let config = SubdivideConfigInput { build_volume: printer.build_volume, strategy: input.strategy.clone(), margin: input.margin, planes: input.planes.clone() };
            match subdivide_mesh_impl(config, state) {
                Ok(r) => {
                    steps.push(step("split", "Dividir en piezas", StepStatus::Done, format!("{} piezas", r.piece_count), Vec::new()));
                    split = Some(r);
                }
                Err(e) => {
                    steps.push(step("split", "Dividir en piezas", StepStatus::Failed, "No se pudo dividir", vec![e]));
                    return Ok(build_report(steps, None, split, joints));
                }
            }
        }
    }

    // 3. Uniones
    let pieces = state.print3d_pieces_plain.lock().unwrap().as_ref().map_or(0, Vec::len);
    match (&input.joints, pieces) {
        (None, _) => steps.push(step("joints", "Uniones entre piezas", StepStatus::Skipped, "No se pidieron", Vec::new())),
        (Some(_), n) if n < 2 => steps.push(step("joints", "Uniones entre piezas", StepStatus::Skipped, "Hay una sola pieza", Vec::new())),
        (Some(config), _) => match add_print_joints_impl(state, config, input.fit.as_ref()) {
            Ok(r) => {
                let notes: Vec<String> = r.connections.iter().flat_map(|c| c.notes.iter().map(|n| format!("{} ↔ {}: {n}", c.piece_a + 1, c.piece_b + 1))).collect();
                let status = if r.joints_placed == 0 || !notes.is_empty() { StepStatus::Warning } else { StepStatus::Done };
                steps.push(step("joints", "Uniones entre piezas", status, format!("{} puestas", r.joints_placed), notes));
                joints = Some(r);
            }
            Err(e) => {
                steps.push(step("joints", "Uniones entre piezas", StepStatus::Failed, "No se pudieron poner", vec![e]));
                return Ok(build_report(steps, None, split, joints));
            }
        },
    }

    // 4 y 5. Vaciar y exportar (el vaciado se hace pieza por pieza al armar las placas)
    match export_print_plates_impl(state, input.plate.clone()) {
        Ok(plates) => {
            match input.plate.hollow_wall {
                None => steps.push(step("hollow", "Vaciar por dentro", StepStatus::Skipped, "No se pidió", Vec::new())),
                Some(wall) => {
                    let status = if plates.notes.is_empty() { StepStatus::Done } else { StepStatus::Warning };
                    steps.push(step("hollow", "Vaciar por dentro", status, format!("Pared de {wall} mm"), plates.notes.clone()));
                }
            }
            let mut notes: Vec<String> = Vec::new();
            if !plates.unplaced.is_empty() {
                notes.push(format!("No caben en la cama: {}", plates.unplaced.join(", ")));
            }
            if !plates.too_tall.is_empty() {
                notes.push(format!("Más altas que la impresora: {}", plates.too_tall.join(", ")));
            }
            let status = if notes.is_empty() { StepStatus::Done } else { StepStatus::Warning };
            steps.push(step("export", "Exportar placas", status, format!("{} placas · {} piezas", plates.plates, plates.pieces), notes));
            Ok(build_report(steps, Some(plates), split, joints))
        }
        Err(e) => {
            if input.plate.hollow_wall.is_some() && e.starts_with("Pieza ") {
                steps.push(step("hollow", "Vaciar por dentro", StepStatus::Failed, "No se pudo vaciar", vec![e]));
                steps.push(step("export", "Exportar placas", StepStatus::Skipped, "No se llegó", Vec::new()));
            } else {
                steps.push(step("hollow", "Vaciar por dentro", if input.plate.hollow_wall.is_some() { StepStatus::Done } else { StepStatus::Skipped }, "", Vec::new()));
                steps.push(step("export", "Exportar placas", StepStatus::Failed, "No se pudieron exportar las placas", vec![e]));
            }
            Ok(build_report(steps, None, split, joints))
        }
    }
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

    fn input(name: &str) -> PrepareInput {
        let dir = std::env::temp_dir().join("pinocchio_prepare_test");
        std::fs::create_dir_all(&dir).unwrap();
        PrepareInput {
            plate: PlateInput {
                printer: PrinterProfile::new("t", [100.0, 100.0, 80.0], 0.4, 0.2),
                material: MaterialProfile::default(),
                format: "stl".into(),
                spacing: 5.0,
                orient: true,
                contour: false,
                hollow_wall: None,
                drain: None,
                vent: false,
                compensate: false,
                path: dir.join(format!("{name}.stl")).to_string_lossy().into_owned(),
            },
            strategy: None,
            margin: None,
            planes: Vec::new(),
            joints: None,
            fit: None,
            force_split: false,
        }
    }

    fn status(r: &PrepareReport, id: &str) -> StepStatus {
        r.steps.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("falta el paso {id}")).status
    }

    #[test]
    fn un_modelo_que_cabe_se_exporta_sin_dividir() {
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(cube(40.0));
        let r = prepare_for_printing_impl(&state, input("cabe")).unwrap();
        assert!(r.ok, "{:?}", r.steps);
        assert_eq!(status(&r, "verify"), StepStatus::Done);
        assert_eq!(status(&r, "split"), StepStatus::Skipped);
        assert_eq!(status(&r, "joints"), StepStatus::Skipped);
        assert_eq!(status(&r, "hollow"), StepStatus::Skipped);
        assert_eq!(status(&r, "export"), StepStatus::Done);
        let plates = r.plates.unwrap();
        assert_eq!((plates.plates, plates.pieces), (1, 1));
        assert!(std::path::Path::new(&plates.files[0]).exists());
    }

    #[test]
    fn un_modelo_grande_se_divide_se_une_y_se_vacia() {
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(cube(150.0));
        let mut inp = input("grande");
        inp.joints = Some(PlanarJointConfig { depth: 8.0, count: 1, ..Default::default() });
        inp.plate.hollow_wall = Some(3.0);
        inp.plate.drain = Some(4.0);
        let r = prepare_for_printing_impl(&state, inp).unwrap();
        assert!(r.ok, "{:?}", r.steps);
        assert_eq!(status(&r, "split"), StepStatus::Done);
        assert_eq!(status(&r, "joints"), StepStatus::Done, "{:?}", r.steps);
        assert_eq!(status(&r, "hollow"), StepStatus::Done);
        let plates = r.plates.unwrap();
        assert!(plates.pieces > 1 && plates.plates >= 1);
        assert!(plates.unplaced.is_empty(), "{:?}", plates.unplaced);
        // Las piezas quedaron en el estado, con uniones
        assert_eq!(state.print3d_pieces.lock().unwrap().as_ref().unwrap().len(), plates.pieces);
    }

    #[test]
    fn un_modelo_abierto_se_detiene_en_verificar() {
        let state = AppState::new();
        let m = cube(40.0);
        let (pos, mut tris) = (m.vertices.iter().map(|v| v.position).collect::<Vec<_>>(), (0..m.num_faces()).map(|f| m.get_face_vertices(f)).collect::<Vec<_>>());
        tris.pop();
        *state.mesh.lock().unwrap() = Some(Mesh::from_triangles(&pos, &tris));
        let r = prepare_for_printing_impl(&state, input("abierto")).unwrap();
        assert!(!r.ok);
        assert_eq!(r.steps.len(), 1, "no sigue después de fallar");
        assert_eq!(status(&r, "verify"), StepStatus::Failed);
        assert!(r.plates.is_none());
    }

    #[test]
    fn sin_modelo_es_un_error() {
        assert!(prepare_for_printing_impl(&AppState::new(), input("nada")).is_err());
    }
}
