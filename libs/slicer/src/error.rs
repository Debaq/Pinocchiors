use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlicerError {
    #[error("No se encontró el motor de laminado (pinocchio-slicer)")]
    EngineMissing,
    #[error("No existe el perfil «{0}»")]
    UnknownPreset(String),
    #[error("No hay piezas que laminar")]
    Empty,
    #[error("Laminado cancelado")]
    Cancelled,
    /// Sin soportes no se puede imprimir bien (interno: se vuelve a laminar con soportes)
    #[error("La pieza necesita soportes: {0}")]
    NeedsSupport(String),
    #[error("El motor de laminado falló: {0}")]
    Engine(String),
    #[error("La pieza no cabe en la impresora: {0}")]
    OutOfBed(String),
    #[error("Ajustes inválidos: {0}")]
    Settings(String),
    #[error("Formato inválido: {0}")]
    Format(String),
    #[error("Error de archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
}
