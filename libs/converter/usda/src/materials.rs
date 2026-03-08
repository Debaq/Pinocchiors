use converter_scene::{Material, Scene, Texture, TextureFormat, TextureRef};
use std::collections::BTreeSet;

use crate::textures::TextureProcessingResult;
use crate::writer::{sanitize_name, UsdWriter};

// ---------------------------------------------------------------------------
// Scope de materiales
// ---------------------------------------------------------------------------

pub(crate) fn write_materials_scope(
    w: &mut UsdWriter,
    scene: &Scene,
    tex_result: &TextureProcessingResult,
) {
    w.open_block("def Scope \"Materials\"");

    for (i, mat) in scene.materials.iter().enumerate() {
        if i > 0 {
            w.blank();
        }
        write_material(w, mat, i, scene, tex_result);
    }

    w.close_block();
}

/// Nombre USD del prim de material (usado también desde writer.rs para bindings).
pub(crate) fn material_prim_name(mat: &Material, idx: usize) -> String {
    sanitize_name(&mat.name, "material", idx)
}

/// Ruta de asset para una textura (usada en USDA y para empaquetado USDZ).
pub fn texture_asset_path(tex: &Texture, idx: usize) -> String {
    let name = if tex.name.is_empty() {
        format!("texture_{}", idx)
    } else {
        sanitize_name(&tex.name, "texture", idx)
    };
    let ext = match tex.format {
        TextureFormat::Png => "png",
        TextureFormat::Jpeg => "jpg",
        TextureFormat::WebP => "png", // WebP no soportado en USD
    };
    format!("textures/{}.{}", name, ext)
}

// ---------------------------------------------------------------------------
// Material individual
// ---------------------------------------------------------------------------

fn write_material(
    w: &mut UsdWriter,
    mat: &Material,
    idx: usize,
    scene: &Scene,
    tex_result: &TextureProcessingResult,
) {
    let name = material_prim_name(mat, idx);
    let mat_path = format!("/Root/Materials/{}", name);

    w.open_block(&format!("def Material \"{}\"", name));
    w.write_fmt_line(format_args!(
        "token outputs:surface.connect = <{}/PBRShader.outputs:surface>",
        mat_path
    ));
    w.blank();

    // Shader principal
    write_pbr_shader(w, mat, &mat_path, tex_result);

    // Shaders de textura
    write_texture_shaders(w, mat, &mat_path, scene, tex_result);

    // Lectores de primvar (UV)
    write_st_readers(w, mat, &mat_path);

    w.close_block();
}

// ---------------------------------------------------------------------------
// UsdPreviewSurface
// ---------------------------------------------------------------------------

fn write_pbr_shader(
    w: &mut UsdWriter,
    mat: &Material,
    mat_path: &str,
    tex_result: &TextureProcessingResult,
) {
    w.open_block("def Shader \"PBRShader\"");
    w.line("uniform token info:id = \"UsdPreviewSurface\"");

    if mat.unlit {
        // Unlit: emissive = base_color, diffuse = negro, metallic = 0, roughness = 0.9
        w.line("color3f inputs:diffuseColor = (0, 0, 0)");
        w.line("float inputs:metallic = 0");
        w.line("float inputs:roughness = 0.9");

        if mat.base_color_texture.is_some() {
            w.write_fmt_line(format_args!(
                "color3f inputs:emissiveColor.connect = <{}/emissiveTex.outputs:rgb>",
                mat_path
            ));
            w.write_fmt_line(format_args!(
                "float inputs:opacity.connect = <{}/emissiveTex.outputs:a>",
                mat_path
            ));
        } else {
            let [r, g, b, _] = mat.base_color_factor;
            w.write_fmt_line(format_args!(
                "color3f inputs:emissiveColor = ({}, {}, {})",
                r, g, b
            ));
            if mat.base_color_factor[3] < 1.0 - 1e-6 {
                w.write_fmt_line(format_args!(
                    "float inputs:opacity = {}",
                    mat.base_color_factor[3]
                ));
            }
        }
    } else {
        // diffuseColor
        if mat.base_color_texture.is_some() {
            w.write_fmt_line(format_args!(
                "color3f inputs:diffuseColor.connect = <{}/diffuseColorTex.outputs:rgb>",
                mat_path
            ));
        } else {
            let [r, g, b, _] = mat.base_color_factor;
            w.write_fmt_line(format_args!(
                "color3f inputs:diffuseColor = ({}, {}, {})",
                r, g, b
            ));
        }

        // opacity
        if mat.base_color_texture.is_some() {
            w.write_fmt_line(format_args!(
                "float inputs:opacity.connect = <{}/diffuseColorTex.outputs:a>",
                mat_path
            ));
        } else if mat.base_color_factor[3] < 1.0 - 1e-6 {
            w.write_fmt_line(format_args!(
                "float inputs:opacity = {}",
                mat.base_color_factor[3]
            ));
        }

        // metallic & roughness — con soporte ORM split
        let mr_split = mat
            .metallic_roughness_texture
            .as_ref()
            .and_then(|tr| tex_result.orm_splits.get(&tr.texture_index));

        if let Some(split) = mr_split {
            // Canales separados: textura gris, usar outputs:r
            w.write_fmt_line(format_args!(
                "float inputs:metallic.connect = <{}/metallicTex.outputs:r>",
                mat_path
            ));
            w.write_fmt_line(format_args!(
                "float inputs:roughness.connect = <{}/roughnessTex.outputs:r>",
                mat_path
            ));
            // occlusion del split
            w.write_fmt_line(format_args!(
                "float inputs:occlusion.connect = <{}/occlusionTex.outputs:r>",
                mat_path
            ));
            let _ = split; // ya usado arriba en las conexiones
        } else {
            // Sin split: comportamiento original
            if mat.metallic_roughness_texture.is_some() {
                w.write_fmt_line(format_args!(
                    "float inputs:metallic.connect = <{}/metallicRoughnessTex.outputs:b>",
                    mat_path
                ));
            } else {
                w.write_fmt_line(format_args!(
                    "float inputs:metallic = {}",
                    mat.metallic_factor
                ));
            }

            if mat.metallic_roughness_texture.is_some() {
                w.write_fmt_line(format_args!(
                    "float inputs:roughness.connect = <{}/metallicRoughnessTex.outputs:g>",
                    mat_path
                ));
            } else {
                w.write_fmt_line(format_args!(
                    "float inputs:roughness = {}",
                    mat.roughness_factor
                ));
            }

            // occlusion (sin split)
            if mat.occlusion_texture.is_some() {
                w.write_fmt_line(format_args!(
                    "float inputs:occlusion.connect = <{}/occlusionTex.outputs:r>",
                    mat_path
                ));
            }
        }

        // normal
        if mat.normal_texture.is_some() {
            w.write_fmt_line(format_args!(
                "normal3f inputs:normal.connect = <{}/normalTex.outputs:rgb>",
                mat_path
            ));
        }

        // emissive
        let has_emissive =
            mat.emissive_factor != [0.0, 0.0, 0.0] || mat.emissive_texture.is_some();
        if has_emissive {
            if mat.emissive_texture.is_some() {
                w.write_fmt_line(format_args!(
                    "color3f inputs:emissiveColor.connect = <{}/emissiveTex.outputs:rgb>",
                    mat_path
                ));
            } else {
                let [r, g, b] = mat.emissive_factor;
                w.write_fmt_line(format_args!(
                    "color3f inputs:emissiveColor = ({}, {}, {})",
                    r, g, b
                ));
            }
        }
    }

    w.line("token outputs:surface");
    w.close_block();
}

// ---------------------------------------------------------------------------
// UsdUVTexture shaders
// ---------------------------------------------------------------------------

fn write_texture_shaders(
    w: &mut UsdWriter,
    mat: &Material,
    mat_path: &str,
    scene: &Scene,
    tex_result: &TextureProcessingResult,
) {
    if mat.unlit {
        // Unlit: base_color_texture → emissiveTex, sin metallic/normal/occlusion
        if let Some(ref tex_ref) = mat.base_color_texture {
            w.blank();
            write_uv_texture(w, "emissiveTex", tex_ref, mat_path, scene, false, tex_result);
        }
        if let Some(ref tex_ref) = mat.emissive_texture {
            // Si hay emissive texture explícita además, no duplicar
            if mat.base_color_texture.is_none() {
                w.blank();
                write_uv_texture(w, "emissiveTex", tex_ref, mat_path, scene, false, tex_result);
            }
        }
        return;
    }

    if let Some(ref tex_ref) = mat.base_color_texture {
        w.blank();
        write_uv_texture(w, "diffuseColorTex", tex_ref, mat_path, scene, false, tex_result);
    }

    // Metallic/Roughness — con soporte ORM split
    if let Some(ref tex_ref) = mat.metallic_roughness_texture {
        if let Some(split) = tex_result.orm_splits.get(&tex_ref.texture_index) {
            // Generar shaders separados para cada canal
            w.blank();
            write_uv_texture_from_path(w, "metallicTex", &split.metallic_path, tex_ref, mat_path, false);
            w.blank();
            write_uv_texture_from_path(w, "roughnessTex", &split.roughness_path, tex_ref, mat_path, false);
            w.blank();
            write_uv_texture_from_path(w, "occlusionTex", &split.occlusion_path, tex_ref, mat_path, false);
        } else {
            w.blank();
            write_uv_texture(w, "metallicRoughnessTex", tex_ref, mat_path, scene, false, tex_result);
        }
    }

    if let Some(ref tex_ref) = mat.normal_texture {
        w.blank();
        write_uv_texture(w, "normalTex", tex_ref, mat_path, scene, true, tex_result);
    }

    // Occlusion separada (solo si no hay ORM split, que ya la incluye)
    if let Some(ref tex_ref) = mat.occlusion_texture {
        let mr_has_split = mat
            .metallic_roughness_texture
            .as_ref()
            .is_some_and(|mr| tex_result.orm_splits.contains_key(&mr.texture_index));
        if !mr_has_split {
            w.blank();
            write_uv_texture(w, "occlusionTex", tex_ref, mat_path, scene, false, tex_result);
        }
    }

    if let Some(ref tex_ref) = mat.emissive_texture {
        w.blank();
        write_uv_texture(w, "emissiveTex", tex_ref, mat_path, scene, false, tex_result);
    }
}

fn write_uv_texture(
    w: &mut UsdWriter,
    shader_name: &str,
    tex_ref: &TextureRef,
    mat_path: &str,
    scene: &Scene,
    is_normal_map: bool,
    tex_result: &TextureProcessingResult,
) {
    // Preferir la ruta del resultado procesado (puede haber cambiado por WebP→PNG)
    let asset_path = tex_result
        .asset_paths
        .get(&tex_ref.texture_index)
        .cloned()
        .or_else(|| {
            scene
                .textures
                .get(tex_ref.texture_index)
                .map(|tex| texture_asset_path(tex, tex_ref.texture_index))
        })
        .unwrap_or_else(|| format!("textures/texture_{}.png", tex_ref.texture_index));

    write_uv_texture_inner(w, shader_name, &asset_path, tex_ref, mat_path, is_normal_map);
}

fn write_uv_texture_from_path(
    w: &mut UsdWriter,
    shader_name: &str,
    asset_path: &str,
    tex_ref: &TextureRef,
    mat_path: &str,
    is_normal_map: bool,
) {
    write_uv_texture_inner(w, shader_name, asset_path, tex_ref, mat_path, is_normal_map);
}

fn write_uv_texture_inner(
    w: &mut UsdWriter,
    shader_name: &str,
    asset_path: &str,
    tex_ref: &TextureRef,
    mat_path: &str,
    is_normal_map: bool,
) {
    let st_reader = st_reader_name(tex_ref.tex_coord_set);

    w.open_block(&format!("def Shader \"{}\"", shader_name));
    w.line("uniform token info:id = \"UsdUVTexture\"");
    w.write_fmt_line(format_args!("asset inputs:file = @{}@", asset_path));
    w.write_fmt_line(format_args!(
        "float2 inputs:st.connect = <{}/{}.outputs:result>",
        mat_path, st_reader
    ));
    w.line("token inputs:wrapS = \"repeat\"");
    w.line("token inputs:wrapT = \"repeat\"");

    // Normal maps necesitan scale/bias para convertir [0,1] → [-1,1]
    if is_normal_map {
        w.line("float4 inputs:scale = (2, 2, 2, 1)");
        w.line("float4 inputs:bias = (-1, -1, -1, 0)");
    }

    w.line("float3 outputs:rgb");
    w.line("float outputs:r");
    w.line("float outputs:g");
    w.line("float outputs:b");
    w.line("float outputs:a");
    w.close_block();
}

// ---------------------------------------------------------------------------
// UsdPrimvarReader_float2 (lectores de UV)
// ---------------------------------------------------------------------------

fn write_st_readers(w: &mut UsdWriter, mat: &Material, _mat_path: &str) {
    let uv_sets = collect_uv_sets(mat);
    if uv_sets.is_empty() {
        return;
    }

    for &uv_set in &uv_sets {
        w.blank();
        let reader_name = st_reader_name(uv_set);
        let varname = st_varname(uv_set);

        w.open_block(&format!("def Shader \"{}\"", reader_name));
        w.line("uniform token info:id = \"UsdPrimvarReader_float2\"");
        w.write_fmt_line(format_args!("token inputs:varname = \"{}\"", varname));
        w.line("float2 outputs:result");
        w.close_block();
    }
}

fn collect_uv_sets(mat: &Material) -> BTreeSet<u32> {
    let mut sets = BTreeSet::new();
    if let Some(ref t) = mat.base_color_texture {
        sets.insert(t.tex_coord_set);
    }
    if let Some(ref t) = mat.metallic_roughness_texture {
        sets.insert(t.tex_coord_set);
    }
    if let Some(ref t) = mat.normal_texture {
        sets.insert(t.tex_coord_set);
    }
    if let Some(ref t) = mat.occlusion_texture {
        sets.insert(t.tex_coord_set);
    }
    if let Some(ref t) = mat.emissive_texture {
        sets.insert(t.tex_coord_set);
    }
    sets
}

fn st_reader_name(uv_set: u32) -> String {
    if uv_set == 0 {
        "stReader".to_string()
    } else {
        format!("stReader_{}", uv_set)
    }
}

fn st_varname(uv_set: u32) -> String {
    if uv_set == 0 {
        "st".to_string()
    } else {
        format!("st{}", uv_set)
    }
}
