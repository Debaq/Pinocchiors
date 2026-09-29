//! Comandos Tauri para la aplicación Pinocchio
//!
//! Fase 1: Import/Export multi-formato usando converter-*
//! Fase 2: Retopología con QuadriFlow

use crate::animation;
use crate::state::{AppState, SkeletonTransformParams, SkeletonType};
use converter_scene::{IndexData, Scene, VertexAttribute};
use pinocchio_attachment::Attachment;
use pinocchio_core::{autorig_with_progress, transfer_weights, AutorigStage, PinocchioConfig, PinocchioOutput, SkeletonFit};
use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3;
use pinocchio_skeleton::{
    BasicSkeleton, BirdSkeleton, Bone, CentaurSkeleton, HorseSkeleton, HumanSkeleton,
    MechSkeleton, QuadSkeleton, SerpentSkeleton, SpiderSkeleton, Skeleton,
};
use quadriflow_core::{remesh_with_callback, Rebuild, RemeshConfig, Symmetry};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{ipc::{Channel, Response}, AppHandle, Manager, State};

// Repair & Print3D
use pinocchio_repair::{self, AnalysisConfig as RepairAnalysisConfig, RepairConfig, HoleFillConfig};
use pinocchio_print3d::{self, SubdivideConfig, SubdivideStrategy};

// USDZ export
use converter_usda::{UsdaExportOptions, write_usdz};

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Información de la malla cargada
#[derive(Debug, Clone, Serialize)]
pub struct MeshInfo {
    pub num_vertices: usize,
    pub num_faces: usize,
    pub num_meshes: usize,
    pub has_normals: bool,
    pub has_uvs: bool,
    pub has_materials: bool,
    pub bounding_box: BoundingBox,
    pub format: String,
    /// Esqueleto y animaciones que trae el archivo (glTF con skin)
    pub rig: Option<ImportedRigInfo>,
}

/// Rig del archivo importado: el esqueleto y los pesos quedan en el estado
#[derive(Debug, Clone, Serialize)]
pub struct ImportedRigInfo {
    pub num_bones: usize,
    pub clips: Vec<crate::imported_rig::ClipDto>,
}

/// Bounding box serializable
#[derive(Debug, Clone, Serialize)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Datos de la malla para Three.js
#[derive(Debug, Clone)]
pub struct MeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub uvs: Option<Vec<f32>>,
    /// Rangos de índices por material: `[inicio, cantidad, material]`
    /// (`u32::MAX` = sin material)
    pub groups: Vec<[u32; 3]>,
}

/// Empaqueta una malla para el visor en binario (little-endian, todo en
/// palabras de 4 bytes para que el frontend lo lea con typed arrays sin
/// copiar). Con JSON, una malla de un millón de caras son decenas de MB de
/// texto que serializar y parsear en el hilo de la ventana.
///
/// Cabecera `u32 × 4`: vértices, índices, 1 si hay UVs, índices de quads.
/// Luego: posiciones `f32 × 3V`, normales `f32 × 3V`, UVs `f32 × 2V` (si
/// hay), índices `u32`, índices de quads `u32`.
fn pack_mesh(positions: &[f32], normals: &[f32], uvs: Option<&[f32]>, indices: &[u32], quad_indices: &[u32]) -> Vec<u8> {
    let words = 4 + positions.len() + normals.len() + uvs.map_or(0, |u| u.len()) + indices.len() + quad_indices.len();
    let mut out = Vec::with_capacity(words * 4);
    let header = [(positions.len() / 3) as u32, indices.len() as u32, uvs.is_some() as u32, quad_indices.len() as u32];
    for w in header {
        out.extend_from_slice(&w.to_le_bytes());
    }
    for f in positions.iter().chain(normals).chain(uvs.unwrap_or(&[])) {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for i in indices.iter().chain(quad_indices) {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

impl MeshData {
    /// [`pack_mesh`] y al final, si hay, los grupos por material: `u32`
    /// cantidad y luego `[inicio, cantidad, material]` por grupo
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = pack_mesh(&self.positions, &self.normals, self.uvs.as_deref(), &self.indices, &[]);
        append_groups(&mut out, &self.groups);
        out
    }
}

/// Grupos por material al final de [`pack_mesh`] (nada si no hay)
fn append_groups(out: &mut Vec<u8>, groups: &[[u32; 3]]) {
    if !groups.is_empty() {
        out.extend_from_slice(&(groups.len() as u32).to_le_bytes());
        for w in groups.iter().flatten() {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }
}

/// Preset de esqueleto
#[derive(Debug, Clone, Serialize)]
pub struct SkeletonPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub num_bones: usize,
}

/// Datos del esqueleto para visualización
#[derive(Debug, Clone, Serialize)]
pub struct SkeletonData {
    pub bones: Vec<BoneData>,
    pub edges: Vec<[usize; 2]>,
    /// Centro de escala y rotación del esqueleto visible, si está transformado
    /// (si no, el visor usa el centro de su caja, que es lo que se elegiría)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[f64; 3]>,
}

/// Datos de un hueso
#[derive(Debug, Clone, Serialize)]
pub struct BoneData {
    pub name: String,
    pub position: [f64; 3],
    pub parent: Option<usize>,
    pub is_leaf: bool,
}

/// Configuración del autorig
#[derive(Debug, Clone, Deserialize)]
pub struct AutorigConfig {
    pub quality: String,
    pub diffusion_weight: Option<f64>,
    pub max_influences: Option<usize>,
}

/// Envía un mensaje de progreso (los errores de envío se ignoran: la
/// ventana puede haberse cerrado)
pub(crate) fn report(channel: &Channel<Progress>, stage: &str, percent: u32, message: impl Into<String>) {
    let _ = channel.send(Progress { stage: stage.to_string(), percent, message: message.into() });
}

/// Mensaje de progreso
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub percent: u32,
    pub message: String,
}

/// Datos de los pesos para visualización
#[derive(Debug, Clone)]
pub struct WeightsData {
    pub num_vertices: usize,
    pub num_bones: usize,
    pub bone_names: Vec<String>,
    /// Por vértice, `max_influences` pares (hueso, peso)
    pub weights: Vec<f32>,
    pub max_influences: usize,
}

impl WeightsData {
    /// Binario little-endian: cabecera `u32 × 4` (vértices, huesos,
    /// influencias, bytes de nombres), nombres en UTF-8 separados por `\n` y
    /// rellenos a múltiplo de 4, y los pares `f32`.
    fn to_bytes(&self) -> Vec<u8> {
        let mut names = self.bone_names.join("\n").into_bytes();
        let names_len = names.len();
        names.resize(names_len.div_ceil(4) * 4, 0);
        let mut out = Vec::with_capacity(16 + names.len() + self.weights.len() * 4);
        for w in [self.num_vertices, self.num_bones, self.max_influences, names_len] {
            out.extend_from_slice(&(w as u32).to_le_bytes());
        }
        out.extend_from_slice(&names);
        for f in &self.weights {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out
    }
}

/// Configuración de exportación
#[derive(Debug, Clone, Deserialize)]
pub struct ExportConfig {
    pub format: String,
    pub path: String,
    pub include_skeleton: Option<bool>,
    /// Exportar el rig (esqueleto + pesos de skinning) del autorig
    pub include_weights: Option<bool>,
    /// Exportar la malla retopologizada en vez de la original
    pub use_retopology: Option<bool>,
    pub texture_quality: Option<u8>,
    pub max_texture_size: Option<u32>,
    pub optimize_geometry: Option<bool>,
    pub generate_normals: Option<bool>,
    pub flatten_transforms: Option<bool>,
    pub scale_factor: Option<f64>,
    pub strip_unused: Option<bool>,
    pub export_animations: Option<bool>,
    /// Fracción de triángulos a conservar (0-1], solo glTF/GLB
    pub simplify_ratio: Option<f32>,
    /// Desviación máxima de la reducción, relativa al tamaño del modelo
    pub simplify_error: Option<f32>,
    /// Comprimir la geometría con Draco, solo glTF/GLB
    pub draco: Option<bool>,
    /// Nivel de compresión Draco 0-10
    pub draco_level: Option<u8>,
    /// Bits de cuantización de las posiciones (precisión de la geometría)
    pub draco_position_bits: Option<u8>,
    // USDZ
    pub arkit_compatible: Option<bool>,
    pub fps: Option<f64>,
    /// Animaciones de la línea de tiempo (solo con el rig)
    pub animations: Option<Vec<animation::AnimationClip>>,
    /// Exportar solo el esqueleto con sus animaciones, sin malla (glTF/GLB)
    pub skeleton_only: Option<bool>,
    /// Con `skeleton_only`: una figura por hueso para verlo en cualquier visor
    pub bone_shapes: Option<bool>,
    /// Huesos que no deforman: su peso pasa al primer ancestro que sí
    pub non_deforming: Option<Vec<usize>>,
}

/// Resultado de exportación
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub success: bool,
    pub path: String,
    pub message: String,
    pub files_created: Vec<String>,
    /// Suma del tamaño de los archivos escritos, en bytes
    pub total_bytes: u64,
}

/// Formatos de importación soportados
#[derive(Debug, Clone, Serialize)]
pub struct SupportedFormats {
    pub import: Vec<FormatInfo>,
    pub export: Vec<FormatInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormatInfo {
    pub id: String,
    pub name: String,
    pub extensions: Vec<String>,
    pub description: String,
}

/// Configuración de retopología
#[derive(Debug, Clone, Deserialize)]
pub struct RetopologyConfig {
    /// Número objetivo de quads
    pub target_quads: usize,
    /// Preservar bordes agudos
    pub preserve_sharp: Option<bool>,
    /// Ángulo de detección de bordes agudos (grados)
    pub sharp_angle: Option<f32>,
    /// Iteraciones de suavizado de los campos por nivel
    pub smooth_iterations: Option<usize>,
    /// Reconstrucción de mallas rotas: "auto", "always" o "never"
    pub rebuild: Option<String>,
    /// Alineación a las direcciones de curvatura (0 = nada, 1 = completa)
    pub curvature_alignment: Option<f64>,
    /// Quads más chicos donde la pieza es más delgada que un quad
    pub adaptive_density: Option<bool>,
    /// Simetría espejo: "none", "x", "y" o "z"
    pub symmetry: Option<String>,
    /// Los quads siguen las costuras de UV (por defecto sí; solo si el
    /// modelo tiene UV)
    pub follow_seams: Option<bool>,
}

impl RetopologyConfig {
    fn to_remesh_config(&self) -> Result<RemeshConfig, String> {
        let defaults = RemeshConfig::default();
        let rebuild = match self.rebuild.as_deref() {
            None | Some("auto") => Rebuild::Auto,
            Some("always") => Rebuild::Always,
            Some("never") => Rebuild::Never,
            Some(other) => return Err(format!("Reconstrucción desconocida: {other}")),
        };
        let symmetry = match self.symmetry.as_deref() {
            None | Some("none") => Symmetry::None,
            Some("x") => Symmetry::X,
            Some("y") => Symmetry::Y,
            Some("z") => Symmetry::Z,
            Some(other) => return Err(format!("Simetría desconocida: {other}")),
        };
        Ok(RemeshConfig {
            target_faces: self.target_quads,
            preserve_sharp: self.preserve_sharp.unwrap_or(defaults.preserve_sharp),
            sharp_angle: self.sharp_angle.map_or(defaults.sharp_angle, |a| (a as f64).to_radians()),
            smooth_iterations: self.smooth_iterations.unwrap_or(defaults.smooth_iterations),
            rebuild,
            curvature_alignment: self.curvature_alignment.unwrap_or(defaults.curvature_alignment).clamp(0.0, 1.0),
            adaptive_density: self.adaptive_density.unwrap_or(defaults.adaptive_density),
            symmetry,
            preserve_seams: self.follow_seams.unwrap_or(true),
        })
    }
}

/// Calidad de la malla de quads (ver `quadriflow_core::QualityReport`)
#[derive(Debug, Clone, Serialize)]
pub struct QuadQuality {
    /// Porcentaje de vértices interiores con valencia distinta de 4
    pub irregular_percent: f64,
    /// Quads con una esquina plegada
    pub folded_quads: usize,
    /// Quads deformes (alguna esquina fuera de ~30°–150°)
    pub poor_quads: usize,
    /// Quads con un lado más de 5 veces el otro
    pub stretched_quads: usize,
    /// Desviación media de los ángulos respecto de 90°, en grados
    pub mean_angle_deviation: f64,
    /// Distancia máxima a la malla original, en % de su diagonal
    pub max_distance_percent: Option<f64>,
}

impl From<quadriflow_core::QualityReport> for QuadQuality {
    fn from(r: quadriflow_core::QualityReport) -> Self {
        Self {
            irregular_percent: r.irregular_percent(),
            folded_quads: r.folded_quads,
            poor_quads: r.poor_quads,
            stretched_quads: r.stretched_quads,
            mean_angle_deviation: r.mean_angle_deviation,
            max_distance_percent: r.max_distance.map(|d| 100.0 * d),
        }
    }
}

/// Información del resultado de retopología
#[derive(Debug, Clone, Serialize)]
pub struct QuadMeshInfo {
    pub num_vertices: usize,
    pub num_quads: usize,
    pub bounding_box: BoundingBox,
    pub quality: QuadQuality,
    /// Caras que cruzan una costura del mapa UV original; `None` si el modelo
    /// no tenía UV
    pub uv_seam_faces: Option<usize>,
    /// Había rig y pasó a la malla nueva (mismo esqueleto, pesos trasladados)
    pub rig_kept: bool,
}

/// Datos de la malla de quads para Three.js
/// Los quads se triangulan para renderizado
#[derive(Debug, Clone)]
pub struct QuadMeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    /// UV por vértice si la malla tiene piel
    pub uvs: Option<Vec<f32>>,
    pub indices: Vec<u32>,
    /// Índices de quads originales (para visualización de wireframe)
    pub quad_indices: Vec<u32>,
    /// Con piel, los triángulos van ordenados por material de la piel:
    /// `[inicio, cantidad, material]` (`u32::MAX` = sin material)
    pub groups: Vec<[u32; 3]>,
}

/// Ejecuta `f` en un hilo de trabajo con acceso al estado.
///
/// Los comandos síncronos de Tauri corren en el hilo de la ventana: cualquier
/// trabajo pesado ahí congela la interfaz. Los comandos que tocan la malla
/// pasan por aquí.
pub(crate) async fn in_background<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| format!("La tarea terminó inesperadamente: {e}"))?
}

// ═══════════════════════════════════════════════════════════════════════════
// IMPORT COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Lista los formatos soportados
#[tauri::command]
pub fn get_supported_formats() -> SupportedFormats {
    SupportedFormats {
        import: vec![
            FormatInfo {
                id: "gltf".to_string(),
                name: "glTF/GLB".to_string(),
                extensions: vec!["gltf".to_string(), "glb".to_string()],
                description: "GL Transmission Format (recomendado)".to_string(),
            },
            FormatInfo {
                id: "obj".to_string(),
                name: "Wavefront OBJ".to_string(),
                extensions: vec!["obj".to_string()],
                description: "Formato clásico con soporte MTL".to_string(),
            },
            FormatInfo {
                id: "stl".to_string(),
                name: "STL".to_string(),
                extensions: vec!["stl".to_string()],
                description: "Stereolithography (impresión 3D)".to_string(),
            },
        ],
        export: vec![
            FormatInfo {
                id: "glb".to_string(),
                name: "GLB".to_string(),
                extensions: vec!["glb".to_string()],
                description: "glTF binario (recomendado)".to_string(),
            },
            FormatInfo {
                id: "gltf".to_string(),
                name: "glTF".to_string(),
                extensions: vec!["gltf".to_string()],
                description: "glTF separado: JSON + .bin".to_string(),
            },
            FormatInfo {
                id: "obj".to_string(),
                name: "Wavefront OBJ".to_string(),
                extensions: vec!["obj".to_string()],
                description: "OBJ + MTL + texturas".to_string(),
            },
            FormatInfo {
                id: "stl".to_string(),
                name: "STL".to_string(),
                extensions: vec!["stl".to_string()],
                description: "Solo geometría (impresión 3D)".to_string(),
            },
            FormatInfo {
                id: "3mf".to_string(),
                name: "3MF".to_string(),
                extensions: vec!["3mf".to_string()],
                description: "Impresión 3D con unidades y colores".to_string(),
            },
            FormatInfo {
                id: "ply".to_string(),
                name: "PLY".to_string(),
                extensions: vec!["ply".to_string()],
                description: "Geometría con color por vértice (escaneo)".to_string(),
            },
            FormatInfo {
                id: "usdz".to_string(),
                name: "USDZ".to_string(),
                extensions: vec!["usdz".to_string()],
                description: "Universal Scene Description (Apple AR)".to_string(),
            },
            FormatInfo {
                id: "bvh".to_string(),
                name: "BVH".to_string(),
                extensions: vec!["bvh".to_string()],
                description: "Esqueleto y animación (captura de movimiento)".to_string(),
            },
            FormatInfo {
                id: "json".to_string(),
                name: "JSON (pesos)".to_string(),
                extensions: vec!["json".to_string()],
                description: "Pesos de skinning en JSON".to_string(),
            },
        ],
    }
}

/// Importa un modelo 3D (auto-detecta formato)
#[tauri::command]
pub async fn import_model(app: AppHandle, path: String, on_progress: Channel<Progress>) -> Result<MeshInfo, String> {
    in_background(app, move |state| import_model_impl(path, &on_progress, state)).await
}

fn import_model_impl(path: String, progress: &Channel<Progress>, state: &AppState) -> Result<MeshInfo, String> {
    let path = Path::new(&path);
    report(progress, "reading", 5, "Leyendo archivo...");

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or("No se puede determinar el formato del archivo")?;

    // Import usando converter apropiado
    let scene = match ext.as_str() {
        "gltf" | "glb" => {
            converter_gltf_io::import_gltf(path)
                .map_err(|e| format!("Error importando glTF: {:?}", e))?
        }
        "obj" => {
            converter_obj::import_obj(path)
                .map_err(|e| format!("Error importando OBJ: {:?}", e))?
        }
        "stl" => {
            converter_stl::import_stl(path)
                .map_err(|e| format!("Error importando STL: {:?}", e))?
        }
        _ => return Err(format!("Formato no soportado: .{}", ext)),
    };

    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    load_scene(scene, name, ext.to_uppercase(), progress, state)
}

/// Deja `scene` como el modelo de trabajo (desde un archivo o el escáner)
pub(crate) fn load_scene(
    scene: Scene,
    name: String,
    format: String,
    progress: &Channel<Progress>,
    state: &AppState,
) -> Result<MeshInfo, String> {
    // Calcular estadísticas
    let (num_vertices, num_faces, has_normals, has_uvs) = calculate_scene_stats(&scene);
    let bbox = calculate_scene_bounds(&scene);

    let info = MeshInfo {
        num_vertices,
        num_faces,
        num_meshes: scene.meshes.len(),
        has_normals,
        has_uvs,
        has_materials: !scene.materials.is_empty(),
        bounding_box: bbox,
        format: format.clone(),
        rig: None,
    };

    // Convertir Scene a Mesh de pinocchio para autorig
    report(progress, "converting", 60, "Preparando malla...");
    let mesh = scene_to_pinocchio_mesh(&scene)?;
    let rig = if scene.skeletons.is_empty() {
        None
    } else {
        report(progress, "rig", 75, "Leyendo esqueleto y animaciones...");
        crate::imported_rig::from_scene(&scene, mesh.num_vertices())
    };
    report(progress, "done", 90, "Modelo importado");

    // Guardar en estado; la copia del original va al proyecto y permite revertir
    *state.original_model.lock().unwrap() = Some(crate::state::OriginalModel { name, format, scene: scene.clone() });
    *state.last_saved_hash.lock().unwrap() = None;
    let mut scene_lock = state.scene.lock().unwrap();
    *scene_lock = Some(scene);

    let mut mesh_lock = state.mesh.lock().unwrap();
    *mesh_lock = Some(mesh);

    // Descartar resultados, backups y piezas del modelo anterior
    drop(scene_lock);
    drop(mesh_lock);
    state.reset_derived();

    // El rig del archivo reemplaza al esqueleto anterior; sin rig, el
    // esqueleto elegido se conserva para ajustarlo al modelo nuevo
    let info = match rig {
        Some(rig) => {
            let skeleton = SkeletonType::Custom(rig.skeleton);
            *state.skeleton.lock().unwrap() = Some(skeleton.clone());
            *state.original_skeleton.lock().unwrap() = Some(skeleton);
            *state.skeleton_preset.lock().unwrap() = None;
            *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
            state.rig_on_quad.store(false, std::sync::atomic::Ordering::SeqCst);
            let num_bones = rig.output.attachment.num_bones();
            *state.result.lock().unwrap() = Some(rig.output);
            MeshInfo { rig: Some(ImportedRigInfo { num_bones, clips: rig.clips }), ..info }
        }
        None => info,
    };

    Ok(info)
}

/// Obtiene los datos de la malla para renderizar en Three.js
///
/// La geometría va en espacio mundo y en el mismo orden de vértices que la
/// malla de pinocchio (`scene_to_mesh`), así los pesos se aplican por índice.
#[tauri::command]
pub async fn get_mesh_data(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |state| get_mesh_data_impl(state).map(|d| d.to_bytes())).await?;
    Ok(Response::new(bytes))
}

fn get_mesh_data_impl(state: &AppState) -> Result<MeshData, String> {
    let scene_lock = state.scene.lock().unwrap();
    let scene = scene_lock.as_ref().ok_or("No hay escena cargada")?;
    Ok(scene_mesh_data(scene))
}

fn scene_mesh_data(scene: &Scene) -> MeshData {
    let prims = scene.world_primitives();
    let has_uvs = prims.iter().any(|p| p.uvs.is_some());

    let mut positions: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut uvs: Vec<f32> = Vec::new();
    let mut groups: Vec<[u32; 3]> = Vec::new();

    for prim in &prims {
        let offset = (positions.len() / 3) as u32;
        groups.push([indices.len() as u32, (prim.triangles.len() * 3) as u32, prim.material.map_or(u32::MAX, |m| m as u32)]);
        positions.extend(prim.positions.iter().flatten());

        match &prim.normals {
            Some(n) => normals.extend(n.iter().flatten()),
            None => normals.extend(compute_vertex_normals(&prim.positions, &prim.triangles).iter().flatten()),
        }

        // Si alguna primitiva tiene UVs, las demás se rellenan para mantener la alineación
        if has_uvs {
            match &prim.uvs {
                Some(uv) => uvs.extend(uv.iter().flatten()),
                None => uvs.extend(std::iter::repeat_n(0.0, prim.positions.len() * 2)),
            }
        }

        indices.extend(prim.triangles.iter().flatten().map(|&i| i + offset));
    }

    MeshData {
        positions,
        normals,
        indices,
        uvs: if has_uvs { Some(uvs) } else { None },
        groups,
    }
}

/// Normales por vértice ponderadas por área
fn compute_vertex_normals(positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let mut acc = vec![[0.0f32; 3]; positions.len()];
    for t in triangles {
        let [a, b, c] = t.map(|i| positions[i as usize]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &i in t {
            for k in 0..3 {
                acc[i as usize][k] += n[k];
            }
        }
    }
    acc.into_iter()
        .map(|n| {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-12 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 1.0, 0.0] }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// EXPORT COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Exporta el modelo actual
#[tauri::command]
pub async fn export_model(app: AppHandle, config: ExportConfig) -> Result<ExportResult, String> {
    in_background(app, move |state| export_model_impl(config, state)).await
}

fn export_model_impl(config: ExportConfig, state: &AppState) -> Result<ExportResult, String> {
    if config.format == "json" {
        return export_weights_json(&config, state);
    }
    if config.format == "bvh" {
        return export_bvh(&config, state);
    }
    let skeleton_only = config.skeleton_only.unwrap_or(false);
    if skeleton_only && !matches!(config.format.as_str(), "glb" | "gltf") {
        return Err(format!("Solo el esqueleto se exporta en GLB, glTF o BVH, no en {}", config.format));
    }
    let export_scene =
        if skeleton_only { skeleton_scene(&config, state)? } else { build_export_scene(&config, state)? };
    let scene = &export_scene;

    let path = Path::new(&config.path);
    let mut files_created = Vec::new();

    match config.format.as_str() {
        "glb" | "gltf" => {
            let glb_opts = converter_gltf_io::GlbExportOptions {
                texture_quality: config.texture_quality,
                max_texture_size: config.max_texture_size,
                optimize_geometry: config.optimize_geometry.unwrap_or(false),
                generate_normals: config.generate_normals.unwrap_or(false),
                flatten_transforms: config.flatten_transforms.unwrap_or(false),
                scale_factor: config.scale_factor,
                export_animations: config.export_animations.unwrap_or(true),
                strip_unused: config.strip_unused.unwrap_or(false),
                simplify: config.simplify_ratio.filter(|&r| r < 1.0).map(|ratio| converter_gltf_io::Simplification {
                    ratio,
                    max_error: config.simplify_error.unwrap_or(0.01),
                }),
                draco: config.draco.unwrap_or(false).then(|| {
                    let defaults = converter_gltf_io::DracoOptions::default();
                    converter_gltf_io::DracoOptions {
                        compression_level: config.draco_level.unwrap_or(defaults.compression_level),
                        position_bits: config.draco_position_bits.unwrap_or(defaults.position_bits),
                        ..defaults
                    }
                }),
            };
            if config.format == "gltf" {
                let bin = converter_gltf_io::export_gltf(scene, path, &glb_opts)
                    .map_err(|e| format!("Error exportando glTF: {e}"))?;
                files_created.push(config.path.clone());
                if bin.exists() {
                    files_created.push(bin.to_string_lossy().to_string());
                }
            } else {
                converter_gltf_io::export_glb(scene, path, &glb_opts)
                    .map_err(|e| format!("Error exportando GLB: {e}"))?;
                files_created.push(config.path.clone());
            }
        }
        "obj" => {
            converter_obj::export_obj(scene, path)
                .map_err(|e| format!("Error exportando OBJ: {:?}", e))?;
            files_created.push(config.path.clone());

            // OBJ también crea MTL y texturas
            let mtl_path = path.with_extension("mtl");
            if mtl_path.exists() {
                files_created.push(mtl_path.to_string_lossy().to_string());
            }
        }
        "stl" => {
            converter_stl::export_stl(scene, path)
                .map_err(|e| format!("Error exportando STL: {:?}", e))?;
            files_created.push(config.path.clone());
        }
        "ply" => {
            converter_ply::export_ply(scene, path).map_err(|e| format!("Error exportando PLY: {e}"))?;
            files_created.push(config.path.clone());
        }
        "3mf" => {
            converter_3mf::export_3mf(scene, path).map_err(|e| format!("Error exportando 3MF: {e}"))?;
            files_created.push(config.path.clone());
        }
        "usdz" => {
            let usda_opts = UsdaExportOptions {
                scale_factor: config.scale_factor,
                max_texture_size: config.max_texture_size,
                split_orm_channels: false,
                arkit_compatible: config.arkit_compatible.unwrap_or(false),
                fps: config.fps.unwrap_or(24.0),
                export_animations: config.export_animations.unwrap_or(true),
                keyframe_tolerance: 1e-4,
            };
            write_usdz(scene, &usda_opts, path)
                .map_err(|e| format!("Error exportando USDZ: {:?}", e))?;
            files_created.push(config.path.clone());
        }
        _ => return Err(format!("Formato de exportación no soportado: {}", config.format)),
    }

    let total_bytes = total_size(&files_created);
    Ok(ExportResult {
        success: true,
        path: config.path,
        message: "Exportación completada".to_string(),
        files_created,
        total_bytes,
    })
}

fn total_size(files: &[String]) -> u64 {
    files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum()
}

/// Escena a exportar según las opciones: geometría original o retopologizada,
/// con o sin el rig del autorig.
fn build_export_scene(config: &ExportConfig, state: &AppState) -> Result<Scene, String> {
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena para exportar")?;
    let use_retopology = config.use_retopology.unwrap_or(false);

    // Con retopología, el vértice de la malla de quads de cada vértice exportado
    let (geometry, quad_vertex_of) = if use_retopology {
        let quad = state.quad_mesh.lock().unwrap();
        let skin = state.quad_skin.lock().unwrap();
        let (geometry, source) =
            quad_mesh_to_scene(quad.as_ref().ok_or("No hay malla retopologizada")?, skin.as_ref(), &scene);
        (geometry, Some(source))
    } else {
        (scene, None)
    };
    if !config.include_weights.unwrap_or(false) {
        return Ok(geometry);
    }

    let result_lock = state.result.lock().unwrap();
    let result = result_lock.as_ref().ok_or("No hay resultado de autorig para exportar el rig")?;
    let skeleton = {
        let skeleton_lock = state.skeleton.lock().unwrap();
        to_basic_skeleton(skeleton_lock.as_ref().ok_or("No hay esqueleto")?)
    };

    let source_weights: Vec<Vec<f64>> = (0..result.attachment.num_vertices())
        .map(|v| result.get_weights(v).to_vec())
        .collect();
    let prims = geometry.world_primitives();
    let vertex_weights = if let Some(quad_vertex_of) = quad_vertex_of {
        let weights = weights_for(state, source_weights, WeightTarget::Retopology)?;
        quad_vertex_of.iter().map(|&v| weights[v].clone()).collect()
    } else {
        weights_for(state, source_weights, WeightTarget::Original)?
    };

    let vertex_count: usize = prims.iter().map(|p| p.positions.len()).sum();
    if vertex_weights.len() != vertex_count {
        return Err(format!(
            "El rig no corresponde a la malla actual ({} pesos, {} vértices): vuelve a ejecutar el autorig",
            vertex_weights.len(),
            vertex_count
        ));
    }
    let vertex_weights = match config.non_deforming.as_deref() {
        Some(bones) if !bones.is_empty() => reassign_non_deforming(vertex_weights, &skeleton, bones),
        _ => vertex_weights,
    };
    let clips = config.animations.as_deref().unwrap_or_default();
    Ok(rigged_scene(&geometry, &prims, &vertex_weights, &skeleton, &result.bone_positions, clips))
}

/// Pasa el peso de los huesos que no deforman al primer ancestro que sí
/// (el propio hueso si ninguno deforma)
fn reassign_non_deforming(weights: Vec<Vec<f64>>, skeleton: &BasicSkeleton, non_deforming: &[usize]) -> Vec<Vec<f64>> {
    let n = skeleton.num_bones();
    let off: std::collections::HashSet<usize> = non_deforming.iter().copied().filter(|&b| b < n).collect();
    let target: Vec<usize> = (0..n)
        .map(|b| {
            let mut t = Some(b);
            while let Some(j) = t.filter(|j| off.contains(j)) {
                t = skeleton.get_parent(j);
            }
            t.unwrap_or(b)
        })
        .collect();
    weights
        .into_iter()
        .map(|w| {
            let mut out = vec![0.0; w.len()];
            for (b, &x) in w.iter().enumerate() {
                out[target.get(b).copied().unwrap_or(b).min(w.len() - 1)] += x;
            }
            out
        })
        .collect()
}

/// Esqueleto actual y posición de cada articulación: la del autorig si lo hay
/// (el esqueleto ajustado a la malla), si no la del esqueleto tal cual
fn current_joints(state: &AppState) -> Result<(BasicSkeleton, Vec<Vector3>), String> {
    let skeleton = to_basic_skeleton(state.skeleton.lock().unwrap().as_ref().ok_or("No hay esqueleto")?);
    let positions = match state.result.lock().unwrap().as_ref() {
        Some(result) if result.bone_positions.len() == skeleton.num_bones() => result.bone_positions.clone(),
        _ => skeleton.bones().iter().map(|b| b.position).collect(),
    };
    Ok((skeleton, positions))
}

/// Escena con solo el esqueleto (un nodo por articulación, con su skin) y las
/// animaciones. Con `bone_shapes`, un octaedro por hueso pegado a él, para
/// que se vea en visores que no dibujan esqueletos.
fn skeleton_scene(config: &ExportConfig, state: &AppState) -> Result<Scene, String> {
    let (skeleton, positions) = current_joints(state)?;
    let clips = config.animations.as_deref().unwrap_or_default();
    Ok(skeleton_only_scene(&skeleton, &positions, clips, config.bone_shapes.unwrap_or(false)))
}

fn skeleton_only_scene(
    skeleton: &BasicSkeleton,
    positions: &[Vector3],
    clips: &[animation::AnimationClip],
    with_shapes: bool,
) -> Scene {
    let (prims, weights) = if with_shapes { bone_shapes(skeleton, positions) } else { (vec![], vec![]) };
    let with_shapes = !prims.is_empty();
    let base = Scene::new();
    let mut scene = rigged_scene(&base, &prims, &weights, skeleton, positions, clips);
    if !with_shapes {
        // Sin malla: el nodo 0 queda como contenedor del esqueleto
        scene.meshes.clear();
        let roots: Vec<usize> = scene.root_nodes.iter().copied().filter(|&n| n != 0).collect();
        let node = &mut scene.nodes[0];
        node.name = "esqueleto".into();
        node.mesh = None;
        node.skin = None;
        node.children = roots;
        scene.root_nodes = vec![0];
    } else if let Some(mesh) = scene.meshes.first_mut() {
        mesh.name = "huesos".into();
        scene.nodes[0].name = "huesos".into();
    }
    scene
}

/// Octaedro de cada hueso (del padre a la articulación), con peso 1 en su hueso
fn bone_shapes(skeleton: &BasicSkeleton, positions: &[Vector3]) -> (Vec<converter_scene::WorldPrimitive>, Vec<Vec<f64>>) {
    use converter_scene::glam::Vec3;
    let n = skeleton.num_bones();
    let v = |p: Vector3| Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32);
    let mut out_positions = Vec::new();
    let mut normals = Vec::new();
    let mut weights = Vec::new();
    for b in 0..n {
        let Some(parent) = skeleton.get_parent(b) else { continue };
        let (head, tail) = (v(positions[parent]), v(positions[b]));
        let axis = tail - head;
        let len = axis.length();
        if len < 1e-6 {
            continue;
        }
        let dir = axis / len;
        let side = dir.any_orthonormal_vector() * (0.1 * len);
        let other = dir.cross(side);
        let waist = head + axis * 0.2;
        let ring = [waist + side, waist + other, waist - side, waist - other];
        for i in 0..4 {
            let (a, c) = (ring[i], ring[(i + 1) % 4]);
            for tri in [[head, c, a], [tail, a, c]] {
                let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
                for p in tri {
                    out_positions.push(p.to_array());
                    normals.push(normal.to_array());
                    let mut w = vec![0.0; n];
                    w[b] = 1.0;
                    weights.push(w);
                }
            }
        }
    }
    if out_positions.is_empty() {
        return (vec![], vec![]);
    }
    let triangles = (0..out_positions.len() as u32 / 3).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect();
    let prim = converter_scene::WorldPrimitive {
        instance: 0,
        mesh: 0,
        primitive: 0,
        node: None,
        positions: out_positions,
        normals: Some(normals),
        uvs: None,
        colors: None,
        triangles,
        material: None,
    };
    (vec![prim], weights)
}

/// BVH del esqueleto con la primera animación que llega (el frontend manda la activa)
fn export_bvh(config: &ExportConfig, state: &AppState) -> Result<ExportResult, String> {
    use converter_scene::glam::Vec3;
    let (skeleton, positions) = current_joints(state)?;
    let joints: Vec<crate::bvh::BvhJoint> = skeleton
        .bones()
        .iter()
        .zip(&positions)
        .map(|(bone, p)| crate::bvh::BvhJoint {
            name: &bone.name,
            parent: bone.parent,
            position: Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32),
        })
        .collect();
    let rest = animation::AnimationClip {
        name: "reposo".into(),
        fps: config.fps.unwrap_or(24.0) as f32,
        tracks: vec![],
        start: None,
        end: None,
    };
    let clip = config.animations.as_ref().and_then(|c| c.first()).unwrap_or(&rest);
    let text = crate::bvh::write_bvh(&joints, clip)?;
    std::fs::write(&config.path, text).map_err(|e| format!("Error escribiendo BVH: {e}"))?;
    let files_created = vec![config.path.clone()];
    Ok(ExportResult {
        success: true,
        path: config.path.clone(),
        message: "Exportación completada".to_string(),
        total_bytes: total_size(&files_created),
        files_created,
    })
}

/// Malla de quads como escena (triangulada, en espacio mundo), con su piel si
/// la tiene. Devuelve también el vértice de quads de cada vértice exportado.
fn quad_mesh_to_scene(
    quad: &quadriflow_core::QuadMesh,
    skin: Option<&uv_core::Skin<4>>,
    base: &Scene,
) -> (Scene, Vec<usize>) {
    let (positions, faces) = quad_arrays(quad);
    uv_core::skin_scene(&positions, &faces, skin, base)
}

/// Construye una escena con skin a partir de la geometría (en espacio mundo),
/// los pesos por vértice `[vértice][hueso]` y el esqueleto embebido.
///
/// Pinocchio asigna el peso del hueso `b` al segmento `padre(b) → b`; en glTF/USD
/// un vértice sigue a un joint. Se crea un joint por hueso ubicado en la cabeza
/// de su segmento (la posición del padre) y colgando del joint del padre, como
/// los huesos de Blender: rotar un joint mueve su segmento y todo lo que cuelga.
///
/// Cada joint es además un nodo de la escena (después del de la malla), para
/// que las animaciones de la línea de tiempo tengan a qué apuntar.
fn rigged_scene(
    base: &Scene,
    prims: &[converter_scene::WorldPrimitive],
    vertex_weights: &[Vec<f64>],
    skeleton: &BasicSkeleton,
    joint_positions: &[Vector3],
    clips: &[animation::AnimationClip],
) -> Scene {
    use converter_scene::glam::{Mat4, Vec3};
    use converter_scene::{Joint, Mesh as SceneMesh, Node, Primitive, Skeleton as SceneSkeleton, Transform};

    const MAX_INFLUENCES: usize = 4;
    let num_bones = skeleton.num_bones();
    let to_vec3 = |p: Vector3| Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32);
    let head = |b: usize| to_vec3(skeleton.get_parent(b).map_or(joint_positions[b], |p| joint_positions[p]));

    let parents: Vec<Option<usize>> = (0..num_bones).map(|b| skeleton.get_parent(b)).collect();
    let rest: Vec<Vec3> = (0..num_bones)
        .map(|b| match parents[b] {
            Some(p) => head(b) - head(p),
            None => head(b),
        })
        .collect();
    let joint_node = |b: usize| 1 + b;
    let joints: Vec<Joint> = (0..num_bones)
        .map(|b| Joint {
            name: skeleton.bones()[b].name.clone(),
            children: skeleton.get_children(b),
            inverse_bind_matrix: Mat4::from_translation(-head(b)),
            local_transform: Mat4::from_translation(rest[b]),
            node_index: Some(joint_node(b)),
        })
        .collect();
    let roots: Vec<usize> = (0..num_bones).filter(|&b| parents[b].is_none()).collect();
    let joint_nodes = (0..num_bones).map(|b| Node {
        name: skeleton.bones()[b].name.clone(),
        transform: Transform::Trs { translation: rest[b], rotation: Default::default(), scale: Vec3::ONE },
        mesh: None,
        skin: None,
        children: skeleton.get_children(b).into_iter().map(joint_node).collect(),
    });
    let joint_node_indices: Vec<usize> = (0..num_bones).map(joint_node).collect();
    let rig = animation::SkinRig { parents: &parents, joint_nodes: &joint_node_indices, rest_translation: &rest };

    // Hasta 4 influencias por vértice, renormalizadas
    let top_weights = |w: &[f64]| -> ([u16; 4], [f32; 4]) {
        let mut idx: Vec<usize> = (0..w.len()).collect();
        idx.sort_by(|&a, &b| w[b].total_cmp(&w[a]));
        let mut joints = [0u16; MAX_INFLUENCES];
        let mut weights = [0f32; MAX_INFLUENCES];
        let sum: f64 = idx.iter().take(MAX_INFLUENCES).map(|&i| w[i].max(0.0)).sum();
        for (slot, &i) in idx.iter().take(MAX_INFLUENCES).enumerate() {
            weights[slot] = if sum > 0.0 { (w[i].max(0.0) / sum) as f32 } else if slot == 0 { 1.0 } else { 0.0 };
            // Una casilla sin peso apunta al joint 0 (glTF avisa si no)
            joints[slot] = if weights[slot] > 0.0 { i as u16 } else { 0 };
        }
        (joints, weights)
    };

    let mut offset = 0;
    let primitives = prims
        .iter()
        .map(|prim| {
            let count = prim.positions.len();
            let (joint_idx, joint_w): (Vec<[u16; 4]>, Vec<[f32; 4]>) =
                vertex_weights[offset..offset + count].iter().map(|w| top_weights(w)).unzip();
            offset += count;

            let mut attributes = vec![VertexAttribute::Positions(prim.positions.clone())];
            if let Some(n) = &prim.normals {
                attributes.push(VertexAttribute::Normals(n.clone()));
            }
            if let Some(uv) = &prim.uvs {
                attributes.push(VertexAttribute::TexCoords(0, uv.clone()));
            }
            attributes.push(VertexAttribute::JointIndices(joint_idx));
            attributes.push(VertexAttribute::JointWeights(joint_w));
            Primitive {
                attributes,
                indices: Some(IndexData::U32(prim.triangles.iter().flatten().copied().collect())),
                material: prim.material,
            }
        })
        .collect();

    let mesh_node = Node {
        name: "rigged".into(),
        transform: Transform::identity(),
        mesh: Some(0),
        skin: Some(0),
        children: vec![],
    };
    Scene {
        meshes: vec![SceneMesh { name: "rigged".into(), primitives }],
        nodes: std::iter::once(mesh_node).chain(joint_nodes).collect(),
        root_nodes: std::iter::once(0).chain(roots.iter().map(|&r| joint_node(r))).collect(),
        materials: base.materials.clone(),
        textures: base.textures.clone(),
        animations: animation::scene_animations(clips, &rig),
        skeletons: vec![SceneSkeleton { name: "pinocchio".into(), joints, roots }],
        meters_per_unit: base.meters_per_unit,
        y_up: base.y_up,
    }
}

/// Pesos del autorig llevados a los vértices de la malla retopologizada, en el
/// orden de `quad.vertices` (el mismo de [`quad_mesh_to_scene`]).
fn weights_on_quad_mesh(
    mesh: &Mesh,
    source_weights: &[Vec<f64>],
    quad: &quadriflow_core::QuadMesh,
) -> Result<Vec<Vec<f64>>, String> {
    if source_weights.len() != mesh.num_vertices() {
        return Err(format!(
            "El rig no corresponde a la malla actual ({} pesos, {} vértices): vuelve a ejecutar el autorig",
            source_weights.len(),
            mesh.num_vertices()
        ));
    }
    let targets: Vec<Vector3> = quad.vertices.iter().map(|v| Vector3::new(v.x, v.y, v.z)).collect();
    Ok(transfer_weights(mesh, source_weights, &targets))
}

/// Lleva el rig a otra malla de la misma forma (la retopología, o la otra
/// malla activa): el esqueleto queda igual y cada vértice nuevo toma los pesos
/// del punto más cercano de `source`, con las mismas influencias por vértice
/// como máximo. `false` si el rig no era de `source`.
fn move_rig(rig: &mut PinocchioOutput, source: &Mesh, target: &Mesh) -> bool {
    let attachment = &rig.attachment;
    if attachment.num_vertices() != source.num_vertices() {
        return false;
    }
    let weights: Vec<Vec<f64>> = (0..attachment.num_vertices()).map(|v| attachment.get_weights(v).to_vec()).collect();
    let max_influences = weights.iter().map(|w| w.iter().filter(|&&x| x > 0.0).count()).max().unwrap_or(1).max(1);
    let targets: Vec<Vector3> = target.vertices.iter().map(|v| v.position).collect();
    let mut moved = Attachment::new(target, transfer_weights(source, &weights, &targets), attachment.num_bones());
    moved.compact_weights(max_influences);
    rig.attachment = moved;
    rig.stats.num_vertices = target.num_vertices();
    true
}

/// Las `max_influences` influencias dominantes de cada vértice (renormalizadas),
/// rellenas con hueso 0 y peso 0 hasta completar.
fn influence_table(weights: &[Vec<f64>], max_influences: usize) -> (Vec<Vec<usize>>, Vec<Vec<f64>>) {
    weights
        .iter()
        .map(|w| {
            let mut dominant = pinocchio_attachment::dominant_influences(w, max_influences);
            dominant.resize(max_influences, (0, 0.0));
            dominant.into_iter().unzip()
        })
        .unzip()
}

fn export_weights_json(config: &ExportConfig, state: &AppState) -> Result<ExportResult, String> {
    let result_lock = state.result.lock().unwrap();
    let result = result_lock.as_ref().ok_or("No hay resultado de autorig")?;

    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto")?;

    let source_weights: Vec<Vec<f64>> = (0..result.attachment.num_vertices())
        .map(|v| result.get_weights(v).to_vec())
        .collect();
    let (source, num_vertices, num_faces, vertex_weights) = if config.use_retopology.unwrap_or(false) {
        let weights = weights_for(state, source_weights, WeightTarget::Retopology)?;
        let quad_lock = state.quad_mesh.lock().unwrap();
        let quad = quad_lock.as_ref().ok_or("No hay malla retopologizada")?;
        ("retopology", quad.num_vertices(), quad.num_faces(), weights)
    } else {
        let weights = weights_for(state, source_weights, WeightTarget::Original)?;
        let mesh_lock = state.mesh.lock().unwrap();
        let mesh = mesh_lock.as_ref().ok_or("No hay malla")?;
        ("original", mesh.num_vertices(), mesh.num_faces(), weights)
    };

    let max_influences = 4;
    let (indices, weights) = influence_table(&vertex_weights, max_influences);

    let bone_names: Vec<String> = get_bone_names(skeleton_type);

    let export_data = serde_json::json!({
        "version": "1.0",
        "mesh": {
            // "retopology": índices de vértice de la malla de quads exportada
            "source": source,
            "num_vertices": num_vertices,
            "num_faces": num_faces,
        },
        "skeleton": {
            "bones": bone_names,
            "positions": result.bone_positions.iter()
                .map(|p| [p.x(), p.y(), p.z()])
                .collect::<Vec<_>>(),
        },
        "weights": {
            "max_influences": max_influences,
            "bone_indices": indices,
            "bone_weights": weights,
        },
        "stats": {
            "num_vertices": result.stats.num_vertices,
            "num_bones": result.stats.num_bones,
            "embedding_quality": result.stats.embedding_quality,
            "avg_influences": result.stats.avg_influences_per_vertex,
        },
    });

    std::fs::write(&config.path, serde_json::to_string_pretty(&export_data).unwrap())
        .map_err(|e| format!("Error escribiendo archivo: {}", e))?;

    Ok(ExportResult {
        success: true,
        path: config.path.clone(),
        message: "Pesos exportados correctamente".to_string(),
        total_bytes: total_size(std::slice::from_ref(&config.path)),
        files_created: vec![config.path.clone()],
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// SKELETON COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Lista los presets de esqueleto disponibles
#[tauri::command]
pub fn list_skeleton_presets() -> Vec<SkeletonPreset> {
    [
        SkeletonPreset {
            id: "human".to_string(),
            name: "Humanoide".to_string(),
            description: "Esqueleto humano bípedo con 20 huesos".to_string(),
            num_bones: HumanSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "quad".to_string(),
            name: "Cuadrúpedo".to_string(),
            description: "Animal de 4 patas genérico".to_string(),
            num_bones: QuadSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "horse".to_string(),
            name: "Caballo".to_string(),
            description: "Cuadrúpedo con proporciones equinas".to_string(),
            num_bones: HorseSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "centaur".to_string(),
            name: "Centauro".to_string(),
            description: "Híbrido humano-caballo con 27 huesos".to_string(),
            num_bones: CentaurSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "bird".to_string(),
            name: "Ave".to_string(),
            description: "Esqueleto de ave con alas".to_string(),
            num_bones: BirdSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "spider".to_string(),
            name: "Araña".to_string(),
            description: "Arácnido de 8 patas".to_string(),
            num_bones: SpiderSkeleton::new().num_bones(),
        },
        SkeletonPreset {
            id: "serpent".to_string(),
            name: "Serpiente".to_string(),
            description: "Esqueleto vertebrado flexible".to_string(),
            num_bones: SerpentSkeleton::default().num_bones(),
        },
        SkeletonPreset {
            id: "mech".to_string(),
            name: "Mech".to_string(),
            description: "Robot bípedo mecánico".to_string(),
            num_bones: MechSkeleton::new().num_bones(),
        },
    ]
    .into_iter()
    .chain(pinocchio_skeleton::BodyPlan::variants().iter().map(|&(id, name, description)| SkeletonPreset {
        id: format!("plan:{id}"),
        name: name.to_string(),
        description: format!("{description} (apéndices configurables)"),
        num_bones: pinocchio_skeleton::BodyPlan::variant(id).map_or(0, |p| p.build().num_bones()),
    }))
    .collect()
}

/// Selecciona un preset de esqueleto
#[tauri::command]
pub fn select_skeleton(preset_id: String, state: State<'_, AppState>) -> Result<SkeletonData, String> {
    let skeleton_type = match preset_id.as_str() {
        "human" => SkeletonType::Human,
        "quad" => SkeletonType::Quad,
        "horse" => SkeletonType::Horse,
        "centaur" => SkeletonType::Centaur,
        "bird" => SkeletonType::Bird,
        "spider" => SkeletonType::Spider,
        "serpent" => SkeletonType::Serpent,
        "mech" => SkeletonType::Mech,
        other => match other.strip_prefix("plan:").and_then(pinocchio_skeleton::BodyPlan::variant) {
            Some(plan) => SkeletonType::Template(plan.build()),
            None => return Err(format!("Preset desconocido: {}", preset_id)),
        },
    };

    let data = get_skeleton_data_for_type(&skeleton_type);

    // El preset pasa a ser la base, sin transformación de gizmo
    *state.skeleton_preset.lock().unwrap() = Some(skeleton_type.clone());
    *state.skeleton.lock().unwrap() = Some(skeleton_type.clone());
    *state.original_skeleton.lock().unwrap() = Some(skeleton_type);
    *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
    *state.result.lock().unwrap() = None;

    Ok(data)
}

/// Borra un objeto de la escena desde el Outliner: "skeleton" (esqueleto y
/// todo lo que depende de él), "weights" (el rig) o "quadmesh" (la retopología
/// y su piel UV). La malla importada no se borra por acá.
#[tauri::command]
pub fn remove_object(kind: String, state: State<'_, AppState>) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    match kind.as_str() {
        "skeleton" => {
            *state.skeleton.lock().unwrap() = None;
            *state.original_skeleton.lock().unwrap() = None;
            *state.skeleton_preset.lock().unwrap() = None;
            *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
            state.active_mesh_changed();
        }
        "weights" => *state.result.lock().unwrap() = None,
        "quadmesh" => {
            // Un rig calculado sobre los quads se queda sin malla
            if state.rig_on_quad.swap(false, Ordering::SeqCst) {
                state.active_mesh_changed();
            }
            state.use_retopology.store(false, Ordering::SeqCst);
            *state.quad_mesh.lock().unwrap() = None;
            *state.quad_skin.lock().unwrap() = None;
        }
        other => return Err(format!("No se puede borrar: {other}")),
    }
    Ok(())
}

/// Forma de cuerpo + apéndices (ver `pinocchio_skeleton::BodyPlan`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyPlanDto {
    /// "biped", "digitigrade", "quadruped", "radial", "fish", "arthropod", "serpent"
    pub shape: String,
    pub neck: usize,
    pub tail: usize,
    pub trunk: usize,
    pub ears: usize,
    pub wings: usize,
    pub limbs: usize,
    pub limb_segments: usize,
    pub fins: bool,
    pub pincers: bool,
    pub antennae: usize,
}

const BODY_SHAPES: [(&str, pinocchio_skeleton::BodyShape); 7] = [
    ("biped", pinocchio_skeleton::BodyShape::Biped),
    ("digitigrade", pinocchio_skeleton::BodyShape::DigitigradeBiped),
    ("quadruped", pinocchio_skeleton::BodyShape::Quadruped),
    ("radial", pinocchio_skeleton::BodyShape::Radial),
    ("fish", pinocchio_skeleton::BodyShape::Fish),
    ("arthropod", pinocchio_skeleton::BodyShape::Arthropod),
    ("serpent", pinocchio_skeleton::BodyShape::Serpent),
];

impl From<pinocchio_skeleton::BodyPlan> for BodyPlanDto {
    fn from(p: pinocchio_skeleton::BodyPlan) -> Self {
        let shape = BODY_SHAPES.iter().find(|(_, s)| *s == p.shape).map_or("biped", |(id, _)| id);
        Self {
            shape: shape.to_string(),
            neck: p.neck,
            tail: p.tail,
            trunk: p.trunk,
            ears: p.ears,
            wings: p.wings,
            limbs: p.limbs,
            limb_segments: p.limb_segments,
            fins: p.fins,
            pincers: p.pincers,
            antennae: p.antennae,
        }
    }
}

impl BodyPlanDto {
    fn to_plan(&self) -> Result<pinocchio_skeleton::BodyPlan, String> {
        let shape = BODY_SHAPES
            .iter()
            .find(|(id, _)| *id == self.shape)
            .map(|(_, s)| *s)
            .ok_or_else(|| format!("Forma desconocida: {}", self.shape))?;
        // Límites para que la interfaz no pida esqueletos absurdos
        let clamp = |n: usize, max: usize| n.min(max);
        Ok(pinocchio_skeleton::BodyPlan {
            shape,
            neck: clamp(self.neck, 12).max(1),
            tail: clamp(self.tail, 32),
            trunk: clamp(self.trunk, 16),
            ears: clamp(self.ears, 4),
            wings: clamp(self.wings, 6),
            limbs: clamp(self.limbs, 12),
            limb_segments: clamp(self.limb_segments, 12),
            fins: self.fins,
            pincers: self.pincers,
            antennae: clamp(self.antennae, 6),
        })
    }
}

/// Plan de una variante con nombre (`plan:<id>`), para editar sus apéndices
#[tauri::command]
pub fn get_body_plan(preset_id: String) -> Option<BodyPlanDto> {
    let id = preset_id.strip_prefix("plan:")?;
    pinocchio_skeleton::BodyPlan::variant(id).map(Into::into)
}

/// Genera la plantilla de un plan (forma + apéndices) y la deja como preset
#[tauri::command]
pub fn select_body_plan(plan: BodyPlanDto, state: State<'_, AppState>) -> Result<SkeletonData, String> {
    let skeleton = SkeletonType::Template(plan.to_plan()?.build());
    let data = get_skeleton_data_for_type(&skeleton);
    *state.skeleton_preset.lock().unwrap() = Some(skeleton.clone());
    *state.skeleton.lock().unwrap() = Some(skeleton.clone());
    *state.original_skeleton.lock().unwrap() = Some(skeleton);
    *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
    *state.result.lock().unwrap() = None;
    Ok(data)
}

// ═══════════════════════════════════════════════════════════════════════════
// SKELETON TRANSFORM COMMANDS
// ═══════════════════════════════════════════════════════════════════════════
//
// El esqueleto visible es `gizmo(base)`: `original_skeleton` es la base (preset,
// auto-fit o con huesos editados) y `skeleton_transform` la transformación de
// gizmo. Cada edición mantiene esa relación para que no se pierda al cambiar
// de herramienta.

/// Aplica la transformación de gizmo: escala → rotación XYZ alrededor del
/// pivote → traslación
fn apply_gizmo(p: Vector3, t: &SkeletonTransformParams) -> Vector3 {
    let [rx, ry, rz] = t.rotation.map(f64::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let [px, py, pz] = t.pivot;
    let (x, y, z) = ((p.x() - px) * t.scale, (p.y() - py) * t.scale, (p.z() - pz) * t.scale);
    // X
    let (y, z) = (y * cx - z * sx, y * sx + z * cx);
    // Y
    let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
    // Z
    let (x, y) = (x * cz - y * sz, x * sz + y * cz);
    Vector3::new(x + px + t.translation[0], y + py + t.translation[1], z + pz + t.translation[2])
}

/// Inversa de [`apply_gizmo`]
fn invert_gizmo(p: Vector3, t: &SkeletonTransformParams) -> Vector3 {
    let [rx, ry, rz] = t.rotation.map(f64::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let [px, py, pz] = t.pivot;
    let (x, y, z) = (
        p.x() - t.translation[0] - px,
        p.y() - t.translation[1] - py,
        p.z() - t.translation[2] - pz,
    );
    // Z⁻¹
    let (x, y) = (x * cz + y * sz, -x * sz + y * cz);
    // Y⁻¹
    let (x, z) = (x * cy - z * sy, x * sy + z * cy);
    // X⁻¹
    let (y, z) = (y * cx + z * sx, -y * sx + z * cx);
    let inv = if t.scale.abs() > 1e-12 { 1.0 / t.scale } else { 1.0 };
    Vector3::new(x * inv + px, y * inv + py, z * inv + pz)
}

/// Centro de la caja de las articulaciones
fn joints_center(skeleton: &BasicSkeleton) -> [f64; 3] {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for b in skeleton.bones() {
        for (k, v) in [b.position.x(), b.position.y(), b.position.z()].into_iter().enumerate() {
            min[k] = min[k].min(v);
            max[k] = max[k].max(v);
        }
    }
    if skeleton.num_bones() == 0 {
        return [0.0; 3];
    }
    [0, 1, 2].map(|k| (min[k] + max[k]) / 2.0)
}

/// Pivote del esqueleto visible (el de la base, llevado por el gizmo)
fn visible_pivot(t: &SkeletonTransformParams) -> Option<[f64; 3]> {
    (!t.is_identity()).then(|| [0, 1, 2].map(|k| t.pivot[k] + t.translation[k]))
}

/// Copia editable de cualquier tipo de esqueleto
fn to_basic_skeleton(skeleton_type: &SkeletonType) -> BasicSkeleton {
    match skeleton_type {
        SkeletonType::Custom(s) | SkeletonType::Template(s) => s.clone(),
        other => {
            let data = get_skeleton_data_for_type(other);
            let bones = data
                .bones
                .iter()
                .map(|b| {
                    let pos = Vector3::new(b.position[0], b.position[1], b.position[2]);
                    let bone = match b.parent {
                        Some(parent) => Bone::with_parent(&b.name, pos, parent),
                        None => Bone::new(&b.name, pos),
                    };
                    if b.is_leaf { bone.as_leaf() } else { bone }
                })
                .collect();
            BasicSkeleton::from_bones(bones)
        }
    }
}

/// Base actual; si no hay, se toma el esqueleto visible con gizmo identidad
fn current_base(state: &AppState) -> Result<BasicSkeleton, String> {
    let mut orig = state.original_skeleton.lock().unwrap();
    if orig.is_none() {
        let skel = state.skeleton.lock().unwrap();
        *orig = Some(skel.as_ref().ok_or("No hay esqueleto seleccionado")?.clone());
        *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
    }
    Ok(to_basic_skeleton(orig.as_ref().unwrap()))
}

/// Transforma el esqueleto (escala, traslación, rotación) respecto de la base
#[tauri::command]
pub fn transform_skeleton(
    scale: f64,
    translation: [f64; 3],
    rotation: [f64; 3],
    state: State<'_, AppState>,
) -> Result<SkeletonData, String> {
    let base = current_base(&state)?;
    // El pivote se elige al salir de la identidad y se mantiene mientras dure
    // la transformación
    let current = *state.skeleton_transform.lock().unwrap();
    let pivot = if current.is_identity() { joints_center(&base) } else { current.pivot };
    let params = SkeletonTransformParams { scale, translation, rotation, pivot };
    let skel = pinocchio_skeleton::map_positions(&base, |p| apply_gizmo(p, &params));
    let mut data = skeleton_to_data(&skel);
    data.pivot = visible_pivot(&params);

    *state.skeleton_transform.lock().unwrap() = params;
    *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(skel));
    *state.result.lock().unwrap() = None;

    Ok(data)
}

/// Mueve un hueso individual (posición en coordenadas del esqueleto visible).
/// Con `mirror`, su par del otro lado (`hand_l` ↔ `hand_r`) se mueve al
/// reflejo en el plano de simetría del esqueleto.
#[tauri::command]
pub fn move_bone(
    bone_index: usize,
    position: [f64; 3],
    mirror: Option<bool>,
    state: State<'_, AppState>,
) -> Result<SkeletonData, String> {
    let mut base = current_base(&state)?;
    if bone_index >= base.num_bones() {
        return Err(format!("Índice de hueso fuera de rango: {}", bone_index));
    }
    let params = *state.skeleton_transform.lock().unwrap();

    // La base guarda la edición sin el gizmo, así sobrevive a cambios de transformación.
    // El plano de simetría se mide en la base antes de mover (la base no tiene
    // el giro del gizmo, así que el reflejo no depende de él)
    let pos = invert_gizmo(Vector3::new(position[0], position[1], position[2]), &params);
    let pair = mirror
        .unwrap_or(false)
        .then(|| pinocchio_skeleton::mirror_pairs(&base)[bone_index])
        .flatten();
    let plane = pair.and_then(|_| pinocchio_skeleton::symmetry_plane(&base));
    base.bones_mut()[bone_index].position = pos;
    if let (Some(pair), Some(plane)) = (pair, plane) {
        base.bones_mut()[pair].position = pinocchio_skeleton::reflect(pos, plane);
    }

    let skel = pinocchio_skeleton::map_positions(&base, |p| apply_gizmo(p, &params));
    let mut data = skeleton_to_data(&skel);
    data.pivot = visible_pivot(&params);

    *state.original_skeleton.lock().unwrap() = Some(SkeletonType::Custom(base));
    *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(skel));
    *state.result.lock().unwrap() = None;

    Ok(data)
}

/// Pose como reposo: cada articulación pasa a estar donde la deja la pose
/// (`positions`, en coordenadas del esqueleto visible). Los pesos se
/// conservan: la piel queda ligada a los huesos en su lugar nuevo, así que la
/// malla en reposo no cambia.
#[tauri::command]
pub fn apply_rest_pose(positions: Vec<[f64; 3]>, state: State<'_, AppState>) -> Result<SkeletonData, String> {
    let mut base = current_base(&state)?;
    if positions.len() != base.num_bones() {
        return Err(format!("Se esperaban {} articulaciones, llegaron {}", base.num_bones(), positions.len()));
    }
    let params = *state.skeleton_transform.lock().unwrap();
    let visible: Vec<Vector3> = positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
    for (bone, &p) in base.bones_mut().iter_mut().zip(&visible) {
        bone.position = invert_gizmo(p, &params);
    }
    let skel = pinocchio_skeleton::map_positions(&base, |p| apply_gizmo(p, &params));
    let mut data = skeleton_to_data(&skel);
    data.pivot = visible_pivot(&params);

    *state.original_skeleton.lock().unwrap() = Some(SkeletonType::Custom(base));
    *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(skel));
    if let Some(result) = state.result.lock().unwrap().as_mut() {
        if result.bone_positions.len() == visible.len() {
            result.bone_rest_transforms = visible.iter().map(|&p| pinocchio_math::Transform::from_translation(p)).collect();
            result.bone_positions = visible;
        }
    }
    Ok(data)
}

/// Resultado del ajuste automático
#[derive(Debug, Clone, Serialize)]
pub struct AutoFitResult {

    pub skeleton: SkeletonData,
    /// Similitud de proporciones con la plantilla (1 = idénticas)
    pub quality: f64,
    /// Extremidades detectadas en la malla
    pub extremities: usize,
    /// Puntas de extremidades que ningún hueso usa (trompa, orejas…)
    pub unused_extremities: Vec<[f64; 3]>,
}

/// Ajusta el esqueleto a la malla: detecta las extremidades, busca la
/// orientación de la plantilla y coloca cada articulación en el eje medial.
///
/// Parte del preset elegido (no de las ediciones) y usa el mismo algoritmo que
/// `autorig`. El resultado pasa a ser la base editable, sin gizmo.
#[tauri::command]
pub async fn auto_fit_skeleton(app: AppHandle) -> Result<AutoFitResult, String> {
    in_background(app, auto_fit_skeleton_impl).await
}

fn auto_fit_skeleton_impl(state: &AppState) -> Result<AutoFitResult, String> {
    let mesh = active_mesh(state)?;
    let preset = state.skeleton_preset.lock().unwrap().clone();
    let (template, fit) = match preset {
        Some(preset) => (to_basic_skeleton(&preset), SkeletonFit::Auto),
        None => {
            let skeleton_lock = state.skeleton.lock().unwrap();
            (to_basic_skeleton(skeleton_lock.as_ref().ok_or("No hay esqueleto seleccionado")?), SkeletonFit::None)
        }
    };
    let report = pinocchio_core::fit_to_mesh(&mesh, &template, fit, 96).map_err(|e| format!("No se pudo ajustar: {e}"))?;
    let data = skeleton_to_data(&report.skeleton);
    let to_array = |p: Vector3| [p.x(), p.y(), p.z()];

    *state.original_skeleton.lock().unwrap() = Some(SkeletonType::Custom(report.skeleton.clone()));
    *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
    *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(report.skeleton));
    *state.result.lock().unwrap() = None;

    Ok(AutoFitResult {
        skeleton: data,
        quality: report.quality,
        extremities: report.extremities.len(),
        unused_extremities: report.unused_extremities.iter().map(|&e| to_array(report.extremities[e])).collect(),
    })
}

/// Centra articulaciones en el volumen de la malla (en la sección del
/// miembro, sin deslizarlas a lo largo). Sin `bones`, todas.
#[tauri::command]
pub async fn center_bones(app: AppHandle, bones: Option<Vec<usize>>) -> Result<SkeletonData, String> {
    in_background(app, move |state| {
        let centering = {
            let mut cache = state.joint_centering.lock().unwrap();
            if cache.is_none() {
                let mesh = active_mesh(state)?;
                *cache = Some(std::sync::Arc::new(pinocchio_embedding::JointCentering::new(&mesh, 128)));
            }
            cache.clone().expect("recién calculado")
        };
        let visible = {
            let skeleton_lock = state.skeleton.lock().unwrap();
            to_basic_skeleton(skeleton_lock.as_ref().ok_or("No hay esqueleto seleccionado")?)
        };
        let joints = bones.unwrap_or_else(|| (0..visible.num_bones()).collect());
        let centered = centering.center(&visible, &joints);
        let data = skeleton_to_data(&centered);

        // El resultado es la nueva base, sin gizmo
        *state.original_skeleton.lock().unwrap() = Some(SkeletonType::Custom(centered.clone()));
        *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
        *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(centered));
        *state.result.lock().unwrap() = None;
        Ok(data)
    })
    .await
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTORIG COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Ejecuta el autorig con progress reporting
///
/// El cálculo corre en un hilo bloqueante para no ocupar el runtime async; el
/// flag `processing` se libera siempre (también ante errores o panics).
#[tauri::command]
pub async fn run_autorig(
    config: AutorigConfig,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _guard = state
        .try_begin_processing()
        .ok_or("Ya hay un proceso en curso")?;

    // La malla activa: la retopologizada si la hay, así los pesos se calculan
    // sobre la malla que se ve, se pinta y se exporta
    let on_quad = state.active_is_quad();
    let mesh = active_mesh(&state)?;
    let skeleton_type = state
        .skeleton
        .lock()
        .unwrap()
        .clone()
        .ok_or("No hay esqueleto seleccionado")?;

    let mut pinocchio_config = match config.quality.as_str() {
        "fast" => PinocchioConfig::fast(),
        "high" => PinocchioConfig::high_quality(),
        _ => PinocchioConfig::default(),
    };
    if let Some(dw) = config.diffusion_weight {
        pinocchio_config = pinocchio_config.with_diffusion_weight(dw);
    }
    if let Some(mi) = config.max_influences {
        pinocchio_config = pinocchio_config.with_max_influences(mi);
    }
    // Los presets son plantillas que autorig encaja en la malla; un esqueleto
    // Custom ya fue ajustado o editado sobre la malla y se usa tal cual.
    let pinocchio_config = pinocchio_config.with_skeleton_fit(match skeleton_type {
        SkeletonType::Custom(_) => SkeletonFit::Exact,
        _ => SkeletonFit::Auto,
    });

    let progress = on_progress.clone();
    let output = tauri::async_runtime::spawn_blocking(move || {
        let report = |stage: AutorigStage| {
            let message = match stage {
                AutorigStage::Preparing => "Preparando malla...",
                AutorigStage::Embedding => "Ajustando el esqueleto a la malla...",
                AutorigStage::Weights => "Calculando pesos de skinning...",
                AutorigStage::Done => "Terminado",
            };
            let _ = progress.send(Progress {
                stage: stage.name().to_string(),
                percent: stage.progress(),
                message: message.to_string(),
            });
        };
        let config = Some(pinocchio_config);
        match &skeleton_type {
            SkeletonType::Human => autorig_with_progress(&mesh, &HumanSkeleton::new(), config, report),
            SkeletonType::Quad => autorig_with_progress(&mesh, &QuadSkeleton::new(), config, report),
            SkeletonType::Horse => autorig_with_progress(&mesh, &HorseSkeleton::new(), config, report),
            SkeletonType::Centaur => autorig_with_progress(&mesh, &CentaurSkeleton::new(), config, report),
            SkeletonType::Bird => autorig_with_progress(&mesh, &BirdSkeleton::new(), config, report),
            SkeletonType::Spider => autorig_with_progress(&mesh, &SpiderSkeleton::new(), config, report),
            SkeletonType::Serpent => autorig_with_progress(&mesh, &SerpentSkeleton::default(), config, report),
            SkeletonType::Mech => autorig_with_progress(&mesh, &MechSkeleton::new(), config, report),
            SkeletonType::Custom(skel) | SkeletonType::Template(skel) => {
                autorig_with_progress(&mesh, skel, config, report)
            }
        }
    })
    .await
    .map_err(|e| format!("El autorig terminó inesperadamente: {e}"))?
    .map_err(|e| format!("Error en autorig: {e}"))?;

    let _ = on_progress.send(Progress {
        stage: "done".to_string(),
        percent: 100,
        message: format!(
            "Completado: {} vértices, {} huesos",
            output.stats.num_vertices, output.stats.num_bones
        ),
    });
    state.rig_on_quad.store(on_quad, std::sync::atomic::Ordering::SeqCst);
    *state.result.lock().unwrap() = Some(output);
    Ok(())
}

/// Obtiene los datos de pesos para visualización
#[tauri::command]
pub async fn get_weights_data(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |state| get_weights_data_impl(state).map(|d| d.to_bytes())).await?;
    Ok(Response::new(bytes))
}

/// Pesos en el orden de vértices del visor (con el rig sobre la malla de
/// quads, los vértices duplicados en costuras UV repiten la fila de su vértice)
fn get_weights_data_impl(state: &AppState) -> Result<WeightsData, String> {
    let view_map = rig_view_map(state)?;
    let result_lock = state.result.lock().unwrap();
    let result = result_lock.as_ref().ok_or("No hay resultado de autorig")?;

    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto")?;

    let bone_names = get_bone_names(skeleton_type);
    let num_bones = bone_names.len();
    let max_influences = 4;

    let (indices, weights_raw) = result.export_weights(max_influences);
    let rig_vertices: Vec<usize> = view_map.unwrap_or_else(|| (0..result.attachment.num_vertices()).collect());
    let num_vertices = rig_vertices.len();

    let mut weights = Vec::with_capacity(num_vertices * max_influences * 2);
    for &vert_idx in &rig_vertices {
        for i in 0..max_influences {
            weights.push(indices[vert_idx][i] as f32);
            weights.push(weights_raw[vert_idx][i] as f32);
        }
    }

    Ok(WeightsData {
        num_vertices,
        num_bones,
        bone_names,
        weights,
        max_influences,
    })
}

/// Reemplaza los pesos de algunos vértices (pincel de pesos). `vertices` son
/// índices del visor y `influences` trae, por vértice, pares `(hueso, peso)`
/// planos; los pesos se normalizan. Es lo que usa la exportación.
#[tauri::command]
pub async fn set_vertex_weights(app: AppHandle, vertices: Vec<u32>, influences: Vec<f32>) -> Result<(), String> {
    in_background(app, move |state| {
        let view_map = rig_view_map(state)?;
        let mut result_lock = state.result.lock().unwrap();
        let result = result_lock.as_mut().ok_or("No hay pesos calculados")?;
        if vertices.is_empty() {
            return Ok(());
        }
        if !influences.len().is_multiple_of(2 * vertices.len()) {
            return Err("Formato de pesos inválido".to_string());
        }
        let per_vertex = influences.len() / vertices.len();
        let num_bones = result.attachment.num_bones();
        let num_vertices = result.attachment.num_vertices();
        for (&v, row) in vertices.iter().zip(influences.chunks(per_vertex)) {
            let v = match &view_map {
                Some(map) => *map.get(v as usize).ok_or_else(|| format!("Vértice fuera de rango: {v}"))?,
                None => v as usize,
            };
            if v >= num_vertices {
                return Err(format!("Vértice fuera de rango: {v}"));
            }
            let mut dense = vec![0.0; num_bones];
            for pair in row.chunks(2) {
                let (bone, weight) = (pair[0] as usize, pair[1] as f64);
                if bone < num_bones {
                    dense[bone] += weight.max(0.0);
                }
            }
            result.attachment.set_weights(v, &dense);
        }
        Ok(())
    })
    .await
}

/// Simetría para pintar pesos en espejo: cabecera `[vértices, huesos]` (u32),
/// luego el vértice espejo de cada vértice y el hueso par de cada hueso
/// (`u32::MAX` = sin espejo), en el orden de vértices del visor. El plano es el
/// de simetría del esqueleto.
#[tauri::command]
pub async fn get_weight_mirror(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |state| {
        // Posiciones de los vértices del visor de la malla del rig
        let positions: Vec<Vector3> = if state.rig_on_quad.load(std::sync::atomic::Ordering::SeqCst) {
            let quad = state.quad_mesh.lock().unwrap();
            let skin = state.quad_skin.lock().unwrap();
            let (data, _) = quad_view(quad.as_ref().ok_or("No hay malla retopologizada")?, skin.as_ref());
            data.positions.chunks(3).map(|p| Vector3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect()
        } else {
            let mesh = state.mesh.lock().unwrap();
            mesh.as_ref().ok_or("No hay malla cargada")?.vertices.iter().map(|v| v.position).collect()
        };
        let skeleton = {
            let lock = state.skeleton.lock().unwrap();
            to_basic_skeleton(lock.as_ref().ok_or("No hay esqueleto")?)
        };
        let bones: Vec<u32> = pinocchio_skeleton::mirror_pairs(&skeleton)
            .into_iter()
            .map(|p| p.map_or(u32::MAX, |b| b as u32))
            .collect();
        let vertices = match pinocchio_skeleton::symmetry_plane(&skeleton) {
            Some(plane) => mirror_vertices(&positions, plane),
            None => vec![u32::MAX; positions.len()],
        };
        let mut out = Vec::with_capacity(4 * (2 + vertices.len() + bones.len()));
        for w in [vertices.len() as u32, bones.len() as u32].iter().chain(&vertices).chain(&bones) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        Ok(out)
    })
    .await?;
    Ok(Response::new(bytes))
}

/// Vértice más cercano al reflejo de cada vértice (si está a menos del 2 % de
/// la diagonal de la caja de los vértices; si no, `u32::MAX`).
fn mirror_vertices(positions: &[Vector3], plane: (Vector3, Vector3)) -> Vec<u32> {
    use rayon::prelude::*;
    let (lo, hi) = positions.iter().fold(
        (Vector3::new(f64::MAX, f64::MAX, f64::MAX), Vector3::new(f64::MIN, f64::MIN, f64::MIN)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    );
    let tolerance = if positions.is_empty() { 0.0 } else { lo.distance(&hi) * 0.02 };
    let cell = tolerance.max(1e-12);
    let key = |p: &Vector3| [p.x(), p.y(), p.z()].map(|c| (c / cell).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for (i, p) in positions.iter().enumerate() {
        grid.entry(key(p)).or_default().push(i as u32);
    }
    positions
        .par_iter()
        .map(|p| {
            let q = pinocchio_skeleton::reflect(*p, plane);
            let [kx, ky, kz] = key(&q);
            let mut best = (tolerance, u32::MAX);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        for &j in grid.get(&[kx + dx, ky + dy, kz + dz]).into_iter().flatten() {
                            let d = positions[j as usize].distance(&q);
                            if d < best.0 {
                                best = (d, j);
                            }
                        }
                    }
                }
            }
            best.1
        })
        .collect()
}

/// Obtiene los datos del esqueleto embebido
#[tauri::command]
pub fn get_skeleton_data(state: State<'_, AppState>) -> Result<SkeletonData, String> {
    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto seleccionado")?;

    let result_lock = state.result.lock().unwrap();

    if let Some(result) = result_lock.as_ref() {
        let base_data = get_skeleton_data_for_type(skeleton_type);

        let bones: Vec<BoneData> = base_data
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let pos = result
                    .bone_positions
                    .get(i)
                    .map(|p| [p.x(), p.y(), p.z()])
                    .unwrap_or(b.position);
                BoneData {
                    name: b.name.clone(),
                    position: pos,
                    parent: b.parent,
                    is_leaf: b.is_leaf,
                }
            })
            .collect();

        Ok(SkeletonData {
            bones,
            edges: base_data.edges,
            pivot: visible_pivot(&state.skeleton_transform.lock().unwrap()),
        })
    } else {
        let mut data = get_skeleton_data_for_type(skeleton_type);
        data.pivot = visible_pivot(&state.skeleton_transform.lock().unwrap());
        Ok(data)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RETOPOLOGY COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Ejecuta la retopología con QuadriFlow
///
/// Corre en un hilo bloqueante; el flag `processing` se libera siempre.
#[tauri::command]
pub async fn run_retopology(
    config: RetopologyConfig,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<QuadMeshInfo, String> {
    let _guard = state
        .try_begin_processing()
        .ok_or("Ya hay un proceso en curso")?;

    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let scene = state.scene.lock().unwrap().clone();

    let _ = on_progress.send(Progress {
        stage: "preparing".to_string(),
        percent: 0,
        message: "Preparando retopología...".to_string(),
    });

    let mut remesh_config = config.to_remesh_config()?;
    // Sin UV no hay piel que conservar: las costuras serían solo de normales
    // o materiales
    let has_uvs = scene.as_ref().is_some_and(|s| s.world_primitives().iter().any(|p| p.uvs.is_some()));
    remesh_config.preserve_seams &= has_uvs;

    let progress = on_progress.clone();
    let (quad_mesh, quality, skin) = tauri::async_runtime::spawn_blocking(move || {
        let quads = remesh_with_callback(&mesh, &remesh_config, |stage, message| {
            let _ = progress.send(Progress {
                stage: stage.name().to_string(),
                percent: stage.progress(),
                message: message.to_string(),
            });
        })?;
        let quality = quadriflow_core::quality::analyze(&quads, Some(&mesh));
        let skin = scene.as_ref().and_then(|scene| {
            let surface = uv_core::scene_surface(scene)?;
            let _ = progress.send(Progress {
                stage: "uv".to_string(),
                percent: 99,
                message: "Trasladando el mapa UV...".to_string(),
            });
            let (positions, faces) = quad_arrays(&quads);
            Some(uv_core::transferred_skin(scene, &surface, &positions, &faces))
        });
        Ok::<_, quadriflow_core::RemeshError>((quads, quality, skin))
    })
    .await
    .map_err(|e| format!("La retopología terminó inesperadamente: {e}"))?
    .map_err(|e| format!("Error en retopología: {e}"))?;

    // Las etapas siguientes pasan a usar la malla nueva. El rig la sigue: el
    // esqueleto ya estaba ajustado a la forma y los pesos se trasladan desde
    // la malla donde se calcularon (la original o la retopología anterior)
    let rig = state.result.lock().unwrap().take();
    let rig_source = if state.rig_on_quad.load(std::sync::atomic::Ordering::SeqCst) {
        state.quad_mesh.lock().unwrap().as_ref().map(quad_as_mesh)
    } else {
        state.mesh.lock().unwrap().clone()
    };
    let rig = match (rig, rig_source) {
        (Some(mut rig), Some(source)) => {
            let _ = on_progress.send(Progress {
                stage: "rig".to_string(),
                percent: 100,
                message: "Trasladando los pesos a la malla nueva...".to_string(),
            });
            let target = quad_as_mesh(&quad_mesh);
            tauri::async_runtime::spawn_blocking(move || move_rig(&mut rig, &source, &target).then_some(rig))
                .await
                .ok()
                .flatten()
        }
        _ => None,
    };

    let info = QuadMeshInfo {
        num_vertices: quad_mesh.num_vertices(),
        num_quads: quad_mesh.num_faces(),
        bounding_box: calculate_quad_mesh_bounds(&quad_mesh),
        quality: quality.into(),
        uv_seam_faces: skin.as_ref().and_then(|s| match s.info {
            uv_core::SkinInfo::Transferred { seam_faces } => Some(seam_faces),
            uv_core::SkinInfo::Unwrapped { .. } => None,
        }),
        rig_kept: rig.is_some(),
    };

    let _ = on_progress.send(Progress {
        stage: "done".to_string(),
        percent: 100,
        message: format!(
            "Retopología completada: {} vértices, {} quads",
            info.num_vertices, info.num_quads
        ),
    });

    *state.quad_mesh.lock().unwrap() = Some(quad_mesh);
    *state.quad_skin.lock().unwrap() = skin;
    state.use_retopology.store(true, std::sync::atomic::Ordering::SeqCst);
    state.active_mesh_changed();
    state.rig_on_quad.store(rig.is_some(), std::sync::atomic::Ordering::SeqCst);
    *state.result.lock().unwrap() = rig;
    Ok(info)
}

/// Obtiene los datos de la malla de quads para renderizar en Three.js
#[tauri::command]
pub async fn get_quad_mesh_data(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |state| {
        get_quad_mesh_data_impl(state).map(|d| {
            let mut out = pack_mesh(&d.positions, &d.normals, d.uvs.as_deref(), &d.indices, &d.quad_indices);
            append_groups(&mut out, &d.groups);
            out
        })
    })
    .await?;
    Ok(Response::new(bytes))
}

fn get_quad_mesh_data_impl(state: &AppState) -> Result<QuadMeshData, String> {
    let quad_lock = state.quad_mesh.lock().unwrap();
    let quad = quad_lock.as_ref().ok_or("No hay malla de quads")?;
    let skin_lock = state.quad_skin.lock().unwrap();
    Ok(quad_view(quad, skin_lock.as_ref()).0)
}

/// Datos del visor de la malla de quads y, por cada vértice mostrado, su
/// vértice de quads (con piel, un vértice se duplica en las costuras UV).
fn quad_view(quad: &quadriflow_core::QuadMesh, skin: Option<&uv_core::Skin<4>>) -> (QuadMeshData, Vec<usize>) {
    let skin = skin.filter(|s| s.corners.len() == quad.num_faces());
    let (positions, faces) = quad_arrays(quad);
    let no_uvs;
    let corners = match skin {
        Some(s) => &s.corners,
        None => {
            no_uvs = vec![[[0.0f32; 2]; 4]; faces.len()];
            &no_uvs
        }
    };
    // Mismas normales que al exportar; con piel, un vértice por (vértice, UV)
    let frames = uv_core::corner_frames(&positions, &faces, corners);
    let mut ids: HashMap<(usize, [u32; 2]), u32> = HashMap::new();
    let mut source = Vec::new();
    let mut data = QuadMeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: skin.map(|_| Vec::new()),
        indices: Vec::with_capacity(faces.len() * 6),
        quad_indices: Vec::with_capacity(faces.len() * 4),
        groups: Vec::new(),
    };
    // Con piel, las caras en orden de material para que cada uno sea un rango
    let material = |f: usize| skin.and_then(|s| s.face_material[f]).map_or(u32::MAX, |m| m as u32);
    let mut order: Vec<usize> = (0..faces.len()).collect();
    if skin.is_some() {
        order.sort_by_key(|&f| material(f));
    }
    for f in order {
        let face = &faces[f];
        let corner: [u32; 4] = std::array::from_fn(|k| {
            let uv = corners[f][k];
            *ids.entry((face[k], uv.map(f32::to_bits))).or_insert_with(|| {
                data.positions.extend(positions[face[k]].map(|c| c as f32));
                data.normals.extend(frames.normals[f][k]);
                if let Some(uvs) = &mut data.uvs {
                    uvs.extend(uv);
                }
                source.push(face[k]);
                (data.positions.len() / 3 - 1) as u32
            })
        });
        data.quad_indices.extend(corner);
        let [a, b, c, d] = corner;
        let start = data.indices.len() as u32;
        data.indices.extend([a, b, c, a, c, d]);
        if skin.is_some() {
            match data.groups.last_mut() {
                Some(group) if group[2] == material(f) => group[1] += 6,
                _ => data.groups.push([start, 6, material(f)]),
            }
        }
    }
    (data, source)
}

/// La malla de quads como malla de pinocchio (vértices en el mismo orden,
/// cada quad en dos triángulos)
fn quad_as_mesh(quad: &quadriflow_core::QuadMesh) -> Mesh {
    let positions: Vec<Vector3> = quad.vertices.iter().map(|v| Vector3::new(v.x, v.y, v.z)).collect();
    let triangles: Vec<[usize; 3]> =
        quad.faces.iter().flat_map(|f| [[f.v[0], f.v[1], f.v[2]], [f.v[0], f.v[2], f.v[3]]]).collect();
    Mesh::from_triangles(&positions, &triangles)
}

/// Malla activa para esqueleto y pesos: la de quads tras retopologizar (si
/// no se eligió volver a la original), si no la original.
fn active_mesh(state: &AppState) -> Result<Mesh, String> {
    if state.active_is_quad() {
        let quad = state.quad_mesh.lock().unwrap();
        return Ok(quad_as_mesh(quad.as_ref().ok_or("No hay malla retopologizada")?));
    }
    state.mesh.lock().unwrap().clone().ok_or_else(|| "No hay malla cargada".to_string())
}

/// Vértice del rig de cada vértice que muestra el visor (`None` = el mismo
/// índice: el rig está sobre la malla original, que el visor muestra tal cual).
fn rig_view_map(state: &AppState) -> Result<Option<Vec<usize>>, String> {
    if !state.rig_on_quad.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(None);
    }
    let quad = state.quad_mesh.lock().unwrap();
    let quad = quad.as_ref().ok_or("El rig era de la malla retopologizada y ya no existe")?;
    let skin = state.quad_skin.lock().unwrap();
    Ok(Some(quad_view(quad, skin.as_ref()).1))
}

/// Qué malla recibe los pesos al exportar
#[derive(Clone, Copy, PartialEq)]
enum WeightTarget {
    Original,
    Retopology,
}

/// Pesos del rig (por vértice de la malla donde se calcularon) llevados a la
/// malla que se exporta: se trasladan sólo si son mallas distintas.
fn weights_for(state: &AppState, source: Vec<Vec<f64>>, target: WeightTarget) -> Result<Vec<Vec<f64>>, String> {
    let on_quad = state.rig_on_quad.load(std::sync::atomic::Ordering::SeqCst);
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;
    let quad_lock = state.quad_mesh.lock().unwrap();
    let quad = || quad_lock.as_ref().ok_or("No hay malla retopologizada");
    let expected = |n: usize| {
        if source.len() == n {
            Ok(())
        } else {
            Err(format!(
                "El rig no corresponde a la malla actual ({} pesos, {n} vértices): vuelve a calcular los pesos",
                source.len()
            ))
        }
    };
    match (target, on_quad) {
        (WeightTarget::Retopology, true) => {
            expected(quad()?.num_vertices())?;
            Ok(source)
        }
        (WeightTarget::Original, false) => {
            expected(mesh.num_vertices())?;
            Ok(source)
        }
        (WeightTarget::Retopology, false) => weights_on_quad_mesh(mesh, &source, quad()?),
        (WeightTarget::Original, true) => {
            let quad = quad()?;
            expected(quad.num_vertices())?;
            let targets: Vec<Vector3> = mesh.vertices.iter().map(|v| v.position).collect();
            Ok(transfer_weights(&quad_as_mesh(quad), &source, &targets))
        }
    }
}

/// Malla activa elegida y si el rig la siguió
#[derive(Debug, Clone, Serialize)]
pub struct ActiveMeshInfo {
    /// Esqueleto y pesos usan la malla retopologizada
    pub retopology: bool,
    /// Había rig y sus pesos pasaron a la malla activa
    pub rig_kept: bool,
}

/// Elige si esqueleto y pesos usan la malla retopologizada (`true`) o la
/// original. El rig pasa a la malla elegida (pesos trasladados).
#[tauri::command]
pub async fn set_active_mesh(app: AppHandle, retopology: bool) -> Result<ActiveMeshInfo, String> {
    use std::sync::atomic::Ordering;
    in_background(app, move |state| {
        state.use_retopology.store(retopology, Ordering::SeqCst);
        *state.joint_centering.lock().unwrap() = None;
        let on_quad = state.active_is_quad();
        let was_on_quad = state.rig_on_quad.load(Ordering::SeqCst);
        let has_rig = state.result.lock().unwrap().is_some();
        if has_rig && was_on_quad != on_quad {
            let quad = state.quad_mesh.lock().unwrap().as_ref().map(quad_as_mesh);
            let original = state.mesh.lock().unwrap().clone();
            let (source, target) = if on_quad { (original, quad) } else { (quad, original) };
            let mut result = state.result.lock().unwrap();
            let moved = match (result.as_mut(), source, target) {
                (Some(rig), Some(source), Some(target)) => move_rig(rig, &source, &target),
                _ => false,
            };
            if moved {
                state.rig_on_quad.store(on_quad, Ordering::SeqCst);
            } else {
                *result = None;
            }
        }
        let rig_kept = state.result.lock().unwrap().is_some();
        Ok(ActiveMeshInfo { retopology: on_quad, rig_kept })
    })
    .await
}

/// Esqueleto y pesos usan la malla retopologizada
#[tauri::command]
pub fn get_active_mesh(state: State<'_, AppState>) -> bool {
    state.active_is_quad()
}

// ═══════════════════════════════════════════════════════════════════════════
// UV / PIEL
// ═══════════════════════════════════════════════════════════════════════════

/// Opciones del desplegado UV
#[derive(Debug, Clone, Deserialize)]
pub struct UvUnwrapConfig {
    /// Lado de las texturas horneadas (px)
    pub texture_size: u32,
    /// Margen entre islas (px)
    pub padding: u32,
    /// Máxima desviación de normal dentro de una isla (grados)
    pub max_angle: f64,
}

/// Estado de la piel de la malla retopologizada
#[derive(Debug, Clone, Serialize)]
pub struct UvInfo {
    /// "transferred" (UV del original) o "unwrapped" (desplegado nuevo)
    pub mode: String,
    /// Caras que cruzan costuras del mapa original (solo "transferred")
    pub seam_faces: Option<usize>,
    pub num_charts: Option<usize>,
    /// Estiramiento L2: 1 es isométrico
    pub stretch: Option<f64>,
    /// Fracción del atlas cubierta por islas
    pub coverage: Option<f64>,
    /// Lado de las texturas horneadas (0 si no hay)
    pub texture_size: u32,
    pub has_base_color: bool,
    pub has_normal: bool,
    /// El modelo original tiene UV: se puede volver a trasladarlas
    pub can_restore: bool,
}

fn uv_info(skin: &uv_core::Skin<4>, scene: Option<&Scene>) -> UvInfo {
    let can_restore = scene.is_some_and(|s| s.world_primitives().iter().any(|p| p.uvs.is_some()));
    let textured = |f: fn(&converter_scene::Material) -> bool| skin.materials.iter().any(f);
    let has_base_color = textured(|m| m.base_color_texture.is_some());
    let has_normal = textured(|m| m.normal_texture.is_some());
    match skin.info {
        uv_core::SkinInfo::Transferred { seam_faces } => UvInfo {
            mode: "transferred".into(),
            seam_faces: Some(seam_faces),
            num_charts: None,
            stretch: None,
            coverage: None,
            texture_size: 0,
            has_base_color,
            has_normal,
            can_restore,
        },
        uv_core::SkinInfo::Unwrapped { num_charts, stretch, coverage, texture_size } => UvInfo {
            mode: "unwrapped".into(),
            seam_faces: None,
            num_charts: Some(num_charts),
            stretch: Some(stretch),
            coverage: Some(coverage),
            texture_size,
            has_base_color,
            has_normal,
            can_restore,
        },
    }
}

/// Piel actual de la malla retopologizada (`None` si no tiene)
#[tauri::command]
pub fn get_uv_info(state: State<'_, AppState>) -> Option<UvInfo> {
    let skin = state.quad_skin.lock().unwrap();
    let scene = state.scene.lock().unwrap();
    skin.as_ref().map(|s| uv_info(s, scene.as_ref()))
}

/// Despliega la malla retopologizada y hornea sobre el mapa nuevo las
/// texturas del original (color, metal/rugosidad, oclusión, emisión y la
/// normal con el detalle de la malla original)
#[tauri::command]
pub async fn run_uv_unwrap(
    config: UvUnwrapConfig,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<UvInfo, String> {
    let _guard = state.try_begin_processing().ok_or("Ya hay un proceso en curso")?;
    let quad = state.quad_mesh.lock().unwrap().clone().ok_or("Primero ejecuta la retopología")?;
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    if !(64..=8192).contains(&config.texture_size) {
        return Err(format!("Tamaño de textura fuera de rango: {}", config.texture_size));
    }

    let _ = on_progress.send(Progress {
        stage: "unwrap".to_string(),
        percent: 10,
        message: "Cortando islas, desplegando y horneando texturas...".to_string(),
    });
    let skin = tauri::async_runtime::spawn_blocking(move || {
        let (positions, faces) = quad_arrays(&quad);
        let surface = uv_core::scene_surface(&scene);
        let options = uv_core::BakeOptions {
            texture_size: config.texture_size,
            unwrap: uv_core::UnwrapOptions {
                charts: uv_core::ChartOptions {
                    max_angle: config.max_angle.clamp(15.0, 85.0),
                    ..Default::default()
                },
                padding: config.padding.clamp(1, 64),
                ..Default::default()
            },
        };
        uv_core::unwrapped_skin(&scene, surface.as_ref(), &positions, &faces, &options)
    })
    .await
    .map_err(|e| format!("El desplegado terminó inesperadamente: {e}"))?;

    let info = uv_info(&skin, state.scene.lock().unwrap().as_ref());
    *state.quad_skin.lock().unwrap() = Some(skin);
    Ok(info)
}

/// Vuelve a las UV trasladadas del modelo original
#[tauri::command]
pub async fn restore_transferred_uvs(app: AppHandle) -> Result<UvInfo, String> {
    in_background(app, |state| {
        let quad = state.quad_mesh.lock().unwrap().clone().ok_or("Primero ejecuta la retopología")?;
        let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
        let surface = uv_core::scene_surface(&scene).ok_or("El modelo original no tiene UV")?;
        let (positions, faces) = quad_arrays(&quad);
        let skin = uv_core::transferred_skin(&scene, &surface, &positions, &faces);
        let info = uv_info(&skin, Some(&scene));
        *state.quad_skin.lock().unwrap() = Some(skin);
        Ok(info)
    })
    .await
}

/// Tablero de ajedrez (PNG) para ver la distorsión de las UV
#[tauri::command]
pub async fn get_checker_texture(app: AppHandle) -> Result<Response, String> {
    let bytes = in_background(app, |_| Ok(uv_core::checker_texture(1024, 16).data)).await?;
    Ok(Response::new(bytes))
}

/// Materiales de la piel de la malla retopologizada (el índice es el de los
/// grupos de `get_quad_mesh_data`)
#[tauri::command]
pub fn get_skin_materials(state: State<'_, AppState>) -> Vec<crate::structure::MaterialInfo> {
    let skin = state.quad_skin.lock().unwrap();
    skin.as_ref().map_or_else(Vec::new, |s| s.materials.iter().map(crate::structure::material_info).collect())
}

/// Imagen de una textura de la piel, tal cual (PNG, JPEG o WebP)
#[tauri::command]
pub async fn get_skin_texture(app: AppHandle, index: usize) -> Result<Response, String> {
    let bytes = in_background(app, move |state| {
        let skin = state.quad_skin.lock().unwrap();
        let skin = skin.as_ref().ok_or("La malla no tiene piel")?;
        skin.textures.get(index).map(|t| t.data.clone()).ok_or_else(|| format!("No existe la textura {index}"))
    })
    .await?;
    Ok(Response::new(bytes))
}

/// Posiciones y caras de la malla de quads en el formato de `uv-core`.
fn quad_arrays(quad: &quadriflow_core::QuadMesh) -> (Vec<[f64; 3]>, Vec<[usize; 4]>) {
    (quad.vertices.iter().map(|v| [v.x, v.y, v.z]).collect(), quad.faces.iter().map(|f| f.v).collect())
}

// ═══════════════════════════════════════════════════════════════════════════
// REPAIR TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct RepairAnalysisConfigInput {
    pub check_self_intersections: Option<bool>,
}

/// Diagnósticos de la librería más los veredictos derivados
#[derive(Debug, Clone, Serialize)]
pub struct MeshDiagnosticsInfo {
    #[serde(flatten)]
    pub diagnostics: pinocchio_repair::MeshDiagnostics,
    pub needs_repair: bool,
    pub is_healthy: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepairConfigInput {
    pub merge_duplicates: Option<bool>,
    pub remove_degenerates: Option<bool>,
    pub fix_normals: Option<bool>,
    pub fix_non_manifold: Option<bool>,
    pub orient_outward: Option<bool>,
    pub remove_small_components: Option<bool>,
    pub fill_holes: Option<bool>,
    /// Máximo de aristas de un agujero a rellenar (0 = sin límite)
    pub max_hole_edges: Option<usize>,
    /// Refinar y suavizar los parches de relleno
    pub refine_fill: Option<bool>,
}

impl RepairConfigInput {
    fn to_repair_config(&self) -> RepairConfig {
        let defaults = RepairConfig::default();
        let refine = self.refine_fill.unwrap_or(true);
        RepairConfig {
            merge_duplicates: self.merge_duplicates.unwrap_or(defaults.merge_duplicates),
            remove_degenerates: self.remove_degenerates.unwrap_or(defaults.remove_degenerates),
            remove_duplicate_faces: self.remove_degenerates.unwrap_or(defaults.remove_duplicate_faces),
            fix_normals: self.fix_normals.unwrap_or(defaults.fix_normals),
            fix_non_manifold: self.fix_non_manifold.unwrap_or(defaults.fix_non_manifold),
            orient_outward: self.orient_outward.unwrap_or(defaults.orient_outward),
            remove_small_components: self.remove_small_components.unwrap_or(false),
            fill_holes: self.fill_holes.unwrap_or(true),
            hole_fill_config: HoleFillConfig {
                max_hole_edges: self.max_hole_edges.unwrap_or(0),
                refine,
                fair: refine,
            },
            ..defaults
        }
    }
}

/// Resumen de la librería, la nueva malla y su diagnóstico
#[derive(Debug, Clone, Serialize)]
pub struct RepairResultInfo {
    #[serde(flatten)]
    pub summary: pinocchio_repair::RepairSummary,
    pub new_mesh_info: MeshInfo,
    pub new_diagnostics: MeshDiagnosticsInfo,
    /// Había rig y pasó a la malla reparada (mismo esqueleto, pesos trasladados)
    pub rig_kept: bool,
}

/// Malla restaurada al deshacer la reparación
#[derive(Debug, Clone, Serialize)]
pub struct UndoRepairInfo {
    #[serde(flatten)]
    pub mesh_info: MeshInfo,
    /// Había rig y pasó a la malla restaurada
    pub rig_kept: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
// PRINT3D TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct Print3dAnalysisInfo {
    pub volume: f64,
    pub surface_area: f64,
    pub center_of_mass: [f64; 3],
    pub dimensions: [f64; 3],
    pub is_closed: bool,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub estimated_weight: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScalePrintInput {
    pub mode: String,
    pub factor: Option<f64>,
    pub target_size: Option<[f64; 3]>,
    pub target_volume: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubdivideConfigInput {
    pub build_volume: [f64; 3],
    pub strategy: Option<String>,
    pub margin: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubdivideResultInfo {
    pub piece_count: usize,
    pub pieces: Vec<PieceInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PieceInfo {
    pub index: usize,
    pub label: String,
    pub vertex_count: usize,
    pub face_count: usize,
    pub dimensions: [f64; 3],
}

// ═══════════════════════════════════════════════════════════════════════════
// REPAIR COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Analiza la malla buscando problemas
#[tauri::command]
pub async fn analyze_mesh(
    config: RepairAnalysisConfigInput,
    state: State<'_, AppState>,
) -> Result<MeshDiagnosticsInfo, String> {
    let mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let analysis_config = RepairAnalysisConfig {
        check_self_intersections: config.check_self_intersections.unwrap_or(false),
        ..RepairAnalysisConfig::default()
    };

    let diagnostics = tauri::async_runtime::spawn_blocking(move || analyze_scene_mesh(&mesh, &scene, &analysis_config))
        .await
        .map_err(|e| format!("El análisis terminó inesperadamente: {e}"))?;

    let info = diagnostics_to_info(&diagnostics);
    *state.diagnostics.lock().unwrap() = Some(diagnostics);
    Ok(info)
}

/// Repara la malla con la configuración dada
///
/// Corre en un hilo bloqueante. Guarda la malla y la escena previas para
/// poder deshacer.
#[tauri::command]
pub async fn repair_mesh(
    app: AppHandle,
    config: RepairConfigInput,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<RepairResultInfo, String> {
    let _guard = state
        .try_begin_processing()
        .ok_or("Ya hay un proceso en curso")?;

    let original_mesh = state.mesh.lock().unwrap().clone().ok_or("No hay malla cargada")?;
    let original_scene = state.scene.lock().unwrap().clone().ok_or("No hay escena cargada")?;
    let repair_config = config.to_repair_config();

    let mesh = original_mesh.clone();
    let progress = on_progress.clone();
    let (mesh, summary, diagnostics) = tauri::async_runtime::spawn_blocking(move || {
        let mut mesh = mesh;
        let mut last = u32::MAX;
        let summary = pinocchio_repair::repair_all_with_progress(&mut mesh, &repair_config, |fraction, stage| {
            // Solo enviar cuando cambia el porcentaje: el canal no es gratis
            let percent = (fraction * 90.0) as u32;
            if percent != last {
                last = percent;
                report(&progress, "repair", percent, format!("{stage}..."));
            }
        })?;
        report(&progress, "analyze", 92, "Analizando resultado...");
        let diagnostics = pinocchio_repair::analyze(&mesh, &RepairAnalysisConfig::default());
        Ok::<_, pinocchio_repair::RepairError>((mesh, summary, diagnostics))
    })
    .await
    .map_err(|e| format!("La reparación terminó inesperadamente: {e}"))?
    .map_err(|e| format!("Error en reparación: {e}"))?;

    // Reconstruir Scene desde la malla reparada (la topología cambió), con la
    // piel del original. La malla queda en el orden de vértices de la escena
    report(&on_progress, "skin", 96, "Trasladando la piel...");
    let (new_scene, mesh) = repaired_scene(&mesh, &original_scene);
    let (num_vertices, num_faces, has_normals, has_uvs) = calculate_scene_stats(&new_scene);
    let new_mesh_info = MeshInfo {
        num_vertices,
        num_faces,
        num_meshes: new_scene.meshes.len(),
        has_normals,
        has_uvs,
        has_materials: !new_scene.materials.is_empty(),
        bounding_box: calculate_scene_bounds(&new_scene),
        format: "REPAIRED".to_string(),
        rig: None,
    };

    report(&on_progress, "rig", 98, "Trasladando los pesos...");
    *state.mesh.lock().unwrap() = Some(mesh);
    *state.scene.lock().unwrap() = Some(new_scene);
    // La topología cambió: la retopología se descarta y el rig pasa a la malla reparada
    let (rig_kept, original_mesh) = tauri::async_runtime::spawn_blocking(move || {
        let kept = mesh_replaced(&app.state::<AppState>(), &original_mesh);
        (kept, original_mesh)
    })
    .await
    .map_err(|e| format!("El traslado de los pesos terminó inesperadamente: {e}"))?;
    *state.mesh_before_repair.lock().unwrap() = Some(original_mesh);
    *state.scene_before_repair.lock().unwrap() = Some(original_scene);

    let new_diagnostics = diagnostics_to_info(&diagnostics);
    *state.diagnostics.lock().unwrap() = Some(diagnostics);

    Ok(RepairResultInfo { summary, new_mesh_info, new_diagnostics, rig_kept })
}

/// Deshace la reparación restaurando backups
#[tauri::command]
pub async fn undo_repair(app: AppHandle) -> Result<UndoRepairInfo, String> {
    in_background(app, undo_repair_impl).await
}

fn undo_repair_impl(state: &AppState) -> Result<UndoRepairInfo, String> {
    let backup_mesh = {
        let mut backup = state.mesh_before_repair.lock().unwrap();
        backup.take().ok_or("No hay reparación que deshacer")?
    };
    let backup_scene = {
        let mut backup = state.scene_before_repair.lock().unwrap();
        backup.take().ok_or("No hay escena de backup")?
    };

    let (num_vertices, num_faces, has_normals, has_uvs) = calculate_scene_stats(&backup_scene);
    let bbox = calculate_scene_bounds(&backup_scene);
    let info = MeshInfo {
        num_vertices,
        num_faces,
        num_meshes: backup_scene.meshes.len(),
        has_normals,
        has_uvs,
        has_materials: !backup_scene.materials.is_empty(),
        bounding_box: bbox,
        format: "RESTORED".to_string(),
        rig: None,
    };

    let repaired = state.mesh.lock().unwrap().replace(backup_mesh);
    *state.scene.lock().unwrap() = Some(backup_scene);
    *state.diagnostics.lock().unwrap() = None;
    let rig_kept = match repaired {
        Some(repaired) => mesh_replaced(state, &repaired),
        None => {
            state.geometry_changed();
            false
        }
    };

    Ok(UndoRepairInfo { mesh_info: info, rig_kept })
}

/// La malla original se reemplazó por otra de la misma forma pero distinta
/// topología (reparar o deshacerlo; `state.mesh` ya es la nueva): la
/// retopología se descarta y el rig pasa a la malla nueva desde la malla donde
/// se calculó (`previous` o los quads). Devuelve si el rig se conservó.
fn mesh_replaced(state: &AppState, previous: &Mesh) -> bool {
    use std::sync::atomic::Ordering;
    let rig = state.result.lock().unwrap().take();
    let source = if state.rig_on_quad.load(Ordering::SeqCst) {
        state.quad_mesh.lock().unwrap().as_ref().map(quad_as_mesh)
    } else {
        Some(previous.clone())
    };
    state.geometry_changed();
    state.rig_on_quad.store(false, Ordering::SeqCst);
    let target = state.mesh.lock().unwrap().clone();
    let rig = match (rig, source, target) {
        (Some(mut rig), Some(source), Some(target)) => move_rig(&mut rig, &source, &target).then_some(rig),
        _ => None,
    };
    let kept = rig.is_some();
    *state.result.lock().unwrap() = rig;
    kept
}

/// Obtiene los diagnósticos guardados
#[tauri::command]
pub fn get_repair_diagnostics(state: State<'_, AppState>) -> Result<MeshDiagnosticsInfo, String> {
    let diag_lock = state.diagnostics.lock().unwrap();
    let diagnostics = diag_lock.as_ref().ok_or("No hay diagnósticos disponibles")?;
    Ok(diagnostics_to_info(diagnostics))
}

// ═══════════════════════════════════════════════════════════════════════════
// PRINT3D COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Analiza la malla para impresión 3D
#[tauri::command]
pub async fn analyze_print3d(app: AppHandle) -> Result<Print3dAnalysisInfo, String> {
    in_background(app, analyze_print3d_impl).await
}

fn analyze_print3d_impl(state: &AppState) -> Result<Print3dAnalysisInfo, String> {
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;

    let analysis = pinocchio_print3d::analyze(mesh)
        .map_err(|e| format!("Error en análisis: {:?}", e))?;

    Ok(analysis_to_info(&analysis, mm_per_unit(state)))
}

/// Escala la malla para impresión
#[tauri::command]
pub async fn scale_mesh_for_print(app: AppHandle, params: ScalePrintInput) -> Result<Print3dAnalysisInfo, String> {
    in_background(app, move |state| scale_mesh_for_print_impl(params, state)).await
}

fn scale_mesh_for_print_impl(params: ScalePrintInput, state: &AppState) -> Result<Print3dAnalysisInfo, String> {
    // Guardar backup (malla y escena) para poder deshacer
    {
        let mesh_lock = state.mesh.lock().unwrap();
        let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?.clone();
        *state.mesh_before_print_scale.lock().unwrap() = Some(mesh);
        *state.scene_before_print_scale.lock().unwrap() = state.scene.lock().unwrap().clone();
    }

    let mut mesh = {
        let mesh_lock = state.mesh.lock().unwrap();
        mesh_lock.as_ref().unwrap().clone()
    };
    let before = mesh.bounding_box();

    match params.mode.as_str() {
        "uniform" => {
            let factor = params.factor.ok_or("Falta factor de escala")?;
            if !(factor.is_finite() && factor > 0.0) {
                return Err(format!("Factor de escala inválido: {factor}"));
            }
            pinocchio_print3d::scale(&mut mesh, factor);
        }
        "fit" => {
            // La UI da ancho, fondo y alto (Z arriba); la malla es Y arriba
            let [w, d, h] = params.target_size.ok_or("Falta tamaño objetivo")?;
            let k = mm_per_unit(state);
            let target = [w, h, d].map(|v| v / k);
            pinocchio_print3d::scale_to_fit(&mut mesh, target).map_err(|e| format!("No se pudo escalar: {e}"))?;
        }
        "volume" => {
            let k = mm_per_unit(state);
            let target_vol = params.target_volume.ok_or("Falta volumen objetivo")? / (k * k * k);
            let current_vol = pinocchio_print3d::compute_volume(&mesh);
            pinocchio_print3d::scale_to_volume(&mut mesh, current_vol, target_vol)
                .map_err(|e| format!("No se pudo escalar por volumen: {e}"))?;
        }
        _ => return Err(format!("Modo de escala no soportado: {}", params.mode)),
    }

    // Factor realmente aplicado (scale_to_fit solo reduce), respecto del centro
    // del bounding box como hace pinocchio_print3d::scale
    let before_len = before.longest_axis_length();
    let factor = if before_len > 0.0 { mesh.bounding_box().longest_axis_length() / before_len } else { 1.0 };

    let new_scene = {
        let scene_lock = state.scene.lock().unwrap();
        let scene = scene_lock.as_ref().ok_or("No hay escena cargada")?;
        if scene.skeletons.is_empty() {
            scale_scene_about(scene, factor, before.center())
        } else {
            // Los joints no cuelgan de scene.nodes: se exporta geometría estática
            mesh_to_scene(&mesh, "print", Some(scene))
        }
    };
    {
        let mut mesh_lock = state.mesh.lock().unwrap();
        *mesh_lock = Some(mesh.clone());
    }
    {
        let mut scene_lock = state.scene.lock().unwrap();
        *scene_lock = Some(new_scene);
    }
    // Las posiciones cambiaron: el rig y la retopología ya no corresponden
    state.geometry_changed();

    // Re-analizar
    let analysis = pinocchio_print3d::analyze(&mesh)
        .map_err(|e| format!("Error en análisis: {:?}", e))?;
    Ok(analysis_to_info(&analysis, mm_per_unit(state)))
}

/// Deshace el último escalado para impresión
#[tauri::command]
pub async fn undo_print_scale(app: AppHandle) -> Result<Print3dAnalysisInfo, String> {
    in_background(app, undo_print_scale_impl).await
}

fn undo_print_scale_impl(state: &AppState) -> Result<Print3dAnalysisInfo, String> {
    let mesh = state
        .mesh_before_print_scale
        .lock()
        .unwrap()
        .take()
        .ok_or("No hay escalado que deshacer")?;
    let scene = state
        .scene_before_print_scale
        .lock()
        .unwrap()
        .take()
        .ok_or("No hay escena de backup")?;

    let analysis = pinocchio_print3d::analyze(&mesh).map_err(|e| format!("Error en análisis: {:?}", e))?;
    *state.mesh.lock().unwrap() = Some(mesh);
    *state.scene.lock().unwrap() = Some(scene);
    state.geometry_changed();
    *state.print3d_pieces.lock().unwrap() = None;

    Ok(analysis_to_info(&analysis, mm_per_unit(state)))
}

/// Medidas de la malla (Y arriba) en el orden de la impresión: ancho, fondo, alto
fn print_size([x, y, z]: [f64; 3]) -> [f64; 3] {
    [x, z, y]
}

/// La malla girada 90° en X: de Y arriba a Z arriba (`to_z_up`) o al revés.
/// Es un giro, no un espejo: las caras siguen hacia afuera
fn mesh_reoriented(mesh: &Mesh, to_z_up: bool) -> Mesh {
    let turn = |v: &Vector3| {
        let (x, y, z) = (v.x(), v.y(), v.z());
        if to_z_up { Vector3::new(x, -z, y) } else { Vector3::new(x, z, -y) }
    };
    let mut out = mesh.clone();
    for v in &mut out.vertices {
        v.position = turn(&v.position);
        v.normal = turn(&v.normal);
    }
    out
}

/// Milímetros por unidad de la escena (la UI de impresión trabaja en mm)
fn mm_per_unit(state: &AppState) -> f64 {
    state
        .scene
        .lock()
        .unwrap()
        .as_ref()
        .map_or(1.0, |s| s.meters_per_unit * 1000.0)
}

/// Análisis de impresión convertido a milímetros (`k` = mm por unidad)
fn analysis_to_info(analysis: &pinocchio_print3d::MeshAnalysis, k: f64) -> Print3dAnalysisInfo {
    Print3dAnalysisInfo {
        volume: analysis.volume * k * k * k,
        surface_area: analysis.surface_area * k * k,
        center_of_mass: analysis.center_of_mass.map(|c| c * k),
        dimensions: print_size(analysis.bounding_box.dimensions()).map(|d| d * k),
        is_closed: analysis.is_closed,
        vertex_count: analysis.vertex_count,
        triangle_count: analysis.triangle_count,
        estimated_weight: analysis.estimated_weight.map(|w| w * k * k * k),
    }
}

/// Subdivide la malla en piezas para impresión
#[tauri::command]
pub async fn subdivide_mesh(app: AppHandle, config: SubdivideConfigInput) -> Result<SubdivideResultInfo, String> {
    in_background(app, move |state| subdivide_mesh_impl(config, state)).await
}

fn subdivide_mesh_impl(config: SubdivideConfigInput, state: &AppState) -> Result<SubdivideResultInfo, String> {
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;

    let strategy = match config.strategy.as_deref() {
        Some("optimal") => SubdivideStrategy::Optimal,
        Some("zlayers") => SubdivideStrategy::ZLayers,
        _ => SubdivideStrategy::Grid,
    };

    // La UI trabaja en milímetros
    let k = mm_per_unit(state);
    let subdivide_config = SubdivideConfig {
        build_volume: config.build_volume.map(|v| v / k),
        max_dimension: None,
        overlap: 0.0,
        strategy,
        margin: config.margin.unwrap_or(2.0) / k,
    };

    // El volumen de impresión y las capas son con Z arriba (la plataforma)
    let mut pieces = pinocchio_print3d::subdivide(&mesh_reoriented(mesh, true), &subdivide_config)
        .map_err(|e| format!("Error subdividiendo: {:?}", e))?;

    // Calcular vecinos
    pinocchio_print3d::find_neighbors(&mut pieces, 1.0 / k);

    let piece_infos: Vec<PieceInfo> = pieces.iter().enumerate().map(|(i, p)| {
        let bbox = pinocchio_print3d::compute_bounding_box(&p.mesh);
        PieceInfo {
            index: i,
            label: p.label.clone(),
            vertex_count: p.mesh.num_vertices(),
            face_count: p.mesh.num_faces(),
            dimensions: bbox.dimensions().map(|d| d * k),
        }
    }).collect();

    let piece_count = pieces.len();
    // De vuelta a Y arriba, como el resto de la escena (al exportar vuelven a Z)
    for piece in &mut pieces {
        piece.mesh = mesh_reoriented(&piece.mesh, false);
    }

    let mut pieces_lock = state.print3d_pieces.lock().unwrap();
    *pieces_lock = Some(pieces);

    Ok(SubdivideResultInfo {
        piece_count,
        pieces: piece_infos,
    })
}

/// Exporta una pieza individual como STL
#[tauri::command]
pub async fn export_print3d_piece(app: AppHandle, piece_index: usize, path: String) -> Result<ExportResult, String> {
    in_background(app, move |state| export_print3d_piece_impl(piece_index, path, state)).await
}

fn export_print3d_piece_impl(piece_index: usize, path: String, state: &AppState) -> Result<ExportResult, String> {
    let pieces_lock = state.print3d_pieces.lock().unwrap();
    let pieces = pieces_lock.as_ref().ok_or("No hay piezas subdivididas")?;

    if piece_index >= pieces.len() {
        return Err(format!("Índice de pieza fuera de rango: {} (hay {})", piece_index, pieces.len()));
    }

    let piece = &pieces[piece_index];
    let scene = {
        let base = state.scene.lock().unwrap();
        mesh_to_scene(&piece.mesh, &format!("pieza_{}", piece.label), base.as_ref())
    };

    let path_ref = Path::new(&path);
    converter_stl::export_stl(&scene, path_ref)
        .map_err(|e| format!("Error exportando STL: {:?}", e))?;

    Ok(ExportResult {
        success: true,
        path: path.clone(),
        message: format!("Pieza '{}' exportada como STL", piece.label),
        total_bytes: total_size(std::slice::from_ref(&path)),
        files_created: vec![path],
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

fn get_skeleton_data_for_type(skeleton_type: &SkeletonType) -> SkeletonData {
    match skeleton_type {
        SkeletonType::Human => skeleton_to_data(&HumanSkeleton::new()),
        SkeletonType::Quad => skeleton_to_data(&QuadSkeleton::new()),
        SkeletonType::Horse => skeleton_to_data(&HorseSkeleton::new()),
        SkeletonType::Centaur => skeleton_to_data(&CentaurSkeleton::new()),
        SkeletonType::Bird => skeleton_to_data(&BirdSkeleton::new()),
        SkeletonType::Spider => skeleton_to_data(&SpiderSkeleton::new()),
        SkeletonType::Serpent => skeleton_to_data(&SerpentSkeleton::default()),
        SkeletonType::Mech => skeleton_to_data(&MechSkeleton::new()),
        SkeletonType::Custom(skel) | SkeletonType::Template(skel) => skeleton_to_data(skel),
    }
}

fn skeleton_to_data<S: Skeleton>(skeleton: &S) -> SkeletonData {
    let bones: Vec<BoneData> = skeleton
        .bones()
        .iter()
        .map(|b| BoneData {
            name: b.name.clone(),
            position: [b.position.x(), b.position.y(), b.position.z()],
            parent: b.parent,
            is_leaf: b.is_leaf,
        })
        .collect();

    let edges = skeleton.get_graph_edges();
    let edges: Vec<[usize; 2]> = edges.into_iter().map(|(a, b)| [a, b]).collect();

    SkeletonData { bones, edges, pivot: None }
}

fn get_bone_names(skeleton_type: &SkeletonType) -> Vec<String> {
    match skeleton_type {
        SkeletonType::Human => HumanSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Quad => QuadSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Horse => HorseSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Centaur => CentaurSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Bird => BirdSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Spider => SpiderSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Serpent => SerpentSkeleton::default().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Mech => MechSkeleton::new().bones().iter().map(|b| b.name.clone()).collect(),
        SkeletonType::Custom(skel) | SkeletonType::Template(skel) => skel.bones().iter().map(|b| b.name.clone()).collect(),
    }
}

fn calculate_scene_stats(scene: &Scene) -> (usize, usize, bool, bool) {
    let prims = scene.world_primitives();
    let vertices = prims.iter().map(|p| p.positions.len()).sum();
    let faces = prims.iter().map(|p| p.triangles.len()).sum();
    let has_normals = prims.iter().any(|p| p.normals.is_some());
    let has_uvs = prims.iter().any(|p| p.uvs.is_some());
    (vertices, faces, has_normals, has_uvs)
}

fn calculate_scene_bounds(scene: &Scene) -> BoundingBox {
    match scene.compute_bounding_box() {
        Some((min, max)) => BoundingBox { min, max },
        None => BoundingBox { min: [0.0; 3], max: [1.0; 3] },
    }
}

pub(crate) fn scene_to_pinocchio_mesh(scene: &Scene) -> Result<Mesh, String> {
    pinocchio_mesh::scene_to_mesh(scene).ok_or_else(|| "Escena vacía o sin geometría".to_string())
}

/// Analiza la malla sin contar como duplicados los vértices que la escena
/// parte a propósito: misma posición con UV, normal o material distintos
/// (costuras de la piel, aristas duras y bordes entre materiales de glTF/OBJ). Reparar los soldaría y al rearmar la
/// escena con su piel volverían a partirse.
fn analyze_scene_mesh(
    mesh: &Mesh,
    scene: &Scene,
    config: &RepairAnalysisConfig,
) -> pinocchio_repair::MeshDiagnostics {
    let mut diagnostics = pinocchio_repair::analyze(mesh, config);
    diagnostics.duplicate_vertices = diagnostics.duplicate_vertices.saturating_sub(split_vertices(scene));
    diagnostics
}

/// Vértices de más por costuras de UV, normales o materiales
fn split_vertices(scene: &Scene) -> usize {
    use std::collections::HashSet;
    let mut positions = HashSet::new();
    let mut corners = HashSet::new();
    for (prim, p) in scene.world_primitives().iter().enumerate() {
        for &v in p.triangles.iter().flatten() {
            let v = v as usize;
            let Some(position) = p.positions.get(v) else { continue };
            let position = position.map(f32::to_bits);
            let uv = p.uvs.as_ref().and_then(|u| u.get(v)).map_or([0; 2], |u| u.map(f32::to_bits));
            let normal = p.normals.as_ref().and_then(|n| n.get(v)).map_or([0; 3], |n| n.map(f32::to_bits));
            positions.insert(position);
            corners.insert((position, prim, uv, normal));
        }
    }
    corners.len() - positions.len()
}

fn diagnostics_to_info(d: &pinocchio_repair::MeshDiagnostics) -> MeshDiagnosticsInfo {
    MeshDiagnosticsInfo { diagnostics: d.clone(), needs_repair: d.needs_repair(), is_healthy: d.is_healthy() }
}

/// Escena de la malla reparada con la piel del modelo original: UV,
/// materiales y texturas llevados cara a cara. Las caras que la reparación no
/// tocó recuperan su UV exacta; los parches de agujeros la toman del entorno.
///
/// Devuelve también la malla de pinocchio armada desde esa escena: las
/// costuras UV quedan sin soldar, como al importar, y el visor y el rig
/// comparten el orden de vértices.
fn repaired_scene(mesh: &Mesh, original: &Scene) -> (Scene, Mesh) {
    let has_uvs = original.world_primitives().iter().any(|p| p.uvs.is_some());
    let surface = if has_uvs {
        uv_core::scene_surface(original)
    } else if !original.materials.is_empty() {
        uv_core::material_surface(original)
    } else {
        None
    };
    let Some(surface) = surface else {
        return (mesh_to_scene(mesh, "repaired", Some(original)), mesh.clone());
    };

    let positions: Vec<[f64; 3]> =
        mesh.vertices.iter().map(|v| [v.position.x(), v.position.y(), v.position.z()]).collect();
    let faces: Vec<[usize; 3]> = (0..mesh.num_faces()).map(|i| mesh.get_face_vertices(i)).collect();
    let skin = uv_core::transferred_skin(original, &surface, &positions, &faces);
    let (mut scene, _) = uv_core::skin_scene(&positions, &faces, Some(&skin), original);
    for m in &mut scene.meshes {
        m.name = "repaired".into();
        if !has_uvs {
            // Solo se trasladaron materiales: sin UV inventadas
            for p in &mut m.primitives {
                p.attributes.retain(|a| !matches!(a, VertexAttribute::TexCoords(..) | VertexAttribute::Tangents(_)));
            }
        }
    }
    for n in &mut scene.nodes {
        n.name = "repaired".into();
    }
    match scene_to_pinocchio_mesh(&scene) {
        Ok(rebuilt) => (scene, rebuilt),
        Err(_) => (mesh_to_scene(mesh, "repaired", Some(original)), mesh.clone()),
    }
}

/// Reconstruye una Scene (converter-scene) a partir de una Mesh de pinocchio.
///
/// La malla de pinocchio está en espacio mundo, así que la escena tiene un solo
/// nodo raíz con identidad. Conserva las unidades y el eje "arriba" de `base`.
/// Se pierden materiales, UVs y skins: la topología pudo cambiar.
pub(crate) fn mesh_to_scene(mesh: &Mesh, name: &str, base: Option<&Scene>) -> Scene {
    use converter_scene::{Mesh as SceneMesh, Node, Primitive, Transform};

    let positions: Vec<[f32; 3]> = mesh
        .vertices
        .iter()
        .map(|v| [v.position.x() as f32, v.position.y() as f32, v.position.z() as f32])
        .collect();

    let triangles: Vec<[u32; 3]> = (0..mesh.num_faces())
        .map(|i| mesh.get_face_vertices(i).map(|v| v as u32))
        .collect();
    let normals = compute_vertex_normals(&positions, &triangles);

    let primitive = Primitive {
        attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::Normals(normals)],
        indices: Some(IndexData::U32(triangles.into_iter().flatten().collect())),
        material: None,
    };

    let mut scene = Scene {
        meshes: vec![SceneMesh { name: name.to_string(), primitives: vec![primitive] }],
        nodes: vec![Node {
            name: name.to_string(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        }],
        root_nodes: vec![0],
        ..Scene::default()
    };
    if let Some(base) = base {
        scene.meters_per_unit = base.meters_per_unit;
        scene.y_up = base.y_up;
    }
    scene
}

/// Escala la escena uniformemente respecto de `center` (espacio mundo)
/// envolviendo sus raíces en un nodo nuevo. Conserva materiales, UVs y texturas.
fn scale_scene_about(scene: &Scene, factor: f64, center: Vector3) -> Scene {
    use converter_scene::glam::{Mat4, Vec3};

    let c = Vec3::new(center.x() as f32, center.y() as f32, center.z() as f32);
    let matrix = Mat4::from_translation(c) * Mat4::from_scale(Vec3::splat(factor as f32)) * Mat4::from_translation(-c);
    wrap_scene(scene, matrix, "print_scale")
}

/// Aplica `matrix` en espacio mundo a toda la escena colgando sus raíces de
/// un nodo nuevo `name`; si la raíz ya es ese nodo, se compone con él.
pub(crate) fn wrap_scene(scene: &Scene, matrix: converter_scene::glam::Mat4, name: &str) -> Scene {
    use converter_scene::{Node, Transform};

    let mut scene = scene.clone();
    if let [root] = scene.root_nodes[..] {
        let node = &mut scene.nodes[root];
        if node.name == name && node.mesh.is_none() {
            node.transform = Transform::Matrix(matrix * node.transform.to_matrix());
            return scene;
        }
    }

    // Escena sin nodos: un nodo por malla para poder colgarlos del nuevo raíz
    if scene.nodes.is_empty() {
        for (i, mesh) in scene.meshes.iter().enumerate() {
            scene.nodes.push(Node {
                name: mesh.name.clone(),
                transform: Transform::identity(),
                mesh: Some(i),
                skin: None,
                children: Vec::new(),
            });
        }
    }
    let children = if scene.root_nodes.is_empty() {
        let mut is_child = vec![false; scene.nodes.len()];
        for n in &scene.nodes {
            for &c in &n.children {
                if c < is_child.len() {
                    is_child[c] = true;
                }
            }
        }
        (0..scene.nodes.len()).filter(|&i| !is_child[i]).collect()
    } else {
        scene.root_nodes.clone()
    };

    scene.nodes.push(Node {
        name: name.to_string(),
        transform: Transform::Matrix(matrix),
        mesh: None,
        skin: None,
        children,
    });
    scene.root_nodes = vec![scene.nodes.len() - 1];
    scene
}

fn calculate_quad_mesh_bounds(quad_mesh: &quadriflow_core::QuadMesh) -> BoundingBox {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for vertex in &quad_mesh.vertices {
        min[0] = min[0].min(vertex.x as f32);
        min[1] = min[1].min(vertex.y as f32);
        min[2] = min[2].min(vertex.z as f32);
        max[0] = max[0].max(vertex.x as f32);
        max[1] = max[1].max(vertex.y as f32);
        max[2] = max[2].max(vertex.z as f32);
    }

    if min[0] == f32::MAX {
        min = [0.0, 0.0, 0.0];
        max = [1.0, 1.0, 1.0];
    }

    BoundingBox { min, max }
}

#[cfg(test)]
mod tests {

    #[test]
    fn retopology_config_maps_to_remesh_config() {
        let config = RetopologyConfig {
            target_quads: 3000,
            preserve_sharp: Some(true),
            sharp_angle: Some(30.0),
            smooth_iterations: None,
            rebuild: Some("never".into()),
            curvature_alignment: Some(2.0),
            adaptive_density: Some(true),
            symmetry: Some("x".into()),
            follow_seams: Some(false),
        };
        let r = config.to_remesh_config().unwrap();
        assert_eq!(r.target_faces, 3000);
        assert!(r.preserve_sharp);
        assert!((r.sharp_angle - 30f64.to_radians()).abs() < 1e-6);
        assert_eq!(r.smooth_iterations, RemeshConfig::default().smooth_iterations);
        assert_eq!(r.rebuild, Rebuild::Never);
        assert_eq!(r.curvature_alignment, 1.0);
        assert!(r.adaptive_density);
        assert_eq!(r.symmetry, Symmetry::X);
        assert!(!r.preserve_seams);

        let unknown = RetopologyConfig { symmetry: Some("w".into()), ..config };
        assert!(unknown.to_remesh_config().is_err());
    }

    use super::*;
    use converter_scene::glam::{Quat, Vec3};
    use converter_scene::{Material, Mesh as SceneMesh, Node, Primitive, Transform};

    /// Cubo en una escena con un nodo trasladado y escalado, y un material
    fn transformed_cube_scene() -> Scene {
        let p = vec![
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ];
        let idx: Vec<u32> = vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4,
            3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5,
        ];
        let mut scene = Scene::new();
        scene.materials.push(Material { name: "rojo".into(), ..Material::default() });
        scene.meshes.push(SceneMesh {
            name: "cubo".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(p)],
                indices: Some(IndexData::U32(idx)),
                material: Some(0),
            }],
        });
        scene.nodes.push(Node {
            name: "cubo".into(),
            transform: Transform::Trs {
                translation: Vec3::new(10.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(2.0),
            },
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn viewer_and_pinocchio_mesh_share_world_space_and_order() {
        let scene = transformed_cube_scene();
        let data = scene_mesh_data(&scene);
        let mesh = scene_to_pinocchio_mesh(&scene).unwrap();
        assert_eq!(data.positions.len() / 3, mesh.num_vertices());
        for (i, v) in mesh.vertices.iter().enumerate() {
            let p = &data.positions[i * 3..i * 3 + 3];
            assert!((p[0] as f64 - v.position.x()).abs() < 1e-5);
            assert!((p[1] as f64 - v.position.y()).abs() < 1e-5);
        }
        let bbox = calculate_scene_bounds(&scene);
        assert_eq!(bbox.min, [10.0, 0.0, 0.0]);
        assert_eq!(bbox.max, [12.0, 2.0, 2.0]);
    }

    #[test]
    fn repaired_scene_exports_visible_glb() {
        let scene = transformed_cube_scene();
        let mesh = scene_to_pinocchio_mesh(&scene).unwrap();
        let repaired = mesh_to_scene(&mesh, "repaired", Some(&scene));
        assert!(repaired.validate().is_ok());

        let glb = converter_gltf_io::export_glb_bytes(&repaired, &Default::default()).unwrap();
        let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + json_len]).unwrap();
        assert_eq!(json["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(json["scenes"][0]["nodes"], serde_json::json!([0]));
        // Sin doble transformación: el bbox sigue siendo el de la escena original
        assert_eq!(repaired.compute_bounding_box(), scene.compute_bounding_box());
    }

    /// Cubo con una isla UV por cara, la cara de arriba con otro material (con
    /// textura) y, si `hole`, un triángulo de menos en la cara de atrás
    fn textured_cube_scene(hole: bool) -> Scene {
        use converter_scene::{Texture, TextureFormat, TextureRef};
        let p = [
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ];
        let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [3, 7, 6, 2], [0, 4, 7, 3], [1, 2, 6, 5]];
        let mut prims = vec![(Vec::new(), Vec::new(), Vec::new()); 2];
        for (f, q) in quads.iter().enumerate() {
            let (pos, uv, idx): &mut (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) = &mut prims[usize::from(f == 3)];
            let base = pos.len() as u32;
            let cell = [(f % 3) as f32 / 3.0, (f / 3) as f32 / 2.0];
            for (k, &v) in q.iter().enumerate() {
                let c = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]][k];
                pos.push(p[v]);
                uv.push([cell[0] + 0.3 * c[0], cell[1] + 0.4 * c[1]]);
            }
            let tris: &[[u32; 3]] = if hole && f == 0 { &[[0, 1, 2]] } else { &[[0, 1, 2], [0, 2, 3]] };
            for t in tris {
                idx.extend(t.map(|k| base + k));
            }
        }
        let mut scene = Scene::new();
        scene.materials.push(Material { name: "piel".into(), ..Material::default() });
        scene.materials.push(Material {
            name: "techo".into(),
            base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
            ..Material::default()
        });
        scene.textures.push(Texture {
            name: "techo".into(),
            data: vec![1, 2, 3],
            format: TextureFormat::Png,
            width: 1,
            height: 1,
        });
        scene.meshes.push(SceneMesh {
            name: "cubo".into(),
            primitives: prims
                .into_iter()
                .enumerate()
                .map(|(m, (pos, uv, idx))| Primitive {
                    attributes: vec![VertexAttribute::Positions(pos), VertexAttribute::TexCoords(0, uv)],
                    indices: Some(IndexData::U32(idx)),
                    material: Some(m),
                })
                .collect(),
        });
        scene.nodes.push(Node {
            name: "cubo".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        scene
    }

    /// Esquinas, UV y material de un triángulo
    type SkinTriangle = ([[f32; 3]; 3], [[f32; 2]; 3], Option<usize>);

    /// UV de cada triángulo de la escena, por las posiciones de sus esquinas
    fn corner_uvs(scene: &Scene) -> Vec<SkinTriangle> {
        scene
            .world_primitives()
            .iter()
            .flat_map(|p| {
                let uvs = p.uvs.clone().unwrap();
                p.triangles
                    .iter()
                    .map(|t| (t.map(|i| p.positions[i as usize]), t.map(|i| uvs[i as usize]), p.material))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn repair_keeps_the_skin() {
        let original = textured_cube_scene(true);
        let mut mesh = scene_to_pinocchio_mesh(&original).unwrap();
        let config = RepairConfig { fill_holes: true, ..RepairConfig::default() };
        pinocchio_repair::repair_all(&mut mesh, &config).unwrap();
        let (scene, rebuilt) = repaired_scene(&mesh, &original);
        assert!(scene.validate().is_ok());

        // Materiales y texturas del original; agujero cerrado
        assert_eq!(scene.materials.len(), 2);
        assert_eq!(scene.textures.len(), 1);
        let after = corner_uvs(&scene);
        assert_eq!(after.len(), 12);

        // Cada triángulo que ya existía conserva exactas sus UV y su material
        let before = corner_uvs(&original);
        for (pos, uv, material) in &before {
            let rotations = [[0, 1, 2], [1, 2, 0], [2, 0, 1]];
            let found = after.iter().any(|(p, u, m)| {
                rotations.iter().any(|r| {
                    r.iter().enumerate().all(|(k, &j)| p[j] == pos[k] && (0..2).all(|c| (u[j][c] - uv[k][c]).abs() < 1e-5))
                }) && m == material
            });
            assert!(found, "triángulo {pos:?} perdió su UV {uv:?} o su material {material:?}");
        }

        // El visor y el rig ven los mismos vértices, y la malla sigue sana
        let data = scene_mesh_data(&scene);
        assert_eq!(data.positions.len() / 3, rebuilt.num_vertices());
        assert!(data.uvs.is_some());
        let diagnostics = analyze_scene_mesh(&rebuilt, &scene, &RepairAnalysisConfig::default());
        assert!(diagnostics.is_healthy(), "{diagnostics:?}");
        // Las costuras del original tampoco piden reparación: solo el agujero
        let before = analyze_scene_mesh(
            &scene_to_pinocchio_mesh(&original).unwrap(),
            &original,
            &RepairAnalysisConfig::default(),
        );
        assert_eq!((before.duplicate_vertices, before.boundary_loops), (0, 1));
    }

    #[test]
    fn repair_keeps_materials_without_uvs() {
        let original = transformed_cube_scene();
        let mesh = scene_to_pinocchio_mesh(&original).unwrap();
        let (scene, rebuilt) = repaired_scene(&mesh, &original);
        assert!(scene.validate().is_ok());
        assert_eq!(scene.materials.len(), 1);
        let prims = scene.world_primitives();
        assert!(prims.iter().all(|p| p.material == Some(0) && p.uvs.is_none()));
        assert_eq!(rebuilt.num_vertices(), 8);
        assert_eq!(scene.compute_bounding_box(), original.compute_bounding_box());
    }

    #[test]
    fn print_scale_keeps_materials_and_center() {
        let scene = transformed_cube_scene();
        let center = Vector3::new(11.0, 1.0, 1.0);
        let scaled = scale_scene_about(&scene, 0.5, center);

        assert_eq!(scaled.materials.len(), 1);
        assert_eq!(scaled.meshes[0].primitives[0].material, Some(0));
        let (min, max) = scaled.compute_bounding_box().unwrap();
        assert_eq!(min, [10.5, 0.5, 0.5]);
        assert_eq!(max, [11.5, 1.5, 1.5]);
    }

    #[test]
    fn gizmo_inverse_roundtrip() {
        let params = SkeletonTransformParams {
            scale: 1.7,
            translation: [3.0, -2.0, 0.5],
            rotation: [30.0, -45.0, 120.0],
            pivot: [0.5, 1.0, -2.0],
        };
        for p in [Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 2.0, 3.0), Vector3::new(-0.4, 0.9, -1.2)] {
            let back = invert_gizmo(apply_gizmo(p, &params), &params);
            assert!(back.distance(&p) < 1e-9, "{p:?} → {back:?}");
        }
        // Identidad
        let id = SkeletonTransformParams::default();
        let p = Vector3::new(1.0, 2.0, 3.0);
        assert!(apply_gizmo(p, &id).distance(&p) < 1e-12);
    }

    #[test]
    fn gizmo_scales_and_rotates_around_pivot() {
        let skeleton = BasicSkeleton::from_bones(vec![
            Bone::new("raiz", Vector3::new(10.0, 1.0, 0.0)),
            Bone::with_parent("punta", Vector3::new(12.0, 3.0, 4.0), 0),
        ]);
        let pivot = joints_center(&skeleton);
        assert_eq!(pivot, [11.0, 2.0, 2.0]);
        let params = SkeletonTransformParams { scale: 2.0, rotation: [0.0, 90.0, 0.0], translation: [1.0, 0.0, 0.0], pivot };
        // El pivote solo se desplaza: el esqueleto no se aleja del modelo
        let moved = apply_gizmo(Vector3::new(11.0, 2.0, 2.0), &params);
        assert!(moved.distance(&Vector3::new(12.0, 2.0, 2.0)) < 1e-9);
        assert_eq!(visible_pivot(&params), Some([12.0, 2.0, 2.0]));
        assert_eq!(visible_pivot(&SkeletonTransformParams { pivot, ..Default::default() }), None);
        // La raíz queda al doble de distancia del centro
        let root = apply_gizmo(Vector3::new(10.0, 1.0, 0.0), &params);
        assert!((root.distance(&moved) - 2.0 * 6f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn processing_guard_releases_on_drop() {
        let state = AppState::new();
        {
            let _guard = state.try_begin_processing().unwrap();
            assert!(state.try_begin_processing().is_none());
        }
        // Liberado al salir del scope (igual que en un `?` temprano o un panic)
        assert!(state.try_begin_processing().is_some());
    }

    #[test]
    fn reset_derived_clears_previous_model_state() {
        let state = AppState::new();
        let mesh = scene_to_pinocchio_mesh(&transformed_cube_scene()).unwrap();
        *state.mesh_before_repair.lock().unwrap() = Some(mesh);
        *state.scene_before_repair.lock().unwrap() = Some(transformed_cube_scene());
        state.reset_derived();
        assert!(state.mesh_before_repair.lock().unwrap().is_none());
        assert!(state.scene_before_repair.lock().unwrap().is_none());
    }

    fn chain_skeleton() -> (BasicSkeleton, Vec<Vector3>) {
        let skel = BasicSkeleton::from_bones(vec![
            Bone::new("root", Vector3::new(11.0, 0.0, 1.0)),
            Bone::with_parent("mid", Vector3::new(11.0, 1.0, 1.0), 0),
            Bone::with_parent("tip", Vector3::new(11.0, 2.0, 1.0), 1).as_leaf(),
        ]);
        let positions = skel.bones().iter().map(|b| b.position).collect();
        (skel, positions)
    }

    #[test]
    fn rigged_scene_exports_skin_to_glb_and_usd() {
        let scene = transformed_cube_scene();
        let prims = scene.world_primitives();
        let (skel, joints) = chain_skeleton();
        // Mitad inferior al segmento root→mid (hueso 1), superior a mid→tip (hueso 2)
        let weights: Vec<Vec<f64>> = prims[0]
            .positions
            .iter()
            .map(|p| if p[1] < 1.0 { vec![0.0, 1.0, 0.0] } else { vec![0.0, 0.3, 0.7] })
            .collect();
        let rigged = rigged_scene(&scene, &prims, &weights, &skel, &joints, &[]);
        assert!(rigged.validate().is_ok());
        assert_eq!(rigged.materials.len(), 1, "conserva materiales");

        // Joints en la cabeza de cada segmento, colgando del padre
        let sk = &rigged.skeletons[0];
        assert_eq!(sk.joints.len(), 3);
        assert_eq!(sk.joints[0].children, vec![1]);
        let world_mid = sk.joints[0].local_transform * sk.joints[1].local_transform;
        assert_eq!(world_mid.w_axis.truncate(), converter_scene::glam::Vec3::new(11.0, 0.0, 1.0));

        // GLB: ida y vuelta conserva skin y pesos
        let glb = converter_gltf_io::export_glb_bytes(&rigged, &Default::default()).unwrap();
        let back = converter_gltf_io::import_gltf_bytes(&glb).unwrap();
        assert_eq!(back.skeletons.len(), 1);
        assert_eq!(back.skeletons[0].joints.len(), 3);
        let attrs = &back.meshes[0].primitives[0].attributes;
        let joint_weights = attrs.iter().find_map(|a| match a {
            VertexAttribute::JointWeights(w) => Some(w),
            _ => None,
        });
        let w = joint_weights.expect("WEIGHTS_0");
        assert!(w.iter().all(|w| (w.iter().sum::<f32>() - 1.0).abs() < 1e-5));

        // USD: UsdSkel con índices y pesos
        let usda = converter_usda::write_usda(&rigged, &Default::default()).unwrap().usda;
        assert!(usda.contains("SkelRoot"), "{usda}");
        assert!(usda.contains("primvars:skel:jointIndices"));
        assert!(usda.contains("primvars:skel:jointWeights"));
    }

    #[test]
    fn skeleton_only_glb_has_skin_and_animation_without_mesh() {
        use crate::animation::{AnimationClip, JointTrack, Key, KeyInterpolation};
        let (skel, joints) = chain_skeleton();
        let clip = AnimationClip {
            name: "doblar".into(),
            fps: 24.0,
            tracks: vec![JointTrack {
                joint: 1,
                rotation: vec![
                    Key { frame: 0.0, value: [0.0, 0.0, 0.0, 1.0], interpolation: KeyInterpolation::Linear },
                    Key { frame: 24.0, value: Quat::from_rotation_z(0.8).to_array(), interpolation: KeyInterpolation::Linear },
                ],
                translation: vec![],
            }],
            start: None,
            end: None,
        };
        for with_shapes in [false, true] {
            let scene = skeleton_only_scene(&skel, &joints, std::slice::from_ref(&clip), with_shapes);
            assert!(scene.validate().is_ok(), "{:?}", scene.validate());
            let glb = converter_gltf_io::export_glb_bytes(&scene, &Default::default()).unwrap();
            if let Ok(dir) = std::env::var("PINOCCHIO_EXPORT_DIR") {
                std::fs::write(format!("{dir}/esqueleto_{with_shapes}.glb"), &glb).unwrap();
            }
            let json = glb_json(&glb);
            assert_eq!(json["skins"].as_array().unwrap().len(), 1);
            assert_eq!(json["animations"].as_array().unwrap().len(), 1);
            assert_eq!(json["nodes"].as_array().unwrap().len(), 1 + skel.num_bones(), "contenedor + joints");
            let meshes = json.get("meshes").and_then(|m| m.as_array()).map_or(0, |m| m.len());
            assert_eq!(meshes, usize::from(with_shapes));
            // El skin es válido y lo usa la malla de huesos (si la hay)
            assert_eq!(json["nodes"][0].get("skin").is_some(), with_shapes);
        }
    }

    #[test]
    fn rigged_scene_animation_targets_skin_joints() {
        use crate::animation::{AnimationClip, JointTrack, Key, KeyInterpolation};
        use converter_scene::glam::Quat;
        let scene = transformed_cube_scene();
        let prims = scene.world_primitives();
        let (skel, joints) = chain_skeleton();
        let weights: Vec<Vec<f64>> = prims[0].positions.iter().map(|_| vec![0.0, 0.5, 0.5]).collect();
        let bend = Quat::from_rotation_z(0.8).to_array();
        let clip = AnimationClip {
            name: "doblar".into(),
            fps: 24.0,
            tracks: vec![
                // Girar en "mid" mueve el segmento mid → tip (joint 2)
                JointTrack {
                    joint: 1,
                    rotation: vec![
                        Key { frame: 0.0, value: [0.0, 0.0, 0.0, 1.0], interpolation: KeyInterpolation::Linear },
                        Key { frame: 24.0, value: bend, interpolation: KeyInterpolation::Linear },
                    ],
                    translation: vec![],
                },
                JointTrack {
                    joint: 0,
                    rotation: vec![],
                    translation: vec![Key { frame: 12.0, value: [0.0, 0.0, 1.0], interpolation: KeyInterpolation::Step }],
                },
            ],
            start: None,
            end: None,
        };
        let rigged = rigged_scene(&scene, &prims, &weights, &skel, &joints, &[clip]);
        assert!(rigged.validate().is_ok(), "{:?}", rigged.validate());
        assert_eq!(rigged.nodes.len(), 4, "malla + un nodo por joint");

        let glb = converter_gltf_io::export_glb_bytes(&rigged, &Default::default()).unwrap();
        let json = glb_json(&glb);
        assert_eq!(json["nodes"].as_array().unwrap().len(), 4, "los joints no se duplican");
        let skin_joints: Vec<u64> = json["skins"][0]["joints"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        let channels = json["animations"][0]["channels"].as_array().unwrap();
        let targets: Vec<(u64, &str)> = channels
            .iter()
            .map(|c| (c["target"]["node"].as_u64().unwrap(), c["target"]["path"].as_str().unwrap()))
            .collect();
        assert_eq!(targets, vec![(skin_joints[2], "rotation"), (skin_joints[0], "translation")]);

        // Ida y vuelta: la animación sigue apuntando a los joints del skin
        let back = converter_gltf_io::import_gltf_bytes(&glb).unwrap();
        let joint_nodes: Vec<usize> = back.skeletons[0].joints.iter().filter_map(|j| j.node_index).collect();
        assert!(back.animations[0].channels.iter().all(|c| joint_nodes.contains(&c.node)));

        let usda = converter_usda::write_usda(&rigged, &Default::default()).unwrap().usda;
        assert!(usda.contains("SkelAnimation"), "{usda}");
    }

    /// JSON de un GLB (primer chunk)
    fn glb_json(glb: &[u8]) -> serde_json::Value {
        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&glb[20..20 + len]).unwrap()
    }

    #[test]
    fn body_plans_round_trip_and_are_listed() {
        for (id, _, _) in pinocchio_skeleton::BodyPlan::variants() {
            let plan = pinocchio_skeleton::BodyPlan::variant(id).unwrap();
            let dto = get_body_plan(format!("plan:{id}")).expect("variante");
            assert_eq!(dto.to_plan().unwrap(), plan, "{id}");
        }
        assert!(get_body_plan("human".into()).is_none());
        let presets = list_skeleton_presets();
        assert!(presets.iter().any(|p| p.id == "plan:elephant" && p.num_bones > 20));
        let bad = BodyPlanDto { shape: "blob".into(), ..get_body_plan("plan:fish".into()).unwrap() };
        assert!(bad.to_plan().is_err());
    }

    /// Tira de dos quads (6 vértices) y la misma geometría como malla original
    fn strip() -> (quadriflow_core::QuadMesh, Mesh) {
        use quadriflow_core::{QuadFace, QuadMesh};
        let p = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [2.0, 0.0], [2.0, 1.0]];
        let quad = QuadMesh {
            vertices: p.iter().map(|q| nalgebra::Vector3::new(q[0], q[1], 0.0)).collect(),
            faces: vec![QuadFace { v: [0, 1, 2, 3] }, QuadFace { v: [1, 4, 5, 2] }],
        };
        let positions: Vec<Vector3> = p.iter().map(|q| Vector3::new(q[0], q[1], 0.0)).collect();
        let mesh = Mesh::from_triangles(&positions, &[[0, 1, 2], [0, 2, 3], [1, 4, 5], [1, 5, 2]]);
        (quad, mesh)
    }

    #[test]
    fn weights_follow_the_active_mesh() {
        use std::sync::atomic::Ordering;
        let (quad, mesh) = strip();
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(mesh);
        *state.quad_mesh.lock().unwrap() = Some(quad);
        // Abajo hueso 0, arriba hueso 1
        let rows: Vec<Vec<f64>> =
            [0.0, 0.0, 1.0, 1.0, 0.0, 1.0].iter().map(|&y| if y < 0.5 { vec![1.0, 0.0] } else { vec![0.0, 1.0] }).collect();

        // Rig sobre los quads: exportar los quads no traslada; la original sí
        state.rig_on_quad.store(true, Ordering::SeqCst);
        assert_eq!(weights_for(&state, rows.clone(), WeightTarget::Retopology).unwrap(), rows);
        let original = weights_for(&state, rows.clone(), WeightTarget::Original).unwrap();
        for (a, b) in original.iter().zip(&rows) {
            assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9));
        }
        // Rig sobre la original: al revés
        state.rig_on_quad.store(false, Ordering::SeqCst);
        assert_eq!(weights_for(&state, rows.clone(), WeightTarget::Original).unwrap(), rows);
        assert!(weights_for(&state, rows[..3].to_vec(), WeightTarget::Original).is_err());

        // La malla activa es la de quads mientras exista y se use
        assert!(state.active_is_quad());
        state.use_retopology.store(false, Ordering::SeqCst);
        assert!(!state.active_is_quad());
    }

    /// Retopologizar o cambiar la malla activa no descarta el rig: los pesos
    /// pasan a la otra malla y el esqueleto queda igual
    #[test]
    fn rig_moves_to_another_mesh() {
        let (quad, mesh) = strip();
        // Izquierda hueso 0, derecha hueso 1, el medio mitad y mitad
        let weights: Vec<Vec<f64>> =
            mesh.vertices.iter().map(|v| v.position.x()).map(|x| vec![1.0 - x / 2.0, x / 2.0]).collect();
        let mut rig = PinocchioOutput {
            attachment: Attachment::new(&mesh, weights.clone(), 2),
            embedding: pinocchio_embedding::EmbeddingResult {
                bone_positions: vec![Vector3::zero(); 2],
                sphere_bone_map: Vec::new(),
                quality_score: 1.0,
            },
            bone_positions: vec![Vector3::zero(); 2],
            bone_rest_transforms: Vec::new(),
            stats: Default::default(),
        };
        let target = quad_as_mesh(&quad);
        assert!(move_rig(&mut rig, &mesh, &target));
        assert_eq!(rig.attachment.num_vertices(), target.num_vertices());
        assert_eq!(rig.stats.num_vertices, target.num_vertices());
        for (v, w) in weights.iter().enumerate() {
            assert!(rig.get_weights(v).iter().zip(w).all(|(a, b)| (a - b).abs() < 1e-9));
        }
        // Pesos de otra malla: no se trasladan
        let small = Mesh::from_triangles(&[Vector3::zero(), Vector3::unit_x(), Vector3::unit_y()], &[[0, 1, 2]]);
        assert!(!move_rig(&mut rig, &small, &target));
    }

    /// Reparar (o deshacerlo) cambia la topología de la malla original: la
    /// retopología se descarta y el rig pasa a la malla nueva
    #[test]
    fn rig_survives_mesh_replacement() {
        use std::sync::atomic::Ordering;
        let (quad, mesh) = strip();
        let weights: Vec<Vec<f64>> =
            mesh.vertices.iter().map(|v| v.position.x()).map(|x| vec![1.0 - x / 2.0, x / 2.0]).collect();
        let rig = |source: &Mesh, weights: Vec<Vec<f64>>| PinocchioOutput {
            attachment: Attachment::new(source, weights, 2),
            embedding: pinocchio_embedding::EmbeddingResult {
                bone_positions: vec![Vector3::zero(); 2],
                sphere_bone_map: Vec::new(),
                quality_score: 1.0,
            },
            bone_positions: vec![Vector3::zero(); 2],
            bone_rest_transforms: Vec::new(),
            stats: Default::default(),
        };
        // La misma tira con la otra diagonal y un vértice más en el medio de abajo
        let positions: Vec<Vector3> = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [2.0, 0.0], [2.0, 1.0], [0.5, 0.0]]
            .iter()
            .map(|q| Vector3::new(q[0], q[1], 0.0))
            .collect();
        let repaired =
            Mesh::from_triangles(&positions, &[[0, 6, 3], [6, 1, 3], [1, 2, 3], [1, 4, 2], [4, 5, 2]]);

        // Rig sobre la original
        let state = AppState::new();
        *state.mesh.lock().unwrap() = Some(repaired.clone());
        *state.quad_mesh.lock().unwrap() = Some(quad.clone());
        *state.result.lock().unwrap() = Some(rig(&mesh, weights.clone()));
        assert!(mesh_replaced(&state, &mesh));
        assert!(state.quad_mesh.lock().unwrap().is_none(), "la retopología ya no corresponde");
        {
            let result = state.result.lock().unwrap();
            let result = result.as_ref().unwrap();
            assert_eq!(result.attachment.num_vertices(), repaired.num_vertices());
            for (v, p) in positions.iter().enumerate() {
                assert!((result.get_weights(v)[1] - p.x() / 2.0).abs() < 1e-9);
            }
        }

        // Rig sobre los quads: pasa de los quads a la malla nueva
        *state.quad_mesh.lock().unwrap() = Some(quad);
        state.rig_on_quad.store(true, Ordering::SeqCst);
        *state.result.lock().unwrap() = Some(rig(&mesh, weights));
        assert!(mesh_replaced(&state, &mesh));
        assert!(!state.rig_on_quad.load(Ordering::SeqCst));
        assert_eq!(state.result.lock().unwrap().as_ref().unwrap().attachment.num_vertices(), repaired.num_vertices());

        // Sin rig no hay nada que conservar
        *state.result.lock().unwrap() = None;
        assert!(!mesh_replaced(&state, &mesh));
    }

    #[test]
    fn quad_view_maps_seam_copies_to_their_vertex() {
        let (quad, _) = strip();
        let skin = uv_core::Skin {
            corners: vec![
                [[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]],
                [[0.6, 0.0], [1.0, 0.0], [1.0, 1.0], [0.6, 1.0]],
            ],
            face_material: vec![Some(0), Some(0)],
            materials: vec![converter_scene::Material::default()],
            textures: vec![],
            info: uv_core::SkinInfo::Transferred { seam_faces: 0 },
        };
        let (data, source) = quad_view(&quad, Some(&skin));
        assert_eq!(source, vec![0, 1, 2, 3, 1, 4, 5, 2], "la arista de la costura se duplica");
        assert_eq!(data.positions.len(), 3 * source.len());
        let (_, plain) = quad_view(&quad, None);
        assert_eq!(plain, vec![0, 1, 2, 3, 4, 5]);
    }

    /// Con piel, los triángulos quedan en rangos por material
    #[test]
    fn quad_view_groups_faces_by_material() {
        let (quad, _) = strip();
        let skin = uv_core::Skin {
            corners: vec![[[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]]; 2],
            face_material: vec![Some(1), Some(0)],
            materials: vec![converter_scene::Material::default(); 2],
            textures: vec![],
            info: uv_core::SkinInfo::Transferred { seam_faces: 0 },
        };
        let (data, source) = quad_view(&quad, Some(&skin));
        assert_eq!(data.groups, vec![[0, 6, 0], [6, 6, 1]]);
        // La primera cara del visor es la segunda de la malla (material 0)
        assert_eq!(&source[..4], &quad.faces[1].v);
        assert!(quad_view(&quad, None).0.groups.is_empty());
    }

    #[test]
    fn mirror_vertices_pair_both_sides() {
        // Tira simétrica respecto de x = 0, más un vértice sin pareja
        let positions = vec![
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(-1.0, 1.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 2.0, 0.0),
            Vector3::new(-0.5, 5.0, 0.0),
        ];
        let plane = (Vector3::zero(), Vector3::unit_x());
        let mirror = mirror_vertices(&positions, plane);
        assert_eq!(&mirror[..5], &[1, 0, 3, 2, 4], "el del medio es su propio espejo");
        assert_eq!(mirror[5], u32::MAX, "sin pareja cerca");
    }

    #[test]
    fn weights_follow_the_quad_mesh() {
        use quadriflow_core::{QuadFace, QuadMesh};
        // Tira vertical: abajo hueso 0, arriba hueso 1
        let positions: Vec<Vector3> = (0..=4)
            .flat_map(|j| [Vector3::new(0.0, j as f64, 0.0), Vector3::new(1.0, j as f64, 0.0)])
            .collect();
        let triangles: Vec<[usize; 3]> = (0..4)
            .flat_map(|j| [[2 * j, 2 * j + 1, 2 * j + 3], [2 * j, 2 * j + 3, 2 * j + 2]])
            .collect();
        let mesh = Mesh::from_triangles(&positions, &triangles);
        let source: Vec<Vec<f64>> = positions
            .iter()
            .map(|p| if p.y() < 2.0 { vec![1.0, 0.0] } else { vec![0.0, 1.0] })
            .collect();

        let quad = QuadMesh {
            vertices: vec![
                nalgebra::Vector3::new(0.0, 0.5, 0.0),
                nalgebra::Vector3::new(1.0, 0.5, 0.0),
                nalgebra::Vector3::new(1.0, 3.5, 0.0),
                nalgebra::Vector3::new(0.0, 3.5, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }],
        };
        let weights = weights_on_quad_mesh(&mesh, &source, &quad).unwrap();
        assert_eq!(weights.len(), 4);
        assert!(weights[0][0] > 0.99 && weights[1][0] > 0.99);
        assert!(weights[2][1] > 0.99 && weights[3][1] > 0.99);

        let (indices, table) = influence_table(&weights, 4);
        assert_eq!(indices[2][0], 1);
        assert!(table.iter().all(|w| w.len() == 4 && (w.iter().sum::<f64>() - 1.0).abs() < 1e-9));

        // Pesos de otra malla: error en vez de índices sin sentido
        assert!(weights_on_quad_mesh(&mesh, &source[..3], &quad).is_err());
    }

    #[test]
    fn quad_mesh_exports_as_triangles() {
        use quadriflow_core::{QuadFace, QuadMesh};
        let quad = QuadMesh {
            vertices: vec![
                nalgebra::Vector3::new(0.0, 0.0, 0.0),
                nalgebra::Vector3::new(1.0, 0.0, 0.0),
                nalgebra::Vector3::new(1.0, 1.0, 0.0),
                nalgebra::Vector3::new(0.0, 1.0, 0.0),
            ],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }],
        };
        let (scene, source) = quad_mesh_to_scene(&quad, None, &Scene::default());
        assert!(scene.validate().is_ok());
        let prims = scene.world_primitives();
        assert_eq!(prims[0].triangles, vec![[0, 1, 2], [0, 2, 3]]);
        assert_eq!(source, vec![0, 1, 2, 3]);
    }

    /// Dos quads que comparten una arista con UV distintas a cada lado: los
    /// vértices de esa arista se duplican y cada material va en su primitiva
    #[test]
    fn quad_mesh_with_uvs_splits_seams_and_materials() {
        use quadriflow_core::{QuadFace, QuadMesh};
        let v = |x: f64, y: f64| nalgebra::Vector3::new(x, y, 0.0);
        let quad = QuadMesh {
            vertices: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0), v(2.0, 0.0), v(2.0, 1.0)],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }, QuadFace { v: [1, 4, 5, 2] }],
        };
        let uvs = uv_core::Skin {
            corners: vec![
                [[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]],
                [[0.6, 0.0], [1.0, 0.0], [1.0, 1.0], [0.6, 1.0]],
            ],
            face_material: vec![Some(0), Some(0)],
            materials: vec![converter_scene::Material::default()],
            textures: vec![],
            info: uv_core::SkinInfo::Transferred { seam_faces: 0 },
        };
        let base = Scene::default();
        let (scene, source) = quad_mesh_to_scene(&quad, Some(&uvs), &base);
        assert!(scene.validate().is_ok());
        let prims = scene.world_primitives();
        assert_eq!(prims.len(), 1);
        assert_eq!(prims[0].material, Some(0));
        assert_eq!(prims[0].positions.len(), 8, "la arista de la costura se duplica");
        assert_eq!(source, vec![0, 1, 2, 3, 1, 4, 5, 2]);
        assert_eq!(prims[0].uvs.as_ref().unwrap()[4], [0.6, 0.0]);

        // Mismas UV a ambos lados: la arista se comparte
        let mut continuous = uvs.clone();
        continuous.corners[1] = [[0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 1.0]];
        let (scene, _) = quad_mesh_to_scene(&quad, Some(&continuous), &base);
        assert_eq!(scene.world_primitives()[0].positions.len(), 6);
    }

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
    }

    /// El decodificador del frontend (apps/web/src/lib/buffers.ts) asume este
    /// formato exacto
    #[test]
    fn mesh_binary_layout() {
        let data = MeshData {
            positions: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            normals: vec![0.5; 9],
            indices: vec![0, 1, 2],
            uvs: Some(vec![0.25; 6]),
            groups: vec![],
        };
        let bytes = data.to_bytes();
        let w = words(&bytes);
        assert_eq!(&w[..4], &[3, 3, 1, 0]);
        assert_eq!(bytes.len(), 4 * (4 + 9 + 9 + 6 + 3));
        assert_eq!(f32::from_bits(w[4 + 1]), 1.0);
        assert_eq!(f32::from_bits(w[4 + 9]), 0.5);
        assert_eq!(f32::from_bits(w[4 + 18]), 0.25);
        assert_eq!(&w[4 + 24..], &[0, 1, 2]);

        // Grupos por material al final: cantidad y [inicio, cantidad, material]
        let grouped = MeshData { groups: vec![[0, 3, 2], [3, 0, u32::MAX]], ..data.clone() };
        let w = words(&grouped.to_bytes());
        assert_eq!(&w[4 + 27..], &[2, 0, 3, 2, 3, 0, u32::MAX]);

        let quads = pack_mesh(&data.positions, &data.normals, None, &data.indices, &[0, 1, 2, 0]);
        let w = words(&quads);
        assert_eq!(&w[..4], &[3, 3, 0, 4]);
        assert_eq!(&w[w.len() - 7..], &[0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn weights_binary_layout() {
        let data = WeightsData {
            num_vertices: 1,
            num_bones: 2,
            bone_names: vec!["raíz".into(), "brazo".into()],
            weights: vec![0.0, 0.75, 1.0, 0.25],
            max_influences: 2,
        };
        let bytes = data.to_bytes();
        let w = words(&bytes[..16]);
        let names_len = "raíz\nbrazo".len();
        assert_eq!(w, vec![1, 2, 2, names_len as u32]);
        assert_eq!(&bytes[16..16 + names_len], "raíz\nbrazo".as_bytes());
        let start = 16 + names_len.div_ceil(4) * 4;
        let weights: Vec<f32> = bytes[start..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(weights, data.weights);
    }
}
