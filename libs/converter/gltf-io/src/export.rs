use crate::geometry;
use crate::options::GlbExportOptions;
use crate::texture_process;
use crate::transforms;
use converter_scene::{
    AlphaMode, IndexData, KeyframeValues, Scene, TextureFormat, TextureRef, Transform,
    VertexAttribute,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GlbExportError {
    #[error("error escribiendo archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("escena vacía: no hay meshes")]
    EmptyScene,
    #[error("serialización JSON falló: {0}")]
    Json(#[from] serde_json::Error),
    #[error("error procesando textura: {0}")]
    TextureProcess(String),
}

/// Exporta una `Scene` a un archivo GLB en disco.
///
/// Usa `GlbExportOptions::default()` para mantener el comportamiento original.
pub fn export_glb(
    scene: &Scene,
    path: impl AsRef<Path>,
    options: &GlbExportOptions,
) -> Result<(), GlbExportError> {
    let bytes = export_glb_bytes(scene, options)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Exporta una `Scene` a bytes GLB en memoria.
///
/// Usa `GlbExportOptions::default()` para mantener el comportamiento original.
pub fn export_glb_bytes(
    scene: &Scene,
    options: &GlbExportOptions,
) -> Result<Vec<u8>, GlbExportError> {
    let scene = preprocess_scene(scene, options);

    let mut builder = GlbBuilder::new(options);
    builder.write_meshes(&scene);
    builder.write_materials(&scene);
    builder.write_textures(&scene)?;
    builder.write_nodes(&scene);
    builder.write_skins(&scene);
    if options.export_animations {
        builder.write_animations(&scene);
    }
    builder.build()
}

/// Preprocesa la escena según las opciones activas.
fn preprocess_scene(scene: &Scene, options: &GlbExportOptions) -> Scene {
    // glTF usa metros: convertir si la escena está en otra unidad (p. ej. STL en mm)
    let to_meters = scene.meters_per_unit;
    if !options.needs_preprocessing() && to_meters == 1.0 {
        return scene.clone();
    }

    let mut scene = scene.clone();

    // 1. Escalar geometría (unidades + factor pedido)
    let factor = options.scale_factor.unwrap_or(1.0) * to_meters;
    if factor != 1.0 {
        transforms::apply_scale(&mut scene, factor);
    }
    scene.meters_per_unit = 1.0;

    // 2. Aplanar transforms
    if options.flatten_transforms {
        transforms::flatten_node_transforms(&mut scene);
    }

    // 3. Generar normales faltantes
    if options.generate_normals {
        transforms::generate_missing_normals(&mut scene);
    }

    // 4. Optimizar geometría
    if options.optimize_geometry {
        for mesh in &mut scene.meshes {
            for prim in &mut mesh.primitives {
                geometry::optimize_primitive(prim);
            }
        }
    }

    // 5. Strip unused
    if options.strip_unused {
        strip_unused_data(&mut scene);
    }

    // 6. Eliminar animaciones si no se exportan
    if !options.export_animations {
        scene.animations.clear();
    }

    scene
}

/// Elimina materiales y texturas no referenciados por ninguna primitiva.
fn strip_unused_data(scene: &mut Scene) {
    // Recopilar material indices usados
    let mut used_materials: HashSet<usize> = HashSet::new();
    for mesh in &scene.meshes {
        for prim in &mesh.primitives {
            if let Some(mat_idx) = prim.material {
                used_materials.insert(mat_idx);
            }
        }
    }

    // Recopilar texture indices referenciados por materiales usados
    let mut used_textures: HashSet<usize> = HashSet::new();
    for &mat_idx in &used_materials {
        if let Some(mat) = scene.materials.get(mat_idx) {
            if let Some(ref t) = mat.base_color_texture { used_textures.insert(t.texture_index); }
            if let Some(ref t) = mat.metallic_roughness_texture { used_textures.insert(t.texture_index); }
            if let Some(ref t) = mat.normal_texture { used_textures.insert(t.texture_index); }
            if let Some(ref t) = mat.occlusion_texture { used_textures.insert(t.texture_index); }
            if let Some(ref t) = mat.emissive_texture { used_textures.insert(t.texture_index); }
        }
    }

    // Si todo está usado, no hacer nada
    if used_materials.len() == scene.materials.len()
        && used_textures.len() == scene.textures.len()
    {
        return;
    }

    // Construir mapa de reasignación de texturas: old_idx → new_idx
    let mut tex_remap: Vec<Option<usize>> = vec![None; scene.textures.len()];
    let mut new_tex_idx = 0;
    for (i, slot) in tex_remap.iter_mut().enumerate() {
        if used_textures.contains(&i) {
            *slot = Some(new_tex_idx);
            new_tex_idx += 1;
        }
    }

    // Construir mapa de reasignación de materiales
    let mut mat_remap: Vec<Option<usize>> = vec![None; scene.materials.len()];
    let mut new_mat_idx = 0;
    for (i, slot) in mat_remap.iter_mut().enumerate() {
        if used_materials.contains(&i) {
            *slot = Some(new_mat_idx);
            new_mat_idx += 1;
        }
    }

    // Filtrar texturas
    let new_textures: Vec<_> = scene
        .textures
        .iter()
        .enumerate()
        .filter(|(i, _)| used_textures.contains(i))
        .map(|(_, t)| t.clone())
        .collect();
    scene.textures = new_textures;

    // Filtrar y remapear materiales
    let new_materials: Vec<_> = scene
        .materials
        .iter()
        .enumerate()
        .filter(|(i, _)| used_materials.contains(i))
        .map(|(_, mat)| {
            let mut mat = mat.clone();
            remap_texture_ref(&mut mat.base_color_texture, &tex_remap);
            remap_texture_ref(&mut mat.metallic_roughness_texture, &tex_remap);
            remap_texture_ref(&mut mat.normal_texture, &tex_remap);
            remap_texture_ref(&mut mat.occlusion_texture, &tex_remap);
            remap_texture_ref(&mut mat.emissive_texture, &tex_remap);
            mat
        })
        .collect();
    scene.materials = new_materials;

    // Remapear material indices en primitivas
    for mesh in &mut scene.meshes {
        for prim in &mut mesh.primitives {
            if let Some(mat_idx) = prim.material {
                prim.material = mat_remap[mat_idx];
            }
        }
    }
}

fn remap_texture_ref(tex_ref: &mut Option<TextureRef>, remap: &[Option<usize>]) {
    if let Some(tr) = tex_ref {
        match remap.get(tr.texture_index) {
            Some(Some(new_idx)) => tr.texture_index = *new_idx,
            _ => *tex_ref = None, // textura eliminada
        }
    }
}

// ---------------------------------------------------------------------------
// GlbBuilder
// ---------------------------------------------------------------------------

struct GlbBuilder<'a> {
    options: &'a GlbExportOptions,
    bin: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    textures_json: Vec<Value>,
    images: Vec<Value>,
    nodes: Vec<Value>,
    skins: Vec<Value>,
    animations: Vec<Value>,
    extensions_used: Vec<String>,
    /// Mapa: skeleton index → (primer nodo de joint en nodes[], cantidad de joints)
    skin_joint_offsets: Vec<(usize, usize)>,
}

impl<'a> GlbBuilder<'a> {
    fn new(options: &'a GlbExportOptions) -> Self {
        Self {
            options,
            bin: Vec::new(),
            buffer_views: Vec::new(),
            accessors: Vec::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures_json: Vec::new(),
            images: Vec::new(),
            nodes: Vec::new(),
            skins: Vec::new(),
            animations: Vec::new(),
            extensions_used: Vec::new(),
            skin_joint_offsets: Vec::new(),
        }
    }

    /// Alinea el buffer BIN a 4 bytes, agrega datos y crea un bufferView.
    /// Retorna el índice del bufferView creado.
    fn push_buffer_view(&mut self, data: &[u8], target: Option<u32>) -> usize {
        // Alinear offset a 4 bytes
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(data);
        let byte_length = data.len();

        let mut bv = json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": byte_length,
        });
        if let Some(t) = target {
            bv["target"] = json!(t);
        }
        let idx = self.buffer_views.len();
        self.buffer_views.push(bv);
        idx
    }

    /// Crea un accessor y retorna su índice.
    fn push_accessor(
        &mut self,
        buffer_view: usize,
        component_type: u32,
        count: usize,
        accessor_type: &str,
        min: Option<Value>,
        max: Option<Value>,
    ) -> usize {
        let mut acc = json!({
            "bufferView": buffer_view,
            "componentType": component_type,
            "count": count,
            "type": accessor_type,
        });
        if let Some(mn) = min {
            acc["min"] = mn;
        }
        if let Some(mx) = max {
            acc["max"] = mx;
        }
        let idx = self.accessors.len();
        self.accessors.push(acc);
        idx
    }

    // -----------------------------------------------------------------------
    // Meshes
    // -----------------------------------------------------------------------

    fn write_meshes(&mut self, scene: &Scene) {
        for mesh in &scene.meshes {
            let mut primitives_json = Vec::new();

            for prim in &mesh.primitives {
                let mut attributes = serde_json::Map::new();

                for attr in &prim.attributes {
                    match attr {
                        VertexAttribute::Positions(positions) => {
                            let (min, max) = compute_vec3_bounds(positions);
                            let data = vec3_to_bytes(positions);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(
                                bv, 5126, positions.len(), "VEC3",
                                Some(json!(min)), Some(json!(max)),
                            );
                            attributes.insert("POSITION".into(), json!(acc));
                        }
                        VertexAttribute::Normals(normals) => {
                            let data = vec3_to_bytes(normals);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5126, normals.len(), "VEC3", None, None);
                            attributes.insert("NORMAL".into(), json!(acc));
                        }
                        VertexAttribute::Tangents(tangents) => {
                            let data = vec4_to_bytes(tangents);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5126, tangents.len(), "VEC4", None, None);
                            attributes.insert("TANGENT".into(), json!(acc));
                        }
                        VertexAttribute::TexCoords(set, uvs) => {
                            let data = vec2_to_bytes(uvs);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5126, uvs.len(), "VEC2", None, None);
                            attributes.insert(format!("TEXCOORD_{set}"), json!(acc));
                        }
                        VertexAttribute::Colors(colors) => {
                            let data = vec4_to_bytes(colors);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5126, colors.len(), "VEC4", None, None);
                            attributes.insert("COLOR_0".into(), json!(acc));
                        }
                        VertexAttribute::JointIndices(joints) => {
                            let data = u16x4_to_bytes(joints);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5123, joints.len(), "VEC4", None, None);
                            attributes.insert("JOINTS_0".into(), json!(acc));
                        }
                        VertexAttribute::JointWeights(weights) => {
                            let data = vec4_to_bytes(weights);
                            let bv = self.push_buffer_view(&data, Some(34962));
                            let acc = self.push_accessor(bv, 5126, weights.len(), "VEC4", None, None);
                            attributes.insert("WEIGHTS_0".into(), json!(acc));
                        }
                    }
                }

                let mut prim_json = json!({
                    "attributes": Value::Object(attributes),
                });

                // Índices
                if let Some(indices) = &prim.indices {
                    let (data, component_type, count) = match indices {
                        IndexData::U16(idx) => {
                            let bytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
                            (bytes, 5123u32, idx.len())
                        }
                        IndexData::U32(idx) => {
                            let bytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
                            (bytes, 5125u32, idx.len())
                        }
                    };
                    let bv = self.push_buffer_view(&data, Some(34963));
                    let acc = self.push_accessor(bv, component_type, count, "SCALAR", None, None);
                    prim_json["indices"] = json!(acc);
                }

                if let Some(mat_idx) = prim.material {
                    prim_json["material"] = json!(mat_idx);
                }

                primitives_json.push(prim_json);
            }

            let mut mesh_json = json!({
                "primitives": primitives_json,
            });
            if !mesh.name.is_empty() {
                mesh_json["name"] = json!(mesh.name);
            }
            self.meshes.push(mesh_json);
        }
    }

    // -----------------------------------------------------------------------
    // Materials + Textures
    // -----------------------------------------------------------------------

    fn write_materials(&mut self, scene: &Scene) {
        for mat in &scene.materials {
            let mut pbr = json!({
                "baseColorFactor": mat.base_color_factor,
                "metallicFactor": mat.metallic_factor,
                "roughnessFactor": mat.roughness_factor,
            });

            if let Some(ref tex_ref) = mat.base_color_texture {
                pbr["baseColorTexture"] = texture_info_json(tex_ref);
            }
            if let Some(ref tex_ref) = mat.metallic_roughness_texture {
                pbr["metallicRoughnessTexture"] = texture_info_json(tex_ref);
            }

            let mut mat_json = json!({
                "pbrMetallicRoughness": pbr,
            });

            if !mat.name.is_empty() {
                mat_json["name"] = json!(mat.name);
            }

            if let Some(ref tex_ref) = mat.normal_texture {
                let mut info = texture_info_json(tex_ref);
                if (mat.normal_scale - 1.0).abs() > f32::EPSILON {
                    info["scale"] = json!(mat.normal_scale);
                }
                mat_json["normalTexture"] = info;
            }

            if let Some(ref tex_ref) = mat.occlusion_texture {
                let mut info = texture_info_json(tex_ref);
                if (mat.occlusion_strength - 1.0).abs() > f32::EPSILON {
                    info["strength"] = json!(mat.occlusion_strength);
                }
                mat_json["occlusionTexture"] = info;
            }

            if mat.emissive_factor != [0.0, 0.0, 0.0] {
                mat_json["emissiveFactor"] = json!(mat.emissive_factor);
            }
            if let Some(ref tex_ref) = mat.emissive_texture {
                mat_json["emissiveTexture"] = texture_info_json(tex_ref);
            }

            match mat.alpha_mode {
                AlphaMode::Opaque => {}
                AlphaMode::Mask(cutoff) => {
                    mat_json["alphaMode"] = json!("MASK");
                    mat_json["alphaCutoff"] = json!(cutoff);
                }
                AlphaMode::Blend => {
                    mat_json["alphaMode"] = json!("BLEND");
                }
            }

            if mat.double_sided {
                mat_json["doubleSided"] = json!(true);
            }

            if mat.unlit {
                mat_json["extensions"] = json!({
                    "KHR_materials_unlit": {}
                });
                if !self.extensions_used.contains(&"KHR_materials_unlit".to_string()) {
                    self.extensions_used.push("KHR_materials_unlit".to_string());
                }
            }

            self.materials.push(mat_json);
        }
    }

    fn write_textures(&mut self, scene: &Scene) -> Result<(), GlbExportError> {
        for tex in &scene.textures {
            let (data, mime) = if self.options.needs_texture_processing() {
                texture_process::process_texture(tex, self.options)?
            } else {
                let mime = match tex.format {
                    TextureFormat::Png => "image/png",
                    TextureFormat::Jpeg => "image/jpeg",
                    TextureFormat::WebP => "image/webp",
                };
                (tex.data.clone(), mime)
            };

            let bv = self.push_buffer_view(&data, None);
            let img_idx = self.images.len();
            self.images.push(json!({
                "bufferView": bv,
                "mimeType": mime,
            }));

            self.textures_json.push(json!({
                "source": img_idx,
            }));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Nodes
    // -----------------------------------------------------------------------

    fn write_nodes(&mut self, scene: &Scene) {
        // Los nodos de la escena se añaden primero.
        // Los nodos de joints de skeletons se añaden después en write_skins.
        for node in &scene.nodes {
            let mut node_json = json!({});

            if !node.name.is_empty() {
                node_json["name"] = json!(node.name);
            }

            match node.transform {
                Transform::Trs { translation, rotation, scale } => {
                    let t = translation;
                    let r = rotation;
                    let s = scale;
                    if t.x != 0.0 || t.y != 0.0 || t.z != 0.0 {
                        node_json["translation"] = json!([t.x, t.y, t.z]);
                    }
                    if r != glam::Quat::IDENTITY {
                        // glTF spec: rotation [x, y, z, w]
                        node_json["rotation"] = json!([r.x, r.y, r.z, r.w]);
                    }
                    if s.x != 1.0 || s.y != 1.0 || s.z != 1.0 {
                        node_json["scale"] = json!([s.x, s.y, s.z]);
                    }
                }
                Transform::Matrix(m) => {
                    let cols = m.to_cols_array();
                    node_json["matrix"] = json!(cols);
                }
            }

            if let Some(mesh_idx) = node.mesh {
                node_json["mesh"] = json!(mesh_idx);
            }

            // skin se asignará después en write_skins
            if !node.children.is_empty() {
                node_json["children"] = json!(node.children);
            }

            self.nodes.push(node_json);
        }
    }

    // -----------------------------------------------------------------------
    // Skins (Skeletons)
    // -----------------------------------------------------------------------

    fn write_skins(&mut self, scene: &Scene) {
        let scene_node_count = scene.nodes.len();

        for (skel_idx, skeleton) in scene.skeletons.iter().enumerate() {
            let joint_node_offset = self.nodes.len();
            self.skin_joint_offsets.push((joint_node_offset, skeleton.joints.len()));

            // Crear nodos para cada joint
            for joint in &skeleton.joints {
                // Si el joint tiene node_index, reusar el nodo de escena ya existente
                // en vez de crear uno duplicado. Si no, crear nodo nuevo.
                let mut node_json = json!({});
                if !joint.name.is_empty() {
                    node_json["name"] = json!(joint.name);
                }

                // Descomponer local_transform en TRS
                let (scale, rotation, translation) =
                    joint.local_transform.to_scale_rotation_translation();
                if translation.x != 0.0 || translation.y != 0.0 || translation.z != 0.0 {
                    node_json["translation"] = json!([translation.x, translation.y, translation.z]);
                }
                if rotation != glam::Quat::IDENTITY {
                    node_json["rotation"] = json!([rotation.x, rotation.y, rotation.z, rotation.w]);
                }
                if scale.x != 1.0 || scale.y != 1.0 || scale.z != 1.0 {
                    node_json["scale"] = json!([scale.x, scale.y, scale.z]);
                }

                // Children: remap joint-local indices → global node indices
                if !joint.children.is_empty() {
                    let children: Vec<usize> = joint.children.iter()
                        .map(|&c| joint_node_offset + c)
                        .collect();
                    node_json["children"] = json!(children);
                }

                self.nodes.push(node_json);
            }

            // joints array (índices de nodos globales)
            let joint_indices: Vec<usize> = (0..skeleton.joints.len())
                .map(|j| joint_node_offset + j)
                .collect();

            // inverseBindMatrices accessor
            let ibm_data: Vec<u8> = skeleton.joints.iter()
                .flat_map(|j| {
                    j.inverse_bind_matrix.to_cols_array().iter()
                        .flat_map(|f| f.to_le_bytes())
                        .collect::<Vec<u8>>()
                })
                .collect();
            let ibm_bv = self.push_buffer_view(&ibm_data, None);
            let ibm_acc = self.push_accessor(
                ibm_bv, 5126, skeleton.joints.len(), "MAT4", None, None,
            );

            let mut skin_json = json!({
                "joints": joint_indices,
                "inverseBindMatrices": ibm_acc,
            });
            if !skeleton.name.is_empty() {
                skin_json["name"] = json!(skeleton.name);
            }
            // Skeleton root
            if let Some(&root) = skeleton.roots.first() {
                skin_json["skeleton"] = json!(joint_node_offset + root);
            }

            self.skins.push(skin_json);

            // Asignar skin a los nodos de escena que lo referencian
            for (ni, node) in scene.nodes.iter().enumerate() {
                if node.skin == Some(skel_idx) {
                    self.nodes[ni]["skin"] = json!(skel_idx);
                }
            }
        }

        // Remap animation node targets: los nodos de animación en Scene apuntan
        // a nodos de escena (no a joints). Nada que hacer aquí — se resuelve en write_animations.
        let _ = scene_node_count;
    }

    // -----------------------------------------------------------------------
    // Animations
    // -----------------------------------------------------------------------

    fn write_animations(&mut self, scene: &Scene) {
        for anim in &scene.animations {
            let mut samplers = Vec::new();
            let mut channels = Vec::new();

            for ch in &anim.channels {
                let interp = match ch.interpolation {
                    converter_scene::Interpolation::Linear => "LINEAR",
                    converter_scene::Interpolation::Step => "STEP",
                    converter_scene::Interpolation::CubicSpline => "CUBICSPLINE",
                };

                // Input: times
                let times_data: Vec<u8> = ch.times.iter()
                    .flat_map(|t| t.to_le_bytes())
                    .collect();
                let times_bv = self.push_buffer_view(&times_data, None);
                let t_min = ch.times.iter().copied().reduce(f32::min);
                let t_max = ch.times.iter().copied().reduce(f32::max);
                let input_acc = self.push_accessor(
                    times_bv, 5126, ch.times.len(), "SCALAR",
                    t_min.map(|v| json!([v])),
                    t_max.map(|v| json!([v])),
                );

                // Output: values
                let (output_data, accessor_type, count) = match &ch.values {
                    KeyframeValues::Translation(vals) => {
                        (vec3_to_bytes(vals), "VEC3", vals.len())
                    }
                    KeyframeValues::Rotation(vals) => {
                        (vec4_to_bytes(vals), "VEC4", vals.len())
                    }
                    KeyframeValues::Scale(vals) => {
                        (vec3_to_bytes(vals), "VEC3", vals.len())
                    }
                    KeyframeValues::Weights(vals) => {
                        let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
                        (bytes, "SCALAR", vals.len())
                    }
                };

                let output_bv = self.push_buffer_view(&output_data, None);
                let output_acc = self.push_accessor(
                    output_bv, 5126, count, accessor_type, None, None,
                );

                let sampler_idx = samplers.len();
                samplers.push(json!({
                    "input": input_acc,
                    "output": output_acc,
                    "interpolation": interp,
                }));

                let path = match &ch.values {
                    KeyframeValues::Translation(_) => "translation",
                    KeyframeValues::Rotation(_) => "rotation",
                    KeyframeValues::Scale(_) => "scale",
                    KeyframeValues::Weights(_) => "weights",
                };

                channels.push(json!({
                    "sampler": sampler_idx,
                    "target": {
                        "node": ch.node,
                        "path": path,
                    }
                }));
            }

            if channels.is_empty() {
                continue;
            }

            let mut anim_json = json!({
                "samplers": samplers,
                "channels": channels,
            });
            if !anim.name.is_empty() {
                anim_json["name"] = json!(anim.name);
            }
            self.animations.push(anim_json);
        }
    }

    // -----------------------------------------------------------------------
    // Build GLB
    // -----------------------------------------------------------------------

    fn build(self) -> Result<Vec<u8>, GlbExportError> {
        let mut root = json!({
            "asset": { "version": "2.0", "generator": "converter-gltf-io" },
        });

        if !self.accessors.is_empty() {
            root["accessors"] = json!(self.accessors);
        }
        if !self.buffer_views.is_empty() {
            root["bufferViews"] = json!(self.buffer_views);
        }
        if !self.meshes.is_empty() {
            root["meshes"] = json!(self.meshes);
        }
        if !self.materials.is_empty() {
            root["materials"] = json!(self.materials);
        }
        if !self.textures_json.is_empty() {
            root["textures"] = json!(self.textures_json);
        }
        if !self.images.is_empty() {
            root["images"] = json!(self.images);
        }
        if !self.nodes.is_empty() {
            root["nodes"] = json!(self.nodes);
        }
        if !self.skins.is_empty() {
            root["skins"] = json!(self.skins);
        }
        if !self.animations.is_empty() {
            root["animations"] = json!(self.animations);
        }
        if !self.extensions_used.is_empty() {
            root["extensionsUsed"] = json!(self.extensions_used);
        }

        // Scene + scenes — recopilar root_nodes de los nodos que NO son joints de skins
        // Los nodos de escena son los primeros; los nodos de joints vienen después
        // Necesitamos reconstruir los root_nodes del JSON original.
        // En realidad, los root_nodes del Scene original son los correctos.
        // Pero ya no tenemos acceso al scene aquí. Los pasamos como parte del JSON de nodes.
        // Solución: incluir la info de scene en los nodos.
        // En build() ya no tenemos scene. Necesitamos almacenar root_nodes.

        // Como no almacenamos root_nodes, los reconstruimos:
        // todos los nodos que no aparecen como children de ningún otro nodo.
        let mut is_child = vec![false; self.nodes.len()];
        for node in &self.nodes {
            if let Some(children) = node.get("children")
                && let Some(arr) = children.as_array() {
                    for c in arr {
                        if let Some(idx) = c.as_u64()
                            && (idx as usize) < is_child.len() {
                                is_child[idx as usize] = true;
                            }
                    }
                }
        }
        let root_nodes: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| !is_child[i])
            .collect();

        if !self.nodes.is_empty() {
            root["scene"] = json!(0);
            root["scenes"] = json!([{ "nodes": root_nodes }]);
        }

        // Buffer
        let bin = self.bin;
        // Pad bin a 4 bytes
        let mut bin_padded = bin;
        while !bin_padded.len().is_multiple_of(4) {
            bin_padded.push(0);
        }

        if !bin_padded.is_empty() {
            root["buffers"] = json!([{ "byteLength": bin_padded.len() }]);
        }

        let mut json_bytes = serde_json::to_vec(&root)?;
        // Pad JSON a 4 bytes con espacios (spec GLB)
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let has_bin = !bin_padded.is_empty();
        let total_length = 12
            + 8 + json_bytes.len()
            + if has_bin { 8 + bin_padded.len() } else { 0 };

        let mut glb = Vec::with_capacity(total_length);

        // Header (12 bytes)
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());

        // JSON chunk
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);

        // BIN chunk
        if has_bin {
            glb.extend_from_slice(&(bin_padded.len() as u32).to_le_bytes());
            glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
            glb.extend_from_slice(&bin_padded);
        }

        Ok(glb)
    }
}

// ---------------------------------------------------------------------------
// Helpers de serialización binaria
// ---------------------------------------------------------------------------

fn vec3_to_bytes(data: &[[f32; 3]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 12);
    for v in data {
        for &f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }
    bytes
}

fn vec4_to_bytes(data: &[[f32; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 16);
    for v in data {
        for &f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }
    bytes
}

fn vec2_to_bytes(data: &[[f32; 2]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for v in data {
        for &f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }
    bytes
}

fn u16x4_to_bytes(data: &[[u16; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for v in data {
        for &u in v {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
    }
    bytes
}

fn compute_vec3_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            if p[i] < min[i] { min[i] = p[i]; }
            if p[i] > max[i] { max[i] = p[i]; }
        }
    }
    (min, max)
}

fn texture_info_json(tex_ref: &TextureRef) -> Value {
    let mut info = json!({
        "index": tex_ref.texture_index,
    });
    if tex_ref.tex_coord_set != 0 {
        info["texCoord"] = json!(tex_ref.tex_coord_set);
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::GlbExportOptions;
    use converter_scene::*;

    fn triangle_scene() -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "Triangle".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ])],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "Triangle".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);
        scene
    }

    #[test]
    fn export_minimal_triangle() {
        let scene = triangle_scene();
        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();

        // Verificar header GLB
        assert_eq!(&glb[0..4], b"glTF");
        let version = u32::from_le_bytes([glb[4], glb[5], glb[6], glb[7]]);
        assert_eq!(version, 2);
        let total_len = u32::from_le_bytes([glb[8], glb[9], glb[10], glb[11]]);
        assert_eq!(total_len as usize, glb.len());

        // JSON chunk
        let json_len = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]);
        let json_type = u32::from_le_bytes([glb[16], glb[17], glb[18], glb[19]]);
        assert_eq!(json_type, 0x4E4F534A);
        assert!(json_len > 0);

        // BIN chunk
        let bin_offset = 20 + json_len as usize;
        let bin_len = u32::from_le_bytes([glb[bin_offset], glb[bin_offset+1], glb[bin_offset+2], glb[bin_offset+3]]);
        let bin_type = u32::from_le_bytes([glb[bin_offset+4], glb[bin_offset+5], glb[bin_offset+6], glb[bin_offset+7]]);
        assert_eq!(bin_type, 0x004E4942);
        assert!(bin_len > 0);
    }

    #[test]
    fn roundtrip_triangle() {
        let scene = triangle_scene();
        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();

        // Escribir a disco y reimportar
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        // Comparar posiciones
        assert_eq!(imported.meshes.len(), 1);
        let prim = &imported.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();
        assert_eq!(positions, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);

        // Comparar índices (el import siempre lee como U32)
        match &prim.indices {
            Some(IndexData::U32(idx)) => assert_eq!(idx, &[0, 1, 2]),
            Some(IndexData::U16(idx)) => assert_eq!(idx, &[0, 1, 2]),
            None => panic!("esperaba índices"),
        }
    }

    #[test]
    fn roundtrip_with_material() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Red".into(),
            base_color_factor: [1.0, 0.0, 0.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.8,
            double_sided: true,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt_mat.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        assert_eq!(imported.materials.len(), 1);
        let mat = &imported.materials[0];
        assert_eq!(mat.name, "Red");
        assert_eq!(mat.base_color_factor, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(mat.metallic_factor, 0.0);
        assert!((mat.roughness_factor - 0.8).abs() < 1e-5);
        assert!(mat.double_sided);
    }

    #[test]
    fn roundtrip_normals_uvs() {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "Tri".into(),
            primitives: vec![Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![
                        [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
                    ]),
                    VertexAttribute::Normals(vec![
                        [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0],
                    ]),
                    VertexAttribute::TexCoords(0, vec![
                        [0.0, 0.0], [1.0, 0.0], [0.0, 1.0],
                    ]),
                ],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "N".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt_nrm.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        let prim = &imported.meshes[0].primitives[0];
        let normals = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Normals(n) = a { Some(n) } else { None }
        }).unwrap();
        assert_eq!(normals, &[[0.0, 0.0, 1.0]; 3]);

        let uvs = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::TexCoords(0, uv) = a { Some(uv) } else { None }
        }).unwrap();
        assert_eq!(uvs, &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    }

    #[test]
    fn roundtrip_with_texture() {
        // Crear un PNG mínimo de 1x1 pixel rojo
        let png_data = create_1x1_red_png();

        let mut scene = triangle_scene();
        scene.textures.push(Texture {
            name: "red".into(),
            data: png_data.clone(),
            format: TextureFormat::Png,
            width: 1,
            height: 1,
        });
        scene.materials.push(Material {
            name: "Textured".into(),
            base_color_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt_tex.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        assert_eq!(imported.textures.len(), 1);
        assert_eq!(imported.textures[0].format, TextureFormat::Png);
        assert_eq!(imported.textures[0].data, png_data);
    }

    #[test]
    fn export_unlit_material() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Unlit".into(),
            unlit: true,
            base_color_factor: [0.8, 0.2, 0.1, 1.0],
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();

        // Parsear JSON del GLB para verificar extensión
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();
        let ext_used = root["extensionsUsed"].as_array().unwrap();
        assert!(ext_used.iter().any(|v| v == "KHR_materials_unlit"));

        let mat = &root["materials"][0];
        assert!(mat["extensions"]["KHR_materials_unlit"].is_object());
    }

    #[test]
    fn export_alpha_modes() {
        let mut scene = triangle_scene();

        // Material con MASK
        scene.materials.push(Material {
            name: "Masked".into(),
            alpha_mode: AlphaMode::Mask(0.3),
            ..Material::default()
        });
        // Material con BLEND
        scene.materials.push(Material {
            name: "Blended".into(),
            alpha_mode: AlphaMode::Blend,
            ..Material::default()
        });
        // Material OPAQUE (default)
        scene.materials.push(Material {
            name: "Opaque".into(),
            alpha_mode: AlphaMode::Opaque,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();

        let mats = root["materials"].as_array().unwrap();
        assert_eq!(mats[0]["alphaMode"], "MASK");
        assert!((mats[0]["alphaCutoff"].as_f64().unwrap() - 0.3).abs() < 1e-5);
        assert_eq!(mats[1]["alphaMode"], "BLEND");
        assert!(mats[2].get("alphaMode").is_none()); // Opaque es default
    }

    #[test]
    fn export_indices_u16() {
        let scene = triangle_scene();
        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();

        // El accessor de índices debe ser componentType 5123 (U16)
        let prim = &root["meshes"][0]["primitives"][0];
        let indices_acc = prim["indices"].as_u64().unwrap() as usize;
        let acc = &root["accessors"][indices_acc];
        assert_eq!(acc["componentType"], 5123);
    }

    // -----------------------------------------------------------------------
    // Helpers de test
    // -----------------------------------------------------------------------

    fn extract_glb_json(glb: &[u8]) -> String {
        let json_len = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]) as usize;
        String::from_utf8(glb[20..20+json_len].to_vec()).unwrap()
    }

    /// Crea un PNG mínimo de 1x1 pixel rojo (RGBA).
    fn create_1x1_red_png() -> Vec<u8> {
        // PNG mínimo válido
        let mut png = Vec::new();
        // Signature
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        // IHDR
        let ihdr_data: [u8; 13] = [
            0, 0, 0, 1, // width
            0, 0, 0, 1, // height
            8,          // bit depth
            6,          // color type (RGBA)
            0,          // compression
            0,          // filter
            0,          // interlace
        ];
        write_png_chunk(&mut png, b"IHDR", &ihdr_data);
        // IDAT — raw pixel data (filter byte 0 + RGBA pixel)
        let raw_data = [0u8, 255, 0, 0, 255]; // filter=0, R=255, G=0, B=0, A=255
        let compressed = deflate_raw(&raw_data);
        write_png_chunk(&mut png, b"IDAT", &compressed);
        // IEND
        write_png_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(chunk_type);
        out.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(chunk_type);
        crc_input.extend_from_slice(data);
        let crc = crc32(&crc_input);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// Deflate mínimo: stored block (sin compresión).
    fn deflate_raw(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        // zlib header
        out.push(0x78);
        out.push(0x01);
        // Stored block: BFINAL=1, BTYPE=00
        out.push(0x01);
        let len = data.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        // Adler-32
        let adler = adler32(data);
        out.extend_from_slice(&adler.to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    // -----------------------------------------------------------------------
    // Tests de opciones
    // -----------------------------------------------------------------------

    #[test]
    fn export_with_generate_normals() {
        // Escena sin normales
        let scene = triangle_scene();
        let options = GlbExportOptions {
            generate_normals: true,
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normals.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        // Debe tener normales generadas
        let prim = &imported.meshes[0].primitives[0];
        let normals = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Normals(n) = a { Some(n) } else { None }
        });
        assert!(normals.is_some(), "normales debieron haberse generado");
        let normals = normals.unwrap();
        assert_eq!(normals.len(), 3);
        // La normal del triángulo XY debe apuntar en Z
        for n in normals {
            assert!(n[2].abs() > 0.9, "normal Z debería ser ~1.0, got {:?}", n);
        }
    }

    #[test]
    fn export_with_scale_factor() {
        let scene = triangle_scene();
        let options = GlbExportOptions {
            scale_factor: Some(2.0),
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scaled.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        let prim = &imported.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();

        // [1.0, 0.0, 0.0] * 2 = [2.0, 0.0, 0.0]
        assert!((positions[1][0] - 2.0).abs() < 1e-5);
        // [0.0, 1.0, 0.0] * 2 = [0.0, 2.0, 0.0]
        assert!((positions[2][1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn export_with_optimize_geometry_dedup() {
        // Escena con vértices duplicados
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "Dup".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0], // 0
                    [1.0, 0.0, 0.0], // 1
                    [0.0, 1.0, 0.0], // 2
                    [0.0, 0.0, 0.0], // 3 - duplicado de 0
                    [1.0, 0.0, 0.0], // 4 - duplicado de 1
                    [1.0, 1.0, 0.0], // 5
                ])],
                indices: Some(IndexData::U32(vec![0, 1, 2, 3, 4, 5])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "N".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(0);

        let options = GlbExportOptions {
            optimize_geometry: true,
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dedup.glb");
        std::fs::write(&path, &glb).unwrap();
        let imported = crate::import_gltf(&path).unwrap();

        let prim = &imported.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();

        // 6 vértices → 4 únicos después de dedup
        assert_eq!(positions.len(), 4, "dedup debería reducir a 4 vértices únicos");
    }

    #[test]
    fn export_with_no_animations() {
        let mut scene = triangle_scene();
        scene.animations.push(converter_scene::Animation {
            name: "test".into(),
            channels: vec![converter_scene::Channel {
                node: 0,
                interpolation: converter_scene::Interpolation::Linear,
                times: vec![0.0, 1.0],
                values: KeyframeValues::Translation(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            }],
        });

        let options = GlbExportOptions {
            export_animations: false,
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();

        // No debe haber animaciones
        assert!(root.get("animations").is_none());
    }

    #[test]
    fn export_with_strip_unused() {
        let mut scene = triangle_scene();
        // Material 0 usado, material 1 no usado
        scene.materials.push(Material {
            name: "Used".into(),
            ..Material::default()
        });
        scene.materials.push(Material {
            name: "Unused".into(),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let options = GlbExportOptions {
            strip_unused: true,
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();

        let mats = root["materials"].as_array().unwrap();
        assert_eq!(mats.len(), 1, "solo el material usado debe quedar");
    }

    #[test]
    fn export_with_texture_quality() {
        let png_data = create_1x1_red_png();

        let mut scene = triangle_scene();
        scene.textures.push(Texture {
            name: "red".into(),
            data: png_data,
            format: TextureFormat::Png,
            width: 1,
            height: 1,
        });
        scene.materials.push(Material {
            name: "Textured".into(),
            base_color_texture: Some(TextureRef {
                texture_index: 0,
                tex_coord_set: 0,
            }),
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let options = GlbExportOptions {
            texture_quality: Some(80),
            ..Default::default()
        };

        let glb = export_glb_bytes(&scene, &options).unwrap();
        let json_str = extract_glb_json(&glb);
        let root: Value = serde_json::from_str(&json_str).unwrap();

        // La textura 1x1 roja (opaca) debe haberse convertido a JPEG
        let mime = root["images"][0]["mimeType"].as_str().unwrap();
        assert_eq!(mime, "image/jpeg", "textura opaca debe recomprimirse a JPEG");
    }

    #[test]
    fn export_converts_scene_units_to_meters() {
        use converter_scene::{IndexData, Mesh, Node, Primitive, Transform, VertexAttribute};
        let mut scene = Scene::new();
        scene.meters_per_unit = 0.001; // p. ej. importada de STL (mm)
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [0.0, 500.0, 0.0]])],
                indices: Some(IndexData::U16(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node { name: "n".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        scene.root_nodes.push(0);

        let glb = export_glb_bytes(&scene, &GlbExportOptions::default()).unwrap();
        let back = crate::import_gltf_bytes(&glb).unwrap();
        let (_, max) = back.compute_bounding_box().unwrap();
        assert!((max[0] - 1.0).abs() < 1e-6 && (max[1] - 0.5).abs() < 1e-6, "{max:?}");
    }
}
