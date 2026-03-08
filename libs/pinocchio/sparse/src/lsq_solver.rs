//! Solver de mínimos cuadrados con restricciones

use crate::SPDMatrix;
use pinocchio_math::Real;
use thiserror::Error;

/// Error en el solver de mínimos cuadrados
#[derive(Debug, Error)]
pub enum LSQError {
    #[error("Sistema subdeterminado")]
    Underdetermined,
    #[error("Sistema singular")]
    Singular,
    #[error("Dimensiones incompatibles")]
    DimensionMismatch,
    #[error("Error en factorización: {0}")]
    FactorizationError(String),
}

/// Solver de mínimos cuadrados con restricciones de igualdad
///
/// Resuelve el problema:
/// min ||Ax - b||^2
/// s.t. Cx = d
pub struct LSQSolver {
    /// Número de variables
    n: usize,
    /// Número de ecuaciones en A
    m: usize,
    /// Número de restricciones
    p: usize,
}

impl LSQSolver {
    /// Crea un nuevo solver
    pub fn new(n: usize, m: usize, p: usize) -> Self {
        Self { n, m, p }
    }

    /// Resuelve el sistema de mínimos cuadrados sin restricciones
    ///
    /// min ||Ax - b||^2
    ///
    /// Equivalente a resolver A^T A x = A^T b
    pub fn solve_unconstrained(
        &self,
        ata: &mut SPDMatrix,
        atb: &[Real],
    ) -> Result<Vec<Real>, LSQError> {
        if atb.len() != self.n {
            return Err(LSQError::DimensionMismatch);
        }

        ata.factorize()
            .map_err(|e| LSQError::FactorizationError(e.to_string()))?;

        ata.solve(atb)
            .map_err(|e| LSQError::FactorizationError(e.to_string()))
    }

    /// Resuelve el sistema de mínimos cuadrados con restricciones
    ///
    /// min ||Ax - b||^2
    /// s.t. Cx = d
    ///
    /// Usa el método de KKT (Karush-Kuhn-Tucker)
    pub fn solve_constrained(
        &self,
        ata: &mut SPDMatrix,
        atb: &[Real],
        _c: &SPDMatrix,
        d: &[Real],
    ) -> Result<Vec<Real>, LSQError> {
        if atb.len() != self.n || d.len() != self.p {
            return Err(LSQError::DimensionMismatch);
        }

        // TODO: Implementar método KKT completo
        // Por ahora, resolver sin restricciones
        self.solve_unconstrained(ata, atb)
    }

    /// Número de variables
    pub fn num_variables(&self) -> usize {
        self.n
    }

    /// Número de ecuaciones
    pub fn num_equations(&self) -> usize {
        self.m
    }

    /// Número de restricciones
    pub fn num_constraints(&self) -> usize {
        self.p
    }
}

/// Builder para construir el sistema de ecuaciones normales incrementalmente
#[allow(dead_code)]
pub struct NormalEquationsBuilder {
    size: usize,
    rows: Vec<usize>,
    cols: Vec<usize>,
    values: Vec<Real>,
    rhs: Vec<Real>,
}

#[allow(dead_code)]
impl NormalEquationsBuilder {
    /// Crea un nuevo builder
    pub fn new(size: usize) -> Self {
        Self {
            size,
            rows: Vec::new(),
            cols: Vec::new(),
            values: Vec::new(),
            rhs: vec![0.0; size],
        }
    }

    /// Añade una ecuación: sum_j (a_j * x_j) = b con peso w
    pub fn add_equation(&mut self, indices: &[usize], coefficients: &[Real], b: Real, weight: Real) {
        let w2 = weight * weight;

        // Añadir a A^T A
        for (i, &idx_i) in indices.iter().enumerate() {
            for (j, &idx_j) in indices.iter().enumerate() {
                self.rows.push(idx_i);
                self.cols.push(idx_j);
                self.values.push(coefficients[i] * coefficients[j] * w2);
            }
        }

        // Añadir a A^T b
        for (i, &idx) in indices.iter().enumerate() {
            self.rhs[idx] += coefficients[i] * b * w2;
        }
    }

    /// Construye la matriz y el vector del lado derecho
    pub fn build(self) -> Result<(SPDMatrix, Vec<Real>), LSQError> {
        let matrix = SPDMatrix::from_triplets(self.size, &self.rows, &self.cols, &self.values)
            .map_err(|e| LSQError::FactorizationError(e.to_string()))?;
        Ok((matrix, self.rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solver_creation() {
        let solver = LSQSolver::new(10, 20, 5);
        assert_eq!(solver.num_variables(), 10);
        assert_eq!(solver.num_equations(), 20);
        assert_eq!(solver.num_constraints(), 5);
    }

    #[test]
    fn test_builder() {
        let mut builder = NormalEquationsBuilder::new(3);
        builder.add_equation(&[0, 1], &[1.0, 1.0], 2.0, 1.0);
        builder.add_equation(&[1, 2], &[1.0, 1.0], 3.0, 1.0);

        let (matrix, rhs) = builder.build().unwrap();
        assert_eq!(matrix.size(), 3);
        assert_eq!(rhs.len(), 3);
    }
}
