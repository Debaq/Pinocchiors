//! Pinocchio Attachment - Heat diffusion para skinning weights
//!
//! Este crate proporciona:
//! - VisibilityTester: ray casting para visibilidad
//! - Heat diffusion: cálculo de pesos de skinning
//! - Attachment: deformación de la malla
//! - SymmetryMap: detección y aplicación de simetría
//! - attach_detached_parts: piezas sueltas sin huesos (colmillos, ojos) rígidas

pub mod visibility;
pub mod heat_diffusion;
pub mod attachment;
pub mod symmetry;
pub mod detached;

pub use visibility::VisibilityTester;
pub use heat_diffusion::{HeatDiffusion, HeatDiffusionError};
pub use attachment::{dominant_influences, Attachment};
pub use symmetry::{SymmetryMap, SymmetryAxis, SymmetryPair};
pub use detached::attach_detached_parts;
