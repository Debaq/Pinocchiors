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
use std::sync::Mutex;
use std::hash::{Hash, Hasher};

use cad_model::{
    AxisDef, BodyOp, Document, EdgeRef, FaceRef, FeatureKind, FeatureState, FeatureStatus, ImportFormat, Plane, PlaneDef,
    PlaneSpec, PointSpec, Region, Sketch, SolveReport, TabId, Tabs,
};
use cad_model::geom::P3;
use cad_scan::{CylinderPick, DetectOptions, Detection, OutlineOptions, PickOptions, PlanePick, ScanMesh, Section};
use converter_scene::{Animation, Channel as AnimChannel, IndexData, Interpolation, KeyframeValues, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
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
    /// Medidas ya calculadas por huella (con roscas, la masa tarda casi un
    /// segundo y se pedía en cada clic)
    pub measures: Vec<(u64, Measures)>,
}

/// Masa del sólido y de cada pieza, que solo dependen de la geometría.
#[derive(Clone)]
pub struct Measures {
    body: Option<BodyInfo>,
    parts: Vec<Option<cad_model::occt::MassInfo>>,
}

fn measures(eval: &cad_model::Evaluation) -> Measures {
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
    Measures { body, parts: eval.parts.iter().map(|p| p.shape.mass().ok()).collect() }
}

/// Malla del escaneo preparada para elegir zonas (cara a cara con el visor).
pub struct ScanCache {
    pub scene_hash: u64,
    pub mesh: ScanMesh,
    /// mm por unidad de la escena
    pub mm_per_unit: f64,
    /// Referencias ajustadas contra este escaneo (por el JSON de su `ScanFit`)
    pub fits: Mutex<HashMap<String, Option<cad_scan::ScanGeom>>>,
    /// Zonas libres ajustadas (borde, puntos de adentro) por el JSON de su semilla y ángulo
    pub zones: Mutex<HashMap<String, Option<(Vec<P3>, Vec<P3>)>>>,
    /// Caminos sobre la piel ya trazados (por el JSON de sus semillas)
    pub paths: Mutex<HashMap<String, Option<Vec<P3>>>>,
    /// Cuerpos del escaneo preparados (por holgura, espesor y triángulos)
    pub bodies: Mutex<HashMap<(u64, u64, usize), cad_model::ScanBodyMesh>>,
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
    /// Sketches 3D resueltos, en el orden del árbol
    pub sketches3d: Vec<Sketch3dView>,
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
    /// Ensamble resuelto (si el documento tiene)
    pub assembly: Option<AssemblyView>,
    /// Roscas (agujeros roscados, roscas, tornillos y tuercas): para
    /// coordinarlas entre sí
    pub threads: Vec<cad_model::ThreadAxis>,
    pub gears: Vec<cad_model::GearAxis>,
    /// Piezas que reemplazó un corte que las partió o una unión (se ven tachadas)
    pub replaced: Vec<ReplacedView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplacedView {
    pub feature: cad_model::FeatureId,
    /// Las que salieron, con el nombre que tenían
    pub from: Vec<PartLabel>,
    pub into: Vec<cad_model::PartId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartLabel {
    pub id: cad_model::PartId,
    pub name: String,
}

/// Número de cada pieza ("Pieza n"), contando también las reemplazadas, por
/// orden de creación: las que salen de un corte siguen después de las que ya
/// había y la tachada conserva el suyo.
fn part_numbers(doc: &Document, eval: &cad_model::Evaluation) -> HashMap<cad_model::PartId, usize> {
    let mut ids: Vec<cad_model::PartId> = eval.parts.iter().map(|p| p.id).collect();
    for r in &eval.replaced {
        for id in r.from.iter().chain(&r.into) {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }
    ids.sort_by_key(|id| (doc.index_of(id.feature), id.index));
    ids.into_iter().enumerate().map(|(n, id)| (id, n + 1)).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct Sketch3dView {
    pub id: cad_model::FeatureId,
    pub sketch: cad_model::Sketch3d,
    pub report: cad_model::Report3d,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssemblyView {
    pub solution: cad_model::AssemblySolution,
    /// Cambia con el ensamble o el diseño: el visor vuelve a pedir su malla
    pub version: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefView {
    pub id: cad_model::FeatureId,
    #[serde(flatten)]
    pub geom: cad_model::RefGeom,
    /// En las referencias por clics, qué se dedujo de lo elegido
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
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
    /// Superficie (sin volumen) en vez de sólido
    pub surface: bool,
    /// Salió de un corte o una unión: la pieza de la que hereda color,
    /// material y si está oculta (la más cercana que los tiene puestos)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherit: Option<cad_model::PartId>,
}

fn inherit_from(doc: &Document, eval: &cad_model::Evaluation, id: cad_model::PartId) -> Option<cad_model::PartId> {
    let mut cur = id;
    for _ in 0..64 {
        if cur != id && doc.parts.iter().any(|p| p.part == cur) {
            return Some(cur);
        }
        cur = *eval.replaced.iter().find(|r| r.into.contains(&cur))?.from.first()?;
    }
    None
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

/// Nombre de cada pieza: el que le puso el usuario, el de catálogo o "Pieza n".
fn part_name(doc: &Document, id: cad_model::PartId, n: usize) -> String {
    // Sin nombre puesto: las piezas estándar se llaman por su catálogo
    let standard = || match doc.get(id.feature).map(|f| &f.kind) {
        Some(FeatureKind::Primitive(p)) if id.index == 0 => cad_model::standard::describe(&p.shape),
        _ => None,
    };
    doc.parts.iter().find(|p| p.part == id).and_then(|p| p.name.clone()).or_else(standard).unwrap_or_else(|| format!("Pieza {n}"))
}

fn doc_hash(doc: &Document) -> u64 {
    doc.geometry_hash()
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
        None => state.cad_doc().ok_or("No hay un diseño abierto")?,
    };
    evaluate_doc(state, &doc)
}

/// Las referencias tomadas del escaneo, ajustadas contra el escaneo de hoy,
/// y los cuerpos del escaneo con su malla preparada (`None` si el diseño no
/// usa el escaneo o no hay: se usa lo guardado). Así siguen al escaneo si se
/// alinea, se deshace o se repara.
fn fit_scan_references(state: &AppState, doc: &Document) -> Option<Document> {
    if !doc.features.iter().any(|f| uses_scan(&f.kind)) {
        return None;
    }
    let mut out = doc.clone();
    let done = with_scan(state, |scan| {
        let mut fits = scan.fits.lock().unwrap();
        let mut geom = |fit: &cad_model::ScanFit| {
            let key = serde_json::to_string(fit).unwrap_or_default();
            fits.entry(key).or_insert_with(|| cad_scan::fit_reference(&scan.mesh, fit, &PickOptions::default()).ok()).clone()
        };
        for f in &mut out.features {
            match &mut f.kind {
                FeatureKind::Plane { def: PlaneDef::Scan { fit, plane } } => {
                    if let Some(cad_scan::ScanGeom::Plane(p)) = geom(fit) {
                        *plane = p;
                    }
                }
                FeatureKind::Axis { def: AxisDef::Scan { fit, origin, direction } } => {
                    if let Some(cad_scan::ScanGeom::Axis { origin: o, direction: d }) = geom(fit) {
                        (*origin, *direction) = (o, d);
                    }
                }
                FeatureKind::Point { def: PointSpec::Scan { fit, point } } => {
                    if let Some(cad_scan::ScanGeom::Point(p)) = geom(fit) {
                        *point = p;
                    }
                }
                // Solo lo que no está suprimido ni vuelto atrás (prepararlo tarda)
                FeatureKind::ScanBody(b) if !f.suppressed => {
                    // Los toques del pincel siguen al escaneo como las referencias
                    if let Some(p) = &mut b.paint {
                        p.dabs = std::mem::take(&mut p.dabs).into_iter().filter_map(|d| scan.mesh.fit_dab(d)).collect();
                    }
                    b.mesh = Some(scan_body_mesh(scan, b.offset, b.thickness, b.faces));
                }
                FeatureKind::ScanSurface(sf) if !f.suppressed => {
                    let key = serde_json::to_string(&(&sf.seed, sf.angle)).unwrap_or_default();
                    let zone = scan
                        .zones
                        .lock()
                        .unwrap()
                        .entry(key)
                        .or_insert_with(|| {
                            let (t, _) = scan.mesh.locate(&sf.seed)?;
                            let mut z = scan.mesh.free_zone(t, sf.angle, 150)?;
                            z.fit_surface(&scan.mesh)?;
                            Some((z.boundary, z.interior))
                        })
                        .clone();
                    if let Some((b, i)) = zone {
                        (sf.boundary, sf.interior) = (b, i);
                    }
                    if let Some(seed) = &sf.close
                        && let Some((t, _)) = scan.mesh.locate(seed)
                        && let Ok(p) = cad_scan::pick_plane(&scan.mesh, t, &PickOptions::default())
                    {
                        sf.close_plane = Some(p.plane);
                    }
                }
                FeatureKind::Curve { def: cad_model::CurveDef::ScanPath { seeds, closed, points } } => {
                    let key = serde_json::to_string(&(&*seeds, *closed)).unwrap_or_default();
                    let traced = scan
                        .paths
                        .lock()
                        .unwrap()
                        .entry(key)
                        .or_insert_with(|| {
                            let stops = seeds.iter().map(|s| scan.mesh.locate(s)).collect::<Option<Vec<_>>>()?;
                            scan.mesh.geodesic(&stops, *closed)
                        })
                        .clone();
                    if let Some(p) = traced {
                        *points = p;
                    }
                }
                FeatureKind::Curve { def: cad_model::CurveDef::ScanProjected { offset, mesh, .. } } if !f.suppressed => {
                    *mesh = Some(scan_body_mesh(scan, *offset, 0.0, SCAN_FACES));
                }
                FeatureKind::Extrude(cad_model::Extrude { extent: cad_model::Extent::UpToScan { offset, mesh }, .. }) if !f.suppressed => {
                    *mesh = Some(scan_body_mesh(scan, *offset, 0.0, SCAN_FACES));
                }
                FeatureKind::FaceToScan { offset, mesh, .. } if !f.suppressed => *mesh = Some(scan_body_mesh(scan, *offset, 0.0, SCAN_FACES)),
                FeatureKind::Rib { scan: Some(r), .. } if !f.suppressed => r.mesh = Some(scan_body_mesh(scan, r.offset, 0.0, SCAN_FACES)),
                _ => {}
            }
        }
        Ok(())
    });
    done.ok().map(|_| out)
}

/// El documento de la pestaña `tab` (`None`: la activa) listo para recalcular,
/// o `None` si va tal cual: las referencias del escaneo al día y las piezas
/// traídas de otras pestañas (calculadas con la caché de operaciones).
fn prepared(state: &AppState, tab: Option<TabId>, doc: &Document) -> Option<Document> {
    let fitted = fit_scan_references(state, doc);
    if !Tabs::has_derived(doc) {
        return fitted;
    }
    let mut out = fitted.unwrap_or_else(|| doc.clone());
    let tabs = state.cad_tabs.lock().unwrap();
    let tab = tab.unwrap_or(tabs.active);
    tabs.fill_derived(tab, &mut out, &mut |d| {
        let fitted = fit_scan_references(state, d);
        fitted.as_ref().unwrap_or(d).evaluate_with(&mut state.cad_ops.lock().unwrap())
    });
    Some(out)
}

fn uses_scan(kind: &FeatureKind) -> bool {
    matches!(
        kind,
        FeatureKind::Plane { def: PlaneDef::Scan { .. } }
            | FeatureKind::Axis { def: AxisDef::Scan { .. } }
            | FeatureKind::Point { def: PointSpec::Scan { .. } }
            | FeatureKind::ScanBody(_)
            | FeatureKind::Extrude(cad_model::Extrude { extent: cad_model::Extent::UpToScan { .. }, .. })
            | FeatureKind::FaceToScan { .. }
            | FeatureKind::Rib { scan: Some(_), .. }
            | FeatureKind::Curve { def: cad_model::CurveDef::ScanPath { .. } | cad_model::CurveDef::ScanProjected { .. } }
            | FeatureKind::ScanSurface(_)
    )
}

/// Triángulos del escaneo cuando lo usa otra operación (extruir hasta él)
const SCAN_FACES: usize = 20_000;

/// El escaneo cerrado con la holgura (o la carcasa de ese espesor), listo para
/// sólido (en caché por escaneo, holgura, espesor y triángulos). Si no se
/// puede, la malla queda vacía con el motivo.
fn scan_body_mesh(scan: &ScanCache, offset: f64, thickness: f64, faces: usize) -> cad_model::ScanBodyMesh {
    use std::hash::{Hash, Hasher};
    let faces = faces.clamp(cad_scan::MIN_FACES, cad_scan::MAX_FACES);
    let key = (offset.to_bits(), thickness.to_bits(), faces);
    let mut bodies = scan.bodies.lock().unwrap();
    // Cada una pesa ~1 MB cada 20 000 triángulos: no guardar cada holgura probada
    if bodies.len() >= 6 && !bodies.contains_key(&key) {
        bodies.clear();
    }
    bodies
        .entry(key)
        .or_insert_with(|| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (scan.scene_hash, key).hash(&mut h);
            let options = cad_scan::BodyOptions { faces, offset, thickness };
            match cad_scan::prepare_body(&scan.mesh.vertices, &scan.mesh.triangles, &options) {
                Ok(b) => cad_model::ScanBodyMesh { hash: h.finish(), data: std::sync::Arc::new((b.vertices, b.triangles)), error: None },
                Err(e) => {
                    e.hash(&mut h);
                    cad_model::ScanBodyMesh { hash: h.finish(), data: Default::default(), error: Some(e) }
                }
            }
        })
        .clone()
}

/// Recalcula el documento guardado, sin el borrador (exportar, pasar a modelo).
fn evaluate_committed(state: &AppState) -> Result<CadResult, String> {
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    evaluate_doc(state, &doc)
}

/// Las pestañas de diseño (con las referencias del escaneo al día), para un ensamble
fn design_tabs(state: &AppState) -> Vec<(TabId, Document)> {
    let designs: Vec<(TabId, Document)> = state
        .cad_tabs
        .lock()
        .unwrap()
        .tabs
        .iter()
        .filter_map(|t| match &t.kind {
            cad_model::TabKind::Design { document } => Some((t.id, (**document).clone())),
            cad_model::TabKind::Assembly { .. } => None,
        })
        .collect();
    designs.into_iter().map(|(id, d)| (id, prepared(state, Some(id), &d).unwrap_or(d))).collect()
}

/// Huella de todos los diseños (lo que cambia el recálculo de un ensamble)
fn designs_hash(designs: &[(TabId, Document)]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (id, d) in designs {
        (id, doc_hash(d)).hash(&mut h);
    }
    h.finish()
}

/// Nombre de cada pieza del recálculo juntado de un ensamble, como se llama
/// en su pestaña (numeradas en su diseño, no entre todas)
fn assembly_part_names(designs: &[(TabId, Document)], merged: &cad_model::Evaluation) -> Vec<(cad_model::PartId, String)> {
    let mut out = Vec::new();
    for (tab, doc) in designs {
        let mut ids: Vec<cad_model::PartId> = merged
            .parts
            .iter()
            .map(|p| p.id)
            .filter(|id| cad_model::split_tab_feature(id.feature).0 == *tab)
            .map(|id| cad_model::PartId { feature: cad_model::split_tab_feature(id.feature).1, index: id.index })
            .collect();
        ids.sort_by_key(|id| (doc.index_of(id.feature), id.index));
        for (n, id) in ids.into_iter().enumerate() {
            out.push((cad_model::PartId { feature: cad_model::tab_feature(*tab, id.feature), index: id.index }, part_name(doc, id, n + 1)));
        }
    }
    out
}

fn evaluate_doc(state: &AppState, doc: &Document) -> Result<CadResult, String> {
    // Pestaña de ensamble: el recálculo es el de todos los diseños juntos
    let designs = state.cad_in_assembly().then(|| design_tabs(state));
    let fitted;
    let doc = match designs.is_none().then(|| prepared(state, None, doc)).flatten() {
        Some(d) => {
            fitted = d;
            &fitted
        }
        None => doc,
    };
    let hash = designs.as_deref().map_or_else(|| doc_hash(doc), designs_hash);
    let mut cache = state.cad_cache.lock().unwrap();
    match cache.take() {
        Some(c) if c.doc_hash == hash => *cache = Some(c),
        Some(CadCache { doc_hash, eval, previous: Some((h, prev)), measures }) if h == hash => {
            *cache = Some(CadCache { doc_hash: h, eval: prev, previous: Some((doc_hash, eval)), measures });
        }
        old => {
            let measures = old.as_ref().map(|c| c.measures.clone()).unwrap_or_default();
            let previous = old.map(|c| (c.doc_hash, c.eval));
            let eval = match &designs {
                Some(designs) => {
                    let mut ops = state.cad_ops.lock().unwrap();
                    let evals: Vec<(TabId, cad_model::Evaluation)> = designs.iter().map(|(id, d)| (*id, d.evaluate_with(&mut ops))).collect();
                    cad_model::merge_evaluations(evals.iter().map(|(id, e)| (*id, e)))
                }
                None => doc.evaluate_with(&mut state.cad_ops.lock().unwrap()),
            };
            *cache = Some(CadCache { doc_hash: hash, eval, previous, measures });
        }
    }
    let c = cache.as_mut().unwrap();
    let m = match c.measures.iter().find(|(h, _)| *h == hash) {
        Some((_, m)) => m.clone(),
        None => {
            let m = measures(&c.eval);
            // Las dos últimas (documento y borrador)
            c.measures.insert(0, (hash, m.clone()));
            c.measures.truncate(2);
            m
        }
    };
    let eval = &c.eval;
    // Ensamble: las piezas con el nombre que tienen en su pestaña
    let named;
    let doc = match &designs {
        Some(designs) => {
            let tabs = state.cad_tabs.lock().unwrap();
            let mut parts = tabs.flat_part_props(&assembly_part_names(designs, eval)).0;
            // El mismo nombre en dos pestañas: se aclara de cuál es
            let names: Vec<Option<String>> = parts.iter().map(|p| p.name.clone()).collect();
            for p in &mut parts {
                let Some(name) = &p.name else { continue };
                if names.iter().filter(|n| n.as_ref() == Some(name)).count() > 1
                    && let Some(tab) = tabs.get(cad_model::split_tab_feature(p.part.feature).0)
                {
                    p.name = Some(format!("{name} ({})", tab.name));
                }
            }
            let mut d = doc.clone();
            d.parts = parts;
            named = d;
            &named
        }
        None => doc,
    };
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
    let mut sketches3d: Vec<Sketch3dView> = eval.sketches3d.iter().map(|(id, r)| Sketch3dView { id: *id, sketch: r.sketch.clone(), report: r.report.clone() }).collect();
    sketches3d.sort_by_key(|s| doc.index_of(s.id));
    let body = m.body;
    let parts = part_views(doc, eval, &m.parts);
    let numbers = part_numbers(doc, eval);
    let replaced = eval
        .replaced
        .iter()
        .map(|r| ReplacedView {
            feature: r.feature,
            from: r.from.iter().map(|&id| PartLabel { id, name: part_name(doc, id, numbers.get(&id).copied().unwrap_or(0)) }).collect(),
            into: r.into.clone(),
        })
        .collect();
    let assembly = doc.assembly.as_ref().map(|a| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        hash.hash(&mut h);
        serde_json::to_string(a).unwrap_or_default().hash(&mut h);
        AssemblyView { solution: cad_model::assembly::solve(a), version: h.finish() }
    });
    let mut references: Vec<RefView> = eval.references.iter().map(|(id, g)| RefView { id: *id, geom: g.clone(), how: eval.ref_kinds.get(id).cloned() }).collect();
    references.sort_by_key(|r| doc.index_of(r.id));
    Ok(CadResult {
        parts,
        replaced,
        references,
        assembly,
        status: eval.status.clone(),
        sketches,
        sketches3d,
        body,
        parameters: eval.parameters.clone(),
        bindings: eval.bindings.clone(),
        version: hash,
        recomputed: eval.recomputed,
        threads: eval.threads.clone(),
        gears: eval.gears.clone(),
    })
}

fn status_impl(state: &AppState) -> CadStatus {
    CadStatus {
        available: cad_model::occt::available(),
        occt_version: cad_model::occt::occt_version(),
        has_document: !state.cad_tabs.lock().unwrap().is_empty(),
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
    state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(Document::new())));
    evaluate(state)
}

#[tauri::command]
pub fn cad_close(state: tauri::State<'_, AppState>) {
    state.set_cad_tabs(cad_model::Tabs::default());
}

#[tauri::command]
pub fn cad_get_document(state: tauri::State<'_, AppState>) -> Option<Document> {
    state.cad_doc()
}

/// Reemplaza el documento y recalcula.
#[tauri::command]
pub async fn cad_set_document(app: AppHandle, document: Document) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| set_document_impl(state, document)).await
}

fn set_document_impl(state: &AppState, document: Document) -> Result<CadResult, String> {
    if state.cad_in_assembly() {
        // De un ensamble solo se guardan su ensamble y sus planos (lo demás sale de los diseños)
        let mut tabs = state.cad_tabs.lock().unwrap();
        let active = tabs.active;
        tabs.set_flat_assembly(active, document.assembly.unwrap_or_default())?;
        tabs.set_assembly_drawing(active, document.drawing)?;
    } else {
        state.with_cad_doc(|d| *d = document)?;
    }
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

// ─── Pestañas ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TabView {
    pub id: TabId,
    pub name: String,
    /// "design" o "assembly"
    pub kind: &'static str,
    /// Ensambles y diseños que usan piezas de esta pestaña
    pub used_by: Vec<TabId>,
}

/// Las pestañas en el orden de la barra y la activa (`None`: no hay ninguna)
#[derive(Serialize)]
pub struct TabsView {
    pub tabs: Vec<TabView>,
    pub active: Option<TabId>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum NewTab {
    Design,
    Assembly,
}

fn tabs_view(tabs: &Tabs) -> TabsView {
    TabsView {
        tabs: tabs
            .tabs
            .iter()
            .map(|t| TabView { id: t.id, name: t.name.clone(), kind: if t.kind.is_design() { "design" } else { "assembly" }, used_by: tabs.users(t.id) })
            .collect(),
        active: tabs.active_tab().map(|t| t.id),
    }
}

/// Cambia las pestañas con `f`. Si cambió la activa, el recálculo de la que
/// se deja se guarda aparte y vuelve el de la nueva (si lo había); el borrador
/// se descarta. Los recálculos de pestañas borradas se sueltan.
fn change_tabs(state: &AppState, f: impl FnOnce(&mut Tabs) -> Result<(), String>) -> Result<TabsView, String> {
    let (before, after, alive, view) = {
        let mut tabs = state.cad_tabs.lock().unwrap();
        let before = tabs.active_tab().map(|t| t.id);
        f(&mut tabs)?;
        let alive: Vec<TabId> = tabs.tabs.iter().map(|t| t.id).collect();
        (before, tabs.active_tab().map(|t| t.id), alive, tabs_view(&tabs))
    };
    let mut caches = state.cad_tab_caches.lock().unwrap();
    caches.retain(|id, _| alive.contains(id));
    if before != after {
        *state.cad_preview.lock().unwrap() = None;
        let mut cache = state.cad_cache.lock().unwrap();
        let left = cache.take();
        if let (Some(b), Some(c)) = (before, left)
            && alive.contains(&b)
        {
            caches.insert(b, c);
        }
        *cache = after.and_then(|a| caches.remove(&a));
    }
    Ok(view)
}

#[tauri::command]
pub fn cad_tabs(state: tauri::State<'_, AppState>) -> TabsView {
    tabs_view(&state.cad_tabs.lock().unwrap())
}

/// El documento de una pestaña de diseño cualquiera (sin activarla)
#[tauri::command]
pub fn cad_tab_document(state: tauri::State<'_, AppState>, id: TabId) -> Option<Document> {
    state.cad_tabs.lock().unwrap().design(id).cloned()
}

/// Las piezas de una pestaña de diseño con el nombre que tienen en ella (para traerlas a otra)
#[tauri::command]
pub async fn cad_tab_parts(app: AppHandle, id: TabId) -> Result<Vec<PartLabel>, String> {
    require_occt()?;
    in_background(app, move |state| tab_parts_impl(state, id)).await
}

fn tab_parts_impl(state: &AppState, id: TabId) -> Result<Vec<PartLabel>, String> {
    let doc = state.cad_tabs.lock().unwrap().design(id).cloned().ok_or("Esa pestaña ya no está o no es un diseño")?;
    let doc = prepared(state, Some(id), &doc).unwrap_or(doc);
    let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
    let numbers = part_numbers(&doc, &eval);
    Ok(eval.parts.iter().map(|p| PartLabel { id: p.id, name: part_name(&doc, p.id, numbers.get(&p.id).copied().unwrap_or(0)) }).collect())
}

/// Pestaña nueva al final; queda activa.
#[tauri::command]
pub fn cad_tab_add(state: tauri::State<'_, AppState>, kind: NewTab, name: Option<String>) -> Result<TabsView, String> {
    tab_add_impl(&state, kind, name)
}

fn tab_add_impl(state: &AppState, kind: NewTab, name: Option<String>) -> Result<TabsView, String> {
    change_tabs(state, |t| {
        match kind {
            NewTab::Design => t.add_design(name),
            NewTab::Assembly => t.add_assembly(name),
        };
        Ok(())
    })
}

#[tauri::command]
pub fn cad_tab_rename(state: tauri::State<'_, AppState>, id: TabId, name: String) -> Result<TabsView, String> {
    change_tabs(&state, |t| t.rename(id, &name))
}

/// Copia al lado de la original; queda activa.
#[tauri::command]
pub fn cad_tab_duplicate(state: tauri::State<'_, AppState>, id: TabId) -> Result<TabsView, String> {
    change_tabs(&state, |t| t.duplicate(id).map(|_| ()))
}

#[tauri::command]
pub fn cad_tab_delete(state: tauri::State<'_, AppState>, id: TabId) -> Result<TabsView, String> {
    change_tabs(&state, |t| t.remove(id).map(|_| ()))
}

#[tauri::command]
pub fn cad_tab_move(state: tauri::State<'_, AppState>, id: TabId, to: usize) -> Result<TabsView, String> {
    change_tabs(&state, |t| t.move_to(id, to))
}

#[tauri::command]
pub fn cad_tab_activate(state: tauri::State<'_, AppState>, id: TabId) -> Result<TabsView, String> {
    change_tabs(&state, |t| t.activate(id))
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

/// Círculos, avisos y error de medidas de un engrane (para el diálogo).
#[tauri::command]
pub fn cad_gear_info(part: cad_model::GearPart) -> cad_model::gear_part::GearInfo {
    cad_model::gear_part::info(&part)
}

/// Dientes de un engrane para engranar con `source` con una relación o una distancia entre centros.
#[tauri::command]
pub fn cad_gear_design(source: cad_model::GearPart, kind: cad_model::gears::GearKind, ratio: Option<f64>, distance: Option<f64>) -> Result<u32, String> {
    cad_model::gear_part::design_teeth(&source, kind, ratio, distance)
}

/// Formas de diente (evolvente, cicloidal, correas y cadenas) con su paso y ancho por defecto.
#[tauri::command]
pub fn cad_tooth_forms() -> Vec<cad_model::belts::FormInfo> {
    cad_model::belts::forms()
}

/// Dientes de correa (o eslabones de cadena) para dos ruedas a una distancia entre centros, y los centros exactos.
#[tauri::command]
pub fn cad_drive_length(form: cad_model::gears::ToothForm, teeth_a: u32, teeth_b: u32, center: f64) -> Result<cad_model::belts::DriveLength, String> {
    if form.is_chain() {
        cad_model::belts::chain_length(form, teeth_a, teeth_b, center)
    } else {
        cad_model::belts::belt_length(form, teeth_a, teeth_b, center)
    }
}

/// Plan de un planetario: dientes del satélite y de la corona, ángulos y relaciones, con la condición de montaje.
#[tauri::command]
pub fn cad_planetary_plan(module: f64, sun_teeth: u32, planet_teeth: Option<u32>, ring_teeth: Option<u32>, planets: u32, pressure_angle: f64) -> Result<cad_model::gear_part::PlanetaryPlan, String> {
    cad_model::gear_part::planetary_plan(module, sun_teeth, planet_teeth, ring_teeth, planets, pressure_angle)
}

/// Materiales con sus límites de fatiga para la resistencia de los engranes.
#[tauri::command]
pub fn cad_gear_materials() -> Vec<cad_model::gear_strength::Material> {
    cad_model::gear_strength::MATERIALS.to_vec()
}

/// Resistencia del par `first` (con el par pedido) y `second` (el que engrana con él): flexión del pie y contacto.
#[tauri::command]
pub fn cad_gear_strength(first: cad_model::GearPart, second: cad_model::GearPart, input: cad_model::gear_strength::StrengthInput) -> Result<cad_model::gear_strength::StrengthResult, String> {
    cad_model::gear_strength::analyze(&first, &second, &input)
}

/// El perfil de un engrane como bloque del sketch (origen en su centro).
#[tauri::command]
pub fn cad_gear_block(part: cad_model::GearPart) -> Result<cad_model::sketch::SketchBlock, String> {
    cad_model::gear_part::sketch_block(&part)
}

/// Restricciones que le faltan al sketch (casi cumplidas o cotas de tamaño).
#[tauri::command]
pub fn cad_sketch_suggest(sketch: Sketch) -> Result<Vec<cad_model::Suggestion>, String> {
    sketch.suggest().map_err(|e| e.to_string())
}

/// Lo que hay que agregar para definir el sketch entero (con las relaciones
/// casi cumplidas si `relations`).
#[tauri::command]
pub fn cad_sketch_define(sketch: Sketch, relations: bool) -> Result<Vec<cad_model::SketchConstraint>, String> {
    sketch.auto_define(relations).map_err(|e| e.to_string())
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

/// Desviación del escaneo (el modelo cargado) respecto del sólido guardado:
/// f32 en binario; primero el resumen (triángulos, media, media absoluta, rms,
/// p95, máximo, fracción dentro de la tolerancia, tolerancia) y después un
/// valor por triángulo del escaneo (mm, + afuera del sólido).
#[tauri::command]
pub async fn cad_deviation(app: AppHandle, tolerance: f64) -> Result<Response, String> {
    in_background(app, move |state| deviation_impl(state, tolerance).map(Response::new)).await
}

fn deviation_impl(state: &AppState, tolerance: f64) -> Result<Vec<u8>, String> {
    require_occt()?;
    evaluate_committed(state)?;
    let (positions, normals, triangles) = {
        let cache = state.cad_cache.lock().unwrap();
        let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("El diseño todavía no tiene un sólido")?;
        let m = body.mass().map_err(|e| e.to_string())?;
        let size = (0..3).map(|k| m.bbox_max[k] - m.bbox_min[k]).fold(0.0, f64::max).max(1.0);
        let t = body.tessellate(size * 2e-4, 0.1).map_err(|e| e.to_string())?;
        (t.positions, t.normals, t.triangles)
    };
    with_scan(state, |scan| {
        let d = cad_scan::deviation(&scan.mesh, &positions, &normals, &triangles, tolerance);
        let s = &d.stats;
        let mut out: Vec<f32> = vec![d.per_face.len() as f32, s.mean as f32, s.mean_abs as f32, s.rms as f32, s.p95 as f32, s.max_abs as f32, s.within as f32, s.tolerance as f32];
        out.extend(d.per_face.iter().map(|v| *v as f32));
        Ok(out.iter().flat_map(|v| v.to_le_bytes()).collect())
    })
}

/// Instancias del ensamble en su lugar, con el ensamble y su solución.
fn assembly_shapes(state: &AppState) -> Result<(cad_model::Assembly, Vec<(u32, cad_model::Shape)>), String> {
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let asm = doc.assembly.clone().ok_or("El diseño no tiene ensamble")?;
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let sol = cad_model::assembly::solve(&asm);
    let shapes = cad_model::assembly::instance_shapes(ev, &asm, &sol);
    Ok((asm, shapes))
}

/// Malla del ensamble (como `cad_mesh`, con las instancias en orden: sus caras
/// van seguidas, ver `AssemblyMeshInfo`).
#[tauri::command]
pub async fn cad_assembly_mesh(app: AppHandle, explode: Option<f64>) -> Result<Response, String> {
    in_background(app, move |state| assembly_mesh_impl(state, explode.unwrap_or(0.0)).map(Response::new)).await
}

/// Vista explosionada: cada instancia se corre desde el centro del conjunto en la dirección de su
/// propio centro, `explode` veces la distancia que los separa (solo para mirar).
fn explode_shapes(shapes: Vec<(u32, cad_model::Shape)>, explode: f64) -> Result<Vec<(u32, cad_model::Shape)>, String> {
    if explode <= 0.0 || shapes.len() < 2 {
        return Ok(shapes);
    }
    let centers: Vec<[f64; 3]> = shapes
        .iter()
        .map(|(_, s)| s.bounds().map(|(lo, hi)| [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let n = centers.len() as f64;
    let mid = [0, 1, 2].map(|k| centers.iter().map(|c| c[k]).sum::<f64>() / n);
    shapes
        .into_iter()
        .zip(centers)
        .map(|((id, s), c)| {
            let v = [0, 1, 2].map(|k| (c[k] - mid[k]) * explode);
            s.translate(v).map(|t| (id, t)).map_err(|e| e.to_string())
        })
        .collect()
}

fn assembly_mesh_impl(state: &AppState, explode: f64) -> Result<Vec<u8>, String> {
    let (_, shapes) = assembly_shapes(state)?;
    let shapes = explode_shapes(shapes, explode)?;
    let scale = 1.0 / mm_per_unit(state);
    if shapes.is_empty() {
        return Ok(vec![0; 16]);
    }
    let list: Vec<cad_model::Shape> = shapes.iter().map(|(_, s)| s.clone()).collect();
    let all = if list.len() == 1 { list[0].clone() } else { cad_model::Shape::compound(&list).map_err(|e| e.to_string())? };
    encode_view_mesh(&all, scale)
}

/// Caras de cada instancia dentro de la malla del ensamble: [id, desde, hasta).
#[tauri::command]
pub async fn cad_assembly_faces(app: AppHandle) -> Result<Vec<[u32; 3]>, String> {
    in_background(app, assembly_faces_impl).await
}

fn assembly_faces_impl(state: &AppState) -> Result<Vec<[u32; 3]>, String> {
    let (_, shapes) = assembly_shapes(state)?;
    let mut start = 0u32;
    Ok(shapes
        .iter()
        .map(|(id, s)| {
            let n = s.face_count() as u32;
            let r = [*id, start, start + n];
            start += n;
            r
        })
        .collect())
}

/// Conector sobre la cara `face` de la malla del ensamble, en coordenadas de
/// su pieza: centro y normal de una cara plana, o un punto del eje y el eje de
/// una cilíndrica o cónica.
#[tauri::command]
pub async fn cad_assembly_connector(app: AppHandle, face: usize) -> Result<cad_model::Connector, String> {
    in_background(app, move |state| assembly_connector_impl(state, face)).await
}

fn assembly_connector_impl(state: &AppState, face: usize) -> Result<cad_model::Connector, String> {
    let ranges = assembly_faces_impl(state)?;
    let [id, start, _] = *ranges.iter().find(|r| (r[1] as usize..r[2] as usize).contains(&face)).ok_or("Esa cara no es de ninguna instancia")?;
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let asm = doc.assembly.ok_or("El diseño no tiene ensamble")?;
    let inst = asm.instances.iter().find(|i| i.id == id).ok_or("La instancia ya no existe")?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let part = ev.parts.iter().find(|p| p.id == inst.part).ok_or("La pieza ya no existe")?;
    let info = part.shape.face_info(face - start as usize).map_err(|e| e.to_string())?;
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (origin, z) = match info.axis {
        Some(ax) if info.surface != cad_model::occt::SurfaceKind::Plane => {
            // El punto del eje a la altura del centro de la cara
            let d = ax.dir;
            let t = dot([info.center[0] - ax.origin[0], info.center[1] - ax.origin[1], info.center[2] - ax.origin[2]], d);
            ([ax.origin[0] + d[0] * t, ax.origin[1] + d[1] * t, ax.origin[2] + d[2] * t], d)
        }
        _ => (info.center, info.normal),
    };
    // X: cualquier perpendicular a Z (la de X del mundo si se puede)
    let helper = if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let k = dot(helper, z);
    let x = [helper[0] - z[0] * k, helper[1] - z[1] * k, helper[2] - z[2] * k];
    let l = dot(x, x).sqrt();
    Ok(cad_model::Connector { instance: id, origin, z, x: [x[0] / l, x[1] / l, x[2] / l] })
}

/// Pares de instancias que se pisan y cuánto volumen comparten (mm³).
#[tauri::command]
pub async fn cad_assembly_interference(app: AppHandle) -> Result<Vec<(u32, u32, f64)>, String> {
    in_background(app, assembly_interference_impl).await
}

fn assembly_interference_impl(state: &AppState) -> Result<Vec<(u32, u32, f64)>, String> {
    let (asm, shapes) = assembly_shapes(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    Ok(cad_model::assembly::interferences_for(ev, &asm, &shapes))
}

/// Pares de instancias que quedan más cerca que la holgura de la impresora (mm). Sin `min_gap`
/// toma el ajuste normal de la impresora del diseño.
#[tauri::command]
pub async fn cad_assembly_clearance(app: AppHandle, min_gap: Option<f64>) -> Result<Vec<(u32, u32, f64)>, String> {
    in_background(app, move |state| assembly_clearance_impl(state, min_gap)).await
}

fn assembly_clearance_impl(state: &AppState, min_gap: Option<f64>) -> Result<Vec<(u32, u32, f64)>, String> {
    let gap = match min_gap {
        Some(g) => g,
        None => {
            let doc = state.cad_doc();
            let fab = doc.as_ref().and_then(|d| d.fabrication.as_ref()).ok_or("Elige una impresora en Fabricar o en Diseñar para saber qué holgura pedir")?;
            fab.tolerances().fit_clearance(fab.fit)
        }
    };
    let (asm, shapes) = assembly_shapes(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    Ok(cad_model::assembly::tight_gaps(ev, &asm, &shapes, gap))
}

/// Acoplamientos que piden los engranes enlazados que están en el ensamble.
#[tauri::command]
pub async fn cad_assembly_suggest(app: AppHandle) -> Result<Vec<cad_model::Relation>, String> {
    in_background(app, assembly_suggest_impl).await
}

fn assembly_suggest_impl(state: &AppState) -> Result<Vec<cad_model::Relation>, String> {
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let asm = doc.assembly.ok_or("El diseño no tiene ensamble")?;
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let sol = cad_model::assembly::solve(&asm);
    Ok(cad_model::assembly::suggest_gear_relations(ev, &asm, &sol))
}

/// Avisos de los engranes enlazados del ensamble (ejes no paralelos, helicoidales cruzados).
#[tauri::command]
pub async fn cad_assembly_gear_warnings(app: AppHandle) -> Result<Vec<String>, String> {
    in_background(app, assembly_gear_warnings_impl).await
}

fn assembly_gear_warnings_impl(state: &AppState) -> Result<Vec<String>, String> {
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let asm = doc.assembly.ok_or("El diseño no tiene ensamble")?;
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let sol = cad_model::assembly::solve(&asm);
    Ok(cad_model::assembly::gear_axis_warnings(ev, &asm, &sol))
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

/// Diferencias entre el diseño guardado y otra versión: lo que se agregó
/// (está ahora y no estaba) y lo que se quitó (estaba y ya no está). Bytes:
/// volumen agregado y quitado (f64), largo de la primera malla (u32) y las dos
/// mallas en el formato de [`cad_mesh`].
#[tauri::command]
pub async fn cad_compare(app: AppHandle, document: Document) -> Result<Response, String> {
    require_occt()?;
    in_background(app, move |state| compare_impl(state, &document).map(Response::new)).await
}

fn compare_impl(state: &AppState, other: &Document) -> Result<Vec<u8>, String> {
    evaluate_committed(state)?;
    let now = state.cad_cache.lock().unwrap().as_ref().and_then(|c| c.eval.body.clone());
    let other = prepared(state, None, other).unwrap_or_else(|| other.clone());
    let then = other.evaluate_with(&mut state.cad_ops.lock().unwrap()).body;
    let scale = 1.0 / mm_per_unit(state);
    let diff = |a: &Option<cad_model::Shape>, b: &Option<cad_model::Shape>| -> Result<Option<cad_model::Shape>, String> {
        match (a, b) {
            (None, _) => Ok(None),
            (Some(a), None) => Ok(Some(a.clone())),
            (Some(a), Some(b)) => a.cut(b).map(Some).map_err(|e| e.to_string()),
        }
    };
    let volume = |s: &Option<cad_model::Shape>| s.as_ref().and_then(|s| s.mass().ok()).map_or(0.0, |m| m.volume);
    let mesh = |s: &Option<cad_model::Shape>| match s {
        Some(s) if s.mass().is_ok_and(|m| m.volume > 1e-9) => encode_view_mesh(s, scale),
        _ => Ok(vec![0; 16]),
    };
    let (added, removed) = (diff(&now, &then)?, diff(&then, &now)?);
    let (a, r) = (mesh(&added)?, mesh(&removed)?);
    let mut out = Vec::with_capacity(20 + a.len() + r.len());
    out.extend_from_slice(&volume(&added).to_le_bytes());
    out.extend_from_slice(&volume(&removed).to_le_bytes());
    out.extend_from_slice(&(a.len() as u32).to_le_bytes());
    out.extend(a);
    out.extend(r);
    Ok(out)
}

/// Los cuerpos hechos de una malla (el escaneo como cuerpo, un STL pasado a
/// sólido) tienen una cara plana por triángulo: sus aristas casi planas no se
/// dibujan (quedan vacías, así no cambian los índices de las demás).
fn hide_facet_edges(body: &cad_model::Shape, t: &mut cad_model::occt::Tessellation) {
    let mut count = vec![0u32; body.face_count()];
    let mut normal = vec![[0.0; 3]; count.len()];
    for (tri, &f) in t.triangles.iter().zip(&t.triangle_face) {
        let Some(c) = count.get_mut(f as usize) else { continue };
        *c += 1;
        let [a, b, c] = tri.map(|i| t.positions[i as usize]);
        normal[f as usize] = cad_model::geom::normalize(cad_model::geom::cross(cad_model::geom::sub(b, a), cad_model::geom::sub(c, a)));
    }
    // Pocas facetas: no vale la pena (ni hay malla)
    if count.iter().filter(|&&c| c == 1).count() < 64 {
        return;
    }
    let Ok(pairs) = body.edge_face_pairs() else { return };
    let facet = |f: Option<usize>| f.filter(|&f| count.get(f) == Some(&1));
    for (edge, pair) in t.edges.iter_mut().zip(pairs) {
        if let (Some(a), Some(b)) = (facet(pair[0]), facet(pair[1])) {
            // Menos de 30° entre caras
            if cad_model::geom::dot(normal[a], normal[b]) > 0.866 {
                edge.clear();
            }
        }
    }
}

/// Teselado para el visor: posiciones y normales (Y arriba, unidades de la
/// escena), triángulos, cara de cada triángulo y aristas como polilíneas.
fn encode_view_mesh(body: &cad_model::Shape, scale: f64) -> Result<Vec<u8>, String> {
    {
        let mut t = body.tessellate(VIEW_DEFLECTION, VIEW_ANGLE).map_err(|e| e.to_string())?;
        hide_facet_edges(body, &mut t);
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

/// Caras del sólido que vienen de la operación `feature` (para resaltarla al
/// pasar el mouse por el árbol).
#[tauri::command]
pub async fn cad_feature_faces(app: AppHandle, feature: u32) -> Result<Vec<usize>, String> {
    in_background(app, move |state| feature_faces_impl(state, feature)).await
}

fn feature_faces_impl(state: &AppState, feature: u32) -> Result<Vec<usize>, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let Some(c) = cache.as_ref() else { return Ok(Vec::new()) };
    Ok(c.eval.face_tags.iter().enumerate().filter(|(_, ts)| ts.iter().any(|t| t.feature.0 == feature)).map(|(f, _)| f).collect())
}

/// Aristas del borde de una cara (para redondear o achaflanar la cara entera).
#[tauri::command]
pub async fn cad_face_edges(app: AppHandle, face: usize) -> Result<Vec<usize>, String> {
    in_background(app, move |state| face_edges_impl(state, face)).await
}

fn face_edges_impl(state: &AppState, face: usize) -> Result<Vec<usize>, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("Todavía no hay un sólido")?;
    let pairs = body.edge_face_pairs().map_err(|e| e.to_string())?;
    Ok(pairs.iter().enumerate().filter(|(_, p)| p.contains(&Some(face))).map(|(e, _)| e).collect())
}

/// Vértice del sólido en `point` como punto que lo sigue: el extremo de una
/// arista que termina ahí.
#[tauri::command]
pub async fn cad_vertex_spec(app: AppHandle, point: [f64; 3]) -> Result<cad_model::PointSpec, String> {
    in_background(app, move |state| vertex_spec_impl(state, point)).await
}

fn vertex_spec_impl(state: &AppState, point: [f64; 3]) -> Result<cad_model::PointSpec, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("No hay diseño")?.eval;
    let body = eval.body.as_ref().ok_or("Todavía no hay un sólido")?;
    let d = |a: [f64; 3]| (0..3).map(|k| (a[k] - point[k]).powi(2)).sum::<f64>().sqrt();
    let mut best: Option<(f64, usize, bool)> = None;
    for (i, e) in body.edges().map_err(|e| e.to_string())?.iter().enumerate() {
        for (end, at) in [(false, e.start), (true, e.end)] {
            if best.is_none_or(|b| d(at) < b.0) {
                best = Some((d(at), i, end));
            }
        }
    }
    let (dist, i, end) = best.ok_or("El sólido no tiene aristas")?;
    if dist > 1e-3 {
        return Err("Ahí no hay un vértice del sólido".into());
    }
    let edge = eval.edge_ref(i).ok_or("Esa arista no existe")?;
    Ok(cad_model::PointSpec::EdgeEnd { edge, end })
}

/// Arista del sólido proyectada al plano de un sketch ("Usar"), en
/// coordenadas del plano, con la referencia que la sigue.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectedEdge {
    #[serde(flatten)]
    pub curve: cad_model::project::Projected,
    #[serde(rename = "ref")]
    pub edge_ref: EdgeRef,
}

#[tauri::command]
pub async fn cad_project_edge(app: AppHandle, edge: usize, plane: cad_model::Plane) -> Result<ProjectedEdge, String> {
    in_background(app, move |state| project_edge_impl(state, edge, &plane)).await
}

fn project_edge_impl(state: &AppState, edge: usize, plane: &cad_model::Plane) -> Result<ProjectedEdge, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("No hay sólido")?.eval;
    let body = eval.body.as_ref().ok_or("No hay sólido")?;
    let edge_ref = eval.edge_ref(edge).ok_or("Esa arista no existe")?;
    let curve = cad_model::project::edge(body, edge, plane)?;
    if matches!(curve, cad_model::project::Projected::Point { .. }) {
        return Err("Esa arista se ve de punta desde el plano: queda como un punto (usar su vértice)".into());
    }
    Ok(ProjectedEdge { curve, edge_ref })
}

/// Contorno de una cara (todas sus aristas, con las de los agujeros), cada
/// arista ligada a la suya. Las que se ven de punta no se traen.
#[tauri::command]
pub async fn cad_project_face(app: AppHandle, face: usize, plane: cad_model::Plane) -> Result<Vec<ProjectedEdge>, String> {
    in_background(app, move |state| project_face_impl(state, face, &plane)).await
}

fn project_face_impl(state: &AppState, face: usize, plane: &cad_model::Plane) -> Result<Vec<ProjectedEdge>, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("No hay sólido")?.eval;
    let body = eval.body.as_ref().ok_or("No hay sólido")?;
    let pairs = body.edge_face_pairs().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, f) in pairs.iter().enumerate() {
        if !f.contains(&Some(face)) {
            continue;
        }
        let (Some(edge_ref), Ok(curve)) = (eval.edge_ref(i), cad_model::project::edge(body, i, plane)) else { continue };
        if !matches!(curve, cad_model::project::Projected::Point { .. }) {
            out.push(ProjectedEdge { curve, edge_ref });
        }
    }
    if out.is_empty() {
        return Err("Esa cara no tiene aristas que se vean desde el plano".into());
    }
    Ok(out)
}

/// Intersección del plano con el sólido (`section`) o su silueta vista desde
/// la normal del plano (`silhouette`), con el sólido que hay justo antes de la
/// operación `index` (el sketch que se edita).
#[tauri::command]
pub async fn cad_project_model(app: AppHandle, document: Document, index: usize, plane: cad_model::Plane, what: String) -> Result<Vec<cad_model::project::Projected>, String> {
    require_occt()?;
    in_background(app, move |state| project_model_impl(state, document, index, &plane, &what)).await
}

fn project_model_impl(state: &AppState, mut doc: Document, index: usize, plane: &cad_model::Plane, what: &str) -> Result<Vec<cad_model::project::Projected>, String> {
    doc.rollback = Some(doc.rollback.map_or(index, |r| r.min(index)));
    let doc = prepared(state, None, &doc).unwrap_or(doc);
    let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
    let body = eval.body.as_ref().ok_or("No hay sólido antes de este sketch")?;
    let curves = match what {
        "section" => cad_model::project::section(body, plane)?,
        "silhouette" => cad_model::project::silhouette(body, plane)?,
        _ => return Err(format!("Proyección desconocida: {what}")),
    };
    if curves.is_empty() {
        return Err(if what == "section" { "El plano del sketch no corta el sólido".into() } else { "El sólido no se ve desde este plano".into() });
    }
    Ok(curves)
}

/// Entidad de otro sketch proyectada al plano.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectedEntity {
    #[serde(flatten)]
    pub curve: cad_model::project::Projected,
    pub entity: u32,
}

/// Todas las entidades del sketch `feature` (ya calculado) en el plano.
#[tauri::command]
pub async fn cad_project_sketch(app: AppHandle, feature: cad_model::FeatureId, plane: cad_model::Plane) -> Result<Vec<ProjectedEntity>, String> {
    in_background(app, move |state| project_sketch_impl(state, feature, &plane)).await
}

fn project_sketch_impl(state: &AppState, feature: cad_model::FeatureId, plane: &cad_model::Plane) -> Result<Vec<ProjectedEntity>, String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("No hay diseño")?.eval;
    let r = eval.sketches.get(&feature).ok_or("Ese sketch no está calculado")?;
    Ok(r.sketch
        .entities
        .iter()
        .filter_map(|e| cad_model::project::sketch_entity(&r.sketch, &r.plane, e.id, plane).ok().map(|curve| ProjectedEntity { curve, entity: e.id }))
        .collect())
}

/// Qué cara o arista del sólido mostrado es cada referencia (`None` = no se encontró).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResolvedRefs {
    pub faces: Vec<Option<usize>>,
    pub edges: Vec<Option<usize>>,
}

/// Resuelve referencias contra el sólido que se ve (el borrador, si hay): para
/// resaltar lo elegido en una caja de selección y marcar lo que ya no está.
/// Desarrollo de chapa de una pieza (sin `part`: la primera que sea de chapa,
/// o la única). Espesor y factor K de la última chapa del diseño (si no hay,
/// se mide el espesor y K = 0,44).
#[tauri::command]
pub async fn cad_flat_pattern(app: AppHandle, part: Option<cad_model::PartId>) -> Result<cad_model::sheet::FlatPattern, String> {
    require_occt()?;
    in_background(app, move |state| flat_pattern_impl(state, part)).await
}

fn flat_pattern_impl(state: &AppState, part: Option<cad_model::PartId>) -> Result<cad_model::sheet::FlatPattern, String> {
    evaluate_committed(state)?;
    let doc = state.cad_doc().unwrap_or_default();
    let cache = state.cad_cache.lock().unwrap();
    let eval = &cache.as_ref().ok_or("No hay sólido")?.eval;
    let sheet = doc.features.iter().rev().find_map(|f| match &f.kind {
        FeatureKind::SheetMetal { thickness, k_factor, .. } if !f.suppressed => Some((f.id, *thickness, *k_factor)),
        _ => None,
    });
    let chosen = match part {
        Some(id) => eval.parts.iter().find(|p| p.id == id).ok_or("Esa pieza ya no existe")?,
        // La pieza que nace de la chapa, o la primera
        None => sheet
            .and_then(|(id, ..)| eval.parts.iter().find(|p| p.id.feature == id))
            .or(eval.parts.first())
            .ok_or("Todavía no hay piezas")?,
    };
    let (thickness, k) = match sheet {
        Some((_, t, k)) => (Some(t), k),
        None => (None, 0.44),
    };
    cad_model::sheet::flat_pattern(&chosen.shape, k, thickness, None)
}

/// Dónde apoyar una pieza estándar en el borde de un agujero.
#[derive(Debug, Clone, Serialize)]
pub struct Seat {
    /// Centro del borde, sobre la cara plana
    pub origin: [f64; 3],
    /// Normal saliente de la cara (la pieza sale hacia ahí)
    pub z: [f64; 3],
    pub x: [f64; 3],
    pub diameter: f64,
    /// Medida que mejor calza ("M6")
    pub size: String,
    /// Material atravesado al lado del agujero (para el largo del tornillo)
    pub depth: Option<f64>,
}

#[tauri::command]
pub async fn cad_edge_seat(app: AppHandle, edge: usize) -> Result<Seat, String> {
    in_background(app, move |state| edge_seat_impl(state, edge)).await
}

fn edge_seat_impl(state: &AppState, edge: usize) -> Result<Seat, String> {
    use cad_model::geom::{add, cross, dot, normalize, scale};
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let body = cache.as_ref().and_then(|c| c.eval.body.as_ref()).ok_or("No hay sólido")?;
    let info = body.edge_info(edge).map_err(|e| e.to_string())?;
    let Some((center, axis, r)) = info.circle else {
        return Err("Elige el borde circular de un agujero".into());
    };
    let axis = normalize(axis);
    // La cara plana del borde da hacia dónde sale la pieza
    let z = body
        .edge_faces(edge)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|f| body.face_info(f).ok())
        .find(|f| f.surface == cad_model::occt::SurfaceKind::Plane && dot(f.normal, axis).abs() > 0.99)
        .map(|f| normalize(f.normal))
        .ok_or("El borde tiene que estar sobre una cara plana")?;
    let x = normalize(cross(z, if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }));
    let x = cross(x, z);
    // Espesor: un rayo hacia adentro, apenas por fuera del agujero
    let start = add(add(center, scale(x, r + (0.05 * r).max(0.2))), scale(z, -1e-3));
    let depth = body.ray_hit(start, scale(z, -1.0)).map(|d| d + 1e-3);
    let size = cad_model::standard::size_for_hole(2.0 * r).name.to_string();
    Ok(Seat { origin: center, z, x, diameter: 2.0 * r, size, depth })
}

/// Eje x que tendría un sketch con la horizontal `x_axis` (el dibujo se gira
/// al revés para quedar en su lugar).
#[tauri::command]
pub async fn cad_sketch_x(app: AppHandle, feature: u32, x_axis: Option<cad_model::AxisSpec>) -> Result<[f64; 3], String> {
    in_background(app, move |state| sketch_x_impl(state, feature, x_axis.as_ref())).await
}

fn sketch_x_impl(state: &AppState, feature: u32, x_axis: Option<&cad_model::AxisSpec>) -> Result<[f64; 3], String> {
    evaluate(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let c = cache.as_ref().ok_or("el diseño no está calculado")?;
    c.eval.sketch_x(cad_model::FeatureId(feature), x_axis)
}

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
    let doc = prepared(state, None, &doc).unwrap_or(doc);
    let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
    let masses: Vec<_> = eval.parts.iter().map(|p| p.shape.mass().ok()).collect();
    Ok(part_views(&doc, &eval, &masses))
}

fn part_views(doc: &Document, eval: &cad_model::Evaluation, masses: &[Option<cad_model::occt::MassInfo>]) -> Vec<PartView> {
    let numbers = part_numbers(doc, eval);
    eval.part_ranges()
        .into_iter()
        .zip(masses)
        .enumerate()
        .map(|(i, ((id, f, e), m))| {
            let m = *m;
            PartView {
                id,
                name: part_name(doc, id, numbers.get(&id).copied().unwrap_or(i + 1)),
                faces: [f.start, f.end],
                edges: [e.start, e.end],
                volume: m.map_or(0.0, |m| m.volume),
                area: m.map_or(0.0, |m| m.area),
                center: m.map_or([0.0; 3], |m| m.center),
                surface: eval.parts.get(i).is_some_and(|p| p.is_surface()),
                inherit: if doc.parts.iter().any(|p| p.part == id) { None } else { inherit_from(doc, eval, id) },
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
    /// Material que se quita: la unión de estos prismas (cortes por una línea o escalonados,
    /// medio corte, rotura). Las caras que quedan sobre sus planos se rayan.
    #[serde(default)]
    pub cut: Vec<CutPrism>,
    /// Devolver también la malla de lo que se ve (para la vista sombreada)
    #[serde(default)]
    pub shaded: bool,
}

/// Un polígono plano (mm, cerrado) extruido por `dir` (el largo va en el vector).
#[derive(Debug, Clone, Deserialize)]
pub struct CutPrism {
    pub points: Vec<[f64; 3]>,
    pub dir: [f64; 3],
}

/// Malla de una vista sombreada: posiciones y normales (mm, de a tres), triángulos y cuáles
/// están sobre un plano de corte (para pintarlos aparte).
#[derive(Debug, Clone, Serialize)]
pub struct DrawingMesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub cut: Vec<u32>,
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
    /// Con varias piezas: de qué pieza (índice en `parts`) es cada triángulo del rayado
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hatch_part: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<DrawingMesh>,
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

/// Lo que muestra un plano: el sólido entero y cada pieza con el índice de la suya en `parts`
/// del recálculo (para rayarla según su material). En un ensamble, las instancias en su lugar.
fn drawing_shapes(state: &AppState) -> Result<(cad_model::Shape, Vec<(u32, cad_model::Shape)>), String> {
    if state.cad_in_assembly() {
        let (asm, shapes) = assembly_shapes(state)?;
        let cache = state.cad_cache.lock().unwrap();
        let parts = &cache.as_ref().ok_or("El ensamble todavía no tiene piezas")?.eval.parts;
        let pieces: Vec<(u32, cad_model::Shape)> = shapes
            .into_iter()
            .filter_map(|(id, s)| {
                let inst = asm.instances.iter().find(|i| i.id == id)?;
                parts.iter().position(|p| p.id == inst.part).map(|k| (k as u32, s))
            })
            .collect();
        let list: Vec<cad_model::Shape> = pieces.iter().map(|(_, s)| s.clone()).collect();
        let body = match list.len() {
            0 => return Err("El ensamble todavía no tiene piezas".into()),
            1 => list[0].clone(),
            _ => cad_model::Shape::compound(&list).map_err(|e| e.to_string())?,
        };
        return Ok((body, pieces));
    }
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let body = ev.body.clone().ok_or("El diseño todavía no tiene un sólido")?;
    let pieces = ev.parts.iter().enumerate().filter(|(_, p)| !p.is_surface()).map(|(i, p)| (i as u32, p.shape.clone())).collect();
    Ok((body, pieces))
}

/// Lo que necesita un plano antes de pedir las vistas: la caja de lo que muestra y, en un
/// ensamble, el centro de cada instancia en su lugar (para los globos).
#[derive(Serialize)]
pub struct DrawingInfo {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub items: Vec<DrawingItem>,
}

#[derive(Serialize)]
pub struct DrawingItem {
    pub instance: u32,
    pub center: [f64; 3],
}

#[tauri::command]
pub async fn cad_drawing_info(app: AppHandle) -> Result<DrawingInfo, String> {
    in_background(app, drawing_info_impl).await
}

fn drawing_info_impl(state: &AppState) -> Result<DrawingInfo, String> {
    require_occt()?;
    if !state.cad_in_assembly() {
        let (body, _) = drawing_shapes(state)?;
        let (min, max) = body.bounds().map_err(|e| e.to_string())?;
        return Ok(DrawingInfo { min, max, items: Vec::new() });
    }
    let (_, shapes) = assembly_shapes(state)?;
    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let mut items = Vec::new();
    for (instance, s) in shapes {
        let (lo, hi) = s.bounds().map_err(|e| e.to_string())?;
        for k in 0..3 {
            min[k] = min[k].min(lo[k]);
            max[k] = max[k].max(hi[k]);
        }
        let center = s.mass().map_or([0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0), |m| m.center);
        items.push(DrawingItem { instance, center });
    }
    if items.is_empty() {
        return Err("El ensamble todavía no tiene piezas".into());
    }
    Ok(DrawingInfo { min, max, items })
}

fn drawing_impl(state: &AppState, views: &[DrawingViewSpec]) -> Result<Vec<DrawingViewResult>, String> {
    require_occt()?;
    // Con varias piezas, el rayado sale pieza por pieza (cada una con el suyo, según su material)
    let (body, pieces) = drawing_shapes(state)?;
    let body = &body;
    let m = body.mass().map_err(|e| e.to_string())?;
    let size = (0..3).map(|k| m.bbox_max[k] - m.bbox_min[k]).fold(0.0, f64::max).max(1.0);
    views
        .iter()
        .map(|v| {
            // Corte: la mitad del lado de la normal; sus caras sobre el plano se rayan
            let mut planes: Vec<([f64; 3], [f64; 3])> = Vec::new();
            let mut cut = match &v.section {
                Some(s) => {
                    planes.push((s.origin, s.normal));
                    Some(body.split_keep(s.origin, s.normal).map_err(|e| e.to_string())?)
                }
                None => None,
            };
            // Prismas que quitan material (unidos): sus caras laterales y tapas son planos de corte
            if !v.cut.is_empty() {
                let tools = v
                    .cut
                    .iter()
                    .map(|p| {
                        planes.extend(prism_planes(p));
                        cad_model::Shape::polygon(&p.points).and_then(|f| f.prism(p.dir))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                let tool = if tools.len() == 1 { tools.into_iter().next().unwrap() } else { cad_model::Shape::fuse_all(&tools).map_err(|e| e.to_string())? };
                let from = cut.as_ref().unwrap_or(body);
                cut = Some(from.cut(&tool).map_err(|e| format!("No se pudo cortar la vista: {e}"))?);
            }
            let shape = cut.as_ref().unwrap_or(body);
            let (mut hatch, mesh) = match &cut {
                Some(c) => {
                    let (h, m) = section_hatch(c, &planes, v, size);
                    (h, if v.shaded { Some(m) } else { None })
                }
                None => (Vec::new(), if v.shaded { shaded_mesh(shape, size) } else { None }),
            };
            // Varias piezas: el mismo corte en cada una, para saber de cuál es cada triángulo
            let mut hatch_part = Vec::new();
            if cut.is_some() && pieces.len() > 1 {
                let mut per = Vec::new();
                for (i, shape) in &pieces {
                    let Ok(Some(c)) = cut_shape(shape, v) else { continue };
                    let (h, _) = section_hatch(&c, &planes, v, size);
                    hatch_part.extend(std::iter::repeat_n(*i, h.len()));
                    per.extend(h);
                }
                if per.is_empty() {
                    hatch_part.clear();
                } else {
                    hatch = per;
                }
            }
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
            Ok(DrawingViewResult { lines, hatch, hatch_part, mesh })
        })
        .collect()
}

/// Una pieza cortada como la vista (el plano del corte y los prismas); `None` si no lleva corte.
fn cut_shape(shape: &cad_model::Shape, v: &DrawingViewSpec) -> Result<Option<cad_model::Shape>, String> {
    let mut cut = match &v.section {
        Some(s) => Some(shape.split_keep(s.origin, s.normal).map_err(|e| e.to_string())?),
        None => None,
    };
    if !v.cut.is_empty() {
        let tools = v.cut.iter().map(|p| cad_model::Shape::polygon(&p.points).and_then(|f| f.prism(p.dir))).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        let tool = if tools.len() == 1 { tools.into_iter().next().unwrap() } else { cad_model::Shape::fuse_all(&tools).map_err(|e| e.to_string())? };
        cut = Some(cut.as_ref().unwrap_or(shape).cut(&tool).map_err(|e| e.to_string())?);
    }
    Ok(cut)
}

/// Planos de las caras de un prisma de corte: uno por lado del polígono y las dos tapas.
fn prism_planes(p: &CutPrism) -> Vec<([f64; 3], [f64; 3])> {
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let n = p.points.len();
    let mut out: Vec<([f64; 3], [f64; 3])> = (0..n)
        .filter_map(|i| {
            let (a, b) = (p.points[i], p.points[(i + 1) % n]);
            let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let m = cross(e, p.dir);
            let l = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            (l > 1e-12).then(|| (a, m.map(|c| c / l)))
        })
        .collect();
    if let Some(&a) = p.points.first() {
        out.push((a, p.dir));
        out.push(([a[0] + p.dir[0], a[1] + p.dir[1], a[2] + p.dir[2]], p.dir));
    }
    out
}

/// Malla para la vista sombreada (sin nada cortado).
fn shaded_mesh(shape: &cad_model::Shape, size: f64) -> Option<DrawingMesh> {
    let t = shape.tessellate(size * 1e-3, 0.3).ok()?;
    Some(DrawingMesh {
        positions: t.positions.iter().flat_map(|p| p.map(|c| c as f32)).collect(),
        normals: t.normals.iter().flat_map(|p| p.map(|c| c as f32)).collect(),
        indices: t.triangles.iter().flatten().copied().collect(),
        cut: Vec::new(),
    })
}

/// Triángulos (en la vista) de las caras de `cut` que quedaron sobre algún plano de corte, y
/// la malla entera con esos triángulos marcados (para la vista sombreada).
fn section_hatch(cut: &cad_model::Shape, planes: &[([f64; 3], [f64; 3])], v: &DrawingViewSpec, size: f64) -> (Vec<[[f64; 2]; 3]>, DrawingMesh) {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let planes: Vec<([f64; 3], [f64; 3])> = planes
        .iter()
        .map(|&(o, n)| {
            let len = dot(n, n).sqrt().max(1e-12);
            (o, n.map(|c| c / len))
        })
        .collect();
    let on_plane: Vec<bool> = (0..cut.face_count())
        .map(|i| {
            cut.face_info(i).is_ok_and(|f| {
                f.surface == cad_model::occt::SurfaceKind::Plane
                    && planes.iter().any(|&(o, n)| dot(f.normal, n).abs() > 0.999 && (dot(f.point, n) - dot(o, n)).abs() < size * 1e-6)
            })
        })
        .collect();
    let empty = DrawingMesh { positions: Vec::new(), normals: Vec::new(), indices: Vec::new(), cut: Vec::new() };
    let Ok(t) = cut.tessellate(size * 1e-3, 0.3) else { return (Vec::new(), empty) };
    // Coordenadas de la vista: x = derecha de la hoja, y = eye × x
    let e = v.eye;
    let x = v.xdir;
    let y = [e[1] * x[2] - e[2] * x[1], e[2] * x[0] - e[0] * x[2], e[0] * x[1] - e[1] * x[0]];
    let proj = |p: [f64; 3]| [dot(p, x), dot(p, y)];
    let is_cut = |f: &u32| on_plane.get(*f as usize).copied().unwrap_or(false);
    let hatch = t
        .triangles
        .iter()
        .zip(&t.triangle_face)
        .filter(|(_, f)| is_cut(f))
        .map(|(tri, _)| tri.map(|k| proj(t.positions[k as usize])))
        .collect();
    let mesh = DrawingMesh {
        positions: t.positions.iter().flat_map(|p| p.map(|c| c as f32)).collect(),
        normals: t.normals.iter().flat_map(|p| p.map(|c| c as f32)).collect(),
        indices: t.triangles.iter().flatten().copied().collect(),
        cut: t.triangle_face.iter().enumerate().filter(|(_, f)| is_cut(f)).map(|(i, _)| i as u32).collect(),
    };
    (hatch, mesh)
}

/// SVG (de un plano) a PDF del mismo tamaño, con las fuentes del sistema para los textos.
fn svg_to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    use svg2pdf::usvg;
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("SVG inválido: {e}"))?;
    // usvg pasa los mm a px de 96 por pulgada: con 96 la página queda del tamaño real
    let page = svg2pdf::PageOptions { dpi: 96.0 };
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), page).map_err(|e| format!("No se pudo armar el PDF: {e:?}"))
}

/// Guarda un plano (SVG) como PDF.
#[tauri::command]
pub async fn cad_write_pdf(path: String, svg: String) -> Result<u64, String> {
    let bytes = svg_to_pdf(&svg)?;
    std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
    Ok(bytes.len() as u64)
}

/// Escribe un archivo binario (la imagen PNG de una vista).
#[tauri::command]
pub async fn cad_write_bytes(path: String, bytes: Vec<u8>) -> Result<u64, String> {
    std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
    Ok(bytes.len() as u64)
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
    let eval = &cache.as_ref().ok_or("No hay sólido")?.eval;
    let body = eval.body.as_ref().ok_or("No hay sólido")?;
    let mut m = cad_model::measure(body, items)?;
    // Las caras de un engrane llevan el nombre de su función
    for (it, out) in items.iter().zip(m.items.iter_mut()) {
        if let cad_model::MeasureItem::Face { index } = it
            && let Some(t) = eval.face_tags.get(*index).and_then(|ts| ts.first())
            && eval.gears.iter().any(|g| g.feature == t.feature)
        {
            out.role = Some(t.name.clone());
        }
    }
    Ok(m)
}

#[cfg(test)]
fn body_scene(state: &AppState, name: &str) -> Result<Scene, String> {
    parts_scene(state, name, None)
}

/// Escena para exportar: una malla por pieza con su nombre (o solo `only`).
fn parts_scene(state: &AppState, name: &str, only: Option<cad_model::PartId>) -> Result<Scene, String> {
    evaluate_committed(state)?;
    let doc = state.cad_doc().unwrap_or_default();
    let cache = state.cad_cache.lock().unwrap();
    parts_scene_of(&doc, &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval, name, only)
}

/// Como `parts_scene`, de la pestaña `tab` (la activa usa su recálculo; las
/// demás se recalculan aparte, sin tocar lo que muestra el visor)
fn tab_parts_scene(state: &AppState, tab: Option<TabId>, name: &str, only: Option<cad_model::PartId>) -> Result<Scene, String> {
    let other = {
        let tabs = state.cad_tabs.lock().unwrap();
        match tab.filter(|t| *t != tabs.active) {
            Some(t) => Some(tabs.design(t).cloned().ok_or("Esa pestaña ya no está o no es un diseño")?),
            None => None,
        }
    };
    let Some(doc) = other else { return parts_scene(state, name, only) };
    let doc = prepared(state, tab, &doc).unwrap_or(doc);
    let eval = doc.evaluate_with(&mut state.cad_ops.lock().unwrap());
    parts_scene_of(&doc, &eval, name, only)
}

fn parts_scene_of(doc: &Document, eval: &cad_model::Evaluation, name: &str, only: Option<cad_model::PartId>) -> Result<Scene, String> {
    if eval.parts.is_empty() {
        return Err("El diseño todavía no tiene un sólido".into());
    }
    let several = eval.parts.len() > 1;
    let items: Vec<(&cad_model::Shape, String)> = eval
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| only.is_none_or(|o| o == p.id))
        // Una pieza sola conserva el nombre del diseño
        .map(|(i, p)| (&p.shape, if several || only.is_some() { part_name(doc, p.id, i + 1) } else { name.to_string() }))
        .collect();
    if items.is_empty() {
        return Err("Esa pieza ya no existe".into());
    }
    shapes_scene(&items)
}

/// Escena con una malla por forma, con su nombre (milímetros, Y arriba).
fn shapes_scene(items: &[(&cad_model::Shape, String)]) -> Result<Scene, String> {
    let mut scene = Scene::new();
    for (shape, label) in items {
        // Más fino que el del visor: es lo que se imprime o se exporta
        let t = shape.tessellate(0.01, 0.1).map_err(|e| e.to_string())?;
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
        scene.nodes.push(Node { name: label.clone(), transform: Transform::identity(), mesh: Some(mesh), skin: None, children: Vec::new() });
    }
    scene.meters_per_unit = 0.001;
    Ok(scene)
}

fn assembly_items_of(doc: &Document, asm: &cad_model::Assembly, shapes: Vec<(u32, cad_model::Shape)>) -> Result<Vec<(cad_model::Shape, String, Option<[f64; 3]>)>, String> {
    let items: Vec<_> = shapes
        .into_iter()
        .filter_map(|(id, shape)| {
            let inst = asm.instances.iter().find(|i| i.id == id)?;
            let color = doc.parts.iter().find(|q| q.part == inst.part).and_then(|q| q.color.as_deref()).and_then(hex_rgb);
            Some((shape, inst.name.clone(), color))
        })
        .collect();
    if items.is_empty() {
        return Err("El ensamble no tiene instancias".into());
    }
    Ok(items)
}

/// Exporta el sólido: STEP (exacto) o malla (STL, 3MF, OBJ, PLY, GLB).
#[tauri::command]
pub async fn cad_export(
    app: AppHandle,
    path: String,
    format: String,
    part: Option<cad_model::PartId>,
    assembly: Option<bool>,
    compensate: Option<bool>,
) -> Result<u64, String> {
    require_occt()?;
    in_background(app, move |state| export_impl(state, &path, &format, part, assembly.unwrap_or(false), compensate.unwrap_or(false))).await
}

/// El diseño evaluado otra vez con todo lo hueco compensado para imprimir (agujeros, roscas
/// interiores, tuercas, arandelas, ejes y chaveteros) y la matriz que agranda lo que el material
/// se encoge. El diseño abierto no cambia.
fn compensated_eval(state: &AppState) -> Result<(Document, cad_model::Evaluation, Option<[[f64; 4]; 3]>), String> {
    let mut doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let fab = doc.fabrication.as_mut().ok_or("Elige una impresora y un material para compensar")?;
    fab.compensate_all = true;
    let scale = fab.tolerances().shrink_scale();
    let fitted = prepared(state, None, &doc);
    let ev = fitted.as_ref().unwrap_or(&doc).evaluate();
    if let Some((_, message)) = ev.errors().first() {
        return Err(format!("El diseño compensado no se pudo calcular: {message}"));
    }
    let matrix = scale.iter().any(|s| (s - 1.0).abs() > 1e-12).then(|| [[scale[0], 0.0, 0.0, 0.0], [0.0, scale[1], 0.0, 0.0], [0.0, 0.0, scale[2], 0.0]]);
    Ok((doc, ev, matrix))
}

fn export_impl(state: &AppState, path: &str, format: &str, part: Option<cad_model::PartId>, assembly: bool, compensate: bool) -> Result<u64, String> {
    let p = std::path::Path::new(path);
    let step = matches!(format, "step" | "stp");
    // Con compensación el diseño se evalúa de nuevo; si no, sale de la caché
    let comp = if compensate { Some(compensated_eval(state)?) } else { None };
    let doc_owned;
    let guard;
    let (doc, ev): (&Document, &cad_model::Evaluation) = match &comp {
        Some((d, e, _)) => (d, e),
        None => {
            evaluate_committed(state)?;
            doc_owned = state.cad_doc().unwrap_or_default();
            guard = state.cad_cache.lock().unwrap();
            (&doc_owned, &guard.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval)
        }
    };
    let shrink = comp.as_ref().and_then(|c| c.2);
    let fit = |s: &cad_model::Shape| -> Result<cad_model::Shape, String> {
        match shrink {
            Some(m) => s.transform(m).map_err(|e| e.to_string()),
            None => Ok(s.clone()),
        }
    };

    if assembly {
        // El ensamble: cada instancia en su lugar, con su nombre
        let asm = doc.assembly.clone().ok_or("El diseño no tiene ensamble")?;
        let sol = cad_model::assembly::solve(&asm);
        let shapes = cad_model::assembly::instance_shapes(ev, &asm, &sol);
        let items: Vec<_> = assembly_items_of(doc, &asm, shapes)?
            .into_iter()
            .map(|(s, n, c)| fit(&s).map(|s| (s, n, c)))
            .collect::<Result<_, _>>()?;
        if step {
            let list: Vec<_> = items.iter().map(|(s, n, c)| (s, n.as_str(), *c)).collect();
            let bytes = cad_model::Shape::parts_to_step(&list).map_err(|e| e.to_string())?;
            std::fs::write(p, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
        } else {
            let scene = shapes_scene(&items.iter().map(|(s, n, _)| (s, n.clone())).collect::<Vec<_>>())?;
            export_scene(&scene, p, format)?;
        }
        return Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    }

    // Una pieza sola conserva el nombre del diseño
    let several = ev.parts.len() > 1;
    let selected: Vec<(String, Option<[f64; 3]>, cad_model::Shape)> = ev
        .parts
        .iter()
        .enumerate()
        .filter(|(_, x)| part.is_none_or(|id| id == x.id))
        .map(|(i, x)| {
            let name = if several || part.is_some() { part_name(doc, x.id, i + 1) } else { "Diseño".to_string() };
            let color = doc.parts.iter().find(|q| q.part == x.id).and_then(|q| q.color.as_deref()).and_then(hex_rgb);
            fit(&x.shape).map(|s| (name, color, s))
        })
        .collect::<Result<_, _>>()?;
    if selected.is_empty() {
        return Err(if part.is_some() { "Esa pieza ya no existe" } else { "El diseño todavía no tiene un sólido" }.into());
    }
    if step {
        // Cada pieza como sólido con su nombre y su color (si se eligió)
        let list: Vec<(&cad_model::Shape, &str, Option<[f64; 3]>)> = selected.iter().map(|(n, c, s)| (s, n.as_str(), *c)).collect();
        let bytes = cad_model::Shape::parts_to_step(&list).map_err(|e| e.to_string())?;
        std::fs::write(p, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
    } else {
        let scene = shapes_scene(&selected.iter().map(|(n, _, s)| (s, n.clone())).collect::<Vec<_>>())?;
        export_scene(&scene, p, format)?;
    }
    Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Cuaternión (x, y, z, w) de una matriz de giro.
fn quat_of(m: &[[f64; 3]; 3]) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, s / 4.0]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [s / 4.0, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][1] + m[1][0]) / s, s / 4.0, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, s / 4.0, (m[1][0] - m[0][1]) / s]
    };
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
    q.map(|x| x / n)
}

/// El ensamble moviendo una unión de `from` a `to` como GLB animado: cada instancia es un nodo
/// con su malla en la pose del primer cuadro y su movimiento (traslación y giro) por cuadro.
#[tauri::command]
pub async fn cad_export_assembly_animation(
    app: AppHandle,
    path: String,
    mate: u32,
    var: cad_model::assembly::SweepVar,
    from: f64,
    to: f64,
    frames: Option<usize>,
    fps: Option<f64>,
) -> Result<u64, String> {
    require_occt()?;
    in_background(app, move |state| export_assembly_animation_impl(state, &path, mate, var, from, to, frames.unwrap_or(30), fps.unwrap_or(30.0))).await
}

#[allow(clippy::too_many_arguments)]
fn export_assembly_animation_impl(state: &AppState, path: &str, mate: u32, var: cad_model::assembly::SweepVar, from: f64, to: f64, frames: usize, fps: f64) -> Result<u64, String> {
    let doc = state.cad_doc().ok_or("No hay un diseño abierto")?;
    let asm = doc.assembly.clone().ok_or("El diseño no tiene ensamble")?;
    evaluate_committed(state)?;
    let cache = state.cad_cache.lock().unwrap();
    let ev = &cache.as_ref().ok_or("El diseño todavía no tiene un sólido")?.eval;
    let sweep = cad_model::assembly::sweep(&asm, mate, var, from, to, frames.clamp(2, 600))?;
    let shapes = cad_model::assembly::instance_shapes(ev, &asm, &sweep[0]);
    let ids: Vec<u32> = shapes.iter().map(|(id, _)| *id).collect();
    let items = assembly_items_of(&doc, &asm, shapes)?;
    let mut scene = shapes_scene(&items.iter().map(|(s, n, _)| (s, n.clone())).collect::<Vec<_>>())?;
    let times: Vec<f32> = (0..sweep.len()).map(|k| (k as f64 / fps.max(1.0)) as f32).collect();
    let pose = |k: usize, id: u32| sweep[k].poses.iter().find(|p| p.0 == id).map(|p| (p.1, cad_model::assembly::rotation_matrix(p.2)));
    let mut channels = Vec::new();
    for (node, id) in ids.iter().enumerate() {
        let Some((t0, r0)) = pose(0, *id) else { continue };
        let (mut moves, mut turns) = (Vec::new(), Vec::new());
        for k in 0..sweep.len() {
            let Some((tk, rk)) = pose(k, *id) else { continue };
            // Movimiento respecto del primer cuadro: x' = Rk·R0ᵀ·(x − t0) + tk
            let d: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|c| rk[i][c] * r0[j][c]).sum()));
            let t: [f64; 3] = std::array::from_fn(|i| tk[i] - (0..3).map(|j| d[i][j] * t0[j]).sum::<f64>());
            let q = quat_of(&d);
            // Z arriba → Y arriba, como la geometría
            let tv = converter_scene::z_up_to_y_up([t[0] as f32, t[1] as f32, t[2] as f32]);
            let qv = converter_scene::z_up_to_y_up([q[0] as f32, q[1] as f32, q[2] as f32]);
            moves.push(tv);
            turns.push([qv[0], qv[1], qv[2], q[3] as f32]);
        }
        channels.push(AnimChannel { node, interpolation: Interpolation::Linear, times: times.clone(), values: KeyframeValues::Translation(moves) });
        channels.push(AnimChannel { node, interpolation: Interpolation::Linear, times: times.clone(), values: KeyframeValues::Rotation(turns) });
    }
    let name = asm.mates.iter().find(|m| m.id == mate).map(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Unión".into());
    scene.animations.push(Animation { name, channels });
    let p = std::path::Path::new(path);
    export_scene(&scene, p, "glb")?;
    Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Malla del diseño a un archivo según el formato.
pub(crate) fn export_scene(scene: &Scene, p: &std::path::Path, format: &str) -> Result<(), String> {
    match format {
        "stl" => converter_stl::export_stl(scene, p).map_err(|e| format!("Error exportando STL: {e:?}")),
        "3mf" => converter_3mf::export_3mf(scene, p).map_err(|e| format!("Error exportando 3MF: {e}")),
        "obj" => converter_obj::export_obj(scene, p).map_err(|e| format!("Error exportando OBJ: {e:?}")),
        "ply" => converter_ply::export_ply(scene, p).map_err(|e| format!("Error exportando PLY: {e}")),
        "glb" => converter_gltf_io::export_glb(scene, p, &Default::default()).map_err(|e| format!("Error exportando GLB: {e}")),
        other => Err(format!("Formato no soportado para el diseño: {other}")),
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
        state.with_cad_doc(|doc| {
            let id = doc.add(FeatureKind::Import { format: ImportFormat::Step, data, op: op.unwrap_or_default() });
            if let Some(name) = std::path::Path::new(&path).file_stem() {
                doc.get_mut(id).unwrap().name = name.to_string_lossy().into_owned();
            }
        })?;
        evaluate(state)
    })
    .await
}

/// Suelda los vértices repetidos por cara (el teselado parte las aristas
/// vivas para las normales) y promedia las normales, como un STL importado:
/// para Reparar y Fabricar la malla tiene que quedar cerrada.
fn weld_scene(scene: &mut Scene) {
    for mesh in &mut scene.meshes {
        for prim in &mut mesh.primitives {
            let Some(positions) = prim.attributes.iter().find_map(|a| match a {
                VertexAttribute::Positions(p) => Some(p.clone()),
                _ => None,
            }) else {
                continue;
            };
            let Some(IndexData::U32(indices)) = &prim.indices else { continue };
            let mut index: HashMap<[u32; 3], u32> = HashMap::new();
            let mut welded: Vec<[f32; 3]> = Vec::new();
            let remap: Vec<u32> = positions
                .iter()
                .map(|p| {
                    *index.entry(p.map(f32::to_bits)).or_insert_with(|| {
                        welded.push(*p);
                        (welded.len() - 1) as u32
                    })
                })
                .collect();
            let indices: Vec<u32> = indices.iter().map(|&i| remap[i as usize]).collect();
            let mut normals = vec![[0.0f32; 3]; welded.len()];
            for t in indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| welded[i as usize]);
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                // Producto vectorial sin normalizar: pesa por el área
                let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                for &i in t {
                    for k in 0..3 {
                        normals[i as usize][k] += n[k];
                    }
                }
            }
            for n in &mut normals {
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len > 1e-12 {
                    *n = n.map(|x| x / len);
                }
            }
            prim.attributes = vec![VertexAttribute::Positions(welded), VertexAttribute::Normals(normals)];
            prim.indices = Some(IndexData::U32(indices));
        }
    }
}

/// Resultado de `cad_part_to_model`
#[derive(Serialize)]
pub struct PartModel {
    /// Se cargó una malla nueva (`false`: la pieza no cambió desde `known`)
    pub changed: bool,
    /// Huella de la malla de la pieza (texto: un u64 no entra entero en JS)
    pub hash: String,
    pub info: Option<MeshInfo>,
}

/// La malla de una pieza del diseño pasa a ser el modelo del objeto activo.
/// `studio` es la pestaña de la pieza (falta = la 0, la de los proyectos de
/// antes). `known` es la huella de la última vez: si la pieza no cambió no hace nada.
/// El esqueleto del objeto se conserva (como al importar un modelo sin rig);
/// lo demás derivado de la malla se descarta y lo rehace la interfaz.
#[tauri::command]
pub async fn cad_part_to_model(
    app: AppHandle,
    part: cad_model::PartId,
    studio: Option<TabId>,
    known: Option<String>,
    on_progress: Channel<Progress>,
) -> Result<PartModel, String> {
    require_occt()?;
    in_background(app, move |state| part_to_model_impl(state, part, studio.unwrap_or(0), known, &on_progress)).await
}

pub(crate) fn part_to_model_impl(
    state: &AppState,
    part: cad_model::PartId,
    studio: TabId,
    known: Option<String>,
    progress: &Channel<Progress>,
) -> Result<PartModel, String> {
    let mut scene = tab_parts_scene(state, Some(studio), "Pieza", Some(part))?;
    weld_scene(&mut scene);
    let hash = format!("{:016x}", scene_hash(&scene));
    if known.as_deref() == Some(hash.as_str()) && state.mesh.is_loaded() {
        return Ok(PartModel { changed: false, hash, info: None });
    }
    let name = scene.meshes.first().map(|m| m.name.clone()).unwrap_or_else(|| "Pieza".into());
    let info = load_scene(scene, name, "CAD".into(), progress, state)?;
    Ok(PartModel { changed: true, hash, info: Some(info) })
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

/// Corre `f` con la malla del escaneo lista para escaneo → CAD (Z arriba, mm):
/// el objeto elegido con `cad_scan_use` (guardado o activo) o, si no hay, el
/// activo. Se prepara una vez y queda en caché mientras no cambie.
fn with_scan<T>(state: &AppState, f: impl FnOnce(&ScanCache) -> Result<T, String>) -> Result<T, String> {
    let object = *state.cad_scan_object.lock().unwrap();
    let stored = object.and_then(|id| crate::project::stored_object_hash(state, id).map(|h| (id, h)));
    let hash = match stored {
        // Otro número que el de una escena activa con la misma huella
        Some((id, h)) => h ^ id.rotate_left(32) ^ 0x5ca1,
        None => scene_hash(state.scene.lock().unwrap().as_ref().ok_or("No hay un modelo cargado para tomar medidas")?),
    };
    let mut cache = state.cad_scan.lock().unwrap();
    if cache.as_ref().is_none_or(|c| c.scene_hash != hash) {
        let scene = match stored {
            Some((id, _)) => crate::project::stored_object_scene(state, id).ok_or("El escaneo elegido ya no está")??,
            None => state.scene.lock().unwrap().clone().ok_or("No hay un modelo cargado para tomar medidas")?,
        };
        let mm = scene.meters_per_unit * 1000.0;
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for prim in scene.world_primitives_z_up() {
            let base = vertices.len() as u32;
            vertices.extend(prim.positions.iter().map(|p| [p[0] as f64 * mm, p[1] as f64 * mm, p[2] as f64 * mm]));
            triangles.extend(prim.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        *cache = Some(ScanCache { scene_hash: hash, mesh: ScanMesh::new(&vertices, &triangles), mm_per_unit: mm, fits: Mutex::default(), paths: Mutex::default(), zones: Mutex::default(), bodies: Mutex::default() });
    }
    f(cache.as_ref().unwrap())
}

/// Qué objeto de la escena es el escaneo (`None`: el activo).
#[tauri::command]
pub fn cad_scan_use(object: Option<u64>, state: tauri::State<'_, AppState>) {
    *state.cad_scan_object.lock().unwrap() = object;
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
    Cone {
        #[serde(flatten)]
        pick: cad_scan::ConePick,
    },
    Torus {
        #[serde(flatten)]
        pick: cad_scan::TorusPick,
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
            "cone" => Ok(ScanPick::Cone { pick: cad_scan::pick_cone(&scan.mesh, triangle, &options.pick()).map_err(|e| e.to_string())? }),
            "torus" => Ok(ScanPick::Torus { pick: cad_scan::pick_torus(&scan.mesh, triangle, &options.pick()).map_err(|e| e.to_string())? }),
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

/// Corte del escaneo por el plano de un sketch: las polilíneas (guía y
/// anclajes) y el contorno ya pasado a sketch (líneas y círculos reconocidos,
/// con paralelas y perpendiculares) para «Usar el corte».
#[derive(Debug, Clone, Serialize)]
pub struct ScanOutline {
    pub sections: Vec<Section>,
    pub sketch: Sketch,
}

#[tauri::command]
pub async fn cad_scan_outline(app: AppHandle, plane: Plane) -> Result<ScanOutline, String> {
    in_background(app, move |state| scan_outline_impl(state, &plane)).await
}

fn scan_outline_impl(state: &AppState, plane: &Plane) -> Result<ScanOutline, String> {
    with_scan(state, |scan| {
        let sections = cad_scan::slice(&scan.mesh, plane);
        let loops: Vec<Vec<[f64; 2]>> = sections.iter().filter(|s| s.closed).map(|s| s.points.clone()).collect();
        let sketch = cad_scan::outline_sketch(&loops, &ScanPickOptions::default().outline(&scan.mesh));
        Ok(ScanOutline { sections, sketch })
    })
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
    /// Cono (tronco) ajustado: macizo (une) o hueco (resta).
    Cone { triangle: u32 },
    /// Toro ajustado.
    Torus { triangle: u32 },
    /// Sketch con el corte del modelo por un plano.
    Slice { plane: Plane },
    /// Plano de referencia (sketch vacío) sobre una zona plana.
    WorkPlane { triangle: u32 },
    /// Referencias ligadas al escaneo (lo siguen si se mueve o se repara):
    /// plano de la zona plana, plano por tres puntos, eje del cilindro y un
    /// punto de la superficie. `at` = el punto del clic (mm).
    RefPlane { triangle: u32, at: P3 },
    RefPlanePoints { points: Vec<(u32, P3)> },
    RefAxis { triangle: u32, at: P3 },
    RefPoint { triangle: u32, at: P3 },
    /// Superficie libre ajustada a la zona suave del clic (`close`: cara plana del escaneo contra la que cerrar el sólido)
    FreeSurface { triangle: u32, at: P3, angle: f64, thickness: f64, close: Option<(u32, P3)> },
    /// Curva por el camino más corto sobre la piel entre los puntos tocados
    PathCurve { points: Vec<(u32, P3)>, closed: bool },
}

/// Toques del pincel sobre el escaneo (triángulo y punto en mm de cada uno).
#[tauri::command]
pub async fn cad_scan_dabs(app: AppHandle, points: Vec<(u32, P3)>) -> Result<Vec<cad_model::ScanDab>, String> {
    in_background(app, move |state| {
        with_scan(state, |scan| Ok(points.iter().filter_map(|(t, at)| scan.mesh.dab(*t, *at)).collect()))
    })
    .await
}

/// Molde de dos mitades alrededor del escaneo (ver [`cad_scan::add_mold`]).
#[tauri::command]
pub async fn cad_scan_mold(app: AppHandle, options: cad_scan::MoldOptions) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| scan_mold_impl(state, options)).await
}

fn scan_mold_impl(state: &AppState, o: cad_scan::MoldOptions) -> Result<CadResult, String> {
    let (lo, hi) = with_scan(state, |scan| Ok((scan.mesh.bbox_min, scan.mesh.bbox_max)))?;
    *state.cad_preview.lock().unwrap() = None;
    state.with_cad_doc(|doc| cad_scan::add_mold(doc, lo, hi, &o))??;
    evaluate(state)
}

/// Revolución del perfil del escaneo cortado por un plano que contiene el eje
/// (`origin`, `direction`): para piezas torneadas.
#[tauri::command]
pub async fn cad_scan_turned(app: AppHandle, origin: P3, direction: P3, op: Option<BodyOp>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| scan_turned_impl(state, origin, direction, op.unwrap_or(BodyOp::New))).await
}

fn scan_turned_impl(state: &AppState, origin: P3, direction: P3, op: BodyOp) -> Result<CadResult, String> {
    *state.cad_preview.lock().unwrap() = None;
    with_scan(state, |scan| {
        state.with_cad_doc(|doc| cad_scan::add_turned(doc, &scan.mesh, origin, direction, &ScanPickOptions::default().outline(&scan.mesh), op).map(|_| ()))?
    })?;
    evaluate(state)
}

/// Recubrimiento por `count` cortes paralelos, perpendiculares a `direction`.
#[tauri::command]
pub async fn cad_scan_skin(app: AppHandle, direction: P3, count: usize, ruled: Option<bool>, op: Option<BodyOp>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| scan_skin_impl(state, direction, count, ruled.unwrap_or(false), op.unwrap_or(BodyOp::New))).await
}

fn scan_skin_impl(state: &AppState, direction: P3, count: usize, ruled: bool, op: BodyOp) -> Result<CadResult, String> {
    *state.cad_preview.lock().unwrap() = None;
    with_scan(state, |scan| {
        state.with_cad_doc(|doc| cad_scan::add_skin(doc, &scan.mesh, direction, count, &ScanPickOptions::default().outline(&scan.mesh), ruled, op).map(|_| ()))?
    })?;
    evaluate(state)
}

/// Árbol de operaciones propuesto para reconstruir el escaneo (base, agujeros, tetones…).
#[tauri::command]
pub async fn cad_scan_propose(app: AppHandle, options: Option<ScanPickOptions>) -> Result<cad_scan::Proposal, String> {
    in_background(app, move |state| scan_propose_impl(state, options.unwrap_or_default())).await
}

fn scan_propose_impl(state: &AppState, options: ScanPickOptions) -> Result<cad_scan::Proposal, String> {
    with_scan(state, |scan| Ok(cad_scan::propose(&scan.mesh, &options.pick(), &options.outline(&scan.mesh))))
}

/// Agrega al diseño las operaciones de los pasos elegidos de la propuesta.
#[tauri::command]
pub async fn cad_scan_apply(app: AppHandle, steps: Vec<u32>, options: Option<ScanPickOptions>) -> Result<CadResult, String> {
    require_occt()?;
    in_background(app, move |state| scan_apply_impl(state, steps, options.unwrap_or_default())).await
}

fn scan_apply_impl(state: &AppState, steps: Vec<u32>, options: ScanPickOptions) -> Result<CadResult, String> {
    *state.cad_preview.lock().unwrap() = None;
    with_scan(state, |scan| {
        state.with_cad_doc(|doc| cad_scan::apply_proposal(doc, &scan.mesh, &options.pick(), &options.outline(&scan.mesh), &steps).map(|_| ()))?
    })?;
    evaluate(state)
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
                ScanFeature::Cone { triangle } => {
                    let pick = cad_scan::pick_cone(mesh, *triangle, &options.pick()).map_err(|e| e.to_string())?;
                    let name = if pick.hole { "Cono hueco escaneado" } else { "Cono escaneado" };
                    vec![(pick.feature(), Some(name.to_string()))]
                }
                ScanFeature::Torus { triangle } => {
                    let pick = cad_scan::pick_torus(mesh, *triangle, &options.pick()).map_err(|e| e.to_string())?;
                    vec![(pick.feature(), Some("Toro escaneado".to_string()))]
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
                ScanFeature::RefPlane { triangle, at }
                | ScanFeature::RefAxis { triangle, at }
                | ScanFeature::RefPoint { triangle, at } => {
                    let seed = mesh.seed(*triangle, *at).ok_or("Ese lugar no es del escaneo")?;
                    let (fit, name) = match &feature {
                        ScanFeature::RefPlane { .. } => (cad_model::ScanFit::Zone { seed }, "Plano del escaneo"),
                        ScanFeature::RefAxis { .. } => (cad_model::ScanFit::Cylinder { seed }, "Eje del escaneo"),
                        _ => (cad_model::ScanFit::Point { seed }, "Punto del escaneo"),
                    };
                    let geom = cad_scan::fit_reference(mesh, &fit, &options.pick()).map_err(|e| e.to_string())?;
                    vec![(scan_reference(fit, geom), Some(name.to_string()))]
                }
                ScanFeature::RefPlanePoints { points } => {
                    let seeds: Vec<_> = points.iter().filter_map(|(t, at)| mesh.seed(*t, *at)).collect();
                    let seeds: [cad_model::ScanSeed; 3] = seeds.try_into().map_err(|_| "Hacen falta tres puntos del escaneo")?;
                    let fit = cad_model::ScanFit::Points { seeds };
                    let geom = cad_scan::fit_reference(mesh, &fit, &options.pick()).map_err(|e| e.to_string())?;
                    vec![(scan_reference(fit, geom), Some("Plano del escaneo".to_string()))]
                }
                ScanFeature::FreeSurface { triangle, at, angle, thickness, close } => {
                    let seed = mesh.seed(*triangle, *at).ok_or("Ese lugar no es del escaneo")?;
                    let mut zone = mesh.free_zone(*triangle, *angle, 150).ok_or("La zona no tiene un borde cerrado: elegir una zona suave rodeada de aristas vivas o del borde del escaneo")?;
                    zone.fit_surface(mesh).ok_or("No se pudo ajustar una superficie a esa zona")?;
                    let close_seed = close.as_ref().and_then(|(t, a)| mesh.seed(*t, *a));
                    let close_plane = match &close_seed {
                        Some(sd) => Some(cad_scan::pick_plane(mesh, sd.triangle, &options.pick()).map_err(|e| e.to_string())?.plane),
                        None => None,
                    };
                    let surface = cad_model::ScanSurface {
                        seed,
                        angle: *angle,
                        thickness: *thickness,
                        close: close_seed,
                        op: BodyOp::New,
                        boundary: zone.boundary,
                        interior: zone.interior,
                        close_plane,
                    };
                    vec![(FeatureKind::ScanSurface(surface), Some("Superficie libre del escaneo".to_string()))]
                }
                ScanFeature::PathCurve { points, closed } => {
                    let seeds: Vec<_> = points.iter().filter_map(|(t, at)| mesh.seed(*t, *at)).collect();
                    if seeds.len() < 2 || seeds.len() != points.len() || (*closed && seeds.len() < 3) {
                        return Err("Hacen falta al menos dos puntos del escaneo (tres si la curva es cerrada)".into());
                    }
                    let stops = points.to_vec();
                    let traced = mesh.geodesic(&stops, *closed).ok_or("No se pudo trazar el camino sobre la piel")?;
                    vec![(
                        FeatureKind::Curve { def: cad_model::CurveDef::ScanPath { seeds, closed: *closed, points: traced } },
                        Some("Camino sobre el escaneo".to_string()),
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
        state.with_cad_doc(|doc| {
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
        })?;
        evaluate(state)
    }
}

/// Operación de referencia (plano, eje o punto) ligada al escaneo
fn scan_reference(fit: cad_model::ScanFit, geom: cad_scan::ScanGeom) -> FeatureKind {
    match geom {
        cad_scan::ScanGeom::Plane(plane) => FeatureKind::Plane { def: PlaneDef::Scan { fit, plane } },
        cad_scan::ScanGeom::Axis { origin, direction } => FeatureKind::Axis { def: AxisDef::Scan { fit, origin, direction } },
        cad_scan::ScanGeom::Point(point) => FeatureKind::Point { def: PointSpec::Scan { fit, point } },
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
                state.set_cad_tabs(cad_model::Tabs::default());
                ok(())
            }
            "cad_get_document" => ok(state.cad_doc()),
            "cad_tabs" => ok(tabs_view(&state.cad_tabs.lock().unwrap())),
            "cad_tab_document" => {
                let id: TabId = arg(args, "id")?;
                ok(state.cad_tabs.lock().unwrap().design(id).cloned())
            }
            "cad_tab_parts" => ok(tab_parts_impl(state, arg(args, "id")?)?),
            "cad_tab_add" => ok(tab_add_impl(state, arg(args, "kind")?, arg(args, "name")?)?),
            "cad_tab_rename" => {
                let (id, name): (TabId, String) = (arg(args, "id")?, arg(args, "name")?);
                ok(change_tabs(state, |t| t.rename(id, &name))?)
            }
            "cad_tab_duplicate" => {
                let id: TabId = arg(args, "id")?;
                ok(change_tabs(state, |t| t.duplicate(id).map(|_| ()))?)
            }
            "cad_tab_delete" => {
                let id: TabId = arg(args, "id")?;
                ok(change_tabs(state, |t| t.remove(id).map(|_| ()))?)
            }
            "cad_tab_move" => {
                let (id, to): (TabId, usize) = (arg(args, "id")?, arg(args, "to")?);
                ok(change_tabs(state, |t| t.move_to(id, to))?)
            }
            "cad_tab_activate" => {
                let id: TabId = arg(args, "id")?;
                ok(change_tabs(state, |t| t.activate(id))?)
            }
            "cad_set_document" => ok(set_document_impl(state, arg(args, "document")?)?),
            "cad_preview" => ok(preview_impl(state, arg(args, "document")?)?),
            "cad_evaluate" => ok(evaluate(state)?),
            "cad_solve_sketch" => ok(cad_solve_sketch(arg(args, "sketch")?, arg(args, "drag")?)?),
            "cad_sketch_suggest" => ok(cad_sketch_suggest(arg(args, "sketch")?)?),
            "cad_gear_info" => ok(cad_gear_info(arg(args, "part")?)),
            "cad_gear_design" => ok(cad_gear_design(arg(args, "source")?, arg(args, "kind")?, arg(args, "ratio")?, arg(args, "distance")?)?),
            "cad_tooth_forms" => ok(cad_tooth_forms()),
            "cad_drive_length" => ok(cad_drive_length(arg(args, "form")?, arg(args, "teeth_a")?, arg(args, "teeth_b")?, arg(args, "center")?)?),
            "cad_planetary_plan" => ok(cad_planetary_plan(arg(args, "module")?, arg(args, "sun_teeth")?, arg(args, "planet_teeth")?, arg(args, "ring_teeth")?, arg(args, "planets")?, arg(args, "pressure_angle")?)?),
            "cad_gear_materials" => ok(cad_gear_materials()),
            "cad_gear_strength" => ok(cad_gear_strength(arg(args, "first")?, arg(args, "second")?, arg(args, "input")?)?),
            "cad_gear_block" => ok(cad_gear_block(arg(args, "part")?)?),
            "cad_sketch_define" => ok(cad_sketch_define(arg(args, "sketch")?, arg(args, "relations")?)?),
            "cad_mesh" => mesh_impl(state).map(Reply::Bytes),
            "cad_tool_mesh" => tool_mesh_impl(state, arg(args, "feature")?).map(Reply::Bytes),
            "cad_flat_pattern" => ok(flat_pattern_impl(state, arg::<Option<cad_model::PartId>>(args, "part").unwrap_or(None))?),
            "cad_edge_seat" => ok(edge_seat_impl(state, arg(args, "edge")?)?),
            "cad_compare" => compare_impl(state, &arg::<Document>(args, "document")?).map(Reply::Bytes),
            "cad_face_ref" => ok(face_ref_impl(state, arg(args, "face")?)?),
            "cad_edge_ref" => ok(edge_ref_impl(state, arg(args, "edge")?)?),
            "cad_vertex_spec" => ok(vertex_spec_impl(state, arg(args, "point")?)?),
            "cad_face_info" => ok(face_info_impl(state, arg(args, "face")?)?),
            "cad_face_edges" => ok(face_edges_impl(state, arg(args, "face")?)?),
            "cad_feature_faces" => ok(feature_faces_impl(state, arg(args, "feature")?)?),
            "cad_deviation" => deviation_impl(state, arg(args, "tolerance")?).map(Reply::Bytes),
            "cad_assembly_mesh" => assembly_mesh_impl(state, arg::<Option<f64>>(args, "explode")?.unwrap_or(0.0)).map(Reply::Bytes),
            "cad_assembly_faces" => ok(assembly_faces_impl(state)?),
            "cad_assembly_connector" => ok(assembly_connector_impl(state, arg(args, "face")?)?),
            "cad_assembly_interference" => ok(assembly_interference_impl(state)?),
            "cad_assembly_clearance" => ok(assembly_clearance_impl(state, arg::<Option<f64>>(args, "min_gap")?)?),
            "cad_assembly_suggest" => ok(assembly_suggest_impl(state)?),
            "cad_assembly_gear_warnings" => ok(assembly_gear_warnings_impl(state)?),
            "cad_drawing_info" => ok(drawing_info_impl(state)?),
            "cad_drawing" => ok(drawing_impl(state, &arg::<Vec<DrawingViewSpec>>(args, "views")?)?),
            "cad_write_pdf" => {
                let (path, svg): (String, String) = (arg(args, "path")?, arg(args, "svg")?);
                let bytes = svg_to_pdf(&svg)?;
                std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
                ok(bytes.len() as u64)
            }
            "cad_write_bytes" => {
                let (path, bytes): (String, Vec<u8>) = (arg(args, "path")?, arg(args, "bytes")?);
                std::fs::write(&path, &bytes).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
                ok(bytes.len() as u64)
            }
            "cad_write_text" => {
                let (path, content): (String, String) = (arg(args, "path")?, arg(args, "content")?);
                std::fs::write(&path, content.as_bytes()).map_err(|e| format!("No se pudo escribir {path}: {e}"))?;
                ok(content.len() as u64)
            }
            "cad_parts_at" => ok(parts_at_impl(state, arg(args, "document")?, arg(args, "index")?)?),
            "cad_measure" => ok(measure_impl(state, &arg::<Vec<cad_model::MeasureItem>>(args, "items")?)?),
            "cad_sketch_x" => ok(sketch_x_impl(state, arg(args, "feature")?, arg::<Option<cad_model::AxisSpec>>(args, "x_axis")?.as_ref())?),
            "cad_resolve_refs" => {
                let (faces, edges): (Vec<FaceRef>, Vec<EdgeRef>) = (arg(args, "faces")?, arg(args, "edges")?);
                ok(resolve_refs_impl(state, &faces, &edges)?)
            }
            "cad_mm_per_unit" => ok(mm_per_unit(state)),
            "cad_project_edge" => ok(project_edge_impl(state, arg(args, "edge")?, &arg::<cad_model::Plane>(args, "plane")?)?),
            "cad_project_face" => ok(project_face_impl(state, arg(args, "face")?, &arg::<cad_model::Plane>(args, "plane")?)?),
            "cad_project_model" => {
                let what: String = arg(args, "what")?;
                ok(project_model_impl(state, arg(args, "document")?, arg(args, "index")?, &arg::<cad_model::Plane>(args, "plane")?, &what)?)
            }
            "cad_project_sketch" => ok(project_sketch_impl(state, arg(args, "feature")?, &arg::<cad_model::Plane>(args, "plane")?)?),
            "cad_export" => {
                let (path, format): (String, String) = (arg(args, "path")?, arg(args, "format")?);
                ok(export_impl(state, &path, &format, arg::<Option<cad_model::PartId>>(args, "part")?, arg::<Option<bool>>(args, "assembly")?.unwrap_or(false), arg::<Option<bool>>(args, "compensate")?.unwrap_or(false))?)
            }
            "cad_export_assembly_animation" => ok(export_assembly_animation_impl(
                state,
                &arg::<String>(args, "path")?,
                arg(args, "mate")?,
                arg(args, "var")?,
                arg(args, "from")?,
                arg(args, "to")?,
                arg::<Option<usize>>(args, "frames")?.unwrap_or(30),
                arg::<Option<f64>>(args, "fps")?.unwrap_or(30.0),
            )?),
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
            "cad_scan_dabs" => {
                let points: Vec<(u32, P3)> = arg(args, "points")?;
                ok(with_scan(state, |scan| Ok(points.iter().filter_map(|(t, at)| scan.mesh.dab(*t, *at)).collect::<Vec<_>>()))?)
            }
            "cad_scan_turned" => ok(scan_turned_impl(state, arg(args, "origin")?, arg(args, "direction")?, arg::<Option<BodyOp>>(args, "op")?.unwrap_or(BodyOp::New))?),
            "cad_scan_skin" => ok(scan_skin_impl(
                state,
                arg(args, "direction")?,
                arg(args, "count")?,
                arg::<Option<bool>>(args, "ruled")?.unwrap_or(false),
                arg::<Option<BodyOp>>(args, "op")?.unwrap_or(BodyOp::New),
            )?),
            "cad_scan_propose" => ok(scan_propose_impl(state, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?),
            "cad_scan_apply" => ok(scan_apply_impl(state, arg(args, "steps")?, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?),
            "cad_scan_mold" => ok(scan_mold_impl(state, arg(args, "options")?)?),
            "cad_scan_add" => ok(scan_add_impl(state, arg(args, "feature")?, arg::<Option<ScanPickOptions>>(args, "options")?.unwrap_or_default())?),
            "import_model" => {
                let channel = Channel::new(|_| Ok(()));
                ok(crate::commands::import_model_impl(arg(args, "path")?, vec![], &channel, state)?)
            }
            "get_mesh_data" => crate::commands::get_mesh_data_impl(state).map(|d| Reply::Bytes(d.to_bytes())),
            "cad_scan_outline" => ok(scan_outline_impl(state, &arg(args, "plane")?)?),
            "cad_scan_use" => {
                *state.cad_scan_object.lock().unwrap() = arg(args, "object")?;
                ok(())
            }
            // Unidades del modelo (grilla y cámara) y materiales, como en la app
            "get_scene_structure" => {
                let scene = state.scene.lock().unwrap();
                ok(crate::structure::scene_structure(scene.as_ref().ok_or("No hay escena cargada")?))
            }
            "get_scene_materials" => {
                let scene = state.scene.lock().unwrap();
                let scene = scene.as_ref().ok_or("No hay escena cargada")?;
                ok(scene.materials.iter().map(crate::structure::material_info).collect::<Vec<_>>())
            }
            "cad_part_to_model" => {
                let channel = Channel::new(|_| Ok(()));
                let studio: Option<TabId> = arg(args, "studio")?;
                ok(part_to_model_impl(state, arg(args, "part")?, studio.unwrap_or(0), arg(args, "known")?, &channel)?)
            }
            "object_activate" => ok(crate::project::activate_impl(state, arg(args, "id")?)?),
            "object_remove" => ok(crate::project::remove_impl(state, arg(args, "id")?)),
            "object_adopt" => ok(crate::project::adopt_impl(state, arg(args, "id")?)),
            "object_mesh_data" => crate::project::object_mesh_bytes(state, arg(args, "id")?).map(Reply::Bytes),
            "objects_info" => ok(crate::project::objects_info_impl(state, &arg::<Vec<u64>>(args, "ids")?)?),
            "export_objects" => ok(crate::project::export_objects_impl(
                state,
                &arg::<Vec<u64>>(args, "ids")?,
                &arg::<Vec<String>>(args, "names")?,
                &arg::<String>(args, "path")?,
                &arg::<String>(args, "format")?,
                arg::<Option<f64>>(args, "spacing").unwrap_or(None),
            )?),
            "take_snapshot" => ok(crate::project::take_snapshot_impl(state)),
            "swap_snapshot" => ok(crate::project::swap_snapshot_impl(state, arg(args, "id")?)?),
            "drop_snapshot" => {
                state.undo_snapshots.lock().unwrap().remove(arg(args, "id")?);
                ok(())
            }
            "analyze_print3d" => ok(crate::commands::analyze_print3d_impl(state)?),
            "scale_mesh_for_print" => ok(crate::commands::scale_mesh_for_print_impl(arg(args, "params")?, state)?),
            "undo_print_scale" => ok(crate::commands::undo_print_scale_impl(state)?),
            "subdivide_mesh" => ok(crate::commands::subdivide_mesh_impl(arg(args, "config")?, state)?),
            "get_placement_info" => ok(crate::placement::placement_info(state)?),
            "apply_placement" => ok(crate::placement::apply_placement_impl(state, arg(args, "matrix")?)?),
            "get_supported_formats" => ok(crate::commands::get_supported_formats()),
            "remesh_preview" => ok(crate::remesh::preview_impl(state, arg(args, "params")?, &Channel::new(|_| Ok(())))?),
            "get_remesh_preview_data" => crate::remesh::preview_bytes(state).map(Reply::Bytes),
            "remesh_discard" => {
                *state.remesh_preview.lock().unwrap() = None;
                ok(())
            }
            "remesh_apply" => {
                let options = args.get("options").cloned().map(serde_json::from_value).transpose().map_err(|e| e.to_string())?;
                ok(crate::remesh::apply_impl(state, arg(args, "params")?, options.unwrap_or_default(), &Channel::new(|_| Ok(())))?)
            }
            "remesh_info" => ok(crate::remesh::info_impl(state)?),
            "export_model" => ok(crate::commands::export_model_impl(arg(args, "config")?, state)?),
            "remesh_voxel_grid" => ok(crate::remesh::voxel_grid_impl(state, arg(args, "voxelMm")?)?),
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
            link: None,
        }));
        doc
    }

    #[test]
    fn evaluate_caches_and_reports() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
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
    fn drawing_svg_to_pdf() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="297mm" height="210mm" viewBox="0 0 297 210"><rect x="10" y="10" width="277" height="190" fill="none" stroke="#000"/><text x="20" y="30" font-size="5">Plano ñandú</text></svg>"##;
        let pdf = svg_to_pdf(svg).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        // A4 apaisada: 297 × 210 mm = 841,89 × 595,28 pt
        let text = String::from_utf8_lossy(&pdf);
        let mb = text.find("MediaBox").map(|i| text[i..i + 60].to_string());
        assert!(text.contains("841.8") && text.contains("595.2"), "tamaño de página: {mb:?}");
    }

    #[test]
    fn compare_with_an_older_version() {
        if !cad_model::occt::available() {
            return;
        }
        // Antes 10×20×30; ahora 10×20×40: se agregaron 2000 mm³ y no se quitó nada
        let old = box_doc();
        let mut now = box_doc();
        if let FeatureKind::Primitive(p) = &mut now.features[0].kind {
            p.shape = cad_model::PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 40.0, centered: false, centered_z: false };
        }
        now.versions.push(cad_model::NamedVersion { name: "v1".into(), created: "2026-10-08T10:00:00Z".into(), note: None, document: old.clone() });
        let h = doc_hash(&now);
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(now)));
        let out = compare_impl(&state, &old).unwrap();
        let f = |i: usize| f64::from_le_bytes(out[i..i + 8].try_into().unwrap());
        assert!((f(0) - 2000.0).abs() < 1e-6, "agregado {}", f(0));
        assert!(f(8).abs() < 1e-6, "quitado {}", f(8));
        // Las versiones no cambian el diseño
        let mut doc = state.cad_doc().unwrap();
        doc.versions.clear();
        assert_eq!(doc_hash(&doc), h);
    }

    #[test]
    fn seat_on_a_clearance_hole() {
        if !cad_model::occt::available() {
            return;
        }
        // Placa de 40×40×10 con un agujero pasante de 6,6 (M6)
        let mut doc = Document::new();
        for (shape, op) in [
            (cad_model::PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false }, BodyOp::Join),
            (cad_model::PrimitiveShape::Cylinder { radius: 3.3, height: 20.0 }, BodyOp::Cut),
        ] {
            let origin = if op == BodyOp::Cut { [0.0, 0.0, -5.0] } else { [0.0; 3] };
            doc.add(FeatureKind::Primitive(cad_model::Primitive { shape, origin, z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op, link: None }));
        }
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(doc)));
        evaluate(&state).unwrap();
        let edges = {
            let cache = state.cad_cache.lock().unwrap();
            let body = cache.as_ref().unwrap().eval.body.clone().unwrap();
            body.edges().unwrap()
        };
        let rim = |zv: f64| edges.iter().position(|e| e.circle.is_some_and(|(c, _, _)| (c[2] - zv).abs() < 1e-6)).unwrap();
        let top = edge_seat_impl(&state, rim(10.0)).unwrap();
        assert_eq!(top.size, "M6");
        assert!((top.diameter - 6.6).abs() < 1e-9 && (top.z[2] - 1.0).abs() < 1e-9);
        assert!((top.depth.unwrap() - 10.0).abs() < 1e-3, "{:?}", top.depth);
        // Del otro lado sale hacia abajo
        let bottom = edge_seat_impl(&state, rim(0.0)).unwrap();
        assert!((bottom.z[2] + 1.0).abs() < 1e-9 && bottom.origin[2].abs() < 1e-9);
    }

    #[test]
    fn exports_compensated_for_printing() {
        if !cad_model::occt::available() {
            return;
        }
        use pinocchio_print3d::{Fit, MaterialProfile, PrinterProfile};
        // Una arandela M6 (agujero de 6,4 y 1,6 de alto) con una PLA de 0,3 % de contracción
        let mut doc = Document::new();
        doc.add(FeatureKind::Primitive(cad_model::Primitive {
            shape: cad_model::PrimitiveShape::Washer { size: "M6".into() },
            origin: [0.0; 3],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::Join,
            link: None,
        }));
        doc.fabrication = Some(cad_model::Fabrication {
            printer: PrinterProfile::new("t", [200.0; 3], 0.4, 0.2),
            material: MaterialProfile::presets().remove(0),
            fit: Fit::Normal,
            compensate_all: false,
        });
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(doc)));
        evaluate(&state).unwrap();
        let dir = std::env::temp_dir().join("pinocchio_cad_comp");
        std::fs::create_dir_all(&dir).unwrap();
        let volume = |compensate: bool, ext: &str| -> f64 {
            let path = dir.join(format!("w_{compensate}.{ext}"));
            export_impl(&state, path.to_str().unwrap(), ext, None, false, compensate).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            std::fs::remove_file(&path).ok();
            cad_model::Shape::from_step(&bytes).unwrap().mass().unwrap().volume
        };
        let m = cad_model::standard::size("M6").unwrap();
        let ring = |hole: f64| std::f64::consts::PI * (m.washer_d2 * m.washer_d2 - hole * hole) / 4.0 * m.washer_h;
        let plain = volume(false, "step");
        assert!((plain - ring(m.washer_d1)).abs() < 1e-6 * plain, "{plain}");
        // Agujero 0,2 más ancho y todo agrandado por 1 / (1 − 0,003) en los tres ejes
        let s = 1.0 / (1.0 - 0.003);
        let comp = volume(true, "step");
        assert!((comp - ring(m.washer_d1 + 0.2) * s * s * s).abs() < 1e-5 * comp, "{comp} vs {}", ring(m.washer_d1 + 0.2) * s * s * s);
        // Sin impresora no hay con qué compensar
        state.cad_tabs.lock().unwrap().active_design_mut().unwrap().fabrication = None;
        assert!(export_impl(&state, dir.join("x.step").to_str().unwrap(), "step", None, false, true).is_err());
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
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
        let base = evaluate(&state).unwrap();
        // Borrador con la caja más alta
        let mut draft = box_doc();
        if let FeatureKind::Primitive(p) = &mut draft.features[0].kind {
            p.shape = cad_model::PrimitiveShape::Box { dx: 10.0, dy: 20.0, dz: 60.0, centered: false, centered_z: false };
        }
        let r = preview_impl(&state, Some(draft)).unwrap();
        assert!((r.body.unwrap().volume - 12000.0).abs() < 1e-6);
        assert_eq!(state.cad_doc().as_ref(), Some(&box_doc()));
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
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
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
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(doc)));
        let tool = tool_mesh_impl(&state, id).unwrap();
        let triangles = u32::from_le_bytes(tool[4..8].try_into().unwrap());
        assert_eq!(triangles, 12, "la caja como herramienta");
        let none = tool_mesh_impl(&state, cad_model::FeatureId(99)).unwrap();
        assert_eq!(none, vec![0; 16]);
    }

    #[test]
    fn part_model_is_closed() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
        let mut scene = body_scene(&state, "Diseño").unwrap();
        weld_scene(&mut scene);
        let prims = scene.world_primitives();
        assert_eq!(prims[0].positions.len(), 8, "un cubo soldado tiene 8 vértices");
        let mesh = crate::commands::scene_to_pinocchio_mesh(&scene).unwrap();
        assert!(pinocchio_print3d::is_mesh_closed(&mesh));
        // La caja de 10 × 20 × 30 mm
        assert!((pinocchio_print3d::compute_volume(&mesh).abs() - 6000.0).abs() < 1e-3);
    }

    #[test]
    fn part_to_model_skips_unchanged_parts() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
        let channel = Channel::new(|_| Ok(()));
        let part = cad_model::PartId { feature: state.cad_doc().as_ref().unwrap().features[0].id, index: 0 };
        let first = part_to_model_impl(&state, part, 0, None, &channel).unwrap();
        assert!(first.changed);
        let again = part_to_model_impl(&state, part, 0, Some(first.hash.clone()), &channel).unwrap();
        assert!(!again.changed, "misma pieza: no recarga");
        state.cad_tabs.lock().unwrap().active_design_mut().unwrap().features[0].kind = FeatureKind::Primitive(cad_model::Primitive {
            shape: cad_model::PrimitiveShape::Box { dx: 40.0, dy: 20.0, dz: 30.0, centered: false, centered_z: false },
            origin: [0.0; 3],
            z: [0.0, 0.0, 1.0],
            x: [1.0, 0.0, 0.0],
            op: BodyOp::Join,
            link: None,
        });
        let changed = part_to_model_impl(&state, part, 0, Some(first.hash), &channel).unwrap();
        assert!(changed.changed, "la pieza cambió: malla nueva");
    }

    fn box_tab(dx: f64) -> cad_model::TabKind {
        let mut doc = box_doc();
        if let FeatureKind::Primitive(p) = &mut doc.features[0].kind {
            p.shape = cad_model::PrimitiveShape::Box { dx, dy: 20.0, dz: 30.0, centered: false, centered_z: false };
        }
        cad_model::TabKind::Design { document: Box::new(doc) }
    }

    /// Cambiar de pestaña muestra su diseño y guarda el recálculo de la otra
    #[test]
    fn switching_tabs_keeps_each_evaluation() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(Tabs::from_legacy(Some(box_doc())));
        let first = evaluate(&state).unwrap();
        assert!((first.body.as_ref().unwrap().volume - 6000.0).abs() < 1e-6);
        let view = change_tabs(&state, |t| {
            t.add(box_tab(40.0), Some("Larga".into()));
            Ok(())
        })
        .unwrap();
        assert_eq!((view.tabs.len(), view.active, view.tabs[1].name.as_str(), view.tabs[1].kind), (2, Some(1), "Larga", "design"));
        let second = evaluate(&state).unwrap();
        assert!((second.body.unwrap().volume - 24000.0).abs() < 1e-6);
        // Vuelta a la primera: su recálculo de antes (misma versión, sin recalcular)
        change_tabs(&state, |t| t.activate(0)).unwrap();
        assert_eq!(state.cad_cache.lock().unwrap().as_ref().unwrap().doc_hash, doc_hash(&box_doc()));
        assert_eq!(evaluate(&state).unwrap().version, first.version);
        // set_document cambia solo la activa
        let mut doc = box_doc();
        doc.parameters.push(cad_model::Parameter { name: "x".into(), expr: "1".into() });
        set_document_impl(&state, doc.clone()).unwrap();
        assert_eq!(state.cad_tabs.lock().unwrap().design(0), Some(&doc));
        assert_ne!(state.cad_tabs.lock().unwrap().design(1), Some(&doc));
        // Borrar la otra suelta su recálculo
        change_tabs(&state, |t| t.remove(1).map(|_| ())).unwrap();
        assert!(state.cad_tab_caches.lock().unwrap().is_empty());
        // Un ensamble activo se ve como un diseño sin operaciones (y no las recibe)
        change_tabs(&state, |t| {
            t.add_assembly(None);
            Ok(())
        })
        .unwrap();
        assert!(state.cad_doc().unwrap().features.is_empty());
        assert!(state.with_cad_doc(|_| ()).is_err());
        assert!(status_impl(&state).has_document);
    }

    /// Un molde que resta la pieza de otra pestaña sigue al original al volver a él
    #[test]
    fn derived_part_follows_its_tab() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(Tabs::from_legacy(Some(box_doc())));
        change_tabs(&state, |t| {
            t.add(box_tab(40.0), Some("Molde".into()));
            Ok(())
        })
        .unwrap();
        let derived = |studio| FeatureKind::Derived(cad_model::Derived { studio, parts: vec![], configuration: None, op: BodyOp::Cut, source: Default::default() });
        state.with_cad_doc(|d| d.add(derived(0))).unwrap();
        let r = evaluate(&state).unwrap();
        assert!(r.status.iter().all(|s| matches!(s.state, cad_model::FeatureState::Ok)), "{:?}", r.status);
        assert!((r.body.unwrap().volume - 18000.0).abs() < 1e-6);
        assert_eq!(tabs_view(&state.cad_tabs.lock().unwrap()).tabs[0].used_by, [1]);
        let parts = tab_parts_impl(&state, 0).unwrap();
        assert_eq!((parts.len(), parts[0].name.as_str()), (1, "Pieza 1"));
        // Cambia el original en su pestaña y se vuelve al molde
        change_tabs(&state, |t| t.activate(0)).unwrap();
        let mut longer = box_doc();
        let FeatureKind::Primitive(p) = &mut longer.features[0].kind else { panic!() };
        p.shape = cad_model::PrimitiveShape::Box { dx: 25.0, dy: 20.0, dz: 30.0, centered: false, centered_z: false };
        set_document_impl(&state, longer).unwrap();
        change_tabs(&state, |t| t.activate(1)).unwrap();
        let r = evaluate(&state).unwrap();
        assert!((r.body.unwrap().volume - 9000.0).abs() < 1e-6);
        // Que el molde traiga de sí mismo: ciclo
        state.with_cad_doc(|d| d.features[1].kind = derived(1)).unwrap();
        let r = evaluate(&state).unwrap();
        assert!(matches!(&r.status[1].state, cad_model::FeatureState::Error { message, .. } if message.contains("ciclo")), "{:?}", r.status[1]);
    }

    /// Una pestaña de ensamble ve las piezas de todos los diseños y guarda cada
    /// instancia con su pestaña
    #[test]
    fn assembly_tab_uses_parts_of_every_design() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        let mut tabs = Tabs::from_legacy(Some(box_doc()));
        let long = tabs.add(box_tab(40.0), Some("Larga".into()));
        let asm = tabs.add_assembly(None);
        state.set_cad_tabs(tabs);
        // Lo que ve la interfaz: un diseño sin operaciones, sin sólido propio, con las piezas de los dos diseños
        let r = evaluate(&state).unwrap();
        assert!(r.body.is_none());
        let volumes: Vec<f64> = r.parts.iter().map(|p| p.volume.round()).collect();
        assert_eq!(volumes, [6000.0, 24000.0]);
        assert_ne!(r.parts[0].id, r.parts[1].id, "la misma operación de dos pestañas: piezas distintas");
        // Insertar las dos como lo hace la interfaz (con los ids que vio) y guardar
        let inst = |id, part, x: f64| cad_model::Instance { id, studio: 0, part, name: format!("I{id}"), position: [x, 0.0, 0.0], rotation: [0.0; 3], fixed: id == 1 };
        let mut doc = state.cad_doc().unwrap();
        doc.assembly = Some(cad_model::Assembly { instances: vec![inst(1, r.parts[0].id, 0.0), inst(2, r.parts[1].id, 50.0)], next_id: 3, ..Default::default() });
        let saved = set_document_impl(&state, doc).unwrap();
        assert_eq!(saved.assembly.unwrap().solution.poses.len(), 2);
        {
            let tabs = state.cad_tabs.lock().unwrap();
            let cad_model::TabKind::Assembly { assembly, .. } = &tabs.get(asm).unwrap().kind else { panic!() };
            let feature = tabs.design(long).unwrap().features[0].id;
            assert_eq!((assembly.instances[1].studio, assembly.instances[1].part.feature), (long, feature), "su pestaña y su operación de siempre");
            assert_eq!(tabs.users(long), [asm]);
        }
        // Las formas del ensamble: una caja de cada diseño, y su malla
        let (_, shapes) = assembly_shapes(&state).unwrap();
        let mut v: Vec<f64> = shapes.iter().map(|(_, s)| s.mass().unwrap().volume.round()).collect();
        v.sort_by(f64::total_cmp);
        assert_eq!(v, [6000.0, 24000.0]);
        assert!(!assembly_mesh_impl(&state, 0.0).unwrap().is_empty());
        // Un ensamble no recibe operaciones
        assert!(state.with_cad_doc(|_| ()).is_err());
        // Cambiar un diseño cambia el ensamble al volver
        change_tabs(&state, |t| t.activate(long)).unwrap();
        let mut d = state.cad_doc().unwrap();
        if let FeatureKind::Primitive(p) = &mut d.features[0].kind {
            p.shape = cad_model::PrimitiveShape::Box { dx: 50.0, dy: 20.0, dz: 30.0, centered: false, centered_z: false };
        }
        set_document_impl(&state, d).unwrap();
        change_tabs(&state, |t| t.activate(asm)).unwrap();
        assert_eq!(evaluate(&state).unwrap().parts[1].volume.round(), 30000.0);
    }

    /// El plano de una pestaña de ensamble dibuja las instancias en su lugar y se guarda en ella
    #[test]
    fn assembly_tab_keeps_its_drawing() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        let mut tabs = Tabs::from_legacy(Some(box_doc()));
        tabs.add(box_tab(40.0), Some("Larga".into()));
        let asm = tabs.add_assembly(None);
        state.set_cad_tabs(tabs);
        let r = evaluate(&state).unwrap();
        let inst = |id, part, x: f64| cad_model::Instance { id, studio: 0, part, name: format!("I{id}"), position: [x, 0.0, 0.0], rotation: [0.0; 3], fixed: true };
        let mut doc = state.cad_doc().unwrap();
        doc.assembly = Some(cad_model::Assembly { instances: vec![inst(1, r.parts[0].id, 0.0), inst(2, r.parts[1].id, 50.0)], next_id: 3, ..Default::default() });
        let sheets = serde_json::json!({ "sheets": [{ "id": 1, "name": "Plano 1", "bom": true }], "next_id": 2 });
        doc.drawing = Some(sheets.clone());
        set_document_impl(&state, doc).unwrap();
        // Caja y centros de las instancias en su lugar
        let info = drawing_info_impl(&state).unwrap();
        assert!((info.min[0] - 0.0).abs() < 1e-6 && (info.max[0] - 90.0).abs() < 1e-6, "{:?} {:?}", info.min, info.max);
        let mut centers: Vec<(u32, f64)> = info.items.iter().map(|i| (i.instance, i.center[0])).collect();
        centers.sort_by_key(|c| c.0);
        assert_eq!(centers, [(1, 5.0), (2, 70.0)]);
        // La vista de frente va de una punta del ensamble a la otra; el corte se raya por pieza
        let front = DrawingViewSpec { eye: [0.0, -1.0, 0.0], xdir: [1.0, 0.0, 0.0], section: Some(DrawingSection { origin: [0.0, 10.0, 0.0], normal: [0.0, 1.0, 0.0] }), cut: vec![], shaded: false };
        let v = drawing_impl(&state, &[front]).unwrap().remove(0);
        let xs = v.lines.iter().flat_map(|l| l.points.iter().map(|p| p[0]));
        let (lo, hi) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
        assert!(lo.abs() < 1e-3 && (hi - 90.0).abs() < 1e-3, "{lo} {hi}");
        let mut which = v.hatch_part.clone();
        which.dedup();
        assert_eq!(which, [0, 1], "cada instancia con la pieza de la que viene");
        // El plano queda en la pestaña y vuelve al volver a ella
        change_tabs(&state, |t| t.activate(0)).unwrap();
        assert!(state.cad_doc().unwrap().drawing.is_none());
        change_tabs(&state, |t| t.activate(asm)).unwrap();
        assert_eq!(state.cad_doc().unwrap().drawing, Some(sheets));
    }

    /// La malla de una pieza de otra pestaña sale sin cambiar lo que muestra el visor
    #[test]
    fn part_of_another_tab_becomes_a_model() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        let mut tabs = Tabs::from_legacy(Some(box_doc()));
        let other = tabs.add(box_tab(40.0), None);
        tabs.activate(0).unwrap();
        state.set_cad_tabs(tabs);
        let shown = evaluate(&state).unwrap().version;
        let part = cad_model::PartId { feature: state.cad_doc().unwrap().features[0].id, index: 0 };
        let channel = Channel::new(|_| Ok(()));
        let made = part_to_model_impl(&state, part, other, None, &channel).unwrap();
        let info = made.info.unwrap();
        // Caja de 40×20×30 (la de la pestaña activa es de 10×20×30): el lado más largo es el doble del más corto
        let size: Vec<f32> = (0..3).map(|i| info.bounding_box.max[i] - info.bounding_box.min[i]).collect();
        let (lo, hi) = size.iter().fold((f32::INFINITY, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!((hi / lo - 2.0).abs() < 1e-3, "{size:?}");
        assert_eq!(evaluate(&state).unwrap().version, shown, "el visor no se tocó");
        assert!(part_to_model_impl(&state, part, 7, None, &channel).is_err());
    }

    #[test]
    fn body_scene_is_y_up_in_mm() {
        if !cad_model::occt::available() {
            return;
        }
        let state = AppState::new();
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
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
        state.set_cad_tabs(cad_model::Tabs::from_legacy(Some(box_doc())));
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
