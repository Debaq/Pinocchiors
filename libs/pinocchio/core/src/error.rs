//! Errores del proceso de auto-rigging

use thiserror::Error;

/// Error durante el proceso de auto-rigging
#[derive(Debug, Error)]
pub enum PinocchioError {
    /// Error de malla
    #[error("Error en la malla: {0}")]
    MeshError(String),

    /// Error de esqueleto
    #[error("Error en el esqueleto: {0}")]
    SkeletonError(String),

    /// Error en el embedding
    #[error("Error en el embedding: {0}")]
    EmbeddingError(#[from] pinocchio_embedding::EmbeddingError),

    /// Error en heat diffusion
    #[error("Error en heat diffusion: {0}")]
    HeatDiffusionError(#[from] pinocchio_attachment::HeatDiffusionError),

    /// Malla vacía
    #[error("La malla está vacía")]
    EmptyMesh,

    /// Esqueleto vacío
    #[error("El esqueleto está vacío")]
    EmptySkeleton,

    /// Verificación de integridad fallida
    #[error("La malla no pasó la verificación de integridad: {0}")]
    IntegrityCheckFailed(String),

    /// Error interno
    #[error("Error interno: {0}")]
    InternalError(String),
}
