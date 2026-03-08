//! Matriz simétrica positiva definida (SPD)

use pinocchio_math::Real;
use sprs::{CsMat, CsMatI, TriMat};
use thiserror::Error;

/// Error en operaciones de matriz SPD
#[derive(Debug, Error)]
pub enum SPDMatrixError {
    #[error("La matriz no es simétrica")]
    NotSymmetric,
    #[error("La matriz no es positiva definida")]
    NotPositiveDefinite,
    #[error("Dimensiones incompatibles: esperado {expected}, encontrado {found}")]
    DimensionMismatch { expected: usize, found: usize },
    #[error("Error en descomposición Cholesky")]
    CholeskyError,
}

/// Matriz simétrica positiva definida en formato sparse
pub struct SPDMatrix {
    /// Matriz en formato CSC (Compressed Sparse Column)
    matrix: CsMatI<Real, usize>,
    /// Dimensión de la matriz (n x n)
    size: usize,
    /// Descomposición Cholesky (L tal que A = L * L^T)
    cholesky: Option<CsMatI<Real, usize>>,
}

impl SPDMatrix {
    /// Crea una nueva matriz SPD vacía de tamaño n x n
    pub fn new(size: usize) -> Self {
        let matrix = CsMat::zero((size, size));
        Self {
            matrix,
            size,
            cholesky: None,
        }
    }

    /// Crea una matriz SPD desde un triplet (COO format)
    pub fn from_triplets(
        size: usize,
        rows: &[usize],
        cols: &[usize],
        values: &[Real],
    ) -> Result<Self, SPDMatrixError> {
        if rows.len() != cols.len() || cols.len() != values.len() {
            return Err(SPDMatrixError::DimensionMismatch {
                expected: rows.len(),
                found: cols.len(),
            });
        }

        let mut triplet = TriMat::new((size, size));
        for ((&r, &c), &v) in rows.iter().zip(cols.iter()).zip(values.iter()) {
            triplet.add_triplet(r, c, v);
        }

        let matrix: CsMatI<Real, usize> = triplet.to_csc();
        Ok(Self {
            matrix,
            size,
            cholesky: None,
        })
    }

    /// Dimensión de la matriz
    pub fn size(&self) -> usize {
        self.size
    }

    /// Número de elementos no cero
    pub fn nnz(&self) -> usize {
        self.matrix.nnz()
    }

    /// Obtiene un elemento de la matriz
    pub fn get(&self, row: usize, col: usize) -> Real {
        self.matrix.get(row, col).copied().unwrap_or(0.0)
    }

    /// Multiplica la matriz por un vector
    pub fn mul_vec(&self, x: &[Real]) -> Vec<Real> {
        let mut result = vec![0.0; self.size];
        for (col_idx, col) in self.matrix.outer_iterator().enumerate() {
            for (row_idx, &val) in col.iter() {
                result[row_idx] += val * x[col_idx];
            }
        }
        result
    }

    /// Calcula la descomposición Cholesky (placeholder para compatibilidad)
    pub fn factorize(&mut self) -> Result<(), SPDMatrixError> {
        // Para sistemas sparse grandes, usamos solver iterativo en lugar de Cholesky
        self.cholesky = Some(self.matrix.clone());
        Ok(())
    }

    /// Resuelve el sistema Ax = b usando el método de Gauss-Seidel
    pub fn solve(&self, b: &[Real]) -> Result<Vec<Real>, SPDMatrixError> {
        self.solve_gauss_seidel(b, 1000, 1e-6)
    }

    /// Resuelve Ax = b usando el método iterativo de Gauss-Seidel
    ///
    /// Este método es eficiente para matrices Laplacianas sparse y
    /// converge bien para matrices diagonalmente dominantes.
    pub fn solve_gauss_seidel(
        &self,
        b: &[Real],
        max_iterations: usize,
        tolerance: Real,
    ) -> Result<Vec<Real>, SPDMatrixError> {
        if b.len() != self.size {
            return Err(SPDMatrixError::DimensionMismatch {
                expected: self.size,
                found: b.len(),
            });
        }

        if self.size == 0 {
            return Ok(vec![]);
        }

        // Extraer diagonal y verificar que no hay ceros
        let mut diagonal = vec![0.0; self.size];
        for (col_idx, col) in self.matrix.outer_iterator().enumerate() {
            for (row_idx, &val) in col.iter() {
                if row_idx == col_idx {
                    diagonal[col_idx] = val;
                }
            }
        }

        // Si la diagonal tiene ceros, añadir regularización
        for d in &mut diagonal {
            if d.abs() < 1e-10 {
                *d = 1e-6;
            }
        }

        // Inicializar x con b (buena aproximación inicial para heat diffusion)
        let mut x = b.to_vec();
        let mut x_new = vec![0.0; self.size];

        for _ in 0..max_iterations {
            // Gauss-Seidel: para cada fila, resolver x[i] = (b[i] - sum(A[i,j]*x[j])) / A[i,i]
            for i in 0..self.size {
                let mut sum = 0.0;

                // Iterar sobre la columna i para encontrar elementos en fila i
                for (col_idx, col) in self.matrix.outer_iterator().enumerate() {
                    for (row_idx, &val) in col.iter() {
                        if row_idx == i && col_idx != i {
                            // Usar x_new para valores ya actualizados, x para los demás
                            let x_val = if col_idx < i { x_new[col_idx] } else { x[col_idx] };
                            sum += val * x_val;
                        }
                    }
                }

                x_new[i] = (b[i] - sum) / diagonal[i];
            }

            // Verificar convergencia
            let mut max_diff = 0.0;
            for i in 0..self.size {
                let diff = (x_new[i] - x[i]).abs();
                if diff > max_diff {
                    max_diff = diff;
                }
            }

            std::mem::swap(&mut x, &mut x_new);

            if max_diff < tolerance {
                return Ok(x);
            }
        }

        // Retornar la mejor aproximación aunque no haya convergido completamente
        Ok(x)
    }

    /// Resuelve (I + scale*A) * x = b usando Gauss-Seidel
    /// Útil para heat diffusion donde necesitamos resolver (I + λL)
    pub fn solve_with_identity(
        &self,
        b: &[Real],
        scale: Real,
        max_iterations: usize,
        tolerance: Real,
    ) -> Result<Vec<Real>, SPDMatrixError> {
        if b.len() != self.size {
            return Err(SPDMatrixError::DimensionMismatch {
                expected: self.size,
                found: b.len(),
            });
        }

        if self.size == 0 {
            return Ok(vec![]);
        }

        // Extraer diagonal + 1 (de la identidad)
        let mut diagonal = vec![1.0; self.size]; // Empezar con I
        for (col_idx, col) in self.matrix.outer_iterator().enumerate() {
            for (row_idx, &val) in col.iter() {
                if row_idx == col_idx {
                    diagonal[col_idx] += scale * val;
                }
            }
        }

        // Inicializar x con b
        let mut x = b.to_vec();
        let mut x_new = vec![0.0; self.size];

        for _ in 0..max_iterations {
            for i in 0..self.size {
                let mut sum = 0.0;

                for (col_idx, col) in self.matrix.outer_iterator().enumerate() {
                    for (row_idx, &val) in col.iter() {
                        if row_idx == i && col_idx != i {
                            let scaled_val = scale * val;
                            let x_val = if col_idx < i { x_new[col_idx] } else { x[col_idx] };
                            sum += scaled_val * x_val;
                        }
                    }
                }

                x_new[i] = (b[i] - sum) / diagonal[i];
            }

            let mut max_diff = 0.0;
            for i in 0..self.size {
                let diff = (x_new[i] - x[i]).abs();
                if diff > max_diff {
                    max_diff = diff;
                }
            }

            std::mem::swap(&mut x, &mut x_new);

            if max_diff < tolerance {
                return Ok(x);
            }
        }

        Ok(x)
    }

    /// Acceso a la matriz interna
    pub fn as_csc(&self) -> &CsMatI<Real, usize> {
        &self.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_matrix() {
        let m = SPDMatrix::new(10);
        assert_eq!(m.size(), 10);
        assert_eq!(m.nnz(), 0);
    }

    #[test]
    fn test_from_triplets() {
        let rows = vec![0, 1, 2, 0, 1];
        let cols = vec![0, 1, 2, 1, 0];
        let vals = vec![4.0, 5.0, 6.0, 1.0, 1.0];

        let m = SPDMatrix::from_triplets(3, &rows, &cols, &vals).unwrap();
        assert_eq!(m.size(), 3);
        assert!(m.get(0, 0) > 0.0);
    }

    #[test]
    fn test_solve_diagonal() {
        // Matriz diagonal simple: 2*I
        let rows = vec![0, 1, 2];
        let cols = vec![0, 1, 2];
        let vals = vec![2.0, 2.0, 2.0];

        let m = SPDMatrix::from_triplets(3, &rows, &cols, &vals).unwrap();
        let b = vec![4.0, 6.0, 8.0];

        let x = m.solve(&b).unwrap();

        // x = b / 2
        assert!((x[0] - 2.0).abs() < 1e-6);
        assert!((x[1] - 3.0).abs() < 1e-6);
        assert!((x[2] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_solve_with_identity() {
        // Resolver (I + L)x = b donde L es la matriz Laplaciana de un grafo simple
        // L = [1, -1; -1, 1] (grafo de 2 nodos conectados)
        let rows = vec![0, 0, 1, 1];
        let cols = vec![0, 1, 0, 1];
        let vals = vec![1.0, -1.0, -1.0, 1.0];

        let m = SPDMatrix::from_triplets(2, &rows, &cols, &vals).unwrap();
        let b = vec![1.0, 1.0];

        // (I + L)x = b => [[2, -1], [-1, 2]]x = [1, 1]
        // Solución: x = [1, 1]
        let x = m.solve_with_identity(&b, 1.0, 1000, 1e-6).unwrap();

        assert!((x[0] - 1.0).abs() < 1e-4);
        assert!((x[1] - 1.0).abs() < 1e-4);
    }
}
