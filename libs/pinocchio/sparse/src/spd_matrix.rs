//! Matriz simétrica positiva definida (SPD)

use pinocchio_math::Real;
use rayon::prelude::*;
use sprs::{CsMat, CsMatI};
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
        let entries = rows.iter().zip(cols).zip(values).map(|((&r, &c), &v)| (r, c, v)).collect();
        Self::from_entries(size, entries)
    }

    /// Crea la matriz desde entradas `(fila, columna, valor)`; las repetidas se suman.
    ///
    /// Ordena en paralelo (por columna y fila, de forma estable: las repetidas se
    /// suman siempre en el mismo orden, con cualquier número de hilos).
    pub fn from_entries(size: usize, mut entries: Vec<(usize, usize, Real)>) -> Result<Self, SPDMatrixError> {
        if size >= u32::MAX as usize {
            return Err(SPDMatrixError::DimensionMismatch { expected: u32::MAX as usize, found: size });
        }
        if let Some(&(r, c, _)) = entries.iter().find(|e| e.0 >= size || e.1 >= size) {
            return Err(SPDMatrixError::DimensionMismatch { expected: size, found: r.max(c) });
        }
        entries.par_sort_by_key(|e| ((e.1 as u64) << 32) | e.0 as u64);

        let mut indptr = vec![0usize; size + 1];
        let mut indices = Vec::with_capacity(entries.len());
        let mut data: Vec<Real> = Vec::with_capacity(entries.len());
        let mut last = None;
        for (r, c, v) in entries {
            if last == Some((r, c)) {
                if let Some(d) = data.last_mut() {
                    *d += v;
                }
                continue;
            }
            last = Some((r, c));
            indices.push(r);
            data.push(v);
            indptr[c + 1] += 1;
        }
        for i in 0..size {
            indptr[i + 1] += indptr[i];
        }
        Ok(Self {
            matrix: CsMatI::new_csc((size, size), indptr, indices, data),
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

                // La matriz es simétrica: la columna i contiene los elementos de la fila i
                if let Some(col) = self.matrix.outer_view(i) {
                    for (j, &val) in col.iter() {
                        if j != i {
                            // Usar x_new para valores ya actualizados, x para los demás
                            let x_val = if j < i { x_new[j] } else { x[j] };
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

                if let Some(col) = self.matrix.outer_view(i) {
                    for (j, &val) in col.iter() {
                        if j != i {
                            let x_val = if j < i { x_new[j] } else { x[j] };
                            sum += scale * val * x_val;
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

    /// Devuelve `A + diag(d)`
    pub fn add_diagonal(&self, d: &[Real]) -> Result<Self, SPDMatrixError> {
        if d.len() != self.size {
            return Err(SPDMatrixError::DimensionMismatch {
                expected: self.size,
                found: d.len(),
            });
        }
        // Las columnas ya están ordenadas: se suma a la entrada diagonal o se inserta, sin reordenar
        let n = self.size;
        let indptr = self.matrix.indptr();
        let ptr = indptr.raw_storage();
        let (idx, dat) = (self.matrix.indices(), self.matrix.data());
        let mut new_ptr = Vec::with_capacity(n + 1);
        let mut new_idx = Vec::with_capacity(idx.len() + n);
        let mut new_dat: Vec<Real> = Vec::with_capacity(idx.len() + n);
        new_ptr.push(0);
        for c in 0..n {
            let (lo, hi) = (ptr[c], ptr[c + 1]);
            let split = lo + idx[lo..hi].partition_point(|&r| r < c);
            new_idx.extend_from_slice(&idx[lo..split]);
            new_dat.extend_from_slice(&dat[lo..split]);
            let on_diagonal = split < hi && idx[split] == c;
            new_idx.push(c);
            new_dat.push(d[c] + if on_diagonal { dat[split] } else { 0.0 });
            let rest = if on_diagonal { split + 1 } else { split };
            new_idx.extend_from_slice(&idx[rest..hi]);
            new_dat.extend_from_slice(&dat[rest..hi]);
            new_ptr.push(new_idx.len());
        }
        Ok(Self {
            matrix: CsMatI::new_csc((n, n), new_ptr, new_idx, new_dat),
            size: n,
            cholesky: None,
        })
    }

    /// Resuelve `Ax = b` con gradiente conjugado precondicionado (Jacobi).
    ///
    /// Converge cuando `‖r‖ ≤ tolerance · ‖b‖`. Cada iteración cuesta O(nnz).
    /// Devuelve `NotPositiveDefinite` si detecta curvatura no positiva.
    pub fn solve_cg(
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
        let n = self.size;
        let dot = |a: &[Real], b: &[Real]| a.iter().zip(b).map(|(x, y)| x * y).sum::<Real>();

        let b_norm = dot(b, b).sqrt();
        if b_norm == 0.0 {
            return Ok(vec![0.0; n]);
        }

        // Precondicionador de Jacobi
        let mut inv_diag = vec![1.0; n];
        for (i, col) in self.matrix.outer_iterator().enumerate() {
            for (j, &val) in col.iter() {
                if i == j && val.abs() > 1e-300 {
                    inv_diag[i] = 1.0 / val;
                }
            }
        }

        let mut x = vec![0.0; n];
        let mut r = b.to_vec();
        let mut z: Vec<Real> = r.iter().zip(&inv_diag).map(|(r, d)| r * d).collect();
        let mut p = z.clone();
        let mut rz = dot(&r, &z);

        for _ in 0..max_iterations {
            let ap = self.mul_vec(&p);
            let pap = dot(&p, &ap);
            if pap <= 0.0 {
                return Err(SPDMatrixError::NotPositiveDefinite);
            }
            let alpha = rz / pap;
            for i in 0..n {
                x[i] += alpha * p[i];
                r[i] -= alpha * ap[i];
            }
            if dot(&r, &r).sqrt() <= tolerance * b_norm {
                return Ok(x);
            }
            for i in 0..n {
                z[i] = r[i] * inv_diag[i];
            }
            let rz_new = dot(&r, &z);
            let beta = rz_new / rz;
            rz = rz_new;
            for i in 0..n {
                p[i] = z[i] + beta * p[i];
            }
        }

        Ok(x)
    }

    /// Factoriza la matriz (Cholesky disperso) para resolver muchos lados derechos
    pub fn cholesky_factor(&self) -> Result<crate::factor::SpdFactor, SPDMatrixError> {
        crate::factor::SpdFactor::new(&self.matrix)
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

    #[test]
    fn test_solve_cg_laplacian_plus_diagonal() {
        // Laplaciano 1D (cadena de n nodos) + diagonal positiva: el sistema de bone heat
        let n = 200;
        let (mut rows, mut cols, mut vals) = (vec![], vec![], vec![]);
        for i in 0..n - 1 {
            for (r, c, v) in [(i, i, 1.0), (i + 1, i + 1, 1.0), (i, i + 1, -1.0), (i + 1, i, -1.0)] {
                rows.push(r);
                cols.push(c);
                vals.push(v);
            }
        }
        let lap = SPDMatrix::from_triplets(n, &rows, &cols, &vals).unwrap();
        let h: Vec<Real> = (0..n).map(|i| if i % 50 == 0 { 2.0 } else { 0.0 }).collect();
        let a = lap.add_diagonal(&h).unwrap();
        let b: Vec<Real> = (0..n).map(|i| if i == 0 { 2.0 } else { 0.0 }).collect();

        let x = a.solve_cg(&b, 1000, 1e-10).unwrap();
        let ax = a.mul_vec(&x);
        let residual: Real = ax.iter().zip(&b).map(|(p, q)| (p - q).powi(2)).sum::<Real>().sqrt();
        assert!(residual < 1e-8, "residuo = {residual}");
        // La solución es positiva y decrece al alejarse del nodo con calor
        assert!(x[0] > x[25] && x[25] > 0.0);
    }

    #[test]
    fn from_entries_sums_duplicates_and_sorts() {
        // Fuera de orden y con una entrada repetida: (1,0) = 2 + 3
        let m = SPDMatrix::from_entries(3, vec![(2, 2, 1.0), (1, 0, 2.0), (0, 0, 4.0), (1, 0, 3.0), (0, 1, 5.0)]).unwrap();
        assert_eq!(m.nnz(), 4);
        assert_eq!(m.get(1, 0), 5.0);
        assert_eq!(m.get(0, 1), 5.0);
        assert_eq!(m.get(0, 0), 4.0);
        assert_eq!(m.get(2, 2), 1.0);
        let csc = m.as_csc();
        assert!(csc.indices().windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn from_triplets_rejects_an_index_out_of_range() {
        assert!(matches!(
            SPDMatrix::from_triplets(2, &[0, 2], &[0, 0], &[1.0, 1.0]),
            Err(SPDMatrixError::DimensionMismatch { expected: 2, .. })
        ));
        assert!(SPDMatrix::from_triplets(2, &[0], &[0, 1], &[1.0]).is_err());
    }

    #[test]
    fn add_diagonal_adds_to_existing_entries_and_inserts_missing_ones() {
        // La fila/columna 1 no tiene entrada diagonal; las 0 y 2 sí
        let m = SPDMatrix::from_entries(3, vec![(0, 0, 1.0), (1, 0, -1.0), (0, 1, -1.0), (2, 2, 2.0), (2, 1, 0.5), (1, 2, 0.5)]).unwrap();
        let out = m.add_diagonal(&[10.0, 20.0, 30.0]).unwrap();
        assert_eq!(out.get(0, 0), 11.0);
        assert_eq!(out.get(1, 1), 20.0);
        assert_eq!(out.get(2, 2), 32.0);
        assert_eq!(out.get(1, 0), -1.0);
        assert_eq!(out.get(1, 2), 0.5);
        assert_eq!(out.nnz(), 7);
        assert!(m.add_diagonal(&[1.0]).is_err());
    }
}
