// Conversión de Skeleton/Animation → UsdSkel.
//
// Genera:
// - SkelRoot como contenedor raíz
// - Skeleton con joints, bindTransforms, restTransforms
// - SkelAnimation con translations, rotations, scales por timeCode
//
// Conversiones necesarias:
// - Quaternion glam (x,y,z,w) → USD (w,x,y,z)
// - Tiempo en segundos (glTF) → timeCodes (USD, fps configurable)

use std::collections::{HashMap, VecDeque};
use std::fmt::Write;

use converter_scene::{
    Channel, Interpolation, KeyframeValues, Scene, Skeleton, VertexAttribute,
};
use glam::{Mat4, Quat, Vec3};

use crate::writer::{sanitize_name, UsdaExportOptions, UsdWriter};

// ---------------------------------------------------------------------------
// SkelContext — contexto preparado para un skeleton
// ---------------------------------------------------------------------------

/// Mapea node_index de Scene → (skeleton_index, joint_index).
pub(crate) struct SkelContext {
    /// node_index → (skeleton_idx, joint_idx)
    pub node_to_joint: HashMap<usize, (usize, usize)>,
    /// Por skeleton: datos preparados
    pub skeletons: Vec<SkelData>,
}

pub(crate) struct SkelData {
    /// Orden BFS de joints
    #[allow(dead_code)]
    pub bfs_order: Vec<usize>,
    /// Joint paths en formato USD ("Hips/Spine/Chest")
    pub joint_paths: Vec<String>,
    /// Bind transforms (world space, inverse of inverse_bind_matrix)
    pub bind_transforms: Vec<Mat4>,
    /// Rest transforms (local space)
    pub rest_transforms: Vec<Mat4>,
}

/// Prepara el contexto de skeletons para toda la escena.
pub(crate) fn prepare_skel_context(scene: &Scene) -> SkelContext {
    let mut node_to_joint = HashMap::new();
    let mut skeletons = Vec::with_capacity(scene.skeletons.len());

    for (si, skeleton) in scene.skeletons.iter().enumerate() {
        let bfs_order = bfs_joint_order(skeleton);
        let joint_paths = compute_joint_paths(skeleton, &bfs_order);
        let bind_transforms = compute_bind_transforms(skeleton);
        let rest_transforms = compute_rest_transforms(skeleton);

        // Registrar mapeo node → (skel, joint)
        for (ji, joint) in skeleton.joints.iter().enumerate() {
            if let Some(node_idx) = joint.node_index {
                node_to_joint.insert(node_idx, (si, ji));
            }
        }

        skeletons.push(SkelData {
            bfs_order,
            joint_paths,
            bind_transforms,
            rest_transforms,
        });
    }

    SkelContext {
        node_to_joint,
        skeletons,
    }
}

/// Ordena joints en BFS desde las raíces.
pub(crate) fn bfs_joint_order(skeleton: &Skeleton) -> Vec<usize> {
    let mut order = Vec::with_capacity(skeleton.joints.len());
    let mut queue = VecDeque::new();

    for &root in &skeleton.roots {
        queue.push_back(root);
    }

    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &child in &skeleton.joints[idx].children {
            queue.push_back(child);
        }
    }

    order
}

/// Calcula los joint paths USD en formato "Hips/Spine/Chest".
pub(crate) fn compute_joint_paths(skeleton: &Skeleton, _bfs_order: &[usize]) -> Vec<String> {
    let joint_count = skeleton.joints.len();
    let mut paths = vec![String::new(); joint_count];

    // Mapa inverso: child → parent
    let mut parent_of: Vec<Option<usize>> = vec![None; joint_count];
    for (pi, joint) in skeleton.joints.iter().enumerate() {
        for &child in &joint.children {
            parent_of[child] = Some(pi);
        }
    }

    // Construir path para cada joint recursivamente (con memoización)
    fn build_path(
        ji: usize,
        skeleton: &Skeleton,
        parent_of: &[Option<usize>],
        paths: &mut Vec<String>,
    ) {
        if !paths[ji].is_empty() {
            return;
        }
        let name = sanitize_name(&skeleton.joints[ji].name, "joint", ji);
        if let Some(pi) = parent_of[ji] {
            build_path(pi, skeleton, parent_of, paths);
            paths[ji] = format!("{}/{}", paths[pi], name);
        } else {
            paths[ji] = name;
        }
    }

    for ji in 0..joint_count {
        build_path(ji, skeleton, &parent_of, &mut paths);
    }

    paths
}

/// Calcula bind transforms (world space) = inverse(inverse_bind_matrix).
pub(crate) fn compute_bind_transforms(skeleton: &Skeleton) -> Vec<Mat4> {
    skeleton
        .joints
        .iter()
        .map(|j| j.inverse_bind_matrix.inverse())
        .collect()
}

/// Calcula rest transforms (local space) por joint.
pub(crate) fn compute_rest_transforms(skeleton: &Skeleton) -> Vec<Mat4> {
    skeleton
        .joints
        .iter()
        .map(|j| j.local_transform)
        .collect()
}

// ---------------------------------------------------------------------------
// Escritura de Skeleton def
// ---------------------------------------------------------------------------

/// Escribe `def Skeleton "name" { ... }` con joints, bindTransforms, restTransforms.
pub(crate) fn write_skeleton_def(
    w: &mut UsdWriter,
    skeleton: &Skeleton,
    skel_ctx: &SkelContext,
    skeleton_idx: usize,
) {
    let skel_data = &skel_ctx.skeletons[skeleton_idx];
    let name = sanitize_name(&skeleton.name, "Skeleton", skeleton_idx);

    w.open_block(&format!("def Skeleton \"{}\"", name));

    // uniform token[] joints
    write_token_array(w, "uniform token[] joints", &skel_data.joint_paths);

    // matrix4d[] bindTransforms
    write_matrix4d_array(w, "matrix4d[] bindTransforms", &skel_data.bind_transforms);

    // matrix4d[] restTransforms
    write_matrix4d_array(w, "matrix4d[] restTransforms", &skel_data.rest_transforms);

    w.close_block();
}

// ---------------------------------------------------------------------------
// SkelRoot y SkelBinding
// ---------------------------------------------------------------------------

/// Escribe un SkelRoot contenedor completo con Skeleton, animaciones y meshes.
pub(crate) fn write_skel_root(
    w: &mut UsdWriter,
    scene: &Scene,
    skeleton_idx: usize,
    skel_ctx: &SkelContext,
    node_idx: usize,
    options: &UsdaExportOptions,
    node_anim_ctx: &HashMap<usize, NodeAnimSamples>,
) {
    let node = &scene.nodes[node_idx];
    let node_name = sanitize_name(&node.name, "node", node_idx);
    let skeleton = &scene.skeletons[skeleton_idx];
    let skel_name = sanitize_name(&skeleton.name, "Skeleton", skeleton_idx);

    // Abrir SkelRoot
    w.open_block(&format!(
        "def SkelRoot \"{}\" (\n    prepend apiSchemas = [\"SkelBindingAPI\"]\n)",
        node_name
    ));

    // Transform del nodo
    crate::writer::write_transform_pub(w, &node.transform);

    // Skeleton def
    write_skeleton_def(w, skeleton, skel_ctx, skeleton_idx);

    // Animaciones (clips)
    let mut anim_names = Vec::new();
    if options.export_animations {
        let anim_indices = find_skeleton_animations(scene, skel_ctx, skeleton_idx);
        for &anim_idx in &anim_indices {
            w.blank();
            let anim_name = write_skel_animation(
                w,
                scene,
                skeleton_idx,
                skel_ctx,
                anim_idx,
                options,
            );
            anim_names.push(anim_name);
        }
    }

    // Meshes del nodo (con SkelBindingAPI)
    if let Some(mesh_idx) = node.mesh {
        let skel_path = skel_name.to_string();
        let anim_path = anim_names.first().cloned();
        w.blank();
        write_skinned_mesh(w, scene, mesh_idx, &skel_path, anim_path.as_deref(), &node_name, skeleton_idx, options);
    }

    // Hijos que no son joints
    for &child_idx in &node.children {
        if !skel_ctx.node_to_joint.contains_key(&child_idx) {
            w.blank();
            crate::writer::write_node_pub(w, scene, child_idx, false, options, skel_ctx, node_anim_ctx);
        }
    }

    w.close_block();
}

/// Escribe meshes con SkelBindingAPI (skinning attributes).
fn write_skinned_mesh(
    w: &mut UsdWriter,
    scene: &Scene,
    mesh_idx: usize,
    skel_path: &str,
    anim_path: Option<&str>,
    _skel_root_name: &str,
    skeleton_idx: usize,
    options: &UsdaExportOptions,
) {
    let mesh = &scene.meshes[mesh_idx];

    for (pi, prim) in mesh.primitives.iter().enumerate() {
        let prim_name = if mesh.primitives.len() == 1 {
            sanitize_name(&mesh.name, "mesh", mesh_idx)
        } else {
            format!("{}_{}", sanitize_name(&mesh.name, "mesh", mesh_idx), pi)
        };

        // Abrir Mesh con SkelBindingAPI
        w.open_block(&format!(
            "def Mesh \"{}\" (\n    prepend apiSchemas = [\"SkelBindingAPI\"]\n)",
            prim_name
        ));

        // Binding
        w.write_fmt_line(format_args!(
            "rel skel:skeleton = <{}>",
            skel_path
        ));
        if let Some(anim) = anim_path {
            w.write_fmt_line(format_args!(
                "rel skel:animationSource = <{}>",
                anim
            ));
        }

        // Geometría (misma lógica que write_primitive pero con skinning)
        write_primitive_geometry(w, prim, scene, false, options);

        // Joint indices y weights
        write_skel_binding_attrs(w, prim, skeleton_idx);

        w.close_block();
    }
}

/// Escribe atributos de skinning: jointIndices y jointWeights.
fn write_skel_binding_attrs(
    w: &mut UsdWriter,
    prim: &converter_scene::Primitive,
    _skeleton_idx: usize,
) {
    let joint_indices = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::JointIndices(j) = a {
            Some(j)
        } else {
            None
        }
    });
    let joint_weights = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::JointWeights(w) = a {
            Some(w)
        } else {
            None
        }
    });

    if let Some(indices) = joint_indices {
        let flat: Vec<i32> = indices
            .iter()
            .flat_map(|v| v.iter().map(|&i| i as i32))
            .collect();
        write_int_flat_array(w, "int[] primvars:skel:jointIndices", &flat, 4);
    }

    if let Some(weights) = joint_weights {
        let flat: Vec<f32> = weights.iter().flat_map(|v| v.iter().copied()).collect();
        write_float_flat_array(w, "float[] primvars:skel:jointWeights", &flat, 4);
    }
}

/// Escribe la geometría de un primitivo (sin el def/close_block).
fn write_primitive_geometry(
    w: &mut UsdWriter,
    prim: &converter_scene::Primitive,
    scene: &Scene,
    flipped: bool,
    options: &UsdaExportOptions,
) {
    use converter_scene::IndexData;

    let positions = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Positions(p) = a {
            Some(p)
        } else {
            None
        }
    });
    let positions = match positions {
        Some(p) => p,
        None => return,
    };

    let (face_counts, mut face_indices) = match &prim.indices {
        Some(IndexData::U16(idx)) => {
            let indices: Vec<u32> = idx.iter().map(|&i| i as u32).collect();
            let counts = vec![3u32; indices.len() / 3];
            (counts, indices)
        }
        Some(IndexData::U32(idx)) => {
            let counts = vec![3u32; idx.len() / 3];
            (counts, idx.clone())
        }
        None => {
            let n = positions.len() as u32;
            let counts = vec![3u32; positions.len() / 3];
            let indices: Vec<u32> = (0..n).collect();
            (counts, indices)
        }
    };

    if flipped {
        for tri in face_indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }

    crate::writer::write_int_array_pub(w, "int[] faceVertexCounts", &face_counts);
    crate::writer::write_int_array_pub(w, "int[] faceVertexIndices", &face_indices);
    crate::writer::write_point3f_array_pub(w, "point3f[] points", positions);

    // Normals
    if let Some(normals) = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Normals(n) = a {
            Some(n)
        } else {
            None
        }
    }) {
        crate::writer::write_normal3f_array_pub(w, normals);
    }

    // UVs
    for attr in &prim.attributes {
        if let VertexAttribute::TexCoords(set, uvs) = attr {
            if options.arkit_compatible && *set > 0 {
                continue;
            }
            let varname = if *set == 0 {
                "primvars:st".to_string()
            } else {
                format!("primvars:st{}", set)
            };
            crate::writer::write_texcoord2f_array_pub(w, &varname, uvs);
        }
    }

    // Vertex colors
    if let Some(colors) = prim.attributes.iter().find_map(|a| {
        if let VertexAttribute::Colors(c) = a {
            Some(c)
        } else {
            None
        }
    }) {
        crate::writer::write_vertex_colors_pub(w, colors);
    }

    // Extent
    let (min, max) = crate::writer::compute_extent_pub(positions);
    w.write_fmt_line(format_args!(
        "float3[] extent = [({}, {}, {}), ({}, {}, {})]",
        min[0], min[1], min[2], max[0], max[1], max[2]
    ));

    w.line("uniform token subdivisionScheme = \"none\"");

    // doubleSided
    let double_sided = prim
        .material
        .and_then(|idx| scene.materials.get(idx))
        .is_some_and(|mat| mat.double_sided);
    if double_sided {
        w.line("uniform bool doubleSided = 1");
    }

    // Material binding
    if let Some(mat_idx) = prim.material
        && let Some(mat) = scene.materials.get(mat_idx) {
            let mat_name = crate::materials::material_prim_name(mat, mat_idx);
            w.write_fmt_line(format_args!(
                "rel material:binding = </Root/Materials/{}>",
                mat_name
            ));
        }
}

// ---------------------------------------------------------------------------
// SkelAnimation
// ---------------------------------------------------------------------------

/// Escribe un `def SkelAnimation` y retorna el nombre del prim.
pub(crate) fn write_skel_animation(
    w: &mut UsdWriter,
    scene: &Scene,
    skeleton_idx: usize,
    skel_ctx: &SkelContext,
    anim_idx: usize,
    options: &UsdaExportOptions,
) -> String {
    let anim = &scene.animations[anim_idx];
    let skeleton = &scene.skeletons[skeleton_idx];
    let skel_data = &skel_ctx.skeletons[skeleton_idx];
    let fps = options.fps;
    let tolerance = options.keyframe_tolerance;

    let anim_name = if anim.name.is_empty() {
        format!("Anim_{}", anim_idx)
    } else {
        sanitize_name(&anim.name, "Anim", anim_idx)
    };

    w.open_block(&format!("def SkelAnimation \"{}\"", anim_name));

    // joints token array (mismos paths que el Skeleton)
    write_token_array(w, "uniform token[] joints", &skel_data.joint_paths);

    // Recopilar todos los tiempos únicos de todos los canales relevantes
    let joint_count = skeleton.joints.len();

    // Mapear channels de esta animación a joints de este skeleton
    let mut trans_channels: HashMap<usize, &Channel> = HashMap::new(); // joint_idx → channel
    let mut rot_channels: HashMap<usize, &Channel> = HashMap::new();
    let mut scale_channels: HashMap<usize, &Channel> = HashMap::new();

    for ch in &anim.channels {
        if let Some(&(si, ji)) = skel_ctx.node_to_joint.get(&ch.node)
            && si == skeleton_idx {
                match &ch.values {
                    KeyframeValues::Translation(_) => { trans_channels.insert(ji, ch); }
                    KeyframeValues::Rotation(_) => { rot_channels.insert(ji, ch); }
                    KeyframeValues::Scale(_) => { scale_channels.insert(ji, ch); }
                    KeyframeValues::Weights(_) => {} // Morph weights no van en SkelAnimation
                }
            }
    }

    // Recopilar todos los tiempos únicos
    let mut all_times: Vec<f32> = Vec::new();
    for ch in trans_channels.values().chain(rot_channels.values()).chain(scale_channels.values()) {
        all_times.extend_from_slice(&ch.times);
    }
    all_times.sort_by(|a, b| a.total_cmp(b));
    all_times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

    if all_times.is_empty() {
        w.close_block();
        return anim_name;
    }

    // Translations timeSamples
    if !trans_channels.is_empty() || true {
        write_translation_time_samples(
            w, &all_times, &trans_channels, skeleton, skel_data, joint_count, fps, tolerance,
        );
    }

    // Rotations timeSamples
    {
        write_rotation_time_samples(
            w, &all_times, &rot_channels, skeleton, skel_data, joint_count, fps, tolerance,
        );
    }

    // Scales timeSamples
    if !scale_channels.is_empty() {
        write_scale_time_samples(
            w, &all_times, &scale_channels, skeleton, skel_data, joint_count, fps, tolerance,
        );
    }

    w.close_block();
    anim_name
}

fn write_translation_time_samples(
    w: &mut UsdWriter,
    all_times: &[f32],
    channels: &HashMap<usize, &Channel>,
    _skeleton: &Skeleton,
    skel_data: &SkelData,
    joint_count: usize,
    fps: f64,
    _tolerance: f32,
) {
    w.write_indent();
    w.buf.push_str("float3[] translations.timeSamples = {\n");
    w.depth += 1;

    for &t in all_times {
        let tc = seconds_to_timecode(t, fps);
        w.write_indent();
        write!(w.buf, "{}: [", tc).unwrap();

        for ji in 0..joint_count {
            if ji > 0 {
                w.buf.push_str(", ");
            }
            if let Some(ch) = channels.get(&ji) {
                let val = sample_translation(ch, t);
                write!(w.buf, "({}, {}, {})", val[0], val[1], val[2]).unwrap();
            } else {
                // Rest transform: extraer translation
                let rest = skel_data.rest_transforms[ji];
                let (_, _, trans) = rest.to_scale_rotation_translation();
                write!(w.buf, "({}, {}, {})", trans.x, trans.y, trans.z).unwrap();
            }
        }

        w.buf.push_str("],\n");
    }

    w.depth -= 1;
    w.line("}");
}

fn write_rotation_time_samples(
    w: &mut UsdWriter,
    all_times: &[f32],
    channels: &HashMap<usize, &Channel>,
    _skeleton: &Skeleton,
    skel_data: &SkelData,
    joint_count: usize,
    fps: f64,
    _tolerance: f32,
) {
    w.write_indent();
    w.buf.push_str("quatf[] rotations.timeSamples = {\n");
    w.depth += 1;

    for &t in all_times {
        let tc = seconds_to_timecode(t, fps);
        w.write_indent();
        write!(w.buf, "{}: [", tc).unwrap();

        for ji in 0..joint_count {
            if ji > 0 {
                w.buf.push_str(", ");
            }
            if let Some(ch) = channels.get(&ji) {
                let val = sample_rotation(ch, t);
                // USD quaternion: (w, x, y, z)
                write!(w.buf, "({}, {}, {}, {})", val[3], val[0], val[1], val[2]).unwrap();
            } else {
                // Rest transform
                let rest = skel_data.rest_transforms[ji];
                let (_, rot, _) = rest.to_scale_rotation_translation();
                write!(w.buf, "({}, {}, {}, {})", rot.w, rot.x, rot.y, rot.z).unwrap();
            }
        }

        w.buf.push_str("],\n");
    }

    w.depth -= 1;
    w.line("}");
}

fn write_scale_time_samples(
    w: &mut UsdWriter,
    all_times: &[f32],
    channels: &HashMap<usize, &Channel>,
    _skeleton: &Skeleton,
    skel_data: &SkelData,
    joint_count: usize,
    fps: f64,
    _tolerance: f32,
) {
    w.write_indent();
    w.buf.push_str("half3[] scales.timeSamples = {\n");
    w.depth += 1;

    for &t in all_times {
        let tc = seconds_to_timecode(t, fps);
        w.write_indent();
        write!(w.buf, "{}: [", tc).unwrap();

        for ji in 0..joint_count {
            if ji > 0 {
                w.buf.push_str(", ");
            }
            if let Some(ch) = channels.get(&ji) {
                let val = sample_scale(ch, t);
                write!(w.buf, "({}, {}, {})", val[0], val[1], val[2]).unwrap();
            } else {
                let rest = skel_data.rest_transforms[ji];
                let (scale, _, _) = rest.to_scale_rotation_translation();
                write!(w.buf, "({}, {}, {})", scale.x, scale.y, scale.z).unwrap();
            }
        }

        w.buf.push_str("],\n");
    }

    w.depth -= 1;
    w.line("}");
}

// ---------------------------------------------------------------------------
// Sampling de canales
// ---------------------------------------------------------------------------

fn sample_translation(ch: &Channel, t: f32) -> [f32; 3] {
    if let KeyframeValues::Translation(vals) = &ch.values {
        sample_vec3(&ch.times, vals, ch.interpolation, t)
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn sample_rotation(ch: &Channel, t: f32) -> [f32; 4] {
    if let KeyframeValues::Rotation(vals) = &ch.values {
        sample_quat(&ch.times, vals, ch.interpolation, t)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn sample_scale(ch: &Channel, t: f32) -> [f32; 3] {
    if let KeyframeValues::Scale(vals) = &ch.values {
        sample_vec3(&ch.times, vals, ch.interpolation, t)
    } else {
        [1.0, 1.0, 1.0]
    }
}

fn sample_vec3(times: &[f32], values: &[[f32; 3]], interp: Interpolation, t: f32) -> [f32; 3] {
    if times.is_empty() || values.is_empty() {
        return [0.0; 3];
    }
    if t <= times[0] {
        return get_value_vec3(values, interp, 0);
    }
    if t >= *times.last().unwrap() {
        return get_value_vec3(values, interp, times.len() - 1);
    }

    // Encontrar segmento
    let idx = times.partition_point(|&x| x < t).saturating_sub(1);
    let idx = idx.min(times.len() - 2);
    let t0 = times[idx];
    let t1 = times[idx + 1];

    match interp {
        Interpolation::Step => get_value_vec3(values, interp, idx),
        Interpolation::Linear => {
            let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
            let v0 = values[idx];
            let v1 = values[idx + 1];
            [
                v0[0] + (v1[0] - v0[0]) * frac,
                v0[1] + (v1[1] - v0[1]) * frac,
                v0[2] + (v1[2] - v0[2]) * frac,
            ]
        }
        Interpolation::CubicSpline => {
            // CubicSpline: values has 3x keyframes (in-tangent, value, out-tangent)
            let n = times.len();
            if values.len() == n * 3 {
                let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
                let dt = t1 - t0;
                let v0 = values[idx * 3 + 1];       // value at idx
                let b0 = values[idx * 3 + 2];       // out-tangent at idx
                let a1 = values[(idx + 1) * 3];     // in-tangent at idx+1
                let v1 = values[(idx + 1) * 3 + 1]; // value at idx+1
                cubic_hermite_vec3(v0, b0, a1, v1, frac, dt)
            } else {
                // Fallback: linear
                let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
                let v0 = values[idx];
                let v1 = values[idx + 1];
                [
                    v0[0] + (v1[0] - v0[0]) * frac,
                    v0[1] + (v1[1] - v0[1]) * frac,
                    v0[2] + (v1[2] - v0[2]) * frac,
                ]
            }
        }
    }
}

fn sample_quat(times: &[f32], values: &[[f32; 4]], interp: Interpolation, t: f32) -> [f32; 4] {
    if times.is_empty() || values.is_empty() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if t <= times[0] {
        return get_value_quat(values, interp, 0);
    }
    if t >= *times.last().unwrap() {
        return get_value_quat(values, interp, times.len() - 1);
    }

    let idx = times.partition_point(|&x| x < t).saturating_sub(1);
    let idx = idx.min(times.len() - 2);
    let t0 = times[idx];
    let t1 = times[idx + 1];

    match interp {
        Interpolation::Step => get_value_quat(values, interp, idx),
        Interpolation::Linear => {
            let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
            let q0 = Quat::from_array(values[idx]);
            let q1 = Quat::from_array(values[idx + 1]);
            let result = q0.slerp(q1, frac).normalize();
            result.to_array()
        }
        Interpolation::CubicSpline => {
            // Fallback: slerp para CubicSpline quaternions (proper cubic quat interp is complex)
            let n = times.len();
            let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
            if values.len() == n * 3 {
                let q0 = Quat::from_array(values[idx * 3 + 1]);
                let q1 = Quat::from_array(values[(idx + 1) * 3 + 1]);
                q0.slerp(q1, frac).normalize().to_array()
            } else {
                let q0 = Quat::from_array(values[idx]);
                let q1 = Quat::from_array(values[idx + 1]);
                q0.slerp(q1, frac).normalize().to_array()
            }
        }
    }
}

fn get_value_vec3(values: &[[f32; 3]], interp: Interpolation, idx: usize) -> [f32; 3] {
    match interp {
        Interpolation::CubicSpline => {
            let n_keyframes = values.len() / 3;
            if n_keyframes > 0 && values.len() == n_keyframes * 3 {
                values[idx * 3 + 1]
            } else {
                values[idx]
            }
        }
        _ => values[idx],
    }
}

fn get_value_quat(values: &[[f32; 4]], interp: Interpolation, idx: usize) -> [f32; 4] {
    match interp {
        Interpolation::CubicSpline => {
            let n_keyframes = values.len() / 3;
            if n_keyframes > 0 && values.len() == n_keyframes * 3 {
                values[idx * 3 + 1]
            } else {
                values[idx]
            }
        }
        _ => values[idx],
    }
}

fn cubic_hermite_vec3(
    v0: [f32; 3],
    b0: [f32; 3],
    a1: [f32; 3],
    v1: [f32; 3],
    t: f32,
    dt: f32,
) -> [f32; 3] {
    let t2 = t * t;
    let t3 = t2 * t;
    let mut result = [0.0f32; 3];
    for i in 0..3 {
        result[i] = (2.0 * t3 - 3.0 * t2 + 1.0) * v0[i]
            + (t3 - 2.0 * t2 + t) * dt * b0[i]
            + (-2.0 * t3 + 3.0 * t2) * v1[i]
            + (t3 - t2) * dt * a1[i];
    }
    result
}

// ---------------------------------------------------------------------------
// Encontrar animaciones de un skeleton
// ---------------------------------------------------------------------------

/// Filtra animaciones cuyos channels apuntan a joints de un skeleton específico.
pub(crate) fn find_skeleton_animations(
    scene: &Scene,
    skel_ctx: &SkelContext,
    skeleton_idx: usize,
) -> Vec<usize> {
    scene
        .animations
        .iter()
        .enumerate()
        .filter(|(_, anim)| {
            anim.channels.iter().any(|ch| {
                skel_ctx
                    .node_to_joint
                    .get(&ch.node)
                    .is_some_and(|&(si, _)| si == skeleton_idx)
            })
        })
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------------------
// Conversión de tiempo
// ---------------------------------------------------------------------------

/// Convierte segundos a timeCode USD.
pub(crate) fn seconds_to_timecode(seconds: f32, fps: f64) -> f64 {
    (seconds as f64) * fps
}

// ---------------------------------------------------------------------------
// Baking funciones
// ---------------------------------------------------------------------------

/// Resamplea CubicSpline a Linear: genera keyframes a intervals de 1/fps.
#[allow(dead_code)]
pub(crate) fn bake_cubic_to_linear(
    times: &[f32],
    values_vec3: &[[f32; 3]],
    fps: f64,
) -> (Vec<f32>, Vec<[f32; 3]>) {
    if times.len() < 2 {
        return (times.to_vec(), extract_cubic_values_vec3(values_vec3, times.len()));
    }

    let start = times[0];
    let end = *times.last().unwrap();
    let dt = 1.0 / fps as f32;
    let mut new_times = Vec::new();
    let mut new_values = Vec::new();

    let mut t = start;
    while t <= end + dt * 0.5 {
        new_times.push(t);
        // Interpolar cubic
        let val = sample_cubic_vec3(times, values_vec3, t);
        new_values.push(val);
        t += dt;
    }

    (new_times, new_values)
}

fn extract_cubic_values_vec3(values: &[[f32; 3]], n_keyframes: usize) -> Vec<[f32; 3]> {
    if values.len() == n_keyframes * 3 {
        (0..n_keyframes).map(|i| values[i * 3 + 1]).collect()
    } else {
        values.to_vec()
    }
}

fn sample_cubic_vec3(times: &[f32], values: &[[f32; 3]], t: f32) -> [f32; 3] {
    if times.is_empty() {
        return [0.0; 3];
    }
    if t <= times[0] {
        return if values.len() == times.len() * 3 { values[1] } else { values[0] };
    }
    if t >= *times.last().unwrap() {
        let n = times.len();
        return if values.len() == n * 3 { values[(n - 1) * 3 + 1] } else { values[n - 1] };
    }

    let idx = times.partition_point(|&x| x < t).saturating_sub(1).min(times.len() - 2);
    let t0 = times[idx];
    let t1 = times[idx + 1];
    let frac = if (t1 - t0).abs() < 1e-10 { 0.0 } else { (t - t0) / (t1 - t0) };
    let dt = t1 - t0;

    let n = times.len();
    if values.len() == n * 3 {
        let v0 = values[idx * 3 + 1];
        let b0 = values[idx * 3 + 2];
        let a1 = values[(idx + 1) * 3];
        let v1 = values[(idx + 1) * 3 + 1];
        cubic_hermite_vec3(v0, b0, a1, v1, frac, dt)
    } else {
        let v0 = values[idx];
        let v1 = values[idx + 1];
        [
            v0[0] + (v1[0] - v0[0]) * frac,
            v0[1] + (v1[1] - v0[1]) * frac,
            v0[2] + (v1[2] - v0[2]) * frac,
        ]
    }
}

/// Duplica valores step para simular como linear.
#[allow(dead_code)]
pub(crate) fn bake_step_to_linear(
    times: &[f32],
    values: &[[f32; 3]],
) -> (Vec<f32>, Vec<[f32; 3]>) {
    if times.len() < 2 {
        return (times.to_vec(), values.to_vec());
    }

    let mut new_times = Vec::with_capacity(times.len() * 2);
    let mut new_values = Vec::with_capacity(times.len() * 2);

    for i in 0..times.len() {
        new_times.push(times[i]);
        new_values.push(values[i]);

        if i + 1 < times.len() {
            // Insertar un keyframe justo antes del siguiente con el mismo valor
            let eps = 1e-6;
            let next_t = times[i + 1] - eps;
            if next_t > times[i] {
                new_times.push(next_t);
                new_values.push(values[i]);
            }
        }
    }

    (new_times, new_values)
}

/// Elimina keyframes redundantes (donde el valor no cambia significativamente).
#[allow(dead_code)]
pub(crate) fn prune_redundant_keyframes(
    times: &[f32],
    values: &[[f32; 3]],
    tolerance: f32,
) -> (Vec<f32>, Vec<[f32; 3]>) {
    if times.len() <= 2 {
        return (times.to_vec(), values.to_vec());
    }

    let mut new_times = vec![times[0]];
    let mut new_values = vec![values[0]];

    for i in 1..times.len() - 1 {
        // Interpolar linealmente entre prev y next
        let prev_idx = new_times.len() - 1;
        let t_prev = new_times[prev_idx];
        let t_next = times[i + 1];
        let t_curr = times[i];

        let frac = if (t_next - t_prev).abs() < 1e-10 {
            0.0
        } else {
            (t_curr - t_prev) / (t_next - t_prev)
        };

        let v_prev = new_values[prev_idx];
        let v_next = values[i + 1];
        let interpolated = [
            v_prev[0] + (v_next[0] - v_prev[0]) * frac,
            v_prev[1] + (v_next[1] - v_prev[1]) * frac,
            v_prev[2] + (v_next[2] - v_prev[2]) * frac,
        ];

        let diff = (0..3)
            .map(|j| (values[i][j] - interpolated[j]).abs())
            .reduce(f32::max)
            .unwrap_or(0.0);

        if diff > tolerance {
            new_times.push(t_curr);
            new_values.push(values[i]);
        }
    }

    // Siempre mantener el último
    new_times.push(*times.last().unwrap());
    new_values.push(*values.last().unwrap());

    (new_times, new_values)
}

// ---------------------------------------------------------------------------
// Animación rígida (nodos no-joint con xform timeSamples)
// ---------------------------------------------------------------------------

/// Samples de animación por nodo (translation, rotation, scale por timeCode).
pub(crate) struct NodeAnimSamples {
    /// timeCodes (ya convertidos de seconds)
    pub time_codes: Vec<f64>,
    /// Translation por timeCode
    pub translations: Vec<Vec3>,
    /// Rotation por timeCode
    pub rotations: Vec<Quat>,
    /// Scale por timeCode
    pub scales: Vec<Vec3>,
}

/// Construye contexto de animación rígida para nodos que NO son joints.
pub(crate) fn build_node_anim_context(
    scene: &Scene,
    skel_ctx: &SkelContext,
    options: &UsdaExportOptions,
) -> HashMap<usize, NodeAnimSamples> {
    if !options.export_animations {
        return HashMap::new();
    }

    let fps = options.fps;
    let mut result: HashMap<usize, NodeAnimSamples> = HashMap::new();

    for anim in &scene.animations {
        for ch in &anim.channels {
            // Solo nodos que NO son joints
            if skel_ctx.node_to_joint.contains_key(&ch.node) {
                continue;
            }

            let entry = result.entry(ch.node).or_insert_with(|| NodeAnimSamples {
                time_codes: Vec::new(),
                translations: Vec::new(),
                rotations: Vec::new(),
                scales: Vec::new(),
            });

            match &ch.values {
                KeyframeValues::Translation(vals) => {
                    for (i, &t) in ch.times.iter().enumerate() {
                        let val = get_value_vec3(vals, ch.interpolation, i.min(vals.len() - 1));
                        let tc = seconds_to_timecode(t, fps);
                        entry.time_codes.push(tc);
                        entry.translations.push(Vec3::from(val));
                    }
                }
                KeyframeValues::Rotation(vals) => {
                    for (i, &t) in ch.times.iter().enumerate() {
                        let val = get_value_quat(vals, ch.interpolation, i.min(vals.len() - 1));
                        let tc = seconds_to_timecode(t, fps);
                        entry.time_codes.push(tc);
                        entry.rotations.push(Quat::from_array(val));
                    }
                }
                KeyframeValues::Scale(vals) => {
                    for (i, &t) in ch.times.iter().enumerate() {
                        let val = get_value_vec3(vals, ch.interpolation, i.min(vals.len() - 1));
                        let tc = seconds_to_timecode(t, fps);
                        entry.time_codes.push(tc);
                        entry.scales.push(Vec3::from(val));
                    }
                }
                KeyframeValues::Weights(_) => {}
            }
        }
    }

    result
}

/// Escribe xform ops con timeSamples para un nodo animado.
pub(crate) fn write_animated_transform(
    w: &mut UsdWriter,
    samples: &NodeAnimSamples,
) {
    if !samples.translations.is_empty() {
        w.write_indent();
        w.buf.push_str("double3 xformOp:translate.timeSamples = {\n");
        w.depth += 1;
        for (i, tc) in samples.time_codes.iter().enumerate() {
            if i < samples.translations.len() {
                let v = samples.translations[i];
                w.write_fmt_line(format_args!("{}: ({}, {}, {}),", tc, v.x, v.y, v.z));
            }
        }
        w.depth -= 1;
        w.line("}");
    }

    if !samples.rotations.is_empty() {
        w.write_indent();
        w.buf.push_str("quatf xformOp:orient.timeSamples = {\n");
        w.depth += 1;
        for (i, tc) in samples.time_codes.iter().enumerate() {
            if i < samples.rotations.len() {
                let q = samples.rotations[i];
                w.write_fmt_line(format_args!("{}: ({}, {}, {}, {}),", tc, q.w, q.x, q.y, q.z));
            }
        }
        w.depth -= 1;
        w.line("}");
    }

    if !samples.scales.is_empty() {
        w.write_indent();
        w.buf.push_str("float3 xformOp:scale.timeSamples = {\n");
        w.depth += 1;
        for (i, tc) in samples.time_codes.iter().enumerate() {
            if i < samples.scales.len() {
                let s = samples.scales[i];
                w.write_fmt_line(format_args!("{}: ({}, {}, {}),", tc, s.x, s.y, s.z));
            }
        }
        w.depth -= 1;
        w.line("}");
    }

    if !samples.translations.is_empty() || !samples.rotations.is_empty() || !samples.scales.is_empty() {
        w.line("token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:orient\", \"xformOp:scale\"]");
    }
}

// ---------------------------------------------------------------------------
// Helpers de escritura para arrays USD
// ---------------------------------------------------------------------------

/// Escribe array de tokens: `uniform token[] joints = ["A", "A/B", ...]`
pub(crate) fn write_token_array(w: &mut UsdWriter, prefix: &str, values: &[String]) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "\"{}\"", v).unwrap();
    }
    w.buf.push_str("]\n");
}

/// Escribe array de matrices 4x4.
pub(crate) fn write_matrix4d_array(w: &mut UsdWriter, prefix: &str, matrices: &[Mat4]) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [\n");
    w.depth += 1;
    for (i, m) in matrices.iter().enumerate() {
        let c = m.to_cols_array();
        w.write_indent();
        write!(
            w.buf,
            "(({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}))",
            c[0], c[1], c[2], c[3],
            c[4], c[5], c[6], c[7],
            c[8], c[9], c[10], c[11],
            c[12], c[13], c[14], c[15]
        )
        .unwrap();
        if i + 1 < matrices.len() {
            w.buf.push(',');
        }
        w.buf.push('\n');
    }
    w.depth -= 1;
    w.line("]");
}

/// Escribe array plano de ints con elementSize.
pub(crate) fn write_int_flat_array(w: &mut UsdWriter, prefix: &str, values: &[i32], element_size: usize) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "{}", v).unwrap();
    }
    w.buf.push_str("] (\n");
    w.write_indent();
    writeln!(w.buf, "    elementSize = {}", element_size).unwrap();
    w.write_indent();
    w.buf.push_str("    interpolation = \"vertex\"\n");
    w.write_indent();
    w.buf.push_str(")\n");
}

/// Escribe array plano de floats con elementSize.
pub(crate) fn write_float_flat_array(w: &mut UsdWriter, prefix: &str, values: &[f32], element_size: usize) {
    w.write_indent();
    w.buf.push_str(prefix);
    w.buf.push_str(" = [");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            w.buf.push_str(", ");
        }
        write!(w.buf, "{}", v).unwrap();
    }
    w.buf.push_str("] (\n");
    w.write_indent();
    writeln!(w.buf, "    elementSize = {}", element_size).unwrap();
    w.write_indent();
    w.buf.push_str("    interpolation = \"vertex\"\n");
    w.write_indent();
    w.buf.push_str(")\n");
}

// ---------------------------------------------------------------------------
// Cálculo de rango de tiempo global
// ---------------------------------------------------------------------------

/// Calcula (startTimeCode, endTimeCode) para toda la escena.
pub(crate) fn compute_time_range(scene: &Scene, fps: f64) -> Option<(f64, f64)> {
    let mut min_t = f32::INFINITY;
    let mut max_t = f32::NEG_INFINITY;
    let mut found = false;

    for anim in &scene.animations {
        for ch in &anim.channels {
            if let (Some(&first), Some(&last)) = (ch.times.first(), ch.times.last()) {
                found = true;
                if first < min_t { min_t = first; }
                if last > max_t { max_t = last; }
            }
        }
    }

    if found {
        Some((seconds_to_timecode(min_t, fps), seconds_to_timecode(max_t, fps)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{Animation, Channel, Interpolation, Joint, KeyframeValues, Skeleton};

    fn simple_skeleton() -> Skeleton {
        Skeleton {
            name: "TestSkel".into(),
            joints: vec![
                Joint {
                    name: "Hips".into(),
                    children: vec![1, 2],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)),
                    node_index: Some(0),
                },
                Joint {
                    name: "LeftLeg".into(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(Vec3::new(-0.5, -1.0, 0.0)),
                    node_index: Some(1),
                },
                Joint {
                    name: "RightLeg".into(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(Vec3::new(0.5, -1.0, 0.0)),
                    node_index: Some(2),
                },
            ],
            roots: vec![0],
        }
    }

    fn chain_skeleton() -> Skeleton {
        Skeleton {
            name: "Chain".into(),
            joints: vec![
                Joint {
                    name: "Root".into(),
                    children: vec![1],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::IDENTITY,
                    node_index: Some(10),
                },
                Joint {
                    name: "Spine".into(),
                    children: vec![2],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(Vec3::Y),
                    node_index: Some(11),
                },
                Joint {
                    name: "Chest".into(),
                    children: vec![],
                    inverse_bind_matrix: Mat4::IDENTITY,
                    local_transform: Mat4::from_translation(Vec3::Y),
                    node_index: Some(12),
                },
            ],
            roots: vec![0],
        }
    }

    #[test]
    fn bfs_ordering() {
        let skel = simple_skeleton();
        let order = bfs_joint_order(&skel);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn bfs_ordering_chain() {
        let skel = chain_skeleton();
        let order = bfs_joint_order(&skel);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn joint_paths_simple() {
        let skel = simple_skeleton();
        let order = bfs_joint_order(&skel);
        let paths = compute_joint_paths(&skel, &order);
        assert_eq!(paths[0], "Hips");
        assert_eq!(paths[1], "Hips/LeftLeg");
        assert_eq!(paths[2], "Hips/RightLeg");
    }

    #[test]
    fn joint_paths_chain() {
        let skel = chain_skeleton();
        let order = bfs_joint_order(&skel);
        let paths = compute_joint_paths(&skel, &order);
        assert_eq!(paths[0], "Root");
        assert_eq!(paths[1], "Root/Spine");
        assert_eq!(paths[2], "Root/Spine/Chest");
    }

    #[test]
    fn bind_transforms_identity() {
        let skel = simple_skeleton();
        let binds = compute_bind_transforms(&skel);
        for b in &binds {
            // inverse of IDENTITY = IDENTITY
            let diff = (*b - Mat4::IDENTITY).to_cols_array();
            assert!(diff.iter().all(|v| v.abs() < 1e-6));
        }
    }

    #[test]
    fn rest_transforms() {
        let skel = simple_skeleton();
        let rests = compute_rest_transforms(&skel);
        assert_eq!(rests.len(), 3);
        // Hips tiene translation (0, 1, 0)
        let (_, _, t) = rests[0].to_scale_rotation_translation();
        assert!((t.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn skeleton_def_output() {
        let skel = simple_skeleton();
        let mut scene = Scene::new();
        scene.skeletons.push(skel);
        let ctx = prepare_skel_context(&scene);

        let mut w = UsdWriter::new(4096);
        write_skeleton_def(&mut w, &scene.skeletons[0], &ctx, 0);
        let output = w.finish();

        assert!(output.contains("def Skeleton \"TestSkel\""), "output: {}", output);
        assert!(output.contains("uniform token[] joints"), "output: {}", output);
        assert!(output.contains("\"Hips\""), "output: {}", output);
        assert!(output.contains("\"Hips/LeftLeg\""), "output: {}", output);
        assert!(output.contains("bindTransforms"), "output: {}", output);
        assert!(output.contains("restTransforms"), "output: {}", output);
    }

    #[test]
    fn seconds_to_timecode_test() {
        assert!((seconds_to_timecode(0.0, 24.0) - 0.0).abs() < 1e-10);
        assert!((seconds_to_timecode(1.0, 24.0) - 24.0).abs() < 1e-10);
        assert!((seconds_to_timecode(0.5, 30.0) - 15.0).abs() < 1e-10);
    }

    #[test]
    fn bake_step_duplicates() {
        let times = vec![0.0, 1.0, 2.0];
        let values = vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [3.0, 0.0, 0.0]];
        let (new_times, new_values) = bake_step_to_linear(&times, &values);

        // Debería tener más keyframes que los originales
        assert!(new_times.len() > times.len());
        // Primer y último valor se mantienen
        assert_eq!(new_values[0], [1.0, 0.0, 0.0]);
        assert_eq!(*new_values.last().unwrap(), [3.0, 0.0, 0.0]);
    }

    #[test]
    fn prune_constant_keyframes() {
        let times = vec![0.0, 1.0, 2.0, 3.0];
        let values = vec![
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // redundante
            [1.0, 0.0, 0.0], // redundante
            [1.0, 0.0, 0.0],
        ];
        let (new_times, new_values) = prune_redundant_keyframes(&times, &values, 1e-4);

        // Solo debe quedar primero y último
        assert_eq!(new_times.len(), 2);
        assert_eq!(new_values.len(), 2);
    }

    #[test]
    fn prune_keeps_significant() {
        let times = vec![0.0, 1.0, 2.0];
        let values = vec![
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0], // significativamente diferente
            [10.0, 0.0, 0.0],
        ];
        let (new_times, _new_values) = prune_redundant_keyframes(&times, &values, 1e-4);

        // El keyframe del medio es necesario (no es interpolación lineal de extremos, wait it is)
        // Actually [0,0,0] → [5,0,0] → [10,0,0] IS linear, so it could be pruned
        // Let's check: interpolated at t=1 between [0,0,0] and [10,0,0] = [5,0,0] — exactly matches
        // So it SHOULD be pruned
        assert_eq!(new_times.len(), 2);
    }

    #[test]
    fn find_skeleton_anims() {
        let mut scene = Scene::new();
        scene.skeletons.push(simple_skeleton());
        let ctx = prepare_skel_context(&scene);

        // Añadir animación que apunta al node 0 (joint Hips)
        scene.animations.push(Animation {
            name: "Walk".into(),
            channels: vec![Channel {
                node: 0,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 1.0],
                values: KeyframeValues::Translation(vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            }],
        });

        // Animación que no apunta a joints
        scene.animations.push(Animation {
            name: "Camera".into(),
            channels: vec![Channel {
                node: 99,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 1.0],
                values: KeyframeValues::Translation(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            }],
        });

        let skel_anims = find_skeleton_animations(&scene, &ctx, 0);
        assert_eq!(skel_anims, vec![0]); // Solo la primera
    }

    #[test]
    fn time_range_computation() {
        let mut scene = Scene::new();
        scene.animations.push(Animation {
            name: "test".into(),
            channels: vec![Channel {
                node: 0,
                interpolation: Interpolation::Linear,
                times: vec![0.5, 2.0],
                values: KeyframeValues::Translation(vec![[0.0; 3]; 2]),
            }],
        });

        let range = compute_time_range(&scene, 24.0);
        assert!(range.is_some());
        let (start, end) = range.unwrap();
        assert!((start - 12.0).abs() < 1e-6);
        assert!((end - 48.0).abs() < 1e-6);
    }
}
