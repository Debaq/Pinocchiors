//! Editar texturas en otras aplicaciones: exportar la textura con la malla UV
//! en capas (XCF para GIMP, PSD para Krita o Photoshop), volver a importarla
//! y "Editar en GIMP", que abre el archivo y lo recarga cada vez que se guarda.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use converter_layers::{LayeredFormat, LayeredImage, Layer, draw_lines, fill_triangles};
use converter_scene::{Material, Scene, Texture, TextureFormat, TextureRef};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::in_background;
use crate::state::AppState;

/// Capas de guía: se ignoran al importar
const WIRE_LAYER: &str = "Malla UV";
const BORDER_LAYER: &str = "Bordes de islas";
const ISLAND_LAYER: &str = "Islas";
const GUIDE_LAYERS: [&str; 3] = [WIRE_LAYER, BORDER_LAYER, ISLAND_LAYER];

/// Lado de una textura nueva cuando el material no tiene ninguna
const NEW_TEXTURE_SIZE: u32 = 2048;

/// Evento al recargar una textura editada afuera
pub const TEXTURE_UPDATED: &str = "texture-updated";
/// Evento si la recarga falló (el archivo quedó a medio guardar, formato raro…)
pub const TEXTURE_UPDATE_FAILED: &str = "texture-update-failed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextureTarget {
    /// Modelo importado
    Original,
    /// Piel de la retopología
    Quad,
}

/// Mapa de un material (mismos nombres que `SceneMaterial.maps` del frontend)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureSlot {
    Base,
    MetallicRoughness,
    Normal,
    Occlusion,
    Emissive,
}

impl TextureSlot {
    fn slug(self) -> &'static str {
        match self {
            Self::Base => "color",
            Self::MetallicRoughness => "metal-rugosidad",
            Self::Normal => "normal",
            Self::Occlusion => "oclusion",
            Self::Emissive => "emision",
        }
    }

    fn reference(self, m: &mut Material) -> &mut Option<TextureRef> {
        match self {
            Self::Base => &mut m.base_color_texture,
            Self::MetallicRoughness => &mut m.metallic_roughness_texture,
            Self::Normal => &mut m.normal_texture,
            Self::Occlusion => &mut m.occlusion_texture,
            Self::Emissive => &mut m.emissive_texture,
        }
    }

    /// Color de una textura nueva: el que el material ya muestra sin ella
    /// (después se neutraliza el factor, que multiplica a la textura)
    fn fill(self, m: &Material) -> [u8; 4] {
        let srgb = linear_to_srgb8;
        match self {
            Self::Base => {
                let c = m.base_color_factor;
                [srgb(c[0]), srgb(c[1]), srgb(c[2]), (c[3].clamp(0.0, 1.0) * 255.0).round() as u8]
            }
            // glTF: rugosidad en G, metal en B
            Self::MetallicRoughness => {
                let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                [0, unit(m.roughness_factor), unit(m.metallic_factor), 255]
            }
            Self::Normal => [128, 128, 255, 255],
            Self::Occlusion => [255, 255, 255, 255],
            Self::Emissive => {
                let e = m.emissive_factor;
                [srgb(e[0]), srgb(e[1]), srgb(e[2]), 255]
            }
        }
    }

    /// Con textura nueva, el factor pasa a neutro (la textura ya lo incluye)
    fn neutralize(self, m: &mut Material) {
        match self {
            Self::Base => m.base_color_factor = [1.0, 1.0, 1.0, 1.0],
            Self::MetallicRoughness => {
                m.metallic_factor = 1.0;
                m.roughness_factor = 1.0;
            }
            Self::Emissive => m.emissive_factor = [1.0, 1.0, 1.0],
            Self::Normal | Self::Occlusion => {}
        }
    }
}

fn linear_to_srgb8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// Qué textura: un mapa de un material del original o de la retopología
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureKey {
    pub target: TextureTarget,
    pub material: usize,
    pub slot: TextureSlot,
}

/// Materiales y texturas donde vive una textura
struct Store<'a> {
    materials: &'a mut Vec<Material>,
    textures: &'a mut Vec<Texture>,
}

fn with_store<T>(state: &AppState, target: TextureTarget, f: impl FnOnce(Store) -> Result<T, String>) -> Result<T, String> {
    match target {
        TextureTarget::Original => {
            let mut scene = state.scene.lock().unwrap();
            let scene = scene.as_mut().ok_or("No hay escena cargada")?;
            f(Store { materials: &mut scene.materials, textures: &mut scene.textures })
        }
        TextureTarget::Quad => {
            let mut skin = state.quad_skin.lock().unwrap();
            let skin = skin.as_mut().ok_or("La retopología no tiene UV")?;
            f(Store { materials: &mut skin.materials, textures: &mut skin.textures })
        }
    }
}

/// Imagen actual de la textura (RGBA) o, si el mapa no tiene, una nueva del
/// color que muestra el material, del tamaño de sus otras texturas
fn current_image(state: &AppState, key: TextureKey) -> Result<(u32, u32, Vec<u8>, bool), String> {
    with_store(state, key.target, |store| {
        let material = store.materials.get_mut(key.material).ok_or("No existe ese material")?;
        if let Some(reference) = key.slot.reference(material).clone() {
            let texture = store.textures.get(reference.texture_index).ok_or("Textura inexistente")?;
            let image = image::load_from_memory(&texture.data).map_err(|e| format!("No se pudo leer la textura: {e}"))?;
            let rgba = image.to_rgba8();
            return Ok((rgba.width(), rgba.height(), rgba.into_raw(), true));
        }
        let size = [&material.base_color_texture, &material.metallic_roughness_texture, &material.normal_texture]
            .into_iter()
            .flatten()
            .filter_map(|r| store.textures.get(r.texture_index))
            .map(|t| t.width.max(t.height))
            .max()
            .unwrap_or(NEW_TEXTURE_SIZE);
        let fill = key.slot.fill(material);
        Ok((size, size, fill.repeat((size * size) as usize), false))
    })
}

/// Polígonos UV (triángulos o quads) de las caras del material
fn material_polygons(state: &AppState, key: TextureKey) -> Result<Vec<Vec<[f32; 2]>>, String> {
    match key.target {
        TextureTarget::Original => {
            let scene = state.scene.lock().unwrap();
            let scene = scene.as_ref().ok_or("No hay escena cargada")?;
            Ok(scene_polygons(scene, key.material))
        }
        TextureTarget::Quad => {
            let skin = state.quad_skin.lock().unwrap();
            let skin = skin.as_ref().ok_or("La retopología no tiene UV")?;
            Ok(skin
                .corners
                .iter()
                .zip(&skin.face_material)
                .filter(|(_, m)| **m == Some(key.material))
                .map(|(c, _)| c.to_vec())
                .collect())
        }
    }
}

fn scene_polygons(scene: &Scene, material: usize) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    for prim in scene.world_primitives() {
        if prim.material != Some(material) {
            continue;
        }
        let Some(uvs) = &prim.uvs else { continue };
        out.extend(prim.triangles.iter().map(|t| t.iter().map(|&i| uvs[i as usize]).collect()));
    }
    out
}

/// Capas de guía sobre un lienzo de `w × h`: malla, bordes de islas e islas
fn guide_layers(polygons: &[Vec<[f32; 2]>], w: u32, h: u32) -> [Layer; 3] {
    let px = |uv: [f32; 2]| [uv[0] * w as f32, uv[1] * h as f32];
    let width = (w.max(h) as f32 / 2048.0).max(1.0);

    // Aristas por UV cuantizada: las de una sola cara son el borde de la isla
    let key = |uv: [f32; 2]| ((uv[0] * 1048576.0).round() as i64, (uv[1] * 1048576.0).round() as i64);
    let mut edges: HashMap<((i64, i64), (i64, i64)), ([[f32; 2]; 2], u32)> = HashMap::new();
    for poly in polygons {
        for k in 0..poly.len() {
            let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            let id = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges.entry(id).or_insert(([px(a), px(b)], 0)).1 += 1;
        }
    }

    let mut wire = Layer::new(WIRE_LAYER, w, h);
    draw_lines(&mut wire, edges.values().map(|(s, _)| *s), [80, 250, 123], width);
    wire.opacity = 0.7;
    let mut borders = Layer::new(BORDER_LAYER, w, h);
    draw_lines(&mut borders, edges.values().filter(|(_, n)| *n == 1).map(|(s, _)| *s), [255, 184, 108], 2.0 * width);
    let mut islands = Layer::new(ISLAND_LAYER, w, h);
    let triangles = polygons.iter().flat_map(|p| (1..p.len().saturating_sub(1)).map(move |k| [px(p[0]), px(p[k]), px(p[k + 1])]));
    fill_triangles(&mut islands, triangles, [189, 147, 249, 255]);
    // Oculta: sirve para seleccionar islas con la varita mágica
    islands.visible = false;
    islands.opacity = 0.4;
    [wire, borders, islands]
}

/// Textura abajo y guías encima, listas para escribir
fn editing_image(state: &AppState, key: TextureKey) -> Result<LayeredImage, String> {
    let (w, h, pixels, _) = current_image(state, key)?;
    let polygons = material_polygons(state, key)?;
    let [wire, borders, islands] = guide_layers(&polygons, w, h);
    Ok(LayeredImage { width: w, height: h, layers: vec![Layer::from_rgba("Textura", w, h, pixels), islands, wire, borders] })
}

fn encode_png(w: u32, h: u32, pixels: Vec<u8>) -> Result<Vec<u8>, String> {
    let image = image::RgbaImage::from_raw(w, h, pixels).ok_or("Imagen de tamaño inválido")?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("No se pudo codificar el PNG: {e}"))?;
    Ok(png)
}

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Escribe la textura para editarla afuera: `.xcf`/`.psd` con capas, `.png`
/// solo la imagen o, con `layout_only`, un PNG transparente con la malla
fn export_to(state: &AppState, key: TextureKey, path: &Path, layout_only: bool) -> Result<(), String> {
    let bytes = if layout_only {
        let (w, h, _, _) = current_image(state, key)?;
        let polygons = material_polygons(state, key)?;
        let [wire, borders, _] = guide_layers(&polygons, w, h);
        let image = LayeredImage { width: w, height: h, layers: vec![wire, borders] };
        encode_png(w, h, image.composite(|_| true))?
    } else if let Some(format) = LayeredFormat::from_extension(&extension(path)) {
        editing_image(state, key)?.write(format)
    } else {
        let (w, h, pixels, exists) = current_image(state, key)?;
        if !exists {
            return Err("Este mapa no tiene imagen: exporta en XCF o PSD para pintarlo desde cero".into());
        }
        encode_png(w, h, pixels)?
    };
    std::fs::write(path, bytes).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))
}

/// Lee una imagen (con capas o plana), junta las capas visibles que no son
/// guías y la pone como textura del mapa
fn import_from(state: &AppState, key: TextureKey, path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let image = LayeredImage::read(&bytes).map_err(|e| e.to_string())?;
    let pixels = image.composite(|l| !GUIDE_LAYERS.contains(&l.name.as_str()));
    let png = encode_png(image.width, image.height, pixels)?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("textura").to_string();
    let texture = Texture { name, data: png, format: TextureFormat::Png, width: image.width, height: image.height };
    set_texture(state, key, texture)
}

/// Reemplaza (o crea) la textura del mapa. Con UV trasladadas la piel
/// comparte las imágenes del original: el cambio va a ambos lados.
fn set_texture(state: &AppState, key: TextureKey, texture: Texture) -> Result<(), String> {
    let shared = {
        let skin = state.quad_skin.lock().unwrap();
        let scene = state.scene.lock().unwrap();
        matches!((skin.as_ref(), scene.as_ref()), (Some(s), Some(sc))
            if matches!(s.info, uv_core::SkinInfo::Transferred { .. })
                && s.textures.len() == sc.textures.len()
                && s.materials.len() == sc.materials.len())
    };
    let targets: &[TextureTarget] = if shared { &[TextureTarget::Original, TextureTarget::Quad] } else { &[key.target] };
    for &target in targets {
        with_store(state, target, |store| {
            let material = store.materials.get_mut(key.material).ok_or("No existe ese material")?;
            match key.slot.reference(material).clone() {
                Some(reference) => {
                    let slot = store.textures.get_mut(reference.texture_index).ok_or("Textura inexistente")?;
                    *slot = texture.clone();
                }
                None => {
                    store.textures.push(texture.clone());
                    *key.slot.reference(material) = Some(TextureRef { texture_index: store.textures.len() - 1, tex_coord_set: 0 });
                    key.slot.neutralize(material);
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tauri::command]
pub async fn export_texture(app: AppHandle, key: TextureKey, path: String, layout_only: bool) -> Result<(), String> {
    in_background(app, move |state| export_to(state, key, Path::new(&path), layout_only)).await
}

#[tauri::command]
pub async fn import_texture(app: AppHandle, key: TextureKey, path: String) -> Result<(), String> {
    in_background(app, move |state| import_from(state, key, Path::new(&path))).await
}

// ─── Editar afuera ──────────────────────────────────────────────────────────

/// Generación del seguimiento en curso: al abrir otro (o cerrar) el anterior termina
#[derive(Default)]
pub struct TextureWatch(AtomicU64);

#[derive(Clone, Serialize)]
struct UpdateFailed {
    key: TextureKey,
    message: String,
}

/// Abre un archivo con GIMP si está instalado (en Linux), si no con la
/// aplicación asociada del sistema
fn open_externally(path: &Path) -> Result<(), String> {
    use std::process::Command;
    let is_xcf = extension(path) == "xcf";
    let spawned = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        if is_xcf { Command::new("open").args(["-a", "GIMP"]).arg(path).spawn() } else { Command::new("open").arg(path).spawn() }
    } else {
        match is_xcf.then(|| Command::new("gimp").arg(path).spawn()) {
            Some(Ok(child)) => Ok(child),
            _ => Command::new("xdg-open").arg(path).spawn(),
        }
    };
    spawned.map(|_| ()).map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))
}

/// Escribe la textura con capas en la carpeta de trabajo, la abre en GIMP
/// (o la aplicación de ese formato) y la vuelve a importar cada vez que el
/// archivo se guarda, avisando con `texture-updated`. Devuelve la ruta.
#[tauri::command]
pub async fn edit_texture_externally(app: AppHandle, key: TextureKey, format: String) -> Result<String, String> {
    let format = LayeredFormat::from_extension(&format).ok_or("Formato: xcf o psd")?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("texturas");
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let ext = if format == LayeredFormat::Xcf { "xcf" } else { "psd" };
    let target = if key.target == TextureTarget::Quad { "retopo" } else { "original" };
    let path = dir.join(format!("{target}-material{}-{}.{ext}", key.material + 1, key.slot.slug()));

    let written = path.clone();
    in_background(app.clone(), move |state| {
        let bytes = editing_image(state, key)?.write(format);
        std::fs::write(&written, bytes).map_err(|e| format!("No se pudo escribir {}: {e}", written.display()))
    })
    .await?;
    open_externally(&path)?;

    let generation = app.state::<TextureWatch>().0.fetch_add(1, Ordering::SeqCst) + 1;
    let watched = path.clone();
    std::thread::spawn(move || watch(app, key, watched, generation));
    Ok(path.display().to_string())
}

/// Deja de seguir el archivo editado afuera
#[tauri::command]
pub fn stop_texture_watch(watch: State<'_, TextureWatch>) {
    watch.0.fetch_add(1, Ordering::SeqCst);
}

fn modified(path: &PathBuf) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

fn watch(app: AppHandle, key: TextureKey, path: PathBuf, generation: u64) {
    let mut seen = modified(&path);
    // Cambio detectado y aún no estable (el programa sigue escribiendo)
    let mut pending: Option<(SystemTime, u64)> = None;
    while app.state::<TextureWatch>().0.load(Ordering::SeqCst) == generation {
        std::thread::sleep(Duration::from_millis(700));
        let now = modified(&path);
        if now.is_none() || now == seen {
            pending = None;
            continue;
        }
        if pending != now {
            pending = now;
            continue;
        }
        // Igual en dos lecturas seguidas: terminó de guardar
        seen = now;
        pending = None;
        let state = app.state::<AppState>();
        match import_from(&state, key, &path) {
            Ok(()) => {
                let _ = app.emit(TEXTURE_UPDATED, key);
            }
            Err(message) => {
                let _ = app.emit(TEXTURE_UPDATE_FAILED, UpdateFailed { key, message });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un cuadrado con UV en dos islas (un triángulo cada una) y color base
    fn textured_state() -> AppState {
        use converter_scene::{IndexData, Mesh, Node, Primitive, Transform, VertexAttribute};
        let mut scene = Scene::new();
        let pixels: Vec<u8> = (0..64 * 64).flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 4) as u8, 90, 255]).collect();
        scene.textures.push(Texture {
            name: "piel".into(),
            data: encode_png(64, 64, pixels).unwrap(),
            format: TextureFormat::Png,
            width: 64,
            height: 64,
        });
        scene.materials.push(Material {
            name: "piel".into(),
            base_color_texture: Some(TextureRef { texture_index: 0, tex_coord_set: 0 }),
            base_color_factor: [0.5, 0.5, 0.5, 1.0],
            ..Material::default()
        });
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let uvs = vec![[0.1, 0.1], [0.4, 0.1], [0.4, 0.4], [0.6, 0.6], [0.9, 0.9], [0.6, 0.9]];
        scene.meshes.push(Mesh {
            name: "cuadrado".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(positions), VertexAttribute::TexCoords(0, uvs)],
                indices: Some(IndexData::U32(vec![0, 1, 2, 3, 4, 5])),
                material: Some(0),
            }],
        });
        scene.nodes.push(Node { name: "cuadrado".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] });
        scene.root_nodes.push(0);
        let state = AppState::new();
        *state.scene.lock().unwrap() = Some(scene);
        state
    }

    fn key(slot: TextureSlot) -> TextureKey {
        TextureKey { target: TextureTarget::Original, material: 0, slot }
    }

    #[test]
    fn export_and_reimport_ignores_guides() {
        let state = textured_state();
        let dir = tempfile::tempdir().unwrap();
        for ext in ["xcf", "psd"] {
            let path = dir.path().join(format!("t.{ext}"));
            export_to(&state, key(TextureSlot::Base), &path, false).unwrap();
            let layered = LayeredImage::read(&std::fs::read(&path).unwrap()).unwrap();
            let names: Vec<&str> = layered.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, ["Textura", ISLAND_LAYER, WIRE_LAYER, BORDER_LAYER]);
            // Las guías tienen algo dibujado
            assert!(layered.layers[2].pixels.chunks(4).any(|p| p[3] > 0));

            let before = state.scene.lock().unwrap().as_ref().unwrap().textures[0].clone();
            import_from(&state, key(TextureSlot::Base), &path).unwrap();
            let after = state.scene.lock().unwrap().as_ref().unwrap().textures[0].clone();
            let decode = |t: &Texture| image::load_from_memory(&t.data).unwrap().to_rgba8().into_raw();
            assert_eq!(decode(&before), decode(&after), "sin las guías vuelve la misma imagen ({ext})");
        }
    }

    #[test]
    fn importing_into_an_empty_slot_creates_the_texture() {
        let state = textured_state();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nueva.xcf");
        // Metal/rugosidad no tiene imagen: se exporta una del color del material
        export_to(&state, key(TextureSlot::MetallicRoughness), &path, false).unwrap();
        import_from(&state, key(TextureSlot::MetallicRoughness), &path).unwrap();
        {
            let scene = state.scene.lock().unwrap();
            let scene = scene.as_ref().unwrap();
            let reference = scene.materials[0].metallic_roughness_texture.as_ref().unwrap();
            assert_eq!(reference.texture_index, 1);
            assert_eq!(scene.materials[0].roughness_factor, 1.0);
        }
        // PNG solo con una imagen existente
        assert!(export_to(&state, key(TextureSlot::Emissive), &dir.path().join("x.png"), false).is_err());
    }
}
