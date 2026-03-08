//! Triangulación de Liepa para relleno de agujeros de alta calidad
//!
//! Implementa el algoritmo de Liepa (2003) que produce triangulaciones
//! de mejor calidad que ear clipping, con refinamiento opcional.

use crate::error::{RepairError, RepairResult};
use super::{
    TriangulationResult,
    project_to_2d,
};
use pinocchio_math::Vector3;

/// Configuración para triangulación de Liepa
#[derive(Debug, Clone)]
pub struct LiepaConfig {
    /// Si true, refina la triangulación inicial
    pub refine: bool,
    /// Máximo área de triángulo para refinamiento
    pub max_area: f64,
    /// Número de iteraciones de suavizado
    pub smooth_iterations: usize,
}

impl Default for LiepaConfig {
    fn default() -> Self {
        Self {
            refine: false,
            max_area: f64::INFINITY,
            smooth_iterations: 0,
        }
    }
}

/// Triangula un polígono usando el algoritmo de Liepa
///
/// Este algoritmo produce triangulaciones de mejor calidad que ear clipping,
/// optimizando para triángulos más equiláteros.
///
/// # Algoritmo
///
/// 1. Usa programación dinámica para encontrar la triangulación óptima
/// 2. La métrica de calidad favorece triángulos con ángulos cercanos a 60°
/// 3. Opcionalmente refina y suaviza el resultado
///
/// Complejidad: O(n³) para la triangulación óptima
pub fn triangulate_liepa(
    vertices: &[Vector3],
    normal: Vector3,
    config: &LiepaConfig,
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

    // Proyectar a 2D para cálculos de calidad
    let (projected, _, _) = project_to_2d(vertices, normal);

    // Programación dinámica para triangulación óptima
    // cost[i][j] = costo mínimo para triangular desde vértice i a j
    // split[i][j] = vértice k óptimo para dividir el subproblema

    let mut cost = vec![vec![f64::INFINITY; n]; n];
    let mut split = vec![vec![0usize; n]; n];

    // Base: triángulos
    for i in 0..n {
        let j = (i + 2) % n;
        if (j > i && j - i == 2) || (i > j && n - i + j == 2) {
            cost[i][j] = triangle_cost(&projected, i, (i + 1) % n, j);
            split[i][j] = (i + 1) % n;
        }
    }

    // Llenar la tabla DP para subproblemas más grandes
    for len in 3..n {
        for i in 0..n {
            let j = (i + len) % n;

            // Intentar todos los puntos de división
            for offset in 1..len {
                let k = (i + offset) % n;

                let tri_cost = triangle_cost(&projected, i, k, j);

                // Costo de los subproblemas
                let left_cost = if offset == 1 { 0.0 } else { cost[i][k] };
                let right_cost = if offset == len - 1 { 0.0 } else { cost[k][j] };

                let total = tri_cost + left_cost + right_cost;

                if total < cost[i][j] {
                    cost[i][j] = total;
                    split[i][j] = k;
                }
            }
        }
    }

    // Reconstruir la triangulación
    let mut triangles = Vec::with_capacity(n - 2);
    reconstruct_triangulation(
        &split,
        &mut triangles,
        0,
        n - 1,
        n,
    );

    // Aplicar refinamiento si está configurado
    if config.refine && config.max_area < f64::INFINITY {
        // TODO: Implementar refinamiento por subdivisión
    }

    Ok(TriangulationResult { triangles })
}

/// Costo de un triángulo (menor es mejor)
///
/// Favorece triángulos con ángulos cercanos a 60° (equiláteros)
fn triangle_cost(projected: &[[f64; 2]], i: usize, j: usize, k: usize) -> f64 {
    let a = projected[i];
    let b = projected[j];
    let c = projected[k];

    // Calcular lados
    let ab = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let bc = ((c[0] - b[0]).powi(2) + (c[1] - b[1]).powi(2)).sqrt();
    let ca = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();

    // Área del triángulo
    let area = ((a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1])) / 2.0).abs();

    if area < 1e-10 {
        return f64::INFINITY;  // Triángulo degenerado
    }

    // Perímetro
    let perimeter = ab + bc + ca;

    // Ratio de aspecto: perímetro² / área
    // Menor es mejor (triángulo equilátero tiene el mínimo)
    (perimeter * perimeter) / (4.0 * 3.0_f64.sqrt() * area)
}

/// Reconstruye la triangulación desde la tabla DP
fn reconstruct_triangulation(
    split: &[Vec<usize>],
    triangles: &mut Vec<[usize; 3]>,
    i: usize,
    j: usize,
    n: usize,
) {
    let len = if j >= i { j - i } else { n - i + j };

    if len < 2 {
        return;
    }

    let k = split[i][j];

    // Añadir el triángulo
    triangles.push([i, k, j]);

    // Recursión en subproblemas
    let left_len = if k >= i { k - i } else { n - i + k };
    let right_len = if j >= k { j - k } else { n - k + j };

    if left_len >= 2 {
        reconstruct_triangulation(split, triangles, i, k, n);
    }
    if right_len >= 2 {
        reconstruct_triangulation(split, triangles, k, j, n);
    }
}

/// Triangulación simple usando el método del abanico
///
/// Conecta todos los vértices a un vértice central (el primero).
/// Es O(n) pero produce triángulos de baja calidad para polígonos cóncavos.
pub fn triangulate_fan(vertices: &[Vector3]) -> RepairResult<TriangulationResult> {
    let n = vertices.len();

    if n < 3 {
        return Err(RepairError::DegeneratePolygon(n));
    }

    let triangles: Vec<[usize; 3]> = (1..n - 1)
        .map(|i| [0, i, i + 1])
        .collect();

    Ok(TriangulationResult { triangles })
}

/// Triangulación usando el centroide
///
/// Crea un vértice en el centroide y conecta todos los lados a él.
/// Produce n triángulos para un polígono de n lados.
pub fn triangulate_centroid(
    vertices: &[Vector3],
) -> (Vec<Vector3>, Vec<[usize; 3]>) {
    let n = vertices.len();

    // Calcular centroide
    let centroid: Vector3 = vertices.iter()
        .fold(Vector3::zero(), |acc, v| acc + *v)
        * (1.0 / n as f64);

    // Crear nuevos vértices (originales + centroide)
    let mut new_vertices = vertices.to_vec();
    new_vertices.push(centroid);
    let centroid_idx = new_vertices.len() - 1;

    // Crear triángulos
    let triangles: Vec<[usize; 3]> = (0..n)
        .map(|i| [i, (i + 1) % n, centroid_idx])
        .collect();

    (new_vertices, triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liepa_triangle() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let result = triangulate_liepa(&vertices, normal, &LiepaConfig::default()).unwrap();
        assert_eq!(result.triangles.len(), 1);
    }

    #[test]
    fn test_liepa_quad() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let normal = Vector3::new(0.0, 0.0, 1.0);

        let result = triangulate_liepa(&vertices, normal, &LiepaConfig::default()).unwrap();
        assert_eq!(result.triangles.len(), 2);
    }

    #[test]
    fn test_fan_triangulation() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.5, 1.5, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];

        let result = triangulate_fan(&vertices).unwrap();
        assert_eq!(result.triangles.len(), 3);  // n - 2
    }

    #[test]
    fn test_centroid_triangulation() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];

        let (new_verts, triangles) = triangulate_centroid(&vertices);
        assert_eq!(new_verts.len(), 5);  // 4 originales + centroide
        assert_eq!(triangles.len(), 4);  // n triángulos
    }

    #[test]
    fn test_triangle_cost() {
        // Triángulo equilátero tiene costo mínimo
        // p² / (4*sqrt(3)*area) = 9 / (4*sqrt(3)*sqrt(3)/4) = 9/3 = 3
        let h = 3.0_f64.sqrt() / 2.0;
        let projected_equilateral = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [0.5, h],
        ];

        let cost_equilateral = triangle_cost(&projected_equilateral, 0, 1, 2);
        assert!((cost_equilateral - 3.0).abs() < 0.01);

        // Triángulo muy alargado tiene mayor costo
        let projected_needle = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [5.0, 0.1],
        ];

        let cost_needle = triangle_cost(&projected_needle, 0, 1, 2);
        assert!(cost_needle > cost_equilateral);
    }
}
