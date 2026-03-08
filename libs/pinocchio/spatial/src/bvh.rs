//! Bounding Volume Hierarchy para aceleración de consultas de distancia
//!
//! Implementa un BVH binario con AABBs para acelerar queries de
//! distancia punto-malla de O(n) a O(log n).

use pinocchio_math::{Real, Rect, Vector3};

/// Nodo del BVH
#[derive(Debug, Clone)]
enum BvhNode {
    /// Nodo hoja con un solo triángulo
    Leaf {
        bounds: Rect,
        triangle_idx: usize,
    },
    /// Nodo interno con dos hijos
    Internal {
        bounds: Rect,
        left: Box<BvhNode>,
        right: Box<BvhNode>,
    },
}

/// Triángulo almacenado con datos precalculados
#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub v0: Vector3,
    pub v1: Vector3,
    pub v2: Vector3,
    pub centroid: Vector3,
}

impl Triangle {
    pub fn new(v0: Vector3, v1: Vector3, v2: Vector3) -> Self {
        let centroid = (v0 + v1 + v2) * (1.0 / 3.0);
        Self { v0, v1, v2, centroid }
    }

    pub fn bounds(&self) -> Rect {
        let min = Vector3::new(
            self.v0.x().min(self.v1.x()).min(self.v2.x()),
            self.v0.y().min(self.v1.y()).min(self.v2.y()),
            self.v0.z().min(self.v1.z()).min(self.v2.z()),
        );
        let max = Vector3::new(
            self.v0.x().max(self.v1.x()).max(self.v2.x()),
            self.v0.y().max(self.v1.y()).max(self.v2.y()),
            self.v0.z().max(self.v1.z()).max(self.v2.z()),
        );
        Rect::new(min, max)
    }
}

/// BVH para consultas rápidas de distancia
#[derive(Debug)]
pub struct Bvh {
    root: Option<BvhNode>,
    triangles: Vec<Triangle>,
}

impl Bvh {
    /// Construye un BVH desde una lista de triángulos
    pub fn build(triangles: Vec<Triangle>) -> Self {
        if triangles.is_empty() {
            return Self { root: None, triangles };
        }

        let indices: Vec<usize> = (0..triangles.len()).collect();
        let root = Self::build_recursive(&triangles, indices);

        Self {
            root: Some(root),
            triangles,
        }
    }

    fn build_recursive(triangles: &[Triangle], indices: Vec<usize>) -> BvhNode {
        if indices.len() == 1 {
            let idx = indices[0];
            return BvhNode::Leaf {
                bounds: triangles[idx].bounds(),
                triangle_idx: idx,
            };
        }

        // Calcular bounds de todos los triángulos
        let bounds = indices.iter()
            .map(|&i| triangles[i].bounds())
            .fold(Rect::empty(), |acc, b| acc.union(&b));

        // Elegir eje de división basado en la extensión máxima
        let size = bounds.size();
        let axis = if size.x() >= size.y() && size.x() >= size.z() {
            0 // X
        } else if size.y() >= size.z() {
            1 // Y
        } else {
            2 // Z
        };

        // Ordenar por centroide en el eje elegido
        let mut sorted_indices = indices;
        sorted_indices.sort_by(|&a, &b| {
            let ca = &triangles[a].centroid;
            let cb = &triangles[b].centroid;
            let va = match axis {
                0 => ca.x(),
                1 => ca.y(),
                _ => ca.z(),
            };
            let vb = match axis {
                0 => cb.x(),
                1 => cb.y(),
                _ => cb.z(),
            };
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Dividir en dos mitades
        let mid = sorted_indices.len() / 2;
        let (left_indices, right_indices) = sorted_indices.split_at(mid);

        let left = Self::build_recursive(triangles, left_indices.to_vec());
        let right = Self::build_recursive(triangles, right_indices.to_vec());

        BvhNode::Internal {
            bounds,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Consulta la distancia mínima desde un punto a la malla
    pub fn query_distance(&self, point: &Vector3) -> Real {
        match &self.root {
            None => Real::INFINITY,
            Some(root) => self.query_recursive(root, point, Real::INFINITY),
        }
    }

    fn query_recursive(&self, node: &BvhNode, point: &Vector3, mut best_dist: Real) -> Real {
        match node {
            BvhNode::Leaf { bounds, triangle_idx } => {
                // Verificar si vale la pena calcular la distancia exacta
                let box_dist = bounds.distance_to_point(point);
                if box_dist >= best_dist {
                    return best_dist;
                }

                let tri = &self.triangles[*triangle_idx];
                let dist = point_triangle_distance(point, &tri.v0, &tri.v1, &tri.v2);
                dist.min(best_dist)
            }
            BvhNode::Internal { bounds, left, right } => {
                // Distancia al AABB
                let box_dist = bounds.distance_to_point(point);
                if box_dist >= best_dist {
                    return best_dist;
                }

                // Calcular distancia a ambos AABBs hijos para determinar orden
                let left_bounds = Self::get_bounds(left);
                let right_bounds = Self::get_bounds(right);
                let left_dist = left_bounds.distance_to_point(point);
                let right_dist = right_bounds.distance_to_point(point);

                // Visitar primero el más cercano
                if left_dist < right_dist {
                    best_dist = self.query_recursive(left, point, best_dist);
                    best_dist = self.query_recursive(right, point, best_dist);
                } else {
                    best_dist = self.query_recursive(right, point, best_dist);
                    best_dist = self.query_recursive(left, point, best_dist);
                }

                best_dist
            }
        }
    }

    fn get_bounds(node: &BvhNode) -> &Rect {
        match node {
            BvhNode::Leaf { bounds, .. } => bounds,
            BvhNode::Internal { bounds, .. } => bounds,
        }
    }

    /// Número de triángulos en el BVH
    pub fn num_triangles(&self) -> usize {
        self.triangles.len()
    }
}

/// Distancia de un punto a un triángulo (copiado de distance_field para evitar dependencia circular)
fn point_triangle_distance(p: &Vector3, v0: &Vector3, v1: &Vector3, v2: &Vector3) -> Real {
    let edge0 = *v1 - *v0;
    let edge1 = *v2 - *v0;
    let v0_to_p = *p - *v0;

    let a = edge0.dot(&edge0);
    let b = edge0.dot(&edge1);
    let c = edge1.dot(&edge1);
    let d = edge0.dot(&v0_to_p);
    let e = edge1.dot(&v0_to_p);

    let det = a * c - b * b;
    let mut s = c * d - b * e;
    let mut t = a * e - b * d;

    if s + t <= det {
        if s < 0.0 {
            if t < 0.0 {
                if d < 0.0 {
                    t = 0.0;
                    s = (-d).min(a).max(0.0);
                } else {
                    s = 0.0;
                    t = (-e).min(c).max(0.0);
                }
            } else {
                s = 0.0;
                t = e.max(0.0).min(c);
                if t > 0.0 { t = -e / c; }
                t = t.clamp(0.0, 1.0);
            }
        } else if t < 0.0 {
            t = 0.0;
            s = d.max(0.0).min(a);
            if s > 0.0 { s = -d / a; }
            s = s.clamp(0.0, 1.0);
        } else {
            let inv_det = 1.0 / det;
            s *= inv_det;
            t *= inv_det;
        }
    } else {
        if s < 0.0 {
            let tmp0 = b + d;
            let tmp1 = c + e;
            if tmp1 > tmp0 {
                let numer = tmp1 - tmp0;
                let denom = a - 2.0 * b + c;
                s = (numer / denom).clamp(0.0, 1.0);
                t = 1.0 - s;
            } else {
                s = 0.0;
                t = (-e / c).clamp(0.0, 1.0);
            }
        } else if t < 0.0 {
            let tmp0 = b + e;
            let tmp1 = a + d;
            if tmp1 > tmp0 {
                let numer = tmp1 - tmp0;
                let denom = a - 2.0 * b + c;
                t = (numer / denom).clamp(0.0, 1.0);
                s = 1.0 - t;
            } else {
                t = 0.0;
                s = (-d / a).clamp(0.0, 1.0);
            }
        } else {
            let numer = (c + e) - (b + d);
            if numer <= 0.0 {
                s = 0.0;
            } else {
                let denom = a - 2.0 * b + c;
                s = (numer / denom).clamp(0.0, 1.0);
            }
            t = 1.0 - s;
        }
    }

    let closest = *v0 + edge0 * s + edge1 * t;
    p.distance(&closest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bvh_single_triangle() {
        let triangles = vec![
            Triangle::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
        ];
        let bvh = Bvh::build(triangles);
        assert_eq!(bvh.num_triangles(), 1);

        // Punto en el triángulo
        let dist = bvh.query_distance(&Vector3::new(0.25, 0.25, 0.0));
        assert!(dist < 0.01);

        // Punto alejado
        let dist2 = bvh.query_distance(&Vector3::new(0.0, 0.0, 5.0));
        assert!((dist2 - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_bvh_multiple_triangles() {
        let triangles = vec![
            Triangle::new(
                Vector3::new(0.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
            Triangle::new(
                Vector3::new(10.0, 0.0, 0.0),
                Vector3::new(11.0, 0.0, 0.0),
                Vector3::new(10.0, 1.0, 0.0),
            ),
        ];
        let bvh = Bvh::build(triangles);

        // Más cerca del primer triángulo
        let dist1 = bvh.query_distance(&Vector3::new(0.0, 0.0, 1.0));
        assert!((dist1 - 1.0).abs() < 0.01);

        // Más cerca del segundo triángulo
        let dist2 = bvh.query_distance(&Vector3::new(10.0, 0.0, 1.0));
        assert!((dist2 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_bvh_empty() {
        let bvh = Bvh::build(vec![]);
        let dist = bvh.query_distance(&Vector3::zero());
        assert!(dist.is_infinite());
    }
}
