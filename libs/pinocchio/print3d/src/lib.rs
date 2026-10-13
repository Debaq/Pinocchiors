//! # pinocchio-print3d
//!
//! Librería para preparación de modelos 3D para impresión.
//!
//! ## Funcionalidades
//!
//! - **Análisis**: Volumen, área superficial, centro de masa, bounding box
//! - **Transformaciones**: Escalar, orientar, centrar
//! - **Corte**: Dividir modelos con planos, subdividir para impresoras pequeñas
//! - **Etiquetado**: Numerar piezas, tracking de vecinos
//! - **Verificación**: Imprimibilidad (topología, voladizos, paredes finas) y perfiles de impresora
//! - **Joints**: Generar insertos para ensamblaje (dowels, dovetails, etc.)
//!
//! ## Ejemplo
//!
//! ```no_run
//! use pinocchio_print3d::{analyze, subdivide, SubdivideConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mesh = pinocchio_mesh::load_obj("modelo.obj")?;
//!
//! // Analizar modelo
//! let analysis = analyze(&mesh)?;
//! println!("Volumen: {} mm³", analysis.volume);
//! println!("Dimensiones: {:?}", analysis.bounding_box.dimensions());
//!
//! // Subdividir para impresora con volumen 220x220x250
//! let config = SubdivideConfig {
//!     build_volume: [220.0, 220.0, 250.0],
//!     ..Default::default()
//! };
//! let pieces = subdivide(&mesh, &config)?;
//! println!("Dividido en {} piezas", pieces.len());
//! # Ok(())
//! # }
//! ```

mod analysis;
mod capping;
mod config;
mod contact;
mod error;
mod fab;
mod geom;
mod groove;
mod hollow;
mod joints;
mod labeling;
mod nesting;
mod orientation;
mod planar_joints;
mod printability;
mod profile;
mod slicer;
mod transform;

pub use analysis::*;
pub use config::*;
pub use contact::*;
pub use error::*;
pub use fab::*;
pub use hollow::*;
pub use joints::*;
pub use labeling::*;
pub use nesting::*;
pub use orientation::*;
pub use planar_joints::*;
pub use printability::*;
pub use profile::*;
pub use slicer::*;
pub use transform::*;
