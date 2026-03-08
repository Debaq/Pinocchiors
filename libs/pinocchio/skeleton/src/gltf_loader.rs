//! Carga de esqueletos desde converter-scene (glTF/GLB)
//!
//! Este módulo está disponible solo con el feature `converter`.

use crate::{BasicSkeleton, Bone};
use converter_scene::Skeleton as SceneSkeleton;
use pinocchio_math::Vector3;

/// Convierte un esqueleto de converter-scene a BasicSkeleton
///
/// # Arguments
/// * `skel` - Esqueleto de converter-scene
///
/// # Returns
/// BasicSkeleton con la jerarquía convertida
pub fn from_scene_skeleton(skel: &SceneSkeleton) -> BasicSkeleton {
    let mut bones = Vec::with_capacity(skel.joints.len());

    // Construir mapa de índice a padre
    let mut parent_map: Vec<Option<usize>> = vec![None; skel.joints.len()];
    for (parent_idx, joint) in skel.joints.iter().enumerate() {
        for &child_idx in &joint.children {
            if child_idx < parent_map.len() {
                parent_map[child_idx] = Some(parent_idx);
            }
        }
    }

    for (idx, joint) in skel.joints.iter().enumerate() {
        // Extraer posición de la matriz local
        let pos = extract_position(&joint.local_transform);

        let bone = match parent_map[idx] {
            Some(parent_idx) => Bone::with_parent(&joint.name, pos, parent_idx),
            None => Bone::new(&joint.name, pos),
        };

        // Marcar como leaf si no tiene hijos
        let bone = if joint.children.is_empty() {
            bone.as_leaf()
        } else {
            bone
        };

        bones.push(bone);
    }

    BasicSkeleton::from_bones(bones)
}

/// Extrae la posición (translación) de una matriz 4x4
fn extract_position(mat: &glam::Mat4) -> Vector3 {
    let (_, _, translation) = mat.to_scale_rotation_translation();
    Vector3::new(
        translation.x as f64,
        translation.y as f64,
        translation.z as f64,
    )
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::Skeleton;
    use glam::Mat4;

    fn make_test_scene_skeleton() -> SceneSkeleton {
        use converter_scene::Joint;

        SceneSkeleton {
            name: "test".to_string(),
            joints: vec![
                Joint {
                    name: "root".to_string(),
                    children: vec![1, 2],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(glam::Vec3::new(0.0, 0.5, 0.0)),
                    node_index: None,
                },
                Joint {
                    name: "left".to_string(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(glam::Vec3::new(-0.2, 0.8, 0.0)),
                    node_index: None,
                },
                Joint {
                    name: "right".to_string(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(glam::Vec3::new(0.2, 0.8, 0.0)),
                    node_index: None,
                },
            ],
            roots: vec![0],
        }
    }

    #[test]
    fn test_from_scene_skeleton() {
        let scene_skel = make_test_scene_skeleton();
        let basic_skel = from_scene_skeleton(&scene_skel);

        assert_eq!(basic_skel.num_bones(), 3);

        // Root no tiene padre
        assert!(basic_skel.get_bone(0).unwrap().parent.is_none());

        // Left y right tienen root como padre
        assert_eq!(basic_skel.get_bone(1).unwrap().parent, Some(0));
        assert_eq!(basic_skel.get_bone(2).unwrap().parent, Some(0));

        // Left y right son leaves
        assert!(basic_skel.get_bone(1).unwrap().is_leaf);
        assert!(basic_skel.get_bone(2).unwrap().is_leaf);
    }

    #[test]
    fn test_extract_position() {
        let mat = Mat4::from_translation(glam::Vec3::new(1.0, 2.0, 3.0));
        let pos = extract_position(&mat);

        assert!((pos.x() - 1.0).abs() < 1e-6);
        assert!((pos.y() - 2.0).abs() < 1e-6);
        assert!((pos.z() - 3.0).abs() < 1e-6);
    }
}
