use converter_scene::{
    AlphaMode, IndexData, Material, Mesh, Node, Primitive, Scene, Transform, VertexAttribute,
};
use std::cell::RefCell;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::maps::{apply_maps, AttachReport, MapKind, MapSet};

#[derive(Debug, Error)]
pub enum ObjImportError {
    #[error("geometría inválida: {0}")]
    InvalidGeometry(#[from] converter_scene::SceneError),
    #[error("error leyendo archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("error parseando OBJ: {0}")]
    Tobj(#[from] tobj::LoadError),
}

/// Importa un archivo OBJ (con MTL opcional) y lo convierte a `Scene`.
pub fn import_obj(path: impl AsRef<Path>) -> Result<Scene, ObjImportError> {
    import_obj_with_mtl(path, None)
}

/// Igual que [`import_obj`], con el MTL elegido a mano: reemplaza al que
/// nombra el OBJ (que a veces trae otro nombre o una ruta de otra máquina).
pub fn import_obj_with_mtl(path: impl AsRef<Path>, mtl: Option<&Path>) -> Result<Scene, ObjImportError> {
    let path = path.as_ref();
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    // MTL leídos de verdad: sus mapas se leen aparte (tobj solo da algunos)
    let used_mtls: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let mut reader = BufReader::new(File::open(path)?);
    let (models, materials_result) = tobj::load_obj_buf(&mut reader, &load_options, |name| {
        let found = match mtl {
            Some(chosen) => Some(chosen.to_path_buf()),
            None => resolve_file(&name.to_string_lossy(), &[base_dir])
                .or_else(|| sibling_mtl(path)),
        };
        let Some(found) = found else {
            return Err(tobj::LoadError::OpenFileFailed);
        };
        let result = tobj::load_mtl(&found);
        if result.is_ok() {
            used_mtls.borrow_mut().push(found);
        }
        result
    })?;
    let tobj_materials = materials_result.unwrap_or_default();
    let mtl_entries: Vec<MtlEntry> = used_mtls.into_inner().iter().flat_map(|p| parse_mtl(p, base_dir)).collect();

    let mut scene = Scene::new();

    // Importar materiales y texturas
    import_materials(&tobj_materials, &mtl_entries, &mut scene);

    // Importar meshes
    for model in &models {
        let mesh = &model.mesh;

        let mut attributes = Vec::new();

        // Posiciones
        if !mesh.positions.is_empty() {
            let positions: Vec<[f32; 3]> = mesh.positions
                .as_chunks::<3>().0.iter()
                .map(|c| [c[0], c[1], c[2]])
                .collect();
            attributes.push(VertexAttribute::Positions(positions));
        }

        // Normales
        if !mesh.normals.is_empty() {
            let normals: Vec<[f32; 3]> = mesh.normals
                .as_chunks::<3>().0.iter()
                .map(|c| [c[0], c[1], c[2]])
                .collect();
            attributes.push(VertexAttribute::Normals(normals));
        }

        // Coordenadas UV
        if !mesh.texcoords.is_empty() {
            let uvs: Vec<[f32; 2]> = mesh.texcoords
                .as_chunks::<2>().0.iter()
                .map(|c| [c[0], c[1]])
                .collect();
            attributes.push(VertexAttribute::TexCoords(0, uvs));
        }

        // Índices
        let indices = if !mesh.indices.is_empty() {
            Some(IndexData::U32(mesh.indices.clone()))
        } else {
            None
        };

        // Material — tobj usa un ID de material por mesh
        let material = mesh.material_id;

        let prim = Primitive {
            attributes,
            indices,
            material,
        };

        scene.meshes.push(Mesh {
            name: model.name.clone(),
            primitives: vec![prim],
        });
    }

    // Nodos — uno por mesh
    for (i, model) in models.iter().enumerate() {
        scene.nodes.push(Node {
            name: model.name.clone(),
            transform: Transform::identity(),
            mesh: Some(i),
            skin: None,
            children: Vec::new(),
        });
        scene.root_nodes.push(i);
    }

    scene.validate_geometry()?;
    Ok(scene)
}

fn import_materials(
    tobj_materials: &[tobj::Material],
    mtl_entries: &[MtlEntry],
    scene: &mut Scene,
) {
    for mat in tobj_materials {
        let entry = mtl_entries.iter().find(|e| e.name == mat.name);

        // Diffuse color → base_color_factor
        let diffuse = mat.diffuse.unwrap_or([0.8, 0.8, 0.8]);
        let dissolve = mat.dissolve.unwrap_or(1.0);
        let base_color_factor = [diffuse[0], diffuse[1], diffuse[2], dissolve];

        // Rugosidad: la PBR del MTL (Pr) o aproximada desde el brillo (Ns)
        let shininess = mat.shininess.unwrap_or(0.0);
        let roughness = entry.and_then(|e| e.roughness).unwrap_or(if shininess > 0.0 {
            (1.0 - (shininess / 1000.0).min(1.0)).max(0.04)
        } else {
            1.0
        });

        let alpha_mode = if dissolve < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };

        let index = scene.materials.len();
        scene.materials.push(Material {
            name: mat.name.clone(),
            base_color_factor,
            metallic_factor: entry.and_then(|e| e.metallic).unwrap_or(0.0),
            roughness_factor: roughness,
            emissive_factor: entry.and_then(|e| e.emissive).unwrap_or([0.0; 3]),
            alpha_mode,
            ..Material::default()
        });

        // Mapas del MTL (color, normales, rugosidad, metal, emisión…)
        if let Some(entry) = entry {
            apply_maps(scene, index, &entry.maps, true, &mut AttachReport::default());
        }
    }
}

/// Lo que se lee del MTL además de lo que da tobj
#[derive(Debug, Default)]
struct MtlEntry {
    name: String,
    maps: MapSet,
    roughness: Option<f32>,
    metallic: Option<f32>,
    emissive: Option<[f32; 3]>,
}

/// Mapa de cada instrucción `map_…` del MTL
fn mtl_map_kind(keyword: &str) -> Option<MapKind> {
    Some(match keyword.to_lowercase().as_str() {
        "map_kd" => MapKind::BaseColor,
        "map_d" => MapKind::Opacity,
        "map_bump" | "bump" => MapKind::Bump,
        "norm" | "map_kn" | "map_normal" | "map_norm" => MapKind::Normal,
        "map_pr" => MapKind::Roughness,
        "map_pm" => MapKind::Metallic,
        "map_ns" => MapKind::Glossiness,
        "map_ke" => MapKind::Emissive,
        "map_ao" | "map_occlusion" => MapKind::Occlusion,
        "map_rma" | "map_orm" => MapKind::Orm,
        _ => return None,
    })
}

/// Opciones de una línea de mapa y cuántos números llevan (`-o`, `-s` y
/// `-t` llevan de 1 a 3)
fn option_args(option: &str) -> (usize, usize) {
    match option {
        "-o" | "-s" | "-t" => (1, 3),
        "-mm" => (2, 2),
        "-blendu" | "-blendv" | "-cc" | "-clamp" | "-imfchan" | "-texres" | "-bm" | "-boost" | "-type" => (1, 1),
        _ => (0, 0),
    }
}

/// Archivo de una línea de mapa: lo que queda después de las opciones (el
/// nombre puede tener espacios)
fn map_file(args: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') && args[i].len() > 1 && args[i].parse::<f32>().is_err() {
        let (min, max) = option_args(args[i]);
        i += 1;
        let mut taken = 0;
        while taken < max && i < args.len() && (taken < min || args[i].parse::<f32>().is_ok()) {
            i += 1;
            taken += 1;
        }
    }
    let name = args[i.min(args.len())..].join(" ");
    (!name.is_empty()).then_some(name)
}

fn parse_mtl(path: &Path, obj_dir: &Path) -> Vec<MtlEntry> {
    let Ok(text) = std::fs::read(path) else { return Vec::new() };
    let text = String::from_utf8_lossy(&text);
    let mtl_dir = path.parent().unwrap_or(obj_dir);
    let mut entries: Vec<MtlEntry> = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&keyword, args)) = words.split_first() else { continue };
        if keyword.starts_with('#') {
            continue;
        }
        if keyword == "newmtl" {
            entries.push(MtlEntry { name: args.join(" "), ..MtlEntry::default() });
            continue;
        }
        let Some(entry) = entries.last_mut() else { continue };
        let number = |i: usize| args.get(i).and_then(|v| v.parse::<f32>().ok());
        match keyword {
            "Pr" => entry.roughness = number(0),
            "Pm" => entry.metallic = number(0),
            "Ke" => {
                if let (Some(r), g, b) = (number(0), number(1), number(2)) {
                    entry.emissive = Some([r, g.unwrap_or(r), b.unwrap_or(r)]);
                }
            }
            _ => {
                let Some(kind) = mtl_map_kind(keyword) else { continue };
                let Some(file) = map_file(args) else { continue };
                if let Some(found) = resolve_file(&file, &[mtl_dir, obj_dir]) {
                    entry.maps.entry(kind).or_insert(found);
                }
            }
        }
    }
    entries
}

/// Busca un archivo nombrado en un OBJ o MTL: tal cual, junto al MTL o al
/// OBJ, sin la carpeta (rutas de otra máquina, con `\\` de Windows), en las
/// subcarpetas típicas de texturas y sin distinguir mayúsculas
fn resolve_file(name: &str, dirs: &[&Path]) -> Option<PathBuf> {
    let unified = name.trim().trim_matches('"').replace('\\', "/");
    let as_is = Path::new(&unified);
    if as_is.is_absolute() && as_is.is_file() {
        return Some(as_is.to_path_buf());
    }
    let bare = as_is.file_name()?.to_string_lossy().into_owned();
    for dir in dirs {
        let mut candidates = vec![dir.join(&unified), dir.join(&bare)];
        for sub in ["textures", "Textures", "texture", "tex", "maps", "images"] {
            candidates.push(dir.join(sub).join(&bare));
        }
        if let Some(found) = candidates.into_iter().find(|c| c.is_file()) {
            return Some(found);
        }
        let lower = bare.to_lowercase();
        let found = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path())
            .find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == lower));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// El MTL con el mismo nombre que el OBJ, o el único MTL de la carpeta
fn sibling_mtl(obj: &Path) -> Option<PathBuf> {
    let same = obj.with_extension("mtl");
    if same.is_file() {
        return Some(same);
    }
    let dir = obj.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mtls: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mtl")))
        .collect();
    match mtls.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_lines_skip_options() {
        assert_eq!(map_file(&["-bm", "0.5", "robot", "normal.png"]).as_deref(), Some("robot normal.png"));
        assert_eq!(map_file(&["-s", "1", "1", "1", "-o", "0", "tex.png"]).as_deref(), Some("tex.png"));
        assert_eq!(map_file(&["-clamp", "on", "a.tga"]).as_deref(), Some("a.tga"));
        assert_eq!(map_file(&["-bm"]), None);
    }

    #[test]
    fn obj_with_pbr_mtl_gets_every_map() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("textures")).unwrap();
        let png = |name: &str, rgba: [u8; 4]| {
            image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba))
                .save_with_format(d.join(name), image::ImageFormat::Png)
                .unwrap();
        };
        png("textures/Casco_col.png", [200, 0, 0, 255]);
        png("textures/Casco_nrm.png", [128, 128, 255, 255]);
        png("textures/Casco_rough.png", [60, 60, 60, 255]);
        png("textures/Casco_metal.png", [250, 250, 250, 255]);
        // Ruta de otra máquina y nombre de MTL distinto al del OBJ
        std::fs::write(d.join("casco.mtl"), "newmtl Casco\nKd 0.8 0.8 0.8\nPm 0.1\n\
            map_Kd C:\\Users\\x\\Casco_col.png\nmap_Bump -bm 1.0 textures/Casco_nrm.png\n\
            map_Pr Casco_rough.png\nmap_Pm Casco_metal.png\n").unwrap();
        std::fs::write(d.join("casco.obj"), "mtllib otro.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\n\
            vt 0 0\nvt 1 0\nvt 0 1\nusemtl Casco\nf 1/1 2/2 3/3\n").unwrap();

        let scene = import_obj(d.join("casco.obj")).unwrap();
        let m = &scene.materials[0];
        assert!(m.base_color_texture.is_some(), "color");
        assert!(m.normal_texture.is_some(), "normales");
        assert_eq!(m.base_color_factor, [1.0, 1.0, 1.0, 1.0]);
        let mr = &scene.textures[m.metallic_roughness_texture.as_ref().unwrap().texture_index];
        let mr = image::load_from_memory(&mr.data).unwrap().to_rgba8();
        assert_eq!(mr.get_pixel(0, 0).0[1..3], [60, 250]);
    }
}
