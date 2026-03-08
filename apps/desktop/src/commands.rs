//! Comandos Tauri para la aplicación Pinocchio
//!
//! Fase 1: Import/Export multi-formato usando converter-*
//! Fase 2: Retopología con QuadriFlow

use crate::state::{AppState, SkeletonType};
use converter_scene::{IndexData, Scene, VertexAttribute};
use pinocchio_core::{autorig, PinocchioConfig};
use pinocchio_mesh::Mesh;
use pinocchio_math::Vector3;
use pinocchio_skeleton::{
    BasicSkeleton, BirdSkeleton, Bone, CentaurSkeleton, HorseSkeleton, HumanSkeleton,
    MechSkeleton, QuadSkeleton, SerpentSkeleton, SpiderSkeleton, Skeleton,
};
use quadriflow_core::{remesh_with_callback, RemeshConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{ipc::Channel, State};

// Repair & Print3D
use pinocchio_repair::{self, AnalysisConfig as RepairAnalysisConfig, RepairConfig, HoleFillConfig, HoleFillMethod};
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
}

/// Bounding box serializable
#[derive(Debug, Clone, Serialize)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Datos de la malla para Three.js
#[derive(Debug, Clone, Serialize)]
pub struct MeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub uvs: Option<Vec<f32>>,
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

/// Mensaje de progreso
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub percent: u32,
    pub message: String,
}

/// Datos de los pesos para visualización
#[derive(Debug, Clone, Serialize)]
pub struct WeightsData {
    pub num_vertices: usize,
    pub num_bones: usize,
    pub bone_names: Vec<String>,
    pub weights: Vec<f64>,
    pub max_influences: usize,
}

/// Configuración de exportación
#[derive(Debug, Clone, Deserialize)]
pub struct ExportConfig {
    pub format: String,
    pub path: String,
    pub include_skeleton: Option<bool>,
    pub include_weights: Option<bool>,
    pub texture_quality: Option<u8>,
    pub max_texture_size: Option<u32>,
    pub optimize_geometry: Option<bool>,
    pub generate_normals: Option<bool>,
    pub flatten_transforms: Option<bool>,
    pub scale_factor: Option<f64>,
    pub strip_unused: Option<bool>,
    pub export_animations: Option<bool>,
    // USDZ
    pub arkit_compatible: Option<bool>,
    pub fps: Option<f64>,
}

/// Resultado de exportación
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub success: bool,
    pub path: String,
    pub message: String,
    pub files_created: Vec<String>,
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
    /// Usar resolución adaptiva
    pub adaptive: Option<bool>,
    /// Iteraciones de suavizado del campo de orientación
    pub smooth_iterations: Option<usize>,
    /// Eliminar T-junctions con SAT solver (más lento pero más limpio)
    pub remove_flips: Option<bool>,
}

/// Información del resultado de retopología
#[derive(Debug, Clone, Serialize)]
pub struct QuadMeshInfo {
    pub num_vertices: usize,
    pub num_quads: usize,
    pub bounding_box: BoundingBox,
}

/// Datos de la malla de quads para Three.js
/// Los quads se triangulan para renderizado
#[derive(Debug, Clone, Serialize)]
pub struct QuadMeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    /// Índices de quads originales (para visualización de wireframe)
    pub quad_indices: Vec<u32>,
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
                id: "usdz".to_string(),
                name: "USDZ".to_string(),
                extensions: vec!["usdz".to_string()],
                description: "Universal Scene Description (Apple AR)".to_string(),
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
pub fn import_model(path: String, state: State<'_, AppState>) -> Result<MeshInfo, String> {
    let path = Path::new(&path);

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
        format: ext.to_uppercase(),
    };

    // Convertir Scene a Mesh de pinocchio para autorig
    let mesh = scene_to_pinocchio_mesh(&scene)?;

    // Guardar en estado
    let mut scene_lock = state.scene.lock().unwrap();
    *scene_lock = Some(scene);

    let mut mesh_lock = state.mesh.lock().unwrap();
    *mesh_lock = Some(mesh);

    // Limpiar resultado anterior
    let mut result_lock = state.result.lock().unwrap();
    *result_lock = None;

    Ok(info)
}

/// Obtiene los datos de la malla para renderizar en Three.js
#[tauri::command]
pub fn get_mesh_data(state: State<'_, AppState>) -> Result<MeshData, String> {
    let scene_lock = state.scene.lock().unwrap();
    let scene = scene_lock.as_ref().ok_or("No hay escena cargada")?;

    // Recolectar todos los vértices y caras de la escena
    let mut positions: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut uvs: Vec<f32> = Vec::new();
    let mut has_uvs = false;

    let mut vertex_offset: u32 = 0;

    for mesh in &scene.meshes {
        for primitive in &mesh.primitives {
            // Extraer posiciones
            let pos_data = primitive.attributes.iter().find_map(|attr| {
                if let VertexAttribute::Positions(p) = attr {
                    Some(p)
                } else {
                    None
                }
            });

            if let Some(pos_data) = pos_data {
                let pos_count = pos_data.len();

                // Aplanar posiciones [f32; 3] -> Vec<f32>
                for p in pos_data {
                    positions.extend_from_slice(p);
                }

                // Extraer normales
                let norm_data = primitive.attributes.iter().find_map(|attr| {
                    if let VertexAttribute::Normals(n) = attr {
                        Some(n)
                    } else {
                        None
                    }
                });

                if let Some(norm_data) = norm_data {
                    for n in norm_data {
                        normals.extend_from_slice(n);
                    }
                } else {
                    // Generar normales dummy si no existen
                    for _ in 0..pos_count {
                        normals.extend_from_slice(&[0.0, 1.0, 0.0]);
                    }
                }

                // Extraer UVs (set 0)
                let uv_data = primitive.attributes.iter().find_map(|attr| {
                    if let VertexAttribute::TexCoords(0, uv) = attr {
                        Some(uv)
                    } else {
                        None
                    }
                });

                if let Some(uv_data) = uv_data {
                    for uv in uv_data {
                        uvs.extend_from_slice(uv);
                    }
                    has_uvs = true;
                }

                // Índices
                if let Some(index_data) = &primitive.indices {
                    match index_data {
                        IndexData::U16(idx) => {
                            indices.extend(idx.iter().map(|&i| i as u32 + vertex_offset));
                        }
                        IndexData::U32(idx) => {
                            indices.extend(idx.iter().map(|&i| i + vertex_offset));
                        }
                    }
                } else {
                    // Sin índices - generar secuenciales
                    for i in 0..pos_count as u32 {
                        indices.push(vertex_offset + i);
                    }
                }

                vertex_offset += pos_count as u32;
            }
        }
    }

    Ok(MeshData {
        positions,
        normals,
        indices,
        uvs: if has_uvs { Some(uvs) } else { None },
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// EXPORT COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Exporta el modelo actual
#[tauri::command]
pub fn export_model(config: ExportConfig, state: State<'_, AppState>) -> Result<ExportResult, String> {
    let scene_lock = state.scene.lock().unwrap();
    let scene = scene_lock.as_ref().ok_or("No hay escena para exportar")?;

    let path = Path::new(&config.path);
    let mut files_created = Vec::new();

    match config.format.as_str() {
        "glb" => {
            let glb_opts = converter_gltf_io::GlbExportOptions {
                texture_quality: config.texture_quality,
                max_texture_size: config.max_texture_size,
                optimize_geometry: config.optimize_geometry.unwrap_or(false),
                generate_normals: config.generate_normals.unwrap_or(false),
                flatten_transforms: config.flatten_transforms.unwrap_or(false),
                scale_factor: config.scale_factor,
                export_animations: config.export_animations.unwrap_or(true),
                strip_unused: config.strip_unused.unwrap_or(false),
            };
            converter_gltf_io::export_glb(scene, path, &glb_opts)
                .map_err(|e| format!("Error exportando GLB: {:?}", e))?;
            files_created.push(config.path.clone());
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
        "json" => {
            // Exportar pesos de skinning
            return export_weights_json(&config, &state);
        }
        _ => return Err(format!("Formato de exportación no soportado: {}", config.format)),
    }

    Ok(ExportResult {
        success: true,
        path: config.path,
        message: "Exportación completada".to_string(),
        files_created,
    })
}

fn export_weights_json(config: &ExportConfig, state: &State<'_, AppState>) -> Result<ExportResult, String> {
    let result_lock = state.result.lock().unwrap();
    let result = result_lock.as_ref().ok_or("No hay resultado de autorig")?;

    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto")?;

    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla")?;

    let max_influences = 4;
    let (indices, weights) = result.export_weights(max_influences);

    let bone_names: Vec<String> = get_bone_names(skeleton_type);

    let export_data = serde_json::json!({
        "version": "1.0",
        "mesh": {
            "num_vertices": mesh.num_vertices(),
            "num_faces": mesh.num_faces(),
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
        files_created: vec![config.path.clone()],
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// SKELETON COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Lista los presets de esqueleto disponibles
#[tauri::command]
pub fn list_skeleton_presets() -> Vec<SkeletonPreset> {
    vec![
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
        _ => return Err(format!("Preset desconocido: {}", preset_id)),
    };

    let data = get_skeleton_data_for_type(&skeleton_type);

    let mut skeleton_lock = state.skeleton.lock().unwrap();
    *skeleton_lock = Some(skeleton_type.clone());

    // Guardar como original (resetear transformaciones previas)
    let mut orig_lock = state.original_skeleton.lock().unwrap();
    *orig_lock = Some(skeleton_type);

    let mut result_lock = state.result.lock().unwrap();
    *result_lock = None;

    Ok(data)
}

// ═══════════════════════════════════════════════════════════════════════════
// SKELETON TRANSFORM COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Transforma el esqueleto (escala, traslación, rotación)
#[tauri::command]
pub fn transform_skeleton(
    scale: f64,
    translation: [f64; 3],
    rotation: [f64; 3],
    state: State<'_, AppState>,
) -> Result<SkeletonData, String> {
    // Asegurar que tenemos original guardado
    {
        let mut orig = state.original_skeleton.lock().unwrap();
        if orig.is_none() {
            let skel = state.skeleton.lock().unwrap();
            let cloned = skel.as_ref().ok_or("No hay esqueleto seleccionado")?.clone();
            drop(skel);
            *orig = Some(cloned);
        }
    }

    // Partir del original
    let original = state.original_skeleton.lock().unwrap();
    let original_type = original.as_ref().ok_or("No hay esqueleto original")?;
    let original_data = get_skeleton_data_for_type(original_type);

    // Crear BasicSkeleton desde los datos originales
    let bones: Vec<Bone> = original_data.bones.iter().map(|b| {
        let mut bone = if let Some(parent) = b.parent {
            Bone::with_parent(&b.name, Vector3::new(b.position[0], b.position[1], b.position[2]), parent)
        } else {
            Bone::new(&b.name, Vector3::new(b.position[0], b.position[1], b.position[2]))
        };
        if b.is_leaf {
            bone = bone.as_leaf();
        }
        bone
    }).collect();

    let mut skel = BasicSkeleton::from_bones(bones);

    // Aplicar escala
    if (scale - 1.0).abs() > 1e-6 {
        skel.scale(scale);
    }

    // Aplicar rotación (euler XYZ en grados)
    let rx = rotation[0].to_radians();
    let ry = rotation[1].to_radians();
    let rz = rotation[2].to_radians();

    if rx.abs() > 1e-6 || ry.abs() > 1e-6 || rz.abs() > 1e-6 {
        let (sx, cx) = rx.sin_cos();
        let (sy, cy) = ry.sin_cos();
        let (sz, cz) = rz.sin_cos();

        for bone in skel.bones_mut() {
            let x = bone.position.x();
            let y = bone.position.y();
            let z = bone.position.z();

            // Rotación X
            let y1 = y * cx - z * sx;
            let z1 = y * sx + z * cx;
            // Rotación Y
            let x2 = x * cy + z1 * sy;
            let z2 = -x * sy + z1 * cy;
            // Rotación Z
            let x3 = x2 * cz - y1 * sz;
            let y3 = x2 * sz + y1 * cz;

            bone.position = Vector3::new(x3, y3, z2);
        }
    }

    // Aplicar traslación
    if translation[0].abs() > 1e-6 || translation[1].abs() > 1e-6 || translation[2].abs() > 1e-6 {
        skel.translate(Vector3::new(translation[0], translation[1], translation[2]));
    }

    let data = skeleton_to_data(&skel);

    // Guardar como Custom
    let mut skeleton_lock = state.skeleton.lock().unwrap();
    *skeleton_lock = Some(SkeletonType::Custom(skel));

    // Limpiar resultado autorig
    let mut result_lock = state.result.lock().unwrap();
    *result_lock = None;

    Ok(data)
}

/// Mueve un hueso individual
#[tauri::command]
pub fn move_bone(
    bone_index: usize,
    position: [f64; 3],
    state: State<'_, AppState>,
) -> Result<SkeletonData, String> {
    let mut skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto seleccionado")?;

    // Convertir a BasicSkeleton si no lo es
    let mut skel = match skeleton_type {
        SkeletonType::Custom(s) => s.clone(),
        other => {
            let data = get_skeleton_data_for_type(other);
            let bones: Vec<Bone> = data.bones.iter().map(|b| {
                let mut bone = if let Some(parent) = b.parent {
                    Bone::with_parent(&b.name, Vector3::new(b.position[0], b.position[1], b.position[2]), parent)
                } else {
                    Bone::new(&b.name, Vector3::new(b.position[0], b.position[1], b.position[2]))
                };
                if b.is_leaf {
                    bone = bone.as_leaf();
                }
                bone
            }).collect();
            BasicSkeleton::from_bones(bones)
        }
    };

    // Guardar original si no existe
    {
        let mut orig = state.original_skeleton.lock().unwrap();
        if orig.is_none() {
            *orig = Some(skeleton_type.clone());
        }
    }

    let bones = skel.bones_mut();
    if bone_index >= bones.len() {
        return Err(format!("Índice de hueso fuera de rango: {}", bone_index));
    }

    bones[bone_index].position = Vector3::new(position[0], position[1], position[2]);

    let data = skeleton_to_data(&skel);

    *skeleton_lock = Some(SkeletonType::Custom(skel));

    // Limpiar resultado autorig
    drop(skeleton_lock);
    let mut result_lock = state.result.lock().unwrap();
    *result_lock = None;

    Ok(data)
}

/// Auto-ajusta el esqueleto al bounding box de la malla
#[tauri::command]
pub fn auto_fit_skeleton(state: State<'_, AppState>) -> Result<SkeletonData, String> {
    let scene_lock = state.scene.lock().unwrap();
    let scene = scene_lock.as_ref().ok_or("No hay escena cargada")?;
    let mesh_bbox = calculate_scene_bounds(scene);
    drop(scene_lock);

    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto seleccionado")?;

    // Guardar original si no existe
    {
        let mut orig = state.original_skeleton.lock().unwrap();
        if orig.is_none() {
            *orig = Some(skeleton_type.clone());
        }
    }

    let skel_data = get_skeleton_data_for_type(skeleton_type);
    drop(skeleton_lock);

    // Calcular bounding box del esqueleto
    let mut skel_min = [f64::MAX; 3];
    let mut skel_max = [f64::MIN; 3];
    for bone in &skel_data.bones {
        for i in 0..3 {
            skel_min[i] = skel_min[i].min(bone.position[i]);
            skel_max[i] = skel_max[i].max(bone.position[i]);
        }
    }

    let mesh_size = [
        (mesh_bbox.max[0] - mesh_bbox.min[0]) as f64,
        (mesh_bbox.max[1] - mesh_bbox.min[1]) as f64,
        (mesh_bbox.max[2] - mesh_bbox.min[2]) as f64,
    ];
    let skel_size = [
        skel_max[0] - skel_min[0],
        skel_max[1] - skel_min[1],
        skel_max[2] - skel_min[2],
    ];

    // Escalar para que quepa en ~80% del bbox de la malla
    let mesh_max_dim = mesh_size[0].max(mesh_size[1]).max(mesh_size[2]);
    let skel_max_dim = skel_size[0].max(skel_size[1]).max(skel_size[2]);

    let scale = if skel_max_dim > 1e-6 {
        (mesh_max_dim * 0.8) / skel_max_dim
    } else {
        1.0
    };

    // Centro de la malla
    let mesh_center = [
        ((mesh_bbox.min[0] + mesh_bbox.max[0]) / 2.0) as f64,
        ((mesh_bbox.min[1] + mesh_bbox.max[1]) / 2.0) as f64,
        ((mesh_bbox.min[2] + mesh_bbox.max[2]) / 2.0) as f64,
    ];

    // Crear BasicSkeleton escalado y centrado
    let bones: Vec<Bone> = skel_data.bones.iter().map(|b| {
        let pos = [
            (b.position[0] - (skel_min[0] + skel_max[0]) / 2.0) * scale + mesh_center[0],
            (b.position[1] - skel_min[1]) * scale + mesh_bbox.min[1] as f64,
            (b.position[2] - (skel_min[2] + skel_max[2]) / 2.0) * scale + mesh_center[2],
        ];
        let mut bone = if let Some(parent) = b.parent {
            Bone::with_parent(&b.name, Vector3::new(pos[0], pos[1], pos[2]), parent)
        } else {
            Bone::new(&b.name, Vector3::new(pos[0], pos[1], pos[2]))
        };
        if b.is_leaf {
            bone = bone.as_leaf();
        }
        bone
    }).collect();

    let skel = BasicSkeleton::from_bones(bones);
    let data = skeleton_to_data(&skel);

    let mut skeleton_lock = state.skeleton.lock().unwrap();
    *skeleton_lock = Some(SkeletonType::Custom(skel));

    // Limpiar resultado autorig
    let mut result_lock = state.result.lock().unwrap();
    *result_lock = None;

    Ok(data)
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTORIG COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Ejecuta el autorig con progress reporting
#[tauri::command]
pub async fn run_autorig(
    config: AutorigConfig,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.processing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Ya hay un proceso de autorig en curso".to_string());
    }

    let mesh = {
        let mesh_lock = state.mesh.lock().unwrap();
        mesh_lock.clone().ok_or("No hay malla cargada")?
    };

    let skeleton_type = {
        let skeleton_lock = state.skeleton.lock().unwrap();
        skeleton_lock.clone().ok_or("No hay esqueleto seleccionado")?
    };

    let _ = on_progress.send(Progress {
        stage: "preparing".to_string(),
        percent: 0,
        message: "Preparando datos...".to_string(),
    });

    let pinocchio_config = match config.quality.as_str() {
        "fast" => PinocchioConfig::fast(),
        "high" => PinocchioConfig::high_quality(),
        _ => PinocchioConfig::default(),
    };

    let pinocchio_config = if let Some(dw) = config.diffusion_weight {
        pinocchio_config.with_diffusion_weight(dw)
    } else {
        pinocchio_config
    };

    let pinocchio_config = if let Some(mi) = config.max_influences {
        pinocchio_config.with_max_influences(mi)
    } else {
        pinocchio_config
    };

    let _ = on_progress.send(Progress {
        stage: "embedding".to_string(),
        percent: 20,
        message: "Calculando embedding del esqueleto...".to_string(),
    });

    let result = match &skeleton_type {
        SkeletonType::Human => autorig(&mesh, &HumanSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Quad => autorig(&mesh, &QuadSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Horse => autorig(&mesh, &HorseSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Centaur => autorig(&mesh, &CentaurSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Bird => autorig(&mesh, &BirdSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Spider => autorig(&mesh, &SpiderSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Serpent => autorig(&mesh, &SerpentSkeleton::default(), Some(pinocchio_config)),
        SkeletonType::Mech => autorig(&mesh, &MechSkeleton::new(), Some(pinocchio_config)),
        SkeletonType::Custom(skel) => autorig(&mesh, skel, Some(pinocchio_config)),
    };

    let _ = on_progress.send(Progress {
        stage: "diffusion".to_string(),
        percent: 70,
        message: "Calculando pesos de skinning...".to_string(),
    });

    match result {
        Ok(output) => {
            let _ = on_progress.send(Progress {
                stage: "done".to_string(),
                percent: 100,
                message: format!(
                    "Completado: {} vértices, {} huesos",
                    output.stats.num_vertices, output.stats.num_bones
                ),
            });

            let mut result_lock = state.result.lock().unwrap();
            *result_lock = Some(output);

            state.processing.store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        Err(e) => {
            state.processing.store(false, std::sync::atomic::Ordering::SeqCst);
            Err(format!("Error en autorig: {:?}", e))
        }
    }
}

/// Obtiene los datos de pesos para visualización
#[tauri::command]
pub fn get_weights_data(state: State<'_, AppState>) -> Result<WeightsData, String> {
    let result_lock = state.result.lock().unwrap();
    let result = result_lock.as_ref().ok_or("No hay resultado de autorig")?;

    let skeleton_lock = state.skeleton.lock().unwrap();
    let skeleton_type = skeleton_lock.as_ref().ok_or("No hay esqueleto")?;

    let bone_names = get_bone_names(skeleton_type);
    let num_vertices = result.attachment.num_vertices();
    let num_bones = bone_names.len();
    let max_influences = 4;

    let (indices, weights_raw) = result.export_weights(max_influences);

    let mut weights = Vec::with_capacity(num_vertices * max_influences * 2);
    for vert_idx in 0..num_vertices {
        for i in 0..max_influences {
            weights.push(indices[vert_idx][i] as f64);
            weights.push(weights_raw[vert_idx][i]);
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
        })
    } else {
        Ok(get_skeleton_data_for_type(skeleton_type))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RETOPOLOGY COMMANDS
// ═══════════════════════════════════════════════════════════════════════════

/// Ejecuta la retopología con QuadriFlow
#[tauri::command]
pub async fn run_retopology(
    config: RetopologyConfig,
    on_progress: Channel<Progress>,
    state: State<'_, AppState>,
) -> Result<QuadMeshInfo, String> {
    if state.processing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Ya hay un proceso en curso".to_string());
    }

    // Obtener la malla del estado
    let mesh = {
        let mesh_lock = state.mesh.lock().unwrap();
        mesh_lock.clone().ok_or("No hay malla cargada")?
    };

    let _ = on_progress.send(Progress {
        stage: "preparing".to_string(),
        percent: 0,
        message: "Preparando retopología...".to_string(),
    });

    // Configurar RemeshConfig
    let remesh_config = RemeshConfig {
        target_faces: config.target_quads,
        preserve_sharp: config.preserve_sharp.unwrap_or(false),
        sharp_angle: config.sharp_angle.map(|a| (a as f64).to_radians())
            .unwrap_or(std::f64::consts::FRAC_PI_4),
        adaptive: config.adaptive.unwrap_or(false),
        smooth_iterations: config.smooth_iterations.unwrap_or(10),
        remove_flips: config.remove_flips.unwrap_or(false),
    };

    // Canal para enviar progreso desde el callback
    let progress_sender = on_progress.clone();

    // Ejecutar retopología con callback de progreso
    let result = remesh_with_callback(&mesh, &remesh_config, |stage, message| {
        let _ = progress_sender.send(Progress {
            stage: stage.name().to_string(),
            percent: stage.progress(),
            message: message.to_string(),
        });
    });

    match result {
        Ok(quad_mesh) => {
            let info = QuadMeshInfo {
                num_vertices: quad_mesh.num_vertices(),
                num_quads: quad_mesh.num_faces(),
                bounding_box: calculate_quad_mesh_bounds(&quad_mesh),
            };

            let _ = on_progress.send(Progress {
                stage: "done".to_string(),
                percent: 100,
                message: format!(
                    "Retopología completada: {} vértices, {} quads",
                    info.num_vertices, info.num_quads
                ),
            });

            // Guardar resultado en estado
            let mut quad_mesh_lock = state.quad_mesh.lock().unwrap();
            *quad_mesh_lock = Some(quad_mesh);

            state.processing.store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(info)
        }
        Err(e) => {
            state.processing.store(false, std::sync::atomic::Ordering::SeqCst);
            Err(format!("Error en retopología: {:?}", e))
        }
    }
}

/// Obtiene los datos de la malla de quads para renderizar en Three.js
#[tauri::command]
pub fn get_quad_mesh_data(state: State<'_, AppState>) -> Result<QuadMeshData, String> {
    let quad_mesh_lock = state.quad_mesh.lock().unwrap();
    let quad_mesh = quad_mesh_lock.as_ref().ok_or("No hay malla de quads")?;

    let mut positions: Vec<f32> = Vec::with_capacity(quad_mesh.num_vertices() * 3);
    let mut normals: Vec<f32> = Vec::with_capacity(quad_mesh.num_vertices() * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(quad_mesh.num_faces() * 6); // 2 triángulos por quad
    let mut quad_indices: Vec<u32> = Vec::with_capacity(quad_mesh.num_faces() * 4);

    // Extraer posiciones
    for vertex in &quad_mesh.vertices {
        positions.push(vertex.x as f32);
        positions.push(vertex.y as f32);
        positions.push(vertex.z as f32);
    }

    // Calcular normales por vértice (promedio de normales de caras adyacentes)
    let mut vertex_normals = vec![[0.0f64; 3]; quad_mesh.num_vertices()];
    let mut vertex_counts = vec![0usize; quad_mesh.num_vertices()];

    for face in &quad_mesh.faces {
        // Calcular normal de la cara
        let v0 = &quad_mesh.vertices[face.v[0]];
        let v1 = &quad_mesh.vertices[face.v[1]];
        let v2 = &quad_mesh.vertices[face.v[2]];

        let e1 = [v1.x - v0.x, v1.y - v0.y, v1.z - v0.z];
        let e2 = [v2.x - v0.x, v2.y - v0.y, v2.z - v0.z];
        let normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];

        // Acumular normal en cada vértice
        for &vi in &face.v {
            vertex_normals[vi][0] += normal[0];
            vertex_normals[vi][1] += normal[1];
            vertex_normals[vi][2] += normal[2];
            vertex_counts[vi] += 1;
        }
    }

    // Normalizar
    for (normal, &count) in vertex_normals.iter_mut().zip(vertex_counts.iter()) {
        if count > 0 {
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if len > 1e-10 {
                normal[0] /= len;
                normal[1] /= len;
                normal[2] /= len;
            } else {
                *normal = [0.0, 1.0, 0.0];
            }
        } else {
            *normal = [0.0, 1.0, 0.0];
        }
        normals.push(normal[0] as f32);
        normals.push(normal[1] as f32);
        normals.push(normal[2] as f32);
    }

    // Triangular quads: cada quad [v0, v1, v2, v3] -> [v0, v1, v2] + [v0, v2, v3]
    for face in &quad_mesh.faces {
        // Guardar índices de quad originales
        for &vi in &face.v {
            quad_indices.push(vi as u32);
        }

        // Triangular
        indices.push(face.v[0] as u32);
        indices.push(face.v[1] as u32);
        indices.push(face.v[2] as u32);

        indices.push(face.v[0] as u32);
        indices.push(face.v[2] as u32);
        indices.push(face.v[3] as u32);
    }

    Ok(QuadMeshData {
        positions,
        normals,
        indices,
        quad_indices,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// REPAIR TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct RepairAnalysisConfigInput {
    pub check_non_manifold: Option<bool>,
    pub check_self_intersections: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshDiagnosticsInfo {
    pub boundary_loops: usize,
    pub boundary_edges: usize,
    pub duplicate_vertices: usize,
    pub degenerate_faces: usize,
    pub zero_area_faces: usize,
    pub needle_faces: usize,
    pub cap_faces: usize,
    pub non_manifold_edges: usize,
    pub non_manifold_vertices: usize,
    pub normals_consistent: bool,
    pub connected_components: usize,
    pub is_closed: bool,
    pub self_intersections: usize,
    pub needs_repair: bool,
    pub is_healthy: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepairConfigInput {
    pub merge_duplicates: Option<bool>,
    pub remove_degenerates: Option<bool>,
    pub fix_normals: Option<bool>,
    pub orient_outward: Option<bool>,
    pub fill_holes: Option<bool>,
    pub hole_fill_method: Option<String>,
    pub fix_non_manifold: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairResultInfo {
    pub vertices_merged: usize,
    pub faces_removed: usize,
    pub faces_flipped: usize,
    pub holes_filled: usize,
    pub faces_added: usize,
    pub non_manifold_fixed: usize,
    pub new_mesh_info: MeshInfo,
    pub new_diagnostics: MeshDiagnosticsInfo,
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
pub fn analyze_mesh(
    config: RepairAnalysisConfigInput,
    state: State<'_, AppState>,
) -> Result<MeshDiagnosticsInfo, String> {
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;

    let analysis_config = RepairAnalysisConfig {
        check_non_manifold: config.check_non_manifold.unwrap_or(true),
        check_self_intersections: config.check_self_intersections.unwrap_or(false),
        ..RepairAnalysisConfig::default()
    };

    let diagnostics = pinocchio_repair::analyze(mesh, &analysis_config);
    let info = diagnostics_to_info(&diagnostics);

    let mut diag_lock = state.diagnostics.lock().unwrap();
    *diag_lock = Some(diagnostics);

    Ok(info)
}

/// Repara la malla con la configuración dada
#[tauri::command]
pub fn repair_mesh(
    config: RepairConfigInput,
    state: State<'_, AppState>,
) -> Result<RepairResultInfo, String> {
    // Guardar backups
    {
        let mesh_lock = state.mesh.lock().unwrap();
        let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?.clone();
        let mut backup = state.mesh_before_repair.lock().unwrap();
        *backup = Some(mesh);
    }
    {
        let scene_lock = state.scene.lock().unwrap();
        let scene = scene_lock.as_ref().ok_or("No hay escena cargada")?.clone();
        let mut backup = state.scene_before_repair.lock().unwrap();
        *backup = Some(scene);
    }

    // Clonar mesh y reparar
    let mut mesh = {
        let mesh_lock = state.mesh.lock().unwrap();
        mesh_lock.as_ref().unwrap().clone()
    };

    let hole_method = match config.hole_fill_method.as_deref() {
        Some("liepa") => HoleFillMethod::Liepa,
        _ => HoleFillMethod::EarClipping,
    };

    let repair_config = RepairConfig {
        merge_duplicates: config.merge_duplicates.unwrap_or(true),
        remove_degenerates: config.remove_degenerates.unwrap_or(true),
        fix_normals: config.fix_normals.unwrap_or(true),
        orient_outward: config.orient_outward.unwrap_or(true),
        fill_holes: config.fill_holes.unwrap_or(true),
        hole_fill_config: HoleFillConfig {
            method: hole_method,
            ..HoleFillConfig::default()
        },
        fix_non_manifold: config.fix_non_manifold.unwrap_or(false),
        ..RepairConfig::default()
    };

    let summary = pinocchio_repair::repair_all(&mut mesh, &repair_config)
        .map_err(|e| format!("Error en reparación: {:?}", e))?;

    // Reconstruir Scene desde mesh reparada
    let new_scene = mesh_to_scene(&mesh);

    // Calcular nuevo MeshInfo
    let (num_vertices, num_faces, has_normals, has_uvs) = calculate_scene_stats(&new_scene);
    let bbox = calculate_scene_bounds(&new_scene);
    let new_mesh_info = MeshInfo {
        num_vertices,
        num_faces,
        num_meshes: new_scene.meshes.len(),
        has_normals,
        has_uvs,
        has_materials: !new_scene.materials.is_empty(),
        bounding_box: bbox,
        format: "REPAIRED".to_string(),
    };

    // Guardar mesh y scene reparadas
    {
        let mut mesh_lock = state.mesh.lock().unwrap();
        *mesh_lock = Some(mesh.clone());
    }
    {
        let mut scene_lock = state.scene.lock().unwrap();
        *scene_lock = Some(new_scene);
    }

    // Re-analizar
    let new_diagnostics = pinocchio_repair::analyze(&mesh, &RepairAnalysisConfig::default());
    let diag_info = diagnostics_to_info(&new_diagnostics);

    let mut diag_lock = state.diagnostics.lock().unwrap();
    *diag_lock = Some(new_diagnostics);

    Ok(RepairResultInfo {
        vertices_merged: summary.vertices_merged,
        faces_removed: summary.faces_removed,
        faces_flipped: summary.faces_flipped,
        holes_filled: summary.holes_filled,
        faces_added: summary.faces_added,
        non_manifold_fixed: summary.non_manifold_fixed,
        new_mesh_info,
        new_diagnostics: diag_info,
    })
}

/// Deshace la reparación restaurando backups
#[tauri::command]
pub fn undo_repair(state: State<'_, AppState>) -> Result<MeshInfo, String> {
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
    };

    let mut mesh_lock = state.mesh.lock().unwrap();
    *mesh_lock = Some(backup_mesh);

    let mut scene_lock = state.scene.lock().unwrap();
    *scene_lock = Some(backup_scene);

    let mut diag_lock = state.diagnostics.lock().unwrap();
    *diag_lock = None;

    Ok(info)
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
pub fn analyze_print3d(state: State<'_, AppState>) -> Result<Print3dAnalysisInfo, String> {
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;

    let analysis = pinocchio_print3d::analyze(mesh)
        .map_err(|e| format!("Error en análisis: {:?}", e))?;

    let dims = analysis.bounding_box.dimensions();

    Ok(Print3dAnalysisInfo {
        volume: analysis.volume,
        surface_area: analysis.surface_area,
        center_of_mass: analysis.center_of_mass,
        dimensions: dims,
        is_closed: analysis.is_closed,
        vertex_count: analysis.vertex_count,
        triangle_count: analysis.triangle_count,
        estimated_weight: analysis.estimated_weight,
    })
}

/// Escala la malla para impresión
#[tauri::command]
pub fn scale_mesh_for_print(
    params: ScalePrintInput,
    state: State<'_, AppState>,
) -> Result<Print3dAnalysisInfo, String> {
    // Guardar backup
    {
        let mesh_lock = state.mesh.lock().unwrap();
        let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?.clone();
        let mut backup = state.mesh_before_print_scale.lock().unwrap();
        *backup = Some(mesh);
    }

    let mut mesh = {
        let mesh_lock = state.mesh.lock().unwrap();
        mesh_lock.as_ref().unwrap().clone()
    };

    match params.mode.as_str() {
        "uniform" => {
            let factor = params.factor.ok_or("Falta factor de escala")?;
            pinocchio_print3d::scale(&mut mesh, factor);
        }
        "fit" => {
            let target = params.target_size.ok_or("Falta tamaño objetivo")?;
            pinocchio_print3d::scale_to_fit(&mut mesh, target);
        }
        "volume" => {
            let target_vol = params.target_volume.ok_or("Falta volumen objetivo")?;
            let current_vol = pinocchio_print3d::compute_volume(&mesh);
            pinocchio_print3d::scale_to_volume(&mut mesh, current_vol, target_vol);
        }
        _ => return Err(format!("Modo de escala no soportado: {}", params.mode)),
    }

    // Actualizar mesh en estado y reconstruir scene
    let new_scene = mesh_to_scene(&mesh);
    {
        let mut mesh_lock = state.mesh.lock().unwrap();
        *mesh_lock = Some(mesh.clone());
    }
    {
        let mut scene_lock = state.scene.lock().unwrap();
        *scene_lock = Some(new_scene);
    }

    // Re-analizar
    let analysis = pinocchio_print3d::analyze(&mesh)
        .map_err(|e| format!("Error en análisis: {:?}", e))?;
    let dims = analysis.bounding_box.dimensions();

    Ok(Print3dAnalysisInfo {
        volume: analysis.volume,
        surface_area: analysis.surface_area,
        center_of_mass: analysis.center_of_mass,
        dimensions: dims,
        is_closed: analysis.is_closed,
        vertex_count: analysis.vertex_count,
        triangle_count: analysis.triangle_count,
        estimated_weight: analysis.estimated_weight,
    })
}

/// Subdivide la malla en piezas para impresión
#[tauri::command]
pub fn subdivide_mesh(
    config: SubdivideConfigInput,
    state: State<'_, AppState>,
) -> Result<SubdivideResultInfo, String> {
    let mesh_lock = state.mesh.lock().unwrap();
    let mesh = mesh_lock.as_ref().ok_or("No hay malla cargada")?;

    let strategy = match config.strategy.as_deref() {
        Some("optimal") => SubdivideStrategy::Optimal,
        Some("zlayers") => SubdivideStrategy::ZLayers,
        _ => SubdivideStrategy::Grid,
    };

    let subdivide_config = SubdivideConfig {
        build_volume: config.build_volume,
        max_dimension: None,
        overlap: 0.0,
        strategy,
        margin: config.margin.unwrap_or(2.0),
    };

    let mut pieces = pinocchio_print3d::subdivide(mesh, &subdivide_config)
        .map_err(|e| format!("Error subdividiendo: {:?}", e))?;

    // Calcular vecinos
    pinocchio_print3d::find_neighbors(&mut pieces, 1.0);

    let piece_infos: Vec<PieceInfo> = pieces.iter().enumerate().map(|(i, p)| {
        let bbox = pinocchio_print3d::compute_bounding_box(&p.mesh);
        PieceInfo {
            index: i,
            label: p.label.clone(),
            vertex_count: p.mesh.num_vertices(),
            face_count: p.mesh.num_faces(),
            dimensions: bbox.dimensions(),
        }
    }).collect();

    let piece_count = pieces.len();

    let mut pieces_lock = state.print3d_pieces.lock().unwrap();
    *pieces_lock = Some(pieces);

    Ok(SubdivideResultInfo {
        piece_count,
        pieces: piece_infos,
    })
}

/// Exporta una pieza individual como STL
#[tauri::command]
pub fn export_print3d_piece(
    piece_index: usize,
    path: String,
    state: State<'_, AppState>,
) -> Result<ExportResult, String> {
    let pieces_lock = state.print3d_pieces.lock().unwrap();
    let pieces = pieces_lock.as_ref().ok_or("No hay piezas subdivididas")?;

    if piece_index >= pieces.len() {
        return Err(format!("Índice de pieza fuera de rango: {} (hay {})", piece_index, pieces.len()));
    }

    let piece = &pieces[piece_index];
    let scene = mesh_to_scene(&piece.mesh);

    let path_ref = Path::new(&path);
    converter_stl::export_stl(&scene, path_ref)
        .map_err(|e| format!("Error exportando STL: {:?}", e))?;

    Ok(ExportResult {
        success: true,
        path: path.clone(),
        message: format!("Pieza '{}' exportada como STL", piece.label),
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
        SkeletonType::Custom(skel) => skeleton_to_data(skel),
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

    SkeletonData { bones, edges }
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
        SkeletonType::Custom(skel) => skel.bones().iter().map(|b| b.name.clone()).collect(),
    }
}

fn calculate_scene_stats(scene: &Scene) -> (usize, usize, bool, bool) {
    let mut total_vertices = 0;
    let mut total_faces = 0;
    let mut has_normals = false;
    let mut has_uvs = false;

    for mesh in &scene.meshes {
        for primitive in &mesh.primitives {
            for attr in &primitive.attributes {
                match attr {
                    VertexAttribute::Positions(p) => {
                        total_vertices += p.len();
                    }
                    VertexAttribute::Normals(_) => {
                        has_normals = true;
                    }
                    VertexAttribute::TexCoords(0, _) => {
                        has_uvs = true;
                    }
                    _ => {}
                }
            }
            if let Some(indices) = &primitive.indices {
                let count = match indices {
                    IndexData::U16(v) => v.len(),
                    IndexData::U32(v) => v.len(),
                };
                total_faces += count / 3;
            }
        }
    }

    (total_vertices, total_faces, has_normals, has_uvs)
}

fn calculate_scene_bounds(scene: &Scene) -> BoundingBox {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for mesh in &scene.meshes {
        for primitive in &mesh.primitives {
            let positions = primitive.attributes.iter().find_map(|attr| {
                if let VertexAttribute::Positions(p) = attr {
                    Some(p)
                } else {
                    None
                }
            });

            if let Some(positions) = positions {
                for p in positions {
                    min[0] = min[0].min(p[0]);
                    min[1] = min[1].min(p[1]);
                    min[2] = min[2].min(p[2]);
                    max[0] = max[0].max(p[0]);
                    max[1] = max[1].max(p[1]);
                    max[2] = max[2].max(p[2]);
                }
            }
        }
    }

    if min[0] == f32::MAX {
        min = [0.0, 0.0, 0.0];
        max = [1.0, 1.0, 1.0];
    }

    BoundingBox { min, max }
}

fn scene_to_pinocchio_mesh(scene: &Scene) -> Result<Mesh, String> {
    pinocchio_mesh::scene_to_mesh(scene).ok_or_else(|| "Escena vacía o sin geometría".to_string())
}

fn diagnostics_to_info(d: &pinocchio_repair::MeshDiagnostics) -> MeshDiagnosticsInfo {
    MeshDiagnosticsInfo {
        boundary_loops: d.boundary_loops,
        boundary_edges: d.boundary_edges,
        duplicate_vertices: d.duplicate_vertices,
        degenerate_faces: d.degenerate_faces,
        zero_area_faces: d.zero_area_faces,
        needle_faces: d.needle_faces,
        cap_faces: d.cap_faces,
        non_manifold_edges: d.non_manifold_edges,
        non_manifold_vertices: d.non_manifold_vertices,
        normals_consistent: d.normals_consistent,
        connected_components: d.connected_components,
        is_closed: d.is_closed,
        self_intersections: d.self_intersections,
        needs_repair: d.needs_repair(),
        is_healthy: d.is_healthy(),
    }
}

/// Reconstruye una Scene (converter-scene) a partir de una Mesh de pinocchio.
fn mesh_to_scene(mesh: &Mesh) -> Scene {
    use converter_scene::{Mesh as SceneMesh, Primitive};

    let num_verts = mesh.num_vertices();
    let mut positions = Vec::with_capacity(num_verts);
    for i in 0..num_verts {
        let v = &mesh.vertices[i].position;
        positions.push([v.x() as f32, v.y() as f32, v.z() as f32]);
    }

    let num_faces = mesh.num_faces();
    let mut indices = Vec::with_capacity(num_faces * 3);
    for i in 0..num_faces {
        let f = mesh.get_face_vertices(i);
        indices.push(f[0] as u32);
        indices.push(f[1] as u32);
        indices.push(f[2] as u32);
    }

    let primitive = Primitive {
        attributes: vec![VertexAttribute::Positions(positions)],
        indices: Some(IndexData::U32(indices)),
        material: None,
    };

    let scene_mesh = SceneMesh {
        name: "repaired".to_string(),
        primitives: vec![primitive],
    };

    Scene {
        meshes: vec![scene_mesh],
        ..Scene::default()
    }
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
