//! Transformaciones 3D (rotación, escala, traslación)

use crate::{Matrix3, Quaternion, Real, Vector3, EPSILON};

/// Transformación afín 3D
///
/// Representa una transformación compuesta de:
/// - Escala (uniforme o no uniforme)
/// - Rotación (quaternion o matriz)
/// - Traslación
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Matriz de rotación (puede incluir escala)
    pub rotation: Matrix3,
    /// Vector de traslación
    pub translation: Vector3,
}

impl Transform {
    /// Crea una transformación desde rotación y traslación
    #[inline]
    pub fn new(rotation: Matrix3, translation: Vector3) -> Self {
        Self { rotation, translation }
    }

    /// Transformación identidad
    #[inline]
    pub fn identity() -> Self {
        Self::new(Matrix3::identity(), Vector3::zero())
    }

    /// Crea una transformación solo con traslación
    #[inline]
    pub fn from_translation(translation: Vector3) -> Self {
        Self::new(Matrix3::identity(), translation)
    }

    /// Crea una transformación solo con rotación (desde matriz)
    #[inline]
    pub fn from_rotation(rotation: Matrix3) -> Self {
        Self::new(rotation, Vector3::zero())
    }

    /// Crea una transformación solo con rotación (desde quaternion)
    #[inline]
    pub fn from_quaternion(q: &Quaternion) -> Self {
        Self::new(q.to_rotation_matrix(), Vector3::zero())
    }

    /// Crea una transformación solo con escala uniforme
    #[inline]
    pub fn from_scale(scale: Real) -> Self {
        Self::new(Matrix3::from_scale(scale), Vector3::zero())
    }

    /// Crea una transformación solo con escala no uniforme
    #[inline]
    pub fn from_scale_nonuniform(sx: Real, sy: Real, sz: Real) -> Self {
        Self::new(Matrix3::from_scale_nonuniform(sx, sy, sz), Vector3::zero())
    }

    /// Crea una transformación desde eje-ángulo y traslación
    pub fn from_axis_angle(axis: &Vector3, angle: Real, translation: Vector3) -> Self {
        Self::new(Matrix3::from_axis_angle(axis, angle), translation)
    }

    /// Transforma un punto (aplica rotación/escala + traslación)
    #[inline]
    pub fn transform_point(&self, point: &Vector3) -> Vector3 {
        self.rotation.transform_vector(point) + self.translation
    }

    /// Transforma un vector/dirección (solo aplica rotación/escala, no traslación)
    #[inline]
    pub fn transform_vector(&self, vector: &Vector3) -> Vector3 {
        self.rotation.transform_vector(vector)
    }

    /// Transforma una normal (usa la transpuesta de la inversa para preservar perpendicularidad)
    pub fn transform_normal(&self, normal: &Vector3) -> Vector3 {
        if let Some(inv) = self.rotation.inverse() {
            let inv_transpose = inv.transpose();
            inv_transpose.transform_vector(normal).normalize()
        } else {
            *normal
        }
    }

    /// Composición de transformaciones: self * other
    /// Aplica primero `other`, luego `self`
    pub fn compose(&self, other: &Transform) -> Transform {
        Transform {
            rotation: self.rotation * other.rotation,
            translation: self.rotation.transform_vector(&other.translation) + self.translation,
        }
    }

    /// Inversa de la transformación
    pub fn inverse(&self) -> Option<Transform> {
        self.rotation.inverse().map(|inv_rot| {
            Transform {
                rotation: inv_rot,
                translation: inv_rot.transform_vector(&(-self.translation)),
            }
        })
    }

    /// Inversa asumiendo que la transformación es invertible
    pub fn inverse_unchecked(&self) -> Transform {
        let inv_rot = self.rotation.inverse_unchecked();
        Transform {
            rotation: inv_rot,
            translation: inv_rot.transform_vector(&(-self.translation)),
        }
    }

    /// Interpolación lineal entre transformaciones
    ///
    /// Nota: Esta interpolación no es ideal para rotaciones grandes.
    /// Para mejor calidad, usar quaterniones directamente.
    pub fn lerp(&self, other: &Transform, t: Real) -> Transform {
        Transform {
            rotation: Matrix3::new(
                crate::lerp(self.rotation.get(0, 0), other.rotation.get(0, 0), t),
                crate::lerp(self.rotation.get(0, 1), other.rotation.get(0, 1), t),
                crate::lerp(self.rotation.get(0, 2), other.rotation.get(0, 2), t),
                crate::lerp(self.rotation.get(1, 0), other.rotation.get(1, 0), t),
                crate::lerp(self.rotation.get(1, 1), other.rotation.get(1, 1), t),
                crate::lerp(self.rotation.get(1, 2), other.rotation.get(1, 2), t),
                crate::lerp(self.rotation.get(2, 0), other.rotation.get(2, 0), t),
                crate::lerp(self.rotation.get(2, 1), other.rotation.get(2, 1), t),
                crate::lerp(self.rotation.get(2, 2), other.rotation.get(2, 2), t),
            ),
            translation: self.translation.lerp(&other.translation, t),
        }
    }

    /// Verifica si la transformación es aproximadamente la identidad
    pub fn is_identity(&self) -> bool {
        self.rotation.is_identity() && self.translation.length_squared() < EPSILON
    }

    /// Extrae la escala de la matriz de rotación (asumiendo escala uniforme)
    pub fn extract_uniform_scale(&self) -> Real {
        // Usar la longitud de la primera columna como aproximación
        self.rotation.column(0).length()
    }

    /// Extrae escalas no uniformes (longitud de cada columna)
    pub fn extract_scale(&self) -> Vector3 {
        Vector3::new(
            self.rotation.column(0).length(),
            self.rotation.column(1).length(),
            self.rotation.column(2).length(),
        )
    }

    /// Extrae la rotación pura (removiendo escala)
    pub fn extract_rotation(&self) -> Matrix3 {
        let scale = self.extract_scale();
        Matrix3::new(
            self.rotation.get(0, 0) / scale.x(),
            self.rotation.get(0, 1) / scale.y(),
            self.rotation.get(0, 2) / scale.z(),
            self.rotation.get(1, 0) / scale.x(),
            self.rotation.get(1, 1) / scale.y(),
            self.rotation.get(1, 2) / scale.z(),
            self.rotation.get(2, 0) / scale.x(),
            self.rotation.get(2, 1) / scale.y(),
            self.rotation.get(2, 2) / scale.z(),
        )
    }

    /// Convierte a matriz 4x4 homogénea (para interoperabilidad)
    pub fn to_matrix4(&self) -> [[Real; 4]; 4] {
        let r = &self.rotation;
        let t = &self.translation;
        [
            [r.get(0, 0), r.get(0, 1), r.get(0, 2), t.x()],
            [r.get(1, 0), r.get(1, 1), r.get(1, 2), t.y()],
            [r.get(2, 0), r.get(2, 1), r.get(2, 2), t.z()],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    /// Crea desde matriz 4x4 homogénea
    pub fn from_matrix4(m: &[[Real; 4]; 4]) -> Self {
        Self {
            rotation: Matrix3::new(
                m[0][0], m[0][1], m[0][2],
                m[1][0], m[1][1], m[1][2],
                m[2][0], m[2][1], m[2][2],
            ),
            translation: Vector3::new(m[0][3], m[1][3], m[2][3]),
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

impl std::ops::Mul for Transform {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        self.compose(&rhs)
    }
}

impl std::ops::Mul<Vector3> for Transform {
    type Output = Vector3;
    fn mul(self, rhs: Vector3) -> Self::Output {
        self.transform_point(&rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{approx_eq, PI};

    #[test]
    fn test_identity() {
        let t = Transform::identity();
        let p = Vector3::new(1.0, 2.0, 3.0);
        let result = t.transform_point(&p);
        assert!(approx_eq(result.x(), p.x()));
        assert!(approx_eq(result.y(), p.y()));
        assert!(approx_eq(result.z(), p.z()));
    }

    #[test]
    fn test_translation() {
        let t = Transform::from_translation(Vector3::new(10.0, 20.0, 30.0));
        let p = Vector3::new(1.0, 2.0, 3.0);
        let result = t.transform_point(&p);
        assert!(approx_eq(result.x(), 11.0));
        assert!(approx_eq(result.y(), 22.0));
        assert!(approx_eq(result.z(), 33.0));
    }

    #[test]
    fn test_rotation() {
        let t = Transform::from_rotation(Matrix3::from_rotation_z(PI / 2.0));
        let p = Vector3::unit_x();
        let result = t.transform_point(&p);
        assert!(approx_eq(result.x(), 0.0));
        assert!(approx_eq(result.y(), 1.0));
        assert!(approx_eq(result.z(), 0.0));
    }

    #[test]
    fn test_scale() {
        let t = Transform::from_scale(2.0);
        let p = Vector3::new(1.0, 2.0, 3.0);
        let result = t.transform_point(&p);
        assert!(approx_eq(result.x(), 2.0));
        assert!(approx_eq(result.y(), 4.0));
        assert!(approx_eq(result.z(), 6.0));
    }

    #[test]
    fn test_composition() {
        let t1 = Transform::from_translation(Vector3::new(1.0, 0.0, 0.0));
        let t2 = Transform::from_scale(2.0);

        // t1 * t2: primero escala, luego traslada
        let composed = t1.compose(&t2);
        let p = Vector3::new(1.0, 1.0, 1.0);
        let result = composed.transform_point(&p);

        // (1,1,1) * 2 = (2,2,2), luego + (1,0,0) = (3,2,2)
        assert!(approx_eq(result.x(), 3.0));
        assert!(approx_eq(result.y(), 2.0));
        assert!(approx_eq(result.z(), 2.0));
    }

    #[test]
    fn test_inverse() {
        let t = Transform::new(
            Matrix3::from_rotation_y(0.5),
            Vector3::new(10.0, 20.0, 30.0),
        );
        let inv = t.inverse().unwrap();
        let composed = t.compose(&inv);

        assert!(composed.is_identity());
    }

    #[test]
    fn test_vector_vs_point() {
        let t = Transform::from_translation(Vector3::new(10.0, 0.0, 0.0));
        let v = Vector3::unit_x();

        // Vector: no se traslada
        let transformed_vector = t.transform_vector(&v);
        assert!(approx_eq(transformed_vector.x(), 1.0));

        // Punto: sí se traslada
        let transformed_point = t.transform_point(&v);
        assert!(approx_eq(transformed_point.x(), 11.0));
    }

    #[test]
    fn test_extract_scale() {
        let t = Transform::from_scale_nonuniform(2.0, 3.0, 4.0);
        let scale = t.extract_scale();
        assert!(approx_eq(scale.x(), 2.0));
        assert!(approx_eq(scale.y(), 3.0));
        assert!(approx_eq(scale.z(), 4.0));
    }
}
