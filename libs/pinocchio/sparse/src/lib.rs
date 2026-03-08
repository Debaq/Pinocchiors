//! Pinocchio Sparse - Operaciones de matrices sparse y descomposición Cholesky
//!
//! Este crate proporciona:
//! - Matrices sparse simétricas positivas definidas (SPD)
//! - Descomposición Cholesky LLT
//! - Solver de mínimos cuadrados con restricciones

mod spd_matrix;
mod lsq_solver;

pub use spd_matrix::SPDMatrix;
pub use lsq_solver::LSQSolver;

pub use sprs;
