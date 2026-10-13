//! Evaluación de animaciones: la pose de la escena en un instante.
//!
//! Se muestrean los canales de una [`Animation`] (lineal, escalón y spline cúbico de
//! glTF) y se obtiene la transformación local de cada nodo; con ella
//! [`Scene::world_primitives_at`] mueve los nodos y deforma las mallas con skin.

use crate::{Animation, Channel, Interpolation, KeyframeValues, Scene, Transform};
use glam::{Quat, Vec3};

impl Animation {
    /// Segundos hasta el último keyframe
    pub fn duration(&self) -> f32 {
        self.channels.iter().filter_map(|c| c.times.last().copied()).fold(0.0, f32::max)
    }
}

/// Segmento de keys que contiene `t`: (anterior, siguiente, fracción 0..1, duración del segmento).
/// Fuera del rango se queda en la primera o la última key.
fn segment(times: &[f32], t: f32) -> (usize, usize, f32, f32) {
    let last = times.len() - 1;
    if t <= times[0] {
        return (0, 0, 0.0, 0.0);
    }
    if t >= times[last] {
        return (last, last, 0.0, 0.0);
    }
    let next = times.partition_point(|&k| k <= t);
    let prev = next - 1;
    let dt = times[next] - times[prev];
    (prev, next, if dt > 0.0 { (t - times[prev]) / dt } else { 0.0 }, dt)
}

/// Spline cúbico de Hermite de glTF: por cada key hay (tangente de entrada, valor, tangente de salida)
fn hermite(p0: Vec3, m0: Vec3, p1: Vec3, m1: Vec3, u: f32, dt: f32) -> Vec3 {
    let (u2, u3) = (u * u, u * u * u);
    p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + m0 * dt * (u3 - 2.0 * u2 + u) + p1 * (-2.0 * u3 + 3.0 * u2) + m1 * dt * (u3 - u2)
}

fn sample_vec3(ch: &Channel, values: &[[f32; 3]], t: f32) -> Option<Vec3> {
    if ch.times.is_empty() {
        return None;
    }
    let cubic = ch.interpolation == Interpolation::CubicSpline;
    // En spline cada key ocupa tres valores
    let at = |k: usize| -> Option<Vec3> { values.get(if cubic { 3 * k + 1 } else { k }).map(|v| Vec3::from(*v)) };
    let (a, b, u, dt) = segment(&ch.times, t);
    if a == b {
        return at(a);
    }
    match ch.interpolation {
        Interpolation::Step => at(a),
        Interpolation::Linear => Some(at(a)?.lerp(at(b)?, u)),
        Interpolation::CubicSpline => {
            let out_tangent = Vec3::from(*values.get(3 * a + 2)?);
            let in_tangent = Vec3::from(*values.get(3 * b)?);
            Some(hermite(at(a)?, out_tangent, at(b)?, in_tangent, u, dt))
        }
    }
}

fn sample_quat(ch: &Channel, values: &[[f32; 4]], t: f32) -> Option<Quat> {
    if ch.times.is_empty() {
        return None;
    }
    let cubic = ch.interpolation == Interpolation::CubicSpline;
    let at = |k: usize| -> Option<Quat> { values.get(if cubic { 3 * k + 1 } else { k }).map(|v| Quat::from_array(*v).normalize()) };
    let (a, b, u, dt) = segment(&ch.times, t);
    if a == b {
        return at(a);
    }
    match ch.interpolation {
        Interpolation::Step => at(a),
        Interpolation::Linear => Some(at(a)?.slerp(at(b)?, u)),
        Interpolation::CubicSpline => {
            let q = |v: &[f32; 4]| glam::Vec4::from(*v);
            let (p0, p1) = (q(&values[3 * a + 1]), q(&values[3 * b + 1]));
            let (m0, m1) = (q(&values[3 * a + 2]), q(&values[3 * b]));
            let (u2, u3) = (u * u, u * u * u);
            let v = p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + m0 * dt * (u3 - 2.0 * u2 + u) + p1 * (-2.0 * u3 + 3.0 * u2) + m1 * dt * (u3 - u2);
            Some(Quat::from_vec4(v).normalize())
        }
    }
}

impl Scene {
    /// Transformación local de cada nodo con los canales de la animación `animation` evaluados
    /// en `time` segundos; los nodos sin canal conservan la suya. `None` si no existe la animación.
    pub fn posed_locals(&self, animation: usize, time: f32) -> Option<Vec<Transform>> {
        let anim = self.animations.get(animation)?;
        let mut trs: Vec<Option<(Vec3, Quat, Vec3)>> = vec![None; self.nodes.len()];
        let decompose = |t: Transform| match t {
            Transform::Trs { translation, rotation, scale } => (translation, rotation, scale),
            Transform::Matrix(m) => {
                let (s, r, t) = m.to_scale_rotation_translation();
                (t, r, s)
            }
        };
        for ch in &anim.channels {
            let Some(node) = self.nodes.get(ch.node) else { continue };
            let slot = trs[ch.node].get_or_insert_with(|| decompose(node.transform));
            match &ch.values {
                KeyframeValues::Translation(v) => {
                    if let Some(t) = sample_vec3(ch, v, time) {
                        slot.0 = t;
                    }
                }
                KeyframeValues::Rotation(v) => {
                    if let Some(r) = sample_quat(ch, v, time) {
                        slot.1 = r;
                    }
                }
                KeyframeValues::Scale(v) => {
                    if let Some(s) = sample_vec3(ch, v, time) {
                        slot.2 = s;
                    }
                }
                KeyframeValues::Weights(_) => {}
            }
        }
        Some(
            self.nodes
                .iter()
                .zip(trs)
                .map(|(n, o)| match o {
                    Some((translation, rotation, scale)) => Transform::Trs { translation, rotation, scale },
                    None => n.transform,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IndexData, Joint, Mesh, Node, Primitive, Skeleton, VertexAttribute};
    use glam::Mat4;

    fn chan(node: usize, interpolation: Interpolation, times: Vec<f32>, values: KeyframeValues) -> Channel {
        Channel { node, interpolation, times, values }
    }

    /// Dos joints en cadena (raíz en el origen, hijo a 1 en X) y una malla de dos puntos pegada a cada uno
    fn arm() -> Scene {
        let mut s = Scene::new();
        s.meshes.push(Mesh {
            name: "brazo".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![[0.5, 0.0, 0.0], [1.5, 0.0, 0.0], [2.0, 0.0, 0.0]]),
                    VertexAttribute::Normals(vec![[0.0, 1.0, 0.0]; 3]),
                    VertexAttribute::JointIndices(vec![[0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 0, 0]]),
                    VertexAttribute::JointWeights(vec![[1.0, 0.0, 0.0, 0.0]; 3]),
                ],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });
        let trs = |t: [f32; 3]| Transform::Trs { translation: Vec3::from(t), rotation: Quat::IDENTITY, scale: Vec3::ONE };
        s.nodes = vec![
            Node { name: "malla".into(), transform: Transform::identity(), mesh: Some(0), skin: Some(0), children: vec![] },
            Node { name: "raiz".into(), transform: trs([0.0; 3]), mesh: None, skin: None, children: vec![2] },
            Node { name: "codo".into(), transform: trs([1.0, 0.0, 0.0]), mesh: None, skin: None, children: vec![] },
        ];
        s.root_nodes = vec![0, 1];
        s.skeletons.push(Skeleton {
            name: "brazo".into(),
            joints: vec![
                Joint { name: "raiz".into(), children: vec![1], inverse_bind_matrix: Mat4::IDENTITY, local_transform: Mat4::IDENTITY, node_index: Some(1) },
                Joint {
                    name: "codo".into(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::from_translation(Vec3::new(-1.0, 0.0, 0.0)),
                    local_transform: Mat4::from_translation(Vec3::X),
                    node_index: Some(2),
                },
            ],
            roots: vec![0],
        });
        s
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        (Vec3::from(a) - Vec3::from(b)).length() < 1e-4
    }

    #[test]
    fn interpola_linear_y_escalon() {
        let mut s = arm();
        let q = |deg: f32| Quat::from_rotation_z(deg.to_radians()).to_array();
        s.animations.push(Animation {
            name: "gira".into(),
            channels: vec![chan(1, Interpolation::Linear, vec![0.0, 1.0], KeyframeValues::Rotation(vec![q(0.0), q(90.0)]))],
        });
        // A mitad del giro, el codo (1,0,0) queda a 45° de la raíz
        let l = s.posed_locals(0, 0.5).unwrap();
        let m = l[1].to_matrix().transform_point3(Vec3::X);
        assert!(near(m.to_array(), [45f32.to_radians().cos(), 45f32.to_radians().sin(), 0.0]), "{m}");
        // Fuera del rango se queda en la última key
        let end = s.posed_locals(0, 5.0).unwrap()[1].to_matrix().transform_point3(Vec3::X);
        assert!(near(end.to_array(), [0.0, 1.0, 0.0]));
        // Escalón: hasta la siguiente key no cambia
        s.animations[0].channels[0].interpolation = Interpolation::Step;
        let st = s.posed_locals(0, 0.9).unwrap()[1].to_matrix().transform_point3(Vec3::X);
        assert!(near(st.to_array(), [1.0, 0.0, 0.0]));
        assert!((s.animations[0].duration() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn spline_cubico_pasa_por_las_keys() {
        let mut s = arm();
        // (entrada, valor, salida) por key; tangentes nulas → llega a las keys con suavidad
        let z = [0.0; 3];
        s.animations.push(Animation {
            name: "sube".into(),
            channels: vec![chan(
                1,
                Interpolation::CubicSpline,
                vec![0.0, 2.0],
                KeyframeValues::Translation(vec![z, [0.0, 0.0, 0.0], z, z, [0.0, 4.0, 0.0], z]),
            )],
        });
        let y = |t: f32| s.posed_locals(0, t).unwrap()[1].to_matrix().transform_point3(Vec3::ZERO).y;
        assert!(y(0.0).abs() < 1e-5 && (y(2.0) - 4.0).abs() < 1e-5);
        // A mitad, con tangentes nulas, la curva está justo a medias
        assert!((y(1.0) - 2.0).abs() < 1e-4, "{}", y(1.0));
    }

    #[test]
    fn skinning_mueve_los_vertices_con_su_joint() {
        let mut s = arm();
        s.animations.push(Animation {
            name: "doblar".into(),
            channels: vec![chan(
                2,
                Interpolation::Linear,
                vec![0.0, 1.0],
                KeyframeValues::Rotation(vec![Quat::IDENTITY.to_array(), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array()]),
            )],
        });
        let at = |t: f32| s.world_primitives_at(0, t).remove(0);
        // Al principio la malla está en reposo
        let rest = at(0.0);
        assert!(near(rest.positions[1], [1.5, 0.0, 0.0]) && near(rest.positions[0], [0.5, 0.0, 0.0]));
        // Con el codo doblado 90°, lo que cuelga de él gira alrededor de (1,0,0); lo de la raíz no se mueve
        let bent = at(1.0);
        assert!(near(bent.positions[0], [0.5, 0.0, 0.0]), "{:?}", bent.positions[0]);
        assert!(near(bent.positions[1], [1.0, 0.5, 0.0]), "{:?}", bent.positions[1]);
        assert!(near(bent.positions[2], [1.0, 1.0, 0.0]), "{:?}", bent.positions[2]);
        // La normal gira con el hueso: (0,1,0) → (-1,0,0)
        assert!(near(bent.normals.unwrap()[2], [-1.0, 0.0, 0.0]));
        // Sin pose, el comportamiento de siempre
        assert!(near(s.world_primitives()[0].positions[1], [1.5, 0.0, 0.0]));
    }

    #[test]
    fn animacion_inexistente_no_cambia_nada() {
        let s = arm();
        assert!(s.posed_locals(3, 0.0).is_none());
        assert!(near(s.world_primitives_at(3, 0.0)[0].positions[2], [2.0, 0.0, 0.0]));
    }

    #[test]
    fn nodo_animado_sin_skin_se_traslada() {
        let mut s = Scene::new();
        s.meshes.push(Mesh {
            name: "p".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])],
                indices: None,
                material: None,
            }],
        });
        s.nodes.push(Node { name: "n".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        s.root_nodes = vec![0];
        s.animations.push(Animation {
            name: "mueve".into(),
            channels: vec![chan(0, Interpolation::Linear, vec![0.0, 2.0], KeyframeValues::Translation(vec![[0.0; 3], [10.0, 0.0, 0.0]]))],
        });
        assert!(near(s.world_primitives_at(0, 1.0)[0].positions[0], [5.0, 0.0, 0.0]));
    }
}
