use converter_scene::{IndexData, KeyframeValues, Scene, Transform, VertexAttribute};
use glam::{Mat3, Mat4, Vec3};

/// Escala la geometría, transforms de nodos, animaciones e inverse bind matrices.
pub(crate) fn apply_scale(scene: &mut Scene, factor: f64) {
    let f = factor as f32;

    // Escalar posiciones de vértices
    for mesh in &mut scene.meshes {
        for prim in &mut mesh.primitives {
            for attr in &mut prim.attributes {
                if let VertexAttribute::Positions(positions) = attr {
                    for p in positions.iter_mut() {
                        p[0] *= f;
                        p[1] *= f;
                        p[2] *= f;
                    }
                }
            }
        }
    }

    // Escalar translations de nodos
    for node in &mut scene.nodes {
        match &mut node.transform {
            Transform::Trs { translation, .. } => {
                *translation *= f;
            }
            Transform::Matrix(m) => {
                let (s, r, t) = m.to_scale_rotation_translation();
                *m = Mat4::from_scale_rotation_translation(s, r, t * f);
            }
        }
    }

    // Escalar translations en keyframes de animación
    for anim in &mut scene.animations {
        for ch in &mut anim.channels {
            if let KeyframeValues::Translation(vals) = &mut ch.values {
                for v in vals.iter_mut() {
                    v[0] *= f;
                    v[1] *= f;
                    v[2] *= f;
                }
            }
        }
    }

    // Escalar inverse bind matrices (columna de translación)
    for skeleton in &mut scene.skeletons {
        for joint in &mut skeleton.joints {
            let cols = joint.inverse_bind_matrix.to_cols_array_2d();
            let mut new_cols = cols;
            // Columna 3 (translación) → escalar
            new_cols[3][0] *= f;
            new_cols[3][1] *= f;
            new_cols[3][2] *= f;
            joint.inverse_bind_matrix = Mat4::from_cols_array_2d(&new_cols);

            // local_transform también
            let (s, r, t) = joint.local_transform.to_scale_rotation_translation();
            joint.local_transform = Mat4::from_scale_rotation_translation(s, r, t * f);
        }
    }
}

/// Aplana los transforms de nodos bakeándolos en la geometría.
/// Saltea si la escena tiene skeletons (romperían el skinning).
pub(crate) fn flatten_node_transforms(scene: &mut Scene) {
    if !scene.skeletons.is_empty() {
        return;
    }

    // Acumular transforms globales
    let mut global_transforms = vec![Mat4::IDENTITY; scene.nodes.len()];
    compute_global_transforms(scene, &mut global_transforms);

    // Aplicar transform a la geometría de cada nodo
    for (node_idx, node) in scene.nodes.iter_mut().enumerate() {
        let global = global_transforms[node_idx];
        if global == Mat4::IDENTITY {
            continue;
        }

        if let Some(mesh_idx) = node.mesh {
            if let Some(mesh) = scene.meshes.get_mut(mesh_idx) {
                for prim in &mut mesh.primitives {
                    transform_primitive(prim, &global);
                }
            }
        }

        // Resetear transform a identidad
        node.transform = Transform::identity();
    }
}

fn compute_global_transforms(scene: &Scene, out: &mut [Mat4]) {
    for &root in &scene.root_nodes {
        compute_global_recursive(scene, root, Mat4::IDENTITY, out);
    }
}

fn compute_global_recursive(scene: &Scene, node_idx: usize, parent: Mat4, out: &mut [Mat4]) {
    let node = &scene.nodes[node_idx];
    let local = node.transform.to_matrix();
    let global = parent * local;
    out[node_idx] = global;

    for &child in &node.children {
        compute_global_recursive(scene, child, global, out);
    }
}

fn transform_primitive(prim: &mut Primitive, matrix: &Mat4) {
    let normal_matrix = Mat3::from_mat4(*matrix);
    let has_non_uniform = {
        let (s, _, _) = matrix.to_scale_rotation_translation();
        (s.x - s.y).abs() > 1e-5 || (s.y - s.z).abs() > 1e-5
    };

    // Para normales: usar inverse transpose si hay escala no uniforme
    let normal_mat = if has_non_uniform {
        normal_matrix.inverse().transpose()
    } else {
        normal_matrix
    };

    for attr in &mut prim.attributes {
        match attr {
            VertexAttribute::Positions(positions) => {
                for p in positions.iter_mut() {
                    let v = matrix.transform_point3(Vec3::from_array(*p));
                    *p = v.to_array();
                }
            }
            VertexAttribute::Normals(normals) => {
                for n in normals.iter_mut() {
                    let v = normal_mat * Vec3::from_array(*n);
                    let normalized = v.normalize_or_zero();
                    *n = normalized.to_array();
                }
            }
            VertexAttribute::Tangents(tangents) => {
                for t in tangents.iter_mut() {
                    let v = normal_mat * Vec3::new(t[0], t[1], t[2]);
                    let normalized = v.normalize_or_zero();
                    t[0] = normalized.x;
                    t[1] = normalized.y;
                    t[2] = normalized.z;
                    // w (handedness) se mantiene
                }
            }
            _ => {}
        }
    }
}

use converter_scene::Primitive;

/// Genera normales para primitivas que tienen posiciones e índices pero no normales.
pub(crate) fn generate_missing_normals(scene: &mut Scene) {
    for mesh in &mut scene.meshes {
        for prim in &mut mesh.primitives {
            let has_normals = prim
                .attributes
                .iter()
                .any(|a| matches!(a, VertexAttribute::Normals(_)));

            if has_normals {
                continue;
            }

            let positions = prim.attributes.iter().find_map(|a| match a {
                VertexAttribute::Positions(p) => Some(p.clone()),
                _ => None,
            });

            let positions = match positions {
                Some(p) => p,
                None => continue,
            };

            let indices = match &prim.indices {
                Some(idx) => idx,
                None => continue,
            };

            let normals = compute_vertex_normals(&positions, indices);
            prim.attributes.push(VertexAttribute::Normals(normals));
        }
    }
}

/// Calcula normales por vértice como promedio ponderado por área de caras.
fn compute_vertex_normals(positions: &[[f32; 3]], indices: &IndexData) -> Vec<[f32; 3]> {
    let n = positions.len();
    let mut normals = vec![[0.0f32; 3]; n];

    let idx_iter: Vec<u32> = match indices {
        IndexData::U16(idx) => idx.iter().map(|&i| i as u32).collect(),
        IndexData::U32(idx) => idx.clone(),
    };

    for tri in idx_iter.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }

        let v0 = Vec3::from_array(positions[i0]);
        let v1 = Vec3::from_array(positions[i1]);
        let v2 = Vec3::from_array(positions[i2]);

        let e1 = v1 - v0;
        let e2 = v2 - v0;
        let face_normal = e1.cross(e2); // magnitud = 2*área → ponderación natural

        for &idx in &[i0, i1, i2] {
            normals[idx][0] += face_normal.x;
            normals[idx][1] += face_normal.y;
            normals[idx][2] += face_normal.z;
        }
    }

    // Normalizar
    for n in &mut normals {
        let v = Vec3::from_array(*n);
        let normalized = v.normalize_or_zero();
        if normalized == Vec3::ZERO {
            *n = [0.0, 1.0, 0.0]; // fallback
        } else {
            *n = normalized.to_array();
        }
    }

    normals
}
