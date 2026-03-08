//! Importación y exportación glTF/GLB.
//!
//! - Lectura: glTF 2.0 JSON y GLB binario → `Scene`
//! - Escritura: `Scene` → GLB binario
//!
//! Usa el crate `gltf` para lectura y genera GLB manualmente para escritura.

mod import;
mod export;
mod options;
mod texture_process;
mod geometry;
mod transforms;

pub use import::{import_gltf, import_gltf_bytes, GltfImportError};
pub use export::{export_glb, export_glb_bytes, GlbExportError};
pub use options::GlbExportOptions;
