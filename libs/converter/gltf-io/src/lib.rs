//! Importación y exportación glTF/GLB.
//!
//! - Lectura: glTF 2.0 JSON y GLB binario → `Scene`
//! - Escritura: `Scene` → GLB binario o glTF + .bin, opcionalmente con Draco y reducción de triángulos
//!
//! Usa el crate `gltf` para lectura y genera GLB manualmente para escritura.

mod import;
mod export;
mod options;
mod texture_process;
mod geometry;
mod transforms;
mod draco;
#[cfg(feature = "simplify")]
mod simplify;

pub use import::{import_gltf, import_gltf_bytes, GltfImportError};
pub use export::{export_glb, export_glb_bytes, export_gltf, GlbExportError};
pub use options::{DracoOptions, GlbExportOptions, Simplification};
