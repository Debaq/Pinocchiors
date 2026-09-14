use converter_scene::{AlphaMode, IndexData, Scene, TextureFormat, VertexAttribute};
use std::fmt::Write as FmtWrite;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObjExportError {
    #[error("error escribiendo archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("error formateando: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Exporta una `Scene` a formato OBJ + MTL en disco.
///
/// Genera `path` (archivo .obj) y un .mtl junto a él si hay materiales.
/// Las texturas se escriben como archivos en el mismo directorio.
pub fn export_obj(scene: &Scene, path: impl AsRef<Path>) -> Result<(), ObjExportError> {
    let path = path.as_ref();
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");

    let has_materials = !scene.materials.is_empty();
    let mtl_filename = format!("{stem}.mtl");

    // Escribir MTL + texturas si hay materiales
    if has_materials {
        let mtl_content = write_mtl(scene, base_dir, stem)?;
        std::fs::write(base_dir.join(&mtl_filename), mtl_content)?;
    }

    // Escribir OBJ
    let obj_content = write_obj(scene, if has_materials { Some(&mtl_filename) } else { None })?;
    std::fs::write(path, obj_content)?;

    Ok(())
}

fn write_obj(scene: &Scene, mtl_filename: Option<&str>) -> Result<String, ObjExportError> {
    let mut out = String::new();

    writeln!(out, "# Exported by converter-obj")?;

    if let Some(mtl) = mtl_filename {
        writeln!(out, "mtllib {mtl}")?;
    }

    // Offsets acumulados para índices 1-based entre grupos
    let mut vertex_offset: usize = 0;
    let mut normal_offset: usize = 0;
    let mut uv_offset: usize = 0;

    for (mi, mesh) in scene.meshes.iter().enumerate() {
        writeln!(out)?;
        let name = if mesh.name.is_empty() {
            format!("mesh_{mi}")
        } else {
            mesh.name.clone()
        };
        writeln!(out, "o {name}")?;

        for prim in &mesh.primitives {
            let mut positions: &[[f32; 3]] = &[];
            let mut normals: &[[f32; 3]] = &[];
            let mut uvs: &[[f32; 2]] = &[];

            for attr in &prim.attributes {
                match attr {
                    VertexAttribute::Positions(p) => positions = p,
                    VertexAttribute::Normals(n) => normals = n,
                    VertexAttribute::TexCoords(0, uv) => uvs = uv,
                    _ => {}
                }
            }

            // Vertices
            for p in positions {
                writeln!(out, "v {} {} {}", p[0], p[1], p[2])?;
            }

            // Normals
            for n in normals {
                writeln!(out, "vn {} {} {}", n[0], n[1], n[2])?;
            }

            // UVs
            for uv in uvs {
                writeln!(out, "vt {} {}", uv[0], uv[1])?;
            }

            // Material
            if let Some(mat_idx) = prim.material {
                if mat_idx < scene.materials.len() {
                    let mat_name = if scene.materials[mat_idx].name.is_empty() {
                        format!("material_{mat_idx}")
                    } else {
                        scene.materials[mat_idx].name.clone()
                    };
                    writeln!(out, "usemtl {mat_name}")?;
                }
            }

            // Faces (1-based)
            let has_normals = !normals.is_empty();
            let has_uvs = !uvs.is_empty();

            let face_indices: Vec<u32> = match &prim.indices {
                Some(IndexData::U16(idx)) => idx.iter().map(|&i| i as u32).collect(),
                Some(IndexData::U32(idx)) => idx.clone(),
                None => (0..positions.len() as u32).collect(),
            };

            for face in face_indices.chunks(3) {
                if face.len() < 3 { continue; }
                write!(out, "f")?;
                for &idx in face {
                    let vi = idx as usize + vertex_offset + 1; // 1-based
                    match (has_uvs, has_normals) {
                        (true, true) => {
                            let ti = idx as usize + uv_offset + 1;
                            let ni = idx as usize + normal_offset + 1;
                            write!(out, " {vi}/{ti}/{ni}")?;
                        }
                        (true, false) => {
                            let ti = idx as usize + uv_offset + 1;
                            write!(out, " {vi}/{ti}")?;
                        }
                        (false, true) => {
                            let ni = idx as usize + normal_offset + 1;
                            write!(out, " {vi}//{ni}")?;
                        }
                        (false, false) => {
                            write!(out, " {vi}")?;
                        }
                    }
                }
                writeln!(out)?;
            }

            vertex_offset += positions.len();
            normal_offset += normals.len();
            uv_offset += uvs.len();
        }
    }

    Ok(out)
}

fn write_mtl(scene: &Scene, base_dir: &Path, stem: &str) -> Result<String, ObjExportError> {
    let mut out = String::new();
    writeln!(out, "# MTL exported by converter-obj")?;

    for (i, mat) in scene.materials.iter().enumerate() {
        let name = if mat.name.is_empty() {
            format!("material_{i}")
        } else {
            mat.name.clone()
        };
        writeln!(out)?;
        writeln!(out, "newmtl {name}")?;

        // Diffuse color (Kd)
        let [r, g, b, _a] = mat.base_color_factor;
        writeln!(out, "Kd {r} {g} {b}")?;

        // Specular color (Ks) — derive from metallic
        let spec = mat.metallic_factor;
        writeln!(out, "Ks {spec} {spec} {spec}")?;

        // Shininess (Ns) — derive from roughness
        let ns = ((1.0 - mat.roughness_factor) * 1000.0).max(0.0);
        writeln!(out, "Ns {ns}")?;

        // Dissolve (d)
        let dissolve = match mat.alpha_mode {
            AlphaMode::Opaque => 1.0,
            AlphaMode::Blend => mat.base_color_factor[3],
            AlphaMode::Mask(_) => mat.base_color_factor[3],
        };
        writeln!(out, "d {dissolve}")?;

        // Illumination model
        writeln!(out, "illum 2")?;

        // Diffuse texture (map_Kd)
        if let Some(ref tex_ref) = mat.base_color_texture {
            if tex_ref.texture_index < scene.textures.len() {
                let tex = &scene.textures[tex_ref.texture_index];
                let ext = match tex.format {
                    TextureFormat::Png => "png",
                    TextureFormat::Jpeg => "jpg",
                    TextureFormat::WebP => "webp",
                };
                let tex_filename = if tex.name.is_empty() {
                    format!("{stem}_tex_{}.{ext}", tex_ref.texture_index)
                } else {
                    // Asegurar que tiene extensión correcta
                    let base = tex.name.split('.').next().unwrap_or(&tex.name);
                    format!("{base}.{ext}")
                };

                // Escribir archivo de textura
                let tex_path = base_dir.join(&tex_filename);
                std::fs::write(&tex_path, &tex.data)?;

                writeln!(out, "map_Kd {tex_filename}")?;
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
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
                indices: Some(IndexData::U32(vec![0, 1, 2])),
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
    fn export_triangle() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.obj");
        export_obj(&scene, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("v 0 0 0"));
        assert!(content.contains("v 1 0 0"));
        assert!(content.contains("v 0 1 0"));
        assert!(content.contains("f 1 2 3"));
    }

    #[test]
    fn export_1based_indices() {
        let scene = triangle_scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.obj");
        export_obj(&scene, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Verificar que los índices son 1-based
        for line in content.lines() {
            if line.starts_with("f ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                for &p in &parts[1..] {
                    let idx: u32 = p.split('/').next().unwrap().parse().unwrap();
                    assert!(idx >= 1, "índice OBJ debe ser 1-based, pero fue {idx}");
                }
            }
        }
    }

    #[test]
    fn export_with_material() {
        let mut scene = triangle_scene();
        scene.materials.push(Material {
            name: "Red".into(),
            base_color_factor: [1.0, 0.0, 0.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.8,
            ..Material::default()
        });
        scene.meshes[0].primitives[0].material = Some(0);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.obj");
        export_obj(&scene, &path).unwrap();

        // Verificar .obj referencia mtllib
        let obj_content = std::fs::read_to_string(&path).unwrap();
        assert!(obj_content.contains("mtllib test.mtl"));
        assert!(obj_content.contains("usemtl Red"));

        // Verificar .mtl existe y tiene contenido correcto
        let mtl_path = dir.path().join("test.mtl");
        let mtl_content = std::fs::read_to_string(&mtl_path).unwrap();
        assert!(mtl_content.contains("newmtl Red"));
        assert!(mtl_content.contains("Kd 1 0 0"));
    }

    #[test]
    fn import_simple_obj() {
        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("cube.obj");
        let obj_content = "\
# Simple cube (8 vertices, 12 triangles)
o Cube
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
v 0.0 0.0 1.0
v 1.0 0.0 1.0
v 1.0 1.0 1.0
v 0.0 1.0 1.0
f 1 2 3
f 1 3 4
f 5 6 7
f 5 7 8
f 1 2 6
f 1 6 5
f 2 3 7
f 2 7 6
f 3 4 8
f 3 8 7
f 4 1 5
f 4 5 8
";
        std::fs::write(&obj_path, obj_content).unwrap();

        let scene = crate::import_obj(&obj_path).unwrap();

        assert_eq!(scene.meshes.len(), 1);
        assert_eq!(scene.meshes[0].name, "Cube");

        let prim = &scene.meshes[0].primitives[0];
        let positions = prim.attributes.iter().find_map(|a| {
            if let VertexAttribute::Positions(p) = a { Some(p) } else { None }
        }).unwrap();
        assert!(positions.len() >= 8, "debe tener al menos 8 vértices");

        assert!(prim.indices.is_some());
    }

    #[test]
    fn import_with_material() {
        let dir = tempfile::tempdir().unwrap();

        let mtl_content = "\
newmtl RedMaterial
Kd 1.0 0.0 0.0
Ns 100.0
d 1.0
";
        std::fs::write(dir.path().join("test.mtl"), mtl_content).unwrap();

        let obj_content = "\
mtllib test.mtl
o Triangle
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
usemtl RedMaterial
f 1 2 3
";
        let obj_path = dir.path().join("test.obj");
        std::fs::write(&obj_path, obj_content).unwrap();

        let scene = crate::import_obj(&obj_path).unwrap();

        assert_eq!(scene.materials.len(), 1);
        let mat = &scene.materials[0];
        assert_eq!(mat.name, "RedMaterial");
        assert_eq!(mat.base_color_factor[0], 1.0);
        assert_eq!(mat.base_color_factor[1], 0.0);
        assert_eq!(mat.base_color_factor[2], 0.0);
    }

    #[test]
    fn roundtrip_obj() {
        let scene = triangle_scene();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt.obj");
        export_obj(&scene, &path).unwrap();
        let imported = crate::import_obj(&path).unwrap();

        // Comparar bounding boxes
        let original_bb = scene.compute_bounding_box().unwrap();
        let imported_bb = imported.compute_bounding_box().unwrap();

        for i in 0..3 {
            assert!(
                (original_bb.0[i] - imported_bb.0[i]).abs() < 1e-5,
                "min[{i}] difiere: {} vs {}",
                original_bb.0[i], imported_bb.0[i],
            );
            assert!(
                (original_bb.1[i] - imported_bb.1[i]).abs() < 1e-5,
                "max[{i}] difiere: {} vs {}",
                original_bb.1[i], imported_bb.1[i],
            );
        }
    }
}
