//! Tipos de error para pinocchio-print3d

use thiserror::Error;

/// Errores de la librería print3d
#[derive(Error, Debug)]
pub enum Print3dError {
    /// La malla no está cerrada (no es watertight)
    #[error("la malla no está cerrada, no se puede calcular volumen")]
    MeshNotClosed,

    /// La malla está vacía
    #[error("la malla está vacía")]
    EmptyMesh,

    /// El plano de corte no intersecta la malla
    #[error("el plano de corte no intersecta la malla")]
    PlaneNoIntersection,

    /// Configuración inválida
    #[error("configuración inválida: {0}")]
    InvalidConfig(String),

    /// Error en operación de joint
    #[error("error en joint: {0}")]
    JointError(String),

    /// Error de geometría
    #[error("error de geometría: {0}")]
    GeometryError(String),

    /// Error de IO
    #[error("error de IO: {0}")]
    IoError(#[from] std::io::Error),
}

/// Tipo Result para operaciones de print3d
pub type Result<T> = std::result::Result<T, Print3dError>;
