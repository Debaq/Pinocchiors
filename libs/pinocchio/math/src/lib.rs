//! Pinocchio Math - Primitivas matemáticas para auto-rigging
//!
//! Este crate proporciona tipos y operaciones matemáticas fundamentales:
//! - Vectores 2D y 3D
//! - Matrices 3x3
//! - Transformaciones (rotación, escala, traslación)
//! - Quaterniones
//! - Bounding boxes (Rect)

mod vector;
mod matrix;
mod transform;
mod rect;
mod quaternion;

pub use vector::{Vector2, Vector3};
pub use matrix::Matrix3;
pub use transform::Transform;
pub use rect::Rect;
pub use quaternion::Quaternion;

/// Re-exportar nalgebra para uso avanzado
pub use nalgebra;

/// Tipo de punto flotante usado en todo el proyecto
pub type Real = f64;

/// Constante PI
pub const PI: Real = std::f64::consts::PI;

/// Tolerancia para comparaciones de punto flotante
pub const EPSILON: Real = 1e-10;

/// Verifica si dos valores son aproximadamente iguales
#[inline]
pub fn approx_eq(a: Real, b: Real) -> bool {
    (a - b).abs() < EPSILON
}

/// Verifica si un valor es aproximadamente cero
#[inline]
pub fn approx_zero(a: Real) -> bool {
    a.abs() < EPSILON
}

/// Clamp un valor entre min y max
#[inline]
pub fn clamp(value: Real, min: Real, max: Real) -> Real {
    value.max(min).min(max)
}

/// Interpolación lineal
#[inline]
pub fn lerp(a: Real, b: Real, t: Real) -> Real {
    a + (b - a) * t
}

/// Convierte grados a radianes
#[inline]
pub fn deg_to_rad(deg: Real) -> Real {
    deg * PI / 180.0
}

/// Convierte radianes a grados
#[inline]
pub fn rad_to_deg(rad: Real) -> Real {
    rad * 180.0 / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approx_eq() {
        assert!(approx_eq(1.0, 1.0 + 1e-11));
        assert!(!approx_eq(1.0, 1.1));
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clamp(-0.5, 0.0, 1.0), 0.0);
        assert_eq!(clamp(1.5, 0.0, 1.0), 1.0);
    }

    #[test]
    fn test_lerp() {
        assert!(approx_eq(lerp(0.0, 10.0, 0.5), 5.0));
        assert!(approx_eq(lerp(0.0, 10.0, 0.0), 0.0));
        assert!(approx_eq(lerp(0.0, 10.0, 1.0), 10.0));
    }

    #[test]
    fn test_deg_rad_conversion() {
        assert!(approx_eq(deg_to_rad(180.0), PI));
        assert!(approx_eq(rad_to_deg(PI), 180.0));
    }
}
