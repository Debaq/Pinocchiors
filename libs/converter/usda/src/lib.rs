//! Escritor USDA/USDZ puro en Rust.
//!
//! Genera archivos USD en formato texto (USDA) y paquetes USDZ
//! sin ninguna dependencia en OpenUSD ni TinyUSDZ.
//!
//! Pipeline:
//! ```text
//! Scene → USDA (texto) → opcionalmente empaquetado en USDZ (ZIP)
//! ```
//!
//! USDZ es un archivo ZIP sin compresión que contiene:
//! - Un archivo .usda (o .usdc) como escena principal
//! - Texturas PNG/JPEG referenciadas

pub(crate) mod writer;
pub(crate) mod materials;
pub(crate) mod textures;
pub(crate) mod skeleton;
mod packager;

pub use writer::{write_usda, UsdaWriteError, UsdaExportOptions, UsdaOutput, ProcessedTexture};
pub use packager::{write_usdz, write_usdz_bytes, UsdzPackageError};
pub use materials::texture_asset_path;
