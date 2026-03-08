//! Algoritmos de triangulación para relleno de agujeros

pub mod ear_clipping;
pub mod liepa;

use pinocchio_math::Vector3;

/// Resultado de una triangulación
#[derive(Debug, Clone)]
pub struct TriangulationResult {
    /// Triángulos generados (índices en el array de vértices del loop)
    pub triangles: Vec<[usize; 3]>,
}

/// Proyecta un polígono 3D a 2D usando su plano dominante
///
/// Devuelve las coordenadas 2D y los ejes del plano para recuperar 3D
pub fn project_to_2d(
    vertices: &[Vector3],
    normal: Vector3,
) -> (Vec<[f64; 2]>, Vector3, Vector3) {
    // Calcular ejes del plano
    let (u_axis, v_axis) = compute_plane_axes(normal);

    // Proyectar vértices
    let projected: Vec<[f64; 2]> = vertices.iter()
        .map(|v| {
            [v.dot(&u_axis), v.dot(&v_axis)]
        })
        .collect();

    (projected, u_axis, v_axis)
}

/// Calcula dos ejes ortogonales en un plano dado su normal
fn compute_plane_axes(normal: Vector3) -> (Vector3, Vector3) {
    // Encontrar un vector no paralelo a la normal
    let up = if normal.x().abs() < 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };

    let u_axis = normal.cross(&up).normalize();
    let v_axis = normal.cross(&u_axis).normalize();

    (u_axis, v_axis)
}

/// Calcula el área signada de un polígono 2D
pub fn signed_area_2d(vertices: &[[f64; 2]]) -> f64 {
    let n = vertices.len();
    let mut area = 0.0;

    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i][0] * vertices[j][1];
        area -= vertices[j][0] * vertices[i][1];
    }

    area * 0.5
}

/// Verifica si un polígono 2D está en sentido antihorario
pub fn is_ccw(vertices: &[[f64; 2]]) -> bool {
    signed_area_2d(vertices) > 0.0
}

/// Verifica si un punto está dentro de un triángulo (2D)
pub fn point_in_triangle_2d(
    p: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> bool {
    let sign = |p1: [f64; 2], p2: [f64; 2], p3: [f64; 2]| -> f64 {
        (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])
    };

    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);

    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);

    !(has_neg && has_pos)
}

/// Calcula el ángulo en un vértice de un polígono (2D)
pub fn vertex_angle_2d(
    prev: [f64; 2],
    curr: [f64; 2],
    next: [f64; 2],
) -> f64 {
    let v1 = [prev[0] - curr[0], prev[1] - curr[1]];
    let v2 = [next[0] - curr[0], next[1] - curr[1]];

    let dot = v1[0] * v2[0] + v1[1] * v2[1];
    let cross = v1[0] * v2[1] - v1[1] * v2[0];

    cross.atan2(dot)
}

/// Verifica si un vértice es convexo (para polígono CCW)
pub fn is_convex_vertex(
    prev: [f64; 2],
    curr: [f64; 2],
    next: [f64; 2],
) -> bool {
    let cross = (curr[0] - prev[0]) * (next[1] - prev[1])
              - (curr[1] - prev[1]) * (next[0] - prev[0]);
    cross > 0.0
}
