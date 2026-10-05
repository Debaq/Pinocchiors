use thiserror::Error;

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("El solver no convergió después de {max_iter} iteraciones (residuo: {residual:.2e})")]
    NotConverged { max_iter: usize, residual: f64 },

    #[error("Sistema sobre-restringido: {0} ecuaciones para {1} variables")]
    OverConstrained(usize, usize),

    #[error("Jacobiano singular en iteración {0}")]
    SingularJacobian(usize),

    #[error("Entidad no encontrada: {0}")]
    EntityNotFound(String),

    #[error("Constraint inválido: {0}")]
    InvalidConstraint(String),
}

pub type Result<T> = std::result::Result<T, SolverError>;
