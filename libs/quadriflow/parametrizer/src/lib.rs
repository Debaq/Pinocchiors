//! # quadriflow-parametrizer
//!
//! Surface parametrization with integer constraints.
//!
//! Converts the continuous position field into integer UV coordinates
//! that define the quad mesh topology.
//!
//! ## Components
//!
//! - **Seamless**: Transition functions across seams (`SeamData`, `SeamTransition`)
//! - **Integer**: Snapping continuous UVs to integers (`compute_integer_parametrization`)
//! - **SAT**: T-junction removal via constraint solving (`remove_tjunctions`)

pub mod integer;
pub mod sat;
pub mod seamless;

pub use integer::{compute_integer_parametrization, IntegerConfig, IntegerStats};
pub use sat::{detect_tjunctions, remove_tjunctions, SatConfig, TJunction, TJunctionStats};
pub use seamless::{SeamData, SeamTransition};

use thiserror::Error;

/// Errors during parametrization.
#[derive(Error, Debug)]
pub enum ParametrizationError {
    #[error("Mesh has inconsistent topology")]
    InconsistentTopology,
    #[error("Failed to solve integer problem: {0}")]
    IntegerSolveError(String),
    #[error("Parametrization has too many singularities")]
    TooManySingularities,
}

/// Integer parametrization result.
#[derive(Debug, Clone)]
pub struct IntegerParametrization {
    /// Integer U coordinate per vertex
    pub u: Vec<i32>,
    /// Integer V coordinate per vertex
    pub v: Vec<i32>,
    /// Rotation index per face (0, 1, 2, or 3 for 0°, 90°, 180°, 270°)
    pub rotations: Vec<u8>,
}

impl IntegerParametrization {
    /// Create a new empty parametrization.
    pub fn new() -> Self {
        Self {
            u: Vec::new(),
            v: Vec::new(),
            rotations: Vec::new(),
        }
    }
}

impl Default for IntegerParametrization {
    fn default() -> Self {
        Self::new()
    }
}
