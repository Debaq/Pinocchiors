//! Errores de reparación de mallas

use thiserror::Error;

/// Errores que pueden ocurrir durante la reparación
#[derive(Debug, Error)]
pub enum RepairError {
    /// La malla no tiene caras
    #[error("la malla está vacía")]
    EmptyMesh,
}

/// Resultado de una operación de reparación
pub type RepairResult<T> = Result<T, RepairError>;
