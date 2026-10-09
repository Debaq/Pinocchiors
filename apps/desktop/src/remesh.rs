//! Remallar: los modos que no son la retopología (ver
//! `libs/quadriflow/PLAN_REMALLAR.md`): Simplificar, Suavizar, Isótropo y
//! Vóxeles.
//!
//! La vista previa calcula sin tocar el modelo y queda guardada hasta
//! aplicarla o descartarla; aplicar con los mismos parámetros sobre la misma
//! malla la reusa, si no recalcula (así también se rehace sola cuando la
//! malla sale de una pieza del diseño que cambió).

use crate::commands::{
    calculate_scene_bounds, calculate_scene_stats, in_background, mesh_replaced, report, scene_mesh_data,
    scene_to_pinocchio_mesh, MeshInfo, Progress,
};
use crate::state::AppState;
use converter_scene::{IndexData, Primitive, Scene, VertexAttribute};
use pinocchio_mesh::Mesh;
use quadriflow_core::remesh::{self, IsotropicOptions, SimplifyInput, SimplifyOptions, SmoothOptions, TriMesh, VoxelOptions};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, State};

/// Modo y sus parámetros
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RemeshParams {
    Simplify(SimplifyParams),
    Smooth(SmoothParams),
    Isotropic(IsotropicParams),
    Voxel(VoxelParams),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SimplifyParams {
    /// Fracción de los triángulos que quedan (0–1)
    pub ratio: f64,
    /// Desviación máxima, en % de la diagonal del modelo (`None`: sin límite)
    pub max_error_percent: Option<f64>,
    /// No mover los bordes abiertos
    #[serde(default)]
    pub lock_borders: bool,
    /// No colapsar a través de las costuras de UV, normales y materiales
    #[serde(default = "yes")]
    pub keep_seams: bool,
    /// Llegar al objetivo aunque cambie la topología
    #[serde(default)]
    pub aggressive: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SmoothParams {
    /// Pasadas del filtro
    pub iterations: usize,
    /// Cuánto se acerca cada vértice al promedio de sus vecinos (0–1)
    pub strength: f64,
    /// Respetar las aristas vivas desde este ángulo entre caras, en grados
    #[serde(default)]
    pub sharp_angle: Option<f64>,
    /// No mover los bordes abiertos
    #[serde(default)]
    pub fix_borders: bool,
    /// Mover los vértices solo según la normal (la textura no se corre)
    #[serde(default = "yes")]
    pub normal_only: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IsotropicParams {
    /// Lado de los triángulos, en mm
    pub edge_mm: f64,
    /// Conservar las aristas vivas desde este ángulo entre caras, en grados
    #[serde(default)]
    pub sharp_angle: Option<f64>,
    #[serde(default = "five")]
    pub iterations: usize,
    /// Triángulos más chicos en las partes delgadas
    #[serde(default)]
    pub thin_features: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoxelParams {
    /// Lado del vóxel, en mm
    pub voxel_mm: f64,
    /// Pasadas de suavizado después
    #[serde(default)]
    pub smooth_iterations: usize,
    /// Isótropo después con este lado, en mm
    #[serde(default)]
    pub isotropic_edge_mm: Option<f64>,
}

fn five() -> usize {
    5
}

fn yes() -> bool {
    true
}

/// Vista previa calculada, a la espera de aplicarse
pub struct Preview {
    params: RemeshParams,
    /// Huella de la malla de la que salió (si cambió, no se aplica)
    source: u64,
    scene: Scene,
    stats: RemeshStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshCounts {
    pub vertices: usize,
    pub triangles: usize,
}

/// Cuánto se aleja el resultado del original
#[derive(Debug, Clone, Serialize)]
pub struct DeviationInfo {
    pub max_percent: f64,
    pub mean_percent: f64,
    pub max_mm: f64,
    pub mean_mm: f64,
}

/// Antes → después
#[derive(Debug, Clone, Serialize)]
pub struct RemeshStats {
    pub before: MeshCounts,
    pub after: MeshCounts,
    pub deviation: DeviationInfo,
}

/// La malla del modelo, para proponer parámetros (en mm)
#[derive(Debug, Clone, Serialize)]
pub struct RemeshInfo {
    pub extent_mm: f64,
    pub mean_edge_mm: f64,
    pub area_mm2: f64,
    pub triangles: usize,
    /// Isótropo la necesita manifold
    pub manifold: bool,
}

/// La grilla que usaría Vóxeles
#[derive(Debug, Clone, Serialize)]
pub struct VoxelGridInfo {
    /// Lado que se usa de verdad (el pedido, dentro de los límites)
    pub voxel_mm: f64,
    pub dims: [usize; 3],
    pub triangles: usize,
    pub memory_mb: f64,
}

/// Remallado aplicado al modelo
#[derive(Debug, Clone, Serialize)]
pub struct RemeshApplied {
    pub mesh_info: MeshInfo,
    pub stats: RemeshStats,
    /// Había rig y pasó a la malla nueva
    pub rig_kept: bool,
}

// ─── Comandos ──────────────────────────────────────────────────────────────

/// Calcula el remallado sin tocar el modelo
#[tauri::command]
pub async fn remesh_preview(app: AppHandle, params: RemeshParams, on_progress: Channel<Progress>) -> Result<RemeshStats, String> {
    in_background(app, move |state| preview_impl(state, params, &on_progress)).await
}

/// La vista previa para el visor (mismo formato que `get_mesh_data`)
#[tauri::command]
pub async fn get_remesh_preview_data(app: AppHandle) -> Result<Response, String> {
    in_background(app, |state| preview_bytes(state)).await.map(Response::new)
}

/// Tamaño y arista media de la malla, y si Isótropo puede con ella
#[tauri::command]
pub async fn remesh_info(app: AppHandle) -> Result<RemeshInfo, String> {
    in_background(app, info_impl).await
}

/// La grilla y la memoria que usaría Vóxeles con vóxeles de `voxel_mm`
#[tauri::command]
pub async fn remesh_voxel_grid(app: AppHandle, voxel_mm: f64) -> Result<VoxelGridInfo, String> {
    in_background(app, move |state| voxel_grid_impl(state, voxel_mm)).await
}

/// Descarta la vista previa
#[tauri::command]
pub fn remesh_discard(state: State<'_, AppState>) {
    *state.remesh_preview.lock().unwrap() = None;
}

/// Reemplaza la malla del modelo por la remallada
#[tauri::command]
pub async fn remesh_apply(app: AppHandle, params: RemeshParams, on_progress: Channel<Progress>) -> Result<RemeshApplied, String> {
    in_background(app, move |state| apply_impl(state, params, &on_progress)).await
}

pub(crate) fn info_impl(state: &AppState) -> Result<RemeshInfo, String> {
    let (scene, _) = current(state)?;
    let (positions, indices) = WorldSurface::of(&scene).buffers();
    let s = remesh::surface_stats(&positions, &indices);
    let mm = scene.meters_per_unit * 1000.0;
    Ok(RemeshInfo {
        extent_mm: s.extent * mm,
        mean_edge_mm: s.mean_edge * mm,
        area_mm2: s.area * mm * mm,
        triangles: indices.len() / 3,
        manifold: s.manifold,
    })
}

pub(crate) fn voxel_grid_impl(state: &AppState, voxel_mm: f64) -> Result<VoxelGridInfo, String> {
    let (scene, _) = current(state)?;
    let (positions, indices) = WorldSurface::of(&scene).buffers();
    let mm = scene.meters_per_unit * 1000.0;
    let g = remesh::voxel_grid(&positions, &indices, voxel_mm / mm);
    Ok(VoxelGridInfo { voxel_mm: g.voxel_size * mm, dims: g.dims, triangles: g.triangles, memory_mb: g.memory as f64 / 1e6 })
}

pub(crate) fn preview_impl(state: &AppState, params: RemeshParams, progress: &Channel<Progress>) -> Result<RemeshStats, String> {
    let _guard = state.try_begin_processing().ok_or("Ya hay un proceso en curso")?;
    let (scene, source) = current(state)?;
    let (result, stats) = compute(&scene, &params, progress)?;
    *state.remesh_preview.lock().unwrap() = Some(Preview { params, source, scene: result, stats: stats.clone() });
    Ok(stats)
}

pub(crate) fn preview_bytes(state: &AppState) -> Result<Vec<u8>, String> {
    let preview = state.remesh_preview.lock().unwrap();
    let preview = preview.as_ref().ok_or("No hay vista previa")?;
    Ok(scene_mesh_data(&preview.scene).to_bytes())
}

pub(crate) fn apply_impl(state: &AppState, params: RemeshParams, progress: &Channel<Progress>) -> Result<RemeshApplied, String> {
    let _guard = state.try_begin_processing().ok_or("Ya hay un proceso en curso")?;
    let (scene, source) = current(state)?;
    let preview = state.remesh_preview.lock().unwrap().take().filter(|p| p.params == params && p.source == source);
    let (scene, stats) = match preview {
        Some(p) => (p.scene, p.stats),
        None => compute(&scene, &params, progress)?,
    };
    report(progress, "apply", 95, "Reemplazando la malla...");
    let mesh = scene_to_pinocchio_mesh(&scene)?;
    let (num_vertices, num_faces, has_normals, has_uvs) = calculate_scene_stats(&scene);
    let mesh_info = MeshInfo {
        num_vertices,
        num_faces,
        num_meshes: scene.meshes.len(),
        has_normals,
        has_uvs,
        has_materials: !scene.materials.is_empty(),
        bounding_box: calculate_scene_bounds(&scene),
        format: "REMESHED".to_string(),
        rig: None,
        textures: Vec::new(),
        textures_skipped: Vec::new(),
    };
    let previous = state.mesh.lock().unwrap().replace(mesh);
    *state.scene.lock().unwrap() = Some(scene);
    // Los respaldos de reparar, desplegar y escalar son de antes: deshacerlos
    // descartaría el remallado (para eso está el historial)
    *state.diagnostics.lock().unwrap() = None;
    *state.mesh_before_repair.lock().unwrap() = None;
    *state.scene_before_repair.lock().unwrap() = None;
    *state.mesh_before_unwrap.lock().unwrap() = None;
    *state.scene_before_unwrap.lock().unwrap() = None;
    *state.mesh_before_print_scale.lock().unwrap() = None;
    *state.scene_before_print_scale.lock().unwrap() = None;
    let rig_kept = match previous {
        Some(previous) => mesh_replaced(state, &previous),
        None => {
            state.geometry_changed();
            false
        }
    };
    Ok(RemeshApplied { mesh_info, stats, rig_kept })
}

// ─── Cálculo ───────────────────────────────────────────────────────────────

/// La escena del modelo y la huella de su malla
fn current(state: &AppState) -> Result<(Scene, u64), String> {
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay modelo cargado")?;
    let source = fingerprint(state.mesh.lock().unwrap().as_ref().ok_or("No hay malla cargada")?);
    Ok((scene, source))
}

fn fingerprint(mesh: &Mesh) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in &mesh.vertices {
        [v.position.x(), v.position.y(), v.position.z()].map(f64::to_bits).hash(&mut h);
    }
    for f in 0..mesh.num_faces() {
        mesh.get_face_vertices(f).hash(&mut h);
    }
    h.finish()
}

fn compute(scene: &Scene, params: &RemeshParams, progress: &Channel<Progress>) -> Result<(Scene, RemeshStats), String> {
    let before = WorldSurface::of(scene);
    if before.triangles.is_empty() {
        return Err("El modelo no tiene triángulos".into());
    }
    let result = match params {
        RemeshParams::Simplify(p) => {
            report(progress, "simplify", 5, "Simplificando...");
            simplify_scene(scene, p, &before)
        }
        RemeshParams::Smooth(p) => {
            report(progress, "smooth", 5, "Suavizando...");
            smooth_scene(scene, p)
        }
        RemeshParams::Isotropic(p) => {
            report(progress, "isotropic", 5, "Remallando con triángulos parejos...");
            let (positions, indices) = before.buffers();
            let mm = scene.meters_per_unit * 1000.0;
            let options = IsotropicOptions {
                edge_length: p.edge_mm / mm,
                sharp_angle: p.sharp_angle,
                iterations: p.iterations,
                thin_features: p.thin_features,
            };
            let mesh = remesh::isotropic(&positions, &indices, &options).map_err(|e| e.to_string())?;
            replace_geometry(scene, &mesh, p.sharp_angle)
        }
        RemeshParams::Voxel(p) => {
            report(progress, "voxel", 5, "Rehaciendo la superficie desde el volumen...");
            let (positions, indices) = before.buffers();
            let mm = scene.meters_per_unit * 1000.0;
            let options = VoxelOptions {
                voxel_size: p.voxel_mm / mm,
                smooth_iterations: p.smooth_iterations,
                isotropic_edge: p.isotropic_edge_mm.map(|e| e / mm),
            };
            let mesh = remesh::voxel(&positions, &indices, &options).map_err(|e| e.to_string())?;
            replace_geometry(scene, &mesh, None)
        }
    };
    report(progress, "deviation", 70, "Midiendo cuánto se aleja del original...");
    let after = WorldSurface::of(&result);
    let d = remesh::deviation((&before.positions, &before.triangles), (&after.positions, &after.triangles));
    let mm = scene.meters_per_unit * 1000.0;
    let counts = |s: &Scene, w: &WorldSurface| MeshCounts { vertices: calculate_scene_stats(s).0, triangles: w.triangles.len() };
    let stats = RemeshStats {
        before: counts(scene, &before),
        after: counts(&result, &after),
        deviation: DeviationInfo {
            max_percent: d.max_percent(),
            mean_percent: d.mean_percent(),
            max_mm: d.max * mm,
            mean_mm: d.mean * mm,
        },
    };
    Ok((result, stats))
}

/// Toda la superficie en espacio mundo
struct WorldSurface {
    positions: Vec<[f64; 3]>,
    triangles: Vec<[usize; 3]>,
    /// Diagonal de la caja de cada malla de la escena (en mundo, primera instancia)
    mesh_diagonals: Vec<Option<f64>>,
}

impl WorldSurface {
    fn of(scene: &Scene) -> Self {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let mut boxes: Vec<Option<([f64; 3], [f64; 3])>> = vec![None; scene.meshes.len()];
        let mut first_instance = vec![None; scene.meshes.len()];
        for p in scene.world_primitives() {
            let offset = positions.len();
            positions.extend(p.positions.iter().map(|v| v.map(f64::from)));
            triangles.extend(p.triangles.iter().map(|t| t.map(|i| i as usize + offset)));
            // Solo la primera instancia de cada malla
            if *first_instance[p.mesh].get_or_insert(p.instance) == p.instance {
                let b = &mut boxes[p.mesh];
                for v in &p.positions {
                    let v = v.map(f64::from);
                    let (lo, hi) = b.get_or_insert((v, v));
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
            }
        }
        let mesh_diagonals = boxes.into_iter().map(|b| b.map(|(lo, hi)| diagonal(lo, hi))).collect();
        Self { positions, triangles, mesh_diagonals }
    }

    /// Posiciones e índices para los modos que rehacen la malla
    fn buffers(&self) -> (Vec<[f32; 3]>, Vec<u32>) {
        (
            self.positions.iter().map(|p| p.map(|c| c as f32)).collect(),
            self.triangles.iter().flat_map(|t| t.map(|i| i as u32)).collect(),
        )
    }

    fn diagonal(&self) -> f64 {
        bounds(self.positions.iter().copied()).map_or(0.0, |(lo, hi)| diagonal(lo, hi))
    }
}

fn bounds(points: impl Iterator<Item = [f64; 3]>) -> Option<([f64; 3], [f64; 3])> {
    points.fold(None, |b, v| {
        let (mut lo, mut hi) = b.unwrap_or((v, v));
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
        Some((lo, hi))
    })
}

fn diagonal(lo: [f64; 3], hi: [f64; 3]) -> f64 {
    (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt()
}

/// Peso de las UV y de las normales frente a la posición al simplificar
const UV_WEIGHT: f32 = 1.0;
const NORMAL_WEIGHT: f32 = 0.5;

/// Simplifica cada malla de la escena (en su espacio local, así las
/// instancias siguen compartiéndola). Las primitivas de una malla se
/// simplifican juntas: el límite entre ellas queda como una costura, sin
/// rajarse. Cada vértice que queda conserva todos sus atributos.
fn simplify_scene(scene: &Scene, params: &SimplifyParams, world: &WorldSurface) -> Scene {
    let ratio = params.ratio.clamp(0.0, 1.0);
    // El error se pide en % del modelo entero, en mundo
    let max_error = params.max_error_percent.map(|p| p / 100.0 * world.diagonal());
    let mut out = scene.clone();
    for (m, mesh) in out.meshes.iter_mut().enumerate() {
        let joined = Joined::of(&mesh.primitives);
        let count = joined.indices.len() / 3;
        if count == 0 {
            continue;
        }
        // Error en unidades locales de la malla (los nodos pueden escalar)
        let local_diagonal = bounds(joined.positions.iter().map(|p| p.map(f64::from))).map_or(0.0, |(lo, hi)| diagonal(lo, hi));
        let to_local = match world.mesh_diagonals.get(m).copied().flatten() {
            Some(w) if w > 0.0 => local_diagonal / w,
            _ => 1.0,
        };
        let options = SimplifyOptions {
            target_triangles: ((count as f64 * ratio).round() as usize).max(1),
            max_error: max_error.map(|e| (e * to_local) as f32),
            lock_borders: params.lock_borders,
            keep_seams: params.keep_seams,
            aggressive: params.aggressive,
        };
        let input = SimplifyInput {
            positions: &joined.positions,
            indices: &joined.indices,
            attributes: &joined.attributes,
            attribute_weights: &joined.weights,
            groups: &joined.groups,
        };
        let result = remesh::simplify(&input, &options);
        mesh.primitives = joined.split(&mesh.primitives, &result.indices);
    }
    out
}

/// La escena con una sola malla nueva en lugar de todas (en espacio mundo:
/// los nodos quedan en la identidad). Sin UV ni piel: el material del
/// modelo queda sin texturas y el esqueleto importado y sus animaciones se
/// descartan (el rig de la app pasa aparte, ver `mesh_replaced`). Las
/// normales se parten donde las caras forman más de `sharp_angle` grados.
fn replace_geometry(scene: &Scene, mesh: &TriMesh, sharp_angle: Option<f64>) -> Scene {
    let (positions, normals, indices) = split_normals(mesh, sharp_angle);
    let material = scene.materials.first().map(|m| converter_scene::Material {
        base_color_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        occlusion_texture: None,
        emissive_texture: None,
        ..m.clone()
    });
    let mut out = Scene::new();
    out.meters_per_unit = scene.meters_per_unit;
    out.y_up = scene.y_up;
    out.materials.extend(material);
    let name = scene.meshes.first().map_or_else(|| "Malla".to_string(), |m| m.name.clone());
    out.meshes.push(converter_scene::Mesh {
        name: name.clone(),
        primitives: vec![Primitive {
            attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
            indices: Some(IndexData::U32(indices)),
            material: (!out.materials.is_empty()).then_some(0),
        }],
    });
    out.nodes.push(converter_scene::Node {
        name,
        transform: converter_scene::Transform::identity(),
        mesh: Some(0),
        skin: None,
        children: Vec::new(),
    });
    out.root_nodes.push(0);
    out
}

/// Normales por esquina: promedio (por área) de las caras del vértice que
/// no forman más de `sharp_angle` grados con la cara de la esquina. Cada
/// normal distinta de un vértice da un vértice aparte.
fn split_normals(mesh: &TriMesh, sharp_angle: Option<f64>) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let p = &mesh.positions;
    let faces: Vec<[u32; 3]> = mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    let face_normal = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|i| p[i as usize].map(f64::from));
        let (u, w) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
        [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
    };
    let normals: Vec<[f64; 3]> = faces.iter().map(face_normal).collect();
    let unit = |n: [f64; 3]| {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.0 { n.map(|c| c / len) } else { n }
    };
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); p.len()];
    for (f, t) in faces.iter().enumerate() {
        for &v in t {
            around[v as usize].push(f as u32);
        }
    }
    let cos = sharp_angle.map(|a| a.to_radians().cos());
    let mut out_p = Vec::with_capacity(p.len());
    let mut out_n = Vec::with_capacity(p.len());
    let mut index: std::collections::HashMap<(u32, [u32; 3]), u32> = std::collections::HashMap::with_capacity(p.len());
    let mut out_i = Vec::with_capacity(mesh.indices.len());
    for (f, t) in faces.iter().enumerate() {
        let own = unit(normals[f]);
        for &v in t {
            let mut sum = [0.0; 3];
            for &g in &around[v as usize] {
                let n = normals[g as usize];
                let smooth = cos.is_none_or(|c| {
                    let u = unit(n);
                    u[0] * own[0] + u[1] * own[1] + u[2] * own[2] >= c
                });
                if smooth {
                    for k in 0..3 {
                        sum[k] += n[k];
                    }
                }
            }
            let n = unit(sum).map(|c| c as f32);
            let key = (v, n.map(f32::to_bits));
            let i = *index.entry(key).or_insert_with(|| {
                out_p.push(p[v as usize]);
                out_n.push(n);
                out_p.len() as u32 - 1
            });
            out_i.push(i);
        }
    }
    (out_p, out_n, out_i)
}

/// Suaviza cada malla de la escena (en su espacio local). Las primitivas de
/// una malla se suavizan juntas, así el límite entre ellas no se raja; la
/// conectividad, las UV y los pesos no cambian. Las normales se rehacen.
fn smooth_scene(scene: &Scene, params: &SmoothParams) -> Scene {
    let options = SmoothOptions {
        iterations: params.iterations,
        strength: params.strength as f32,
        sharp_angle: params.sharp_angle.map(|a| a as f32),
        fix_borders: params.fix_borders,
        normal_only: params.normal_only,
    };
    let mut out = scene.clone();
    for mesh in &mut out.meshes {
        let joined = Joined::of(&mesh.primitives);
        if joined.indices.is_empty() {
            continue;
        }
        let smoothed = remesh::smooth(&joined.positions, &joined.indices, &options);
        let new_normals = recompute_normals(&mesh.primitives, &joined, &smoothed);
        for (k, prim) in mesh.primitives.iter_mut().enumerate() {
            let offset = joined.offsets[k] as usize;
            for a in &mut prim.attributes {
                match a {
                    VertexAttribute::Positions(p) => {
                        let n = p.len();
                        p.copy_from_slice(&smoothed[offset..offset + n]);
                    }
                    VertexAttribute::Normals(nr) => {
                        for (v, n) in nr.iter_mut().enumerate() {
                            *n = new_normals[offset + v];
                        }
                    }
                    _ => {}
                }
            }
            // Las tangentes siguen perpendiculares a la normal nueva
            let prim_normals: Vec<[f32; 3]> = normals(prim).cloned().unwrap_or_default();
            for a in &mut prim.attributes {
                if let VertexAttribute::Tangents(t) = a {
                    for (v, t) in t.iter_mut().enumerate() {
                        if let Some(n) = prim_normals.get(v) {
                            *t = orthogonal_tangent(*t, *n);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Normales de los vértices sobre las posiciones nuevas. Los vértices con la
/// misma posición y la misma normal de antes comparten la nueva (suave a
/// través de las costuras de UV y entre primitivas); los que tenían normales
/// distintas siguen partidos (aristas vivas).
fn recompute_normals(primitives: &[Primitive], joined: &Joined, positions: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let old: Vec<[f32; 3]> = primitives
        .iter()
        .flat_map(|prim| {
            let n = self::positions(prim).map_or(0, Vec::len);
            (0..n).map(move |v| normals(prim).and_then(|nr| nr.get(v)).copied().unwrap_or([0.0; 3]))
        })
        .collect();
    let bits = |v: [f32; 3]| v.map(|f| if f == 0.0 { 0 } else { f.to_bits() });
    let mut group_of = std::collections::HashMap::new();
    let groups: Vec<usize> = (0..positions.len())
        .map(|v| {
            let next = group_of.len();
            *group_of.entry((bits(joined.positions[v]), bits(old[v]))).or_insert(next)
        })
        .collect();
    let mut sum = vec![[0.0f64; 3]; group_of.len()];
    for t in joined.indices.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| positions[i as usize].map(f64::from));
        let (u, w) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        for &v in t {
            let g = &mut sum[groups[v as usize]];
            for k in 0..3 {
                g[k] += n[k];
            }
        }
    }
    groups
        .iter()
        .enumerate()
        .map(|(v, &g)| {
            let n = sum[g];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 { n.map(|c| (c / len) as f32) } else { old[v] }
        })
        .collect()
}

/// La tangente sin su componente según la normal (conserva el signo en w)
fn orthogonal_tangent(t: [f32; 4], n: [f32; 3]) -> [f32; 4] {
    let d = t[0] * n[0] + t[1] * n[1] + t[2] * n[2];
    let v = [t[0] - d * n[0], t[1] - d * n[1], t[2] - d * n[2]];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-12 { [v[0] / len, v[1] / len, v[2] / len, t[3]] } else { t }
}

/// Las primitivas de una malla en un solo búfer
struct Joined {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    attributes: Vec<f32>,
    weights: Vec<f32>,
    /// Primitiva de cada vértice
    groups: Vec<u32>,
    /// Primer vértice de cada primitiva en el búfer
    offsets: Vec<u32>,
}

impl Joined {
    fn of(primitives: &[Primitive]) -> Self {
        let has_uv = primitives.iter().any(|p| uv0(p).is_some());
        let has_normals = primitives.iter().any(|p| normals(p).is_some());
        let mut weights = Vec::new();
        if has_uv {
            weights.extend([UV_WEIGHT; 2]);
        }
        if has_normals {
            weights.extend([NORMAL_WEIGHT; 3]);
        }
        let mut j = Joined { positions: Vec::new(), indices: Vec::new(), attributes: Vec::new(), weights, groups: Vec::new(), offsets: Vec::new() };
        for (k, prim) in primitives.iter().enumerate() {
            let offset = j.positions.len() as u32;
            j.offsets.push(offset);
            let Some(positions) = positions(prim) else { continue };
            let n = positions.len();
            j.positions.extend_from_slice(positions);
            j.groups.extend(std::iter::repeat_n(k as u32, n));
            for v in 0..n {
                if has_uv {
                    j.attributes.extend(uv0(prim).and_then(|uv| uv.get(v)).copied().unwrap_or([0.0; 2]));
                }
                if has_normals {
                    j.attributes.extend(normals(prim).and_then(|nr| nr.get(v)).copied().unwrap_or([0.0; 3]));
                }
            }
            let indices = indices_u32(prim, n);
            j.indices.extend(indices.chunks_exact(3).filter(|t| t.iter().all(|&i| (i as usize) < n)).flatten().map(|&i| i + offset));
        }
        j
    }

    /// Reparte los triángulos que quedaron entre sus primitivas, cada una
    /// con solo los vértices que usa (en orden de primer uso)
    fn split(&self, primitives: &[Primitive], indices: &[u32]) -> Vec<Primitive> {
        let mut per: Vec<Vec<u32>> = vec![Vec::new(); primitives.len()];
        for t in indices.chunks_exact(3) {
            per[self.groups[t[0] as usize] as usize].extend_from_slice(t);
        }
        primitives
            .iter()
            .zip(per)
            .enumerate()
            .filter(|(_, (_, tris))| !tris.is_empty())
            .map(|(k, (prim, tris))| {
                let offset = self.offsets[k];
                let mut remap = std::collections::HashMap::new();
                let mut kept = Vec::new();
                let local: Vec<u32> = tris
                    .iter()
                    .map(|&i| {
                        let v = (i - offset) as usize;
                        *remap.entry(v).or_insert_with(|| {
                            kept.push(v);
                            kept.len() as u32 - 1
                        })
                    })
                    .collect();
                let mut prim = prim.clone();
                reindex(&mut prim, &kept);
                prim.indices = Some(if kept.len() <= u16::MAX as usize + 1 {
                    IndexData::U16(local.iter().map(|&i| i as u16).collect())
                } else {
                    IndexData::U32(local)
                });
                prim
            })
            .collect()
    }
}

fn positions(prim: &Primitive) -> Option<&Vec<[f32; 3]>> {
    prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Positions(p) => Some(p),
        _ => None,
    })
}

fn normals(prim: &Primitive) -> Option<&Vec<[f32; 3]>> {
    prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::Normals(n) => Some(n),
        _ => None,
    })
}

fn uv0(prim: &Primitive) -> Option<&Vec<[f32; 2]>> {
    prim.attributes.iter().find_map(|a| match a {
        VertexAttribute::TexCoords(0, uv) => Some(uv),
        _ => None,
    })
}

fn indices_u32(prim: &Primitive, vertices: usize) -> Vec<u32> {
    match &prim.indices {
        Some(IndexData::U16(i)) => i.iter().map(|&i| i as u32).collect(),
        Some(IndexData::U32(i)) => i.clone(),
        None => (0..vertices as u32).collect(),
    }
}

/// Deja en cada atributo solo los vértices de `kept`, en ese orden
fn reindex(prim: &mut Primitive, kept: &[usize]) {
    fn pick<T: Copy + Default>(v: &mut Vec<T>, kept: &[usize]) {
        *v = kept.iter().map(|&i| v.get(i).copied().unwrap_or_default()).collect();
    }
    for a in &mut prim.attributes {
        match a {
            VertexAttribute::Positions(v) | VertexAttribute::Normals(v) => pick(v, kept),
            VertexAttribute::Tangents(v) | VertexAttribute::Colors(v) | VertexAttribute::JointWeights(v) => pick(v, kept),
            VertexAttribute::TexCoords(_, v) => pick(v, kept),
            VertexAttribute::JointIndices(v) => pick(v, kept),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{Mesh as SceneMesh, Node};

    /// Grilla ondulada de n×n en dos primitivas (mitad izquierda y derecha),
    /// sin índices compartidos entre ellas
    fn two_part_scene(n: u32) -> Scene {
        let half = |x0: u32| {
            let mut positions = Vec::new();
            let mut uvs = Vec::new();
            for y in 0..=n {
                for x in x0..=x0 + n / 2 {
                    let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
                    positions.push([u, v, 0.05 * (u * 6.0).sin() * (v * 4.0).cos()]);
                    uvs.push([u, v]);
                }
            }
            let w = n / 2 + 1;
            let mut indices = Vec::new();
            for y in 0..n {
                for x in 0..n / 2 {
                    let a = y * w + x;
                    indices.extend_from_slice(&[a, a + 1, a + w + 1, a, a + w + 1, a + w]);
                }
            }
            Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::TexCoords(0, uvs)],
                indices: Some(IndexData::U32(indices)),
                material: None,
            }
        };
        let mut scene = Scene::new();
        scene.meshes.push(SceneMesh { name: "grilla".into(), primitives: vec![half(0), half(n / 2)] });
        scene.nodes.push(Node {
            name: "grilla".into(),
            transform: converter_scene::Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    fn params(ratio: f64) -> SimplifyParams {
        SimplifyParams { ratio, max_error_percent: None, lock_borders: false, keep_seams: true, aggressive: false }
    }

    #[test]
    fn simplifies_each_primitive_and_keeps_attributes_aligned() {
        let scene = two_part_scene(40);
        let world = WorldSurface::of(&scene);
        let out = simplify_scene(&scene, &params(0.2), &world);
        let after = WorldSurface::of(&out);
        assert!(after.triangles.len() * 4 < world.triangles.len(), "{} → {}", world.triangles.len(), after.triangles.len());
        assert_eq!(out.meshes[0].primitives.len(), 2);
        for prim in &out.meshes[0].primitives {
            let n = positions(prim).unwrap().len();
            assert_eq!(uv0(prim).unwrap().len(), n);
            assert!(indices_u32(prim, n).iter().all(|&i| (i as usize) < n));
        }
        // El límite entre las primitivas (x = 0.5) sigue con los mismos vértices de los dos lados
        let column = |prim: &Primitive| {
            let mut ys: Vec<u32> = positions(prim).unwrap().iter().filter(|p| (p[0] - 0.5).abs() < 1e-6).map(|p| p[1].to_bits()).collect();
            ys.sort();
            ys
        };
        assert_eq!(column(&out.meshes[0].primitives[0]), column(&out.meshes[0].primitives[1]));
    }

    #[test]
    fn stats_report_counts_and_deviation() {
        let scene = two_part_scene(40);
        let channel = Channel::new(|_| Ok(()));
        let (_, stats) = compute(&scene, &RemeshParams::Simplify(params(0.1)), &channel).unwrap();
        assert_eq!(stats.before.triangles, 40 * 40 * 2);
        assert!(stats.after.triangles <= 40 * 40 * 2 / 9);
        assert!(stats.deviation.max_percent > 0.0 && stats.deviation.max_percent < 5.0, "{:?}", stats.deviation);
        assert!(stats.deviation.mean_percent <= stats.deviation.max_percent);
    }

    #[test]
    fn error_limit_in_percent_is_respected() {
        let scene = two_part_scene(40);
        let channel = Channel::new(|_| Ok(()));
        let p = SimplifyParams { max_error_percent: Some(0.2), ..params(0.01) };
        let (_, stats) = compute(&scene, &RemeshParams::Simplify(p), &channel).unwrap();
        assert!(stats.deviation.max_percent <= 0.2, "{:?}", stats.deviation);
        assert!(stats.after.triangles < stats.before.triangles);
    }

    #[test]
    fn smoothing_keeps_connectivity_and_attributes() {
        let scene = two_part_scene(20);
        let p = SmoothParams { iterations: 10, strength: 0.5, sharp_angle: None, fix_borders: false, normal_only: true };
        let out = smooth_scene(&scene, &p);
        for (a, b) in scene.meshes[0].primitives.iter().zip(&out.meshes[0].primitives) {
            assert_eq!(indices_u32(a, 0), indices_u32(b, 0));
            assert_eq!(uv0(a), uv0(b));
            assert_eq!(positions(a).unwrap().len(), positions(b).unwrap().len());
            assert_ne!(positions(a), positions(b), "las ondas se suavizan");
        }
        // El límite entre las primitivas se mueve igual de los dos lados
        let column = |prim: &Primitive| {
            let mut c: Vec<[u32; 3]> = positions(prim).unwrap().iter().filter(|p| (p[0] - 0.5).abs() < 1e-3).map(|p| p.map(f32::to_bits)).collect();
            c.sort();
            c
        };
        let [left, right] = [0, 1].map(|k| column(&out.meshes[0].primitives[k]));
        assert_eq!(left.len(), 21);
        assert_eq!(left, right);
    }

    #[test]
    fn recomputed_normals_follow_the_new_surface() {
        let mut scene = two_part_scene(10);
        // Normales hacia arriba (planas) en la entrada
        for prim in &mut scene.meshes[0].primitives {
            let n = positions(prim).unwrap().len();
            prim.attributes.push(VertexAttribute::Normals(vec![[0.0, 0.0, 1.0]; n]));
        }
        let p = SmoothParams { iterations: 1, strength: 0.1, sharp_angle: None, fix_borders: false, normal_only: true };
        let out = smooth_scene(&scene, &p);
        let prim = &out.meshes[0].primitives[0];
        let nr = normals(prim).unwrap();
        assert!(nr.iter().all(|n| (n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4 && n[2] > 0.5));
        assert!(nr.iter().any(|n| n[0].abs() > 0.01), "la grilla ondulada no es plana");
    }

    /// Esfera UV de radio 10 (unidades = mm con `meters_per_unit` 0,001)
    fn sphere_scene() -> Scene {
        let (rings, segments) = (24, 48);
        let mut positions = vec![[0.0, 10.0, 0.0]];
        for i in 1..rings {
            let phi = std::f32::consts::PI * i as f32 / rings as f32;
            for s in 0..segments {
                let theta = std::f32::consts::TAU * s as f32 / segments as f32;
                positions.push([10.0 * phi.sin() * theta.cos(), 10.0 * phi.cos(), 10.0 * phi.sin() * theta.sin()]);
            }
        }
        positions.push([0.0, -10.0, 0.0]);
        let bottom = (positions.len() - 1) as u32;
        let ring = |i: usize, s: usize| (1 + (i - 1) * segments + s % segments) as u32;
        let mut indices = Vec::new();
        for s in 0..segments {
            indices.extend([0, ring(1, s + 1), ring(1, s)]);
            indices.extend([bottom, ring(rings - 1, s), ring(rings - 1, s + 1)]);
            for i in 1..rings - 1 {
                let (a, b, c, d) = (ring(i, s), ring(i, s + 1), ring(i + 1, s), ring(i + 1, s + 1));
                indices.extend([a, b, d, a, d, c]);
            }
        }
        let uvs = positions.iter().map(|p| [p[0], p[1]]).collect();
        let mut scene = Scene::new();
        scene.meters_per_unit = 0.001;
        scene.materials.push(converter_scene::Material { base_color_texture: Some(converter_scene::TextureRef { texture_index: 0, tex_coord_set: 0 }), ..Default::default() });
        scene.meshes.push(SceneMesh {
            name: "bola".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::TexCoords(0, uvs)],
                indices: Some(IndexData::U32(indices)),
                material: Some(0),
            }],
        });
        scene.nodes.push(Node {
            name: "bola".into(),
            transform: converter_scene::Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn isotropic_and_voxel_make_a_new_mesh_in_mm() {
        let scene = sphere_scene();
        let channel = Channel::new(|_| Ok(()));
        let iso = RemeshParams::Isotropic(IsotropicParams { edge_mm: 1.0, sharp_angle: None, iterations: 5, thin_features: false });
        let (out, stats) = compute(&scene, &iso, &channel).unwrap();
        // Esfera de área 4π·100 ≈ 1257 mm² con triángulos de 1 mm: ≈ 2900
        assert!((2000..4000).contains(&stats.after.triangles), "{}", stats.after.triangles);
        assert!(stats.deviation.max_mm < 0.3, "{:?}", stats.deviation);
        assert_eq!(out.meshes.len(), 1);
        let prim = &out.meshes[0].primitives[0];
        assert!(uv0(prim).is_none());
        assert_eq!(normals(prim).unwrap().len(), positions(prim).unwrap().len());
        assert!(out.materials[0].base_color_texture.is_none(), "sin UV, el material queda sin textura");

        let vox = RemeshParams::Voxel(VoxelParams { voxel_mm: 0.5, smooth_iterations: 2, isotropic_edge_mm: None });
        let (_, stats) = compute(&scene, &vox, &channel).unwrap();
        assert!(stats.after.triangles > 1000);
        assert!(stats.deviation.max_mm < 0.5, "{:?}", stats.deviation);
    }

    #[test]
    fn split_normals_part_only_sharp_edges() {
        // Dos triángulos en ángulo recto: con 60° se parten, sin ángulo no
        let mesh = TriMesh { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], indices: vec![0, 1, 2, 1, 0, 3] };
        assert_eq!(split_normals(&mesh, Some(60.0)).0.len(), 6);
        assert_eq!(split_normals(&mesh, None).0.len(), 4);
    }

    #[test]
    fn params_deserialize_with_defaults() {
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"simplify","ratio":0.5,"max_error_percent":null}"#).unwrap();
        assert_eq!(p, RemeshParams::Simplify(SimplifyParams { keep_seams: true, ..params(0.5) }));
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"smooth","iterations":5,"strength":0.4}"#).unwrap();
        assert_eq!(
            p,
            RemeshParams::Smooth(SmoothParams { iterations: 5, strength: 0.4, sharp_angle: None, fix_borders: false, normal_only: true })
        );
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"isotropic","edge_mm":2}"#).unwrap();
        assert_eq!(p, RemeshParams::Isotropic(IsotropicParams { edge_mm: 2.0, sharp_angle: None, iterations: 5, thin_features: false }));
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"voxel","voxel_mm":0.5}"#).unwrap();
        assert_eq!(p, RemeshParams::Voxel(VoxelParams { voxel_mm: 0.5, smooth_iterations: 0, isotropic_edge_mm: None }));
    }
}
