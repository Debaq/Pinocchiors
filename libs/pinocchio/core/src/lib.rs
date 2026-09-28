//! Pinocchio Core - API principal para auto-rigging
//!
//! Este crate proporciona la API principal para realizar auto-rigging
//! de personajes 3D.
//!
//! # Ejemplo
//!
//! ```no_run
//! use pinocchio_core::{autorig, PinocchioConfig};
//! use pinocchio_core::mesh::load_obj;
//! use pinocchio_core::skeleton::HumanSkeleton;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mesh = load_obj("character.obj")?;
//! let skeleton = HumanSkeleton::new();
//! let result = autorig(&mesh, &skeleton, Some(PinocchioConfig::default()))?;
//! println!("{} huesos embebidos", result.bone_positions.len());
//! # Ok(())
//! # }
//! ```

mod config;
mod output;
mod error;
mod autorig;

pub use config::{PinocchioConfig, SkeletonFit};
pub use output::{PinocchioOutput, ProcessStats};
pub use error::PinocchioError;
pub use autorig::{autorig, autorig_with_progress, fit_to_mesh, transfer_weights, AutorigStage, SkeletonFitReport};

// Re-export sub-crates for convenience
pub use pinocchio_math as math;
pub use pinocchio_mesh as mesh;
pub use pinocchio_skeleton as skeleton;
