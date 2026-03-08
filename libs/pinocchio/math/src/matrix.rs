//! Matriz 3x3 para transformaciones

use crate::{Real, Vector3, EPSILON};
use nalgebra::Matrix3 as NaMatrix3;
use std::ops::{Add, Mul, Sub};

/// Matriz 3x3
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix3(pub NaMatrix3<Real>);

impl Matrix3 {
    /// Crea una matriz desde componentes (row-major)
    #[inline]
    pub fn new(
        m00: Real, m01: Real, m02: Real,
        m10: Real, m11: Real, m12: Real,
        m20: Real, m21: Real, m22: Real,
    ) -> Self {
        // nalgebra usa column-major, así que transponemos
        Self(NaMatrix3::new(
            m00, m01, m02,
            m10, m11, m12,
            m20, m21, m22,
        ))
    }

    /// Matriz identidad
    #[inline]
    pub fn identity() -> Self {
        Self(NaMatrix3::identity())
    }

    /// Matriz cero
    #[inline]
    pub fn zero() -> Self {
        Self(NaMatrix3::zeros())
    }

    /// Crea una matriz de escala uniforme
    #[inline]
    pub fn from_scale(scale: Real) -> Self {
        Self(NaMatrix3::from_diagonal_element(scale))
    }

    /// Crea una matriz de escala no uniforme
    #[inline]
    pub fn from_scale_nonuniform(sx: Real, sy: Real, sz: Real) -> Self {
        Self(NaMatrix3::from_diagonal(&nalgebra::Vector3::new(sx, sy, sz)))
    }

    /// Crea una matriz de rotación alrededor del eje X
    #[inline]
    pub fn from_rotation_x(angle: Real) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(
            1.0, 0.0, 0.0,
            0.0, c, -s,
            0.0, s, c,
        )
    }

    /// Crea una matriz de rotación alrededor del eje Y
    #[inline]
    pub fn from_rotation_y(angle: Real) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(
            c, 0.0, s,
            0.0, 1.0, 0.0,
            -s, 0.0, c,
        )
    }

    /// Crea una matriz de rotación alrededor del eje Z
    #[inline]
    pub fn from_rotation_z(angle: Real) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(
            c, -s, 0.0,
            s, c, 0.0,
            0.0, 0.0, 1.0,
        )
    }

    /// Crea una matriz de rotación alrededor de un eje arbitrario (Rodrigues)
    pub fn from_axis_angle(axis: &Vector3, angle: Real) -> Self {
        let axis = axis.normalize();
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let x = axis.x();
        let y = axis.y();
        let z = axis.z();

        Self::new(
            t * x * x + c,     t * x * y - s * z, t * x * z + s * y,
            t * x * y + s * z, t * y * y + c,     t * y * z - s * x,
            t * x * z - s * y, t * y * z + s * x, t * z * z + c,
        )
    }

    /// Accede a un elemento (row, col)
    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Real {
        self.0[(row, col)]
    }

    /// Establece un elemento (row, col)
    #[inline]
    pub fn set(&mut self, row: usize, col: usize, value: Real) {
        self.0[(row, col)] = value;
    }

    /// Obtiene una fila como Vector3
    #[inline]
    pub fn row(&self, index: usize) -> Vector3 {
        let r = self.0.row(index);
        Vector3::new(r[0], r[1], r[2])
    }

    /// Obtiene una columna como Vector3
    #[inline]
    pub fn column(&self, index: usize) -> Vector3 {
        let c = self.0.column(index);
        Vector3::new(c[0], c[1], c[2])
    }

    /// Transpuesta
    #[inline]
    pub fn transpose(&self) -> Self {
        Self(self.0.transpose())
    }

    /// Determinante
    #[inline]
    pub fn determinant(&self) -> Real {
        self.0.determinant()
    }

    /// Traza (suma de la diagonal)
    #[inline]
    pub fn trace(&self) -> Real {
        self.0.trace()
    }

    /// Inversa de la matriz
    #[inline]
    pub fn inverse(&self) -> Option<Self> {
        self.0.try_inverse().map(Self)
    }

    /// Inversa asumiendo que la matriz es invertible
    #[inline]
    pub fn inverse_unchecked(&self) -> Self {
        Self(self.0.try_inverse().expect("Matrix is not invertible"))
    }

    /// Transforma un vector
    #[inline]
    pub fn transform_vector(&self, v: &Vector3) -> Vector3 {
        Vector3(self.0 * v.0)
    }

    /// Crea una matriz "outer product" de dos vectores: v1 * v2^T
    #[inline]
    pub fn outer_product(v1: &Vector3, v2: &Vector3) -> Self {
        Self(v1.0 * v2.0.transpose())
    }

    /// Norma de Frobenius
    #[inline]
    pub fn frobenius_norm(&self) -> Real {
        self.0.norm()
    }

    /// Verifica si la matriz es aproximadamente la identidad
    #[inline]
    pub fn is_identity(&self) -> bool {
        (self.0 - NaMatrix3::identity()).norm() < EPSILON
    }

    /// Verifica si la matriz es ortogonal (R^T * R = I)
    #[inline]
    pub fn is_orthogonal(&self) -> bool {
        let product = self.0.transpose() * self.0;
        (product - NaMatrix3::identity()).norm() < EPSILON
    }

    /// Acceso al vector interno de nalgebra
    #[inline]
    pub fn as_nalgebra(&self) -> &NaMatrix3<Real> {
        &self.0
    }

    /// Convierte a array row-major
    pub fn to_array(&self) -> [[Real; 3]; 3] {
        [
            [self.get(0, 0), self.get(0, 1), self.get(0, 2)],
            [self.get(1, 0), self.get(1, 1), self.get(1, 2)],
            [self.get(2, 0), self.get(2, 1), self.get(2, 2)],
        ]
    }
}

impl Default for Matrix3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl From<[[Real; 3]; 3]> for Matrix3 {
    fn from(arr: [[Real; 3]; 3]) -> Self {
        Self::new(
            arr[0][0], arr[0][1], arr[0][2],
            arr[1][0], arr[1][1], arr[1][2],
            arr[2][0], arr[2][1], arr[2][2],
        )
    }
}

impl Add for Matrix3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Matrix3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul for Matrix3 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Mul<Real> for Matrix3 {
    type Output = Self;
    fn mul(self, rhs: Real) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Mul<Vector3> for Matrix3 {
    type Output = Vector3;
    fn mul(self, rhs: Vector3) -> Self::Output {
        self.transform_vector(&rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{approx_eq, PI};

    #[test]
    fn test_identity() {
        let m = Matrix3::identity();
        let v = Vector3::new(1.0, 2.0, 3.0);
        let result = m.transform_vector(&v);
        assert!(approx_eq(result.x(), 1.0));
        assert!(approx_eq(result.y(), 2.0));
        assert!(approx_eq(result.z(), 3.0));
    }

    #[test]
    fn test_scale() {
        let m = Matrix3::from_scale(2.0);
        let v = Vector3::new(1.0, 2.0, 3.0);
        let result = m.transform_vector(&v);
        assert!(approx_eq(result.x(), 2.0));
        assert!(approx_eq(result.y(), 4.0));
        assert!(approx_eq(result.z(), 6.0));
    }

    #[test]
    fn test_rotation_z() {
        let m = Matrix3::from_rotation_z(PI / 2.0);
        let v = Vector3::unit_x();
        let result = m.transform_vector(&v);
        assert!(approx_eq(result.x(), 0.0));
        assert!(approx_eq(result.y(), 1.0));
        assert!(approx_eq(result.z(), 0.0));
    }

    #[test]
    fn test_rotation_orthogonal() {
        let m = Matrix3::from_rotation_x(0.5);
        assert!(m.is_orthogonal());
    }

    #[test]
    fn test_inverse() {
        let m = Matrix3::from_scale_nonuniform(2.0, 3.0, 4.0);
        let inv = m.inverse().unwrap();
        let product = m * inv;
        assert!(product.is_identity());
    }

    #[test]
    fn test_determinant() {
        let m = Matrix3::from_scale(2.0);
        assert!(approx_eq(m.determinant(), 8.0)); // 2^3
    }

    #[test]
    fn test_transpose() {
        let m = Matrix3::new(
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0,
            7.0, 8.0, 9.0,
        );
        let t = m.transpose();
        assert!(approx_eq(t.get(0, 1), 4.0));
        assert!(approx_eq(t.get(1, 0), 2.0));
    }

    #[test]
    fn test_axis_angle() {
        // Rotación de 90 grados alrededor de Z debería ser igual a from_rotation_z
        let m1 = Matrix3::from_axis_angle(&Vector3::unit_z(), PI / 2.0);
        let m2 = Matrix3::from_rotation_z(PI / 2.0);

        for i in 0..3 {
            for j in 0..3 {
                assert!(approx_eq(m1.get(i, j), m2.get(i, j)));
            }
        }
    }
}
