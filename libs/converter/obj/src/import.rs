use converter_scene::{
    AlphaMode, IndexData, Material, Mesh, Node, Primitive, Scene, Texture, TextureFormat,
    TextureRef, Transform, VertexAttribute,
};
use std::path::Path;
use thiserror::Error;

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
    let path = path.as_ref();
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };

    let (models, materials_result) = tobj::load_obj(path, &load_options)?;
    let tobj_materials = materials_result.unwrap_or_default();

    let mut scene = Scene::new();

    // Importar materiales y texturas
    import_materials(&tobj_materials, base_dir, &mut scene);

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
    base_dir: &Path,
    scene: &mut Scene,
) {
    for mat in tobj_materials {
        // Diffuse color → base_color_factor
        let diffuse = mat.diffuse.unwrap_or([0.8, 0.8, 0.8]);
        let dissolve = mat.dissolve.unwrap_or(1.0);
        let base_color_factor = [diffuse[0], diffuse[1], diffuse[2], dissolve];

        // Shininess → roughness (approximation)
        let shininess = mat.shininess.unwrap_or(0.0);
        let roughness = if shininess > 0.0 {
            (1.0 - (shininess / 1000.0).min(1.0)).max(0.04)
        } else {
            1.0
        };

        // Diffuse texture
        let base_color_texture = if let Some(ref tex_name) = mat.diffuse_texture {
            load_texture(tex_name, base_dir, scene)
        } else {
            None
        };

        let alpha_mode = if dissolve < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };

        scene.materials.push(Material {
            name: mat.name.clone(),
            base_color_factor,
            base_color_texture,
            metallic_factor: 0.0,
            roughness_factor: roughness,
            alpha_mode,
            ..Material::default()
        });
    }
}

fn load_texture(
    tex_name: &str,
    base_dir: &Path,
    scene: &mut Scene,
) -> Option<TextureRef> {
    let tex_path = base_dir.join(tex_name);
    let data = std::fs::read(&tex_path).ok()?;

    let format = if tex_name.to_lowercase().ends_with(".png") {
        TextureFormat::Png
    } else if tex_name.to_lowercase().ends_with(".jpg")
        || tex_name.to_lowercase().ends_with(".jpeg")
    {
        TextureFormat::Jpeg
    } else if tex_name.to_lowercase().ends_with(".webp") {
        TextureFormat::WebP
    } else {
        // Sniff from bytes
        if data.starts_with(b"\x89PNG") {
            TextureFormat::Png
        } else if data.starts_with(b"\xFF\xD8\xFF") {
            TextureFormat::Jpeg
        } else {
            TextureFormat::Png
        }
    };

    let idx = scene.textures.len();
    scene.textures.push(Texture {
        name: tex_name.to_string(),
        data,
        format,
        width: 0,
        height: 0,
    });

    Some(TextureRef {
        texture_index: idx,
        tex_coord_set: 0,
    })
}
