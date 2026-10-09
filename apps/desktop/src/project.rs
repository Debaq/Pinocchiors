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
    /// Partes del mapa por partes de la malla original
    original_parts: Option<SkinPartsDto>,
    mesh_before_repair: Option<MeshDto>,
    scene_before_repair: Option<Scene>,
    mesh_before_unwrap: Option<MeshDto>,
    scene_before_unwrap: Option<Scene>,
    print3d_pieces: Option<Vec<PieceDto>>,
    mesh_before_print_scale: Option<MeshDto>,
    scene_before_print_scale: Option<Scene>,
    /// Diseño CAD (recetas: el sólido se recalcula al abrir)
    cad: Option<cad_model::Document>,
    /// Los objetos de la escena que no están activos (el activo es el resto
    /// de los campos); `None` = el activo todavía no tiene número
    active_object: Option<u64>,
    objects: Vec<StoredObject>,
}

/// Un objeto que no está activo: su modelo completo (sin diseño ni objetos)
/// ya serializado como `ProjectState`
#[derive(Serialize, Deserialize, Clone)]
struct StoredObject {
    id: u64,
    #[serde(with = "serde_bytes")]
    state: Vec<u8>,
}

/// Objetos de la escena. El activo vive en `AppState` (escena, malla,
/// esqueleto, quads…); los demás quedan acá, serializados. Cambiar de objeto
/// guarda el activo en su casillero y restaura el otro.
#[derive(Default)]
pub struct ObjectSlots {
    pub active: Option<u64>,
    stored: Vec<StoredObject>,
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
    /// Partes del cuerpo del mapa por partes (proyectos viejos no las tienen)
    #[serde(default)]
    parts: Option<SkinPartsDto>,
}

#[derive(Serialize, Deserialize)]
struct SkinPartsDto {
    names: Vec<String>,
    face_part: Vec<usize>,
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
            parts: s.parts.as_ref().map(|p| SkinPartsDto { names: p.names.clone(), face_part: p.face_part.clone() }),
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
            parts: dto.parts.map(|p| uv_core::SkinParts { names: p.names, face_part: p.face_part }),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ESTADO ↔ PROYECTO
// ═══════════════════════════════════════════════════════════════════════════

/// Todo el estado: el objeto activo, los demás y el diseño
fn capture(state: &AppState) -> ProjectState {
    let objects = state.objects.lock().unwrap();
    ProjectState {
        cad: state.cad_document.lock().unwrap().clone(),
        active_object: objects.active,
        objects: objects.stored.clone(),
        ..capture_model(state)
    }
}

/// Solo el modelo del objeto activo (para deshacer y para guardarlo en su casillero)
fn capture_model(state: &AppState) -> ProjectState {
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
        original_parts: state
            .original_parts
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| SkinPartsDto { names: p.names.clone(), face_part: p.face_part.clone() }),
        mesh_before_repair: mesh(&state.mesh_before_repair),
        scene_before_repair: state.scene_before_repair.lock().unwrap().clone(),
        mesh_before_unwrap: mesh(&state.mesh_before_unwrap),
        scene_before_unwrap: state.scene_before_unwrap.lock().unwrap().clone(),
        print3d_pieces: state.print3d_pieces.lock().unwrap().as_ref().map(|pieces| {
            pieces.iter().map(|p| PieceDto { piece: p.clone(), mesh: MeshDto::from(&p.mesh) }).collect()
        }),
        mesh_before_print_scale: mesh(&state.mesh_before_print_scale),
        scene_before_print_scale: state.scene_before_print_scale.lock().unwrap().clone(),
        ..Default::default()
    }
}

/// Reemplaza todo el estado por el del proyecto. Se convierte primero y se
/// asigna al final: si algo falla, el estado actual queda intacto.
fn restore(state: &AppState, mut p: ProjectState) -> Result<(), String> {
    let cad = p.cad.take();
    let objects = ObjectSlots { active: p.active_object, stored: std::mem::take(&mut p.objects) };
    restore_model(state, p)?;
    *state.objects.lock().unwrap() = objects;
    *state.cad_document.lock().unwrap() = cad;
    *state.cad_preview.lock().unwrap() = None;
    *state.cad_cache.lock().unwrap() = None;
    state.cad_ops.lock().unwrap().clear();
    *state.cad_scan.lock().unwrap() = None;
    Ok(())
}

/// Reemplaza el modelo del objeto activo (no toca el diseño ni los demás objetos)
fn restore_model(state: &AppState, p: ProjectState) -> Result<(), String> {
    let mesh = |m: Option<MeshDto>| m.map(MeshDto::into_mesh).transpose();
    let mesh_now = mesh(p.mesh)?;
    let before_repair = mesh(p.mesh_before_repair)?;
    let before_unwrap = mesh(p.mesh_before_unwrap)?;
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
    *state.original_parts.lock().unwrap() =
        p.original_parts.map(|p| uv_core::SkinParts { names: p.names, face_part: p.face_part });
    *state.diagnostics.lock().unwrap() = None;
    *state.mesh_before_repair.lock().unwrap() = before_repair;
    *state.scene_before_repair.lock().unwrap() = p.scene_before_repair;
    *state.mesh_before_unwrap.lock().unwrap() = before_unwrap;
    *state.scene_before_unwrap.lock().unwrap() = p.scene_before_unwrap;
    *state.print3d_pieces.lock().unwrap() = pieces;
    *state.mesh_before_print_scale.lock().unwrap() = before_print;
    *state.scene_before_print_scale.lock().unwrap() = p.scene_before_print_scale;
    // La malla del escaneo → CAD sale del modelo activo
    *state.cad_scan.lock().unwrap() = None;
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
///
/// El archivo de recuperación lleva su propia huella: guardar ahí no cuenta
/// como guardar el proyecto (ver `project_changed`).
#[tauri::command]
pub async fn save_project(app: AppHandle, path: String, ui: String, only_if_changed: bool) -> Result<ProjectSaved, String> {
    let is_recovery = recovery_path(&app).is_ok_and(|r| r == Path::new(&path));
    in_background(app, move |state| {
        let project = capture(state);
        let source_name = project.original.as_ref().map(|o| o.name.clone());
        let state_bytes = rmp_serde::to_vec_named(&project).map_err(|e| format!("No se pudo serializar: {e}"))?;
        let hash = content_hash(&state_bytes, &ui);
        let saved_hash = if is_recovery { &state.last_recovery_hash } else { &state.last_saved_hash };
        if only_if_changed && *saved_hash.lock().unwrap() == Some(hash) {
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
        *saved_hash.lock().unwrap() = Some(hash);
        Ok(ProjectSaved { written: true, bytes: std::fs::metadata(path).map_or(0, |m| m.len()) })
    })
    .await
}

/// Hay cambios desde el último guardado o apertura del proyecto (`ui` es el
/// estado de la interfaz, como en `save_project`)
#[tauri::command]
pub async fn project_changed(app: AppHandle, ui: String) -> Result<bool, String> {
    in_background(app, move |state| {
        let Some(saved) = *state.last_saved_hash.lock().unwrap() else { return Ok(true) };
        let state_bytes = rmp_serde::to_vec_named(&capture(state)).map_err(|e| format!("No se pudo serializar: {e}"))?;
        Ok(content_hash(&state_bytes, &ui) != saved)
    })
    .await
}

/// Borra el archivo de recuperación: el trabajo quedó guardado o se descartó a propósito
#[tauri::command]
pub fn clear_recovery(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state.last_recovery_hash.lock().unwrap() = None;
    let path = recovery_path(&app)?;
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("No se pudo borrar {}: {e}", path.display())),
        _ => Ok(()),
    }
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
        state.undo_snapshots.lock().unwrap().entries.clear();
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

/// Proyecto nuevo: descarta el modelo, el esqueleto y todo lo derivado
#[tauri::command]
pub fn new_project(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _guard = state.try_begin_processing().ok_or("Hay un proceso en curso: espera a que termine")?;
    state.clear_all();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// OBJETOS
// ═══════════════════════════════════════════════════════════════════════════

/// Deja activo el objeto `id`: el actual se guarda en su casillero (si tiene
/// número) y se restaura el de `id`; si `id` no tiene casillero es un objeto
/// nuevo y empieza vacío. El diseño no cambia.
#[tauri::command]
pub async fn object_activate(app: AppHandle, id: u64) -> Result<(), String> {
    in_background(app, move |state| {
        let _guard = state.try_begin_processing().ok_or("Hay un proceso en curso: espera a que termine")?;
        activate_impl(state, id)
    })
    .await
}

pub(crate) fn activate_impl(state: &AppState, id: u64) -> Result<(), String> {
    let mut objects = state.objects.lock().unwrap();
    if objects.active == Some(id) {
        return Ok(());
    }
    let target = match objects.stored.iter().position(|o| o.id == id) {
        Some(i) => {
            let bytes = &objects.stored[i].state;
            Some(rmp_serde::from_slice::<ProjectState>(bytes).map_err(|e| format!("Objeto ilegible: {e}"))?)
        }
        None => None,
    };
    if let Some(current) = objects.active {
        let bytes = rmp_serde::to_vec_named(&capture_model(state)).map_err(|e| format!("No se pudo guardar el objeto: {e}"))?;
        objects.stored.retain(|o| o.id != current);
        objects.stored.push(StoredObject { id: current, state: bytes });
    }
    objects.stored.retain(|o| o.id != id);
    objects.active = Some(id);
    drop(objects);
    // Las ediciones de nodos para deshacer eran del otro objeto
    state.scene_edits.lock().unwrap().clear();
    match target {
        Some(p) => restore_model(state, p),
        None => {
            state.clear_model();
            Ok(())
        }
    }
}

/// Borra un objeto (si es el activo, el modelo queda vacío)
#[tauri::command]
pub fn object_remove(id: u64, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _guard = state.try_begin_processing().ok_or("Hay un proceso en curso: espera a que termine")?;
    remove_impl(&state, id);
    Ok(())
}

pub(crate) fn remove_impl(state: &AppState, id: u64) {
    let mut objects = state.objects.lock().unwrap();
    objects.stored.retain(|o| o.id != id);
    if objects.active == Some(id) {
        objects.active = None;
        drop(objects);
        state.clear_model();
    }
}

/// Malla de un objeto que no está activo, para verlo en gris junto al activo,
/// en las unidades de la escena activa (mismo formato que `get_mesh_data`)
#[tauri::command]
pub async fn object_mesh_data(app: AppHandle, id: u64) -> Result<tauri::ipc::Response, String> {
    in_background(app, move |state| object_mesh_bytes(state, id)).await.map(tauri::ipc::Response::new)
}

pub(crate) fn object_mesh_bytes(state: &AppState, id: u64) -> Result<Vec<u8>, String> {
    let project: ProjectState = {
        let objects = state.objects.lock().unwrap();
        let stored = objects.stored.iter().find(|o| o.id == id).ok_or("Ese objeto no está guardado")?;
        rmp_serde::from_slice(&stored.state).map_err(|e| format!("Objeto ilegible: {e}"))?
    };
    let scene = project.scene.ok_or("El objeto no tiene malla")?;
    let target = state.scene.lock().unwrap().as_ref().map_or(scene.meters_per_unit, |s| s.meters_per_unit);
    let mut data = crate::commands::scene_mesh_data(&scene);
    let k = (scene.meters_per_unit / target) as f32;
    if (k - 1.0).abs() > 1e-6 {
        data.positions.iter_mut().for_each(|v| *v *= k);
    }
    Ok(data.to_bytes())
}

/// Escena de un objeto: la del activo o la de su casillero
fn object_scene(state: &AppState, id: u64) -> Result<Scene, String> {
    if state.objects.lock().unwrap().active == Some(id) {
        return state.scene.lock().unwrap().clone().ok_or_else(|| "El objeto activo no tiene malla".to_string());
    }
    let project: ProjectState = {
        let objects = state.objects.lock().unwrap();
        // Una pieza del diseño que nunca se abrió fuera de Diseñar no tiene malla todavía
        let stored = objects.stored.iter().find(|o| o.id == id).ok_or("Un objeto elegido todavía no tiene malla: elegirlo una vez solo")?;
        rmp_serde::from_slice(&stored.state).map_err(|e| format!("Objeto ilegible: {e}"))?
    };
    project.scene.ok_or_else(|| "Un objeto elegido no tiene malla".to_string())
}

/// Cómo llevar una escena a milímetros con Y arriba: giro y escala, y su caja
/// envolvente así (sin trasladar)
fn to_mm(scene: &Scene) -> (converter_scene::glam::Quat, f32, [f32; 3], [f32; 3], usize, f64) {
    use converter_scene::glam::{Quat, Vec3};
    let k = (scene.meters_per_unit * 1000.0) as f32;
    // Z arriba → Y arriba: (x, y, z) → (x, z, −y)
    let rot = if scene.y_up { Quat::IDENTITY } else { Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) };
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let mut triangles = 0;
    let mut volume = 0.0f64;
    for prim in scene.world_primitives() {
        let pts: Vec<Vec3> = prim.positions.iter().map(|p| rot * (Vec3::from(*p) * k)).collect();
        for q in &pts {
            for i in 0..3 {
                min[i] = min[i].min(q[i]);
                max[i] = max[i].max(q[i]);
            }
        }
        triangles += prim.triangles.len();
        // Volumen con signo (vale si la malla es cerrada)
        for t in &prim.triangles {
            let [a, b, c] = t.map(|i| pts[i as usize].as_dvec3());
            volume += a.dot(b.cross(c)) / 6.0;
        }
    }
    (rot, k, min, max, triangles, volume.abs())
}

/// Medidas de un objeto para Fabricar (en mm, como se ve: Z arriba)
#[derive(Debug, Clone, Serialize)]
pub struct ObjectInfo {
    pub id: u64,
    pub size: [f32; 3],
    pub triangles: usize,
    /// En mm³ (si la malla no es cerrada no significa nada)
    pub volume: f64,
}

#[tauri::command]
pub async fn objects_info(app: AppHandle, ids: Vec<u64>) -> Result<Vec<ObjectInfo>, String> {
    in_background(app, move |state| objects_info_impl(state, &ids)).await
}

pub(crate) fn objects_info_impl(state: &AppState, ids: &[u64]) -> Result<Vec<ObjectInfo>, String> {
    ids.iter()
        .map(|&id| {
            let (_, _, min, max, triangles, volume) = to_mm(&object_scene(state, id)?);
            let d = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
            Ok(ObjectInfo { id, size: [d[0], d[2], d[1]], triangles, volume })
        })
        .collect()
}

/// Varios objetos en una escena en mm, cada uno en su nodo con su nombre.
/// Con `spacing`, apoyados en la cama (abajo en 0) y en fila a lo largo de X
/// con esa separación; sin él, donde están.
pub(crate) fn objects_scene(state: &AppState, ids: &[u64], names: &[String], spacing: Option<f64>) -> Result<Scene, String> {
    use converter_scene::glam::Vec3;
    if ids.is_empty() {
        return Err("No hay objetos elegidos".into());
    }
    let mut out = Scene::new();
    out.meters_per_unit = 0.001;
    out.y_up = true;
    let mut cursor = 0.0f32;
    for (i, &id) in ids.iter().enumerate() {
        let scene = object_scene(state, id)?;
        let (rot, k, min, max, _, _) = to_mm(&scene);
        if !min[0].is_finite() {
            continue;
        }
        let translation = match spacing {
            Some(gap) => {
                let t = Vec3::new(cursor - min[0], -min[1], -(min[2] + max[2]) / 2.0);
                cursor += max[0] - min[0] + gap as f32;
                t
            }
            None => Vec3::ZERO,
        };
        let name = names.get(i).cloned().unwrap_or_else(|| format!("Objeto {id}"));
        let (roots_before, meshes_before) = (out.root_nodes.len(), out.meshes.len());
        out.merge(scene);
        // Las mallas con el nombre del objeto (el 3MF nombra así cada objeto)
        let added = out.meshes.len() - meshes_before;
        for (k, m) in out.meshes[meshes_before..].iter_mut().enumerate() {
            m.name = if added == 1 { name.clone() } else { format!("{name} {}", k + 1) };
        }
        let children: Vec<usize> = out.root_nodes.drain(roots_before..).collect();
        out.nodes.push(converter_scene::Node {
            name,
            transform: converter_scene::Transform::Trs { translation, rotation: rot, scale: Vec3::splat(k) },
            mesh: None,
            skin: None,
            children,
        });
        out.root_nodes.push(out.nodes.len() - 1);
    }
    if out.root_nodes.is_empty() {
        return Err("Los objetos elegidos no tienen malla".into());
    }
    Ok(out)
}

/// Exporta varios objetos juntos (para imprimirlos de una vez)
#[tauri::command]
pub async fn export_objects(app: AppHandle, ids: Vec<u64>, names: Vec<String>, path: String, format: String, spacing: Option<f64>) -> Result<u64, String> {
    in_background(app, move |state| export_objects_impl(state, &ids, &names, &path, &format, spacing)).await
}

pub(crate) fn export_objects_impl(state: &AppState, ids: &[u64], names: &[String], path: &str, format: &str, spacing: Option<f64>) -> Result<u64, String> {
    let scene = objects_scene(state, ids, names, spacing)?;
    let p = Path::new(path);
    crate::cad::export_scene(&scene, p, format)?;
    Ok(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// Le da número al objeto activo sin cambiar nada (el modelo que ya estaba
/// cuando todavía no había objetos)
#[tauri::command]
pub fn object_adopt(id: u64, state: tauri::State<'_, AppState>) {
    adopt_impl(&state, id);
}

pub(crate) fn adopt_impl(state: &AppState, id: u64) {
    let mut objects = state.objects.lock().unwrap();
    if objects.active.is_none() {
        objects.active = Some(id);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DESHACER
// ═══════════════════════════════════════════════════════════════════════════

/// Copias completas del estado para deshacer las operaciones que lo cambian
/// en el backend (reparar, retopología, pesos, UV…). Es lo mismo que va al
/// proyecto, pero queda en memoria: no se guarda en el archivo.
#[derive(Default)]
pub struct UndoSnapshots {
    next: u64,
    entries: std::collections::BTreeMap<u64, ProjectState>,
}

/// Cuántas copias se guardan: cada una lleva el modelo entero
const MAX_UNDO_SNAPSHOTS: usize = 30;

#[derive(Serialize)]
pub struct SnapshotTaken {
    pub id: u64,
    /// Copias más viejas que se descartaron para no pasar del máximo
    pub evicted: Vec<u64>,
}

/// Guarda una copia del estado actual (antes de una operación)
#[tauri::command]
pub async fn take_snapshot(app: AppHandle) -> Result<SnapshotTaken, String> {
    in_background(app, |state| {
        let _guard = state.try_begin_processing().ok_or("Ya hay un proceso en curso")?;
        let project = capture_model(state);
        Ok(store_snapshot(&mut state.undo_snapshots.lock().unwrap(), project))
    })
    .await
}

/// Copia del modelo activo para deshacer (lo de `take_snapshot`, sin la marca de proceso)
pub(crate) fn take_snapshot_impl(state: &AppState) -> SnapshotTaken {
    let project = capture_model(state);
    store_snapshot(&mut state.undo_snapshots.lock().unwrap(), project)
}

fn store_snapshot(snapshots: &mut UndoSnapshots, project: ProjectState) -> SnapshotTaken {
    snapshots.next += 1;
    let id = snapshots.next;
    snapshots.entries.insert(id, project);
    let mut evicted = Vec::new();
    while snapshots.entries.len() > MAX_UNDO_SNAPSHOTS {
        let oldest = *snapshots.entries.keys().next().unwrap();
        snapshots.entries.remove(&oldest);
        evicted.push(oldest);
    }
    SnapshotTaken { id, evicted }
}

/// Deshace o rehace la operación de la copia `id`: el estado actual y el de
/// la copia se intercambian. El historial llama siempre en orden (deshacer,
/// rehacer, deshacer…), así que con intercambiar alcanza.
#[tauri::command]
pub async fn swap_snapshot(app: AppHandle, id: u64) -> Result<(), String> {
    in_background(app, move |state| {
        let _guard = state.try_begin_processing().ok_or("Ya hay un proceso en curso")?;
        swap_snapshot_impl(state, id)
    })
    .await
}

pub(crate) fn swap_snapshot_impl(state: &AppState, id: u64) -> Result<(), String> {
    let mut snapshots = state.undo_snapshots.lock().unwrap();
    let other = snapshots
        .entries
        .remove(&id)
        .ok_or("La copia para deshacer ya no está (no se guarda en el proyecto, y las más viejas se descartan)")?;
    let current = capture_model(state);
    restore_model(state, other)?;
    snapshots.entries.insert(id, current);
    Ok(())
}

/// Descarta una copia (la operación no se hizo)
#[tauri::command]
pub fn drop_snapshot(id: u64, state: tauri::State<'_, AppState>) {
    state.undo_snapshots.lock().unwrap().remove(id);
}

impl UndoSnapshots {
    /// Descarta una copia (la operación no se hizo, o su paso ya no se puede deshacer)
    pub fn remove(&mut self, id: u64) {
        self.entries.remove(&id);
    }
}

/// Descarta todas las copias (modelo nuevo: historial nuevo)
#[tauri::command]
pub fn clear_snapshots(state: tauri::State<'_, AppState>) {
    state.undo_snapshots.lock().unwrap().entries.clear();
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

    /// Cambiar de objeto guarda el activo en su casillero y trae el otro; el
    /// diseño no se toca y el proyecto lleva todos los objetos
    #[test]
    fn objects_swap_and_survive_save() {
        let state = AppState::new();
        *state.cad_document.lock().unwrap() = Some(cad_model::Document::new());
        *state.mesh.lock().unwrap() = Some(cube());
        *state.skeleton.lock().unwrap() = Some(SkeletonType::Human);
        adopt_impl(&state, 1);
        // Objeto nuevo: empieza vacío
        activate_impl(&state, 2).unwrap();
        assert!(state.mesh.lock().unwrap().is_none());
        assert!(state.skeleton.lock().unwrap().is_none());
        assert!(state.cad_document.lock().unwrap().is_some(), "el diseño queda");
        // Vuelta al primero: su malla y su esqueleto
        activate_impl(&state, 1).unwrap();
        assert_eq!(state.mesh.lock().unwrap().as_ref().unwrap().num_vertices(), 8);
        assert!(matches!(*state.skeleton.lock().unwrap(), Some(SkeletonType::Human)));
        // Ida y vuelta por el archivo: los dos objetos siguen
        let bytes = rmp_serde::to_vec_named(&capture(&state)).unwrap();
        let other = AppState::new();
        restore(&other, rmp_serde::from_slice(&bytes).unwrap()).unwrap();
        assert_eq!(other.objects.lock().unwrap().active, Some(1));
        activate_impl(&other, 2).unwrap();
        assert!(other.mesh.lock().unwrap().is_none());
        assert!(other.cad_document.lock().unwrap().is_some());
        remove_impl(&other, 2);
        assert_eq!(other.objects.lock().unwrap().active, None);
        activate_impl(&other, 1).unwrap();
        assert_eq!(other.mesh.lock().unwrap().as_ref().unwrap().num_vertices(), 8);
    }

    /// Escena de un cubo de `side` (en las unidades de la escena) con una esquina en `at`
    fn cube_scene(side: f32, at: [f32; 3], meters_per_unit: f64) -> Scene {
        use converter_scene::{IndexData, Mesh as SceneMesh, Node, Primitive, VertexAttribute};
        let unit = [
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ];
        let positions = unit.iter().map(|p: &[f32; 3]| [at[0] + side * p[0], at[1] + side * p[1], at[2] + side * p[2]]).collect();
        let tris: [[u32; 3]; 12] = [
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [3, 7, 6], [3, 6, 2], [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
        ];
        let indices = tris.iter().flatten().copied().collect();
        let mut scene = Scene::new();
        scene.meshes.push(SceneMesh {
            name: "cubo".into(),
            primitives: vec![Primitive { attributes: vec![VertexAttribute::Positions(positions)], indices: Some(IndexData::U32(indices)), material: None }],
        });
        scene.nodes.push(Node { name: "cubo".into(), transform: converter_scene::Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        scene.root_nodes.push(0);
        scene.meters_per_unit = meters_per_unit;
        scene
    }

    /// Varios objetos juntos en mm: medidas, nombres y en fila sobre la cama
    #[test]
    fn several_objects_in_one_scene() {
        let state = AppState::new();
        // Objeto 1: cubo de 10 mm; objeto 2: cubo de 2 cm (en metros), lejos y en el aire
        *state.scene.lock().unwrap() = Some(cube_scene(10.0, [0.0, 0.0, 0.0], 0.001));
        adopt_impl(&state, 1);
        activate_impl(&state, 2).unwrap();
        *state.scene.lock().unwrap() = Some(cube_scene(0.02, [1.0, 0.5, 0.0], 1.0));
        let info = objects_info_impl(&state, &[1, 2]).unwrap();
        assert!((info[0].size[0] - 10.0).abs() < 1e-3 && (info[1].size[2] - 20.0).abs() < 1e-3, "{info:?}");
        assert!((info[0].volume - 1000.0).abs() < 1e-3 && (info[1].volume - 8000.0).abs() < 1e-2, "{info:?}");
        assert_eq!(info[0].triangles, 12);
        let names = vec!["Tapa".to_string(), "Caja".to_string()];
        let scene = objects_scene(&state, &[1, 2], &names, Some(5.0)).unwrap();
        assert_eq!(scene.meters_per_unit, 0.001);
        assert_eq!(scene.root_nodes.len(), 2);
        assert_eq!(scene.meshes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Tapa", "Caja"]);
        // En fila: el primero de 0 a 10, el segundo de 15 a 35; los dos apoyados en y = 0
        let prims = scene.world_primitives();
        let range = |k: usize, axis: usize| {
            let v = prims[k].positions.iter().map(|p| p[axis]);
            (v.clone().fold(f32::INFINITY, f32::min), v.fold(f32::NEG_INFINITY, f32::max))
        };
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        assert!(close(range(0, 0), (0.0, 10.0)), "{:?}", range(0, 0));
        assert!(close(range(1, 0), (15.0, 35.0)), "{:?}", range(1, 0));
        assert!(close(range(1, 1), (0.0, 20.0)), "{:?}", range(1, 1));
        // Sin acomodar: donde están (el segundo a 1 m = 1000 mm)
        let scene = objects_scene(&state, &[1, 2], &names, None).unwrap();
        let prims = scene.world_primitives();
        assert!((prims[1].positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min) - 1000.0).abs() < 1e-2);
        // A un 3MF con los dos objetos
        let path = std::env::temp_dir().join(format!("objetos-{}.3mf", std::process::id()));
        let size = export_objects_impl(&state, &[1, 2], &names, path.to_str().unwrap(), "3mf", Some(5.0)).unwrap();
        assert!(size > 0);
        let _ = std::fs::remove_file(path);
    }

    /// Deshacer y rehacer intercambian el estado con la copia
    #[test]
    fn snapshot_swap_undoes_and_redoes() {
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(cube());
        let id = store_snapshot(&mut state.undo_snapshots.lock().unwrap(), capture(&state)).id;
        // La "operación": retopología
        *state.quad_mesh.lock().unwrap() = Some(QuadMesh {
            vertices: vec![pinocchio_math::nalgebra::Vector3::new(0.0, 0.0, 0.0); 4],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }],
        });
        swap_snapshot_impl(&state, id).unwrap();
        assert!(state.quad_mesh.lock().unwrap().is_none());
        assert_eq!(state.mesh.lock().unwrap().as_ref().unwrap().vertices.len(), 8);
        swap_snapshot_impl(&state, id).unwrap();
        assert_eq!(state.quad_mesh.lock().unwrap().as_ref().unwrap().faces.len(), 1);
        assert!(swap_snapshot_impl(&state, id + 1).is_err());
    }

    /// Las copias más viejas se descartan al pasar del máximo
    #[test]
    fn old_snapshots_are_evicted() {
        let mut snapshots = UndoSnapshots::default();
        let taken: Vec<_> = (0..MAX_UNDO_SNAPSHOTS + 2).map(|_| store_snapshot(&mut snapshots, ProjectState::default())).collect();
        assert_eq!(snapshots.entries.len(), MAX_UNDO_SNAPSHOTS);
        assert_eq!(taken[MAX_UNDO_SNAPSHOTS].evicted, vec![1]);
        assert_eq!(taken[MAX_UNDO_SNAPSHOTS + 1].evicted, vec![2]);
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
        // Diseño CAD con sketch, restricciones y un STEP importado (bytes)
        let mut doc = cad_model::Document::new();
        let mut sk = cad_model::Sketch::new();
        let lines = sk.rectangle([0.0, 0.0], [10.0, 5.0]);
        sk.constrain(cad_model::SketchConstraint::Length { line: lines[0], value: 12.0, reference: false, opts: Default::default() });
        sk.circle([5.0, 2.5], 1.0);
        let sid = doc.add(cad_model::FeatureKind::Sketch { plane: cad_model::PlaneSpec::Xz, offset: 2.0, sketch: sk });
        doc.add(cad_model::FeatureKind::Extrude(cad_model::Extrude {
            sketch: sid,
            regions: cad_model::RegionSelection::Points { points: vec![[1.0, 1.0]] },
            extent: cad_model::Extent::Symmetric { distance: 4.0 },
            reverse: true,
            op: cad_model::BodyOp::Join,
            draft: 0.0,
            thin: None,
        }));
        doc.add(cad_model::FeatureKind::Import { format: cad_model::ImportFormat::Step, data: vec![0, 1, 2, 255], op: cad_model::BodyOp::Cut });
        *state.cad_document.lock().unwrap() = Some(doc.clone());

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

        assert_eq!(loaded.cad_document.lock().unwrap().as_ref(), Some(&doc));
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
