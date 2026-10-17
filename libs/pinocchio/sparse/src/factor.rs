//! Factorización Cholesky dispersa para resolver muchos lados derechos con la misma matriz
//!
//! Con la misma matriz y muchos lados derechos (un sistema por hueso en los
//! pesos de piel) conviene factorizar una vez y resolver cada uno con dos
//! sustituciones triangulares, en vez de repetir cientos de iteraciones de CG
//! por hueso. Usa `faer`: ordenamiento de reducción de relleno (AMD) y
//! factorización supernodal.
//!
//! El paralelismo se elige aquí y no con el global de `faer`. La factorización
//! va siempre en secuencial: en mallas (grafos casi planos, supernodos chicos)
//! el paralelismo de `faer` es más lento, medido en una malla de 164k vértices:
//! 0,94 s en secuencial, 1,07 s con 2 hilos, 1,06 s con 4, 1,6 s con 8 y
//! 3,4 s con 16. Lo que sí escala son los solves: se reparten las columnas
//! entre hilos, con cada solve en secuencial.

use faer::dyn_stack::{MemBuffer, MemStack};
use faer::linalg::cholesky::llt::factor::LltRegularization;
use faer::sparse::linalg::cholesky::{
    CholeskySymbolicParams, LltRef, SymbolicCholesky, SymmetricOrdering, factorize_symbolic_cholesky,
};
use faer::sparse::{SparseColMatRef, SymbolicSparseColMatRef};
use faer::{Conj, MatMut, Par, Side};
use pinocchio_math::Real;
use rayon::prelude::*;
use sprs::CsMatI;

use crate::spd_matrix::SPDMatrixError;

/// Columnas que resuelve cada hilo a la vez (comparten la lectura del factor)
const SOLVE_BATCH: usize = 8;

/// Factorización `A = L·Lᵀ` de una matriz SPD
pub struct SpdFactor {
    /// `None` en la matriz vacía (no hay nada que factorizar)
    inner: Option<(SymbolicCholesky<usize>, Vec<Real>)>,
    size: usize,
}

impl SpdFactor {
    /// Factoriza una matriz CSC con índices ordenados (solo se lee el triángulo inferior)
    pub(crate) fn new(m: &CsMatI<Real, usize>) -> Result<Self, SPDMatrixError> {
        let n = m.rows();
        if m.rows() != m.cols() {
            return Err(SPDMatrixError::CholeskyError);
        }
        if n == 0 {
            return Ok(Self { inner: None, size: 0 });
        }
        if !m.is_csc() {
            return Err(SPDMatrixError::CholeskyError);
        }
        let indptr = m.indptr();
        let ptr = indptr.raw_storage();
        let idx = m.indices();
        // `new_checked` exige índices estrictamente crecientes por columna
        let sorted = (0..n).all(|c| idx[ptr[c]..ptr[c + 1]].windows(2).all(|w| w[0] < w[1]));
        if !sorted {
            return Err(SPDMatrixError::CholeskyError);
        }
        let pattern = SymbolicSparseColMatRef::new_checked(n, n, ptr, None, idx);
        let mat = SparseColMatRef::new(pattern, m.data());

        let symbolic = factorize_symbolic_cholesky(pattern, Side::Lower, SymmetricOrdering::Amd, CholeskySymbolicParams::default())
            .map_err(|_| SPDMatrixError::CholeskyError)?;
        let par = Par::Seq;
        let mut values = vec![0.0; symbolic.len_val()];
        let mut scratch = MemBuffer::new(symbolic.factorize_numeric_llt_scratch::<Real>(par, Default::default()));
        symbolic
            .factorize_numeric_llt::<Real>(
                &mut values,
                mat,
                Side::Lower,
                LltRegularization::default(),
                par,
                MemStack::new(&mut scratch),
                Default::default(),
            )
            .map_err(|_| SPDMatrixError::NotPositiveDefinite)?;
        Ok(Self { inner: Some((symbolic, values)), size: n })
    }

    /// Dimensión de la matriz
    pub fn size(&self) -> usize {
        self.size
    }

    /// Resuelve `A·x = b` para cada columna de `rhs`, repartiendo tandas de columnas entre hilos
    pub fn solve_columns(&self, rhs: &[&[Real]]) -> Result<Vec<Vec<Real>>, SPDMatrixError> {
        if let Some(bad) = rhs.iter().find(|c| c.len() != self.size) {
            return Err(SPDMatrixError::DimensionMismatch { expected: self.size, found: bad.len() });
        }
        let Some((symbolic, values)) = &self.inner else {
            return Ok(vec![Vec::new(); rhs.len()]);
        };
        let llt = LltRef::new(symbolic, values);
        let batches: Vec<Vec<Vec<Real>>> = rhs
            .par_chunks(SOLVE_BATCH)
            .map(|cols| {
                let k = cols.len();
                let n = self.size;
                // Columnas contiguas (n × k, por columnas): `faer` las lee como una matriz
                let mut data = vec![0.0; n * k];
                for (j, c) in cols.iter().enumerate() {
                    data[j * n..(j + 1) * n].copy_from_slice(c);
                }
                let mut scratch = MemBuffer::new(symbolic.solve_in_place_scratch::<Real>(k, Par::Seq));
                llt.solve_in_place_with_conj(
                    Conj::No,
                    MatMut::from_column_major_slice_mut(&mut data, n, k),
                    Par::Seq,
                    MemStack::new(&mut scratch),
                );
                data.chunks_exact(n).map(<[Real]>::to_vec).collect()
            })
            .collect();
        Ok(batches.into_iter().flatten().collect())
    }

    /// Resuelve `A·x = b`
    pub fn solve(&self, b: &[Real]) -> Result<Vec<Real>, SPDMatrixError> {
        Ok(self.solve_columns(&[b])?.pop().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use crate::{SPDMatrix, SPDMatrixError};
    use pinocchio_math::Real;

    /// Laplaciano 1D con diagonal reforzada: tridiagonal, simétrica y definida positiva
    fn tridiagonal(n: usize) -> SPDMatrix {
        let (mut r, mut c, mut v) = (vec![], vec![], vec![]);
        for i in 0..n {
            r.push(i);
            c.push(i);
            v.push(2.5);
            if i + 1 < n {
                r.extend([i, i + 1]);
                c.extend([i + 1, i]);
                v.extend([-1.0, -1.0]);
            }
        }
        SPDMatrix::from_triplets(n, &r, &c, &v).unwrap()
    }

    fn residual(a: &SPDMatrix, x: &[Real], b: &[Real]) -> Real {
        a.mul_vec(x).iter().zip(b).map(|(ax, b)| (ax - b).abs()).fold(0.0, Real::max)
    }

    #[test]
    fn solves_a_small_system() {
        let a = SPDMatrix::from_triplets(
            3,
            &[0, 0, 1, 1, 1, 2, 2],
            &[0, 1, 0, 1, 2, 1, 2],
            &[4.0, 1.0, 1.0, 3.0, 1.0, 1.0, 2.0],
        )
        .unwrap();
        let x = a.cholesky_factor().unwrap().solve(&[1.0, 2.0, 3.0]).unwrap();
        assert!(residual(&a, &x, &[1.0, 2.0, 3.0]) < 1e-12);
    }

    #[test]
    fn many_columns_match_single_solves() {
        // 11 columnas: cruza el tamaño de tanda de los solves (8)
        let n = 200;
        let a = tridiagonal(n);
        let factor = a.cholesky_factor().unwrap();
        let rhs: Vec<Vec<Real>> = (0..11).map(|k| (0..n).map(|i| ((i * (k + 3)) % 17) as Real - 8.0).collect()).collect();
        let refs: Vec<&[Real]> = rhs.iter().map(Vec::as_slice).collect();
        let all = factor.solve_columns(&refs).unwrap();
        assert_eq!(all.len(), 11);
        for (b, x) in rhs.iter().zip(&all) {
            assert!(residual(&a, x, b) < 1e-10);
            assert_eq!(&factor.solve(b).unwrap(), x, "resolver todas juntas o de a una da lo mismo");
        }
        assert!(factor.solve_columns(&[]).unwrap().is_empty());
    }

    #[test]
    fn matches_conjugate_gradient() {
        let n = 300;
        let a = tridiagonal(n);
        let b: Vec<Real> = (0..n).map(|i| (i as Real * 0.37).sin()).collect();
        let direct = a.cholesky_factor().unwrap().solve(&b).unwrap();
        let cg = a.solve_cg(&b, 5000, 1e-12).unwrap();
        let worst = direct.iter().zip(&cg).map(|(d, c)| (d - c).abs()).fold(0.0, Real::max);
        assert!(worst < 1e-9, "difieren en {worst}");
    }

    #[test]
    fn reports_a_matrix_that_is_not_positive_definite() {
        // Autovalores 3 y -1
        let a = SPDMatrix::from_triplets(2, &[0, 0, 1, 1], &[0, 1, 0, 1], &[1.0, 2.0, 2.0, 1.0]).unwrap();
        assert!(matches!(a.cholesky_factor(), Err(SPDMatrixError::NotPositiveDefinite)));
    }

    #[test]
    fn rejects_a_right_hand_side_of_the_wrong_size() {
        let factor = tridiagonal(5).cholesky_factor().unwrap();
        assert!(matches!(
            factor.solve(&[1.0, 2.0]),
            Err(SPDMatrixError::DimensionMismatch { expected: 5, found: 2 })
        ));
    }

    #[test]
    fn an_empty_matrix_has_nothing_to_solve() {
        let factor = SPDMatrix::new(0).cholesky_factor().unwrap();
        assert_eq!(factor.size(), 0);
        assert_eq!(factor.solve_columns(&[&[], &[]]).unwrap(), vec![Vec::<Real>::new(), Vec::new()]);
    }
}
