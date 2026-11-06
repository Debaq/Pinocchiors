//! Laminado FDM con el motor de OrcaSlicer (v2.4.2 congelado), y para resina (MSLA)
//! con un núcleo propio en Rust (`sla*`: capas, revisión, soportes, perfiles; formato en `ctb`).
//!
//! El motor (`pinocchio-slicer`, C++ sobre `libslic3r`) corre como proceso aparte:
//! si se cae o se cuelga, la app sigue viva, y cancelar es matar el proceso. Este
//! crate arma la configuración desde los perfiles embebidos, escribe las piezas,
//! lanza el motor, lee su progreso y convierte el gcode en datos para la vista previa.
//!
//! Coordenadas: milímetros, Z arriba, origen en la esquina de la cama (como Orca).

pub mod ctb;
mod engine;
mod error;
pub mod gcode;
pub mod presets;
mod settings;
pub mod sla;
pub mod sla_calibration;
pub mod sla_check;
pub mod sla_prepare;
pub mod sla_profiles;
pub mod sla_morph;
pub mod sla_supports;
mod stl;
pub mod thumbnail;

pub use engine::{find_engine, run, Event, RoleUse, SliceJob, Summary};
pub use error::SlicerError;
pub use gcode::{parse_gcode, Feature, Preview};
pub use presets::{index, preset, PresetIndex, PresetKind, PrinterEntry};
pub use settings::{build_config, SliceSettings};
pub use stl::write_binary_stl;
pub use thumbnail::{parse_color, render_thumbnail};

/// Versión de Orca de la que sale el motor y los perfiles.
pub const ORCA_VERSION: &str = "2.4.2";
