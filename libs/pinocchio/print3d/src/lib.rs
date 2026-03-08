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
//! - **Joints**: Generar insertos para ensamblaje (dowels, dovetails, etc.)
//!
//! ## Ejemplo
//!
//! ```rust,ignore
//! use pinocchio_print3d::{analyze, subdivide, SubdivideConfig};
//!
//! // Analizar modelo
//! let analysis = analyze(&mesh);
//! println!("Volumen: {} mm³", analysis.volume);
//! println!("Dimensiones: {:?}", analysis.bounding_box.dimensions());
//!
//! // Subdividir para impresora con volumen 220x220x250
//! let config = SubdivideConfig {
//!     build_volume: Vector3::new(220.0, 220.0, 250.0),
//!     ..Default::default()
//! };
//! let pieces = subdivide(&mesh, &config);
//! println!("Dividido en {} piezas", pieces.len());
//! ```

mod analysis;
mod config;
mod error;
mod joints;
mod labeling;
mod slicer;
mod transform;

pub use analysis::*;
pub use config::*;
pub use error::*;
pub use joints::*;
pub use labeling::*;
pub use slicer::*;
pub use transform::*;
