//! Esqueleto, pesos y animaciones de un modelo importado con skin (glTF).
//!
//! La app anima **articulaciones** con huesos entre ellas (ver
//! `animation.rs`): el peso del hueso `b` es el del segmento `padre(b) → b` y
//! girar la articulación `J` mueve los segmentos que salen de `J`. En glTF un
//! vértice sigue a un joint `J`, que es justamente lo que mueven esos
//! segmentos; así que el peso de `J` va a uno de sus hijos (el segmento más
//! cercano al vértice; todos giran igual). A un joint hoja con pesos se le
//! agrega una punta (`<nombre>_fin`) para que tenga segmento.
//!
//! Las animaciones se muestrean cuadro a cuadro: `D_J = mundo(J, t) · IBM_J`
//! es el giro que el skin aplica a lo que sigue a `J`, y el giro local de la
//! app es `D_padre⁻¹ · D_J` (la raíz además se desplaza). Después se quitan
//! las keys que la interpolación lineal ya reproduce.

use crate::animation::{Key, KeyInterpolation};
use converter_scene::glam::{Mat4, Quat, Vec3};
use converter_scene::{Animation, Interpolation, KeyframeValues, Scene, Transform, VertexAttribute};
use pinocchio_attachment::Attachment;
use pinocchio_core::{PinocchioOutput, ProcessStats};
use pinocchio_embedding::EmbeddingResult;
use pinocchio_math::Vector3;
use pinocchio_skeleton::{BasicSkeleton, Bone};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Rig del archivo, listo para la app
pub struct ImportedRig {
    pub skeleton: BasicSkeleton,
    pub output: PinocchioOutput,
    pub clips: Vec<ClipDto>,
}

/// Animación con el formato de la línea de tiempo (`lib/animation.ts`)
#[derive(Debug, Clone, Serialize)]
pub struct ClipDto {
    pub name: String,
    pub fps: f32,
    pub start: f32,
    pub end: f32,
    pub tracks: Vec<TrackDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackDto {
    pub bone: String,
    pub rotation: Vec<Key<[f32; 4]>>,
    pub translation: Vec<Key<[f32; 3]>>,
}

/// Articulación de la app: un joint del archivo o una punta agregada
struct AppJoint {
    name: String,
    position: Vec3,
    parent: Option<usize>,
    /// Joint del esqueleto del archivo (`None` en las puntas)
    source: Option<usize>,
}

/// Rig del modelo, si alguna malla tiene skin. `num_vertices` es el de la
/// malla de pinocchio (vértices de `world_primitives` en orden).
pub fn from_scene(scene: &Scene, num_vertices: usize) -> Option<ImportedRig> {
    let prims = scene.world_primitives();
    let skin_of = |node: Option<usize>| node.and_then(|n| scene.nodes[n].skin).filter(|&s| s < scene.skeletons.len());

    // El esqueleto de la malla con más vértices
    let mut usage = vec![0usize; scene.skeletons.len()];
    for prim in &prims {
        if let Some(s) = skin_of(prim.node) {
            usage[s] += prim.positions.len();
        }
    }
    let (skin, _) = usage.iter().enumerate().filter(|&(_, &n)| n > 0).max_by_key(|&(_, &n)| n)?;
    let skel = &scene.skeletons[skin];
    if skel.joints.is_empty() {
        return None;
    }

    let node_parent = node_parents(scene);
    let joint_of_node: HashMap<usize, usize> =
        skel.joints.iter().enumerate().filter_map(|(j, joint)| Some((joint.node_index?, j))).collect();
    // Joint más cercano hacia arriba en el grafo de nodos (el propio, si lo es)
    let ancestor_joint = |mut node: Option<usize>| {
        while let Some(n) = node {
            if let Some(&j) = joint_of_node.get(&n) {
                return Some(j);
            }
            node = node_parent[n];
        }
        None
    };

    // Reposo: la inversa de la matriz de bind, en el espacio de los vértices
    let bind: Vec<Mat4> = skel.joints.iter().map(|j| j.inverse_bind_matrix.inverse()).collect();
    let rest: Vec<Vec3> = bind.iter().map(|m| m.transform_point3(Vec3::ZERO)).collect();
    let mut source_parent: Vec<Option<usize>> = skel
        .joints
        .iter()
        .map(|j| ancestor_joint(j.node_index.and_then(|n| node_parent[n])))
        .collect();

    // Influencias de cada vértice sobre joints del archivo (`None`: la raíz)
    let mut influences: Vec<Vec<(Option<usize>, f32)>> = Vec::with_capacity(num_vertices);
    for prim in &prims {
        let count = prim.positions.len();
        let attributes = &scene.meshes[prim.mesh].primitives[prim.primitive].attributes;
        let skinned = skin_of(prim.node).and_then(|s| {
            let indices = attributes.iter().find_map(|a| match a {
                VertexAttribute::JointIndices(v) if v.len() == count => Some(v),
                _ => None,
            })?;
            let weights = attributes.iter().find_map(|a| match a {
                VertexAttribute::JointWeights(v) if v.len() == count => Some(v),
                _ => None,
            })?;
            Some((&scene.skeletons[s], indices, weights))
        });
        // Sin skin: rígida con el joint del que cuelga, o con la raíz
        let rigid = ancestor_joint(prim.node);
        for v in 0..count {
            let mut list: Vec<(Option<usize>, f32)> = Vec::new();
            if let Some((s, indices, weights)) = skinned {
                for k in 0..4 {
                    let w = weights[v][k];
                    let joint = s.joints.get(indices[v][k] as usize).and_then(|j| joint_of_node.get(&j.node_index?));
                    if let (true, Some(&j)) = (w > 0.0, joint) {
                        match list.iter_mut().find(|(a, _)| *a == Some(j)) {
                            Some(entry) => entry.1 += w,
                            None => list.push((Some(j), w)),
                        }
                    }
                }
            }
            if list.is_empty() {
                list.push((rigid, 1.0));
            }
            influences.push(list);
        }
    }
    if influences.len() != num_vertices {
        return None;
    }

    // La app solo desplaza raíces: las raíces sin pesos con un solo hijo (el
    // "root" de Blender encima de la cadera) se quitan para que la cadera,
    // que es la que se traslada, quede de raíz. Sus giros siguen incluidos en
    // el mundo de los hijos.
    let weighted: HashSet<usize> = influences.iter().flatten().filter_map(|&(j, _)| j).collect();
    loop {
        let removable = (0..skel.joints.len()).find(|&r| {
            source_parent[r].is_none()
                && !weighted.contains(&r)
                && source_parent.iter().filter(|&&p| p == Some(r)).count() == 1
        });
        let Some(r) = removable else { break };
        for p in source_parent.iter_mut().filter(|p| **p == Some(r)) {
            *p = None;
        }
        // Fuera del árbol: su propio padre apunta a sí mismo
        source_parent[r] = Some(r);
    }

    // Orden de la app: padres antes que hijos
    let mut order = Vec::with_capacity(skel.joints.len());
    let mut stack: Vec<usize> = (0..skel.joints.len()).filter(|&j| source_parent[j].is_none()).rev().collect();
    let mut seen = vec![false; skel.joints.len()];
    while let Some(j) = stack.pop() {
        if std::mem::replace(&mut seen[j], true) {
            continue;
        }
        order.push(j);
        stack.extend((0..skel.joints.len()).rev().filter(|&c| c != j && source_parent[c] == Some(j)));
    }
    let mut app_of = vec![usize::MAX; skel.joints.len()];
    for (a, &j) in order.iter().enumerate() {
        app_of[j] = a;
    }
    let mut joints: Vec<AppJoint> = order
        .iter()
        .map(|&j| AppJoint {
            name: skel.joints[j].name.clone(),
            position: rest[j],
            parent: source_parent[j].map(|p| app_of[p]),
            source: Some(j),
        })
        .collect();
    // Un joint quitado nunca tiene pesos: todas las influencias caen en la app
    let influences: Vec<Vec<(usize, f32)>> = influences
        .into_iter()
        .map(|list| list.into_iter().map(|(j, w)| (j.map_or(0, |j| app_of[j]), w)).collect())
        .collect();

    // Puntas para los joints hoja que mueven vértices
    let diag = {
        let (min, max) = rest.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), &p| (a.min(p), b.max(p)));
        (max - min).length().max(1e-3)
    };
    let positions: Vec<Vec3> = prims.iter().flat_map(|p| p.positions.iter().map(|&x| Vec3::from(x))).collect();
    let weighted: HashSet<usize> = influences.iter().flatten().map(|&(a, _)| a).collect();
    let has_children: HashSet<usize> = joints.iter().filter_map(|j| j.parent).collect();
    for a in 0..joints.len() {
        if has_children.contains(&a) || !weighted.contains(&a) {
            continue;
        }
        let p = joints[a].position;
        let parent_dir = joints[a].parent.map(|q| p - joints[q].position).filter(|d| d.length() > 1e-6 * diag);
        // Hacia donde sigue el hueso; en glTF el eje Y del joint suele apuntar por el hueso
        let dir = parent_dir.map_or_else(
            || bind[joints[a].source.unwrap()].transform_vector3(Vec3::Y).normalize_or(Vec3::Y),
            |d| d.normalize(),
        );
        let reach = influences
            .iter()
            .zip(&positions)
            .filter(|(list, _)| list.iter().any(|&(j, w)| j == a && w >= 0.5))
            .map(|(_, &x)| (x - p).dot(dir))
            .fold(0.0f32, f32::max);
        let length = reach.max(parent_dir.map_or(0.0, |d| 0.25 * d.length())).max(0.02 * diag);
        joints.push(AppJoint { name: format!("{}_fin", joints[a].name), position: p + dir * length, parent: Some(a), source: None });
    }
    unique_names(&mut joints);

    // Pesos por hueso: el de `J` va al segmento más cercano que sale de `J`
    let num_bones = joints.len();
    let children: Vec<Vec<usize>> =
        (0..num_bones).map(|a| (0..num_bones).filter(|&c| joints[c].parent == Some(a)).collect()).collect();
    let root = 0;
    let weights: Vec<Vec<f64>> = influences
        .iter()
        .zip(&positions)
        .map(|(list, &x)| {
            let mut w = vec![0.0f64; num_bones];
            for &(a, weight) in list {
                let bone = children[a]
                    .iter()
                    .copied()
                    .min_by(|&c, &d| {
                        let dc = segment_distance(x, joints[a].position, joints[c].position);
                        let dd = segment_distance(x, joints[a].position, joints[d].position);
                        dc.total_cmp(&dd)
                    })
                    .unwrap_or(a);
                w[bone] += weight as f64;
            }
            let sum: f64 = w.iter().sum();
            if sum > 1e-12 {
                w.iter_mut().for_each(|v| *v /= sum);
            } else {
                w[root] = 1.0;
            }
            w
        })
        .collect();

    let bone_positions: Vec<Vector3> =
        joints.iter().map(|j| Vector3::new(j.position.x as f64, j.position.y as f64, j.position.z as f64)).collect();
    let skeleton = BasicSkeleton::from_bones(
        joints
            .iter()
            .enumerate()
            .map(|(a, j)| {
                let bone = match j.parent {
                    Some(p) => Bone::with_parent(&j.name, bone_positions[a], p),
                    None => Bone::new(&j.name, bone_positions[a]),
                };
                if children[a].is_empty() { bone.as_leaf() } else { bone }
            })
            .collect(),
    );

    let avg_influences = weights.iter().map(|w| w.iter().filter(|&&v| v > 0.0).count() as f64).sum::<f64>()
        / weights.len().max(1) as f64;
    let output = PinocchioOutput {
        attachment: Attachment::from_rest_positions(
            positions.iter().map(|p| Vector3::new(p.x as f64, p.y as f64, p.z as f64)).collect(),
            weights,
            num_bones,
        ),
        embedding: EmbeddingResult { bone_positions: bone_positions.clone(), sphere_bone_map: Vec::new(), quality_score: 1.0 },
        bone_rest_transforms: bone_positions.iter().map(|&p| pinocchio_math::Transform::from_translation(p)).collect(),
        bone_positions,
        stats: ProcessStats {
            num_vertices,
            num_bones,
            num_medial_spheres: 0,
            embedding_quality: 1.0,
            avg_influences_per_vertex: avg_influences,
        },
    };

    let inverse_bind: Vec<Mat4> = skel.joints.iter().map(|j| j.inverse_bind_matrix).collect();
    let rig = SampleRig { inverse_bind: &inverse_bind, joints: &joints, node_parent: &node_parent, diag };
    let clips = scene
        .animations
        .iter()
        .enumerate()
        .filter_map(|(i, anim)| sample_clip(scene, anim, i, &rig, skel))
        .collect();

    Some(ImportedRig { skeleton, output, clips })
}

/// Nombres no vacíos y distintos: la línea de tiempo guarda las pistas por nombre
fn unique_names(joints: &mut [AppJoint]) {
    let mut used = HashSet::new();
    for (a, joint) in joints.iter_mut().enumerate() {
        let base = if joint.name.trim().is_empty() { format!("hueso_{a}") } else { joint.name.clone() };
        let mut name = base.clone();
        let mut n = 1;
        while !used.insert(name.clone()) {
            name = format!("{base}.{n:03}");
            n += 1;
        }
        joint.name = name;
    }
}

fn segment_distance(x: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((x - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (x - (a + ab * t)).length()
}

fn node_parents(scene: &Scene) -> Vec<Option<usize>> {
    let mut parent = vec![None; scene.nodes.len()];
    for (i, node) in scene.nodes.iter().enumerate() {
        for &c in &node.children {
            if c < parent.len() && parent[c].is_none() && c != i {
                parent[c] = Some(i);
            }
        }
    }
    parent
}

// ─── Animaciones ────────────────────────────────────────────────────────────

struct SampleRig<'a> {
    inverse_bind: &'a [Mat4],
    joints: &'a [AppJoint],
    node_parent: &'a [Option<usize>],
    diag: f32,
}

/// Traslación, giro y escala locales de un nodo
#[derive(Clone, Copy)]
struct Trs {
    t: Vec3,
    r: Quat,
    s: Vec3,
}

impl Trs {
    fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.s, self.r, self.t)
    }
}

/// Cuadros por segundo con los que caen las keys del archivo (30 si ninguno)
fn detect_fps(anim: &Animation, start: f32) -> f32 {
    let times: Vec<f32> = anim.channels.iter().flat_map(|c| c.times.iter().map(|t| t - start)).collect();
    [24.0f32, 30.0, 25.0, 60.0]
        .into_iter()
        .find(|fps| times.iter().all(|t| ((t * fps) - (t * fps).round()).abs() < 0.02))
        .unwrap_or(30.0)
}

fn sample_clip(
    scene: &Scene,
    anim: &Animation,
    index: usize,
    rig: &SampleRig,
    skel: &converter_scene::Skeleton,
) -> Option<ClipDto> {
    let start = anim.channels.iter().flat_map(|c| c.times.first()).copied().reduce(f32::min)?;
    let end = anim.channels.iter().flat_map(|c| c.times.last()).copied().reduce(f32::max)?;
    let fps = detect_fps(anim, start);
    let frames = ((end - start) * fps).round().max(0.0) as usize;

    let pose = NodePose::new(scene, rig.node_parent);

    // Giro local (y desplazamiento de las raíces) de cada articulación por cuadro
    let num = rig.joints.len();
    let mut rotations: Vec<Vec<Quat>> = vec![Vec::with_capacity(frames + 1); num];
    let mut translations: Vec<Vec<Vec3>> = vec![Vec::with_capacity(frames + 1); num];
    for frame in 0..=frames {
        let world = pose.world(anim, start + frame as f32 / fps);
        // Lo que el skin le hace a lo que sigue a cada articulación
        let delta: Vec<Mat4> = rig
            .joints
            .iter()
            .map(|j| match j.source.and_then(|s| Some((s, skel.joints[s].node_index?))) {
                Some((s, node)) if node < world.len() => world[node] * rig.inverse_bind[s],
                _ => Mat4::IDENTITY,
            })
            .collect();
        let rotation = |a: usize| delta[a].to_scale_rotation_translation().1.normalize();
        for (a, joint) in rig.joints.iter().enumerate() {
            let q = match joint.parent {
                Some(p) => rotation(p).inverse() * rotation(a),
                None => rotation(a),
            };
            rotations[a].push(q.normalize());
            if joint.parent.is_none() {
                translations[a].push(delta[a].transform_point3(joint.position) - joint.position);
            }
        }
    }

    let rot_tol = 2e-3;
    let pos_tol = 1e-4 * rig.diag;
    let animatable = |a: usize| rig.joints[a].parent.is_none() || rig.joints.iter().any(|j| j.parent == Some(a));
    let tracks: Vec<TrackDto> = (0..num)
        .filter(|&a| animatable(a))
        .filter_map(|a| {
            let rot = &rotations[a];
            let moves = rot.iter().any(|q| q.angle_between(Quat::IDENTITY) > rot_tol)
                || translations[a].iter().any(|t| t.length() > pos_tol);
            if !moves {
                return None;
            }
            let rotation = reduce(rot, |a, b, t| a.slerp(*b, t), |a, b| a.angle_between(*b) <= rot_tol)
                .into_iter()
                .map(|f| key(f, rot[f].to_array()))
                .collect();
            let tr = &translations[a];
            let translation = if tr.iter().any(|t| t.length() > pos_tol) {
                reduce(tr, |a, b, t| a.lerp(*b, t), |a, b| a.distance(*b) <= pos_tol)
                    .into_iter()
                    .map(|f| key(f, tr[f].to_array()))
                    .collect()
            } else {
                Vec::new()
            };
            Some(TrackDto { bone: rig.joints[a].name.clone(), rotation, translation })
        })
        .collect();

    let name = if anim.name.trim().is_empty() { format!("Animación {}", index + 1) } else { anim.name.clone() };
    Some(ClipDto { name, fps, start: 0.0, end: frames.max(1) as f32, tracks })
}

fn key<T>(frame: usize, value: T) -> Key<T> {
    Key { frame: frame as f32, value, interpolation: KeyInterpolation::Linear }
}

/// Cuadros que hay que conservar para que la interpolación lineal entre
/// ellos reproduzca todos los demás
fn reduce<T>(values: &[T], mix: impl Fn(&T, &T, f32) -> T, close: impl Fn(&T, &T) -> bool) -> Vec<usize> {
    if values.len() <= 2 {
        return (0..values.len()).collect();
    }
    let mut kept = vec![0];
    let mut anchor = 0;
    let mut candidate = 1;
    while candidate + 1 < values.len() {
        let next = candidate + 1;
        let fits = (anchor + 1..next).all(|i| {
            let t = (i - anchor) as f32 / (next - anchor) as f32;
            close(&mix(&values[anchor], &values[next], t), &values[i])
        });
        if !fits {
            kept.push(candidate);
            anchor = candidate;
        }
        candidate = next;
    }
    kept.push(values.len() - 1);
    // Una pista quieta queda en una sola key
    if kept.len() == 2 && close(&values[0], &values[values.len() - 1]) && (1..values.len()).all(|i| close(&values[0], &values[i])) {
        kept.pop();
    }
    kept
}

/// Transformaciones de mundo de los nodos en un instante de una animación
struct NodePose<'a> {
    base: Vec<Trs>,
    /// Nodos de padres a hijos
    order: Vec<usize>,
    node_parent: &'a [Option<usize>],
}

impl<'a> NodePose<'a> {
    fn new(scene: &Scene, node_parent: &'a [Option<usize>]) -> Self {
        let base = scene
            .nodes
            .iter()
            .map(|n| match n.transform {
                Transform::Trs { translation, rotation, scale } => Trs { t: translation, r: rotation, s: scale },
                Transform::Matrix(m) => {
                    let (s, r, t) = m.to_scale_rotation_translation();
                    Trs { t, r, s }
                }
            })
            .collect();
        let mut order = Vec::with_capacity(scene.nodes.len());
        let mut seen = vec![false; scene.nodes.len()];
        let mut stack: Vec<usize> = (0..scene.nodes.len()).filter(|&n| node_parent[n].is_none()).rev().collect();
        while let Some(n) = stack.pop() {
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            order.push(n);
            stack.extend(scene.nodes[n].children.iter().rev().copied().filter(|&c| node_parent.get(c) == Some(&Some(n))));
        }
        Self { base, order, node_parent }
    }

    fn world(&self, anim: &Animation, time: f32) -> Vec<Mat4> {
        let mut local = self.base.clone();
        for channel in &anim.channels {
            if let Some(trs) = local.get_mut(channel.node) {
                apply_channel(channel, time, trs);
            }
        }
        let mut world = vec![Mat4::IDENTITY; local.len()];
        for &n in &self.order {
            world[n] = self.node_parent[n].map_or(Mat4::IDENTITY, |p| world[p]) * local[n].matrix();
        }
        world
    }
}

/// Valor del canal en `time` sobre el TRS del nodo
fn apply_channel(channel: &converter_scene::Channel, time: f32, trs: &mut Trs) {
    let times = &channel.times;
    if times.is_empty() {
        return;
    }
    let cubic = channel.interpolation == Interpolation::CubicSpline;
    let stride = if cubic { 3 } else { 1 };
    // Segmento [k, k+1] y avance s ∈ [0, 1]
    let (k, s) = match times.iter().position(|&t| t > time) {
        Some(0) => (0, 0.0),
        None => (times.len() - 1, 0.0),
        Some(i) => {
            let span = times[i] - times[i - 1];
            (i - 1, if span > 0.0 { (time - times[i - 1]) / span } else { 0.0 })
        }
    };
    let last = k + 1 >= times.len() || s == 0.0;
    let dt = if last { 0.0 } else { times[k + 1] - times[k] };
    let s = match channel.interpolation {
        Interpolation::Step => 0.0,
        _ if last => 0.0,
        _ => s,
    };

    fn at<T: Copy>(values: &[T], stride: usize, i: usize, part: usize) -> Option<T> {
        values.get(i * stride + if stride == 3 { part } else { 0 }).copied()
    }
    // Hermite con tangentes (entrada, valor, salida) de glTF
    fn hermite(v0: [f32; 4], out0: [f32; 4], v1: [f32; 4], in1: [f32; 4], s: f32, dt: f32) -> [f32; 4] {
        let (s2, s3) = (s * s, s * s * s);
        let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
        std::array::from_fn(|i| h00 * v0[i] + h10 * dt * out0[i] + h01 * v1[i] + h11 * dt * in1[i])
    }
    let vec4 = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];

    match &channel.values {
        KeyframeValues::Translation(v) | KeyframeValues::Scale(v) => {
            let Some(a) = at(v, stride, k, 1) else { return };
            let value = if s == 0.0 {
                a
            } else if let Some(b) = at(v, stride, k + 1, 1) {
                if cubic {
                    let out0 = at(v, stride, k, 2).unwrap_or([0.0; 3]);
                    let in1 = at(v, stride, k + 1, 0).unwrap_or([0.0; 3]);
                    let h = hermite(vec4(a), vec4(out0), vec4(b), vec4(in1), s, dt);
                    [h[0], h[1], h[2]]
                } else {
                    Vec3::from(a).lerp(Vec3::from(b), s).to_array()
                }
            } else {
                a
            };
            if matches!(channel.values, KeyframeValues::Translation(_)) {
                trs.t = Vec3::from(value);
            } else {
                trs.s = Vec3::from(value);
            }
        }
        KeyframeValues::Rotation(v) => {
            let Some(a) = at(v, stride, k, 1) else { return };
            let q = if s == 0.0 {
                Quat::from_array(a)
            } else if let Some(b) = at(v, stride, k + 1, 1) {
                if cubic {
                    let out0 = at(v, stride, k, 2).unwrap_or([0.0; 4]);
                    let in1 = at(v, stride, k + 1, 0).unwrap_or([0.0; 4]);
                    Quat::from_array(hermite(a, out0, b, in1, s, dt))
                } else {
                    Quat::from_array(a).slerp(Quat::from_array(b), s)
                }
            } else {
                Quat::from_array(a)
            };
            trs.r = q.normalize();
        }
        KeyframeValues::Weights(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{Channel, IndexData, Joint, Mesh, Node, Primitive, Skeleton};
    use pinocchio_skeleton::Skeleton as _;

    #[test]
    fn reduce_keeps_only_corners() {
        let values: Vec<f32> = vec![0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 1.0];
        let kept = reduce(&values, |a, b, t| a + (b - a) * t, |a, b| (a - b).abs() < 1e-4);
        assert_eq!(kept, vec![0, 3, 5, 6]);
        let still = reduce(&[1.0f32; 5], |a, b, t| a + (b - a) * t, |a, b| (a - b).abs() < 1e-4);
        assert_eq!(still, vec![0]);
    }

    /// Vértices posados por el skin del archivo: Σ w · mundo(J, t) · IBM_J · v
    fn file_pose(scene: &Scene, anim: &Animation, time: f32) -> Vec<Vec3> {
        let parents = node_parents(scene);
        let world = NodePose::new(scene, &parents).world(anim, time);
        let mut out = Vec::new();
        for prim in scene.world_primitives() {
            let attributes = &scene.meshes[prim.mesh].primitives[prim.primitive].attributes;
            let skel = &scene.skeletons[scene.nodes[prim.node.unwrap()].skin.unwrap()];
            let indices = attributes.iter().find_map(|a| if let VertexAttribute::JointIndices(v) = a { Some(v) } else { None }).unwrap();
            let weights = attributes.iter().find_map(|a| if let VertexAttribute::JointWeights(v) = a { Some(v) } else { None }).unwrap();
            for (v, &p) in prim.positions.iter().enumerate() {
                let mut posed = Vec3::ZERO;
                let total: f32 = weights[v].iter().sum();
                for k in 0..4 {
                    let joint = &skel.joints[indices[v][k] as usize];
                    let m = world[joint.node_index.unwrap()] * joint.inverse_bind_matrix;
                    posed += m.transform_point3(Vec3::from(p)) * (weights[v][k] / total);
                }
                out.push(posed);
            }
        }
        out
    }

    fn sample<T: Copy>(keys: &[Key<T>], frame: f32, mix: impl Fn(T, T, f32) -> T) -> Option<T> {
        let first = keys.first()?;
        if frame <= first.frame {
            return Some(first.value);
        }
        for w in keys.windows(2) {
            if frame < w[1].frame {
                return Some(mix(w[0].value, w[1].value, (frame - w[0].frame) / (w[1].frame - w[0].frame)));
            }
        }
        keys.last().map(|k| k.value)
    }

    /// Vértices posados por la app, como el visor: un hueso por articulación
    /// en la cabeza de su segmento; el giro de `J` va a los huesos de sus hijos
    fn app_pose(rig: &ImportedRig, clip: &ClipDto, frame: f32) -> Vec<Vec3> {
        let bones = rig.skeleton.bones();
        let pos = |b: usize| {
            let p = bones[b].position;
            Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32)
        };
        let head = |b: usize| pos(bones[b].parent.unwrap_or(b));
        let mut rotation = vec![Quat::IDENTITY; bones.len()];
        let mut offset = vec![Vec3::ZERO; bones.len()];
        for track in &clip.tracks {
            let j = bones.iter().position(|b| b.name == track.bone).unwrap();
            let q = sample(&track.rotation, frame, |a, b, t| Quat::from_array(a).slerp(Quat::from_array(b), t).to_array());
            if let Some(q) = q {
                for b in 0..bones.len() {
                    if (bones[j].parent.is_none() && b == j) || (bones[j].parent.is_some() && bones[b].parent == Some(j)) {
                        rotation[b] = Quat::from_array(q);
                    }
                }
            }
            if let Some(t) = sample(&track.translation, frame, |a, b, t| Vec3::from(a).lerp(Vec3::from(b), t).to_array()) {
                offset[j] = Vec3::from(t);
            }
        }
        let mut matrix = vec![Mat4::IDENTITY; bones.len()];
        for b in 0..bones.len() {
            let local = match bones[b].parent {
                Some(p) => Mat4::from_rotation_translation(rotation[b], head(b) - head(p)),
                None => Mat4::from_rotation_translation(rotation[b], head(b) + offset[b]),
            };
            matrix[b] = bones[b].parent.map_or(Mat4::IDENTITY, |p| matrix[p]) * local;
        }
        let a = &rig.output.attachment;
        (0..a.num_vertices())
            .map(|v| {
                let r = a.rest_positions()[v];
                let r = Vec3::new(r.x() as f32, r.y() as f32, r.z() as f32);
                a.get_weights(v)
                    .iter()
                    .enumerate()
                    .filter(|&(_, &w)| w > 0.0)
                    .map(|(b, &w)| (matrix[b] * Mat4::from_translation(-head(b))).transform_point3(r) * w as f32)
                    .sum()
            })
            .collect()
    }

    /// Error máximo entre las dos poses, relativo al tamaño del modelo
    fn max_pose_error(scene: &Scene) -> f32 {
        let mesh = pinocchio_mesh::scene_to_mesh(scene).unwrap();
        let rig = from_scene(scene, mesh.num_vertices()).unwrap();
        assert_eq!(rig.clips.len(), scene.animations.len());
        let (min, max) = scene.compute_bounding_box().unwrap();
        let size = (Vec3::from(max) - Vec3::from(min)).length();
        let mut worst = 0.0f32;
        for (anim, clip) in scene.animations.iter().zip(&rig.clips) {
            let start = anim.channels.iter().flat_map(|c| c.times.first()).copied().reduce(f32::min).unwrap();
            for frame in (0..=clip.end as usize).step_by(3) {
                let file = file_pose(scene, anim, start + frame as f32 / clip.fps);
                let app = app_pose(&rig, clip, frame as f32);
                let err = file.iter().zip(&app).map(|(a, b)| a.distance(*b)).fold(0.0, f32::max);
                worst = worst.max(err / size);
            }
        }
        worst
    }

    /// Brazo de tres joints bajo un nodo de armadura girado y escalado (como
    /// los de Mixamo), con giros de reposo, raíz que se desplaza y codo que gira
    fn arm_scene() -> Scene {
        let armature = Mat4::from_scale_rotation_translation(
            Vec3::splat(0.01),
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            Vec3::new(0.0, 0.0, 0.5),
        );
        let locals = [
            Mat4::from_rotation_translation(Quat::from_rotation_z(0.3), Vec3::ZERO),
            Mat4::from_rotation_translation(Quat::from_rotation_x(0.2), Vec3::new(0.0, 100.0, 0.0)),
            Mat4::from_rotation_translation(Quat::IDENTITY, Vec3::new(0.0, 80.0, 0.0)),
        ];
        let rest = [armature * locals[0], armature * locals[0] * locals[1], armature * locals[0] * locals[1] * locals[2]];
        let trs = |m: Mat4| {
            let (s, r, t) = m.to_scale_rotation_translation();
            Transform::Trs { translation: t, rotation: r, scale: s }
        };

        // Un anillo de vértices cada tanto a lo largo del brazo, en mundo
        let mut positions = Vec::new();
        let mut joints = Vec::new();
        let mut weights = Vec::new();
        for step in 0..=12 {
            let along = step as f32 * 20.0;
            let (bone, local_y) = if along < 100.0 { (0, along) } else if along < 180.0 { (1, along - 100.0) } else { (2, along - 180.0) };
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_2;
                let p = rest[bone].transform_point3(Vec3::new(10.0 * a.cos(), local_y, 10.0 * a.sin()));
                positions.push(p.to_array());
                let blend = if bone < 2 && local_y > 60.0 { 0.3 } else { 0.0 };
                joints.push([bone as u16, (bone + 1).min(2) as u16, 0, 0]);
                weights.push([1.0 - blend, blend, 0.0, 0.0]);
            }
        }
        let rings = positions.len() / 4 - 1;
        let indices: Vec<u32> = (0..rings as u32)
            .flat_map(|r| (0..4u32).flat_map(move |k| {
                let (a, b, c, d) = (r * 4 + k, r * 4 + (k + 1) % 4, r * 4 + 4 + k, r * 4 + 4 + (k + 1) % 4);
                [a, b, c, b, d, c]
            }))
            .collect();

        let joint_nodes = [1usize, 2, 3];
        let names = ["hombro", "codo", "muñeca"];
        let skeleton = Skeleton {
            name: "brazo".into(),
            joints: (0..3)
                .map(|j| Joint {
                    name: names[j].into(),
                    children: if j < 2 { vec![j + 1] } else { vec![] },
                    inverse_bind_matrix: rest[j].inverse(),
                    local_transform: locals[j],
                    node_index: Some(joint_nodes[j]),
                })
                .collect(),
            roots: vec![0],
        };
        let node = |name: &str, transform, children| Node { name: name.into(), transform, mesh: None, skin: None, children };
        let mut scene = Scene::default();
        scene.nodes = vec![
            node("Armature", trs(armature), vec![1, 4]),
            node("hombro", trs(locals[0]), vec![2]),
            node("codo", trs(locals[1]), vec![3]),
            node("muñeca", trs(locals[2]), vec![]),
            Node { name: "malla".into(), transform: Transform::identity(), mesh: Some(0), skin: Some(0), children: vec![] },
        ];
        scene.root_nodes = vec![0];
        scene.meshes = vec![Mesh {
            name: "brazo".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(positions),
                    VertexAttribute::JointIndices(joints),
                    VertexAttribute::JointWeights(weights),
                ],
                indices: Some(IndexData::U32(indices)),
                material: None,
            }],
        }];
        scene.skeletons = vec![skeleton];
        let bend = |a: f32| (Quat::from_rotation_x(0.2) * Quat::from_rotation_z(a)).to_array();
        scene.animations = vec![Animation {
            name: "saludo".into(),
            channels: vec![
                Channel {
                    node: 2,
                    interpolation: Interpolation::Linear,
                    times: vec![0.0, 0.5, 1.0],
                    values: KeyframeValues::Rotation(vec![bend(0.0), bend(1.2), bend(0.0)]),
                },
                Channel {
                    node: 1,
                    interpolation: Interpolation::Linear,
                    times: vec![0.0, 1.0],
                    values: KeyframeValues::Translation(vec![[0.0; 3], [30.0, 0.0, 0.0]]),
                },
            ],
        }];
        scene
    }

    #[test]
    fn arm_rig_matches_file_skinning() {
        let scene = arm_scene();
        let mesh = pinocchio_mesh::scene_to_mesh(&scene).unwrap();
        let rig = from_scene(&scene, mesh.num_vertices()).unwrap();
        let names: Vec<&str> = rig.skeleton.bones().iter().map(|b| b.name.as_str()).collect();
        // La muñeca mueve vértices: le toca una punta
        assert_eq!(names, vec!["hombro", "codo", "muñeca", "muñeca_fin"]);
        let clip = &rig.clips[0];
        assert_eq!((clip.name.as_str(), clip.fps, clip.end), ("saludo", 24.0, 24.0));
        // El codo gira en tres keys (lineal entre ellas); la muñeca no se mueve
        let codo = clip.tracks.iter().find(|t| t.bone == "codo").unwrap();
        assert!(codo.rotation.len() <= 5, "{}", codo.rotation.len());
        assert!(clip.tracks.iter().all(|t| t.bone != "muñeca"));

        let err = max_pose_error(&scene);
        assert!(err < 2e-3, "error relativo {err}");
    }

    /// Como el Fox de Khronos: un joint "root" sin pesos encima del hombro, que
    /// es el que se traslada
    #[test]
    fn unweighted_root_above_translated_joint_is_dropped() {
        let mut scene = arm_scene();
        let armature = scene.nodes[0].transform.to_matrix();
        scene.nodes[0].children = vec![5, 4];
        scene.nodes.push(Node {
            name: "root".into(),
            transform: Transform::identity(),
            mesh: None,
            skin: None,
            children: vec![1],
        });
        scene.skeletons[0].joints.push(Joint {
            name: "root".into(),
            children: vec![0],
            inverse_bind_matrix: armature.inverse(),
            local_transform: Mat4::IDENTITY,
            node_index: Some(5),
        });
        scene.skeletons[0].roots = vec![3];
        let mesh = pinocchio_mesh::scene_to_mesh(&scene).unwrap();
        let rig = from_scene(&scene, mesh.num_vertices()).unwrap();
        assert_eq!(rig.skeleton.bones()[0].name, "hombro");
        assert!(rig.skeleton.bones().iter().all(|b| b.name != "root"));
        let err = max_pose_error(&scene);
        assert!(err < 2e-3, "error relativo {err}");
    }

    #[test]
    fn scene_without_skin_has_no_rig() {
        let mut scene = arm_scene();
        scene.nodes[4].skin = None;
        let mesh = pinocchio_mesh::scene_to_mesh(&scene).unwrap();
        assert!(from_scene(&scene, mesh.num_vertices()).is_none());
    }

    /// Modelos reales: `PINOCCHIO_RIG_SAMPLES=dir cargo test ... samples -- --nocapture`
    #[test]
    fn samples() {
        let Ok(dir) = std::env::var("PINOCCHIO_RIG_SAMPLES") else { return };
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let scene = converter_gltf_io::import_gltf(&path).unwrap();
            let mesh = pinocchio_mesh::scene_to_mesh(&scene).unwrap();
            let t = std::time::Instant::now();
            let rig = from_scene(&scene, mesh.num_vertices()).unwrap();
            let keys: usize = rig.clips.iter().flat_map(|c| &c.tracks).map(|t| t.rotation.len() + t.translation.len()).sum();
            println!(
                "{}: {} huesos, {} clips {:?}, {} keys, {:?}, error {:.2e}",
                path.display(),
                rig.skeleton.num_bones(),
                rig.clips.len(),
                rig.clips.iter().map(|c| (c.name.as_str(), c.fps, c.end)).collect::<Vec<_>>(),
                keys,
                t.elapsed(),
                max_pose_error(&scene)
            );
        }
    }
}
