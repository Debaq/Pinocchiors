//! Detección de caras degeneradas

use pinocchio_mesh::Mesh;
use pinocchio_math::Real;
use serde::Serialize;

/// Tipo de cara degenerada
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DegenerateFace {
    /// Cara con área cero o casi cero
    ZeroArea(usize),
    /// Triángulo "aguja" con un ángulo muy pequeño
    Needle(usize),
    /// Triángulo "gorra" con un ángulo muy grande (casi 180°)
    Cap(usize),
}

impl DegenerateFace {
    /// Obtiene el índice de la cara
    pub fn face_index(&self) -> usize {
        match *self {
            DegenerateFace::ZeroArea(i) => i,
            DegenerateFace::Needle(i) => i,
            DegenerateFace::Cap(i) => i,
        }
    }
}

/// Encuentra todas las caras degeneradas en la malla
///
/// Detecta tres tipos de degeneración:
/// - **Zero area**: Caras con área menor al umbral
/// - **Needle**: Triángulos con al menos un ángulo muy pequeño
/// - **Cap**: Triángulos con al menos un ángulo muy grande
///
/// # Argumentos
///
/// * `mesh` - La malla a analizar
/// * `area_threshold` - Umbral de área mínima
/// * `needle_angle` - Ángulo mínimo en radianes para detectar needles
/// * `cap_angle` - Ángulo máximo en radianes para detectar caps
pub fn find_degenerate_faces(
    mesh: &Mesh,
    area_threshold: Real,
    needle_angle: Real,
    cap_angle: Real,
) -> Vec<DegenerateFace> {
    let mut degenerates = Vec::new();

    for face_idx in 0..mesh.num_faces() {
        let area = mesh.get_face_area(face_idx);

        // Detectar área cero
        if area < area_threshold {
            degenerates.push(DegenerateFace::ZeroArea(face_idx));
            continue;
        }

        // Calcular ángulos
        let angles = compute_face_angles(mesh, face_idx);

        // Detectar needle (ángulo muy pequeño)
        if angles.iter().any(|&a| a < needle_angle) {
            degenerates.push(DegenerateFace::Needle(face_idx));
            continue;
        }

        // Detectar cap (ángulo muy grande)
        if angles.iter().any(|&a| a > cap_angle) {
            degenerates.push(DegenerateFace::Cap(face_idx));
        }
    }

    degenerates
}

/// Calcula los tres ángulos internos de un triángulo
fn compute_face_angles(mesh: &Mesh, face_idx: usize) -> [Real; 3] {
    let [p0, p1, p2] = mesh.get_face_positions(face_idx);

    // Vectores de aristas
    let e01 = p1 - p0;
    let e12 = p2 - p1;
    let e20 = p0 - p2;

    // Calcular ángulos usando dot product
    // angle = acos(dot(a, b) / (|a| * |b|))
    let angle0 = angle_between(-e20, e01);
    let angle1 = angle_between(-e01, e12);
    let angle2 = angle_between(-e12, e20);

    [angle0, angle1, angle2]
}

/// Calcula el ángulo entre dos vectores (en radianes)
fn angle_between(a: pinocchio_math::Vector3, b: pinocchio_math::Vector3) -> Real {
    let len_a = a.length();
    let len_b = b.length();

    if len_a < 1e-10 || len_b < 1e-10 {
        return 0.0;
    }

    let cos_angle = a.dot(&b) / (len_a * len_b);
    // Clamp para evitar errores numéricos en acos
    cos_angle.clamp(-1.0, 1.0).acos()
}

/// Calcula el aspect ratio de un triángulo
///
/// El aspect ratio es la relación entre el lado más largo y la altura mínima.
/// Un triángulo equilátero tiene aspect ratio ~1.15.
/// Valores mayores indican triángulos más degenerados.
pub fn compute_aspect_ratio(mesh: &Mesh, face_idx: usize) -> Real {
    let [p0, p1, p2] = mesh.get_face_positions(face_idx);

    // Longitudes de aristas
    let len01 = (p1 - p0).length();
    let len12 = (p2 - p1).length();
    let len20 = (p0 - p2).length();

    let max_edge = len01.max(len12).max(len20);
    let area = mesh.get_face_area(face_idx);

    if area < 1e-10 {
        return Real::INFINITY;
    }

    // Altura mínima = 2 * area / lado_máximo
    // Aspect ratio = lado_máximo / altura_mínima = lado_máximo^2 / (2 * area)
    (max_edge * max_edge) / (2.0 * area)
}

/// Verifica si una cara específica es degenerada
pub fn is_face_degenerate(
    mesh: &Mesh,
    face_idx: usize,
    area_threshold: Real,
    needle_angle: Real,
    cap_angle: Real,
) -> Option<DegenerateFace> {
    let area = mesh.get_face_area(face_idx);

    if area < area_threshold {
        return Some(DegenerateFace::ZeroArea(face_idx));
    }

    let angles = compute_face_angles(mesh, face_idx);

    if angles.iter().any(|&a| a < needle_angle) {
        return Some(DegenerateFace::Needle(face_idx));
    }

    if angles.iter().any(|&a| a > cap_angle) {
        return Some(DegenerateFace::Cap(face_idx));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    #[test]
    fn test_normal_triangle_not_degenerate() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let degenerates = find_degenerate_faces(&mesh, 1e-10, 0.01, 3.13);
        assert!(degenerates.is_empty());
    }

    #[test]
    fn test_zero_area_triangle() {
        // Triángulo con vértices colineales
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let degenerates = find_degenerate_faces(&mesh, 1e-10, 0.01, 3.13);
        assert_eq!(degenerates.len(), 1);
        assert!(matches!(degenerates[0], DegenerateFace::ZeroArea(0)));
    }

    #[test]
    fn test_needle_triangle() {
        // Triángulo muy alargado
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(100.0, 0.0, 0.0),
            Vector3::new(50.0, 0.001, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let degenerates = find_degenerate_faces(&mesh, 1e-10, 0.01, 3.13);
        assert!(!degenerates.is_empty());
        assert!(matches!(degenerates[0], DegenerateFace::Needle(0)));
    }

    #[test]
    fn test_aspect_ratio() {
        // Triángulo equilátero
        let h = 3.0_f64.sqrt() / 2.0;
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, h, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let ar = compute_aspect_ratio(&mesh, 0);
        // Triángulo equilátero tiene aspect ratio ~1.15
        assert!(ar > 1.0 && ar < 1.5);
    }
}
