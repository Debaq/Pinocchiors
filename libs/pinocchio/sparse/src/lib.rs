//! Pinocchio Sparse - Operaciones de matrices sparse y descomposición Cholesky
//!
//! Este crate proporciona:
//! - Matrices sparse simétricas positivas definidas (SPD)
//! - Descomposición Cholesky LLT
//! - Solver de mínimos cuadrados con restricciones

mod factor;
mod spd_matrix;
mod lsq_solver;
#[cfg(test)] mod perf_tests;

pub use factor::SpdFactor;
pub use spd_matrix::{SPDMatrix, SPDMatrixError};
pub use lsq_solver::LSQSolver;

pub use sprs;
