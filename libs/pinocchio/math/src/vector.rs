//! Tipos de vectores 2D y 3D

use crate::Real;
use nalgebra::{Vector2 as NaVector2, Vector3 as NaVector3};
use std::ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};

/// Vector 2D
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2(pub NaVector2<Real>);

/// Vector 3D
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector3(pub NaVector3<Real>);

// ============================================================================
// Vector2 Implementation
// ============================================================================

impl Vector2 {
    /// Crea un nuevo Vector2
    #[inline]
    pub fn new(x: Real, y: Real) -> Self {
        Self(NaVector2::new(x, y))
    }

    /// Vector cero
    #[inline]
    pub fn zero() -> Self {
        Self(NaVector2::zeros())
    }

    /// Vector unitario X
    #[inline]
    pub fn unit_x() -> Self {
        Self(NaVector2::x())
    }

    /// Vector unitario Y
    #[inline]
    pub fn unit_y() -> Self {
        Self(NaVector2::y())
    }

    /// Componente X
    #[inline]
    pub fn x(&self) -> Real {
        self.0.x
    }

    /// Componente Y
    #[inline]
    pub fn y(&self) -> Real {
        self.0.y
    }

    /// Referencia mutable a X
    #[inline]
    pub fn x_mut(&mut self) -> &mut Real {
        &mut self.0.x
    }

    /// Referencia mutable a Y
    #[inline]
    pub fn y_mut(&mut self) -> &mut Real {
        &mut self.0.y
    }

    /// Longitud del vector (norma L2)
    #[inline]
    pub fn length(&self) -> Real {
        self.0.norm()
    }

    /// Longitud al cuadrado (más eficiente si no necesitas la raíz)
    #[inline]
    pub fn length_squared(&self) -> Real {
        self.0.norm_squared()
    }

    /// Normaliza el vector (devuelve vector unitario)
    #[inline]
    pub fn normalize(&self) -> Self {
        Self(self.0.normalize())
    }

    /// Intenta normalizar, devuelve None si el vector es muy pequeño
    #[inline]
    pub fn try_normalize(&self) -> Option<Self> {
        self.0.try_normalize(crate::EPSILON).map(Self)
    }

    /// Producto punto
    #[inline]
    pub fn dot(&self, other: &Self) -> Real {
        self.0.dot(&other.0)
    }

    /// "Cross product" 2D (determinante, devuelve escalar)
    #[inline]
    pub fn cross(&self, other: &Self) -> Real {
        self.x() * other.y() - self.y() * other.x()
    }

    /// Distancia a otro punto
    #[inline]
    pub fn distance(&self, other: &Self) -> Real {
        (*self - *other).length()
    }

    /// Distancia al cuadrado
    #[inline]
    pub fn distance_squared(&self, other: &Self) -> Real {
        (*self - *other).length_squared()
    }

    /// Interpolación lineal
    #[inline]
    pub fn lerp(&self, other: &Self, t: Real) -> Self {
        Self(self.0.lerp(&other.0, t))
    }

    /// Vector perpendicular (rotado 90 grados)
    #[inline]
    pub fn perpendicular(&self) -> Self {
        Self::new(-self.y(), self.x())
    }

    /// Componente mínimo
    #[inline]
    pub fn min_component(&self) -> Real {
        self.x().min(self.y())
    }

    /// Componente máximo
    #[inline]
    pub fn max_component(&self) -> Real {
        self.x().max(self.y())
    }

    /// Mínimo por componente con otro vector
    #[inline]
    pub fn min(&self, other: &Self) -> Self {
        Self::new(self.x().min(other.x()), self.y().min(other.y()))
    }

    /// Máximo por componente con otro vector
    #[inline]
    pub fn max(&self, other: &Self) -> Self {
        Self::new(self.x().max(other.x()), self.y().max(other.y()))
    }

    /// Acceso al vector interno de nalgebra
    #[inline]
    pub fn as_nalgebra(&self) -> &NaVector2<Real> {
        &self.0
    }
}

impl Default for Vector2 {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<[Real; 2]> for Vector2 {
    fn from(arr: [Real; 2]) -> Self {
        Self::new(arr[0], arr[1])
    }
}

impl From<(Real, Real)> for Vector2 {
    fn from((x, y): (Real, Real)) -> Self {
        Self::new(x, y)
    }
}

impl From<Vector2> for [Real; 2] {
    fn from(v: Vector2) -> Self {
        [v.x(), v.y()]
    }
}

impl Index<usize> for Vector2 {
    type Output = Real;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Vector2 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Neg for Vector2 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Add for Vector2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Vector2 {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Vector2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Vector2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul<Real> for Vector2 {
    type Output = Self;
    fn mul(self, rhs: Real) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Mul<Vector2> for Real {
    type Output = Vector2;
    fn mul(self, rhs: Vector2) -> Self::Output {
        Vector2(rhs.0 * self)
    }
}

impl MulAssign<Real> for Vector2 {
    fn mul_assign(&mut self, rhs: Real) {
        self.0 *= rhs;
    }
}

impl Div<Real> for Vector2 {
    type Output = Self;
    fn div(self, rhs: Real) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl DivAssign<Real> for Vector2 {
    fn div_assign(&mut self, rhs: Real) {
        self.0 /= rhs;
    }
}

// ============================================================================
// Vector3 Implementation
// ============================================================================

impl Vector3 {
    /// Crea un nuevo Vector3
    #[inline]
    pub fn new(x: Real, y: Real, z: Real) -> Self {
        Self(NaVector3::new(x, y, z))
    }

    /// Vector cero
    #[inline]
    pub fn zero() -> Self {
        Self(NaVector3::zeros())
    }

    /// Vector unitario X
    #[inline]
    pub fn unit_x() -> Self {
        Self(NaVector3::x())
    }

    /// Vector unitario Y
    #[inline]
    pub fn unit_y() -> Self {
        Self(NaVector3::y())
    }

    /// Vector unitario Z
    #[inline]
    pub fn unit_z() -> Self {
        Self(NaVector3::z())
    }

    /// Componente X
    #[inline]
    pub fn x(&self) -> Real {
        self.0.x
    }

    /// Componente Y
    #[inline]
    pub fn y(&self) -> Real {
        self.0.y
    }

    /// Componente Z
    #[inline]
    pub fn z(&self) -> Real {
        self.0.z
    }

    /// Referencia mutable a X
    #[inline]
    pub fn x_mut(&mut self) -> &mut Real {
        &mut self.0.x
    }

    /// Referencia mutable a Y
    #[inline]
    pub fn y_mut(&mut self) -> &mut Real {
        &mut self.0.y
    }

    /// Referencia mutable a Z
    #[inline]
    pub fn z_mut(&mut self) -> &mut Real {
        &mut self.0.z
    }

    /// Longitud del vector (norma L2)
    #[inline]
    pub fn length(&self) -> Real {
        self.0.norm()
    }

    /// Longitud al cuadrado
    #[inline]
    pub fn length_squared(&self) -> Real {
        self.0.norm_squared()
    }

    /// Normaliza el vector
    #[inline]
    pub fn normalize(&self) -> Self {
        Self(self.0.normalize())
    }

    /// Intenta normalizar, devuelve None si el vector es muy pequeño
    #[inline]
    pub fn try_normalize(&self) -> Option<Self> {
        self.0.try_normalize(crate::EPSILON).map(Self)
    }

    /// Producto punto
    #[inline]
    pub fn dot(&self, other: &Self) -> Real {
        self.0.dot(&other.0)
    }

    /// Producto cruz
    #[inline]
    pub fn cross(&self, other: &Self) -> Self {
        Self(self.0.cross(&other.0))
    }

    /// Distancia a otro punto
    #[inline]
    pub fn distance(&self, other: &Self) -> Real {
        (*self - *other).length()
    }

    /// Distancia al cuadrado
    #[inline]
    pub fn distance_squared(&self, other: &Self) -> Real {
        (*self - *other).length_squared()
    }

    /// Interpolación lineal
    #[inline]
    pub fn lerp(&self, other: &Self, t: Real) -> Self {
        Self(self.0.lerp(&other.0, t))
    }

    /// Componente mínimo
    #[inline]
    pub fn min_component(&self) -> Real {
        self.x().min(self.y()).min(self.z())
    }

    /// Componente máximo
    #[inline]
    pub fn max_component(&self) -> Real {
        self.x().max(self.y()).max(self.z())
    }

    /// Mínimo por componente con otro vector
    #[inline]
    pub fn min(&self, other: &Self) -> Self {
        Self::new(
            self.x().min(other.x()),
            self.y().min(other.y()),
            self.z().min(other.z()),
        )
    }

    /// Máximo por componente con otro vector
    #[inline]
    pub fn max(&self, other: &Self) -> Self {
        Self::new(
            self.x().max(other.x()),
            self.y().max(other.y()),
            self.z().max(other.z()),
        )
    }

    /// Convierte a Vector2 descartando Z
    #[inline]
    pub fn xy(&self) -> Vector2 {
        Vector2::new(self.x(), self.y())
    }

    /// Convierte a Vector2 descartando Y
    #[inline]
    pub fn xz(&self) -> Vector2 {
        Vector2::new(self.x(), self.z())
    }

    /// Convierte a Vector2 descartando X
    #[inline]
    pub fn yz(&self) -> Vector2 {
        Vector2::new(self.y(), self.z())
    }

    /// Acceso al vector interno de nalgebra
    #[inline]
    pub fn as_nalgebra(&self) -> &NaVector3<Real> {
        &self.0
    }

    /// Proyección sobre otro vector
    #[inline]
    pub fn project_onto(&self, other: &Self) -> Self {
        let dot = self.dot(other);
        let len_sq = other.length_squared();
        if len_sq < crate::EPSILON {
            Self::zero()
        } else {
            *other * (dot / len_sq)
        }
    }

    /// Refleja el vector respecto a una normal
    #[inline]
    pub fn reflect(&self, normal: &Self) -> Self {
        *self - *normal * (2.0 * self.dot(normal))
    }
}

impl Default for Vector3 {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<[Real; 3]> for Vector3 {
    fn from(arr: [Real; 3]) -> Self {
        Self::new(arr[0], arr[1], arr[2])
    }
}

impl From<(Real, Real, Real)> for Vector3 {
    fn from((x, y, z): (Real, Real, Real)) -> Self {
        Self::new(x, y, z)
    }
}

impl From<Vector3> for [Real; 3] {
    fn from(v: Vector3) -> Self {
        [v.x(), v.y(), v.z()]
    }
}

impl Index<usize> for Vector3 {
    type Output = Real;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Vector3 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Neg for Vector3 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Add for Vector3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Vector3 {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Vector3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Vector3 {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul<Real> for Vector3 {
    type Output = Self;
    fn mul(self, rhs: Real) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Mul<Vector3> for Real {
    type Output = Vector3;
    fn mul(self, rhs: Vector3) -> Self::Output {
        Vector3(rhs.0 * self)
    }
}

impl MulAssign<Real> for Vector3 {
    fn mul_assign(&mut self, rhs: Real) {
        self.0 *= rhs;
    }
}

impl Div<Real> for Vector3 {
    type Output = Self;
    fn div(self, rhs: Real) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl DivAssign<Real> for Vector3 {
    fn div_assign(&mut self, rhs: Real) {
        self.0 /= rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approx_eq;

    #[test]
    fn test_vector2_basic() {
        let v = Vector2::new(3.0, 4.0);
        assert!(approx_eq(v.length(), 5.0));
        assert!(approx_eq(v.length_squared(), 25.0));
    }

    #[test]
    fn test_vector2_normalize() {
        let v = Vector2::new(3.0, 4.0);
        let n = v.normalize();
        assert!(approx_eq(n.length(), 1.0));
        assert!(approx_eq(n.x(), 0.6));
        assert!(approx_eq(n.y(), 0.8));
    }

    #[test]
    fn test_vector2_dot() {
        let a = Vector2::new(1.0, 2.0);
        let b = Vector2::new(3.0, 4.0);
        assert!(approx_eq(a.dot(&b), 11.0));
    }

    #[test]
    fn test_vector2_cross() {
        let a = Vector2::new(1.0, 0.0);
        let b = Vector2::new(0.0, 1.0);
        assert!(approx_eq(a.cross(&b), 1.0));
    }

    #[test]
    fn test_vector2_ops() {
        let a = Vector2::new(1.0, 2.0);
        let b = Vector2::new(3.0, 4.0);

        let sum = a + b;
        assert!(approx_eq(sum.x(), 4.0));
        assert!(approx_eq(sum.y(), 6.0));

        let diff = b - a;
        assert!(approx_eq(diff.x(), 2.0));
        assert!(approx_eq(diff.y(), 2.0));

        let scaled = a * 2.0;
        assert!(approx_eq(scaled.x(), 2.0));
        assert!(approx_eq(scaled.y(), 4.0));
    }

    #[test]
    fn test_vector3_basic() {
        let v = Vector3::new(1.0, 2.0, 2.0);
        assert!(approx_eq(v.length(), 3.0));
        assert!(approx_eq(v.length_squared(), 9.0));
    }

    #[test]
    fn test_vector3_cross() {
        let x = Vector3::unit_x();
        let y = Vector3::unit_y();
        let z = x.cross(&y);
        assert!(approx_eq(z.x(), 0.0));
        assert!(approx_eq(z.y(), 0.0));
        assert!(approx_eq(z.z(), 1.0));
    }

    #[test]
    fn test_vector3_project() {
        let v = Vector3::new(3.0, 4.0, 0.0);
        let onto = Vector3::unit_x();
        let proj = v.project_onto(&onto);
        assert!(approx_eq(proj.x(), 3.0));
        assert!(approx_eq(proj.y(), 0.0));
        assert!(approx_eq(proj.z(), 0.0));
    }

    #[test]
    fn test_vector3_reflect() {
        let v = Vector3::new(1.0, -1.0, 0.0);
        let n = Vector3::unit_y();
        let r = v.reflect(&n);
        assert!(approx_eq(r.x(), 1.0));
        assert!(approx_eq(r.y(), 1.0));
        assert!(approx_eq(r.z(), 0.0));
    }

    #[test]
    fn test_vector3_lerp() {
        let a = Vector3::zero();
        let b = Vector3::new(10.0, 20.0, 30.0);
        let mid = a.lerp(&b, 0.5);
        assert!(approx_eq(mid.x(), 5.0));
        assert!(approx_eq(mid.y(), 10.0));
        assert!(approx_eq(mid.z(), 15.0));
    }
}
