//! Test de visibilidad mediante ray casting

use pinocchio_math::{Real, Vector3, EPSILON};
use pinocchio_mesh::Mesh;

/// Tester de visibilidad para ray casting
pub struct VisibilityTester<'a> {
    mesh: &'a Mesh,
}

impl<'a> VisibilityTester<'a> {
    /// Crea un nuevo tester de visibilidad
    pub fn new(mesh: &'a Mesh) -> Self {
        Self { mesh }
    }

    /// Verifica si dos puntos son mutuamente visibles
    pub fn is_visible(&self, from: &Vector3, to: &Vector3) -> bool {
        let direction = *to - *from;
        let distance = direction.length();

        if distance < EPSILON {
            return true;
        }

        let dir_normalized = direction / distance;

        // Verificar intersección con cada triángulo
        for face_idx in 0..self.mesh.num_faces() {
            let [v0, v1, v2] = self.mesh.get_face_positions(face_idx);

            if let Some(t) = self.ray_triangle_intersect(from, &dir_normalized, &v0, &v1, &v2) {
                // Si la intersección ocurre antes del destino (con margen)
                if t > EPSILON && t < distance - EPSILON {
                    return false;
                }
            }
        }

        true
    }

    /// Intersección de rayo con triángulo (Möller-Trumbore)
    fn ray_triangle_intersect(
        &self,
        origin: &Vector3,
        direction: &Vector3,
        v0: &Vector3,
        v1: &Vector3,
        v2: &Vector3,
    ) -> Option<Real> {
        let edge1 = *v1 - *v0;
        let edge2 = *v2 - *v0;

        let h = direction.cross(&edge2);
        let a = edge1.dot(&h);

        if a.abs() < EPSILON {
            return None; // Rayo paralelo al triángulo
        }

        let f = 1.0 / a;
        let s = *origin - *v0;
        let u = f * s.dot(&h);

        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(&edge1);
        let v = f * direction.dot(&q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * edge2.dot(&q);

        if t > EPSILON {
            Some(t)
        } else {
            None
        }
    }

    /// Encuentra el punto más cercano de intersección con la malla
    pub fn closest_intersection(
        &self,
        origin: &Vector3,
        direction: &Vector3,
    ) -> Option<(Real, Vector3, usize)> {
        let mut closest: Option<(Real, Vector3, usize)> = None;

        for face_idx in 0..self.mesh.num_faces() {
            let [v0, v1, v2] = self.mesh.get_face_positions(face_idx);

            if let Some(t) = self.ray_triangle_intersect(origin, direction, &v0, &v1, &v2) {
                if t > EPSILON {
                    let should_update = closest.map_or(true, |(best_t, _, _)| t < best_t);
                    if should_update {
                        let point = *origin + *direction * t;
                        closest = Some((t, point, face_idx));
                    }
                }
            }
        }

        closest
    }

    /// Calcula el factor de visibilidad entre un punto y un hueso
    /// Devuelve un valor entre 0 (no visible) y 1 (completamente visible)
    pub fn visibility_factor(
        &self,
        point: &Vector3,
        bone_start: &Vector3,
        bone_end: &Vector3,
        num_samples: usize,
    ) -> Real {
        let mut visible_count = 0;

        for i in 0..num_samples {
            let t = (i as Real + 0.5) / num_samples as Real;
            let bone_point = bone_start.lerp(bone_end, t);

            if self.is_visible(point, &bone_point) {
                visible_count += 1;
            }
        }

        visible_count as Real / num_samples as Real
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_mesh() -> Mesh {
        // Un triángulo en el plano XY
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let indices = vec![[0, 1, 2]];
        Mesh::from_triangles(&positions, &indices)
    }

    #[test]
    fn test_ray_triangle_intersect() {
        let mesh = make_simple_mesh();
        let tester = VisibilityTester::new(&mesh);

        let v0 = Vector3::new(0.0, 0.0, 0.0);
        let v1 = Vector3::new(1.0, 0.0, 0.0);
        let v2 = Vector3::new(0.5, 1.0, 0.0);

        // Rayo que atraviesa el triángulo
        let origin = Vector3::new(0.4, 0.3, -1.0);
        let direction = Vector3::unit_z();

        let result = tester.ray_triangle_intersect(&origin, &direction, &v0, &v1, &v2);
        assert!(result.is_some());
        assert!((result.unwrap() - 1.0).abs() < 1e-6);

        // Rayo que no atraviesa
        let origin2 = Vector3::new(10.0, 10.0, -1.0);
        let result2 = tester.ray_triangle_intersect(&origin2, &direction, &v0, &v1, &v2);
        assert!(result2.is_none());
    }

    #[test]
    fn test_visibility() {
        let mesh = make_simple_mesh();
        let tester = VisibilityTester::new(&mesh);

        // Dos puntos del mismo lado del triángulo
        let p1 = Vector3::new(0.4, 0.3, 1.0);
        let p2 = Vector3::new(0.5, 0.4, 2.0);
        assert!(tester.is_visible(&p1, &p2));

        // Dos puntos en lados opuestos
        let p3 = Vector3::new(0.4, 0.3, -1.0);
        assert!(!tester.is_visible(&p1, &p3));
    }
}
