//! Forma de los triángulos: degenerados y de mala calidad.

use pinocchio_math::{Real, Vector3};
use serde::Serialize;

/// Clasificación de un triángulo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FaceQuality {
    /// Bien formado
    Good,
    /// Área nula: sus vértices son (casi) colineales o coincidentes
    Degenerate,
    /// Tiene un ángulo muy agudo; válido pero de mala calidad
    Needle,
    /// Tiene un ángulo casi llano; válido pero de mala calidad
    Cap,
}

/// Medidas de un triángulo
#[derive(Debug, Clone, Copy)]
pub struct FaceShape {
    /// Largo de cada arista local (de la esquina `i` a la `i + 1`)
    pub lengths: [Real; 3],
    /// Arista local más larga
    pub longest: usize,
    /// Arista local más corta
    pub shortest: usize,
    /// Altura sobre la arista más larga
    pub height: Real,
}

impl FaceShape {
    /// Mide un triángulo
    pub fn new(corners: [Vector3; 3]) -> Self {
        let [a, b, c] = corners;
        let lengths = [(b - a).length(), (c - b).length(), (a - c).length()];
        let longest = argmax(&lengths);
        let shortest = argmin(&lengths);
        let double_area = (b - a).cross(&(c - a)).length();
        let height = if lengths[longest] > 0.0 { double_area / lengths[longest] } else { 0.0 };
        Self { lengths, longest, shortest, height }
    }

    /// En una cara degenerada, la esquina opuesta a la arista más larga y la
    /// distancia a su extremo más cercano: `(esquina, extremo, distancia)`
    pub fn nearest_to_apex(&self) -> (usize, usize, Real) {
        let apex = (self.longest + 2) % 3;
        // La arista que sale de la esquina y la que llega a ella
        let (to_next, from_prev) = (self.lengths[apex], self.lengths[(apex + 2) % 3]);
        if to_next <= from_prev {
            (apex, (apex + 1) % 3, to_next)
        } else {
            (apex, (apex + 2) % 3, from_prev)
        }
    }

    /// Si la altura (o la arista más larga) no supera `tolerance`
    pub fn is_degenerate(&self, tolerance: Real) -> bool {
        // Con NaN también es degenerado
        let longest = self.lengths[self.longest];
        self.height.is_nan() || self.height <= tolerance || longest.is_nan() || longest <= tolerance
    }
}

/// Clasifica un triángulo. `tolerance` es absoluta; los ángulos en radianes.
pub fn classify_face(corners: [Vector3; 3], tolerance: Real, needle_angle: Real, cap_angle: Real) -> FaceQuality {
    if FaceShape::new(corners).is_degenerate(tolerance) {
        return FaceQuality::Degenerate;
    }
    let angles = face_angles(corners);
    if angles.iter().any(|&a| a < needle_angle) {
        FaceQuality::Needle
    } else if angles.iter().any(|&a| a > cap_angle) {
        FaceQuality::Cap
    } else {
        FaceQuality::Good
    }
}

/// Ángulos interiores en cada esquina
pub fn face_angles([a, b, c]: [Vector3; 3]) -> [Real; 3] {
    [angle(b - a, c - a), angle(c - b, a - b), angle(a - c, b - c)]
}

/// Ángulo entre dos vectores, robusto cerca de 0 y π
pub fn angle(u: Vector3, v: Vector3) -> Real {
    u.cross(&v).length().atan2(u.dot(&v))
}

fn argmax(l: &[Real; 3]) -> usize {
    if l[0] >= l[1] && l[0] >= l[2] { 0 } else if l[1] >= l[2] { 1 } else { 2 }
}

fn argmin(l: &[Real; 3]) -> usize {
    if l[0] <= l[1] && l[0] <= l[2] { 0 } else if l[1] <= l[2] { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEEDLE: Real = 0.017;
    const CAP: Real = 3.12;

    #[test]
    fn collinear_is_degenerate() {
        let c = [Vector3::zero(), Vector3::new(2.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0)];
        assert_eq!(classify_face(c, 1e-9, NEEDLE, CAP), FaceQuality::Degenerate);
    }

    #[test]
    fn long_thin_triangle_is_needle_not_degenerate() {
        let c = [Vector3::zero(), Vector3::new(100.0, 0.0, 0.0), Vector3::new(100.0, 1.0, 0.0)];
        assert_eq!(classify_face(c, 1e-9, NEEDLE, CAP), FaceQuality::Needle);
    }

    #[test]
    fn equilateral_is_good() {
        let h = 3.0_f64.sqrt() / 2.0;
        let c = [Vector3::zero(), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.5, h, 0.0)];
        assert_eq!(classify_face(c, 1e-9, NEEDLE, CAP), FaceQuality::Good);
        let sum: Real = face_angles(c).iter().sum();
        assert!((sum - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn shape_finds_longest_edge() {
        let s = FaceShape::new([Vector3::zero(), Vector3::new(4.0, 0.0, 0.0), Vector3::new(2.0, 1e-12, 0.0)]);
        assert_eq!(s.longest, 0);
        assert!(s.is_degenerate(1e-9));
    }
}
