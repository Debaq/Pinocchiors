//! Carga y guardado de esqueletos en formato JSON

use crate::{BasicSkeleton, Bone};
use pinocchio_math::Vector3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error al cargar un esqueleto desde JSON
#[derive(Debug, Error)]
pub enum JsonLoadError {
    #[error("Error de parseo JSON: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Hueso padre no encontrado: {parent} para hueso {bone}")]
    ParentNotFound { bone: String, parent: String },
    #[error("No hay huesos definidos")]
    EmptyBones,
}

/// Representación JSON de un hueso
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneJson {
    /// Nombre del hueso
    pub name: String,
    /// Posición [x, y, z]
    pub position: [f64; 3],
    /// Nombre del hueso padre (None para raíz)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Si es un hueso terminal (leaf)
    #[serde(default)]
    pub is_leaf: bool,
}

/// Metadatos del esqueleto
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkeletonMetadata {
    /// Pares simétricos de huesos (ej: [["arm_l", "arm_r"]])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symmetric_pairs: Vec<[String; 2]>,
    /// Descripción opcional
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Autor opcional
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// Representación JSON completa de un esqueleto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletonJson {
    /// Nombre del esqueleto
    pub name: String,
    /// Lista de huesos
    pub bones: Vec<BoneJson>,
    /// Metadatos opcionales
    #[serde(default)]
    pub metadata: SkeletonMetadata,
}

/// Carga un esqueleto desde una cadena JSON
pub fn load_skeleton_json(json: &str) -> Result<BasicSkeleton, JsonLoadError> {
    let skel_json: SkeletonJson = serde_json::from_str(json)?;

    if skel_json.bones.is_empty() {
        return Err(JsonLoadError::EmptyBones);
    }

    // Crear mapa de nombres a índices
    let name_to_idx: std::collections::HashMap<&str, usize> = skel_json
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect();

    let mut bones = Vec::with_capacity(skel_json.bones.len());

    for bone_json in &skel_json.bones {
        let position = Vector3::new(
            bone_json.position[0],
            bone_json.position[1],
            bone_json.position[2],
        );

        let parent = match &bone_json.parent {
            Some(parent_name) => {
                let idx = name_to_idx.get(parent_name.as_str()).ok_or_else(|| {
                    JsonLoadError::ParentNotFound {
                        bone: bone_json.name.clone(),
                        parent: parent_name.clone(),
                    }
                })?;
                Some(*idx)
            }
            None => None,
        };

        let mut bone = Bone {
            name: bone_json.name.clone(),
            position,
            parent,
            is_leaf: bone_json.is_leaf,
        };

        if bone_json.is_leaf {
            bone = bone.as_leaf();
        }

        bones.push(bone);
    }

    Ok(BasicSkeleton::from_bones(bones))
}

/// Guarda un esqueleto a formato JSON
pub fn save_skeleton_json<S: crate::Skeleton>(skeleton: &S, name: &str) -> String {
    let bones: Vec<BoneJson> = skeleton
        .bones()
        .iter()
        .map(|bone| {
            let parent = bone.parent.map(|idx| {
                skeleton
                    .get_bone(idx)
                    .map(|b| b.name.clone())
                    .unwrap_or_else(|| format!("bone_{}", idx))
            });

            BoneJson {
                name: bone.name.clone(),
                position: [bone.position.x(), bone.position.y(), bone.position.z()],
                parent,
                is_leaf: bone.is_leaf,
            }
        })
        .collect();

    let skel_json = SkeletonJson {
        name: name.to_string(),
        bones,
        metadata: SkeletonMetadata::default(),
    };

    serde_json::to_string_pretty(&skel_json).unwrap_or_else(|_| "{}".to_string())
}

/// Carga un esqueleto desde un archivo JSON
pub fn load_skeleton_from_file(path: &std::path::Path) -> Result<BasicSkeleton, JsonLoadError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| JsonLoadError::ParseError(serde_json::Error::io(e)))?;
    load_skeleton_json(&content)
}

/// Guarda un esqueleto a un archivo JSON
pub fn save_skeleton_to_file<S: crate::Skeleton>(
    skeleton: &S,
    name: &str,
    path: &std::path::Path,
) -> std::io::Result<()> {
    let json = save_skeleton_json(skeleton, name);
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::HumanSkeleton;
    use crate::Skeleton;

    #[test]
    fn test_load_simple_skeleton() {
        let json = r#"
        {
            "name": "Simple",
            "bones": [
                {"name": "root", "position": [0, 0.5, 0]},
                {"name": "spine", "position": [0, 0.7, 0], "parent": "root"},
                {"name": "head", "position": [0, 1.0, 0], "parent": "spine", "is_leaf": true}
            ],
            "metadata": {}
        }
        "#;

        let skel = load_skeleton_json(json).unwrap();
        assert_eq!(skel.num_bones(), 3);
        assert_eq!(skel.get_bone(0).unwrap().name, "root");
        assert!(skel.get_bone(2).unwrap().is_leaf);
    }

    #[test]
    fn test_roundtrip() {
        let original = HumanSkeleton::new();
        let json = save_skeleton_json(&original, "Human");
        let loaded = load_skeleton_json(&json).unwrap();

        assert_eq!(original.num_bones(), loaded.num_bones());

        for i in 0..original.num_bones() {
            let orig_bone = original.get_bone(i).unwrap();
            let load_bone = loaded.get_bone(i).unwrap();
            assert_eq!(orig_bone.name, load_bone.name);
            assert!((orig_bone.position.x() - load_bone.position.x()).abs() < 1e-10);
            assert!((orig_bone.position.y() - load_bone.position.y()).abs() < 1e-10);
            assert!((orig_bone.position.z() - load_bone.position.z()).abs() < 1e-10);
        }
    }

    #[test]
    fn test_parent_not_found() {
        let json = r#"
        {
            "name": "Bad",
            "bones": [
                {"name": "root", "position": [0, 0, 0]},
                {"name": "child", "position": [0, 1, 0], "parent": "nonexistent"}
            ]
        }
        "#;

        let result = load_skeleton_json(json);
        assert!(matches!(result, Err(JsonLoadError::ParentNotFound { .. })));
    }

    #[test]
    fn test_empty_bones() {
        let json = r#"{"name": "Empty", "bones": []}"#;
        let result = load_skeleton_json(json);
        assert!(matches!(result, Err(JsonLoadError::EmptyBones)));
    }

    #[test]
    fn test_metadata() {
        let json = r#"
        {
            "name": "WithMeta",
            "bones": [{"name": "root", "position": [0, 0, 0]}],
            "metadata": {
                "symmetric_pairs": [["arm_l", "arm_r"]],
                "description": "Test skeleton",
                "author": "Test"
            }
        }
        "#;

        let skel_json: SkeletonJson = serde_json::from_str(json).unwrap();
        assert_eq!(skel_json.metadata.symmetric_pairs.len(), 1);
        assert_eq!(skel_json.metadata.description, Some("Test skeleton".to_string()));
    }
}
