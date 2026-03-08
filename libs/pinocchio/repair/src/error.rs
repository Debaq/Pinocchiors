//! Errores de reparación de mallas

use thiserror::Error;

/// Errores que pueden ocurrir durante la reparación
#[derive(Debug, Error)]
pub enum RepairError {
    /// La malla está vacía
    #[error("la malla está vacía")]
    EmptyMesh,

    /// La malla no es cerrada (requerido para algunas operaciones)
    #[error("la malla no es cerrada, tiene {0} aristas de borde")]
    OpenMesh(usize),

    /// El boundary loop está vacío o es inválido
    #[error("boundary loop inválido")]
    InvalidBoundaryLoop,

    /// No se pudo triangular el agujero
    #[error("fallo en triangulación: {0}")]
    TriangulationFailed(String),

    /// Índice de cara fuera de rango
    #[error("índice de cara {0} fuera de rango (máximo: {1})")]
    FaceIndexOutOfRange(usize, usize),

    /// Índice de vértice fuera de rango
    #[error("índice de vértice {0} fuera de rango (máximo: {1})")]
    VertexIndexOutOfRange(usize, usize),

    /// No se pudo determinar la orientación
    #[error("no se pudo determinar la orientación de las normales")]
    OrientationUndetermined,

    /// Polígono degenerado
    #[error("polígono degenerado con {0} vértices")]
    DegeneratePolygon(usize),

    /// Error de integridad en la malla
    #[error("error de integridad: {0}")]
    IntegrityError(String),
}

/// Resultado de una operación de reparación
pub type RepairResult<T> = Result<T, RepairError>;
