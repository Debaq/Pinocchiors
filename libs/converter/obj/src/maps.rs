//! Mapas de textura en archivos sueltos (PNG, JPG, TGA…): reconocerlos por
//! el nombre del archivo o por la línea del MTL y armarlos como los pide
//! glTF (rugosidad en G y metal en B de una misma imagen, normales en
//! espacio tangente, opacidad en el alfa del color).

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use converter_scene::{AlphaMode, Material, Scene, Texture, TextureFormat, TextureRef, VertexAttribute};
use image::{DynamicImage, GrayImage, ImageFormat, RgbaImage};
use image::imageops::FilterType;

/// Qué guarda un archivo de textura
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapKind {
    BaseColor,
    /// Mapa de normales (espacio tangente)
    Normal,
    /// Relieve en escala de grises: se convierte a normales
    Height,
    /// `bump` del MTL: normales o relieve según la imagen
    Bump,
    Roughness,
    /// Brillo (lo inverso de la rugosidad)
    Glossiness,
    Metallic,
    /// Ya empaquetado como glTF: rugosidad en G, metal en B
    MetallicRoughness,
    /// Oclusión, rugosidad y metal en R, G y B
    Orm,
    Occlusion,
    Emissive,
    Opacity,
}

impl MapKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::BaseColor => "color",
            Self::Normal => "normales",
            Self::Height | Self::Bump => "relieve",
            Self::Roughness => "rugosidad",
            Self::Glossiness => "brillo",
            Self::Metallic => "metal",
            Self::MetallicRoughness => "metal y rugosidad",
            Self::Orm => "oclusión, rugosidad y metal",
            Self::Occlusion => "oclusión",
            Self::Emissive => "emisión",
            Self::Opacity => "opacidad",
        }
    }
}

/// Mapas de un material, cada uno con su archivo
pub type MapSet = BTreeMap<MapKind, PathBuf>;

/// Extensiones de imagen que se leen
pub const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "tga", "bmp"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

// ─── Reconocer el mapa por el nombre ────────────────────────────────────────

/// Resultado de mirar el nombre de un archivo
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detected {
    /// Un mapa que se usa, y lo que queda del nombre antes de él (el
    /// material o el modelo: `robot` en `robot_normal_4k.png`)
    Map(MapKind, String),
    /// Un mapa que glTF no usa (desplazamiento, especular, curvatura…)
    Unused,
    /// Sin ninguna palabra conocida
    Unknown,
}

/// Parte el nombre en palabras: separadores, camelCase y cambios de letra a número
fn tokens(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut prev: Option<char> = None;
    for c in stem.chars() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            prev = None;
            continue;
        }
        if let Some(p) = prev {
            let camel = p.is_lowercase() && c.is_uppercase();
            let digit = p.is_ascii_digit() != c.is_ascii_digit();
            if (camel || digit) && !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
        }
        word.extend(c.to_lowercase());
        prev = Some(c);
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Palabra de mapa (o `None` si la palabra no dice nada)
fn keyword(word: &str) -> Option<Detected> {
    use MapKind::*;
    let kind = match word {
        "basecolor" | "base" | "albedo" | "diffuse" | "diff" | "dif" | "color" | "colour" | "col" | "kd" => BaseColor,
        "normal" | "normals" | "normalmap" | "nrm" | "nor" | "norm" | "nml" => Normal,
        "height" | "bump" | "bumpmap" => Height,
        "roughness" | "rough" | "rgh" => Roughness,
        "gloss" | "glossiness" | "smoothness" => Glossiness,
        "metallic" | "metalness" | "metal" | "met" => Metallic,
        "metallicroughness" => MetallicRoughness,
        "orm" | "arm" | "occlusionroughnessmetallic" => Orm,
        "ao" | "occlusion" | "ambientocclusion" | "occ" => Occlusion,
        "emissive" | "emission" | "emit" | "glow" => Emissive,
        "opacity" | "alpha" | "transparency" => Opacity,
        "displacement" | "disp" | "specular" | "spec" | "cavity" | "curvature" | "thickness" | "sss"
        | "translucency" | "id" | "mask" => return Some(Detected::Unused),
        _ => return None,
    };
    Some(Detected::Map(kind, String::new()))
}

fn kind_of(word: &str) -> Option<MapKind> {
    match keyword(word) {
        Some(Detected::Map(kind, _)) => Some(kind),
        _ => None,
    }
}

/// Reconoce el mapa por el nombre. Manda la última palabra conocida (los
/// nombres terminan en el mapa: `MetalPlate_BaseColor` es color); los pares
/// `metallic_roughness` y `occlusion_roughness_metallic` van juntos.
pub fn classify(stem: &str) -> Detected {
    let words = tokens(stem);
    let Some(last) = words.iter().rposition(|w| keyword(w).is_some()) else {
        // `texture` solo, sin otra palabra: es el color (Meshy, Tripo…)
        return match words.iter().rposition(|w| matches!(w.as_str(), "texture" | "tex" | "diffuse")) {
            Some(i) => Detected::Map(MapKind::BaseColor, words[..i].join("_")),
            None => Detected::Unknown,
        };
    };
    let Some(kind) = kind_of(&words[last]) else {
        return Detected::Unused;
    };
    // Palabras de mapa seguidas que forman un solo mapa
    let mut start = last;
    while start > 0 && last - start < 2 && kind_of(&words[start - 1]).is_some() {
        start -= 1;
    }
    let run: Vec<MapKind> = words[start..=last].iter().filter_map(|w| kind_of(w)).collect();
    let has = |k: MapKind| run.contains(&k);
    let (kind, start) = if has(MapKind::Occlusion) && has(MapKind::Roughness) && has(MapKind::Metallic) {
        (MapKind::Orm, start)
    } else if has(MapKind::Roughness) && has(MapKind::Metallic) {
        let first = (start..=last).find(|&i| matches!(kind_of(&words[i]), Some(MapKind::Roughness | MapKind::Metallic))).unwrap_or(last);
        (MapKind::MetallicRoughness, first)
    } else if kind == MapKind::BaseColor && words[last] == "color" && last > 0 && words[last - 1] == "base" {
        (MapKind::BaseColor, last - 1)
    } else if kind == MapKind::Occlusion && last > 0 && words[last - 1] == "ambient" {
        (MapKind::Occlusion, last - 1)
    } else {
        (kind, last)
    };
    Detected::Map(kind, words[..start].join("_"))
}

/// Letras y números en minúscula, para comparar nombres
fn normalized(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// ¿El prefijo del archivo nombra a este material (o al modelo)?
fn names_match(prefix: &str, name: &str) -> bool {
    let (a, b) = (normalized(prefix), normalized(name));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len() >= 3 && b.len() >= 3 && (a.ends_with(&b) || b.ends_with(&a) || a.starts_with(&b) || b.starts_with(&a)))
}

// ─── Asignar archivos sueltos a los materiales ──────────────────────────────

/// Qué se hizo con los archivos
#[derive(Debug, Default, Clone)]
pub struct AttachReport {
    /// `normales: robot_normal.png → Robot`
    pub attached: Vec<String>,
    /// Archivos que no se usaron y por qué
    pub skipped: Vec<String>,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn has_uvs(scene: &Scene) -> bool {
    scene.meshes.iter().flat_map(|m| &m.primitives).any(|p| p.attributes.iter().any(|a| matches!(a, VertexAttribute::TexCoords(..))))
}

/// Un material para las primitivas que no tienen: los modelos sin material
/// (un GLB exportado solo con la malla) igual reciben sus texturas
fn ensure_material(scene: &mut Scene, name: &str) {
    let orphans = scene.meshes.iter().flat_map(|m| &m.primitives).any(|p| p.material.is_none());
    if !orphans {
        return;
    }
    let index = scene.materials.len();
    scene.materials.push(Material {
        name: name.to_string(),
        metallic_factor: 0.0,
        roughness_factor: 1.0,
        ..Material::default()
    });
    for p in scene.meshes.iter_mut().flat_map(|m| &mut m.primitives) {
        p.material.get_or_insert(index);
    }
}

/// Pone las texturas de `files` en los materiales de la escena. Cada
/// archivo va al material que nombra (`Casco_normal.png` → `Casco`); los que
/// no nombran a ninguno van a los materiales que no recibieron nada.
/// `overwrite`: si el material ya tiene ese mapa, se reemplaza (archivos
/// elegidos a mano) o se respeta (archivos encontrados en la carpeta).
pub fn attach_textures(scene: &mut Scene, files: &[PathBuf], model_name: &str, overwrite: bool) -> AttachReport {
    let mut report = AttachReport::default();
    if files.is_empty() {
        return report;
    }
    if !has_uvs(scene) {
        report.skipped.extend(files.iter().map(|f| format!("{}: el modelo no tiene coordenadas UV", file_name(f))));
        return report;
    }

    // Clasificar; un único archivo sin palabra conocida es el color
    let mut sets: BTreeMap<String, MapSet> = BTreeMap::new();
    let mut unknown = Vec::new();
    for file in files {
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        match classify(stem) {
            Detected::Map(kind, prefix) => {
                let set = sets.entry(prefix).or_default();
                if set.contains_key(&kind) {
                    report.skipped.push(format!("{}: ya hay otro archivo de {}", file_name(file), kind.label()));
                } else {
                    set.insert(kind, file.clone());
                }
            }
            Detected::Unused => report.skipped.push(format!("{}: glTF no usa ese mapa", file_name(file))),
            Detected::Unknown => unknown.push(file.clone()),
        }
    }
    let no_color = !sets.values().any(|s| s.contains_key(&MapKind::BaseColor));
    if unknown.len() == 1 && no_color {
        let file = unknown.remove(0);
        let prefix = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        sets.entry(prefix).or_default().insert(MapKind::BaseColor, file);
    }
    report.skipped.extend(unknown.iter().map(|f| format!("{}: no se reconoce qué mapa es", file_name(f))));
    if sets.is_empty() {
        return report;
    }

    ensure_material(scene, model_name);
    let count = scene.materials.len();

    // Juego de mapas por material: el que lo nombra; si no, los juegos que
    // no nombran a ningún material (con uno solo de esos, va a todos)
    let mut targets: Vec<Option<String>> = vec![None; count];
    for (i, material) in scene.materials.iter().enumerate() {
        targets[i] = sets.keys().find(|p| names_match(p, &material.name)).cloned();
    }
    let loose: Vec<&String> = sets.keys().filter(|p| !targets.iter().any(|t| t.as_ref() == Some(*p))).collect();
    let mut loose_iter = loose.iter();
    let single_loose = loose.len() == 1;
    for i in 0..count {
        if targets[i].is_some() {
            continue;
        }
        targets[i] = if single_loose { Some(loose[0].clone()) } else { loose_iter.next().map(|p| (*p).clone()) };
    }
    if !single_loose {
        for prefix in loose_iter {
            report.skipped.push(format!("texturas \"{prefix}\": no hay material para ellas"));
        }
    }

    for (material, prefix) in targets.into_iter().enumerate() {
        if let Some(set) = prefix.and_then(|p| sets.get(&p)) {
            apply_maps(scene, material, set, overwrite, &mut report);
        }
    }
    report
}

/// Busca texturas sueltas junto al modelo y las pone en los mapas que el
/// modelo no trae. Solo toma archivos que nombran a un mapa y, si hay
/// varios juegos en la carpeta, los que nombran al modelo o a un material.
pub fn attach_textures_from_folder(scene: &mut Scene, model: &Path) -> AttachReport {
    let dir = model.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let model_stem = model.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    for folder in texture_folders(dir) {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_image(&path) {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if let Detected::Map(_, prefix) = classify(stem) {
                candidates.push((path, prefix));
            }
        }
    }
    candidates.sort();
    let prefixes: std::collections::BTreeSet<&String> = candidates.iter().map(|(_, p)| p).collect();
    let single_set = prefixes.len() == 1;
    let related = |prefix: &str| {
        single_set
            || prefix.is_empty()
            || names_match(prefix, &model_stem)
            || scene.materials.iter().any(|m| names_match(prefix, &m.name))
    };
    let files: Vec<PathBuf> = candidates.iter().filter(|(_, p)| related(p)).map(|(f, _)| f.clone()).collect();
    let mut report = attach_textures(scene, &files, &model_stem, false);
    // Lo que se dejó en la carpeta no es noticia
    report.skipped.clear();
    report
}

/// La carpeta del modelo y sus subcarpetas típicas de texturas
fn texture_folders(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = file_name(&path).to_lowercase();
            if path.is_dir() && matches!(name.as_str(), "textures" | "texture" | "tex" | "maps" | "images" | "materials") {
                out.push(path);
            }
        }
    }
    out
}

// ─── Armar los mapas de un material ─────────────────────────────────────────

fn read_image(path: &Path) -> Result<(Vec<u8>, DynamicImage), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", file_name(path)))?;
    // TGA no tiene firma: el formato sale de la extensión
    let format = image::guess_format(&bytes).or_else(|_| ImageFormat::from_path(path)).map_err(|e| format!("{}: {e}", file_name(path)))?;
    let image = image::load_from_memory_with_format(&bytes, format).map_err(|e| format!("{}: {e}", file_name(path)))?;
    Ok((bytes, image))
}

fn png_texture(name: &str, image: &DynamicImage) -> Result<Texture, String> {
    let mut data = Vec::new();
    image.write_to(&mut Cursor::new(&mut data), ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(Texture { name: name.to_string(), data, format: TextureFormat::Png, width: image.width(), height: image.height() })
}

/// La imagen tal cual si glTF la acepta (PNG, JPG, WebP); si no, en PNG
fn texture_from(path: &Path, bytes: Vec<u8>, image: &DynamicImage) -> Result<Texture, String> {
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("textura").to_string();
    let format = match image::guess_format(&bytes) {
        Ok(ImageFormat::Png) => TextureFormat::Png,
        Ok(ImageFormat::Jpeg) => TextureFormat::Jpeg,
        Ok(ImageFormat::WebP) => TextureFormat::WebP,
        _ => return png_texture(&name, image),
    };
    Ok(Texture { name, data: bytes, format, width: image.width(), height: image.height() })
}

/// ¿Escala de grises? (relieve, no normales)
fn is_grayscale(image: &DynamicImage) -> bool {
    let rgb = image.to_rgb8();
    let step = ((rgb.width() as usize * rgb.height() as usize) / 4096).max(1);
    rgb.pixels().step_by(step).all(|p| {
        let [r, g, b] = p.0;
        r.abs_diff(g) <= 6 && g.abs_diff(b) <= 6
    })
}

/// Relieve → normales en espacio tangente (diferencias centrales).
/// `strength`: cuánto relieve da el blanco frente al negro, en píxeles.
fn height_to_normal(height: &GrayImage, strength: f32) -> RgbaImage {
    let (w, h) = height.dimensions();
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, w as i64 - 1) as u32;
        let y = y.clamp(0, h as i64 - 1) as u32;
        height.get_pixel(x, y).0[0] as f32 / 255.0
    };
    RgbaImage::from_fn(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let dx = (at(x + 1, y) - at(x - 1, y)) * 0.5 * strength;
        // V de glTF crece hacia abajo en la imagen: el eje Y de la normal sube
        let dy = (at(x, y - 1) - at(x, y + 1)) * 0.5 * strength;
        let n = [-dx, -dy, 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let byte = |v: f32| ((v / len * 0.5 + 0.5) * 255.0).round() as u8;
        image::Rgba([byte(n[0]), byte(n[1]), byte(n[2]), 255])
    })
}

/// Un canal en escala de grises del tamaño pedido
fn channel(image: &DynamicImage, w: u32, h: u32) -> GrayImage {
    let gray = image.to_luma8();
    if gray.dimensions() == (w, h) { gray } else { image::imageops::resize(&gray, w, h, FilterType::Triangle) }
}

fn push(scene: &mut Scene, texture: Texture) -> TextureRef {
    scene.textures.push(texture);
    TextureRef { texture_index: scene.textures.len() - 1, tex_coord_set: 0 }
}

/// Pone los mapas de `maps` en el material. Sin `overwrite`, los mapas que
/// el material ya tiene se respetan.
pub fn apply_maps(scene: &mut Scene, material: usize, maps: &MapSet, overwrite: bool, report: &mut AttachReport) {
    let Some(current) = scene.materials.get(material).cloned() else { return };
    let mut m = current;
    let free = |r: &Option<TextureRef>| overwrite || r.is_none();
    let mut note = |kind: MapKind, path: &Path, name: &str| {
        report.attached.push(format!("{}: {} → {}", kind.label(), file_name(path), if name.is_empty() { "material" } else { name }));
    };
    let mut errors = Vec::new();
    let mut load = |path: &Path| read_image(path).map_err(|e| errors.push(e)).ok();

    // Color, con la opacidad en el alfa si viene aparte
    let color = maps.get(&MapKind::BaseColor);
    let opacity = maps.get(&MapKind::Opacity);
    if free(&m.base_color_texture) && (color.is_some() || opacity.is_some()) {
        let base = color.and_then(|p| load(p).map(|(bytes, img)| (p, bytes, img)));
        let alpha = opacity.and_then(|p| load(p).map(|(_, img)| (p, img)));
        let texture = match (base, alpha) {
            (Some((path, bytes, img)), None) => texture_from(path, bytes, &img).ok().map(|t| (t, false)),
            (base, Some((path, alpha))) => {
                let (w, h) = base.as_ref().map_or((alpha.width(), alpha.height()), |(_, _, b)| (b.width(), b.height()));
                let mut rgba = base.as_ref().map_or_else(|| RgbaImage::from_pixel(w, h, image::Rgba([255; 4])), |(_, _, b)| b.to_rgba8());
                let a = channel(&alpha, w, h);
                for (p, v) in rgba.pixels_mut().zip(a.pixels()) {
                    p.0[3] = v.0[0];
                }
                let name = base.as_ref().map_or(path, |(p, _, _)| *p).file_stem().and_then(|s| s.to_str()).unwrap_or("color");
                png_texture(name, &DynamicImage::ImageRgba8(rgba)).ok().map(|t| (t, true))
            }
            (None, None) => None,
        };
        if let Some((texture, blended)) = texture {
            m.base_color_texture = Some(push(scene, texture));
            // El factor multiplica a la textura: el color ya viene en ella
            m.base_color_factor = [1.0, 1.0, 1.0, if blended { 1.0 } else { m.base_color_factor[3] }];
            if blended {
                m.alpha_mode = AlphaMode::Blend;
            }
            if let Some(p) = color {
                note(MapKind::BaseColor, p, &m.name);
            }
            if let Some(p) = opacity {
                note(MapKind::Opacity, p, &m.name);
            }
        }
    }

    // Normales: el mapa de normales; si no, el relieve convertido
    let normal = [MapKind::Normal, MapKind::Bump, MapKind::Height].into_iter().find_map(|k| maps.get(&k).map(|p| (k, p)));
    if let Some((kind, path)) = normal.filter(|_| free(&m.normal_texture)) {
        if let Some((bytes, img)) = load(path) {
            let as_height = kind == MapKind::Height || (kind == MapKind::Bump && is_grayscale(&img));
            let texture = if as_height {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("normal");
                png_texture(&format!("{name}_normal"), &DynamicImage::ImageRgba8(height_to_normal(&img.to_luma8(), 4.0 * m.normal_scale)))
            } else {
                texture_from(path, bytes, &img)
            };
            if let Ok(texture) = texture {
                m.normal_texture = Some(push(scene, texture));
                if as_height {
                    m.normal_scale = 1.0;
                }
                note(kind, path, &m.name);
            }
        }
    }

    // Metal y rugosidad: ORM o ya empaquetado; si no, se empaqueta
    let packed = maps.get(&MapKind::Orm).map(|p| (MapKind::Orm, p))
        .or_else(|| maps.get(&MapKind::MetallicRoughness).map(|p| (MapKind::MetallicRoughness, p)));
    let mut orm_ref = None;
    if let Some((kind, path)) = packed {
        if free(&m.metallic_roughness_texture) {
            if let Some((bytes, img)) = load(path) {
                if let Ok(texture) = texture_from(path, bytes, &img) {
                    let reference = push(scene, texture);
                    if kind == MapKind::Orm {
                        orm_ref = Some(reference.clone());
                    }
                    m.metallic_roughness_texture = Some(reference);
                    m.metallic_factor = 1.0;
                    m.roughness_factor = 1.0;
                    note(kind, path, &m.name);
                }
            }
        }
    } else if free(&m.metallic_roughness_texture) {
        let rough = maps.get(&MapKind::Roughness).map(|p| (p, false))
            .or_else(|| maps.get(&MapKind::Glossiness).map(|p| (p, true)));
        let metal = maps.get(&MapKind::Metallic);
        let rough_img = rough.and_then(|(p, inverted)| load(p).map(|(_, img)| (p, img, inverted)));
        let metal_img = metal.and_then(|p| load(p).map(|(_, img)| (p, img)));
        if rough_img.is_some() || metal_img.is_some() {
            let (w, h) = [rough_img.as_ref().map(|r| (r.1.width(), r.1.height())), metal_img.as_ref().map(|r| (r.1.width(), r.1.height()))]
                .into_iter().flatten().max_by_key(|&(w, h)| w as u64 * h as u64).unwrap_or((1, 1));
            let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            // El canal que falta lleva el factor del material
            let g = rough_img.as_ref().map_or_else(
                || GrayImage::from_pixel(w, h, image::Luma([unit(m.roughness_factor)])),
                |(_, img, inverted)| {
                    let mut c = channel(img, w, h);
                    if *inverted {
                        image::imageops::invert(&mut c);
                    }
                    c
                },
            );
            let b = metal_img.as_ref().map_or_else(
                || GrayImage::from_pixel(w, h, image::Luma([unit(m.metallic_factor)])),
                |(_, img)| channel(img, w, h),
            );
            let mr = RgbaImage::from_fn(w, h, |x, y| image::Rgba([255, g.get_pixel(x, y).0[0], b.get_pixel(x, y).0[0], 255]));
            let name = if m.name.is_empty() { "metal_rugosidad".to_string() } else { format!("{}_metal_rugosidad", m.name) };
            if let Ok(texture) = png_texture(&name, &DynamicImage::ImageRgba8(mr)) {
                m.metallic_roughness_texture = Some(push(scene, texture));
                m.metallic_factor = 1.0;
                m.roughness_factor = 1.0;
                if let Some((p, _, inverted)) = &rough_img {
                    note(if *inverted { MapKind::Glossiness } else { MapKind::Roughness }, p, &m.name);
                }
                if let Some((p, _)) = &metal_img {
                    note(MapKind::Metallic, p, &m.name);
                }
            }
        }
    }

    // Oclusión: la propia o la R del ORM
    if free(&m.occlusion_texture) {
        if let Some(path) = maps.get(&MapKind::Occlusion) {
            if let Some((bytes, img)) = load(path) {
                if let Ok(texture) = texture_from(path, bytes, &img) {
                    m.occlusion_texture = Some(push(scene, texture));
                    note(MapKind::Occlusion, path, &m.name);
                }
            }
        } else if let Some(reference) = orm_ref {
            m.occlusion_texture = Some(reference);
        }
    }

    if let Some(path) = maps.get(&MapKind::Emissive).filter(|_| free(&m.emissive_texture)) {
        if let Some((bytes, img)) = load(path) {
            if let Ok(texture) = texture_from(path, bytes, &img) {
                m.emissive_texture = Some(push(scene, texture));
                // Sin factor la emisión no se vería
                if m.emissive_factor.iter().all(|&v| v <= 0.0) {
                    m.emissive_factor = [1.0, 1.0, 1.0];
                }
                note(MapKind::Emissive, path, &m.name);
            }
        }
    }

    report.skipped.extend(errors);
    scene.materials[material] = m;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(stem: &str) -> (MapKind, String) {
        match classify(stem) {
            Detected::Map(kind, prefix) => (kind, prefix),
            other => panic!("{stem}: {other:?}"),
        }
    }

    #[test]
    fn recognizes_common_names() {
        assert_eq!(map("Robot_BaseColor"), (MapKind::BaseColor, "robot".into()));
        assert_eq!(map("robot_normal_4k"), (MapKind::Normal, "robot".into()));
        assert_eq!(map("MetalPlate_Roughness"), (MapKind::Roughness, "metal_plate".into()));
        assert_eq!(map("MetalPlate_BaseColor"), (MapKind::BaseColor, "metal_plate".into()));
        assert_eq!(map("Casco_metallicRoughness"), (MapKind::MetallicRoughness, "casco".into()));
        assert_eq!(map("Casco_OcclusionRoughnessMetallic"), (MapKind::Orm, "casco".into()));
        assert_eq!(map("wood_ambient_occlusion"), (MapKind::Occlusion, "wood".into()));
        assert_eq!(map("Meshy_texture"), (MapKind::BaseColor, "meshy".into()));
        assert_eq!(map("Meshy_texture_normal"), (MapKind::Normal, "meshy_texture".into()));
        assert_eq!(map("albedo"), (MapKind::BaseColor, String::new()));
        assert_eq!(classify("rock_displacement"), Detected::Unused);
        assert_eq!(classify("foto_vacaciones"), Detected::Unknown);
    }

    fn write_png(path: &Path, image: DynamicImage) {
        image.save_with_format(path, ImageFormat::Png).unwrap();
    }

    fn quad_scene(material: Option<usize>) -> Scene {
        let mut scene = Scene::new();
        scene.meshes.push(converter_scene::Mesh {
            name: "quad".into(),
            primitives: vec![converter_scene::Primitive {
                attributes: vec![
                    VertexAttribute::Positions(vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
                    VertexAttribute::TexCoords(0, vec![[0.0; 2], [1.0, 0.0], [0.0, 1.0]]),
                ],
                indices: None,
                material,
            }],
        });
        scene
    }

    #[test]
    fn loose_maps_fill_a_model_without_material() {
        let dir = tempfile::tempdir().unwrap();
        let gray = |v: u8| DynamicImage::ImageLuma8(GrayImage::from_pixel(4, 4, image::Luma([v])));
        write_png(&dir.path().join("robot_basecolor.png"), DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, image::Rgba([200, 10, 10, 255]))));
        write_png(&dir.path().join("robot_normal.png"), DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, image::Rgba([128, 128, 255, 255]))));
        write_png(&dir.path().join("robot_roughness.png"), gray(40));
        write_png(&dir.path().join("robot_metallic.png"), gray(220));
        write_png(&dir.path().join("vacaciones.png"), gray(0));

        let mut scene = quad_scene(None);
        let report = attach_textures_from_folder(&mut scene, &dir.path().join("robot.glb"));
        assert_eq!(report.attached.len(), 4, "{report:?}");

        let m = &scene.materials[0];
        assert_eq!(scene.meshes[0].primitives[0].material, Some(0));
        assert!(m.base_color_texture.is_some() && m.normal_texture.is_some());
        let mr = &scene.textures[m.metallic_roughness_texture.as_ref().unwrap().texture_index];
        let mr = image::load_from_memory(&mr.data).unwrap().to_rgba8();
        assert_eq!(mr.get_pixel(0, 0).0[1..3], [40, 220]);
        assert_eq!((m.metallic_factor, m.roughness_factor), (1.0, 1.0));
    }

    #[test]
    fn folder_textures_keep_the_maps_the_model_has() {
        let dir = tempfile::tempdir().unwrap();
        write_png(&dir.path().join("normal.png"), DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, image::Rgba([128, 128, 255, 255]))));
        let mut scene = quad_scene(Some(0));
        scene.textures.push(Texture { name: "propia".into(), data: Vec::new(), format: TextureFormat::Png, width: 0, height: 0 });
        scene.materials.push(Material { normal_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }), ..Material::default() });
        let report = attach_textures_from_folder(&mut scene, &dir.path().join("modelo.glb"));
        assert!(report.attached.is_empty());
        assert_eq!(scene.materials[0].normal_texture.as_ref().unwrap().texture_index, 0);

        // Elegida a mano, reemplaza
        let report = attach_textures(&mut scene, &[dir.path().join("normal.png")], "modelo", true);
        assert_eq!(report.attached.len(), 1);
        assert_eq!(scene.materials[0].normal_texture.as_ref().unwrap().texture_index, 1);
    }

    #[test]
    fn grayscale_bump_becomes_normals() {
        let height = GrayImage::from_fn(8, 8, |x, _| image::Luma([(x * 30) as u8]));
        let normal = height_to_normal(&height, 4.0);
        let p = normal.get_pixel(4, 4).0;
        // Sube hacia +X: la normal se inclina hacia -X
        assert!(p[0] < 128 && p[2] > 128, "{p:?}");
        assert!(p[1].abs_diff(128) <= 1);
    }
}
