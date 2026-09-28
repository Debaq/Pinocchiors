//! Archivo de proyecto `.pinocchio`: todo lo generado desde el modelo
//! original, para cerrar la app y seguir después.
//!
//! Es un ZIP con:
//! - `manifest.json`: formato, versión, app, fecha y nombre del modelo.
//! - `state.msgpack`: el estado del backend en MessagePack con nombres de
//!   campo (un campo nuevo con `#[serde(default)]` no rompe proyectos viejos).
//! - `ui.json`: el estado de la interfaz; lo arma y lo lee el frontend.
//!
//! Los tipos de las librerías no se serializan tal cual: se copian a las
//! estructuras `*Dto` de acá, así un cambio interno en una librería no deja
//! ilegibles los proyectos guardados. Las cachés (centrado de articulaciones)
//! y los diagnósticos se recalculan.

use crate::commands::in_background;
use crate::state::{AppState, OriginalModel, SkeletonTransformParams, SkeletonType};
use converter_scene::{Material, Scene, Texture};
use pinocchio_attachment::Attachment;
use pinocchio_core::{PinocchioOutput, ProcessStats};
use pinocchio_embedding::EmbeddingResult;
use pinocchio_math::{Matrix3, Transform, Vector3};
use pinocchio_mesh::{Mesh, MeshEdge, MeshVertex};
use pinocchio_print3d::LabeledPiece;
use pinocchio_skeleton::{BasicSkeleton, Bone, Skeleton};
use quadriflow_core::{QuadFace, QuadMesh};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::AppHandle;
use uv_core::{Skin, SkinInfo};

const FORMAT: &str = "pinocchio-project";
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    app_version: String,
    /// Segundos desde 1970 (UTC)
    saved_at: u64,
    source_name: Option<String>,
}

/// Todo el estado del backend que vale la pena guardar
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ProjectState {
    original: Option<OriginalDto>,
    scene: Option<Scene>,
    mesh: Option<MeshDto>,
    skeleton: Option<SkeletonDto>,
    original_skeleton: Option<SkeletonDto>,
    skeleton_preset: Option<SkeletonDto>,
    skeleton_transform: TransformParamsDto,
    result: Option<OutputDto>,
    rig_on_quad: bool,
    use_retopology: bool,
    quad_mesh: Option<QuadMeshDto>,
    quad_skin: Option<SkinDto>,
    mesh_before_repair: Option<MeshDto>,
    scene_before_repair: Option<Scene>,
    print3d_pieces: Option<Vec<PieceDto>>,
    mesh_before_print_scale: Option<MeshDto>,
    scene_before_print_scale: Option<Scene>,
}

#[derive(Serialize, Deserialize)]
struct OriginalDto {
    name: String,
    format: String,
    scene: Scene,
}

/// Malla half-edge copiada tal cual (mismo orden de vértices: los pesos van por índice)
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct MeshDto {
    positions: Vec<[f64; 3]>,
    normals: Vec<[f64; 3]>,
    vertex_edge: Vec<Option<usize>>,
    edge_vertex: Vec<usize>,
    edge_twin: Vec<Option<usize>>,
    edge_next: Vec<usize>,
    edge_face: Vec<Option<usize>>,
    faces: Vec<usize>,
}

#[derive(Serialize, Deserialize)]
struct BoneDto {
    name: String,
    position: [f64; 3],
    parent: Option<usize>,
    is_leaf: bool,
}

#[derive(Serialize, Deserialize)]
enum SkeletonDto {
    Human,
    Quad,
    Horse,
    Centaur,
    Bird,
    Spider,
    Serpent,
    Mech,
    Template(Vec<BoneDto>),
    Custom(Vec<BoneDto>),
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct TransformParamsDto {
    scale: f64,
    translation: [f64; 3],
    rotation: [f64; 3],
    pivot: [f64; 3],
}

impl Default for TransformParamsDto {
    fn default() -> Self {
        SkeletonTransformParams::default().into()
    }
}

#[derive(Serialize, Deserialize)]
struct RigidDto {
    /// Filas de la rotación
    rotation: [[f64; 3]; 3],
    translation: [f64; 3],
}

#[derive(Serialize, Deserialize)]
struct OutputDto {
    /// Por vértice, un peso por hueso
    weights: Vec<Vec<f64>>,
    rest_positions: Vec<[f64; 3]>,
    num_bones: usize,
    embedding_bones: Vec<[f64; 3]>,
    sphere_bone_map: Vec<Option<usize>>,
    quality_score: f64,
    bone_positions: Vec<[f64; 3]>,
    bone_rest_transforms: Vec<RigidDto>,
    num_vertices: usize,
    num_medial_spheres: usize,
    avg_influences_per_vertex: f64,
}

#[derive(Serialize, Deserialize)]
struct QuadMeshDto {
    vertices: Vec<[f64; 3]>,
    faces: Vec<[usize; 4]>,
}

#[derive(Serialize, Deserialize)]
enum SkinInfoDto {
    Transferred { seam_faces: usize },
    Unwrapped { num_charts: usize, stretch: f64, coverage: f64, texture_size: u32 },
}

#[derive(Serialize, Deserialize)]
struct SkinDto {
    corners: Vec<[[f32; 2]; 4]>,
    face_material: Vec<Option<usize>>,
    materials: Vec<Material>,
    textures: Vec<Texture>,
    info: SkinInfoDto,
}

/// La pieza sin su malla (así la serializa la librería) más la malla aparte
#[derive(Serialize, Deserialize)]
struct PieceDto {
    piece: LabeledPiece,
    mesh: MeshDto,
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVERSIONES
// ═══════════════════════════════════════════════════════════════════════════

fn v3(v: &Vector3) -> [f64; 3] {
    [v.x(), v.y(), v.z()]
}

fn to_v3([x, y, z]: [f64; 3]) -> Vector3 {
    Vector3::new(x, y, z)
}

impl From<&Mesh> for MeshDto {
    fn from(mesh: &Mesh) -> Self {
        Self {
            positions: mesh.vertices.iter().map(|v| v3(&v.position)).collect(),
            normals: mesh.vertices.iter().map(|v| v3(&v.normal)).collect(),
            vertex_edge: mesh.vertices.iter().map(|v| v.edge).collect(),
            edge_vertex: mesh.edges.iter().map(|e| e.vertex).collect(),
            edge_twin: mesh.edges.iter().map(|e| e.twin).collect(),
            edge_next: mesh.edges.iter().map(|e| e.next).collect(),
            edge_face: mesh.edges.iter().map(|e| e.face).collect(),
            faces: mesh.faces.clone(),
        }
    }
}

impl MeshDto {
    fn into_mesh(self) -> Result<Mesh, String> {
        let n = self.positions.len();
        let m = self.edge_vertex.len();
        if self.normals.len() != n
            || self.vertex_edge.len() != n
            || self.edge_twin.len() != m
            || self.edge_next.len() != m
            || self.edge_face.len() != m
        {
            return Err("malla con tamaños inconsistentes".into());
        }
        let vertices = self
            .positions
            .into_iter()
            .zip(self.normals)
            .zip(self.vertex_edge)
            .map(|((p, nrm), edge)| MeshVertex { position: to_v3(p), normal: to_v3(nrm), edge })
            .collect();
        let edges = (0..m)
            .map(|i| MeshEdge {
                vertex: self.edge_vertex[i],
                twin: self.edge_twin[i],
                next: self.edge_next[i],
                face: self.edge_face[i],
            })
            .collect();
        Ok(Mesh { vertices, edges, faces: self.faces })
    }
}

fn bones_dto(skeleton: &BasicSkeleton) -> Vec<BoneDto> {
    skeleton
        .bones()
        .iter()
        .map(|b| BoneDto { name: b.name.clone(), position: v3(&b.position), parent: b.parent, is_leaf: b.is_leaf })
        .collect()
}

fn bones_from(bones: Vec<BoneDto>) -> BasicSkeleton {
    BasicSkeleton::from_bones(
        bones
            .into_iter()
            .map(|b| Bone { name: b.name, position: to_v3(b.position), parent: b.parent, is_leaf: b.is_leaf })
            .collect(),
    )
}

impl From<&SkeletonType> for SkeletonDto {
    fn from(skeleton: &SkeletonType) -> Self {
        match skeleton {
            SkeletonType::Human => Self::Human,
            SkeletonType::Quad => Self::Quad,
            SkeletonType::Horse => Self::Horse,
            SkeletonType::Centaur => Self::Centaur,
            SkeletonType::Bird => Self::Bird,
            SkeletonType::Spider => Self::Spider,
            SkeletonType::Serpent => Self::Serpent,
            SkeletonType::Mech => Self::Mech,
            SkeletonType::Template(s) => Self::Template(bones_dto(s)),
            SkeletonType::Custom(s) => Self::Custom(bones_dto(s)),
        }
    }
}

impl From<SkeletonDto> for SkeletonType {
    fn from(dto: SkeletonDto) -> Self {
        match dto {
            SkeletonDto::Human => Self::Human,
            SkeletonDto::Quad => Self::Quad,
            SkeletonDto::Horse => Self::Horse,
            SkeletonDto::Centaur => Self::Centaur,
            SkeletonDto::Bird => Self::Bird,
            SkeletonDto::Spider => Self::Spider,
            SkeletonDto::Serpent => Self::Serpent,
            SkeletonDto::Mech => Self::Mech,
            SkeletonDto::Template(b) => Self::Template(bones_from(b)),
            SkeletonDto::Custom(b) => Self::Custom(bones_from(b)),
        }
    }
}

impl From<SkeletonTransformParams> for TransformParamsDto {
    fn from(t: SkeletonTransformParams) -> Self {
        Self { scale: t.scale, translation: t.translation, rotation: t.rotation, pivot: t.pivot }
    }
}

impl From<TransformParamsDto> for SkeletonTransformParams {
    fn from(t: TransformParamsDto) -> Self {
        Self { scale: t.scale, translation: t.translation, rotation: t.rotation, pivot: t.pivot }
    }
}

impl From<&PinocchioOutput> for OutputDto {
    fn from(out: &PinocchioOutput) -> Self {
        let a = &out.attachment;
        Self {
            weights: (0..a.num_vertices()).map(|i| a.get_weights(i).to_vec()).collect(),
            rest_positions: a.rest_positions().iter().map(v3).collect(),
            num_bones: a.num_bones(),
            embedding_bones: out.embedding.bone_positions.iter().map(v3).collect(),
            sphere_bone_map: out.embedding.sphere_bone_map.clone(),
            quality_score: out.embedding.quality_score,
            bone_positions: out.bone_positions.iter().map(v3).collect(),
            bone_rest_transforms: out
                .bone_rest_transforms
                .iter()
                .map(|t| {
                    let r = &t.rotation.0;
                    RigidDto {
                        rotation: [0, 1, 2].map(|i| [r[(i, 0)], r[(i, 1)], r[(i, 2)]]),
                        translation: v3(&t.translation),
                    }
                })
                .collect(),
            num_vertices: out.stats.num_vertices,
            num_medial_spheres: out.stats.num_medial_spheres,
            avg_influences_per_vertex: out.stats.avg_influences_per_vertex,
        }
    }
}

impl From<OutputDto> for PinocchioOutput {
    fn from(dto: OutputDto) -> Self {
        let num_bones = dto.num_bones;
        Self {
            attachment: Attachment::from_rest_positions(
                dto.rest_positions.into_iter().map(to_v3).collect(),
                dto.weights,
                num_bones,
            ),
            embedding: EmbeddingResult {
                bone_positions: dto.embedding_bones.into_iter().map(to_v3).collect(),
                sphere_bone_map: dto.sphere_bone_map,
                quality_score: dto.quality_score,
            },
            bone_positions: dto.bone_positions.into_iter().map(to_v3).collect(),
            bone_rest_transforms: dto
                .bone_rest_transforms
                .into_iter()
                .map(|t| Transform::new(Matrix3::from(t.rotation), to_v3(t.translation)))
                .collect(),
            stats: ProcessStats {
                num_vertices: dto.num_vertices,
                num_bones,
                num_medial_spheres: dto.num_medial_spheres,
                embedding_quality: dto.quality_score,
                avg_influences_per_vertex: dto.avg_influences_per_vertex,
            },
        }
    }
}

impl From<&QuadMesh> for QuadMeshDto {
    fn from(q: &QuadMesh) -> Self {
        Self { vertices: q.vertices.iter().map(|v| [v.x, v.y, v.z]).collect(), faces: q.faces.iter().map(|f| f.v).collect() }
    }
}

impl From<QuadMeshDto> for QuadMesh {
    fn from(dto: QuadMeshDto) -> Self {
        Self {
            vertices: dto.vertices.into_iter().map(|[x, y, z]| pinocchio_math::nalgebra::Vector3::new(x, y, z)).collect(),
            faces: dto.faces.into_iter().map(|v| QuadFace { v }).collect(),
        }
    }
}

impl From<&Skin<4>> for SkinDto {
    fn from(s: &Skin<4>) -> Self {
        Self {
            corners: s.corners.clone(),
            face_material: s.face_material.clone(),
            materials: s.materials.clone(),
            textures: s.textures.clone(),
            info: match s.info {
                SkinInfo::Transferred { seam_faces } => SkinInfoDto::Transferred { seam_faces },
                SkinInfo::Unwrapped { num_charts, stretch, coverage, texture_size } => {
                    SkinInfoDto::Unwrapped { num_charts, stretch, coverage, texture_size }
                }
            },
        }
    }
}

impl From<SkinDto> for Skin<4> {
    fn from(dto: SkinDto) -> Self {
        Self {
            corners: dto.corners,
            face_material: dto.face_material,
            materials: dto.materials,
            textures: dto.textures,
            info: match dto.info {
                SkinInfoDto::Transferred { seam_faces } => SkinInfo::Transferred { seam_faces },
                SkinInfoDto::Unwrapped { num_charts, stretch, coverage, texture_size } => {
                    SkinInfo::Unwrapped { num_charts, stretch, coverage, texture_size }
                }
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ESTADO ↔ PROYECTO
// ═══════════════════════════════════════════════════════════════════════════

fn capture(state: &AppState) -> ProjectState {
    let mesh = |m: &std::sync::Mutex<Option<Mesh>>| m.lock().unwrap().as_ref().map(MeshDto::from);
    let skeleton = |s: &std::sync::Mutex<Option<SkeletonType>>| s.lock().unwrap().as_ref().map(SkeletonDto::from);
    ProjectState {
        original: state.original_model.lock().unwrap().as_ref().map(|o| OriginalDto {
            name: o.name.clone(),
            format: o.format.clone(),
            scene: o.scene.clone(),
        }),
        scene: state.scene.lock().unwrap().clone(),
        mesh: mesh(&state.mesh),
        skeleton: skeleton(&state.skeleton),
        original_skeleton: skeleton(&state.original_skeleton),
        skeleton_preset: skeleton(&state.skeleton_preset),
        skeleton_transform: (*state.skeleton_transform.lock().unwrap()).into(),
        result: state.result.lock().unwrap().as_ref().map(OutputDto::from),
        rig_on_quad: state.rig_on_quad.load(Ordering::SeqCst),
        use_retopology: state.use_retopology.load(Ordering::SeqCst),
        quad_mesh: state.quad_mesh.lock().unwrap().as_ref().map(QuadMeshDto::from),
        quad_skin: state.quad_skin.lock().unwrap().as_ref().map(SkinDto::from),
        mesh_before_repair: mesh(&state.mesh_before_repair),
        scene_before_repair: state.scene_before_repair.lock().unwrap().clone(),
        print3d_pieces: state.print3d_pieces.lock().unwrap().as_ref().map(|pieces| {
            pieces.iter().map(|p| PieceDto { piece: p.clone(), mesh: MeshDto::from(&p.mesh) }).collect()
        }),
        mesh_before_print_scale: mesh(&state.mesh_before_print_scale),
        scene_before_print_scale: state.scene_before_print_scale.lock().unwrap().clone(),
    }
}

/// Reemplaza todo el estado por el del proyecto. Se convierte primero y se
/// asigna al final: si algo falla, el estado actual queda intacto.
fn restore(state: &AppState, p: ProjectState) -> Result<(), String> {
    let mesh = |m: Option<MeshDto>| m.map(MeshDto::into_mesh).transpose();
    let mesh_now = mesh(p.mesh)?;
    let before_repair = mesh(p.mesh_before_repair)?;
    let before_print = mesh(p.mesh_before_print_scale)?;
    let pieces = p
        .print3d_pieces
        .map(|pieces| {
            pieces
                .into_iter()
                .map(|d| d.mesh.into_mesh().map(|mesh| LabeledPiece { mesh, ..d.piece }))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    *state.original_model.lock().unwrap() =
        p.original.map(|o| OriginalModel { name: o.name, format: o.format, scene: o.scene });
    *state.scene.lock().unwrap() = p.scene;
    *state.mesh.lock().unwrap() = mesh_now;
    *state.skeleton.lock().unwrap() = p.skeleton.map(Into::into);
    *state.original_skeleton.lock().unwrap() = p.original_skeleton.map(Into::into);
    *state.skeleton_preset.lock().unwrap() = p.skeleton_preset.map(Into::into);
    *state.skeleton_transform.lock().unwrap() = p.skeleton_transform.into();
    *state.joint_centering.lock().unwrap() = None;
    *state.result.lock().unwrap() = p.result.map(Into::into);
    state.rig_on_quad.store(p.rig_on_quad, Ordering::SeqCst);
    state.use_retopology.store(p.use_retopology, Ordering::SeqCst);
    *state.quad_mesh.lock().unwrap() = p.quad_mesh.map(Into::into);
    *state.quad_skin.lock().unwrap() = p.quad_skin.map(Into::into);
    *state.diagnostics.lock().unwrap() = None;
    *state.mesh_before_repair.lock().unwrap() = before_repair;
    *state.scene_before_repair.lock().unwrap() = p.scene_before_repair;
    *state.print3d_pieces.lock().unwrap() = pieces;
    *state.mesh_before_print_scale.lock().unwrap() = before_print;
    *state.scene_before_print_scale.lock().unwrap() = p.scene_before_print_scale;
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Escribe el ZIP en un temporal junto al destino y lo renombra: un corte a
/// mitad de camino no deja el proyecto anterior a medias
fn write_project(path: &Path, manifest: &Manifest, state_bytes: &[u8], ui: &str) -> Result<(), String> {
    use zip::write::SimpleFileOptions;
    let tmp = path.with_extension("pinocchio.tmp");
    let file = std::fs::File::create(&tmp).map_err(|e| format!("No se pudo crear {}: {e}", tmp.display()))?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let entries: [(&str, &[u8]); 3] = [
        ("manifest.json", &serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?),
        ("state.msgpack", state_bytes),
        ("ui.json", ui.as_bytes()),
    ];
    for (name, bytes) in entries {
        zip.start_file(name, deflate).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| format!("No se pudo guardar {}: {e}", path.display()))
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Vec<u8>, String> {
    let mut entry = zip.by_name(name).map_err(|_| format!("El proyecto no tiene {name}"))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn content_hash(state_bytes: &[u8], ui: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state_bytes.hash(&mut h);
    ui.hash(&mut h);
    h.finish()
}

// ═══════════════════════════════════════════════════════════════════════════
// COMANDOS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct ProjectSaved {
    /// `false` si no había cambios desde el último guardado (solo con `only_if_changed`)
    pub written: bool,
    pub bytes: u64,
}

/// Guarda el proyecto en `path`. `ui` es el estado de la interfaz (JSON).
/// Con `only_if_changed` (guardado automático) no escribe si nada cambió.
#[tauri::command]
pub async fn save_project(app: AppHandle, path: String, ui: String, only_if_changed: bool) -> Result<ProjectSaved, String> {
    in_background(app, move |state| {
        let project = capture(state);
        let source_name = project.original.as_ref().map(|o| o.name.clone());
        let state_bytes = rmp_serde::to_vec_named(&project).map_err(|e| format!("No se pudo serializar: {e}"))?;
        let hash = content_hash(&state_bytes, &ui);
        if only_if_changed && *state.last_saved_hash.lock().unwrap() == Some(hash) {
            return Ok(ProjectSaved { written: false, bytes: 0 });
        }
        let manifest = Manifest {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            saved_at: now_secs(),
            source_name,
        };
        let path = Path::new(&path);
        write_project(path, &manifest, &state_bytes, &ui)?;
        *state.last_saved_hash.lock().unwrap() = Some(hash);
        Ok(ProjectSaved { written: true, bytes: std::fs::metadata(path).map_or(0, |m| m.len()) })
    })
    .await
}

#[derive(Serialize)]
pub struct ProjectOpened {
    /// Estado de la interfaz tal como se guardó
    pub ui: String,
    pub source_name: Option<String>,
    pub saved_at: u64,
}

/// Abre un proyecto: reemplaza todo el estado del backend
#[tauri::command]
pub async fn open_project(app: AppHandle, path: String) -> Result<ProjectOpened, String> {
    in_background(app, move |state| {
        let file = std::fs::File::open(&path).map_err(|e| format!("No se pudo abrir {path}: {e}"))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| "No es un proyecto de Pinocchio válido".to_string())?;
        let manifest: Manifest =
            serde_json::from_slice(&read_entry(&mut zip, "manifest.json")?).map_err(|e| format!("Manifiesto inválido: {e}"))?;
        if manifest.format != FORMAT {
            return Err("No es un proyecto de Pinocchio".into());
        }
        if manifest.version > FORMAT_VERSION {
            return Err(format!(
                "El proyecto es de una versión más nueva de la app (formato {}); actualiza Pinocchio para abrirlo",
                manifest.version
            ));
        }
        let state_bytes = read_entry(&mut zip, "state.msgpack")?;
        let ui = String::from_utf8(read_entry(&mut zip, "ui.json")?).map_err(|e| e.to_string())?;
        let project: ProjectState =
            rmp_serde::from_slice(&state_bytes).map_err(|e| format!("Estado del proyecto ilegible: {e}"))?;
        restore(state, project)?;
        *state.last_saved_hash.lock().unwrap() = Some(content_hash(&state_bytes, &ui));
        Ok(ProjectOpened { ui, source_name: manifest.source_name, saved_at: manifest.saved_at })
    })
    .await
}

/// Archivo donde el guardado automático deja los proyectos que aún no tienen
/// archivo propio (en la carpeta de datos de la app)
fn recovery_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(dir.join("recuperacion.pinocchio"))
}

#[tauri::command]
pub fn recovery_project_path(app: AppHandle) -> Result<String, String> {
    Ok(recovery_path(&app)?.to_string_lossy().into_owned())
}

#[derive(Serialize)]
pub struct RecoveryInfo {
    pub path: String,
    pub saved_at: u64,
    pub source_name: Option<String>,
}

/// La última recuperación automática, si existe y es legible
#[tauri::command]
pub fn recovery_info(app: AppHandle) -> Option<RecoveryInfo> {
    let path = recovery_path(&app).ok()?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).ok()?).ok()?;
    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut zip, "manifest.json").ok()?).ok()?;
    (manifest.format == FORMAT).then(|| RecoveryInfo {
        path: path.to_string_lossy().into_owned(),
        saved_at: manifest.saved_at,
        source_name: manifest.source_name,
    })
}

/// Vuelve al modelo tal como se importó: descarta todo lo generado después
#[tauri::command]
pub fn revert_to_original(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let scene = state.original_model.lock().unwrap().as_ref().map(|o| o.scene.clone()).ok_or("No hay modelo original guardado")?;
    let mesh = crate::commands::scene_to_pinocchio_mesh(&scene)?;
    *state.scene.lock().unwrap() = Some(scene);
    *state.mesh.lock().unwrap() = Some(mesh);
    *state.skeleton.lock().unwrap() = None;
    *state.original_skeleton.lock().unwrap() = None;
    *state.skeleton_preset.lock().unwrap() = None;
    *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
    state.rig_on_quad.store(false, Ordering::SeqCst);
    state.use_retopology.store(false, Ordering::SeqCst);
    state.reset_derived();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        let p = [
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ]
        .map(|[x, y, z]| Vector3::new(x, y, z));
        let t = [
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [3, 7, 6], [3, 6, 2], [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
        ];
        Mesh::try_from_triangles(&p, &t).unwrap()
    }

    /// Guardar y abrir deja el estado igual (malla, escena, esqueleto, pesos, quads)
    #[test]
    fn roundtrip_through_file() {
        let state = AppState::new();
        let mesh = cube();
        let scene = crate::commands::mesh_to_scene(&mesh, "cubo", None);
        *state.original_model.lock().unwrap() = Some(OriginalModel { name: "cubo.stl".into(), format: "STL".into(), scene: scene.clone() });
        *state.scene.lock().unwrap() = Some(scene);
        *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(BasicSkeleton::from_bones(vec![
            Bone { name: "raíz".into(), position: Vector3::new(0.5, 0.5, 0.5), parent: None, is_leaf: false },
            Bone { name: "punta".into(), position: Vector3::new(0.5, 1.0, 0.5), parent: Some(0), is_leaf: true },
        ])));
        *state.skeleton_preset.lock().unwrap() = Some(SkeletonType::Human);
        *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams { scale: 2.0, ..Default::default() };
        let weights: Vec<Vec<f64>> = (0..8).map(|i| vec![i as f64 / 8.0, 1.0 - i as f64 / 8.0]).collect();
        *state.result.lock().unwrap() = Some(PinocchioOutput {
            attachment: Attachment::new(&mesh, weights.clone(), 2),
            embedding: EmbeddingResult { bone_positions: vec![Vector3::new(1.0, 2.0, 3.0)], sphere_bone_map: vec![Some(1), None], quality_score: 0.7 },
            bone_positions: vec![Vector3::new(0.0, 1.0, 0.0)],
            bone_rest_transforms: vec![Transform::new(Matrix3::from_rotation_y(0.3), Vector3::new(1.0, 0.0, 0.0))],
            stats: ProcessStats { num_vertices: 8, num_bones: 2, num_medial_spheres: 5, embedding_quality: 0.7, avg_influences_per_vertex: 2.0 },
        });
        *state.quad_mesh.lock().unwrap() = Some(QuadMesh {
            vertices: vec![pinocchio_math::nalgebra::Vector3::new(0.0, 0.0, 0.0); 4],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }],
        });
        state.use_retopology.store(true, Ordering::SeqCst);
        *state.mesh_before_repair.lock().unwrap() = Some(cube());
        *state.mesh.lock().unwrap() = Some(mesh.clone());

        let bytes = rmp_serde::to_vec_named(&capture(&state)).unwrap();
        let dir = std::env::temp_dir().join(format!("pinocchio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.pinocchio");
        let manifest = Manifest { format: FORMAT.into(), version: FORMAT_VERSION, app_version: "t".into(), saved_at: 0, source_name: None };
        write_project(&path, &manifest, &bytes, "{\"a\":1}").unwrap();

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(read_entry(&mut zip, "ui.json").unwrap(), b"{\"a\":1}");
        let project: ProjectState = rmp_serde::from_slice(&read_entry(&mut zip, "state.msgpack").unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let loaded = AppState::new();
        restore(&loaded, project).unwrap();

        let m = loaded.mesh.lock().unwrap().clone().unwrap();
        assert_eq!(m.vertices.len(), mesh.vertices.len());
        assert_eq!(m.edges.len(), mesh.edges.len());
        assert_eq!(m.faces, mesh.faces);
        for (a, b) in m.vertices.iter().zip(&mesh.vertices) {
            assert_eq!(v3(&a.position), v3(&b.position));
            assert_eq!(a.edge, b.edge);
        }
        for (a, b) in m.edges.iter().zip(&mesh.edges) {
            assert_eq!((a.vertex, a.twin, a.next, a.face), (b.vertex, b.twin, b.next, b.face));
        }
        assert!(loaded.mesh_before_repair.lock().unwrap().is_some());
        assert_eq!(loaded.original_model.lock().unwrap().as_ref().unwrap().name, "cubo.stl");
        assert_eq!(
            loaded.scene.lock().unwrap().as_ref().unwrap().world_primitives()[0].positions.len(),
            state.scene.lock().unwrap().as_ref().unwrap().world_primitives()[0].positions.len()
        );
        match loaded.skeleton.lock().unwrap().as_ref().unwrap() {
            SkeletonType::Custom(s) => {
                assert_eq!(s.bones().len(), 2);
                assert_eq!(s.bones()[0].name, "raíz");
                assert_eq!(s.bones()[1].parent, Some(0));
            }
            _ => panic!("esqueleto distinto"),
        }
        assert!(matches!(loaded.skeleton_preset.lock().unwrap().as_ref(), Some(SkeletonType::Human)));
        assert_eq!(loaded.skeleton_transform.lock().unwrap().scale, 2.0);
        let out = loaded.result.lock().unwrap();
        let out = out.as_ref().unwrap();
        for (i, w) in weights.iter().enumerate() {
            assert_eq!(out.attachment.get_weights(i), w.as_slice());
        }
        assert_eq!(out.embedding.sphere_bone_map, vec![Some(1), None]);
        let r = &out.bone_rest_transforms[0].rotation.0;
        let expected = Matrix3::from_rotation_y(0.3).0;
        assert!((r - expected).norm() < 1e-12);
        assert_eq!(loaded.quad_mesh.lock().unwrap().as_ref().unwrap().faces[0].v, [0, 1, 2, 3]);
        assert!(loaded.use_retopology.load(Ordering::SeqCst));
    }
}
