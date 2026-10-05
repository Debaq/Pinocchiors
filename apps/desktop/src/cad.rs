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
}

/// Resultado de recalcular: estado de cada operación, sketches resueltos y
/// medidas del sólido.
#[derive(Debug, Clone, Serialize)]
pub struct CadResult {
    pub status: Vec<FeatureStatus>,
    pub sketches: Vec<SketchView>,
    pub body: Option<BodyInfo>,
    /// Cambia con cada recálculo: el visor vuelve a pedir la malla
    pub version: u64,
}

fn doc_hash(doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // El JSON es estable y cubre todo el documento
    serde_json::to_string(doc).unwrap_or_default().hash(&mut h);
    h.finish()
}

fn require_occt() -> Result<(), String> {
    if cad_model::occt::available() {
        Ok(())
    } else {
        Err("Esta compilación no incluye el núcleo CAD (OpenCASCADE)".into())
    }
}

/// Recalcula si el documento cambió desde la última vez.
fn evaluate(state: &AppState) -> Result<CadResult, String> {
    let doc = state.cad_document.lock().unwrap().clone().ok_or("No hay un diseño abierto")?;
    let hash = doc_hash(&doc);
    let mut cache = state.cad_cache.lock().unwrap();
    if cache.as_ref().is_none_or(|c| c.doc_hash != hash) {
        *cache = Some(CadCache { doc_hash: hash, eval: doc.evaluate() });
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
        })
    });
    Ok(CadResult { status: eval.status.clone(), sketches, body, version: hash })
}

#[tauri::command]
pub fn cad_status(state: tauri::State<'_, AppState>) -> CadStatus {
    CadStatus {
        available: cad_model::occt::available(),
        occt_version: cad_model::occt::occt_version(),
        has_document: state.cad_document.lock().unwrap().is_some(),
    }
}

/// Empieza un diseño vacío (reemplaza el actual).
#[tauri::command]
pub async fn cad_new(app: AppHandle) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, |state| {
        *state.cad_document.lock().unwrap() = Some(Document::new());
        evaluate(state)
    })
    .await
}

#[tauri::command]
pub fn cad_close(state: tauri::State<'_, AppState>) {
    *state.cad_document.lock().unwrap() = None;
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
    in_background(app, move |state| {
        *state.cad_document.lock().unwrap() = Some(document);
        evaluate(state)
    })
    .await
}

/// Estado del recálculo actual (sin cambiar nada).
#[tauri::command]
pub async fn cad_evaluate(app: AppHandle) -> Result<CadResult, String> {
    in_background(app, evaluate).await
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
    in_background(app, |state| {
        evaluate(state)?;
        let scale = 1.0 / mm_per_unit(state);
        let cache = state.cad_cache.lock().unwrap();
        let Some(body) = cache.as_ref().and_then(|c| c.eval.body.as_ref()) else {
            return Ok(Response::new(vec![0; 16]));
        };
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
        Ok(Response::new(out))
    })
    .await
}

/// Referencia estable a una cara del sólido actual (índice del teselado).
#[tauri::command]
pub async fn cad_face_ref(app: AppHandle, face: usize) -> Result<FaceRef, String> {
    in_background(app, move |state| {
        evaluate(state)?;
        let cache = state.cad_cache.lock().unwrap();
        cache.as_ref().and_then(|c| c.eval.face_ref(face)).ok_or_else(|| "Esa cara no existe".to_string())
    })
    .await
}

#[tauri::command]
pub async fn cad_edge_ref(app: AppHandle, edge: usize) -> Result<EdgeRef, String> {
    in_background(app, move |state| {
        evaluate(state)?;
        let cache = state.cad_cache.lock().unwrap();
        cache.as_ref().and_then(|c| c.eval.edge_ref(edge)).ok_or_else(|| "Esa arista no existe".to_string())
    })
    .await
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
    in_background(app, move |state| {
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
    })
    .await
}

fn body_scene(state: &AppState, name: &str) -> Result<Scene, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("El diseño todavía no tiene un sólido")?;
    // Más fino que el del visor: es lo que se imprime o se exporta
    let t = body.tessellate(0.01, 0.1).map_err(|e| e.to_string())?;
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
    let mut scene = Scene::new();
    scene.meshes.push(SceneMesh {
        name: name.to_string(),
        primitives: vec![Primitive {
            attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
            indices: Some(IndexData::U32(t.triangles.iter().flatten().copied().collect())),
            material: None,
        }],
    });
    scene.nodes.push(Node { name: name.to_string(), transform: Transform::identity(), mesh: Some(0), skin: None, children: Vec::new() });
    scene.root_nodes.push(0);
    scene.meters_per_unit = 0.001;
    Ok(scene)
}

/// Exporta el sólido: STEP (exacto) o malla (STL, 3MF, OBJ, PLY, GLB).
#[tauri::command]
pub async fn cad_export(app: AppHandle, path: String, format: String) -> Result<u64, String> {
    require_occt()?;
    in_background(app, move |state| {
        let p = std::path::Path::new(&path);
        match format.as_str() {
            "step" | "stp" => {
                evaluate(state)?;
                let cache = state.cad_cache.lock().unwrap();
                let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("El diseño todavía no tiene un sólido")?;
                let bytes = body.to_step().map_err(|e| e.to_string())?;
                std::fs::write(p, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
            }
            "stl" => converter_stl::export_stl(&body_scene(state, "Diseño")?, p).map_err(|e| format!("Error exportando STL: {e:?}"))?,
            "3mf" => converter_3mf::export_3mf(&body_scene(state, "Diseño")?, p).map_err(|e| format!("Error exportando 3MF: {e}"))?,
            "obj" => converter_obj::export_obj(&body_scene(state, "Diseño")?, p).map_err(|e| format!("Error exportando OBJ: {e:?}"))?,
            "ply" => converter_ply::export_ply(&body_scene(state, "Diseño")?, p).map_err(|e| format!("Error exportando PLY: {e}"))?,
            "glb" => converter_gltf_io::export_glb(&body_scene(state, "Diseño")?, p, &Default::default())
                .map_err(|e| format!("Error exportando GLB: {e}"))?,
            other => return Err(format!("Formato no soportado para el diseño: {other}")),
        }
        Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
    })
    .await
}

/// Agrega un STEP como operación (unir, restar o intersecar).
#[tauri::command]
pub async fn cad_import_step(app: AppHandle, path: String, op: Option<BodyOp>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| {
        let data = std::fs::read(&path).map_err(|e| format!("No se pudo leer {path}: {e}"))?;
        // Validar antes de meterlo al documento
        cad_model::Shape::from_step(&data).map_err(|e| e.to_string())?;
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
    let options = options.unwrap_or_default();
    in_background(app, move |state| {
        with_scan(state, |scan| match kind.as_str() {
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
    })
    .await
}

/// Detecta todas las zonas planas, cilíndricas y esféricas del modelo.
#[tauri::command]
pub async fn cad_scan_detect(app: AppHandle, options: Option<ScanPickOptions>) -> Result<Vec<Detection>, String> {
    let options = options.unwrap_or_default();
    in_background(app, move |state| {
        with_scan(state, |scan| {
            let d = DetectOptions::default();
            let opts = DetectOptions {
                angle_threshold: options.angle_threshold.unwrap_or(d.angle_threshold),
                tolerance: options.tolerance,
                ..d
            };
            Ok(cad_scan::detect_all(&scan.mesh, &opts))
        })
    })
    .await
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
    let options = options.unwrap_or_default();
    in_background(app, move |state| {
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
    })
    .await
}

/// mm por unidad del modelo (para mostrar medidas del escaneo en su escala).
#[tauri::command]
pub async fn cad_scan_mm_per_unit(app: AppHandle) -> Result<f64, String> {
    in_background(app, |state| with_scan(state, |scan| Ok(scan.mm_per_unit))).await
}

/// Errores de recálculo legibles (para avisos).
pub fn errors_text(result: &CadResult, doc: &Document) -> Vec<String> {
    let names: HashMap<_, _> = doc.features.iter().map(|f| (f.id, f.name.clone())).collect();
    result
        .status
        .iter()
        .filter_map(|s| match &s.state {
            FeatureState::Error { message } => Some(format!("{}: {message}", names.get(&s.id).cloned().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_doc() -> Document {
        let mut doc = Document::new();
        doc.add(FeatureKind::Primitive(cad_model::Primitive {
            shape: cad_model::PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 30.0 },
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
