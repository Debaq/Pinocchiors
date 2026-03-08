//! Fachada unificada para conversión de formatos 3D.
//!
//! `converter-core` es la **única dependencia necesaria** para consumidores.
//! Re-exporta todos los tipos de `converter-scene` y provee una API simple:
//!
//! ```text
//! // Conversión directa archivo → archivo
//! convert("input.glb", "output.usdz", &options)?;
//!
//! // Import → manipular → export
//! let scene = import("model.glb")?;
//! export(&scene, "output.usdz", &options)?;
//!
//! // Operaciones en memoria (WASM, pipelines)
//! let scene = import_bytes(data, Format::Gltf)?;
//! let bytes = export_bytes(&scene, Format::Usdz, &options)?;
//! ```

mod format;
mod options;
mod convert;

pub use format::Format;
pub use options::ConvertOptions;
pub use convert::{convert, import, export, import_bytes, export_bytes, ConvertError};

// Re-export completo de converter-scene para que consumidores no necesiten
// depender de converter-scene directamente.
pub use converter_scene::{
    // Escena y grafo
    Scene, Node, SceneError,
    // Geometría
    Mesh, Primitive, VertexAttribute, IndexData,
    // Materiales
    Material, AlphaMode,
    // Texturas
    Texture, TextureFormat, TextureRef,
    // Esqueletos
    Skeleton, Joint,
    // Animaciones
    Animation, Channel, Interpolation, KeyframeTimes, KeyframeValues,
    // Transforms
    Transform,
};
