//! Estructura del archivo de origen: nodos, mallas, materiales, texturas,
//! esqueletos y animaciones de la escena importada, y los materiales con sus
//! texturas para que el visor muestre el modelo como viene.

use crate::state::AppState;
use converter_scene::{AlphaMode, KeyframeValues, Scene, TextureFormat, Transform, VertexAttribute};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};

/// Nodo del grafo de escena
#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub children: Vec<usize>,
    pub translation: [f32; 3],
    /// Cuaternión (x, y, z, w)
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// Primitiva de una malla
#[derive(Debug, Clone, Serialize)]
pub struct PrimitiveInfo {
    pub vertices: usize,
    pub triangles: usize,
    pub material: Option<usize>,
    /// Atributos presentes, con nombres de glTF (POSITION, NORMAL, TEXCOORD_0…)
    pub attributes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshInfo {
    pub name: String,
    pub primitives: Vec<PrimitiveInfo>,
}

/// Material PBR (metal/rugosidad, como glTF)
#[derive(Debug, Clone, Serialize)]
pub struct MaterialInfo {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub normal_scale: f32,
    pub occlusion_strength: f32,
    /// "opaque", "mask" o "blend"
    pub alpha_mode: String,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub unlit: bool,
    /// Índices en `textures` de cada canal
    pub base_color_texture: Option<usize>,
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub occlusion_texture: Option<usize>,
    pub emissive_texture: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextureInfo {
    pub name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkeletonInfo {
    pub name: String,
    pub joints: Vec<String>,
    pub roots: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnimationInfo {
    pub name: String,
    pub channels: usize,
    /// Nodos animados
    pub nodes: usize,
    /// Duración en segundos
    pub duration: f32,
    /// Canales por propiedad: traslación, rotación, escala, pesos de morph
    pub translation: usize,
    pub rotation: usize,
    pub scale: usize,
    pub weights: usize,
}

/// Todo lo que trae el archivo de origen
#[derive(Debug, Clone, Serialize)]
pub struct SceneStructure {
    pub meters_per_unit: f64,
    pub y_up: bool,
    pub roots: Vec<usize>,
    pub nodes: Vec<NodeInfo>,
    pub meshes: Vec<MeshInfo>,
    pub materials: Vec<MaterialInfo>,
    pub textures: Vec<TextureInfo>,
    pub skeletons: Vec<SkeletonInfo>,
    pub animations: Vec<AnimationInfo>,
}

fn attribute_name(attribute: &VertexAttribute) -> String {
    match attribute {
        VertexAttribute::Positions(_) => "POSITION".into(),
        VertexAttribute::Normals(_) => "NORMAL".into(),
        VertexAttribute::Tangents(_) => "TANGENT".into(),
        VertexAttribute::TexCoords(set, _) => format!("TEXCOORD_{set}"),
        VertexAttribute::Colors(_) => "COLOR_0".into(),
        VertexAttribute::JointIndices(_) => "JOINTS_0".into(),
        VertexAttribute::JointWeights(_) => "WEIGHTS_0".into(),
    }
}

fn vertex_count(attributes: &[VertexAttribute]) -> usize {
    attributes
        .iter()
        .find_map(|a| match a {
            VertexAttribute::Positions(p) => Some(p.len()),
            _ => None,
        })
        .unwrap_or(0)
}

fn material_info(m: &converter_scene::Material) -> MaterialInfo {
    let (alpha_mode, alpha_cutoff) = match m.alpha_mode {
        AlphaMode::Opaque => ("opaque", 0.5),
        AlphaMode::Mask(cutoff) => ("mask", cutoff),
        AlphaMode::Blend => ("blend", 0.5),
    };
    let texture = |t: &Option<converter_scene::TextureRef>| t.as_ref().map(|t| t.texture_index);
    MaterialInfo {
        name: m.name.clone(),
        base_color: m.base_color_factor,
        metallic: m.metallic_factor,
        roughness: m.roughness_factor,
        emissive: m.emissive_factor,
        normal_scale: m.normal_scale,
        occlusion_strength: m.occlusion_strength,
        alpha_mode: alpha_mode.into(),
        alpha_cutoff,
        double_sided: m.double_sided,
        unlit: m.unlit,
        base_color_texture: texture(&m.base_color_texture),
        metallic_roughness_texture: texture(&m.metallic_roughness_texture),
        normal_texture: texture(&m.normal_texture),
        occlusion_texture: texture(&m.occlusion_texture),
        emissive_texture: texture(&m.emissive_texture),
    }
}

/// Estructura de una escena
pub fn scene_structure(scene: &Scene) -> SceneStructure {
    let nodes = scene
        .nodes
        .iter()
        .map(|n| {
            let (t, r, s) = match n.transform {
                Transform::Trs { translation, rotation, scale } => (translation, rotation, scale),
                Transform::Matrix(m) => {
                    let (s, r, t) = m.to_scale_rotation_translation();
                    (t, r, s)
                }
            };
            NodeInfo {
                name: n.name.clone(),
                mesh: n.mesh,
                skin: n.skin,
                children: n.children.clone(),
                translation: t.to_array(),
                rotation: r.to_array(),
                scale: s.to_array(),
            }
        })
        .collect();
    let meshes = scene
        .meshes
        .iter()
        .map(|m| MeshInfo {
            name: m.name.clone(),
            primitives: m
                .primitives
                .iter()
                .map(|p| {
                    let vertices = vertex_count(&p.attributes);
                    let indices = match &p.indices {
                        Some(converter_scene::IndexData::U16(i)) => i.len(),
                        Some(converter_scene::IndexData::U32(i)) => i.len(),
                        None => vertices,
                    };
                    PrimitiveInfo {
                        vertices,
                        triangles: indices / 3,
                        material: p.material,
                        attributes: p.attributes.iter().map(attribute_name).collect(),
                    }
                })
                .collect(),
        })
        .collect();
    let textures = scene
        .textures
        .iter()
        .map(|t| TextureInfo {
            name: t.name.clone(),
            format: match t.format {
                TextureFormat::Png => "PNG",
                TextureFormat::Jpeg => "JPEG",
                TextureFormat::WebP => "WebP",
            }
            .into(),
            width: t.width,
            height: t.height,
            bytes: t.data.len(),
        })
        .collect();
    let skeletons = scene
        .skeletons
        .iter()
        .map(|s| SkeletonInfo {
            name: s.name.clone(),
            joints: s.joints.iter().map(|j| j.name.clone()).collect(),
            roots: s.roots.clone(),
        })
        .collect();
    let animations = scene
        .animations
        .iter()
        .map(|a| {
            let mut nodes: Vec<usize> = a.channels.iter().map(|c| c.node).collect();
            nodes.sort_unstable();
            nodes.dedup();
            let count = |f: fn(&KeyframeValues) -> bool| a.channels.iter().filter(|c| f(&c.values)).count();
            AnimationInfo {
                name: a.name.clone(),
                channels: a.channels.len(),
                nodes: nodes.len(),
                duration: a.channels.iter().flat_map(|c| c.times.iter().copied()).fold(0.0, f32::max),
                translation: count(|v| matches!(v, KeyframeValues::Translation(_))),
                rotation: count(|v| matches!(v, KeyframeValues::Rotation(_))),
                scale: count(|v| matches!(v, KeyframeValues::Scale(_))),
                weights: count(|v| matches!(v, KeyframeValues::Weights(_))),
            }
        })
        .collect();
    SceneStructure {
        meters_per_unit: scene.meters_per_unit,
        y_up: scene.y_up,
        roots: if scene.root_nodes.is_empty() {
            (0..scene.nodes.len()).filter(|i| !scene.nodes.iter().any(|n| n.children.contains(i))).collect()
        } else {
            scene.root_nodes.clone()
        },
        nodes,
        meshes,
        materials: scene.materials.iter().map(material_info).collect(),
        textures,
        skeletons,
        animations,
    }
}

/// Estructura del archivo importado
#[tauri::command]
pub fn get_scene_structure(state: State<'_, AppState>) -> Result<SceneStructure, String> {
    let scene = state.scene.lock().unwrap();
    Ok(scene_structure(scene.as_ref().ok_or("No hay escena cargada")?))
}

/// Materiales de la escena (el índice es el de los grupos de `get_mesh_data`)
#[tauri::command]
pub fn get_scene_materials(state: State<'_, AppState>) -> Result<Vec<MaterialInfo>, String> {
    let scene = state.scene.lock().unwrap();
    Ok(scene.as_ref().ok_or("No hay escena cargada")?.materials.iter().map(material_info).collect())
}

/// Imagen de una textura de la escena, tal cual (PNG, JPEG o WebP)
#[tauri::command]
pub async fn get_scene_texture(app: AppHandle, index: usize) -> Result<Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let scene = state.scene.lock().unwrap();
        let scene = scene.as_ref().ok_or("No hay escena cargada")?;
        scene
            .textures
            .get(index)
            .map(|t| t.data.clone())
            .ok_or_else(|| format!("No existe la textura {index}"))
    })
    .await
    .map_err(|e| format!("La tarea terminó inesperadamente: {e}"))??;
    Ok(Response::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{Animation, Channel, Interpolation, Material, Mesh, Node, Primitive, Texture, TextureRef};

    #[test]
    fn structure_lists_everything_in_the_file() {
        let scene = Scene {
            meshes: vec![Mesh {
                name: "cuerpo".into(),
                primitives: vec![Primitive {
                    attributes: vec![
                        VertexAttribute::Positions(vec![[0.0; 3]; 4]),
                        VertexAttribute::TexCoords(0, vec![[0.0; 2]; 4]),
                    ],
                    indices: Some(converter_scene::IndexData::U16(vec![0, 1, 2, 0, 2, 3])),
                    material: Some(0),
                }],
            }],
            nodes: vec![Node { name: "raiz".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] }],
            materials: vec![Material {
                name: "piel".into(),
                base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
                alpha_mode: AlphaMode::Mask(0.3),
                ..Default::default()
            }],
            textures: vec![Texture { name: "albedo".into(), data: vec![1, 2, 3], format: TextureFormat::Png, width: 8, height: 4 }],
            animations: vec![Animation {
                name: "caminar".into(),
                channels: vec![Channel {
                    node: 0,
                    interpolation: Interpolation::Linear,
                    times: vec![0.0, 1.5],
                    values: KeyframeValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0]; 2]),
                }],
            }],
            ..Scene::default()
        };
        let s = scene_structure(&scene);
        assert_eq!(s.roots, vec![0], "sin raíces declaradas, los nodos sin padre");
        assert_eq!(s.meshes[0].primitives[0].triangles, 2);
        assert_eq!(s.meshes[0].primitives[0].attributes, vec!["POSITION", "TEXCOORD_0"]);
        assert_eq!(s.materials[0].base_color_texture, Some(0));
        assert_eq!((s.materials[0].alpha_mode.as_str(), s.materials[0].alpha_cutoff), ("mask", 0.3));
        assert_eq!((s.textures[0].width, s.textures[0].bytes), (8, 3));
        assert_eq!((s.animations[0].duration, s.animations[0].rotation), (1.5, 1));
    }
}
