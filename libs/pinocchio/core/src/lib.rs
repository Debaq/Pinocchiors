//! Pinocchio Core - API principal para auto-rigging
//!
//! Este crate proporciona la API principal para realizar auto-rigging
//! de personajes 3D.
//!
//! # Ejemplo
//!
//! ```ignore
//! use pinocchio_core::{autorig, PinocchioConfig};
//! use pinocchio_mesh::Mesh;
//! use pinocchio_skeleton::HumanSkeleton;
//!
//! let mesh = Mesh::load_obj("character.obj")?;
//! let skeleton = HumanSkeleton::new();
//! let result = autorig(&mesh, &skeleton, None)?;
//! ```

mod config;
mod output;
mod error;
mod autorig;

pub use config::{PinocchioConfig, SkeletonFit};
pub use output::PinocchioOutput;
pub use error::PinocchioError;
pub use autorig::{autorig, autorig_with_progress, transfer_weights, AutorigStage};

// Re-export sub-crates for convenience
pub use pinocchio_math as math;
pub use pinocchio_mesh as mesh;
pub use pinocchio_skeleton as skeleton;
