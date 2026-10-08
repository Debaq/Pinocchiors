//! Remallar: los modos que no son la retopología (ver
//! `libs/quadriflow/PLAN_REMALLAR.md`). Por ahora, Simplificar y Suavizar.
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
use quadriflow_core::remesh::{self, SimplifyInput, SimplifyOptions, SmoothOptions};
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

    #[test]
    fn params_deserialize_with_defaults() {
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"simplify","ratio":0.5,"max_error_percent":null}"#).unwrap();
        assert_eq!(p, RemeshParams::Simplify(SimplifyParams { keep_seams: true, ..params(0.5) }));
        let p: RemeshParams = serde_json::from_str(r#"{"mode":"smooth","iterations":5,"strength":0.4}"#).unwrap();
        assert_eq!(
            p,
            RemeshParams::Smooth(SmoothParams { iterations: 5, strength: 0.4, sharp_angle: None, fix_borders: false, normal_only: true })
        );
    }
}
