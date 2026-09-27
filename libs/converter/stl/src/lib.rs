//! Importación y exportación STL.
//!
//! STL solo almacena geometría (triángulos + normales), sin materiales,
//! texturas, esqueletos ni animaciones.
//!
//! - Lectura: STL ASCII y binario → `Scene` (solo mesh)
//! - Escritura: `Scene` → STL binario

mod import;
mod export;

pub use import::{import_stl, import_stl_bytes, StlImportError};
pub use export::{export_stl, export_stl_bytes, StlExportError};
