//! Bounding boxes (AABB) 3D

use crate::{Real, Vector3, EPSILON};

/// Axis-Aligned Bounding Box (AABB) en 3D
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Esquina mínima (menor x, y, z)
    pub min: Vector3,
    /// Esquina máxima (mayor x, y, z)
    pub max: Vector3,
}

impl Rect {
    /// Crea un Rect desde las esquinas mínima y máxima
    #[inline]
    pub fn new(min: Vector3, max: Vector3) -> Self {
        Self { min, max }
    }

    /// Crea un Rect que contiene un solo punto
    #[inline]
    pub fn from_point(point: Vector3) -> Self {
        Self { min: point, max: point }
    }

    /// Crea un Rect desde centro y half-extents
    #[inline]
    pub fn from_center_half_extents(center: Vector3, half_extents: Vector3) -> Self {
        Self {
            min: center - half_extents,
            max: center + half_extents,
        }
    }

    /// Crea un Rect vacío (invertido, para expansión)
    #[inline]
    pub fn empty() -> Self {
        Self {
            min: Vector3::new(Real::INFINITY, Real::INFINITY, Real::INFINITY),
            max: Vector3::new(Real::NEG_INFINITY, Real::NEG_INFINITY, Real::NEG_INFINITY),
        }
    }

    /// Verifica si el Rect está vacío
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.min.x() > self.max.x() || self.min.y() > self.max.y() || self.min.z() > self.max.z()
    }

    /// Centro del Rect
    #[inline]
    pub fn center(&self) -> Vector3 {
        (self.min + self.max) * 0.5
    }

    /// Tamaño del Rect en cada dimensión
    #[inline]
    pub fn size(&self) -> Vector3 {
        self.max - self.min
    }

    /// Half-extents (mitad del tamaño)
    #[inline]
    pub fn half_extents(&self) -> Vector3 {
        self.size() * 0.5
    }

    /// Volumen del Rect
    #[inline]
    pub fn volume(&self) -> Real {
        let s = self.size();
        s.x() * s.y() * s.z()
    }

    /// Área de superficie del Rect
    #[inline]
    pub fn surface_area(&self) -> Real {
        let s = self.size();
        2.0 * (s.x() * s.y() + s.y() * s.z() + s.z() * s.x())
    }

    /// Diagonal del Rect (longitud de la diagonal)
    #[inline]
    pub fn diagonal(&self) -> Real {
        self.size().length()
    }

    /// Dimensión más larga
    #[inline]
    pub fn longest_axis(&self) -> usize {
        let s = self.size();
        if s.x() > s.y() && s.x() > s.z() {
            0
        } else if s.y() > s.z() {
            1
        } else {
            2
        }
    }

    /// Longitud del eje más largo
    #[inline]
    pub fn longest_axis_length(&self) -> Real {
        self.size().max_component()
    }

    /// Expande el Rect para incluir un punto
    #[inline]
    pub fn expand_to_point(&mut self, point: Vector3) {
        self.min = self.min.min(&point);
        self.max = self.max.max(&point);
    }

    /// Devuelve un nuevo Rect expandido para incluir un punto
    #[inline]
    pub fn expanded_to_point(&self, point: Vector3) -> Self {
        Self {
            min: self.min.min(&point),
            max: self.max.max(&point),
        }
    }

    /// Expande el Rect para incluir otro Rect
    #[inline]
    pub fn expand_to_rect(&mut self, other: &Rect) {
        self.min = self.min.min(&other.min);
        self.max = self.max.max(&other.max);
    }

    /// Devuelve la unión de dos Rects
    #[inline]
    pub fn union(&self, other: &Rect) -> Self {
        Self {
            min: self.min.min(&other.min),
            max: self.max.max(&other.max),
        }
    }

    /// Devuelve la intersección de dos Rects
    #[inline]
    pub fn intersection(&self, other: &Rect) -> Self {
        Self {
            min: self.min.max(&other.min),
            max: self.max.min(&other.max),
        }
    }

    /// Verifica si interseca con otro Rect
    #[inline]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x() <= other.max.x() && self.max.x() >= other.min.x() &&
        self.min.y() <= other.max.y() && self.max.y() >= other.min.y() &&
        self.min.z() <= other.max.z() && self.max.z() >= other.min.z()
    }

    /// Verifica si contiene un punto
    #[inline]
    pub fn contains_point(&self, point: &Vector3) -> bool {
        point.x() >= self.min.x() && point.x() <= self.max.x() &&
        point.y() >= self.min.y() && point.y() <= self.max.y() &&
        point.z() >= self.min.z() && point.z() <= self.max.z()
    }

    /// Verifica si contiene completamente otro Rect
    #[inline]
    pub fn contains_rect(&self, other: &Rect) -> bool {
        self.min.x() <= other.min.x() && self.max.x() >= other.max.x() &&
        self.min.y() <= other.min.y() && self.max.y() >= other.max.y() &&
        self.min.z() <= other.min.z() && self.max.z() >= other.max.z()
    }

    /// Expande el Rect por un margen uniforme
    #[inline]
    pub fn expand(&self, margin: Real) -> Self {
        let m = Vector3::new(margin, margin, margin);
        Self {
            min: self.min - m,
            max: self.max + m,
        }
    }

    /// Contrae el Rect por un margen uniforme
    #[inline]
    pub fn contract(&self, margin: Real) -> Self {
        self.expand(-margin)
    }

    /// Devuelve las 8 esquinas del Rect
    pub fn corners(&self) -> [Vector3; 8] {
        [
            Vector3::new(self.min.x(), self.min.y(), self.min.z()),
            Vector3::new(self.max.x(), self.min.y(), self.min.z()),
            Vector3::new(self.min.x(), self.max.y(), self.min.z()),
            Vector3::new(self.max.x(), self.max.y(), self.min.z()),
            Vector3::new(self.min.x(), self.min.y(), self.max.z()),
            Vector3::new(self.max.x(), self.min.y(), self.max.z()),
            Vector3::new(self.min.x(), self.max.y(), self.max.z()),
            Vector3::new(self.max.x(), self.max.y(), self.max.z()),
        ]
    }

    /// Distancia desde un punto al Rect (0 si está dentro)
    pub fn distance_to_point(&self, point: &Vector3) -> Real {
        let dx = (self.min.x() - point.x()).max(0.0).max(point.x() - self.max.x());
        let dy = (self.min.y() - point.y()).max(0.0).max(point.y() - self.max.y());
        let dz = (self.min.z() - point.z()).max(0.0).max(point.z() - self.max.z());
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Distancia al cuadrado desde un punto al Rect
    pub fn distance_squared_to_point(&self, point: &Vector3) -> Real {
        let dx = (self.min.x() - point.x()).max(0.0).max(point.x() - self.max.x());
        let dy = (self.min.y() - point.y()).max(0.0).max(point.y() - self.max.y());
        let dz = (self.min.z() - point.z()).max(0.0).max(point.z() - self.max.z());
        dx * dx + dy * dy + dz * dz
    }

    /// Clampea un punto al interior del Rect
    #[inline]
    pub fn clamp_point(&self, point: &Vector3) -> Vector3 {
        Vector3::new(
            point.x().clamp(self.min.x(), self.max.x()),
            point.y().clamp(self.min.y(), self.max.y()),
            point.z().clamp(self.min.z(), self.max.z()),
        )
    }

    /// Subdivide el Rect en 8 octantes
    pub fn subdivide(&self) -> [Rect; 8] {
        let c = self.center();
        [
            Rect::new(self.min, c),
            Rect::new(Vector3::new(c.x(), self.min.y(), self.min.z()), Vector3::new(self.max.x(), c.y(), c.z())),
            Rect::new(Vector3::new(self.min.x(), c.y(), self.min.z()), Vector3::new(c.x(), self.max.y(), c.z())),
            Rect::new(Vector3::new(c.x(), c.y(), self.min.z()), Vector3::new(self.max.x(), self.max.y(), c.z())),
            Rect::new(Vector3::new(self.min.x(), self.min.y(), c.z()), Vector3::new(c.x(), c.y(), self.max.z())),
            Rect::new(Vector3::new(c.x(), self.min.y(), c.z()), Vector3::new(self.max.x(), c.y(), self.max.z())),
            Rect::new(Vector3::new(self.min.x(), c.y(), c.z()), Vector3::new(c.x(), self.max.y(), self.max.z())),
            Rect::new(c, self.max),
        ]
    }

    /// Intersección de rayo con el Rect
    /// Devuelve (t_near, t_far) o None si no hay intersección
    pub fn ray_intersect(&self, origin: &Vector3, direction: &Vector3) -> Option<(Real, Real)> {
        let inv_dir = Vector3::new(
            1.0 / direction.x(),
            1.0 / direction.y(),
            1.0 / direction.z(),
        );

        let t1 = (self.min.x() - origin.x()) * inv_dir.x();
        let t2 = (self.max.x() - origin.x()) * inv_dir.x();
        let t3 = (self.min.y() - origin.y()) * inv_dir.y();
        let t4 = (self.max.y() - origin.y()) * inv_dir.y();
        let t5 = (self.min.z() - origin.z()) * inv_dir.z();
        let t6 = (self.max.z() - origin.z()) * inv_dir.z();

        let t_min = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
        let t_max = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

        if t_max < 0.0 || t_min > t_max {
            None
        } else {
            Some((t_min.max(0.0), t_max))
        }
    }

    /// Verifica si dos Rects son aproximadamente iguales
    pub fn approx_eq(&self, other: &Rect) -> bool {
        self.min.distance(&other.min) < EPSILON && self.max.distance(&other.max) < EPSILON
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::empty()
    }
}

/// Construye un Rect desde un iterador de puntos
impl FromIterator<Vector3> for Rect {
    fn from_iter<I: IntoIterator<Item = Vector3>>(iter: I) -> Self {
        let mut rect = Rect::empty();
        for point in iter {
            rect.expand_to_point(point);
        }
        rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approx_eq;

    #[test]
    fn test_basic_properties() {
        let r = Rect::new(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(2.0, 4.0, 6.0),
        );

        assert!(approx_eq(r.center().x(), 1.0));
        assert!(approx_eq(r.center().y(), 2.0));
        assert!(approx_eq(r.center().z(), 3.0));

        assert!(approx_eq(r.size().x(), 2.0));
        assert!(approx_eq(r.size().y(), 4.0));
        assert!(approx_eq(r.size().z(), 6.0));

        assert!(approx_eq(r.volume(), 48.0));
    }

    #[test]
    fn test_contains_point() {
        let r = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));

        assert!(r.contains_point(&Vector3::new(0.5, 0.5, 0.5)));
        assert!(r.contains_point(&Vector3::zero()));
        assert!(r.contains_point(&Vector3::new(1.0, 1.0, 1.0)));
        assert!(!r.contains_point(&Vector3::new(1.5, 0.5, 0.5)));
    }

    #[test]
    fn test_expand() {
        let mut r = Rect::from_point(Vector3::zero());
        r.expand_to_point(Vector3::new(1.0, 1.0, 1.0));
        r.expand_to_point(Vector3::new(-1.0, -1.0, -1.0));

        assert!(approx_eq(r.min.x(), -1.0));
        assert!(approx_eq(r.max.x(), 1.0));
    }

    #[test]
    fn test_intersects() {
        let r1 = Rect::new(Vector3::zero(), Vector3::new(2.0, 2.0, 2.0));
        let r2 = Rect::new(Vector3::new(1.0, 1.0, 1.0), Vector3::new(3.0, 3.0, 3.0));
        let r3 = Rect::new(Vector3::new(5.0, 5.0, 5.0), Vector3::new(6.0, 6.0, 6.0));

        assert!(r1.intersects(&r2));
        assert!(!r1.intersects(&r3));
    }

    #[test]
    fn test_union() {
        let r1 = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));
        let r2 = Rect::new(Vector3::new(2.0, 2.0, 2.0), Vector3::new(3.0, 3.0, 3.0));
        let u = r1.union(&r2);

        assert!(approx_eq(u.min.x(), 0.0));
        assert!(approx_eq(u.max.x(), 3.0));
    }

    #[test]
    fn test_ray_intersect() {
        let r = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));
        let origin = Vector3::new(-1.0, 0.5, 0.5);
        let direction = Vector3::unit_x();

        let result = r.ray_intersect(&origin, &direction);
        assert!(result.is_some());

        let (t_near, t_far) = result.unwrap();
        assert!(approx_eq(t_near, 1.0));
        assert!(approx_eq(t_far, 2.0));
    }

    #[test]
    fn test_distance_to_point() {
        let r = Rect::new(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0));

        // Punto dentro
        assert!(approx_eq(r.distance_to_point(&Vector3::new(0.5, 0.5, 0.5)), 0.0));

        // Punto fuera
        assert!(approx_eq(r.distance_to_point(&Vector3::new(2.0, 0.5, 0.5)), 1.0));
    }

    #[test]
    fn test_from_iterator() {
        let points = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(-1.0, -2.0, -3.0),
        ];

        let r: Rect = points.into_iter().collect();
        assert!(approx_eq(r.min.x(), -1.0));
        assert!(approx_eq(r.max.z(), 3.0));
    }
}
