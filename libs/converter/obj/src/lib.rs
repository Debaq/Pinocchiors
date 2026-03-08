//! Importación y exportación OBJ/MTL.
//!
//! - Lectura: OBJ + MTL → `Scene`
//! - Escritura: `Scene` → OBJ + MTL + texturas a disco

mod import;
mod export;

pub use import::{import_obj, ObjImportError};
pub use export::{export_obj, ObjExportError};
