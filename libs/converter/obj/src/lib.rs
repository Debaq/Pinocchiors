//! Importación y exportación OBJ/MTL.
//!
//! - Lectura: OBJ + MTL → `Scene` (todos los mapas del MTL, PBR incluido)
//! - Texturas sueltas: reconocer los mapas por el nombre ([`maps`])
//! - Escritura: `Scene` → OBJ + MTL + texturas a disco

mod import;
mod export;
pub mod maps;

pub use import::{import_obj, import_obj_with_mtl, ObjImportError};
pub use export::{export_obj, ObjExportError};
