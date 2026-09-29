//! Animaciones de la línea de tiempo → canales de la escena exportada.
//!
//! La app anima **articulaciones** (las esferas del visor): girar la
//! articulación `J` gira todo lo que cuelga de ella alrededor de su punto. En
//! la escena con skin (ver `rigged_scene`) el joint de cada hueso `b` está en
//! la cabeza de su segmento, la posición de `padre(b)`; así que el giro de `J`
//! va a los joints de sus hijos, que están justo en `J`. La raíz no tiene
//! segmento: su giro y su traslación van a su propio joint y mueven todo.

use converter_scene::glam::{Quat, Vec3};
use converter_scene::{Animation, Channel, Interpolation, KeyframeValues};
use serde::{Deserialize, Serialize};

/// Interpolación desde una key hasta la siguiente
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyInterpolation {
    Linear,
    Step,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key<T> {
    pub frame: f32,
    pub value: T,
    pub interpolation: KeyInterpolation,
}

/// Keys de una articulación: giro local (x, y, z, w) y, en la raíz,
/// desplazamiento respecto del reposo
#[derive(Debug, Clone, Deserialize)]
pub struct JointTrack {
    pub joint: usize,
    #[serde(default)]
    pub rotation: Vec<Key<[f32; 4]>>,
    #[serde(default)]
    pub translation: Vec<Key<[f32; 3]>>,
}

/// Una animación de la línea de tiempo (en cuadros)
#[derive(Debug, Clone, Deserialize)]
pub struct AnimationClip {
    pub name: String,
    pub fps: f32,
    pub tracks: Vec<JointTrack>,
}

/// Esqueleto de la escena con skin: padre de cada articulación, nodo de
/// escena de cada joint y su traslación de reposo (local al padre)
pub struct SkinRig<'a> {
    pub parents: &'a [Option<usize>],
    pub joint_nodes: &'a [usize],
    pub rest_translation: &'a [Vec3],
}

impl SkinRig<'_> {
    /// Joints que gira la articulación `joint`
    fn rotated_joints(&self, joint: usize) -> Vec<usize> {
        if self.parents[joint].is_none() {
            return vec![joint];
        }
        (0..self.parents.len()).filter(|&b| self.parents[b] == Some(joint)).collect()
    }
}

/// Canales de escena de cada animación. Las pistas de articulaciones que no
/// existen (el esqueleto cambió) se ignoran.
pub fn scene_animations(clips: &[AnimationClip], rig: &SkinRig) -> Vec<Animation> {
    clips
        .iter()
        .map(|clip| {
            let fps = if clip.fps > 0.0 { clip.fps } else { 24.0 };
            let mut channels = Vec::new();
            for track in clip.tracks.iter().filter(|t| t.joint < rig.parents.len()) {
                if let Some((interpolation, times, values)) = rotation_samples(&track.rotation, fps) {
                    for joint in rig.rotated_joints(track.joint) {
                        channels.push(Channel {
                            node: rig.joint_nodes[joint],
                            interpolation,
                            times: times.clone(),
                            values: KeyframeValues::Rotation(values.clone()),
                        });
                    }
                }
                // Solo la raíz se desplaza: el resto está sujeto a su padre
                if rig.parents[track.joint].is_none() {
                    let rest = rig.rest_translation[track.joint];
                    if let Some((interpolation, times, values)) = translation_samples(&track.translation, fps, rest) {
                        channels.push(Channel {
                            node: rig.joint_nodes[track.joint],
                            interpolation,
                            times,
                            values: KeyframeValues::Translation(values),
                        });
                    }
                }
            }
            Animation { name: clip.name.clone(), channels }
        })
        .collect()
}

type Samples<T> = (Interpolation, Vec<f32>, Vec<T>);

/// Keys ordenadas; si mezclan interpolaciones (glTF usa una por canal) se
/// hornean cuadro a cuadro como lineales
fn resample<T: Copy>(
    keys: &[Key<T>],
    fps: f32,
    lerp: impl Fn(T, T, f32) -> T,
) -> Option<Samples<T>> {
    let mut keys: Vec<&Key<T>> = keys.iter().collect();
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    keys.dedup_by(|b, a| a.frame == b.frame);
    let first = keys.first()?;
    let uniform = keys.iter().all(|k| k.interpolation == first.interpolation);
    if uniform || keys.len() == 1 {
        let interpolation = match first.interpolation {
            KeyInterpolation::Linear => Interpolation::Linear,
            KeyInterpolation::Step => Interpolation::Step,
        };
        let times = keys.iter().map(|k| k.frame / fps).collect();
        let values = keys.iter().map(|k| k.value).collect();
        return Some((interpolation, times, values));
    }

    let start = first.frame.floor() as i64;
    let end = keys.last()?.frame.ceil() as i64;
    let mut times = Vec::new();
    let mut values = Vec::new();
    let mut segment = 0;
    for frame in start..=end {
        let f = frame as f32;
        while segment + 1 < keys.len() && keys[segment + 1].frame <= f {
            segment += 1;
        }
        let a = keys[segment];
        let value = match keys.get(segment + 1) {
            Some(b) if f > a.frame && a.interpolation == KeyInterpolation::Linear => {
                lerp(a.value, b.value, (f - a.frame) / (b.frame - a.frame))
            }
            _ => a.value,
        };
        times.push(f / fps);
        values.push(value);
    }
    Some((Interpolation::Linear, times, values))
}

fn rotation_samples(keys: &[Key<[f32; 4]>], fps: f32) -> Option<Samples<[f32; 4]>> {
    let slerp = |a: [f32; 4], b: [f32; 4], t: f32| Quat::from_array(a).slerp(Quat::from_array(b), t).to_array();
    let (interpolation, times, mut values) = resample(keys, fps, slerp)?;
    // Unitarios y en el mismo hemisferio que el anterior: interpolar entre q y
    // -q (el mismo giro) daría una vuelta completa
    let mut previous: Option<Quat> = None;
    for v in &mut values {
        let mut q = Quat::from_array(*v).normalize();
        if previous.is_some_and(|p| p.dot(q) < 0.0) {
            q = -q;
        }
        previous = Some(q);
        *v = q.to_array();
    }
    Some((interpolation, times, values))
}

fn translation_samples(keys: &[Key<[f32; 3]>], fps: f32, rest: Vec3) -> Option<Samples<[f32; 3]>> {
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| Vec3::from(a).lerp(Vec3::from(b), t).to_array();
    let (interpolation, times, values) = resample(keys, fps, lerp)?;
    let values = values.into_iter().map(|v| (rest + Vec3::from(v)).to_array()).collect();
    Some((interpolation, times, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key<T>(frame: f32, value: T, interpolation: KeyInterpolation) -> Key<T> {
        Key { frame, value, interpolation }
    }

    /// raíz 0 → 1 → {2, 3}
    fn rig() -> (Vec<Option<usize>>, Vec<usize>, Vec<Vec3>) {
        let parents = vec![None, Some(0), Some(1), Some(1)];
        let nodes = vec![10, 11, 12, 13];
        let rest = vec![Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, Vec3::Y, Vec3::Y];
        (parents, nodes, rest)
    }

    fn clip(tracks: Vec<JointTrack>) -> AnimationClip {
        AnimationClip { name: "c".into(), fps: 24.0, tracks }
    }

    #[test]
    fn joint_rotation_targets_children_and_root_itself() {
        let (parents, nodes, rest) = rig();
        let r = SkinRig { parents: &parents, joint_nodes: &nodes, rest_translation: &rest };
        let q = Quat::from_rotation_z(0.5).to_array();
        let tracks = vec![
            JointTrack { joint: 1, rotation: vec![key(0.0, q, KeyInterpolation::Linear)], translation: vec![] },
            JointTrack { joint: 0, rotation: vec![key(0.0, q, KeyInterpolation::Linear)], translation: vec![] },
            // Hoja: sin hijos, no gira nada
            JointTrack { joint: 2, rotation: vec![key(0.0, q, KeyInterpolation::Linear)], translation: vec![] },
        ];
        let anims = scene_animations(&[clip(tracks)], &r);
        let targets: Vec<usize> = anims[0].channels.iter().map(|c| c.node).collect();
        assert_eq!(targets, vec![12, 13, 10]);
    }

    #[test]
    fn root_translation_is_offset_from_rest_and_only_root_moves() {
        let (parents, nodes, rest) = rig();
        let r = SkinRig { parents: &parents, joint_nodes: &nodes, rest_translation: &rest };
        let tracks = vec![
            JointTrack {
                joint: 0,
                rotation: vec![],
                translation: vec![key(0.0, [0.0, 0.0, 0.0], KeyInterpolation::Linear), key(12.0, [2.0, 0.0, 0.0], KeyInterpolation::Linear)],
            },
            JointTrack { joint: 1, rotation: vec![], translation: vec![key(0.0, [5.0, 0.0, 0.0], KeyInterpolation::Linear)] },
        ];
        let anims = scene_animations(&[clip(tracks)], &r);
        assert_eq!(anims[0].channels.len(), 1);
        let ch = &anims[0].channels[0];
        assert_eq!(ch.node, 10);
        assert_eq!(ch.times, vec![0.0, 0.5]);
        match &ch.values {
            KeyframeValues::Translation(v) => assert_eq!(v, &vec![[0.0, 1.0, 0.0], [2.0, 1.0, 0.0]]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mixed_interpolation_is_baked_per_frame() {
        let a = Quat::IDENTITY.to_array();
        let b = Quat::from_rotation_x(1.0).to_array();
        let keys = vec![
            key(0.0, a, KeyInterpolation::Step),
            key(2.0, b, KeyInterpolation::Linear),
            key(4.0, a, KeyInterpolation::Linear),
        ];
        let (interp, times, values) = rotation_samples(&keys, 2.0).unwrap();
        assert_eq!(interp, Interpolation::Linear);
        assert_eq!(times, vec![0.0, 0.5, 1.0, 1.5, 2.0]);
        // Cuadro 1: step mantiene a; cuadro 3: mitad entre b y a
        assert_eq!(values[1], a);
        let mid = Quat::from_array(values[3]);
        assert!(mid.angle_between(Quat::from_rotation_x(0.5)) < 1e-4);
    }

    #[test]
    fn quaternions_stay_in_one_hemisphere() {
        let q = Quat::from_rotation_y(0.3);
        let keys = vec![key(0.0, q.to_array(), KeyInterpolation::Linear), key(1.0, (-q).to_array(), KeyInterpolation::Linear)];
        let (_, _, values) = rotation_samples(&keys, 24.0).unwrap();
        assert!(Quat::from_array(values[0]).dot(Quat::from_array(values[1])) > 0.0);
    }
}
