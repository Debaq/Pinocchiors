//! WASM bindings para conversión de formatos 3D.
//!
//! Provee funciones accesibles desde JavaScript para convertir
//! entre formatos 3D en el navegador.

use wasm_bindgen::prelude::*;

/// Convierte bytes de un formato 3D a otro.
///
/// # Argumentos
/// - `input_bytes`: bytes del archivo de entrada
/// - `input_format`: formato de entrada ("glb", "stl")
/// - `output_format`: formato de salida ("glb", "usda", "usdz", "stl")
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
    let scene = import_scene(input_bytes, input_format)?;
    let options = parse_options(options_json)?;
    export_scene(&scene, output_format, &options)
}

/// Retorna los formatos soportados como JSON.
#[wasm_bindgen]
pub fn supported_formats() -> String {
    serde_json::json!({
        "import": ["glb", "stl"],
        "export": ["glb", "usda", "usdz", "stl"]
    })
    .to_string()
}

/// Importa un archivo y retorna metadatos de la escena como JSON.
#[wasm_bindgen]
pub fn import_to_json(input_bytes: &[u8], format: &str) -> Result<String, JsError> {
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

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn import_scene(
    data: &[u8],
    format: &str,
) -> Result<converter_scene::Scene, JsError> {
    match format.to_ascii_lowercase().as_str() {
        "glb" | "gltf" => converter_gltf_io::import_gltf_bytes(data)
            .map_err(|e| JsError::new(&e.to_string())),
        "stl" => {
            // STL import desde bytes: escribir a cursor
            let cursor = std::io::Cursor::new(data);
            import_stl_from_reader(cursor)
                .map_err(|e| JsError::new(&e.to_string()))
        }
        _ => Err(JsError::new(&format!("formato de importación no soportado: {}", format))),
    }
}

fn import_stl_from_reader(_reader: impl std::io::Read + std::io::Seek) -> Result<converter_scene::Scene, String> {
    // STL crate espera un path, pero para WASM necesitamos bytes.
    // Por ahora retornamos error — el import desde bytes requiere una API adicional en converter-stl.
    Err("STL import desde bytes no implementado aún".into())
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
}

fn parse_options(json: Option<String>) -> Result<WasmOptions, JsError> {
    let Some(json) = json else {
        return Ok(WasmOptions {
            fps: 24.0,
            export_animations: true,
            ..WasmOptions::default()
        });
    };

    let v: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| JsError::new(&e.to_string()))?;

    Ok(WasmOptions {
        scale_factor: v.get("scale_factor").and_then(|v| v.as_f64()),
        max_texture_size: v.get("max_texture_size").and_then(|v| v.as_u64()).map(|v| v as u32),
        arkit_compatible: v.get("arkit_compatible").and_then(|v| v.as_bool()).unwrap_or(false),
        fps: v.get("fps").and_then(|v| v.as_f64()).unwrap_or(24.0),
        export_animations: v.get("export_animations").and_then(|v| v.as_bool()).unwrap_or(true),
        texture_quality: v.get("texture_quality").and_then(|v| v.as_u64()).map(|v| v as u8),
        optimize_geometry: v.get("optimize_geometry").and_then(|v| v.as_bool()).unwrap_or(false),
        generate_normals: v.get("generate_normals").and_then(|v| v.as_bool()).unwrap_or(false),
        flatten_transforms: v.get("flatten_transforms").and_then(|v| v.as_bool()).unwrap_or(false),
        strip_unused: v.get("strip_unused").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

fn export_scene(
    scene: &converter_scene::Scene,
    format: &str,
    options: &WasmOptions,
) -> Result<Vec<u8>, JsError> {
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
    };

    match format.to_ascii_lowercase().as_str() {
        "glb" | "gltf" => converter_gltf_io::export_glb_bytes(scene, &glb_opts)
            .map_err(|e| JsError::new(&e.to_string())),
        "usda" => {
            let output = converter_usda::write_usda(scene, &usda_opts)
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(output.usda.into_bytes())
        }
        "usdz" => converter_usda::write_usdz_bytes(scene, &usda_opts)
            .map_err(|e| JsError::new(&e.to_string())),
        _ => Err(JsError::new(&format!(
            "formato de exportación no soportado: {}",
            format
        ))),
    }
}
