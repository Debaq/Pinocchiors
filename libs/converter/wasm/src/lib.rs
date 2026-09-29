//! WASM bindings para conversión de formatos 3D.
//!
//! Provee funciones accesibles desde JavaScript para convertir
//! entre formatos 3D en el navegador.

use wasm_bindgen::prelude::*;

/// Convierte bytes de un formato 3D a otro.
///
/// # Argumentos
/// - `input_bytes`: bytes del archivo de entrada
/// - `input_format`: formato de entrada ("glb", "stl", "ply")
/// - `output_format`: formato de salida ("glb", "usda", "usdz", "stl", "ply", "3mf")
/// - `options_json`: opciones en formato JSON (opcional)
///
/// # Retorna
/// Bytes del archivo convertido.
#[wasm_bindgen]
pub fn convert(
    input_bytes: &[u8],
    input_format: &str,
    output_format: &str,
    options_json: Option<String>,
) -> Result<Vec<u8>, JsError> {
    convert_impl(input_bytes, input_format, output_format, options_json.as_deref())
        .map_err(|e| JsError::new(&e))
}

/// Retorna los formatos soportados como JSON.
#[wasm_bindgen]
pub fn supported_formats() -> String {
    serde_json::json!({
        "import": IMPORT_FORMATS,
        "export": EXPORT_FORMATS,
    })
    .to_string()
}

/// Importa un archivo y retorna metadatos de la escena como JSON.
#[wasm_bindgen]
pub fn import_to_json(input_bytes: &[u8], format: &str) -> Result<String, JsError> {
    import_to_json_impl(input_bytes, format).map_err(|e| JsError::new(&e))
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------
//
// La lógica devuelve `String` como error: `JsError::new` solo funciona dentro
// de wasm, y así se puede testear de forma nativa.

const IMPORT_FORMATS: &[&str] = &["glb", "stl", "ply"];
const EXPORT_FORMATS: &[&str] = &["glb", "usda", "usdz", "stl", "ply", "3mf"];

fn convert_impl(
    input_bytes: &[u8],
    input_format: &str,
    output_format: &str,
    options_json: Option<&str>,
) -> Result<Vec<u8>, String> {
    let scene = import_scene(input_bytes, input_format)?;
    let options = parse_options(options_json)?;
    export_scene(&scene, output_format, &options)
}

fn import_to_json_impl(input_bytes: &[u8], format: &str) -> Result<String, String> {
    let scene = import_scene(input_bytes, format)?;

    let info = serde_json::json!({
        "nodes": scene.nodes.len(),
        "meshes": scene.meshes.len(),
        "materials": scene.materials.len(),
        "textures": scene.textures.len(),
        "skeletons": scene.skeletons.len(),
        "animations": scene.animations.len(),
    });

    Ok(info.to_string())
}

fn import_scene(data: &[u8], format: &str) -> Result<converter_scene::Scene, String> {
    match format.to_ascii_lowercase().as_str() {
        "glb" | "gltf" => converter_gltf_io::import_gltf_bytes(data).map_err(|e| e.to_string()),
        "stl" => converter_stl::import_stl_bytes(data).map_err(|e| e.to_string()),
        "ply" => converter_ply::import_ply_bytes(data).map_err(|e| e.to_string()),
        _ => Err(format!("formato de importación no soportado: {}", format)),
    }
}

#[derive(Default)]
struct WasmOptions {
    scale_factor: Option<f64>,
    max_texture_size: Option<u32>,
    arkit_compatible: bool,
    fps: f64,
    export_animations: bool,
    texture_quality: Option<u8>,
    optimize_geometry: bool,
    generate_normals: bool,
    flatten_transforms: bool,
    strip_unused: bool,
    draco: bool,
}

fn parse_options(json: Option<&str>) -> Result<WasmOptions, String> {
    let Some(json) = json else {
        return Ok(WasmOptions {
            fps: 24.0,
            export_animations: true,
            ..WasmOptions::default()
        });
    };

    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("opciones JSON inválidas: {e}"))?;

    let scale_factor = v.get("scale_factor").and_then(|v| v.as_f64());
    if scale_factor.is_some_and(|f| !(f.is_finite() && f > 0.0)) {
        return Err("scale_factor debe ser un número positivo".into());
    }
    let max_texture_size = match v.get("max_texture_size").and_then(|v| v.as_u64()) {
        Some(size) => Some(
            u32::try_from(size)
                .ok()
                .filter(|&s| s > 0)
                .ok_or("max_texture_size fuera de rango")?,
        ),
        None => None,
    };
    let texture_quality = match v.get("texture_quality").and_then(|v| v.as_u64()) {
        Some(q @ 1..=100) => Some(q as u8),
        Some(_) => return Err("texture_quality debe estar entre 1 y 100".into()),
        None => None,
    };

    Ok(WasmOptions {
        scale_factor,
        max_texture_size,
        arkit_compatible: v.get("arkit_compatible").and_then(|v| v.as_bool()).unwrap_or(false),
        fps: v.get("fps").and_then(|v| v.as_f64()).unwrap_or(24.0),
        export_animations: v.get("export_animations").and_then(|v| v.as_bool()).unwrap_or(true),
        texture_quality,
        optimize_geometry: v.get("optimize_geometry").and_then(|v| v.as_bool()).unwrap_or(false),
        generate_normals: v.get("generate_normals").and_then(|v| v.as_bool()).unwrap_or(false),
        flatten_transforms: v.get("flatten_transforms").and_then(|v| v.as_bool()).unwrap_or(false),
        strip_unused: v.get("strip_unused").and_then(|v| v.as_bool()).unwrap_or(false),
        draco: v.get("draco").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

fn export_scene(
    scene: &converter_scene::Scene,
    format: &str,
    options: &WasmOptions,
) -> Result<Vec<u8>, String> {
    let usda_opts = converter_usda::UsdaExportOptions {
        scale_factor: options.scale_factor,
        max_texture_size: options.max_texture_size,
        split_orm_channels: false,
        arkit_compatible: options.arkit_compatible,
        fps: options.fps,
        export_animations: options.export_animations,
        keyframe_tolerance: 1e-4,
    };

    let glb_opts = converter_gltf_io::GlbExportOptions {
        texture_quality: options.texture_quality,
        max_texture_size: options.max_texture_size,
        optimize_geometry: options.optimize_geometry,
        generate_normals: options.generate_normals,
        flatten_transforms: options.flatten_transforms,
        scale_factor: options.scale_factor,
        export_animations: options.export_animations,
        strip_unused: options.strip_unused,
        draco: options.draco.then(converter_gltf_io::DracoOptions::default),
        ..Default::default()
    };

    match format.to_ascii_lowercase().as_str() {
        "glb" | "gltf" => {
            converter_gltf_io::export_glb_bytes(scene, &glb_opts).map_err(|e| e.to_string())
        }
        "usda" => converter_usda::write_usda(scene, &usda_opts)
            .map(|output| output.usda.into_bytes())
            .map_err(|e| e.to_string()),
        "usdz" => converter_usda::write_usdz_bytes(scene, &usda_opts).map_err(|e| e.to_string()),
        "stl" => converter_stl::export_stl_bytes(scene).map_err(|e| e.to_string()),
        "ply" => converter_ply::export_ply_bytes(scene).map_err(|e| e.to_string()),
        "3mf" => converter_3mf::export_3mf_bytes(scene).map_err(|e| e.to_string()),
        _ => Err(format!("formato de exportación no soportado: {}", format)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converter_scene::{IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};

    fn triangle_glb() -> Vec<u8> {
        let mut scene = Scene::new();
        scene.meshes.push(Mesh {
            name: "tri".into(),
            primitives: vec![Primitive {
                attributes: vec![VertexAttribute::Positions(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 2.0, 0.0],
                ])],
                indices: Some(IndexData::U32(vec![0, 1, 2])),
                material: None,
            }],
        });
        scene.nodes.push(Node {
            name: "tri".into(),
            transform: Transform::identity(),
            mesh: Some(0),
            skin: None,
            children: vec![],
        });
        scene.root_nodes.push(0);
        converter_gltf_io::export_glb_bytes(&scene, &Default::default()).unwrap()
    }

    #[test]
    fn every_advertised_format_works() {
        let formats: serde_json::Value = serde_json::from_str(&supported_formats()).unwrap();
        let glb = triangle_glb();
        let stl = convert_impl(&glb, "glb", "stl", None).unwrap();
        let ply = convert_impl(&glb, "glb", "ply", None).unwrap();

        for input in formats["import"].as_array().unwrap() {
            let input = input.as_str().unwrap();
            let bytes = match input {
                "stl" => &stl,
                "ply" => &ply,
                _ => &glb,
            };
            for output in formats["export"].as_array().unwrap() {
                let output = output.as_str().unwrap();
                let out = convert_impl(bytes, input, output, None)
                    .unwrap_or_else(|e| panic!("{input} → {output}: {e}"));
                assert!(!out.is_empty(), "{input} → {output} vacío");
            }
        }
    }

    #[test]
    fn glb_to_stl_converts_meters_to_millimeters() {
        let stl = convert_impl(&triangle_glb(), "GLB", "STL", None).unwrap();
        let scene = converter_stl::import_stl_bytes(&stl).unwrap();
        let (_, max) = scene.compute_bounding_box().unwrap();
        assert!((max[0] - 1000.0).abs() < 1e-3 && (max[1] - 2000.0).abs() < 1e-3);
    }

    #[test]
    fn stl_roundtrip_keeps_geometry() {
        let stl = convert_impl(&triangle_glb(), "glb", "stl", None).unwrap();
        let again = convert_impl(&stl, "stl", "stl", None).unwrap();
        assert_eq!(stl, again);
    }

    #[test]
    fn output_matches_requested_format() {
        let glb = triangle_glb();
        assert_eq!(&convert_impl(&glb, "glb", "glb", None).unwrap()[..4], b"glTF");
        assert_eq!(&convert_impl(&glb, "glb", "usdz", None).unwrap()[..2], b"PK");
        let usda = convert_impl(&glb, "glb", "usda", None).unwrap();
        assert!(String::from_utf8(usda).unwrap().starts_with("#usda 1.0"));
    }

    #[test]
    fn import_to_json_counts_scene_elements() {
        let json: serde_json::Value =
            serde_json::from_str(&import_to_json_impl(&triangle_glb(), "glb").unwrap()).unwrap();
        assert_eq!(json["meshes"], 1);
        assert_eq!(json["nodes"], 1);
        assert_eq!(json["skeletons"], 0);
    }

    #[test]
    fn unknown_formats_and_bad_input_are_errors() {
        let glb = triangle_glb();
        assert!(convert_impl(&glb, "fbx", "glb", None).unwrap_err().contains("fbx"));
        assert!(convert_impl(&glb, "glb", "obj", None).unwrap_err().contains("obj"));
        assert!(convert_impl(b"garbage", "glb", "stl", None).is_err());
        assert!(convert_impl(b"garbage", "stl", "glb", None).is_err());
    }

    #[test]
    fn options_are_parsed_and_validated() {
        let o = parse_options(None).unwrap();
        assert_eq!(o.fps, 24.0);
        assert!(o.export_animations);

        let o = parse_options(Some(
            r#"{"scale_factor": 2.5, "fps": 30, "export_animations": false,
                "texture_quality": 80, "max_texture_size": 1024, "arkit_compatible": true}"#,
        ))
        .unwrap();
        assert_eq!(o.scale_factor, Some(2.5));
        assert_eq!(o.fps, 30.0);
        assert!(!o.export_animations);
        assert_eq!(o.texture_quality, Some(80));
        assert_eq!(o.max_texture_size, Some(1024));
        assert!(o.arkit_compatible);

        assert!(parse_options(Some("{not json")).is_err());
        assert!(parse_options(Some(r#"{"texture_quality": 300}"#)).is_err());
        assert!(parse_options(Some(r#"{"texture_quality": 0}"#)).is_err());
        assert!(parse_options(Some(r#"{"max_texture_size": 99999999999}"#)).is_err());
        assert!(parse_options(Some(r#"{"scale_factor": -1}"#)).is_err());
    }

    #[test]
    fn scale_factor_option_reaches_exporter() {
        let glb = triangle_glb();
        let scaled = convert_impl(&glb, "glb", "glb", Some(r#"{"scale_factor": 2}"#)).unwrap();
        let stl = convert_impl(&scaled, "glb", "stl", None).unwrap();
        let (_, max) = converter_stl::import_stl_bytes(&stl)
            .unwrap()
            .compute_bounding_box()
            .unwrap();
        assert!((max[1] - 4000.0).abs() < 1e-2, "max = {max:?}");
    }
}
