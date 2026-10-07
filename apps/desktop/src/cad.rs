//! CAD paramétrico: documento de operaciones sobre OpenCASCADE y escaneo → CAD.
//!
//! El documento (`cad_model::Document`) vive en el estado y se guarda en el
//! proyecto. El frontend lo edita entero y lo devuelve con [`cad_set_document`]:
//! así deshacer/rehacer son copias del documento y no hace falta un comando por
//! cada cambio. El recálculo queda en caché hasta el próximo cambio.
//!
//! Coordenadas: el CAD trabaja en mm con Z arriba. El visor muestra la escena
//! con Y arriba y en sus unidades: las mallas que van y vienen se convierten
//! acá (también la del escaneo, para que el sólido calce encima).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use cad_model::{
    BodyOp, Document, EdgeRef, FaceRef, FeatureKind, FeatureState, FeatureStatus, ImportFormat, Plane, PlaneSpec, Region,
    Sketch, SolveReport,
};
use cad_scan::{CylinderPick, DetectOptions, Detection, OutlineOptions, PickOptions, PlanePick, ScanMesh, Section};
use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::AppHandle;

use crate::commands::{in_background, load_scene, MeshInfo, Progress};
use crate::state::AppState;

/// Error de cuerda de los teselados para el visor (mm) y ángulo (rad).
const VIEW_DEFLECTION: f64 = 0.05;
const VIEW_ANGLE: f64 = 0.25;

/// Recálculo vigente del documento.
pub struct CadCache {
    /// Huella del documento con el que se calculó
    pub doc_hash: u64,
    pub eval: cad_model::Evaluation,
    /// El recálculo anterior: ir y volver entre el borrador de una operación y
    /// el documento (cancelar, exportar con el diálogo abierto) no recalcula
    pub previous: Option<(u64, cad_model::Evaluation)>,
}

/// Malla del escaneo preparada para elegir zonas (cara a cara con el visor).
pub struct ScanCache {
    pub scene_hash: u64,
    pub mesh: ScanMesh,
    /// mm por unidad de la escena
    pub mm_per_unit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CadStatus {
    pub available: bool,
    pub occt_version: String,
    pub has_document: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SketchView {
    pub id: cad_model::FeatureId,
    pub plane: Plane,
    pub sketch: Sketch,
    pub report: SolveReport,
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BodyInfo {
    pub volume: f64,
    pub area: f64,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    pub faces: usize,
    pub edges: usize,
    pub valid: bool,
    /// Centro de masa (mm) y momentos principales de inercia con densidad 1
    /// (mm⁵; × densidad en kg/mm³ = kg·mm²) con sus ejes.
    pub center: [f64; 3],
    pub inertia: [f64; 3],
    pub axes: [[f64; 3]; 3],
}

/// Resultado de recalcular: estado de cada operación, sketches resueltos y
/// medidas del sólido.
#[derive(Debug, Clone, Serialize)]
pub struct CadResult {
    pub status: Vec<FeatureStatus>,
    pub sketches: Vec<SketchView>,
    pub body: Option<BodyInfo>,
    /// Parámetros calculados (valor o error)
    pub parameters: Vec<cad_model::ResolvedValue>,
    /// Campos calculados por fórmula (ruta → valor o error)
    pub bindings: Vec<cad_model::ResolvedValue>,
    /// Cambia con cada recálculo: el visor vuelve a pedir la malla
    pub version: u64,
    /// Operaciones que se calcularon en este recálculo (las demás, de la caché)
    pub recomputed: usize,
    /// Piezas, con sus caras y aristas dentro del cuerpo
    pub parts: Vec<PartView>,
    /// Planos, ejes y puntos de referencia, en el orden del árbol
    pub references: Vec<RefView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefView {
    pub id: cad_model::FeatureId,
    #[serde(flatten)]
    pub geom: cad_model::RefGeom,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartView {
    pub id: cad_model::PartId,
    pub name: String,
    /// Rango [desde, hasta) de caras y de aristas del cuerpo
    pub faces: [usize; 2],
    pub edges: [usize; 2],
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
}

/// "#rrggbb" → (r, g, b) en 0..1.
fn hex_rgb(s: &str) -> Option<[f64; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f64 / 255.0);
    Some([c(0)?, c(2)?, c(4)?])
}

/// Nombre de cada pieza: el que le puso el usuario o "Pieza n".
fn part_name(doc: &Document, id: cad_model::PartId, n: usize) -> String {
    doc.parts.iter().find(|p| p.part == id).and_then(|p| p.name.clone()).unwrap_or_else(|| format!("Pieza {n}"))
}

fn doc_hash(doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // El JSON es estable y cubre todo el documento; el material, las carpetas
    // y los nombres y colores de piezas no cambian la geometría: no recalculan
    let doc = Document { material: None, folders: Vec::new(), parts: Vec::new(), ..doc.clone() };
    serde_json::to_string(&doc).unwrap_or_default().hash(&mut h);
    h.finish()
}

fn require_occt() -> Result<(), String> {
    if cad_model::occt::available() {
        Ok(())
    } else {
        Err("Esta compilación no incluye el núcleo CAD (OpenCASCADE)".into())
    }
}

/// Recalcula lo que muestra el visor (el borrador abierto, o el documento) si
/// cambió desde la última vez.
fn evaluate(state: &AppState) -> Result<CadResult, String> {
    let preview = state.cad_preview.lock().unwrap().clone();
    let doc = match preview {
        Some(d) => d,
        None => state.cad_document.lock().unwrap().clone().ok_or("No hay un diseño abierto")?,
    };
    evaluate_doc(state, &doc)
}

/// Recalcula el documento guardado, sin el borrador (exportar, pasar a modelo).
fn evaluate_committed(state: &AppState) -> Result<CadResult, String> {
    let doc = state.cad_document.lock().unwrap().clone().ok_or("No hay un diseño abierto")?;
    evaluate_doc(state, &doc)
}

fn evaluate_doc(state: &AppState, doc: &Document) -> Result<CadResult, String> {
    let hash = doc_hash(doc);
    let mut cache = state.cad_cache.lock().unwrap();
    match cache.take() {
        Some(c) if c.doc_hash == hash => *cache = Some(c),
        Some(CadCache { doc_hash, eval, previous: Some((h, prev)) }) if h == hash => {
            *cache = Some(CadCache { doc_hash: h, eval: prev, previous: Some((doc_hash, eval)) });
        }
        old => {
            let previous = old.map(|c| (c.doc_hash, c.eval));
            let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
            *cache = Some(CadCache { doc_hash: hash, eval, previous });
        }
    }
    let eval = &cache.as_ref().unwrap().eval;
    let mut sketches: Vec<SketchView> = eval
        .sketches
        .iter()
        .map(|(id, r)| SketchView {
            id: *id,
            plane: r.plane,
            sketch: r.sketch.clone(),
            report: r.report.clone(),
            regions: r.regions.clone(),
        })
        .collect();
    sketches.sort_by_key(|s| doc.index_of(s.id));
    let body = eval.body.as_ref().and_then(|b| {
        let m = b.mass().ok()?;
        Some(BodyInfo {
            volume: m.volume,
            area: m.area,
            bbox_min: m.bbox_min,
            bbox_max: m.bbox_max,
            faces: b.face_count(),
            edges: b.edge_count(),
            valid: b.is_valid(),
            center: m.center,
            inertia: m.inertia,
            axes: m.axes,
        })
    });
    let parts = part_views(doc, eval);
    let mut references: Vec<RefView> = eval.references.iter().map(|(id, g)| RefView { id: *id, geom: g.clone() }).collect();
    references.sort_by_key(|r| doc.index_of(r.id));
    Ok(CadResult {
        parts,
        references,
        status: eval.status.clone(),
        sketches,
        body,
        parameters: eval.parameters.clone(),
        bindings: eval.bindings.clone(),
        version: hash,
        recomputed: eval.recomputed,
    })
}

fn status_impl(state: &AppState) -> CadStatus {
    CadStatus {
        available: cad_model::occt::available(),
        occt_version: cad_model::occt::occt_version(),
        has_document: state.cad_document.lock().unwrap().is_some(),
    }
}

#[tauri::command]
pub fn cad_status(state: tauri::State<'_, AppState>) -> CadStatus {
    status_impl(&state)
}

/// Empieza un diseño vacío (reemplaza el actual).
#[tauri::command]
pub async fn cad_new(app: AppHandle) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, new_impl).await
}

fn new_impl(state: &AppState) -> Result<CadResult, String> {
    *state.cad_document.lock().unwrap() = Some(Document::new());
    *state.cad_preview.lock().unwrap() = None;
    evaluate(state)
}

#[tauri::command]
pub fn cad_close(state: tauri::State<'_, AppState>) {
    *state.cad_document.lock().unwrap() = None;
    *state.cad_preview.lock().unwrap() = None;
    *state.cad_cache.lock().unwrap() = None;
}

#[tauri::command]
pub fn cad_get_document(state: tauri::State<'_, AppState>) -> Option<Document> {
    state.cad_document.lock().unwrap().clone()
}

/// Reemplaza el documento y recalcula.
#[tauri::command]
pub async fn cad_set_document(app: AppHandle, document: Document) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| set_document_impl(state, document)).await
}

fn set_document_impl(state: &AppState, document: Document) -> Result<CadResult, String> {
    *state.cad_document.lock().unwrap() = Some(document);
    *state.cad_preview.lock().unwrap() = None;
    evaluate(state)
}

/// Vista previa de una operación en edición: el visor muestra `document` sin
/// reemplazar el documento guardado. `None` vuelve al documento (cancelar).
#[tauri::command]
pub async fn cad_preview(app: AppHandle, document: Option<Document>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| preview_impl(state, document)).await
}

fn preview_impl(state: &AppState, document: Option<Document>) -> Result<CadResult, String> {
    *state.cad_preview.lock().unwrap() = document;
    evaluate(state)
}

/// Estado del recálculo actual (sin cambiar nada).
#[tauri::command]
pub async fn cad_evaluate(app: AppHandle) -> Result<CadResult, String> {
    in_background(app, evaluate).await
}

/// Calcula una fórmula con los parámetros dados (para validar mientras se escribe).
#[tauri::command]
pub fn cad_eval_expr(expr: String, parameters: Vec<cad_model::Parameter>) -> Result<f64, String> {
    Document { parameters, ..Default::default() }.eval_expr(&expr)
}

#[derive(Debug, Clone, Serialize)]
pub struct SolvedSketch {
    pub sketch: Sketch,
    pub report: SolveReport,
    pub regions: Vec<Region>,
}

/// Resuelve un sketch suelto mientras se dibuja o se arrastra un punto, sin
/// recalcular el árbol.
#[tauri::command]
pub fn cad_solve_sketch(mut sketch: Sketch, drag: Option<(u32, [f64; 2])>) -> Result<SolvedSketch, String> {
    let report = match drag {
        Some((p, target)) => sketch.solve_drag(p, target),
        None => sketch.solve(),
    }
    .map_err(|e| e.to_string())?;
    let regions = cad_model::find_regions(&sketch).map_err(|e| e.to_string())?;
    Ok(SolvedSketch { sketch, report, regions })
}

/// mm por unidad de la escena (1 si no hay escena: el CAD solo se ve en mm).
fn mm_per_unit(state: &AppState) -> f64 {
    state.scene.lock().unwrap().as_ref().map_or(1.0, |s| s.meters_per_unit * 1000.0)
}

/// Malla del sólido para el visor (Y arriba, unidades de la escena).
///
/// Binario little-endian, palabras de 4 bytes. Cabecera `u32 × 4`: vértices,
/// triángulos, aristas, puntos de aristas. Luego posiciones `f32 × 3V`,
/// normales `f32 × 3V`, índices `u32 × 3T`, cara de cada triángulo `u32 × T`,
/// fin de cada arista (en puntos, acumulado) `u32 × E`, puntos `f32 × 3P`.
#[tauri::command]
pub async fn cad_mesh(app: AppHandle) -> Result<Response, String> {
    in_background(app, |state| mesh_impl(state).map(Response::new)).await
}

fn mesh_impl(state: &AppState) -> Result<Vec<u8>, String> {
    evaluate(state)?;
    let scale = 1.0 / mm_per_unit(state);
    let cache = state.cad_cache.lock().unwrap();
    match cache.as_ref().and_then(|c| c.eval.body.as_ref()) {
        Some(body) => encode_view_mesh(body, scale),
        None => Ok(vec![0; 16]),
    }
}

/// Herramienta de una operación (vista previa: verde si suma, roja si resta),
/// en el mismo formato que [`cad_mesh`]; vacía si la operación no tiene.
#[tauri::command]
pub async fn cad_tool_mesh(app: AppHandle, feature: cad_model::FeatureId) -> Result<Response, String> {
    in_background(app, move |state| tool_mesh_impl(state, feature).map(Response::new)).await
}

fn tool_mesh_impl(state: &AppState, feature: cad_model::FeatureId) -> Result<Vec<u8>, String> {
    evaluate(state)?;
    let scale = 1.0 / mm_per_unit(state);
    let cache = state.cad_cache.lock().unwrap();
    match cache.as_ref().and_then(|c| c.eval.tool(feature)) {
        Some((shape, _)) => encode_view_mesh(shape, scale),
        None => Ok(vec![0; 16]),
    }
}

/// Teselado para el visor: posiciones y normales (Y arriba, unidades de la
/// escena), triángulos, cara de cada triángulo y aristas como polilíneas.
fn encode_view_mesh(body: &cad_model::Shape, scale: f64) -> Result<Vec<u8>, String> {
    {
        let t = body.tessellate(VIEW_DEFLECTION, VIEW_ANGLE).map_err(|e| e.to_string())?;
        let edge_points: usize = t.edges.iter().map(|e| e.len()).sum();
        let mut out = Vec::with_capacity(16 + t.positions.len() * 24 + t.triangles.len() * 16 + edge_points * 12);
        for w in [t.positions.len(), t.triangles.len(), t.edges.len(), edge_points] {
            out.extend_from_slice(&(w as u32).to_le_bytes());
        }
        let put = |out: &mut Vec<u8>, p: [f64; 3], s: f64| {
            for v in converter_scene::z_up_to_y_up([(p[0] * s) as f32, (p[1] * s) as f32, (p[2] * s) as f32]) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        };
        t.positions.iter().for_each(|p| put(&mut out, *p, scale));
        t.normals.iter().for_each(|n| put(&mut out, *n, 1.0));
        t.triangles.iter().flatten().for_each(|i| out.extend_from_slice(&i.to_le_bytes()));
        t.triangle_face.iter().for_each(|f| out.extend_from_slice(&f.to_le_bytes()));
        let mut end = 0u32;
        for e in &t.edges {
            end += e.len() as u32;
            out.extend_from_slice(&end.to_le_bytes());
        }
        t.edges.iter().flatten().for_each(|p| put(&mut out, *p, scale));
        Ok(out)
    }
}

/// Referencia estable a una cara del sólido actual (índice del teselado).
#[tauri::command]
pub async fn cad_face_ref(app: AppHandle, face: usize) -> Result<FaceRef, String> {
    in_background(app, move |state| face_ref_impl(state, face)).await
}

fn face_ref_impl(state: &AppState, face: usize) -> Result<FaceRef, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    cache.as_ref().and_then(|c| c.eval.face_ref(face)).ok_or_else(|| "Esa cara no existe".to_string())
}

#[tauri::command]
pub async fn cad_edge_ref(app: AppHandle, edge: usize) -> Result<EdgeRef, String> {
    in_background(app, move |state| edge_ref_impl(state, edge)).await
}

fn edge_ref_impl(state: &AppState, edge: usize) -> Result<EdgeRef, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    cache.as_ref().and_then(|c| c.eval.edge_ref(edge)).ok_or_else(|| "Esa arista no existe".to_string())
}

/// Qué cara o arista del sólido mostrado es cada referencia (`None` = no se encontró).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResolvedRefs {
    pub faces: Vec<Option<usize>>,
    pub edges: Vec<Option<usize>>,
}

/// Resuelve referencias contra el sólido que se ve (el borrador, si hay): para
/// resaltar lo elegido en una caja de selección y marcar lo que ya no está.
#[tauri::command]
pub async fn cad_resolve_refs(app: AppHandle, faces: Vec<FaceRef>, edges: Vec<EdgeRef>) -> Result<ResolvedRefs, String> {
    in_background(app, move |state| resolve_refs_impl(state, &faces, &edges)).await
}

fn resolve_refs_impl(state: &AppState, faces: &[FaceRef], edges: &[EdgeRef]) -> Result<ResolvedRefs, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let Some(c) = cache.as_ref() else { return Ok(ResolvedRefs::default()) };
    Ok(ResolvedRefs {
        faces: faces.iter().map(|r| c.eval.resolve_face(r).ok()).collect(),
        edges: edges.iter().map(|r| c.eval.resolve_edge(r).ok()).collect(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct FaceDescription {
    pub surface: String,
    pub area: f64,
    pub point: [f64; 3],
    pub normal: [f64; 3],
    pub radius: Option<f64>,
}

/// Qué es una cara (para el panel de medidas y para ofrecer operaciones).
#[tauri::command]
pub async fn cad_face_info(app: AppHandle, face: usize) -> Result<FaceDescription, String> {
    in_background(app, move |state| face_info_impl(state, face)).await
}

fn face_info_impl(state: &AppState, face: usize) -> Result<FaceDescription, String> {
    {
        evaluate(state)?;
        let cache = state.cad_cache.lock().unwrap();
        let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("No hay sólido")?;
        let f = body.face_info(face).map_err(|e| e.to_string())?;
        Ok(FaceDescription {
            surface: format!("{:?}", f.surface).to_lowercase(),
            area: f.area,
            point: f.point,
            normal: f.normal,
            radius: f.radius,
        })
    }
}

/// Piezas que hay justo antes de la operación `index` de `document` (para
/// elegirlas en su diálogo: la vista previa ya las muestra cambiadas).
#[tauri::command]
pub async fn cad_parts_at(app: AppHandle, document: Document, index: usize) -> Result<Vec<PartView>, String> {
    in_background(app, move |state| parts_at_impl(state, document, index)).await
}

fn parts_at_impl(state: &AppState, mut doc: Document, index: usize) -> Result<Vec<PartView>, String> {
    require_occt()?;
    doc.rollback = Some(doc.rollback.map_or(index, |r| r.min(index)));
    let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
    Ok(part_views(&doc, &eval))
}

fn part_views(doc: &Document, eval: &cad_model::Evaluation) -> Vec<PartView> {
    eval.part_ranges()
        .into_iter()
        .zip(&eval.parts)
        .enumerate()
        .map(|(i, ((id, f, e), p))| {
            let m = p.shape.mass().ok();
            PartView {
                id,
                name: part_name(doc, id, i + 1),
                faces: [f.start, f.end],
                edges: [e.start, e.end],
                volume: m.map_or(0.0, |m| m.volume),
                area: m.map_or(0.0, |m| m.area),
                center: m.map_or([0.0; 3], |m| m.center),
            }
        })
        .collect()
}

/// Una vista de un plano: desde dónde se mira (hacia quien mira) y la derecha de la hoja.
#[derive(Debug, Clone, Deserialize)]
pub struct DrawingViewSpec {
    pub eye: [f64; 3],
    pub xdir: [f64; 3],
    /// Vista en corte: se conserva el lado de `normal` del plano por `origin`
    #[serde(default)]
    pub section: Option<DrawingSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrawingSection {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

/// Una vista: sus líneas y, si es un corte, los triángulos de la cara cortada (para rayar).
#[derive(Debug, Clone, Serialize)]
pub struct DrawingViewResult {
    pub lines: Vec<DrawingLine>,
    pub hatch: Vec<[[f64; 2]; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrawingLine {
    /// "visible", "outline", "hidden", "hidden_outline" o "smooth"
    pub kind: &'static str,
    pub points: Vec<[f64; 2]>,
}

/// Líneas de cada vista del diseño guardado (líneas ocultas exactas), en mm.
#[tauri::command]
pub async fn cad_drawing(app: AppHandle, views: Vec<DrawingViewSpec>) -> Result<Vec<DrawingViewResult>, String> {
    in_background(app, move |state| drawing_impl(state, &views)).await
}

fn drawing_impl(state: &AppState, views: &[DrawingViewSpec]) -> Result<Vec<DrawingViewResult>, String> {
    require_occt()?;
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("El diseño todavía no tiene un sólido")?;
    let m = body.mass().map_err(|e| e.to_string())?;
    let size = (0..3).map(|k| m.bbox_max[k] - m.bbox_min[k]).fold(0.0, f64::max).max(1.0);
    views
        .iter()
        .map(|v| {
            // Corte: la mitad del lado de la normal; sus caras sobre el plano se rayan
            let cut = match &v.section {
                Some(s) => Some(body.split_keep(s.origin, s.normal).map_err(|e| e.to_string())?),
                None => None,
            };
            let shape = cut.as_ref().unwrap_or(body);
            let hatch = match (&v.section, &cut) {
                (Some(s), Some(c)) => section_hatch(c, s, v, size),
                _ => Vec::new(),
            };
            let lines = shape.hlr(v.eye, v.xdir, size * 2e-4).map_err(|e| e.to_string())?;
            let lines = lines
                .into_iter()
                .map(|l| DrawingLine {
                    kind: match l.kind {
                        cad_model::occt::HlrKind::Visible => "visible",
                        cad_model::occt::HlrKind::VisibleOutline => "outline",
                        cad_model::occt::HlrKind::Hidden => "hidden",
                        cad_model::occt::HlrKind::HiddenOutline => "hidden_outline",
                        cad_model::occt::HlrKind::Smooth => "smooth",
                    },
                    points: l.points,
                })
                .collect();
            Ok(DrawingViewResult { lines, hatch })
        })
        .collect()
}

/// Triángulos (en la vista) de las caras de `cut` que quedaron sobre el plano de corte.
fn section_hatch(cut: &cad_model::Shape, s: &DrawingSection, v: &DrawingViewSpec, size: f64) -> Vec<[[f64; 2]; 3]> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len = dot(s.normal, s.normal).sqrt().max(1e-12);
    let n = s.normal.map(|c| c / len);
    let on_plane: Vec<bool> = (0..cut.face_count())
        .map(|i| {
            cut.face_info(i).is_ok_and(|f| {
                f.surface == cad_model::occt::SurfaceKind::Plane
                    && dot(f.normal, n).abs() > 0.999
                    && (dot(f.point, n) - dot(s.origin, n)).abs() < size * 1e-6
            })
        })
        .collect();
    let Ok(t) = cut.tessellate(size * 1e-3, 0.3) else { return Vec::new() };
    // Coordenadas de la vista: x = derecha de la hoja, y = eye × x
    let e = v.eye;
    let x = v.xdir;
    let y = [e[1] * x[2] - e[2] * x[1], e[2] * x[0] - e[0] * x[2], e[0] * x[1] - e[1] * x[0]];
    let proj = |p: [f64; 3]| [dot(p, x), dot(p, y)];
    t.triangles
        .iter()
        .zip(&t.triangle_face)
        .filter(|(_, f)| on_plane.get(**f as usize).copied().unwrap_or(false))
        .map(|(tri, _)| tri.map(|k| proj(t.positions[k as usize])))
        .collect()
}

/// Escribe un archivo de texto (planos SVG o DXF hechos en la interfaz).
#[tauri::command]
pub async fn cad_write_text(path: String, content: String) -> Result<u64, String> {
    std::fs::write(&path, content.as_bytes()).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
    Ok(content.len() as u64)
}

/// Medidas de una o dos cosas elegidas en el sólido que se ve.
#[tauri::command]
pub async fn cad_measure(app: AppHandle, items: Vec<cad_model::MeasureItem>) -> Result<cad_model::Measurement, String> {
    in_background(app, move |state| measure_impl(state, &items)).await
}

fn measure_impl(state: &AppState, items: &[cad_model::MeasureItem]) -> Result<cad_model::Measurement, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("No hay sólido")?;
    cad_model::measure(body, items)
}

fn body_scene(state: &AppState, name: &str) -> Result<Scene, String> {
    parts_scene(state, name, None)
}

/// Escena para exportar: una malla por pieza con su nombre (o solo `only`).
fn parts_scene(state: &AppState, name: &str, only: Option<cad_model::PartId>) -> Result<Scene, String> {
    evaluate_committed(state)?;
    let doc = state.cad_document.lock().unwrap().clone().unwrap_or_default();
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    if eval.parts.is_empty() {
        return Err("El diseño todavía no tiene un sólido".into());
    }
    let mut scene = Scene::new();
    let several = eval.parts.len() > 1;
    for (i, p) in eval.parts.iter().enumerate() {
        if only.is_some_and(|o| o != p.id) {
            continue;
        }
        // Más fino que el del visor: es lo que se imprime o se exporta
        let t = p.shape.tessellate(0.01, 0.1).map_err(|e| e.to_string())?;
        let positions: Vec<[f32; 3]> = t
            .positions
            .iter()
            .map(|p| converter_scene::z_up_to_y_up([p[0] as f32, p[1] as f32, p[2] as f32]))
            .collect();
        let normals: Vec<[f32; 3]> = t
            .normals
            .iter()
            .map(|n| converter_scene::z_up_to_y_up([n[0] as f32, n[1] as f32, n[2] as f32]))
            .collect();
        // Una pieza sola conserva el nombre del diseño
        let label = if several || only.is_some() { part_name(&doc, p.id, i + 1) } else { name.to_string() };
        let mesh = scene.meshes.len();
        scene.meshes.push(SceneMesh {
            name: label.clone(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
                indices: Some(IndexData::U32(t.triangles.iter().flatten().copied().collect())),
                material: None,
            }],
        });
        scene.root_nodes.push(scene.nodes.len());
        scene.nodes.push(Node { name: label, transform: Transform::identity(), mesh: Some(mesh), skin: None, children: Vec::new() });
    }
    if scene.meshes.is_empty() {
        return Err("Esa pieza ya no existe".into());
    }
    scene.meters_per_unit = 0.001;
    Ok(scene)
}

/// Exporta el sólido: STEP (exacto) o malla (STL, 3MF, OBJ, PLY, GLB).
#[tauri::command]
pub async fn cad_export(app: AppHandle, path: String, format: String, part: Option<cad_model::PartId>) -> Result<u64, String> {
    require_occt()?;
    in_background(app, move |state| export_impl(state, &path, &format, part)).await
}

fn export_impl(state: &AppState, path: &str, format: &str, part: Option<cad_model::PartId>) -> Result<u64, String> {
    {
        let p = std::path::Path::new(path);
        let scene = || parts_scene(state, "Diseño", part);
        match format {
            "step" | "stp" => {
                // Cada pieza como sólido con su nombre y su color (si se eligió)
                evaluate_committed(state)?;
                let doc = state.cad_document.lock().unwrap().clone().unwrap_or_default();
                let cache = state.cad_cache.lock().unwrap();
                let eval = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
                let names: Vec<String> = eval.parts.iter().enumerate().map(|(i, x)| part_name(&doc, x.id, i + 1)).collect();
                let list: Vec<(&cad_model::Shape, &str, Option<[f64; 3]>)> = eval
                    .parts
                    .iter()
                    .zip(&names)
                    .filter(|(x, _)| part.is_none_or(|id| id == x.id))
                    .map(|(x, n)| {
                        let color = doc.parts.iter().find(|q| q.part == x.id).and_then(|q| q.color.as_deref()).and_then(hex_rgb);
                        (&x.shape, n.as_str(), color)
                    })
                    .collect();
                if list.is_empty() {
                    return Err(if part.is_some() { "Esa pieza ya no existe" } else { "El diseño todavía no tiene un sólido" }.into());
                }
                let bytes = cad_model::Shape::parts_to_step(&list).map_err(|e| e.to_string())?;
                std::fs::write(p, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
            }
            "stl" => converter_stl::export_stl(&scene()?, p).map_err(|e| format!("Error exportando STL: {e:?}"))?,
            "3mf" => converter_3mf::export_3mf(&scene()?, p).map_err(|e| format!("Error exportando 3MF: {e}"))?,
            "obj" => converter_obj::export_obj(&scene()?, p).map_err(|e| format!("Error exportando OBJ: {e:?}"))?,
            "ply" => converter_ply::export_ply(&scene()?, p).map_err(|e| format!("Error exportando PLY: {e}"))?,
            "glb" => converter_gltf_io::export_glb(&scene()?, p, &Default::default())
                .map_err(|e| format!("Error exportando GLB: {e}"))?,
            other => return Err(format!("Formato no soportado para el diseño: {other}")),
        }
        Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
    }
}

/// Agrega un STEP como operación (unir, restar o intersecar).
#[tauri::command]
pub async fn cad_import_step(app: AppHandle, path: String, op: Option<BodyOp>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| {
        let data = std::fs::read(&path).map_err(|e| format!("No se pudo leer {path}: {e}"))?;
        // Validar antes de meterlo al documento
        cad_model::Shape::from_step(&data).map_err(|e| e.to_string())?;
        *state.cad_preview.lock().unwrap() = None;
        let mut lock = state.cad_document.lock().unwrap();
        let doc = lock.get_or_insert_with(Document::new);
        let id = doc.add(FeatureKind::Import { format: ImportFormat::Step, data, op: op.unwrap_or_default() });
        if let Some(name) = std::path::Path::new(&path).file_stem() {
            doc.get_mut(id).unwrap().name = name.to_string_lossy().into_owned();
        }
        drop(lock);
        evaluate(state)
    })
    .await
}

/// El sólido pasa a ser el modelo de la app (para imprimir, pintar, rigging…).
#[tauri::command]
pub async fn cad_to_model(app: AppHandle, on_progress: Channel<Progress>) -> Result<MeshInfo, String> {
    require_occt()?;
    in_background(app, move |state| {
        let scene = body_scene(state, "Diseño")?;
        load_scene(scene, "Diseño".into(), "CAD".into(), &on_progress, state)
    })
    .await
}

// ═══════════════════════════════════════════════════════════════════════════
// ESCANEO → CAD
// ═══════════════════════════════════════════════════════════════════════════

fn scene_hash(scene: &Scene) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for prim in scene.world_primitives() {
        prim.positions.len().hash(&mut h);
        for p in &prim.positions {
            for v in p {
                v.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Corre `f` con la malla del modelo actual lista para escaneo → CAD (Z arriba,
/// mm). Se prepara una vez y queda en caché mientras el modelo no cambie.
fn with_scan<T>(state: &AppState, f: impl FnOnce(&ScanCache) -> Result<T, String>) -> Result<T, String> {
    let scene_lock = state.scene.lock().unwrap();
    let scene = scene_lock.as_ref().ok_or("No hay un modelo cargado para tomar medidas")?;
    let hash = scene_hash(scene);
    let mut cache = state.cad_scan.lock().unwrap();
    if cache.as_ref().is_none_or(|c| c.scene_hash != hash) {
        let mm = scene.meters_per_unit * 1000.0;
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for prim in scene.world_primitives_z_up() {
            let base = vertices.len() as u32;
            vertices.extend(prim.positions.iter().map(|p| [p[0] as f64 * mm, p[1] as f64 * mm, p[2] as f64 * mm]));
            triangles.extend(prim.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        *cache = Some(ScanCache { scene_hash: hash, mesh: ScanMesh::new(&vertices, &triangles), mm_per_unit: mm });
    }
    f(cache.as_ref().unwrap())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScanPickOptions {
    pub angle_threshold: Option<f64>,
    pub tolerance: Option<f64>,
    /// Simplificación de contornos (mm)
    pub outline_tolerance: Option<f64>,
}

impl ScanPickOptions {
    fn pick(&self) -> PickOptions {
        let d = PickOptions::default();
        PickOptions { angle_threshold: self.angle_threshold.unwrap_or(d.angle_threshold), tolerance: self.tolerance, ..d }
    }

    fn outline(&self, mesh: &ScanMesh) -> OutlineOptions {
        OutlineOptions { tolerance: self.outline_tolerance.unwrap_or(mesh.diagonal() * 0.002), ..Default::default() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScanPick {
    Plane {
        #[serde(flatten)]
        pick: PlanePick,
        /// Espesor del material bajo la zona, si se pudo medir
        depth: Option<f64>,
    },
    Cylinder {
        #[serde(flatten)]
        pick: CylinderPick,
    },
}

/// Elige una zona del modelo con un clic (`triangle` = índice del triángulo en
/// la malla del visor) y le ajusta un plano o un cilindro.
#[tauri::command]
pub async fn cad_scan_pick(app: AppHandle, kind: String, triangle: u32, options: Option<ScanPickOptions>) -> Result<ScanPick, String> {
    in_background(app, move |state| scan_pick_impl(state, &kind, triangle, options.unwrap_or_default())).await
}

fn scan_pick_impl(state: &AppState, kind: &str, triangle: u32, options: ScanPickOptions) -> Result<ScanPick, String> {
    {
        with_scan(state, |scan| match kind {
            "plane" => {
                let pick = cad_scan::pick_plane(&scan.mesh, triangle, &options.pick()).map_err(|e| e.to_string())?;
                let depth = pick.depth(&scan.mesh);
                Ok(ScanPick::Plane { pick, depth })
            }
            "cylinder" => {
                let pick = cad_scan::pick_cylinder(&scan.mesh, triangle, &options.pick()).map_err(|e| e.to_string())?;
                Ok(ScanPick::Cylinder { pick })
            }
            other => Err(format!("Tipo de zona desconocido: {other}")),
        })
    }
}

/// Detecta todas las zonas planas, cilíndricas y esféricas del modelo.
#[tauri::command]
pub async fn cad_scan_detect(app: AppHandle, options: Option<ScanPickOptions>) -> Result<Vec<Detection>, String> {
    in_background(app, move |state| scan_detect_impl(state, options.unwrap_or_default())).await
}

fn scan_detect_impl(state: &AppState, options: ScanPickOptions) -> Result<Vec<Detection>, String> {
    {
        with_scan(state, |scan| {
            let d = DetectOptions::default();
            let opts = DetectOptions {
                angle_threshold: options.angle_threshold.unwrap_or(d.angle_threshold),
                tolerance: options.tolerance,
                ..d
            };
            Ok(cad_scan::detect_all(&scan.mesh, &opts))
        })
    }
}

/// Corte del modelo por un plano (coordenadas del plano, mm).
#[tauri::command]
pub async fn cad_scan_slice(app: AppHandle, plane: Plane) -> Result<Vec<Section>, String> {
    in_background(app, move |state| with_scan(state, |scan| Ok(cad_scan::slice(&scan.mesh, &plane)))).await
}

/// Qué agregar al diseño desde el escaneo.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScanFeature {
    /// Sketch con el contorno de una zona plana y, si `extrude`, su extrusión
    /// hacia adentro con la profundidad medida (o `depth`).
    PlaneOutline { triangle: u32, extrude: bool, depth: Option<f64>, op: Option<BodyOp> },
    /// Cilindro ajustado: agujero (resta) o tetón (une).
    Cylinder { triangle: u32 },
    /// Sketch con el corte del modelo por un plano.
    Slice { plane: Plane },
    /// Plano de referencia (sketch vacío) sobre una zona plana.
    WorkPlane { triangle: u32 },
}

/// Convierte lo elegido en el escaneo en operaciones del diseño.
#[tauri::command]
pub async fn cad_scan_add(app: AppHandle, feature: ScanFeature, options: Option<ScanPickOptions>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| scan_add_impl(state, feature, options.unwrap_or_default())).await
}

fn scan_add_impl(state: &AppState, feature: ScanFeature, options: ScanPickOptions) -> Result<CadResult, String> {
    {
        let kinds: Vec<(FeatureKind, Option<String>)> = with_scan(state, |scan| {
            let mesh = &scan.mesh;
            Ok(match &feature {
                ScanFeature::PlaneOutline { triangle, extrude, depth, op } => {
                    let pick = cad_scan::pick_plane(mesh, *triangle, &options.pick()).map_err(|e| e.to_string())?;
                    let mut v = vec![(pick.sketch_feature(&options.outline(mesh)), Some("Contorno escaneado".to_string()))];
                    if *extrude {
                        let d = depth.or_else(|| pick.depth(mesh)).ok_or("No se pudo medir la profundidad: indicarla a mano")?;
                        // El id del sketch se completa al agregar
                        v.push((cad_scan::extrude_into(cad_model::FeatureId(u32::MAX), d, op.unwrap_or_default()), None));
                    }
                    v
                }
                ScanFeature::Cylinder { triangle } => {
                    let pick = cad_scan::pick_cylinder(mesh, *triangle, &options.pick()).map_err(|e| e.to_string())?;
                    let name = if pick.hole { "Agujero escaneado" } else { "Cilindro escaneado" };
                    vec![(pick.feature(), Some(name.to_string()))]
                }
                ScanFeature::Slice { plane } => {
                    let loops: Vec<Vec<[f64; 2]>> =
                        cad_scan::slice(mesh, plane).into_iter().filter(|s| s.closed).map(|s| s.points).collect();
                    if loops.is_empty() {
                        return Err("El plano no corta el modelo en contornos cerrados".into());
                    }
                    let sketch = cad_scan::outline_sketch(&loops, &options.outline(mesh));
                    vec![(
                        FeatureKind::Sketch { plane: PlaneSpec::Custom { plane: *plane }, offset: 0.0, sketch },
                        Some("Corte escaneado".to_string()),
                    )]
                }
                ScanFeature::WorkPlane { triangle } => {
                    let pick = cad_scan::pick_plane(mesh, *triangle, &options.pick()).map_err(|e| e.to_string())?;
                    vec![(
                        FeatureKind::Sketch { plane: PlaneSpec::Custom { plane: pick.plane }, offset: 0.0, sketch: Sketch::new() },
                        Some("Plano escaneado".to_string()),
                    )]
                }
            })
        })?;
        *state.cad_preview.lock().unwrap() = None;
        let mut lock = state.cad_document.lock().unwrap();
        let doc = lock.get_or_insert_with(Document::new);
        let mut last_sketch = None;
        for (mut kind, name) in kinds {
            if let FeatureKind::Extrude(e) = &mut kind
                && let Some(s) = last_sketch
            {
                e.sketch = s;
            }
            let is_sketch = matches!(kind, FeatureKind::Sketch { .. });
            let id = doc.add(kind);
            if let Some(n) = name {
                doc.get_mut(id).unwrap().name = n;
            }
            if is_sketch {
                last_sketch = Some(id);
            }
        }
        drop(lock);
        evaluate(state)
    }
}

/// mm por unidad de la escena: el visor convierte el sketch (mm) a sus unidades.
#[tauri::command]
pub fn cad_mm_per_unit(state: tauri::State<'_, AppState>) -> f64 {
    mm_per_unit(&state)
}

/// Errores de recálculo legibles (para avisos).
pub fn errors_text(result: &CadResult, doc: &Document) -> Vec<String> {
    let names: HashMap<_, _> = doc.features.iter().map(|f| (f.id, f.name.clone())).collect();
    result
        .status
        .iter()
        .filter_map(|s| match &s.state {
            FeatureState::Error { message, .. } => Some(format!("{}: {message}", names.get(&s.id).cloned().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

/// Los comandos CAD sin Tauri, para probar el frontend en un navegador con el
/// backend real (ver `examples/cad_http.rs`).
pub mod bridge {
    use super::*;
    use serde_json::{json, Value};

    pub enum Reply {
        Json(Value),
        Bytes(Vec<u8>),
    }

    fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
        serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("argumento {name}: {e}"))
    }

    fn ok<T: Serialize>(v: T) -> Result<Reply, String> {
        serde_json::to_value(v).map(Reply::Json).map_err(|e| e.to_string())
    }

    /// Ejecuta un comando por nombre. Los que no son del CAD devuelven `null`
    /// (la app arranca igual), salvo importar un modelo y su malla.
    pub fn dispatch(state: &AppState, cmd: &str, args: &Value) -> Result<Reply, String> {
        match cmd {
            "cad_status" => ok(status_impl(state)),
            "cad_new" => ok(new_impl(state)?),
            "cad_close" => {
                *state.cad_document.lock().unwrap() = None;
                *state.cad_preview.lock().unwrap() = None;
                *state.cad_cache.lock().unwrap() = None;
                ok(())
            }
            "cad_get_document" => ok(state.cad_document.lock().unwrap().clone()),
            "cad_set_document" => ok(set_document_impl(state, arg(args, "document")?)?),
            "cad_preview" => ok(preview_impl(state, arg(args, "document")?)?),
            "cad_evaluate" => ok(evaluate(state)?),
            "cad_solve_sketch" => ok(cad_solve_sketch(arg(args, "sketch")?, arg(args, "drag")?)?),
            "cad_mesh" => mesh_impl(state).map(Reply::Bytes),
            "cad_tool_mesh" => tool_mesh_impl(state, arg(args, "feature")?).map(Reply::Bytes),
            "cad_face_ref" => ok(face_ref_impl(state, arg(args, "face")?)?),
            "cad_edge_ref" => ok(edge_ref_impl(state, arg(args, "edge")?)?),
            "cad_face_info" => ok(face_info_impl(state, arg(args, "face")?)?),
            "cad_drawing" => ok(drawing_impl(state, &arg::<Vec<DrawingViewSpec>>(args, "views")?)?),
            "cad_write_text" => {
                let (path, content): (String, String) = (arg(args, "path")?, arg(args, "content")?);
                std::fs::write(&path, content.as_bytes()).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
                ok(content.len() as u64)
            }
            "cad_parts_at" => ok(parts_at_impl(state, arg(args, "document")?, arg(args, "index")?)?),
            "cad_measure" => ok(measure_impl(state, &arg::<Vec<cad_model::MeasureItem>>(args, "items")?)?),
            "cad_resolve_refs" => {
                let (faces, edges): (Vec<FaceRef>, Vec<EdgeRef>) = (arg(args, "faces")?, arg(args, "edges")?);
                ok(resolve_refs_impl(state, &faces, &edges)?)
            }
            "cad_mm_per_unit" => ok(mm_per_unit(state)),
            "cad_export" => {
                let (path, format): (String, String) = (arg(args, "path")?, arg(args, "format")?);
                ok(export_impl(state, &path, &format, arg::<Option<cad_model::PartId>>(args, "part")?)?)
            }
            "cad_eval_expr" => ok(cad_eval_expr(arg(args, "expr")?, arg(args, "parameters")?)?),
            "cad_scan_pick" => {
                let kind: String = arg(args, "kind")?;
                ok(scan_pick_impl(state, &kind, arg(args, "triangle")?, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?)
            }
            "cad_scan_detect" => ok(scan_detect_impl(state, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?),
            "cad_scan_slice" => {
                let plane: Plane = arg(args, "plane")?;
                ok(with_scan(state, |scan| Ok(cad_scan::slice(&scan.mesh, &plane)))?)
            }
            "cad_scan_add" => ok(scan_add_impl(state, arg(args, "feature")?, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?),
            "import_model" => {
                let channel = Channel::new(|_| Ok(()));
                ok(crate::commands::import_model_impl(arg(args, "path")?, vec![], &channel, state)?)
            }
            "get_mesh_data" => crate::commands::get_mesh_data_impl(state).map(|d| Reply::Bytes(d.to_bytes())),
            "get_supported_formats" => ok(crate::commands::get_supported_formats()),
            _ => Ok(Reply::Json(json!(null))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_doc() -> Document {
        let mut doc = Document::new();
        doc.add(FeatureKind::Primitive(cad_model::Primitive {
            shape: cad_model::PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 30.0, centered: false, centered_z: false },
            origin: [0.0; 3],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::Join,
        }));
        doc
    }

    #[test]
    fn evaluate_caches_and_reports() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(box_doc());
        let r = evaluate(&state).unwrap();
        let body = r.body.unwrap();
        assert!((body.volume - 6000.0).abs() < 1e-6);
        assert_eq!(body.faces, 6);
        assert!(body.valid);
        let v = r.version;
        // Sin cambios: misma versión (caché)
        assert_eq!(evaluate(&state).unwrap().version, v);
        assert!(errors_text(&evaluate(&state).unwrap(), &box_doc()).is_empty());
    }

    #[test]
    fn material_and_folders_do_not_recalculate() {
        let mut doc = box_doc();
        let h = doc_hash(&doc);
        doc.material = Some(cad_model::Material { name: "PLA".into(), density: 1240.0 });
        doc.folders = vec![cad_model::Folder { name: "Base".into(), first: doc.features[0].id, last: doc.features[0].id, collapsed: true }];
        assert_eq!(doc_hash(&doc), h);
        doc.rollback = Some(0);
        assert_ne!(doc_hash(&doc), h);
    }

    #[test]
    fn preview_leaves_document_alone() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(box_doc());
        let base = evaluate(&state).unwrap();
        // Borrador con la caja más alta
        let mut draft = box_doc();
        if let FeatureKind::Primitive(p) = &mut draft.features[0].kind {
            p.shape = cad_model::PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 60.0, centered: false, centered_z: false };
        }
        let r = preview_impl(&state, Some(draft)).unwrap();
        assert!((r.body.unwrap().volume - 12000.0).abs() < 1e-6);
        assert_eq!(state.cad_document.lock().unwrap().as_ref(), Some(&box_doc()));
        // Exportar usa el documento, no el borrador
        let scene = body_scene(&state, "Diseño").unwrap();
        let max_y = scene.world_primitives()[0].positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((max_y - 30.0).abs() < 1e-4);
        // Cancelar vuelve al recálculo anterior
        let back = preview_impl(&state, None).unwrap();
        assert_eq!(back.version, base.version);
        assert!((back.body.unwrap().volume - 6000.0).abs() < 1e-6);
        // Guardar un documento descarta el borrador
        preview_impl(&state, Some(Document::new())).unwrap();
        set_document_impl(&state, box_doc()).unwrap();
        assert!(state.cad_preview.lock().unwrap().is_none());
    }

    #[test]
    fn resolve_refs_finds_and_misses() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(box_doc());
        evaluate(&state).unwrap();
        let (face, edge) = (face_ref_impl(&state, 2).unwrap(), edge_ref_impl(&state, 5).unwrap());
        let far = FaceRef { point: [500.0, 500.0, 500.0], normal: [0.3, 0.3, 0.9], ..Default::default() };
        let r = resolve_refs_impl(&state, &[face, far], &[edge]).unwrap();
        assert_eq!(r.faces, vec![Some(2), None]);
        assert_eq!(r.edges, vec![Some(5)]);
    }

    #[test]
    fn tool_mesh_of_a_feature() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        let doc = box_doc();
        let id = doc.features[0].id;
        *state.cad_document.lock().unwrap() = Some(doc);
        let tool = tool_mesh_impl(&state, id).unwrap();
        let triangles = u32::from_le_bytes(tool[4..8].try_into().unwrap());
        assert_eq!(triangles, 12, "la caja como herramienta");
        let none = tool_mesh_impl(&state, cad_model::FeatureId(99)).unwrap();
        assert_eq!(none, vec![0; 16]);
    }

    #[test]
    fn body_scene_is_y_up_in_mm() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(box_doc());
        let scene = body_scene(&state, "Diseño").unwrap();
        assert!((scene.meters_per_unit - 0.001).abs() < 1e-12);
        let prims = scene.world_primitives();
        let max_y = prims[0].positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((max_y - 30.0).abs() < 1e-4, "la altura (Z del CAD) queda en Y: {max_y}");
        // Y vuelve a Z arriba al pedirla así
        let z = scene.world_primitives_z_up()[0].positions.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
        assert!((z - 30.0).abs() < 1e-4);
    }

    #[test]
    fn scan_pick_from_scene() {
        if !cad_model::occt::available() {
            return;
        }
        // El sólido como "escaneo": su cara superior se elige y mide 30 de profundidad
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(box_doc());
        let scene = body_scene(&state, "Escaneo").unwrap();
        *state.scene.lock().unwrap() = Some(scene);
        let top = with_scan(&state, |scan| {
            Ok((0..scan.mesh.face_count()).find(|&f| scan.mesh.face_normals[f][2] > 0.99).unwrap() as u32)
        })
        .unwrap();
        let pick = with_scan(&state, |scan| {
            let p = cad_scan::pick_plane(&scan.mesh, top, &PickOptions::default()).map_err(|e| e.to_string())?;
            Ok((p.plane.origin[2], p.depth(&scan.mesh)))
        })
        .unwrap();
        assert!((pick.0 - 30.0).abs() < 1e-3);
        assert!((pick.1.unwrap() - 30.0).abs() < 1e-3);
    }
}
