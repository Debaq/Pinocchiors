//! Solver de restricciones geométricas 2D para sketches CAD, más ajuste de
//! primitivas (plano, esfera, cilindro) y segmentación de mallas por normales,
//! base del flujo escaneo → CAD.
//!
//! Portado de `cad-blender` (crate `cadblender_solver`) sin la capa de Python.

// Código numérico portado tal cual: índices explícitos a propósito.
#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod bspline;
pub mod constraint;
pub mod diagnostics;
pub mod error;
pub mod fitting;
pub mod mesh_simplify;
pub mod segmentation;
pub mod solver;
pub mod system;
pub mod types;

pub use bspline::BSpline;
pub use constraint::{BSplineRef, Constraint, CurveEnd, CurvePart, closest_on_spline};
pub use diagnostics::{DiagnosticResult, DiagnosticStatus, diagnose};
pub use error::{Result, SolverError};
pub use fitting::{FitResult, PrimitiveParams, PrimitiveType};
pub use solver::{SolverParams, solve, solve_drag};
pub use system::ConstraintSystem;
pub use types::{Point2, SolveResult, SolveStatus};
