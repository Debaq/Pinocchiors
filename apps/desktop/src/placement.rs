//! Orientación del modelo: piso, frente, origen.
//!
//! El frontend compone la transformación del modelo (rotación, traslación,
//! escala uniforme y quizás un espejo) y el backend la aplica a todo lo que
//! depende de la posición: escena, malla, quads, esqueleto colocado, rig y
//! respaldos. Al conservar las formas (sin cizalla ni escala dispareja) no se
//! descarta nada, a diferencia de reparar.

use crate::commands::{in_background, wrap_scene};
use crate::state::{AppState, SkeletonTransformParams, SkeletonType};
use pinocchio_math::{Matrix3, Transform, Vector3};
use pinocchio_mesh::placement;
use pinocchio_mesh::Mesh;
use serde::Serialize;
use tauri::AppHandle;

/// Nodo raíz de la escena que lleva la orientación (se compone, no se anida)
const PLACEMENT_NODE: &str = "placement";

/// Planos candidatos que se muestran
const MAX_CANDIDATES: usize = 24;

#[derive(Debug, Clone, Serialize)]
pub struct FloorCandidateInfo {
    /// Normal hacia afuera: al apoyar el modelo apunta hacia abajo
    pub normal: [f64; 3],
    pub polygon: Vec<[f64; 3]>,
    pub area: f64,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementInfo {
    pub candidates: Vec<FloorCandidateInfo>,
    pub center_of_mass: [f64; 3],
}

/// Planos sobre los que puede apoyarse la malla y su centro de masa
#[tauri::command]
pub async fn get_placement_info(app: AppHandle) -> Result<PlacementInfo, String> {
    in_background(app, placement_info).await
}

fn placement_info(state: &AppState) -> Result<PlacementInfo, String> {
    let mesh = state.mesh.lock().unwrap();
    let mesh = mesh.as_ref().ok_or("No hay malla cargada")?;
    let positions: Vec<Vector3> = mesh.vertices.iter().map(|v| v.position).collect();
    let triangles: Vec<[usize; 3]> = (0..mesh.num_faces()).map(|f| mesh.get_face_vertices(f)).collect();
    let com = placement::center_of_mass(&positions, &triangles);
    let array = |v: Vector3| [v.x(), v.y(), v.z()];
    let candidates = placement::floor_candidates(&positions, com, MAX_CANDIDATES)
        .into_iter()
        .map(|c| FloorCandidateInfo {
            normal: array(c.normal),
            polygon: c.polygon.into_iter().map(array).collect(),
            area: c.area,
            stable: c.stable,
        })
        .collect();
    Ok(PlacementInfo { candidates, center_of_mass: array(com) })
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementResult {
    /// El esqueleto colocado se transformó y su gizmo volvió a cero
    pub skeleton_reset: bool,
}

/// Aplica una transformación rígida (matriz 4×4 por columnas, como Three.js)
#[tauri::command]
pub async fn apply_placement(app: AppHandle, matrix: [f64; 16]) -> Result<PlacementResult, String> {
    in_background(app, move |state| apply_placement_impl(state, matrix)).await
}

/// Rotación (quizás con espejo), escala uniforme y traslación: `p' = s·R·p + t`
#[derive(Debug, Clone, Copy)]
struct Rigid {
    /// Por filas, ortonormal
    rotation: [[f64; 3]; 3],
    /// Escala uniforme (1 = sin escala)
    scale: f64,
    translation: [f64; 3],
    /// Determinante negativo: invierte la orientación de las caras
    mirrored: bool,
}

impl Rigid {
    fn from_column_major(m: [f64; 16]) -> Result<Self, String> {
        if m.iter().any(|x| !x.is_finite()) {
            return Err("La matriz tiene valores no finitos".into());
        }
        if m[3].abs() > 1e-9 || m[7].abs() > 1e-9 || m[11].abs() > 1e-9 || (m[15] - 1.0).abs() > 1e-9 {
            return Err("La matriz no es afín".into());
        }
        let linear = [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]];
        // Columnas ortogonales y del mismo largo: escala uniforme, sin cizalla
        let scale = (0..3).map(|k| linear[k][0] * linear[k][0]).sum::<f64>().sqrt();
        if !(scale > 1e-9) {
            return Err("La escala es cero".into());
        }
        let rotation = linear.map(|row| row.map(|x| x / scale));
        for i in 0..3 {
            for j in 0..3 {
                let dot: f64 = (0..3).map(|k| rotation[k][i] * rotation[k][j]).sum();
                if (dot - if i == j { 1.0 } else { 0.0 }).abs() > 1e-6 {
                    return Err("La transformación deforma el modelo (escala dispareja o cizalla)".into());
                }
            }
        }
        let r = rotation;
        let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1]) - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
        Ok(Self { rotation, scale, translation: [m[12], m[13], m[14]], mirrored: det < 0.0 })
    }

    fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let (r, s) = (&self.rotation, self.scale);
        std::array::from_fn(|i| s * (r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2]) + self.translation[i])
    }

    fn point(&self, p: Vector3) -> Vector3 {
        let [x, y, z] = self.apply([p.x(), p.y(), p.z()]);
        Vector3::new(x, y, z)
    }

    fn transform(&self) -> Transform {
        let r = self.rotation.map(|row| row.map(|x| x * self.scale));
        let [x, y, z] = self.translation;
        Transform::new(
            Matrix3::new(r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2]),
            Vector3::new(x, y, z),
        )
    }

    fn glam(&self) -> converter_scene::glam::Mat4 {
        let r = self.rotation.map(|row| row.map(|x| x * self.scale));
        let t = &self.translation;
        converter_scene::glam::Mat4::from_cols_array(&[
            r[0][0] as f32, r[1][0] as f32, r[2][0] as f32, 0.0,
            r[0][1] as f32, r[1][1] as f32, r[2][1] as f32, 0.0,
            r[0][2] as f32, r[1][2] as f32, r[2][2] as f32, 0.0,
            t[0] as f32, t[1] as f32, t[2] as f32, 1.0,
        ])
    }
}

/// Malla transformada; con espejo se invierten las caras para que sigan
/// mirando hacia afuera (los vértices conservan su índice: los pesos siguen valiendo)
fn transform_mesh(mesh: &Mesh, rigid: &Rigid) -> Result<Mesh, String> {
    let positions: Vec<Vector3> = mesh.vertices.iter().map(|v| rigid.point(v.position)).collect();
    let mut out = if rigid.mirrored {
        let triangles: Vec<[usize; 3]> = (0..mesh.num_faces())
            .map(|f| {
                let [a, b, c] = mesh.get_face_vertices(f);
                [a, c, b]
            })
            .collect();
        Mesh::try_from_triangles(&positions, &triangles)?
    } else {
        let mut m = mesh.clone();
        for (v, p) in m.vertices.iter_mut().zip(positions) {
            v.position = p;
        }
        m
    };
    out.compute_vertex_normals();
    Ok(out)
}

fn apply_placement_impl(state: &AppState, matrix: [f64; 16]) -> Result<PlacementResult, String> {
    let rigid = Rigid::from_column_major(matrix)?;
    let _guard = state.try_begin_processing().ok_or("Espera a que termine el proceso en curso")?;

    {
        let mut mesh = state.mesh.lock().unwrap();
        let current = mesh.as_ref().ok_or("No hay malla cargada")?;
        *mesh = Some(transform_mesh(current, &rigid)?);
    }
    for slot in [&state.scene, &state.scene_before_repair, &state.scene_before_unwrap, &state.scene_before_print_scale] {
        let mut scene = slot.lock().unwrap();
        if let Some(s) = scene.as_ref() {
            *scene = Some(wrap_scene(s, rigid.glam(), PLACEMENT_NODE));
        }
    }
    for slot in [&state.mesh_before_repair, &state.mesh_before_unwrap, &state.mesh_before_print_scale] {
        let mut mesh = slot.lock().unwrap();
        if let Some(m) = mesh.as_ref() {
            *mesh = Some(transform_mesh(m, &rigid)?);
        }
    }

    if let Some(quad) = state.quad_mesh.lock().unwrap().as_mut() {
        for v in &mut quad.vertices {
            let [x, y, z] = rigid.apply([v.x, v.y, v.z]);
            *v = pinocchio_math::nalgebra::Vector3::new(x, y, z);
        }
        if rigid.mirrored {
            for f in &mut quad.faces {
                f.v = [f.v[0], f.v[3], f.v[2], f.v[1]];
            }
        }
    }
    if rigid.mirrored
        && let Some(skin) = state.quad_skin.lock().unwrap().as_mut()
    {
        for c in &mut skin.corners {
            *c = [c[0], c[3], c[2], c[1]];
        }
    }

    // Solo el esqueleto colocado sobre la malla; un preset sin ajustar no
    // está en sus coordenadas. Se hornea con su gizmo, que vuelve a cero
    let skeleton_reset = {
        let mut skeleton = state.skeleton.lock().unwrap();
        match skeleton.as_ref() {
            Some(SkeletonType::Custom(s)) => {
                let placed = pinocchio_skeleton::map_positions(s, |p| rigid.point(p));
                *skeleton = Some(SkeletonType::Custom(placed.clone()));
                *state.original_skeleton.lock().unwrap() = Some(SkeletonType::Custom(placed));
                *state.skeleton_transform.lock().unwrap() = SkeletonTransformParams::default();
                true
            }
            _ => false,
        }
    };

    if let Some(result) = state.result.lock().unwrap().as_mut() {
        for p in result.bone_positions.iter_mut().chain(result.embedding.bone_positions.iter_mut()) {
            *p = rigid.point(*p);
        }
        let t = rigid.transform();
        for rest in &mut result.bone_rest_transforms {
            *rest = t.compose(rest);
        }
    }

    // Derivados que se recalculan solos
    *state.joint_centering.lock().unwrap() = None;
    *state.print3d_pieces.lock().unwrap() = None;

    Ok(PlacementResult { skeleton_reset })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Por columnas, como `Matrix4.elements` de Three.js
    fn column_major(r: [[f64; 3]; 3], t: [f64; 3]) -> [f64; 16] {
        [
            r[0][0], r[1][0], r[2][0], 0.0,
            r[0][1], r[1][1], r[2][1], 0.0,
            r[0][2], r[1][2], r[2][2], 0.0,
            t[0], t[1], t[2], 1.0,
        ]
    }

    /// 90° alrededor de X (Z arriba → Y arriba) y trasladado
    fn z_up_to_y_up() -> [f64; 16] {
        column_major([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], [1.0, 2.0, 3.0])
    }

    fn cube_state() -> AppState {
        let p = [
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0],
        ]
        .map(|[x, y, z]| Vector3::new(x, y, z));
        let t = [
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [3, 7, 6], [3, 6, 2], [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
        ];
        let mesh = Mesh::try_from_triangles(&p, &t).unwrap();
        let state = AppState::new();
        *state.scene.lock().unwrap() = Some(crate::commands::mesh_to_scene(&mesh, "cubo", None));
        *state.mesh.lock().unwrap() = Some(mesh);
        state
    }

    fn positions(state: &AppState) -> Vec<[f64; 3]> {
        let mesh = state.mesh.lock().unwrap();
        mesh.as_ref().unwrap().vertices.iter().map(|v| [v.position.x(), v.position.y(), v.position.z()]).collect()
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5)
    }

    #[test]
    fn accepts_uniform_scale_rejects_uneven_and_detects_mirror() {
        let uneven = column_major([[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]);
        assert!(Rigid::from_column_major(uneven).is_err());
        let uniform = column_major([[0.0, -2.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]], [1.0, 0.0, 0.0]);
        let r = Rigid::from_column_major(uniform).unwrap();
        assert!((r.scale - 2.0).abs() < 1e-12 && !r.mirrored);
        assert!(close(r.apply([1.0, 0.0, 0.0]), [1.0, 2.0, 0.0]));
        assert!(!Rigid::from_column_major(z_up_to_y_up()).unwrap().mirrored);
        let mirror = column_major([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]);
        assert!(Rigid::from_column_major(mirror).unwrap().mirrored);
    }

    #[test]
    fn moves_mesh_scene_and_placed_skeleton_and_undoes() {
        let state = cube_state();
        let before = positions(&state);
        let skeleton = BasicSkeleton::from_bones(vec![
            Bone::new("raiz", Vector3::new(0.5, 0.0, 0.5)),
            Bone::with_parent("punta", Vector3::new(0.5, 0.0, 1.0), 0),
        ]);
        *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(skeleton));
        state.skeleton_transform.lock().unwrap().scale = 2.0;

        let r = apply_placement_impl(&state, z_up_to_y_up()).unwrap();
        assert!(r.skeleton_reset);
        assert_eq!(*state.skeleton_transform.lock().unwrap(), SkeletonTransformParams::default());
        let rigid = Rigid::from_column_major(z_up_to_y_up()).unwrap();
        let after = positions(&state);
        assert!(before.iter().zip(&after).all(|(b, a)| close(rigid.apply(*b), *a)));
        // La punta sobre +Z pasa a +Y (más la traslación)
        let tip = match state.skeleton.lock().unwrap().as_ref().unwrap() {
            SkeletonType::Custom(s) => s.bones()[1].position,
            _ => unreachable!(),
        };
        assert!(close([tip.x(), tip.y(), tip.z()], [1.5, 3.0, 3.0]));

        // La escena exporta lo mismo que la malla
        let scene = state.scene.lock().unwrap().clone().unwrap();
        let world: Vec<[f32; 3]> = scene.world_primitives().into_iter().flat_map(|p| p.positions).collect();
        assert!(world.iter().zip(&after).all(|(w, a)| close(w.map(f64::from), *a)));

        // Deshacer = aplicar la inversa; el nodo de orientación se compone
        let inverse = column_major([[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]], [-1.0, 3.0, -2.0]);
        apply_placement_impl(&state, inverse).unwrap();
        assert!(before.iter().zip(&positions(&state)).all(|(b, a)| close(*b, *a)));
        let scene = state.scene.lock().unwrap().clone().unwrap();
        assert_eq!(scene.nodes.iter().filter(|n| n.name == PLACEMENT_NODE).count(), 1);
    }

    #[test]
    fn uniform_scale_moves_mesh_scene_and_skeleton_and_undoes() {
        let state = cube_state();
        let before = positions(&state);
        let skeleton = BasicSkeleton::from_bones(vec![Bone::new("raiz", Vector3::new(0.5, 1.0, 0.5))]);
        *state.skeleton.lock().unwrap() = Some(SkeletonType::Custom(skeleton));

        // ×1000 (metros → milímetros) con el origen fijo
        let k = 1000.0;
        apply_placement_impl(&state, column_major([[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]], [0.0; 3])).unwrap();
        let after = positions(&state);
        assert!(before.iter().zip(&after).all(|(b, a)| close(b.map(|x| x * k), *a)));
        let root = match state.skeleton.lock().unwrap().as_ref().unwrap() {
            SkeletonType::Custom(s) => s.bones()[0].position,
            _ => unreachable!(),
        };
        assert!(close([root.x(), root.y(), root.z()], [500.0, 1000.0, 500.0]));
        let scene = state.scene.lock().unwrap().clone().unwrap();
        let world: Vec<[f32; 3]> = scene.world_primitives().into_iter().flat_map(|p| p.positions).collect();
        assert!(world.iter().zip(&after).all(|(w, a)| (0..3).all(|i| (f64::from(w[i]) - a[i]).abs() < 1e-2)));

        let inv = 1.0 / k;
        apply_placement_impl(&state, column_major([[inv, 0.0, 0.0], [0.0, inv, 0.0], [0.0, 0.0, inv]], [0.0; 3])).unwrap();
        assert!(before.iter().zip(&positions(&state)).all(|(b, a)| close(*b, *a)));
    }

    #[test]
    fn mirror_keeps_faces_outward() {
        let state = cube_state();
        let mirror = column_major([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]);
        apply_placement_impl(&state, mirror).unwrap();
        let mesh = state.mesh.lock().unwrap();
        assert!(mesh.as_ref().unwrap().volume() > 0.99);
    }

    #[test]
    fn floor_candidates_of_cube() {
        let info = placement_info(&cube_state()).unwrap();
        assert_eq!(info.candidates.len(), 6);
        assert!(info.candidates.iter().all(|c| c.stable));
        assert!(close(info.center_of_mass, [0.5, 0.5, 0.5]));
    }

    use pinocchio_skeleton::{BasicSkeleton, Bone, Skeleton};
}
