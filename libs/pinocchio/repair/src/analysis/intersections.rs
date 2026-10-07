//! Detección de auto-intersecciones en mallas
//!
//! Usa BVH para acelerar la detección de triángulos que se intersectan.

use crate::trimesh::TriMesh;
use pinocchio_math::{Real, Vector3, Rect};
use serde::Serialize;

/// Una intersección entre dos triángulos
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Intersection {
    /// Índice del primer triángulo
    pub face_a: usize,
    /// Índice del segundo triángulo
    pub face_b: usize,
}

impl Intersection {
    pub fn new(a: usize, b: usize) -> Self {
        // Normalizar para que a < b
        if a < b {
            Self { face_a: a, face_b: b }
        } else {
            Self { face_a: b, face_b: a }
        }
    }
}

/// Resultado del análisis de intersecciones
#[derive(Debug, Clone, Default, Serialize)]
pub struct IntersectionAnalysis {
    /// Lista de pares de triángulos que se intersectan
    pub intersections: Vec<Intersection>,
    /// Número de tests de intersección realizados
    pub tests_performed: usize,
    /// Número de tests evitados por BVH
    pub tests_pruned: usize,
}

impl IntersectionAnalysis {
    /// Verifica si hay alguna intersección
    pub fn has_intersections(&self) -> bool {
        !self.intersections.is_empty()
    }

    /// Número de intersecciones encontradas
    pub fn count(&self) -> usize {
        self.intersections.len()
    }
}

/// Encuentra todas las auto-intersecciones en la malla
///
/// Usa un BVH para acelerar la búsqueda, reduciendo la complejidad de
/// O(n²) a O(n log n) en casos típicos.
///
/// # Argumentos
///
/// * `mesh` - La malla a analizar (conviene soldada: los triángulos que
///   comparten un vértice no se comparan)
/// * `tolerance` - Distancia absoluta bajo la cual un contacto no cuenta como
///   cruce (triángulos que solo se tocan)
///
/// # Algoritmo
///
/// 1. Construir BVH con todos los triángulos
/// 2. Para cada triángulo, encontrar candidatos cuyo AABB se intersecta
/// 3. Filtrar triángulos adyacentes (comparten vértices)
/// 4. Test exacto de intersección triángulo-triángulo
pub fn find_self_intersections(mesh: &TriMesh, tolerance: Real) -> IntersectionAnalysis {
    let num_faces = mesh.num_faces();

    if num_faces < 2 {
        return IntersectionAnalysis::default();
    }

    // Construir datos de triángulos
    let triangles: Vec<TriangleData> = (0..num_faces)
        .map(|i| {
            let [v0, v1, v2] = mesh.triangles[i];
            let [p0, p1, p2] = mesh.corners(i);
            TriangleData {
                vertices: [v0, v1, v2],
                positions: [p0, p1, p2],
                bounds: compute_triangle_bounds(p0, p1, p2, tolerance),
            }
        })
        .collect();

    // Construir BVH simple
    let bvh = SimpleBvh::build(&triangles);

    let mut analysis = IntersectionAnalysis::default();

    // Para cada triángulo, buscar intersecciones
    for i in 0..num_faces {
        let tri_a = &triangles[i];

        // Encontrar candidatos usando BVH
        let candidates = bvh.query_overlapping(&tri_a.bounds);
        analysis.tests_pruned += num_faces - candidates.len();

        for &j in &candidates {
            if i >= j {
                continue; // Evitar duplicados y auto-comparación
            }

            let tri_b = &triangles[j];

            // Filtrar triángulos adyacentes (comparten vértices)
            if triangles_share_vertex(tri_a, tri_b) {
                continue;
            }

            analysis.tests_performed += 1;

            // Test exacto de intersección
            if triangles_intersect(
                tri_a.positions[0], tri_a.positions[1], tri_a.positions[2],
                tri_b.positions[0], tri_b.positions[1], tri_b.positions[2],
                tolerance,
            ) {
                analysis.intersections.push(Intersection::new(i, j));
            }
        }
    }

    analysis
}

/// Pares de triángulos cuyas cajas se solapan y que no comparten una arista:
/// candidatos para cortar las auto-intersecciones (incluye los que comparten
/// un vértice, que [`find_self_intersections`] omite).
pub(crate) fn candidate_pairs(mesh: &TriMesh) -> Vec<(usize, usize)> {
    let triangles: Vec<TriangleData> = (0..mesh.num_faces())
        .map(|i| {
            let [p0, p1, p2] = mesh.corners(i);
            TriangleData { vertices: mesh.triangles[i], positions: [p0, p1, p2], bounds: compute_triangle_bounds(p0, p1, p2, 0.0) }
        })
        .collect();
    let bvh = SimpleBvh::build(&triangles);
    let mut pairs = Vec::new();
    for (i, a) in triangles.iter().enumerate() {
        for j in bvh.query_overlapping(&a.bounds) {
            if j <= i {
                continue;
            }
            let shared = a.vertices.iter().filter(|v| triangles[j].vertices.contains(v)).count();
            if shared < 2 {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// Datos precalculados de un triángulo
struct TriangleData {
    vertices: [usize; 3],
    positions: [Vector3; 3],
    bounds: Rect,
}

/// Calcula el AABB de un triángulo expandido por tolerancia
fn compute_triangle_bounds(p0: Vector3, p1: Vector3, p2: Vector3, tolerance: Real) -> Rect {
    let min = Vector3::new(
        p0.x().min(p1.x()).min(p2.x()) - tolerance,
        p0.y().min(p1.y()).min(p2.y()) - tolerance,
        p0.z().min(p1.z()).min(p2.z()) - tolerance,
    );
    let max = Vector3::new(
        p0.x().max(p1.x()).max(p2.x()) + tolerance,
        p0.y().max(p1.y()).max(p2.y()) + tolerance,
        p0.z().max(p1.z()).max(p2.z()) + tolerance,
    );
    Rect::new(min, max)
}

/// Verifica si dos triángulos comparten al menos un vértice (por índice o por
/// posición exacta: los vértices separados de un punto non-manifold siguen
/// tocándose ahí sin cruzarse)
fn triangles_share_vertex(a: &TriangleData, b: &TriangleData) -> bool {
    for (va, pa) in a.vertices.iter().zip(&a.positions) {
        for (vb, pb) in b.vertices.iter().zip(&b.positions) {
            if va == vb || pa == pb {
                return true;
            }
        }
    }
    false
}

// ============================================================================
// BVH simple para intersección de AABBs
// ============================================================================

struct SimpleBvh {
    nodes: Vec<BvhNode>,
    triangle_indices: Vec<usize>,
}

enum BvhNode {
    Leaf {
        bounds: Rect,
        start: usize,
        count: usize,
    },
    Internal {
        bounds: Rect,
        left: usize,
        right: usize,
    },
}

impl SimpleBvh {
    fn build(triangles: &[TriangleData]) -> Self {
        if triangles.is_empty() {
            return Self {
                nodes: vec![],
                triangle_indices: vec![],
            };
        }

        let mut indices: Vec<usize> = (0..triangles.len()).collect();
        let mut nodes = Vec::new();

        Self::build_recursive(triangles, &mut indices, 0, triangles.len(), &mut nodes);

        Self {
            nodes,
            triangle_indices: indices,
        }
    }

    fn build_recursive(
        triangles: &[TriangleData],
        indices: &mut [usize],
        start: usize,
        end: usize,
        nodes: &mut Vec<BvhNode>,
    ) -> usize {
        let count = end - start;

        // Calcular bounds
        let bounds = indices[start..end].iter()
            .map(|&i| &triangles[i].bounds)
            .fold(Rect::empty(), |acc, b| acc.union(b));

        // Hoja si pocos triángulos
        if count <= 4 {
            let node_idx = nodes.len();
            nodes.push(BvhNode::Leaf {
                bounds,
                start,
                count,
            });
            return node_idx;
        }

        // Elegir eje de división
        let size = bounds.size();
        let axis = if size.x() >= size.y() && size.x() >= size.z() {
            0
        } else if size.y() >= size.z() {
            1
        } else {
            2
        };

        // Ordenar por centroide
        let slice = &mut indices[start..end];
        slice.sort_by(|&a, &b| {
            let ca = triangles[a].bounds.center();
            let cb = triangles[b].bounds.center();
            let va = match axis { 0 => ca.x(), 1 => ca.y(), _ => ca.z() };
            let vb = match axis { 0 => cb.x(), 1 => cb.y(), _ => cb.z() };
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mid = start + count / 2;

        // Reservar espacio para este nodo
        let node_idx = nodes.len();
        nodes.push(BvhNode::Leaf { bounds: Rect::empty(), start: 0, count: 0 }); // Placeholder

        let left = Self::build_recursive(triangles, indices, start, mid, nodes);
        let right = Self::build_recursive(triangles, indices, mid, end, nodes);

        nodes[node_idx] = BvhNode::Internal {
            bounds,
            left,
            right,
        };

        node_idx
    }

    fn query_overlapping(&self, query_bounds: &Rect) -> Vec<usize> {
        let mut result = Vec::new();
        if !self.nodes.is_empty() {
            self.query_recursive(0, query_bounds, &mut result);
        }
        result
    }

    fn query_recursive(&self, node_idx: usize, query_bounds: &Rect, result: &mut Vec<usize>) {
        match &self.nodes[node_idx] {
            BvhNode::Leaf { bounds, start, count } => {
                if bounds.intersects(query_bounds) {
                    for i in *start..(*start + *count) {
                        result.push(self.triangle_indices[i]);
                    }
                }
            }
            BvhNode::Internal { bounds, left, right } => {
                if bounds.intersects(query_bounds) {
                    self.query_recursive(*left, query_bounds, result);
                    self.query_recursive(*right, query_bounds, result);
                }
            }
        }
    }
}

// ============================================================================
// Test de intersección triángulo-triángulo (Möller)
// ============================================================================

/// Test de intersección triángulo-triángulo usando el algoritmo de Möller
fn triangles_intersect(
    a0: Vector3, a1: Vector3, a2: Vector3,
    b0: Vector3, b1: Vector3, b2: Vector3,
    tolerance: Real,
) -> bool {
    // Plano del triángulo B (normal unitaria: las distancias quedan en
    // unidades de longitud y se comparan con `tolerance`). Un triángulo
    // degenerado no define plano; se reporta aparte como degenerado.
    let Some(n2) = (b1 - b0).cross(&(b2 - b0)).try_normalize() else {
        return false;
    };
    let d2 = -n2.dot(&b0);

    // Distancias signadas de vértices de A al plano de B
    let da0 = n2.dot(&a0) + d2;
    let da1 = n2.dot(&a1) + d2;
    let da2 = n2.dot(&a2) + d2;

    // Aplicar tolerancia
    let da0 = if da0.abs() < tolerance { 0.0 } else { da0 };
    let da1 = if da1.abs() < tolerance { 0.0 } else { da1 };
    let da2 = if da2.abs() < tolerance { 0.0 } else { da2 };

    // Si todos los vértices de A están del mismo lado del plano de B, no hay intersección
    if da0 * da1 > 0.0 && da0 * da2 > 0.0 {
        return false;
    }

    // Plano del triángulo A
    let Some(n1) = (a1 - a0).cross(&(a2 - a0)).try_normalize() else {
        return false;
    };
    let d1 = -n1.dot(&a0);

    // Distancias signadas de vértices de B al plano de A
    let db0 = n1.dot(&b0) + d1;
    let db1 = n1.dot(&b1) + d1;
    let db2 = n1.dot(&b2) + d1;

    // Aplicar tolerancia
    let db0 = if db0.abs() < tolerance { 0.0 } else { db0 };
    let db1 = if db1.abs() < tolerance { 0.0 } else { db1 };
    let db2 = if db2.abs() < tolerance { 0.0 } else { db2 };

    // Si todos los vértices de B están del mismo lado del plano de A, no hay intersección
    if db0 * db1 > 0.0 && db0 * db2 > 0.0 {
        return false;
    }

    // Calcular línea de intersección de los dos planos
    let dir = n1.cross(&n2);

    // Si los planos son paralelos (o casi)
    if dir.length_squared() < 1e-18 {
        // Caso coplanar - verificar intersección 2D
        return triangles_intersect_coplanar(a0, a1, a2, b0, b1, b2, n1, tolerance);
    }

    // Planos no paralelos: se decide con predicados exactos (las distancias
    // en coma flotante dan falsos positivos con planos casi paralelos). Solo
    // los contactos exactos (una arista justo sobre otra) usan los intervalos.
    if let Some(cross) = triangles_cross_exact([a0, a1, a2], [b0, b1, b2]) {
        return cross;
    }

    // Proyectar al eje dominante de la línea de intersección
    let axis = if dir.x().abs() >= dir.y().abs() && dir.x().abs() >= dir.z().abs() {
        0
    } else if dir.y().abs() >= dir.z().abs() {
        1
    } else {
        2
    };

    let project = |v: Vector3| match axis {
        0 => v.x(),
        1 => v.y(),
        _ => v.z(),
    };

    // Calcular intervalos de intersección
    let (t_a_min, t_a_max) = compute_interval(
        project(a0), project(a1), project(a2),
        da0, da1, da2,
    );

    let (t_b_min, t_b_max) = compute_interval(
        project(b0), project(b1), project(b2),
        db0, db1, db2,
    );

    // Verificar si los intervalos se solapan
    intervals_overlap(t_a_min, t_a_max, t_b_min, t_b_max, tolerance)
}

/// Si los triángulos se atraviesan: alguna arista de uno cruza el interior
/// del otro, con predicados exactos. `None` si algún predicado da cero
/// (contacto exacto) y no hubo un cruce claro.
fn triangles_cross_exact(a: [Vector3; 3], b: [Vector3; 3]) -> Option<bool> {
    use robust::{orient3d, Coord3D};
    let c = |v: Vector3| Coord3D { x: v.x(), y: v.y(), z: v.z() };
    let mut exact = true;
    let mut edge_crosses = |p: Vector3, q: Vector3, t: [Vector3; 3]| {
        let op = orient3d(c(t[0]), c(t[1]), c(t[2]), c(p));
        let oq = orient3d(c(t[0]), c(t[1]), c(t[2]), c(q));
        if op == 0.0 || oq == 0.0 {
            exact = false;
        }
        if op * oq >= 0.0 {
            return false;
        }
        let s = [
            orient3d(c(p), c(q), c(t[0]), c(t[1])),
            orient3d(c(p), c(q), c(t[1]), c(t[2])),
            orient3d(c(p), c(q), c(t[2]), c(t[0])),
        ];
        if s.contains(&0.0) {
            exact = false;
        }
        s.iter().all(|&x| x > 0.0) || s.iter().all(|&x| x < 0.0)
    };
    for k in 0..3 {
        if edge_crosses(a[k], a[(k + 1) % 3], b) || edge_crosses(b[k], b[(k + 1) % 3], a) {
            return Some(true);
        }
    }
    exact.then_some(false)
}

/// Calcula el intervalo de un triángulo en la línea de intersección
fn compute_interval(
    p0: Real, p1: Real, p2: Real,
    d0: Real, d1: Real, d2: Real,
) -> (Real, Real) {
    // Encontrar qué vértice está solo de un lado
    let (v0, v1, v2, d0, d1, d2) = if d0 * d1 > 0.0 {
        // v2 está solo
        (p2, p0, p1, d2, d0, d1)
    } else if d0 * d2 > 0.0 {
        // v1 está solo
        (p1, p0, p2, d1, d0, d2)
    } else {
        // v0 está solo (o todos en el plano)
        (p0, p1, p2, d0, d1, d2)
    };

    // Calcular puntos de intersección
    let t1 = if (d0 - d1).abs() > 1e-10 {
        v0 + (v1 - v0) * d0 / (d0 - d1)
    } else {
        v0
    };

    let t2 = if (d0 - d2).abs() > 1e-10 {
        v0 + (v2 - v0) * d0 / (d0 - d2)
    } else {
        v0
    };

    if t1 < t2 {
        (t1, t2)
    } else {
        (t2, t1)
    }
}

/// Verifica si dos intervalos se solapan
fn intervals_overlap(a_min: Real, a_max: Real, b_min: Real, b_max: Real, tolerance: Real) -> bool {
    // Estricto: los triángulos que solo se tocan no cuentan
    a_min < b_max - tolerance && b_min < a_max - tolerance
}

/// Test de intersección para triángulos coplanares
#[allow(clippy::too_many_arguments)]
fn triangles_intersect_coplanar(
    a0: Vector3, a1: Vector3, a2: Vector3,
    b0: Vector3, b1: Vector3, b2: Vector3,
    normal: Vector3,
    tolerance: Real,
) -> bool {
    // Proyectar a 2D eliminando el eje con mayor componente de la normal
    let axis = if normal.x().abs() >= normal.y().abs() && normal.x().abs() >= normal.z().abs() {
        0 // Eliminar X
    } else if normal.y().abs() >= normal.z().abs() {
        1 // Eliminar Y
    } else {
        2 // Eliminar Z
    };

    let project_2d = |v: Vector3| -> [Real; 2] {
        match axis {
            0 => [v.y(), v.z()],
            1 => [v.x(), v.z()],
            _ => [v.x(), v.y()],
        }
    };

    let a0_2d = project_2d(a0);
    let a1_2d = project_2d(a1);
    let a2_2d = project_2d(a2);
    let b0_2d = project_2d(b0);
    let b1_2d = project_2d(b1);
    let b2_2d = project_2d(b2);

    // Verificar si alguna arista de A intersecta el triángulo B
    if edge_intersects_triangle_2d(a0_2d, a1_2d, b0_2d, b1_2d, b2_2d, tolerance) ||
       edge_intersects_triangle_2d(a1_2d, a2_2d, b0_2d, b1_2d, b2_2d, tolerance) ||
       edge_intersects_triangle_2d(a2_2d, a0_2d, b0_2d, b1_2d, b2_2d, tolerance) {
        return true;
    }

    // Verificar si alguna arista de B intersecta el triángulo A
    if edge_intersects_triangle_2d(b0_2d, b1_2d, a0_2d, a1_2d, a2_2d, tolerance) ||
       edge_intersects_triangle_2d(b1_2d, b2_2d, a0_2d, a1_2d, a2_2d, tolerance) ||
       edge_intersects_triangle_2d(b2_2d, b0_2d, a0_2d, a1_2d, a2_2d, tolerance) {
        return true;
    }

    // Verificar si un triángulo contiene al otro
    if point_in_triangle_2d(a0_2d, b0_2d, b1_2d, b2_2d) ||
       point_in_triangle_2d(b0_2d, a0_2d, a1_2d, a2_2d) {
        return true;
    }

    false
}

/// Verifica si una arista intersecta un triángulo en 2D
fn edge_intersects_triangle_2d(
    e0: [Real; 2], e1: [Real; 2],
    t0: [Real; 2], t1: [Real; 2], t2: [Real; 2],
    _tolerance: Real,
) -> bool {
    // Verificar intersección con cada arista del triángulo
    segments_intersect_2d(e0, e1, t0, t1) ||
    segments_intersect_2d(e0, e1, t1, t2) ||
    segments_intersect_2d(e0, e1, t2, t0)
}

/// Verifica si dos segmentos se intersectan en 2D
fn segments_intersect_2d(
    a0: [Real; 2], a1: [Real; 2],
    b0: [Real; 2], b1: [Real; 2],
) -> bool {
    let d1 = cross_2d(sub_2d(b1, b0), sub_2d(a0, b0));
    let d2 = cross_2d(sub_2d(b1, b0), sub_2d(a1, b0));
    let d3 = cross_2d(sub_2d(a1, a0), sub_2d(b0, a0));
    let d4 = cross_2d(sub_2d(a1, a0), sub_2d(b1, a0));

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) &&
       ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }

    false
}

fn sub_2d(a: [Real; 2], b: [Real; 2]) -> [Real; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross_2d(a: [Real; 2], b: [Real; 2]) -> Real {
    a[0] * b[1] - a[1] * b[0]
}

/// Verifica si un punto está dentro de un triángulo en 2D
fn point_in_triangle_2d(
    p: [Real; 2],
    t0: [Real; 2], t1: [Real; 2], t2: [Real; 2],
) -> bool {
    let d1 = sign_2d(p, t0, t1);
    let d2 = sign_2d(p, t1, t2);
    let d3 = sign_2d(p, t2, t0);

    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);

    !(has_neg && has_pos)
}

fn sign_2d(p1: [Real; 2], p2: [Real; 2], p3: [Real; 2]) -> Real {
    (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tetrahedron() -> TriMesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
            [1, 3, 2],
            [2, 3, 0],
        ];
        TriMesh::new(vertices, faces)
    }

    #[test]
    fn test_no_self_intersections() {
        let mesh = make_tetrahedron();
        let analysis = find_self_intersections(&mesh, 1e-6);

        assert!(!analysis.has_intersections());
        assert_eq!(analysis.count(), 0);
    }

    #[test]
    fn test_single_triangle_no_intersections() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = TriMesh::new(vertices, faces);

        let analysis = find_self_intersections(&mesh, 1e-6);
        assert!(!analysis.has_intersections());
    }

    #[test]
    fn test_intersecting_triangles() {
        // Dos triángulos que claramente se intersectan
        let vertices = vec![
            // Triángulo A en plano XY
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            // Triángulo B en plano XZ, atravesando A
            Vector3::new(-1.0, 0.5, -1.0),
            Vector3::new(1.0, 0.5, -1.0),
            Vector3::new(0.0, 0.5, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [3, 4, 5],
        ];
        let mesh = TriMesh::new(vertices, faces);

        let analysis = find_self_intersections(&mesh, 1e-6);
        assert!(analysis.has_intersections());
        assert_eq!(analysis.count(), 1);
    }

    #[test]
    fn test_adjacent_triangles_not_intersecting() {
        // Dos triángulos que comparten una arista (no deben reportarse como intersección)
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, -1.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
        ];
        let mesh = TriMesh::new(vertices, faces);

        let analysis = find_self_intersections(&mesh, 1e-6);
        assert!(!analysis.has_intersections());
    }

    #[test]
    fn test_coplanar_intersecting() {
        // Dos triángulos coplanares que se solapan
        let vertices = vec![
            // Triángulo A
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(2.0, 0.0, 0.0),
            Vector3::new(1.0, 2.0, 0.0),
            // Triángulo B (desplazado, solapando)
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(3.0, 0.0, 0.0),
            Vector3::new(2.0, 2.0, 0.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [3, 4, 5],
        ];
        let mesh = TriMesh::new(vertices, faces);

        let analysis = find_self_intersections(&mesh, 1e-6);
        assert!(analysis.has_intersections());
    }

    #[test]
    fn test_parallel_triangles_no_intersection() {
        // Dos triángulos paralelos sin intersección
        let vertices = vec![
            // Triángulo A en z=0
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            // Triángulo B en z=1
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 1.0),
            Vector3::new(0.5, 1.0, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [3, 4, 5],
        ];
        let mesh = TriMesh::new(vertices, faces);

        let analysis = find_self_intersections(&mesh, 1e-6);
        assert!(!analysis.has_intersections());
    }

    #[test]
    fn test_bvh_pruning() {
        // Crear una malla con triángulos muy separados
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        for i in 0..10 {
            let offset = i as f64 * 100.0;
            let base = vertices.len();
            vertices.push(Vector3::new(offset, 0.0, 0.0));
            vertices.push(Vector3::new(offset + 1.0, 0.0, 0.0));
            vertices.push(Vector3::new(offset + 0.5, 1.0, 0.0));
            faces.push([base, base + 1, base + 2]);
        }

        let mesh = TriMesh::new(vertices, faces);
        let analysis = find_self_intersections(&mesh, 1e-6);

        // No hay intersecciones
        assert!(!analysis.has_intersections());
        // El BVH debería haber podado muchos tests
        assert!(analysis.tests_pruned > 0);
    }
}
