use converter_scene::{
    AlphaMode, Animation, Channel, IndexData, Interpolation, Joint, KeyframeValues, Material,
    Mesh, Node, Primitive, Scene, Skeleton, Texture, TextureFormat, TextureRef, Transform,
    VertexAttribute,
};
use glam::{Mat4, Quat, Vec3};
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GltfImportError {
    #[error("geometría inválida: {0}")]
    InvalidGeometry(#[from] converter_scene::SceneError),
    #[error("error leyendo archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("error parseando glTF: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("extensión no soportada: {0} (requerida por el archivo, {1})")]
    UnsupportedExtension(String, String),
}

/// Transform de textura pendiente para bakear en UVs.
struct PendingUvTransform {
    material_index: usize,
    tex_coord_set: u32,
    offset: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
}

/// Aplica una transformación de textura (KHR_texture_transform) a un set de UVs.
///
/// Fórmula de la spec glTF:
/// `uv' = rotation_matrix * (uv * scale) + offset`
fn apply_texture_transform(
    uvs: &[[f32; 2]],
    offset: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
) -> Vec<[f32; 2]> {
    let cos_r = rotation.cos();
    let sin_r = rotation.sin();

    uvs.iter()
        .map(|&[u, v]| {
            // Escalar
            let su = u * scale[0];
            let sv = v * scale[1];
            // Rotar (counter-clockwise)
            let ru = cos_r * su + sin_r * sv;
            let rv = -sin_r * su + cos_r * sv;
            // Trasladar
            [ru + offset[0], rv + offset[1]]
        })
        .collect()
}

/// Recolecta texture transforms de un `texture::Info` (para base_color, emissive, etc.).
fn collect_texture_transforms(
    info: &gltf::texture::Info,
    mat_idx: usize,
    pending: &mut Vec<PendingUvTransform>,
) {
    if let Some(tt) = info.texture_transform() {
        let tex_coord = tt.tex_coord().unwrap_or_else(|| info.tex_coord());
        pending.push(PendingUvTransform {
            material_index: mat_idx,
            tex_coord_set: tex_coord,
            offset: tt.offset(),
            rotation: tt.rotation(),
            scale: tt.scale(),
        });
    }
}

/// Recolecta texture transforms de un `NormalTexture`.
fn collect_texture_transforms_normal(
    info: &gltf::material::NormalTexture,
    mat_idx: usize,
    pending: &mut Vec<PendingUvTransform>,
) {
    // NormalTexture no expone texture_transform() directamente,
    // el tex_coord viene del JSON subyacente. Omitimos transform para normal maps.
    let _ = (info, mat_idx, pending);
}

/// Recolecta texture transforms de un `OcclusionTexture`.
fn collect_texture_transforms_occlusion(
    info: &gltf::material::OcclusionTexture,
    mat_idx: usize,
    pending: &mut Vec<PendingUvTransform>,
) {
    let _ = (info, mat_idx, pending);
}

/// Resuelve el tex_coord con override de texture_transform.
fn resolve_tex_coord(info: &gltf::texture::Info) -> u32 {
    if let Some(tt) = info.texture_transform() {
        tt.tex_coord().unwrap_or_else(|| info.tex_coord())
    } else {
        info.tex_coord()
    }
}

/// Importa un archivo glTF 2.0 o GLB y lo convierte a `Scene`.
///
/// Soporta geometría completa (posiciones, normales, tangentes, UVs, colores,
/// joints, weights), materiales PBR, texturas embebidas/externas, esqueletos
/// y animaciones.
pub fn import_gltf(path: impl AsRef<Path>) -> Result<Scene, GltfImportError> {
    let path = path.as_ref();
    let (doc, buffers, images) = gltf::import(path)?;

    // Verificar extensiones requeridas no soportadas
    for ext in doc.extensions_required() {
        if ext == "KHR_draco_mesh_compression" {
            return Err(GltfImportError::UnsupportedExtension(
                ext.to_string(),
                "decodificación Draco no implementada".to_string(),
            ));
        }
    }

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut scene = Scene::new();

    import_textures(&doc, &buffers, &images, base_dir, &mut scene)?;
    let pending_uv_transforms = import_materials(&doc, &mut scene);
    import_meshes(&doc, &buffers, &pending_uv_transforms, &mut scene);
    import_nodes(&doc, &mut scene);
    import_skins(&doc, &buffers, &mut scene);
    import_animations(&doc, &buffers, &mut scene);

    scene.validate_geometry()?;
    Ok(scene)
}

/// Importa un GLB desde bytes en memoria (útil para WASM).
///
/// Solo soporta GLB (binario), no glTF con archivos externos.
pub fn import_gltf_bytes(data: &[u8]) -> Result<Scene, GltfImportError> {
    let gltf::Glb { json, bin, .. } = gltf::Glb::from_slice(data)?;
    let root: gltf::json::Root = gltf::json::Root::from_slice(&json)
        .map_err(|e| gltf::Error::Deserialize(e))?;
    let doc = gltf::Document::from_json_without_validation(root);

    // Verificar extensiones requeridas
    for ext in doc.extensions_required() {
        if ext == "KHR_draco_mesh_compression" {
            return Err(GltfImportError::UnsupportedExtension(
                ext.to_string(),
                "decodificación Draco no implementada".to_string(),
            ));
        }
    }

    // El bin chunk es el único buffer para GLB
    let buffers: Vec<gltf::buffer::Data> = if let Some(bin_data) = bin {
        vec![gltf::buffer::Data(bin_data.into_owned())]
    } else {
        vec![]
    };

    // Extraer imágenes manualmente (no podemos usar gltf::import)
    let images: Vec<gltf::image::Data> = Vec::new();

    let base_dir = Path::new(".");

    let mut scene = Scene::new();

    import_textures(&doc, &buffers, &images, base_dir, &mut scene)?;
    let pending_uv_transforms = import_materials(&doc, &mut scene);
    import_meshes(&doc, &buffers, &pending_uv_transforms, &mut scene);
    import_nodes(&doc, &mut scene);
    import_skins(&doc, &buffers, &mut scene);
    import_animations(&doc, &buffers, &mut scene);

    scene.validate_geometry()?;
    Ok(scene)
}

// ---------------------------------------------------------------------------
// Texturas
// ---------------------------------------------------------------------------

fn import_textures(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    base_dir: &Path,
    scene: &mut Scene,
) -> Result<(), GltfImportError> {
    for (i, image) in doc.images().enumerate() {
        let (data, format) = extract_raw_image(&image, buffers, base_dir)?;
        let (width, height) = if i < images.len() {
            (images[i].width, images[i].height)
        } else {
            image_dimensions(&data, format)
        };

        scene.textures.push(Texture {
            name: image.name().unwrap_or("").to_string(),
            data,
            format,
            width,
            height,
        });
    }
    Ok(())
}

fn extract_raw_image(
    image: &gltf::Image,
    buffers: &[gltf::buffer::Data],
    base_dir: &Path,
) -> Result<(Vec<u8>, TextureFormat), GltfImportError> {
    match image.source() {
        gltf::image::Source::View { view, mime_type } => {
            let start = view.offset();
            let end = start + view.length();
            let data = buffers[view.buffer().index()][start..end].to_vec();
            Ok((data, mime_to_format(mime_type)))
        }
        gltf::image::Source::Uri { uri, mime_type } => {
            if uri.starts_with("data:") {
                let data = decode_data_uri(uri).unwrap_or_default();
                let fmt = mime_type
                    .map(mime_to_format)
                    .unwrap_or_else(|| sniff_format(&data));
                Ok((data, fmt))
            } else {
                let decoded_uri = percent_decode(uri);
                let img_path = base_dir.join(&decoded_uri);
                let data = std::fs::read(&img_path)?;
                let fmt = mime_type
                    .map(mime_to_format)
                    .unwrap_or_else(|| format_from_extension(&decoded_uri));
                Ok((data, fmt))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Materiales
// ---------------------------------------------------------------------------

/// Convierte parámetros specular-glossiness a metallic-roughness.
///
/// Aproximación simplificada:
/// - `metallic ≈ 1 - max(specular)` (dieléctricos tienen specular bajo)
/// - `roughness = 1 - glossiness`
/// - `base_color ≈ diffuse`
fn convert_spec_gloss(
    diffuse: [f32; 4],
    specular: [f32; 3],
    glossiness: f32,
) -> ([f32; 4], f32, f32) {
    let max_spec = specular[0].max(specular[1]).max(specular[2]);
    let metallic = (1.0 - max_spec).clamp(0.0, 1.0);
    let roughness = (1.0 - glossiness).clamp(0.0, 1.0);
    (diffuse, metallic, roughness)
}

fn import_materials(doc: &gltf::Document, scene: &mut Scene) -> Vec<PendingUvTransform> {
    let mut pending_transforms = Vec::new();

    for (mat_idx, mat) in doc.materials().enumerate() {
        // Determinar base_color, metallic, roughness y texturas base
        let (base_color_factor, metallic_factor, roughness_factor, base_color_texture, mr_texture) =
            if let Some(sg) = mat.pbr_specular_glossiness() {
                let (bc, met, rough) =
                    convert_spec_gloss(sg.diffuse_factor(), sg.specular_factor(), sg.glossiness_factor());
                let bc_tex = sg.diffuse_texture().map(|info| {
                    collect_texture_transforms(&info, mat_idx, &mut pending_transforms);
                    TextureRef {
                        texture_index: info.texture().source().index(),
                        tex_coord_set: resolve_tex_coord(&info),
                    }
                });
                (bc, met, rough, bc_tex, None)
            } else {
                let pbr = mat.pbr_metallic_roughness();
                let bc_tex = pbr.base_color_texture().map(|info| {
                    collect_texture_transforms(&info, mat_idx, &mut pending_transforms);
                    TextureRef {
                        texture_index: info.texture().source().index(),
                        tex_coord_set: resolve_tex_coord(&info),
                    }
                });
                let mr_tex = pbr.metallic_roughness_texture().map(|info| {
                    collect_texture_transforms(&info, mat_idx, &mut pending_transforms);
                    TextureRef {
                        texture_index: info.texture().source().index(),
                        tex_coord_set: resolve_tex_coord(&info),
                    }
                });
                (pbr.base_color_factor(), pbr.metallic_factor(), pbr.roughness_factor(), bc_tex, mr_tex)
            };

        // Texturas del material base (normal, occlusion, emissive)
        let normal_texture = mat.normal_texture().map(|info| {
            collect_texture_transforms_normal(&info, mat_idx, &mut pending_transforms);
            TextureRef {
                texture_index: info.texture().source().index(),
                tex_coord_set: info.tex_coord(),
            }
        });
        let occlusion_texture = mat.occlusion_texture().map(|info| {
            collect_texture_transforms_occlusion(&info, mat_idx, &mut pending_transforms);
            TextureRef {
                texture_index: info.texture().source().index(),
                tex_coord_set: info.tex_coord(),
            }
        });
        let emissive_texture = mat.emissive_texture().map(|info| {
            collect_texture_transforms(&info, mat_idx, &mut pending_transforms);
            TextureRef {
                texture_index: info.texture().source().index(),
                tex_coord_set: resolve_tex_coord(&info),
            }
        });

        scene.materials.push(Material {
            name: mat.name().unwrap_or("").to_string(),
            base_color_factor,
            base_color_texture,
            metallic_factor,
            roughness_factor,
            metallic_roughness_texture: mr_texture,
            normal_texture,
            normal_scale: mat.normal_texture().map(|t| t.scale()).unwrap_or(1.0),
            occlusion_texture,
            occlusion_strength: mat
                .occlusion_texture()
                .map(|t| t.strength())
                .unwrap_or(1.0),
            emissive_factor: mat.emissive_factor(),
            emissive_texture,
            alpha_mode: match mat.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => {
                    AlphaMode::Mask(mat.alpha_cutoff().unwrap_or(0.5))
                }
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            double_sided: mat.double_sided(),
            unlit: mat.unlit(),
        });
    }

    pending_transforms
}

// ---------------------------------------------------------------------------
// Meshes
// ---------------------------------------------------------------------------

fn import_meshes(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    pending_transforms: &[PendingUvTransform],
    scene: &mut Scene,
) {
    for mesh in doc.meshes() {
        let mut primitives = Vec::new();

        for prim in mesh.primitives() {
            let mode = prim.mode();
            if !matches!(
                mode,
                gltf::mesh::Mode::Triangles
                    | gltf::mesh::Mode::TriangleStrip
                    | gltf::mesh::Mode::TriangleFan
            ) {
                continue;
            }

            let reader = prim.reader(|buf| Some(&buffers[buf.index()]));

            // Posiciones (obligatorias)
            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(iter) => iter.collect(),
                None => continue,
            };
            let vertex_count = positions.len();
            let mut attributes = vec![VertexAttribute::Positions(positions)];

            // Normales
            if let Some(iter) = reader.read_normals() {
                attributes.push(VertexAttribute::Normals(iter.collect()));
            }

            // Tangentes
            if let Some(iter) = reader.read_tangents() {
                attributes.push(VertexAttribute::Tangents(iter.collect()));
            }

            // Coordenadas UV (hasta 4 sets)
            for set in 0..4u32 {
                if let Some(iter) = reader.read_tex_coords(set) {
                    attributes
                        .push(VertexAttribute::TexCoords(set, iter.into_f32().collect()));
                } else {
                    break;
                }
            }

            // Aplicar transforms de textura pendientes (KHR_texture_transform)
            if let Some(mat_idx) = prim.material().index() {
                let transforms: Vec<&PendingUvTransform> = pending_transforms
                    .iter()
                    .filter(|t| t.material_index == mat_idx)
                    .collect();

                for pt in &transforms {
                    // Buscar el set UV correspondiente y transformarlo in-place
                    for attr in &mut attributes {
                        if let VertexAttribute::TexCoords(set, uvs) = attr {
                            if *set == pt.tex_coord_set {
                                *uvs = apply_texture_transform(
                                    uvs, pt.offset, pt.rotation, pt.scale,
                                );
                                break;
                            }
                        }
                    }
                }
            }

            // Colores de vértice
            if let Some(iter) = reader.read_colors(0) {
                attributes.push(VertexAttribute::Colors(iter.into_rgba_f32().collect()));
            }

            // Joint indices (skinning)
            if let Some(iter) = reader.read_joints(0) {
                attributes.push(VertexAttribute::JointIndices(iter.into_u16().collect()));
            }

            // Joint weights (skinning)
            if let Some(iter) = reader.read_weights(0) {
                attributes.push(VertexAttribute::JointWeights(iter.into_f32().collect()));
            }

            // Índices
            let indices = if mode == gltf::mesh::Mode::Triangles {
                reader
                    .read_indices()
                    .map(|iter| IndexData::U32(iter.into_u32().collect()))
            } else {
                // TriangleStrip / TriangleFan → convertir a lista de triángulos
                let raw: Vec<u32> = reader
                    .read_indices()
                    .map(|iter| iter.into_u32().collect())
                    .unwrap_or_else(|| (0..vertex_count as u32).collect());
                let converted = convert_indices_for_mode(raw, mode);
                if converted.is_empty() {
                    // Sin triángulos: `None` se interpretaría como lista implícita
                    continue;
                }
                Some(IndexData::U32(converted))
            };

            primitives.push(Primitive {
                attributes,
                indices,
                material: prim.material().index(),
            });
        }

        scene.meshes.push(Mesh {
            name: mesh.name().unwrap_or("").to_string(),
            primitives,
        });
    }
}

/// Convierte TriangleStrip o TriangleFan a lista de triángulos.
fn convert_indices_for_mode(indices: Vec<u32>, mode: gltf::mesh::Mode) -> Vec<u32> {
    match mode {
        gltf::mesh::Mode::Triangles => indices,
        gltf::mesh::Mode::TriangleStrip => {
            if indices.len() < 3 {
                return Vec::new();
            }
            let mut tris = Vec::with_capacity((indices.len() - 2) * 3);
            for i in 0..indices.len() - 2 {
                if i % 2 == 0 {
                    tris.extend_from_slice(&[indices[i], indices[i + 1], indices[i + 2]]);
                } else {
                    // Alternar winding para mantener orientación correcta
                    tris.extend_from_slice(&[indices[i + 1], indices[i], indices[i + 2]]);
                }
            }
            tris
        }
        gltf::mesh::Mode::TriangleFan => {
            if indices.len() < 3 {
                return Vec::new();
            }
            let mut tris = Vec::with_capacity((indices.len() - 2) * 3);
            for i in 1..indices.len() - 1 {
                tris.extend_from_slice(&[indices[0], indices[i], indices[i + 1]]);
            }
            tris
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Grafo de escena (nodos)
// ---------------------------------------------------------------------------

fn import_nodes(doc: &gltf::Document, scene: &mut Scene) {
    for node in doc.nodes() {
        let (t, r, s) = node.transform().decomposed();
        scene.nodes.push(Node {
            name: node.name().unwrap_or("").to_string(),
            transform: Transform::Trs {
                translation: Vec3::from(t),
                rotation: Quat::from_array(r),
                scale: Vec3::from(s),
            },
            mesh: node.mesh().map(|m| m.index()),
            skin: node.skin().map(|s| s.index()),
            children: node.children().map(|c| c.index()).collect(),
        });
    }

    // Seleccionar escena default o la primera disponible
    if let Some(gltf_scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        scene.root_nodes = gltf_scene.nodes().map(|n| n.index()).collect();
    }
}

// ---------------------------------------------------------------------------
// Esqueletos (skins)
// ---------------------------------------------------------------------------

fn import_skins(doc: &gltf::Document, buffers: &[gltf::buffer::Data], scene: &mut Scene) {
    for skin in doc.skins() {
        let joint_nodes: Vec<gltf::Node> = skin.joints().collect();
        let joint_count = joint_nodes.len();

        // Mapa: índice de nodo glTF → índice de joint en el skeleton
        let node_to_joint: std::collections::HashMap<usize, usize> = joint_nodes
            .iter()
            .enumerate()
            .map(|(ji, node)| (node.index(), ji))
            .collect();

        let reader = skin.reader(|buf| Some(&buffers[buf.index()]));
        let ibms: Vec<[[f32; 4]; 4]> = reader
            .read_inverse_bind_matrices()
            .map(|iter| iter.collect())
            .unwrap_or_else(|| vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count]);

        let mut joints = Vec::with_capacity(joint_count);

        for (ji, node) in joint_nodes.iter().enumerate() {
            let children: Vec<usize> = node
                .children()
                .filter_map(|child| node_to_joint.get(&child.index()).copied())
                .collect();

            let (t, r, s) = node.transform().decomposed();
            let local_transform = Mat4::from_scale_rotation_translation(
                Vec3::from(s),
                Quat::from_array(r),
                Vec3::from(t),
            );

            joints.push(Joint {
                name: node.name().unwrap_or("").to_string(),
                children,
                inverse_bind_matrix: Mat4::from_cols_array_2d(&ibms[ji]),
                local_transform,
                node_index: Some(node.index()),
            });
        }

        // Encontrar joints raíz: los que ningún otro joint lista como hijo
        let all_children: HashSet<usize> =
            joints.iter().flat_map(|j| &j.children).copied().collect();
        let roots: Vec<usize> = (0..joint_count)
            .filter(|ji| !all_children.contains(ji))
            .collect();

        scene.skeletons.push(Skeleton {
            name: skin.name().unwrap_or("").to_string(),
            joints,
            roots,
        });
    }
}

// ---------------------------------------------------------------------------
// Animaciones
// ---------------------------------------------------------------------------

fn import_animations(doc: &gltf::Document, buffers: &[gltf::buffer::Data], scene: &mut Scene) {
    for anim in doc.animations() {
        let mut channels = Vec::new();

        for channel in anim.channels() {
            let target = channel.target();
            let node_idx = target.node().index();

            let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

            let times: Vec<f32> = match reader.read_inputs() {
                Some(iter) => iter.collect(),
                None => continue,
            };

            let interp = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            };

            let outputs = match reader.read_outputs() {
                Some(outputs) => outputs,
                None => continue,
            };

            let values = match outputs {
                gltf::animation::util::ReadOutputs::Translations(iter) => {
                    KeyframeValues::Translation(iter.collect())
                }
                gltf::animation::util::ReadOutputs::Rotations(iter) => {
                    KeyframeValues::Rotation(iter.into_f32().collect())
                }
                gltf::animation::util::ReadOutputs::Scales(iter) => {
                    KeyframeValues::Scale(iter.collect())
                }
                gltf::animation::util::ReadOutputs::MorphTargetWeights(iter) => {
                    KeyframeValues::Weights(iter.into_f32().collect())
                }
            };

            channels.push(Channel {
                node: node_idx,
                interpolation: interp,
                times,
                values,
            });
        }

        scene.animations.push(Animation {
            name: anim.name().unwrap_or("").to_string(),
            channels,
        });
    }
}

// ---------------------------------------------------------------------------
// Utilidades de imagen
// ---------------------------------------------------------------------------

fn mime_to_format(mime: &str) -> TextureFormat {
    match mime {
        "image/png" => TextureFormat::Png,
        "image/jpeg" => TextureFormat::Jpeg,
        "image/webp" => TextureFormat::WebP,
        _ => TextureFormat::Png,
    }
}

fn format_from_extension(path: &str) -> TextureFormat {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        TextureFormat::Jpeg
    } else if lower.ends_with(".webp") {
        TextureFormat::WebP
    } else {
        TextureFormat::Png
    }
}

fn sniff_format(data: &[u8]) -> TextureFormat {
    if data.starts_with(b"\x89PNG") {
        TextureFormat::Png
    } else if data.starts_with(b"\xFF\xD8\xFF") {
        TextureFormat::Jpeg
    } else if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        TextureFormat::WebP
    } else {
        TextureFormat::Png
    }
}

fn image_dimensions(data: &[u8], format: TextureFormat) -> (u32, u32) {
    match format {
        TextureFormat::Png => png_dimensions(data).unwrap_or((0, 0)),
        TextureFormat::Jpeg => jpeg_dimensions(data).unwrap_or((0, 0)),
        TextureFormat::WebP => (0, 0),
    }
}

fn png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((w, h))
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || &data[0..2] != &[0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // SOF markers (Start of Frame) contienen las dimensiones
        if (0xC0..=0xC3).contains(&marker) || (0xC5..=0xC7).contains(&marker)
            || (0xC9..=0xCB).contains(&marker) || (0xCD..=0xCF).contains(&marker)
        {
            if i + 9 < data.len() {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
        }
        if i + 3 < data.len() {
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + len;
        } else {
            break;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Utilidades de URIs
// ---------------------------------------------------------------------------

fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    let comma = uri.find(',')?;
    let data_part = &uri[comma + 1..];
    if uri[..comma].contains(";base64") {
        Some(base64_decode(data_part.as_bytes()))
    } else {
        Some(percent_decode(data_part).into_bytes())
    }
}

fn base64_decode(input: &[u8]) -> Vec<u8> {
    const DECODE: [u8; 128] = {
        let mut table = [255u8; 128];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i;
            i += 1;
        }
        i = 0;
        while i < 26 {
            table[(b'a' + i) as usize] = 26 + i;
            i += 1;
        }
        i = 0;
        while i < 10 {
            table[(b'0' + i) as usize] = 52 + i;
            i += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &byte in input {
        if byte == b'=' || byte == b'\n' || byte == b'\r' || byte == b' ' {
            continue;
        }
        let val = if (byte as usize) < 128 {
            DECODE[byte as usize]
        } else {
            255
        };
        if val == 255 {
            continue;
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    output
}

fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                result.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::VertexAttribute;

    /// Construye un archivo GLB binario mínimo con un triángulo.
    fn build_minimal_glb() -> Vec<u8> {
        // Buffer binario: 3 posiciones (f32×3) + 3 índices (u16) + padding
        let positions: [[f32; 3]; 3] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for p in &positions {
            for &v in p {
                bin.extend_from_slice(&v.to_le_bytes());
            }
        }
        // Positions: 3×3×4 = 36 bytes
        for &i in &indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        // Indices: 3×2 = 6 bytes, total = 42
        // Pad to 4-byte alignment
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        // bin.len() = 44

        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "name": "Triangle", "mesh": 0 }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 },
                    "indices": 1
                }]
            }],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3",
                    "max": [1.0, 1.0, 0.0],
                    "min": [0.0, 0.0, 0.0]
                },
                {
                    "bufferView": 1,
                    "componentType": 5123,
                    "count": 3,
                    "type": "SCALAR",
                    "max": [2],
                    "min": [0]
                }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 6 }
            ],
            "buffers": [{ "byteLength": bin.len() }]
        });

        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        // Pad JSON to 4-byte alignment (con espacios, spec GLB)
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();

        let mut glb = Vec::with_capacity(total_length);
        // Header
        glb.extend_from_slice(b"glTF");                         // magic
        glb.extend_from_slice(&2u32.to_le_bytes());             // version
        glb.extend_from_slice(&(total_length as u32).to_le_bytes()); // length

        // JSON chunk
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes()); // chunk length
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());            // "JSON"
        glb.extend_from_slice(&json_bytes);

        // BIN chunk
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes()); // chunk length
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());      // "BIN\0"
        glb.extend_from_slice(&bin);

        glb
    }

    /// Construye un GLB con material PBR.
    fn build_glb_with_material() -> Vec<u8> {
        let positions: [[f32; 3]; 3] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let normals: [[f32; 3]; 3] = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for p in &positions {
            for &v in p { bin.extend_from_slice(&v.to_le_bytes()); }
        }
        for n in &normals {
            for &v in n { bin.extend_from_slice(&v.to_le_bytes()); }
        }
        for &i in &indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        while bin.len() % 4 != 0 { bin.push(0); }

        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "mesh": 0 }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0, "NORMAL": 1 },
                    "indices": 2,
                    "material": 0
                }]
            }],
            "materials": [{
                "name": "Red",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1.0, 0.0, 0.0, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.8
                },
                "doubleSided": true
            }],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                  "max": [1.0, 1.0, 0.0], "min": [0.0, 0.0, 0.0] },
                { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3",
                  "max": [0.0, 0.0, 1.0], "min": [0.0, 0.0, 1.0] },
                { "bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR",
                  "max": [2], "min": [0] }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 72, "byteLength": 6 }
            ],
            "buffers": [{ "byteLength": bin.len() }]
        });

        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        while json_bytes.len() % 4 != 0 { json_bytes.push(b' '); }

        let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total_length);

        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());

        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);

        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin);

        glb
    }

    fn write_glb(glb_data: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.glb");
        std::fs::write(&path, glb_data).unwrap();
        (dir, path)
    }

    #[test]
    fn import_minimal_triangle() {
        let glb = build_minimal_glb();
        let (_dir, path) = write_glb(&glb);

        let scene = import_gltf(&path).unwrap();

        // Un mesh con una primitiva
        assert_eq!(scene.meshes.len(), 1);
        assert_eq!(scene.meshes[0].primitives.len(), 1);

        let prim = &scene.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(positions[2], [0.0, 1.0, 0.0]);

        // Índices
        match &prim.indices {
            Some(IndexData::U32(idx)) => assert_eq!(idx, &[0, 1, 2]),
            _ => panic!("esperaba U32 indices"),
        }

        // Un nodo con el mesh
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].mesh, Some(0));
        assert_eq!(scene.nodes[0].name, "Triangle");

        // Root nodes
        assert_eq!(scene.root_nodes, vec![0]);

        // Sin materiales, texturas, skeletons, animaciones
        assert!(scene.materials.is_empty());
        assert!(scene.textures.is_empty());
        assert!(scene.skeletons.is_empty());
        assert!(scene.animations.is_empty());
    }

    #[test]
    fn import_with_material() {
        let glb = build_glb_with_material();
        let (_dir, path) = write_glb(&glb);

        let scene = import_gltf(&path).unwrap();

        // Material
        assert_eq!(scene.materials.len(), 1);
        let mat = &scene.materials[0];
        assert_eq!(mat.name, "Red");
        assert_eq!(mat.base_color_factor, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(mat.metallic_factor, 0.0);
        assert!((mat.roughness_factor - 0.8).abs() < 1e-6);
        assert!(mat.double_sided);

        // La primitiva referencia el material
        assert_eq!(scene.meshes[0].primitives[0].material, Some(0));

        // Tiene normales
        let prim = &scene.meshes[0].primitives[0];
        let normals = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Normals(n) = a { Some(n) } else { None }
        });
        assert!(normals.is_some());
        assert_eq!(normals.unwrap().len(), 3);
    }

    #[test]
    fn import_bounding_box() {
        let glb = build_minimal_glb();
        let (_dir, path) = write_glb(&glb);

        let scene = import_gltf(&path).unwrap();
        let (min, max) = scene.compute_bounding_box().unwrap();

        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 0.0]);
    }

    #[test]
    fn base64_roundtrip() {
        let original = b"Hello, World!";
        // "SGVsbG8sIFdvcmxkIQ=="
        let encoded = b"SGVsbG8sIFdvcmxkIQ==";
        let decoded = base64_decode(encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn percent_decode_test() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("no%2Fslash"), "no/slash");
        assert_eq!(percent_decode("plain"), "plain");
    }

    // -----------------------------------------------------------------------
    // KHR_materials_unlit
    // -----------------------------------------------------------------------

    #[test]
    fn import_unlit_material() {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for p in &positions {
            for &v in p { bin.extend_from_slice(&v.to_le_bytes()); }
        }
        for &i in &indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        while bin.len() % 4 != 0 { bin.push(0); }

        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "extensionsUsed": ["KHR_materials_unlit"],
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "mesh": 0 }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 },
                    "indices": 1,
                    "material": 0
                }]
            }],
            "materials": [{
                "name": "Unlit",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.8, 0.2, 0.1, 1.0]
                },
                "extensions": {
                    "KHR_materials_unlit": {}
                }
            }],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                  "max": [1.0, 1.0, 0.0], "min": [0.0, 0.0, 0.0] },
                { "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR",
                  "max": [2], "min": [0] }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 6 }
            ],
            "buffers": [{ "byteLength": bin.len() }]
        });

        let glb = build_glb_from_json_bin(&json, &bin);
        let (_dir, path) = write_glb(&glb);
        let scene = import_gltf(&path).unwrap();

        assert_eq!(scene.materials.len(), 1);
        assert!(scene.materials[0].unlit);
        assert_eq!(scene.materials[0].base_color_factor, [0.8, 0.2, 0.1, 1.0]);
    }

    // -----------------------------------------------------------------------
    // KHR_draco_mesh_compression
    // -----------------------------------------------------------------------

    #[test]
    fn draco_required_error() {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices: [u16; 3] = [0, 1, 2];

        let mut bin = Vec::new();
        for p in &positions {
            for &v in p { bin.extend_from_slice(&v.to_le_bytes()); }
        }
        for &i in &indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        while bin.len() % 4 != 0 { bin.push(0); }

        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "extensionsUsed": ["KHR_draco_mesh_compression"],
            "extensionsRequired": ["KHR_draco_mesh_compression"],
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "mesh": 0 }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "POSITION": 0 },
                    "indices": 1
                }]
            }],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                  "max": [1.0, 1.0, 0.0], "min": [0.0, 0.0, 0.0] },
                { "bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR",
                  "max": [2], "min": [0] }
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 6 }
            ],
            "buffers": [{ "byteLength": bin.len() }]
        });

        let glb = build_glb_from_json_bin(&json, &bin);
        let (_dir, path) = write_glb(&glb);
        let result = import_gltf(&path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("KHR_draco_mesh_compression"),
            "error debería mencionar Draco: {}",
            err
        );
    }

    // -----------------------------------------------------------------------
    // KHR_materials_pbrSpecularGlossiness
    // -----------------------------------------------------------------------

    #[test]
    fn convert_spec_gloss_dielectric() {
        // Specular bajo → dieléctrico (metallic alto ~= 1 - 0.04 = 0.96)
        let (bc, met, rough) = convert_spec_gloss(
            [0.8, 0.2, 0.1, 1.0],
            [0.04, 0.04, 0.04],
            0.5,
        );
        assert_eq!(bc, [0.8, 0.2, 0.1, 1.0]);
        assert!((met - 0.96).abs() < 1e-5, "metallic={}", met);
        assert!((rough - 0.5).abs() < 1e-5, "roughness={}", rough);
    }

    #[test]
    fn convert_spec_gloss_metallic() {
        // Specular alto → metálico (metallic bajo ~= 1 - 0.95 = 0.05)
        let (bc, met, rough) = convert_spec_gloss(
            [0.9, 0.9, 0.9, 1.0],
            [0.95, 0.93, 0.88],
            0.8,
        );
        assert_eq!(bc, [0.9, 0.9, 0.9, 1.0]);
        assert!((met - 0.05).abs() < 1e-5, "metallic={}", met);
        assert!((rough - 0.2).abs() < 1e-5, "roughness={}", rough);
    }

    // -----------------------------------------------------------------------
    // KHR_texture_transform
    // -----------------------------------------------------------------------

    #[test]
    fn apply_texture_transform_identity() {
        let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let result = apply_texture_transform(&uvs, [0.0, 0.0], 0.0, [1.0, 1.0]);
        for (a, b) in result.iter().zip(uvs.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-6);
            assert!((a[1] - b[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn apply_texture_transform_offset_scale() {
        let uvs = vec![[0.0, 0.0], [1.0, 1.0]];
        let result = apply_texture_transform(&uvs, [0.5, 0.25], 0.0, [2.0, 3.0]);
        // [0,0] * [2,3] + [0.5, 0.25] = [0.5, 0.25]
        assert!((result[0][0] - 0.5).abs() < 1e-6);
        assert!((result[0][1] - 0.25).abs() < 1e-6);
        // [1,1] * [2,3] + [0.5, 0.25] = [2.5, 3.25]
        assert!((result[1][0] - 2.5).abs() < 1e-6);
        assert!((result[1][1] - 3.25).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // Helper para construir GLB desde JSON + bin
    // -----------------------------------------------------------------------

    fn build_glb_from_json_bin(json: &serde_json::Value, bin: &[u8]) -> Vec<u8> {
        let mut json_bytes = serde_json::to_vec(json).unwrap();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }

        let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total_length);

        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());

        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json_bytes);

        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin);

        glb
    }
}
