//! Piel de una malla en términos de una escena (`converter-scene`): UV por
//! esquina de cara más los materiales y texturas que usan.

use crate::{
    bake, corner_frames, transfer_uvs, unwrap, BakeChannel, CornerFrames, TexelContext, UnwrapOptions, UvPart, UvSurface,
};
use converter_scene::{AlphaMode, Material, Scene, Texture, TextureFormat, TextureRef, WorldPrimitive};
use image::RgbaImage;
use rayon::prelude::*;
use std::collections::HashMap;

/// UV, material y texturas de una malla de caras de `N` vértices.
#[derive(Debug, Clone)]
pub struct Skin<const N: usize> {
    /// UV por cara y esquina.
    pub corners: Vec<[[f32; 2]; N]>,
    /// Material de cada cara (índice en `materials`).
    pub face_material: Vec<Option<usize>>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub info: SkinInfo,
}

/// Cómo se obtuvo la piel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkinInfo {
    /// UV trasladadas del modelo original.
    Transferred {
        /// Caras que cruzan una costura del mapa original.
        seam_faces: usize,
    },
    /// Desplegado nuevo, con las texturas originales horneadas si había.
    Unwrapped {
        num_charts: usize,
        stretch: f64,
        coverage: f64,
        /// Tamaño de las texturas horneadas (0 si no se horneó nada).
        texture_size: u32,
    },
}

/// Superficie con las UV de la escena. Grupo = material + 1 (0 = sin
/// material). `None` si ninguna primitiva tiene UV.
///
/// Las primitivas sin UV (una pieza de color liso) entran con UV en cero:
/// si quedaran afuera, las caras que caen sobre ellas tomarían el material y
/// la textura de la pieza con UV más cercana.
pub fn scene_surface(scene: &Scene) -> Option<UvSurface> {
    let prims = scene.world_primitives();
    if !prims.iter().any(|p| p.uvs.is_some()) {
        return None;
    }
    primitives_surface(&prims)
}

/// Superficie de la escena aunque no tenga UV (todas en cero): sirve para
/// trasladar solo el material de cada cara.
pub fn material_surface(scene: &Scene) -> Option<UvSurface> {
    primitives_surface(&scene.world_primitives())
}

fn primitives_surface(prims: &[WorldPrimitive]) -> Option<UvSurface> {
    let zeros: Vec<Vec<[f32; 2]>> =
        prims.iter().map(|p| if p.uvs.is_some() { Vec::new() } else { vec![[0.0; 2]; p.positions.len()] }).collect();
    UvSurface::new(prims.iter().zip(&zeros).map(|(p, zeros)| UvPart {
        group: p.material.map_or(0, |m| m + 1),
        positions: &p.positions,
        uvs: p.uvs.as_deref().unwrap_or(zeros),
        normals: p.normals.as_deref(),
        triangles: &p.triangles,
    }))
}

/// UV de la escena llevadas a otra malla; conserva los materiales.
pub fn transferred_skin<const N: usize>(
    scene: &Scene,
    surface: &UvSurface,
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
) -> Skin<N> {
    let transfer = transfer_uvs(surface, positions, faces);
    Skin {
        face_material: transfer.groups.iter().map(|&g| g.checked_sub(1).filter(|&m| m < scene.materials.len())).collect(),
        corners: transfer.corners,
        materials: scene.materials.clone(),
        textures: scene.textures.clone(),
        info: SkinInfo::Transferred { seam_faces: transfer.seam_faces },
    }
}

/// Opciones de [`unwrapped_skin`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeOptions {
    pub unwrap: UnwrapOptions,
    /// Lado de las texturas horneadas (px). Se usa también como
    /// `unwrap.texture_size` para el margen entre cartas.
    pub texture_size: u32,
}

impl Default for BakeOptions {
    fn default() -> Self {
        Self { unwrap: UnwrapOptions::default(), texture_size: 2048 }
    }
}

/// Despliega la malla y hornea sobre el mapa nuevo los materiales de la
/// escena (si `surface` existe). Todas las caras quedan con un solo material.
///
/// La normal se hornea siempre que haya superficie de referencia: la normal
/// de sombreado del original (más su normal map, si tiene) en el espacio
/// tangente de la malla nueva, así la malla liviana conserva el detalle.
pub fn unwrapped_skin<const N: usize>(
    scene: &Scene,
    surface: Option<&UvSurface>,
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    options: &BakeOptions,
) -> Skin<N> {
    let size = options.texture_size.max(1);
    let unwrap_options = UnwrapOptions { texture_size: size, ..options.unwrap };
    let layout = unwrap(positions, faces, &unwrap_options);

    let (material, textures) = match surface {
        Some(surface) => {
            let frames = corner_frames(positions, faces, &layout.corners);
            let bake_input = BakeInput { surface, positions, faces, corners: &layout.corners, frames: &frames, size };
            bake_materials(scene, &bake_input, unwrap_options.padding.max(2) * 2)
        }
        None => (single_material(scene), Vec::new()),
    };
    let baked = !textures.is_empty();

    Skin {
        face_material: vec![Some(0); faces.len()],
        corners: layout.corners,
        materials: vec![material],
        textures,
        info: SkinInfo::Unwrapped {
            num_charts: layout.num_charts,
            stretch: layout.stretch,
            coverage: layout.coverage,
            texture_size: if baked { size } else { 0 },
        },
    }
}

/// Sin UV de origen: el primer material de la escena sin texturas.
fn single_material(scene: &Scene) -> Material {
    let base = scene.materials.first().cloned().unwrap_or_default();
    Material {
        base_color_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        occlusion_texture: None,
        emissive_texture: None,
        ..base
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn to_byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Muestreo bilineal con repetición, en [0, 1] por canal.
fn sample(image: &RgbaImage, uv: [f32; 2]) -> [f32; 4] {
    let (w, h) = (image.width() as i64, image.height() as i64);
    let x = uv[0] * w as f32 - 0.5;
    let y = uv[1] * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let texel = |dx: i64, dy: i64| {
        let px = (x0 as i64 + dx).rem_euclid(w) as u32;
        let py = (y0 as i64 + dy).rem_euclid(h) as u32;
        image.get_pixel(px, py).0.map(|c| c as f32 / 255.0)
    };
    let (a, b, c, d) = (texel(0, 0), texel(1, 0), texel(0, 1), texel(1, 1));
    std::array::from_fn(|k| {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bottom = c[k] + (d[k] - c[k]) * fx;
        top + (bottom - top) * fy
    })
}

/// Canal de material a hornear.
#[derive(Clone, Copy, PartialEq)]
enum Channel {
    BaseColor,
    MetallicRoughness,
    Occlusion,
    Emissive,
    Normal,
}

impl Channel {
    fn texture(self, m: &Material) -> Option<&TextureRef> {
        match self {
            Channel::BaseColor => m.base_color_texture.as_ref(),
            Channel::MetallicRoughness => m.metallic_roughness_texture.as_ref(),
            Channel::Occlusion => m.occlusion_texture.as_ref(),
            Channel::Emissive => m.emissive_texture.as_ref(),
            Channel::Normal => m.normal_texture.as_ref(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Channel::BaseColor => "baked_base_color",
            Channel::MetallicRoughness => "baked_metallic_roughness",
            Channel::Occlusion => "baked_occlusion",
            Channel::Emissive => "baked_emissive",
            Channel::Normal => "baked_normal",
        }
    }
}

/// Malla destino y superficie de referencia para hornear.
struct BakeInput<'a, const N: usize> {
    surface: &'a UvSurface,
    positions: &'a [[f64; 3]],
    faces: &'a [[usize; N]],
    corners: &'a [[[f32; 2]; N]],
    frames: &'a CornerFrames<N>,
    size: u32,
}

type BoxedChannel<'a> = Box<dyn Fn(&TexelContext) -> [u8; 4] + Sync + 'a>;

/// Hornea los canales de los materiales de la escena en un material nuevo.
fn bake_materials<const N: usize>(scene: &Scene, input: &BakeInput<N>, padding: u32) -> (Material, Vec<Texture>) {
    // Materiales por grupo (grupo 0 = sin material)
    let default = Material::default();
    let material = |group: usize| group.checked_sub(1).and_then(|m| scene.materials.get(m)).unwrap_or(&default);
    let groups: Vec<usize> = (0..=scene.materials.len()).collect();

    // Imágenes decodificadas por índice de textura
    let mut used: Vec<usize> = scene
        .materials
        .iter()
        .flat_map(|m| {
            [&m.base_color_texture, &m.metallic_roughness_texture, &m.occlusion_texture, &m.emissive_texture, &m.normal_texture]
                .into_iter()
                .flatten()
                .map(|t| t.texture_index)
        })
        .collect();
    used.sort_unstable();
    used.dedup();
    let images: HashMap<usize, RgbaImage> = used
        .into_par_iter()
        .filter_map(|i| {
            let tex = scene.textures.get(i)?;
            Some((i, image::load_from_memory(&tex.data).ok()?.to_rgba8()))
        })
        .collect();
    let image_of = |m: &Material, channel: Channel| channel.texture(m).and_then(|r| images.get(&r.texture_index));

    // Canales necesarios: con textura en algún material, o con factores que
    // difieren (una sola malla, un solo material). La normal siempre.
    let needed: Vec<Channel> = [Channel::BaseColor, Channel::MetallicRoughness, Channel::Occlusion, Channel::Emissive, Channel::Normal]
        .into_iter()
        .filter(|&channel| {
            let any_texture = groups.iter().any(|&g| image_of(material(g), channel).is_some());
            let factors_differ = scene.materials.len() > 1
                && scene.materials.windows(2).any(|w| match channel {
                    Channel::BaseColor => w[0].base_color_factor != w[1].base_color_factor,
                    Channel::MetallicRoughness => {
                        (w[0].metallic_factor, w[0].roughness_factor) != (w[1].metallic_factor, w[1].roughness_factor)
                    }
                    Channel::Emissive => w[0].emissive_factor != w[1].emissive_factor,
                    Channel::Occlusion | Channel::Normal => false,
                });
            channel == Channel::Normal || any_texture || factors_differ
        })
        .collect();

    let shade = |channel: Channel, ctx: &TexelContext| -> [u8; 4] {
        let m = material(ctx.group);
        let tex = image_of(m, channel).map(|img| sample(img, ctx.uv));
        let t = tex.unwrap_or([1.0; 4]);
        match channel {
            Channel::BaseColor => {
                let f = m.base_color_factor;
                let rgb = [0, 1, 2].map(|k| linear_to_srgb(srgb_to_linear(t[k]) * f[k]));
                [to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2]), to_byte(t[3] * f[3])]
            }
            Channel::MetallicRoughness => [0, to_byte(t[1] * m.roughness_factor), to_byte(t[2] * m.metallic_factor), 255],
            Channel::Occlusion => {
                let ao = to_byte(1.0 + m.occlusion_strength * (t[0] - 1.0));
                [ao, ao, ao, 255]
            }
            Channel::Emissive => {
                let f = m.emissive_factor;
                let rgb = [0, 1, 2].map(|k| linear_to_srgb(srgb_to_linear(t[k]) * f[k]));
                [to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2]), 255]
            }
            Channel::Normal => {
                // Normal del original en espacio objeto
                let [st, sb, sn] = ctx.source_frame;
                let local = match tex {
                    Some(t) => {
                        let s = m.normal_scale;
                        [(2.0 * t[0] - 1.0) * s, (2.0 * t[1] - 1.0) * s, 2.0 * t[2] - 1.0]
                    }
                    None => [0.0, 0.0, 1.0],
                };
                let object: [f32; 3] = std::array::from_fn(|k| st[k] * local[0] + sb[k] * local[1] + sn[k] * local[2]);
                // Al espacio tangente de la malla nueva
                let [tt, tb, tn] = ctx.target_frame;
                let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                let mut n = [dot(object, tt), dot(object, tb), dot(object, tn).max(0.0)];
                let len = dot(n, n).sqrt();
                n = if len > 1e-6 { n.map(|c| c / len) } else { [0.0, 0.0, 1.0] };
                [to_byte(0.5 * n[0] + 0.5), to_byte(0.5 * n[1] + 0.5), to_byte(0.5 * n[2] + 0.5), 255]
            }
        }
    };
    let closures: Vec<BoxedChannel> =
        needed.iter().map(|&c| Box::new(move |ctx: &TexelContext| shade(c, ctx)) as Box<_>).collect();
    let refs: Vec<BakeChannel> = closures.iter().map(|b| b.as_ref() as BakeChannel).collect();
    let baked = bake(
        input.surface,
        input.positions,
        input.faces,
        input.corners,
        input.frames,
        input.size,
        input.size,
        padding,
        &refs,
    );

    let mut out = Material {
        name: "baked".into(),
        alpha_mode: scene
            .materials
            .iter()
            .map(|m| m.alpha_mode)
            .max_by_key(|a| match a {
                AlphaMode::Opaque => 0,
                AlphaMode::Mask(_) => 1,
                AlphaMode::Blend => 2,
            })
            .unwrap_or(AlphaMode::Opaque),
        double_sided: scene.materials.iter().any(|m| m.double_sided),
        unlit: !scene.materials.is_empty() && scene.materials.iter().all(|m| m.unlit),
        ..single_material(scene)
    };
    let mut textures = Vec::new();
    for (channel, pixels) in needed.into_iter().zip(baked.images) {
        let bytes: Vec<u8> = pixels.into_iter().flatten().collect();
        let image = RgbaImage::from_raw(baked.width, baked.height, bytes).expect("tamaño consistente");
        let mut png = Vec::new();
        if image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_err() {
            continue;
        }
        textures.push(Texture {
            name: channel.name().into(),
            data: png,
            format: TextureFormat::Png,
            width: baked.width,
            height: baked.height,
        });
        let reference = Some(TextureRef { texture_index: textures.len() - 1, tex_coord_set: 0 });
        match channel {
            Channel::BaseColor => {
                out.base_color_texture = reference;
                out.base_color_factor = [1.0; 4];
            }
            Channel::MetallicRoughness => {
                out.metallic_roughness_texture = reference;
                out.metallic_factor = 1.0;
                out.roughness_factor = 1.0;
            }
            Channel::Occlusion => {
                out.occlusion_texture = reference;
                out.occlusion_strength = 1.0;
            }
            Channel::Emissive => {
                out.emissive_texture = reference;
                out.emissive_factor = [1.0; 3];
            }
            Channel::Normal => {
                out.normal_texture = reference;
                out.normal_scale = 1.0;
            }
        }
    }
    (out, textures)
}

/// Escena con la malla de caras de `N` vértices (triangulada en abanico, en
/// espacio mundo) y su piel.
///
/// Con piel, cada material es una primitiva y un vértice se duplica donde sus
/// caras tienen UV distintas (costuras). Devuelve también, por cada vértice
/// exportado en orden de primitivas, el vértice de la malla de origen.
pub fn skin_scene<const N: usize>(
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    skin: Option<&Skin<N>>,
    base: &Scene,
) -> (Scene, Vec<usize>) {
    use converter_scene::{IndexData, Mesh, Node, Primitive, Transform, VertexAttribute};

    let points: Vec<[f32; 3]> = positions.iter().map(|p| p.map(|c| c as f32)).collect();
    let all_triangles: Vec<[u32; 3]> = faces
        .iter()
        .flat_map(|&f| (1..N.saturating_sub(1)).map(move |k| [f[0], f[k], f[k + 1]].map(|i| i as u32)))
        .collect();
    let skin = skin.filter(|s| s.corners.len() == faces.len());
    let zero_uvs;
    let corners = match skin {
        Some(s) => &s.corners,
        None => {
            zero_uvs = vec![[[0.0f32; 2]; N]; faces.len()];
            &zero_uvs
        }
    };
    // Las mismas normales y tangentes con que se hornea el normal map
    let frames = corner_frames(positions, faces, corners);
    let mut normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    for (face, n) in faces.iter().zip(&frames.normals) {
        for k in 0..N {
            normals[face[k]] = n[k];
        }
    }

    let (primitives, source) = match skin {
        None => {
            let primitive = Primitive {
                attributes: vec![VertexAttribute::Positions(points), VertexAttribute::Normals(normals)],
                indices: Some(IndexData::U32(all_triangles.into_iter().flatten().collect())),
                material: None,
            };
            (vec![primitive], (0..positions.len()).collect())
        }
        Some(skin) => {
            let mut materials: Vec<Option<usize>> = skin.face_material.clone();
            materials.sort_unstable();
            materials.dedup();
            let mut primitives = Vec::new();
            let mut source = Vec::new();
            let with_tangents = skin.materials.iter().any(|m| m.normal_texture.is_some());
            for material in materials {
                // Vértice exportado por (vértice de origen, UV exacta)
                let mut ids: HashMap<(usize, [u32; 2]), u32> = HashMap::new();
                let (mut pos, mut nor, mut tan, mut tex, mut indices) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
                for (f, (face, uvs)) in faces
                    .iter()
                    .zip(&skin.corners)
                    .enumerate()
                    .filter(|(f, _)| skin.face_material[*f] == material)
                {
                    let corner: [u32; N] = std::array::from_fn(|k| {
                        let (v, uv) = (face[k], uvs[k]);
                        *ids.entry((v, uv.map(f32::to_bits))).or_insert_with(|| {
                            pos.push(points[v]);
                            nor.push(frames.normals[f][k]);
                            tan.push(frames.tangents[f][k]);
                            tex.push(uv);
                            source.push(v);
                            (pos.len() - 1) as u32
                        })
                    });
                    for k in 1..N.saturating_sub(1) {
                        indices.extend([corner[0], corner[k], corner[k + 1]]);
                    }
                }
                let mut attributes = vec![VertexAttribute::Positions(pos), VertexAttribute::Normals(nor)];
                if with_tangents {
                    attributes.push(VertexAttribute::Tangents(tan));
                }
                attributes.push(VertexAttribute::TexCoords(0, tex));
                primitives.push(Primitive {
                    attributes,
                    indices: Some(IndexData::U32(indices)),
                    material: material.filter(|&m| m < skin.materials.len()),
                });
            }
            (primitives, source)
        }
    };

    let scene = Scene {
        meshes: vec![Mesh { name: "retopology".into(), primitives }],
        nodes: vec![Node {
            name: "retopology".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: vec![],
        }],
        root_nodes: vec![0],
        materials: skin.map(|s| s.materials.clone()).unwrap_or_default(),
        textures: skin.map(|s| s.textures.clone()).unwrap_or_default(),
        meters_per_unit: base.meters_per_unit,
        y_up: base.y_up,
        ..Scene::default()
    };
    (scene, source)
}

/// Textura de tablero (`cells × cells` casillas) para ver la distorsión y las
/// costuras de un mapa UV.
pub fn checker_texture(size: u32, cells: u32) -> Texture {
    let cell = (size / cells.max(1)).max(1);
    let image = RgbaImage::from_fn(size, size, |x, y| {
        let (cx, cy) = (x / cell, y / cell);
        let dark = (cx + cy) % 2 == 0;
        // Tono por columna para distinguir orientación
        let hue = (cx as f32 / cells.max(1) as f32) * 0.6;
        let base = if dark { 0.25 } else { 0.9 };
        image::Rgba([
            ((base + hue * 0.3).min(1.0) * 255.0) as u8,
            (base * 255.0) as u8,
            ((base + (0.6 - hue) * 0.3).min(1.0) * 255.0) as u8,
            255,
        ])
    });
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("PNG en memoria");
    Texture { name: "checker".into(), data: png, format: TextureFormat::Png, width: size, height: size }
}

impl<const N: usize> Skin<N> {
    /// La misma piel con un tablero en vez de sus texturas.
    pub fn with_checker(&self, cells: u32) -> Self {
        Self {
            materials: vec![Material {
                name: "checker".into(),
                base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
                ..Material::default()
            }],
            textures: vec![checker_texture(1024, cells)],
            face_material: vec![Some(0); self.corners.len()],
            ..self.clone()
        }
    }
}
