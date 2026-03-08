//! Quaterniones para rotaciones 3D

use crate::{approx_zero, Matrix3, Real, Vector3, EPSILON};
use std::ops::{Mul, MulAssign, Neg};

/// Quaternion unitario para representar rotaciones
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    /// Componente escalar (w)
    pub w: Real,
    /// Componente vectorial X
    pub x: Real,
    /// Componente vectorial Y
    pub y: Real,
    /// Componente vectorial Z
    pub z: Real,
}

impl Quaternion {
    /// Crea un nuevo quaternion
    #[inline]
    pub fn new(w: Real, x: Real, y: Real, z: Real) -> Self {
        Self { w, x, y, z }
    }

    /// Quaternion identidad (sin rotación)
    #[inline]
    pub fn identity() -> Self {
        Self::new(1.0, 0.0, 0.0, 0.0)
    }

    /// Crea un quaternion desde eje y ángulo
    pub fn from_axis_angle(axis: &Vector3, angle: Real) -> Self {
        let half_angle = angle * 0.5;
        let (s, c) = half_angle.sin_cos();
        let axis = axis.normalize();
        Self::new(c, axis.x() * s, axis.y() * s, axis.z() * s)
    }

    /// Crea un quaternion desde una matriz de rotación
    pub fn from_rotation_matrix(m: &Matrix3) -> Self {
        let trace = m.trace();

        if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            Self::new(
                0.25 * s,
                (m.get(2, 1) - m.get(1, 2)) / s,
                (m.get(0, 2) - m.get(2, 0)) / s,
                (m.get(1, 0) - m.get(0, 1)) / s,
            )
        } else if m.get(0, 0) > m.get(1, 1) && m.get(0, 0) > m.get(2, 2) {
            let s = (1.0 + m.get(0, 0) - m.get(1, 1) - m.get(2, 2)).sqrt() * 2.0;
            Self::new(
                (m.get(2, 1) - m.get(1, 2)) / s,
                0.25 * s,
                (m.get(0, 1) + m.get(1, 0)) / s,
                (m.get(0, 2) + m.get(2, 0)) / s,
            )
        } else if m.get(1, 1) > m.get(2, 2) {
            let s = (1.0 + m.get(1, 1) - m.get(0, 0) - m.get(2, 2)).sqrt() * 2.0;
            Self::new(
                (m.get(0, 2) - m.get(2, 0)) / s,
                (m.get(0, 1) + m.get(1, 0)) / s,
                0.25 * s,
                (m.get(1, 2) + m.get(2, 1)) / s,
            )
        } else {
            let s = (1.0 + m.get(2, 2) - m.get(0, 0) - m.get(1, 1)).sqrt() * 2.0;
            Self::new(
                (m.get(1, 0) - m.get(0, 1)) / s,
                (m.get(0, 2) + m.get(2, 0)) / s,
                (m.get(1, 2) + m.get(2, 1)) / s,
                0.25 * s,
            )
        }
    }

    /// Crea un quaternion desde ángulos de Euler (roll, pitch, yaw)
    pub fn from_euler(roll: Real, pitch: Real, yaw: Real) -> Self {
        let (sr, cr) = (roll * 0.5).sin_cos();
        let (sp, cp) = (pitch * 0.5).sin_cos();
        let (sy, cy) = (yaw * 0.5).sin_cos();

        Self::new(
            cr * cp * cy + sr * sp * sy,
            sr * cp * cy - cr * sp * sy,
            cr * sp * cy + sr * cp * sy,
            cr * cp * sy - sr * sp * cy,
        )
    }

    /// Convierte a matriz de rotación 3x3
    pub fn to_rotation_matrix(&self) -> Matrix3 {
        let xx = self.x * self.x;
        let yy = self.y * self.y;
        let zz = self.z * self.z;
        let xy = self.x * self.y;
        let xz = self.x * self.z;
        let yz = self.y * self.z;
        let wx = self.w * self.x;
        let wy = self.w * self.y;
        let wz = self.w * self.z;

        Matrix3::new(
            1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz),       2.0 * (xz + wy),
            2.0 * (xy + wz),       1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx),
            2.0 * (xz - wy),       2.0 * (yz + wx),       1.0 - 2.0 * (xx + yy),
        )
    }

    /// Extrae el eje y ángulo de rotación
    pub fn to_axis_angle(&self) -> (Vector3, Real) {
        let angle = 2.0 * self.w.acos();
        let s = (1.0 - self.w * self.w).sqrt();

        if s < EPSILON {
            (Vector3::unit_x(), angle)
        } else {
            (Vector3::new(self.x / s, self.y / s, self.z / s), angle)
        }
    }

    /// Norma del quaternion
    #[inline]
    pub fn norm(&self) -> Real {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Norma al cuadrado
    #[inline]
    pub fn norm_squared(&self) -> Real {
        self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Normaliza el quaternion
    #[inline]
    pub fn normalize(&self) -> Self {
        let n = self.norm();
        if approx_zero(n) {
            Self::identity()
        } else {
            Self::new(self.w / n, self.x / n, self.y / n, self.z / n)
        }
    }

    /// Conjugado del quaternion
    #[inline]
    pub fn conjugate(&self) -> Self {
        Self::new(self.w, -self.x, -self.y, -self.z)
    }

    /// Inverso del quaternion
    #[inline]
    pub fn inverse(&self) -> Self {
        let n2 = self.norm_squared();
        let conj = self.conjugate();
        Self::new(conj.w / n2, conj.x / n2, conj.y / n2, conj.z / n2)
    }

    /// Producto punto entre quaterniones
    #[inline]
    pub fn dot(&self, other: &Self) -> Real {
        self.w * other.w + self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Rota un vector 3D
    pub fn rotate_vector(&self, v: &Vector3) -> Vector3 {
        // q * v * q^(-1) optimizado
        let qv = Vector3::new(self.x, self.y, self.z);
        let uv = qv.cross(v);
        let uuv = qv.cross(&uv);
        *v + (uv * self.w + uuv) * 2.0
    }

    /// Interpolación lineal esférica (SLERP)
    pub fn slerp(&self, other: &Self, t: Real) -> Self {
        let mut dot = self.dot(other);
        let mut other = *other;

        // Si el producto punto es negativo, invertir uno para tomar el camino corto
        if dot < 0.0 {
            other = -other;
            dot = -dot;
        }

        // Si los quaterniones son muy cercanos, usar interpolación lineal
        if dot > 0.9995 {
            return Self::new(
                self.w + (other.w - self.w) * t,
                self.x + (other.x - self.x) * t,
                self.y + (other.y - self.y) * t,
                self.z + (other.z - self.z) * t,
            ).normalize();
        }

        let theta_0 = dot.acos();
        let theta = theta_0 * t;
        let sin_theta = theta.sin();
        let sin_theta_0 = theta_0.sin();

        let s0 = (theta_0 - theta).cos() - dot * sin_theta / sin_theta_0;
        let s1 = sin_theta / sin_theta_0;

        Self::new(
            self.w * s0 + other.w * s1,
            self.x * s0 + other.x * s1,
            self.y * s0 + other.y * s1,
            self.z * s0 + other.z * s1,
        )
    }

    /// Interpolación lineal normalizada (NLERP) - más rápida que SLERP
    pub fn nlerp(&self, other: &Self, t: Real) -> Self {
        let mut other = *other;
        if self.dot(&other) < 0.0 {
            other = -other;
        }

        Self::new(
            self.w + (other.w - self.w) * t,
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        ).normalize()
    }
}

impl Default for Quaternion {
    fn default() -> Self {
        Self::identity()
    }
}

impl Neg for Quaternion {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(-self.w, -self.x, -self.y, -self.z)
    }
}

impl Mul for Quaternion {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        )
    }
}

impl MulAssign for Quaternion {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{approx_eq, PI};

    #[test]
    fn test_identity() {
        let q = Quaternion::identity();
        let v = Vector3::new(1.0, 2.0, 3.0);
        let rotated = q.rotate_vector(&v);
        assert!(approx_eq(rotated.x(), v.x()));
        assert!(approx_eq(rotated.y(), v.y()));
        assert!(approx_eq(rotated.z(), v.z()));
    }

    #[test]
    fn test_axis_angle_rotation() {
        // Rotar 90 grados alrededor del eje Z
        let q = Quaternion::from_axis_angle(&Vector3::unit_z(), PI / 2.0);
        let v = Vector3::unit_x();
        let rotated = q.rotate_vector(&v);

        assert!(approx_eq(rotated.x(), 0.0));
        assert!(approx_eq(rotated.y(), 1.0));
        assert!(approx_eq(rotated.z(), 0.0));
    }

    #[test]
    fn test_to_from_matrix() {
        let q = Quaternion::from_euler(0.3, 0.5, 0.7);
        let m = q.to_rotation_matrix();
        let q2 = Quaternion::from_rotation_matrix(&m);

        // Los quaterniones q y -q representan la misma rotación
        let dot = q.dot(&q2).abs();
        assert!(approx_eq(dot, 1.0));
    }

    #[test]
    fn test_inverse() {
        let q = Quaternion::from_axis_angle(&Vector3::new(1.0, 2.0, 3.0), 1.0);
        let q_inv = q.inverse();
        let product = q * q_inv;

        assert!(approx_eq(product.w, 1.0));
        assert!(approx_eq(product.x, 0.0));
        assert!(approx_eq(product.y, 0.0));
        assert!(approx_eq(product.z, 0.0));
    }

    #[test]
    fn test_slerp() {
        let q1 = Quaternion::identity();
        let q2 = Quaternion::from_axis_angle(&Vector3::unit_z(), PI);

        // A la mitad del camino debería ser 90 grados
        let q_mid = q1.slerp(&q2, 0.5);
        let v = Vector3::unit_x();
        let rotated = q_mid.rotate_vector(&v);

        assert!(approx_eq(rotated.x(), 0.0));
        assert!(approx_eq(rotated.y(), 1.0));
        assert!(approx_eq(rotated.z(), 0.0));
    }

    #[test]
    fn test_concatenation() {
        // Dos rotaciones de 90 grados alrededor de Z = 180 grados
        let q = Quaternion::from_axis_angle(&Vector3::unit_z(), PI / 2.0);
        let q2 = q * q;
        let v = Vector3::unit_x();
        let rotated = q2.rotate_vector(&v);

        assert!(approx_eq(rotated.x(), -1.0));
        assert!(approx_eq(rotated.y(), 0.0));
        assert!(approx_eq(rotated.z(), 0.0));
    }
}
