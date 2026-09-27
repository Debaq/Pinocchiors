//! Triangulación por ear clipping
//!
//! Algoritmo simple O(n²) para triangular polígonos.

use crate::error::{RepairError, RepairResult};
use super::{
    TriangulationResult,
    project_to_2d,
    is_ccw,
    is_convex_vertex,
    point_in_triangle_2d,
};
use pinocchio_math::Vector3;

/// Triangula un polígono usando ear clipping
///
/// # Algoritmo
///
/// 1. Proyectar el polígono 3D a 2D
/// 2. Asegurar orientación antihoraria
/// 3. Iterar encontrando "orejas" (vértices convexos sin otros vértices dentro)
/// 4. Recortar orejas hasta que quede un triángulo
///
/// Complejidad: O(n²) donde n es el número de vértices
///
/// # Argumentos
///
/// * `vertices` - Vértices del polígono en orden (cerrado)
/// * `normal` - Normal del plano del polígono
pub fn triangulate_ear_clipping(
    vertices: &[Vector3],
    normal: Vector3,
) -> RepairResult<TriangulationResult> {
    let n = vertices.len();

    if n < 3 {
        return Err(RepairError::DegeneratePolygon(n));
    }

    if n == 3 {
        return Ok(TriangulationResult {
            triangles: vec![[0, 1, 2]],
        });
    }

    // Proyectar a 2D
    let (projected, _, _) = project_to_2d(vertices, normal);

    // Determinar si necesitamos invertir el orden
    let ccw = is_ccw(&projected);

    // Crear lista de índices activos
    let mut active: Vec<usize> = if ccw {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };

    let projected: Vec<[f64; 2]> = if ccw {
        projected
    } else {
        projected.into_iter().rev().collect()
    };

    let mut triangles = Vec::with_capacity(n - 2);

    // Ear clipping
    while active.len() > 3 {
        let mut found_ear = false;

        for i in 0..active.len() {
            let prev_i = if i == 0 { active.len() - 1 } else { i - 1 };
            let next_i = (i + 1) % active.len();

            let prev = projected[active[prev_i]];
            let curr = projected[active[i]];
            let next = projected[active[next_i]];

            // Verificar si es convexo
            if !is_convex_vertex(prev, curr, next) {
                continue;
            }

            // Verificar que ningún otro vértice esté dentro del triángulo
            let mut ear_valid = true;
            for j in 0..active.len() {
                if j == prev_i || j == i || j == next_i {
                    continue;
                }

                let p = projected[active[j]];
                if point_in_triangle_2d(p, prev, curr, next) {
                    ear_valid = false;
                    break;
                }
            }

            if ear_valid {
                // Añadir triángulo
                if ccw {
                    triangles.push([active[prev_i], active[i], active[next_i]]);
                } else {
                    // Revertir índices para mantener la orientación original
                    let orig_prev = n - 1 - active[prev_i];
                    let orig_curr = n - 1 - active[i];
                    let orig_next = n - 1 - active[next_i];
                    triangles.push([orig_prev, orig_curr, orig_next]);
                }

                // Remover el vértice
                active.remove(i);
                found_ear = true;
                break;
            }
        }

        if !found_ear {
            // No se encontró oreja válida, el polígono puede ser degenerado
            return Err(RepairError::TriangulationFailed(
                "no se encontró oreja válida".to_string()
            ));
        }
    }

    // Último triángulo
    if active.len() == 3 {
        if ccw {
            triangles.push([active[0], active[1], active[2]]);
        } else {
            triangles.push([n - 1 - active[0], n - 1 - active[1], n - 1 - active[2]]);
        }
    }

    Ok(TriangulationResult { triangles })
}

/// Triangula un polígono simple (sin necesidad de normal)
///
/// Calcula la normal automáticamente usando el método de Newell.
pub fn triangulate_polygon(vertices: &[Vector3]) -> RepairResult<TriangulationResult> {
    if vertices.len() < 3 {
        return Err(RepairError::DegeneratePolygon(vertices.len()));
    }

    let normal = compute_polygon_normal(vertices);
    triangulate_ear_clipping(vertices, normal)
}

/// Calcula la normal de un polígono usando el método de Newell
fn compute_polygon_normal(vertices: &[Vector3]) -> Vector3 {
    let n = vertices.len();
    let mut normal = Vector3::zero();

    for i in 0..n {
        let curr = vertices[i];
        let next = vertices[(i + 1) % n];

        // Newell's method
        normal += Vector3::new(
            (curr.y() - next.y()) * (curr.z() + next.z()),
            (curr.z() - next.z()) * (curr.x() + next.x()),
            (curr.x() - next.x()) * (curr.y() + next.y()),
        );
    }

    normal.try_normalize().unwrap_or(Vector3::new(0.0, 0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangulate_triangle() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];

        let result = triangulate_polygon(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 1);
    }

    #[test]
    fn test_triangulate_quad() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];

        let result = triangulate_polygon(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 2);
    }

    #[test]
    fn test_triangulate_pentagon() {
        // Pentágono regular
        let n = 5;
        let vertices: Vec<Vector3> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                Vector3::new(angle.cos(), angle.sin(), 0.0)
            })
            .collect();

        let result = triangulate_polygon(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 3);  // n - 2 triángulos
    }

    #[test]
    fn test_triangulate_degenerate() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
        ];

        let result = triangulate_polygon(&vertices);
        assert!(result.is_err());
    }

    #[test]
    fn test_triangulate_concave() {
        // Polígono en forma de L
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),
            Vector3::new(2.0, 1.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(1.0, 2.0, 0.0),
            Vector3::new(0.0, 2.0, 0.0),
        ];

        let result = triangulate_polygon(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 4);  // 6 - 2 = 4 triángulos
    }

    #[test]
    fn test_triangulate_3d_polygon() {
        // Cuadrado en el plano XZ
        let vertices = vec![
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(0.0, 1.0, 1.0),
        ];

        let result = triangulate_polygon(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 2);
    }
}
