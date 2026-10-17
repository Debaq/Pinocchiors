//! Bounding Volume Hierarchy para aceleración de consultas de distancia
//!
//! Implementa un BVH binario con AABBs para acelerar queries de
//! distancia punto-malla de O(n) a O(log n).

use pinocchio_math::{Real, Rect, Vector3};
use rayon::prelude::*;

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

    /// Normal geométrica (sin normalizar) según el orden de los vértices
    pub fn normal(&self) -> Vector3 {
        (self.v1 - self.v0).cross(&(self.v2 - self.v0))
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

/// Resultado de [`Bvh::query_closest`]
#[derive(Debug, Clone, Copy)]
pub struct ClosestHit {
    /// Distancia al punto más cercano
    pub distance: Real,
    /// Punto más cercano sobre la malla
    pub point: Vector3,
    /// Índice del triángulo que contiene ese punto
    pub triangle: usize,
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

        // Las cajas se calculan una vez (antes se recalculaban en cada nivel del árbol)
        let tri_bounds: Vec<Rect> = triangles.par_iter().map(Triangle::bounds).collect();
        let mut indices: Vec<usize> = (0..triangles.len()).collect();
        let root = Self::build_recursive(&triangles, &tri_bounds, &mut indices);

        Self {
            root: Some(root),
            triangles,
        }
    }

    /// Por debajo de este tamaño un subárbol se construye en el hilo que lo recibe
    const PAR_BUILD_MIN: usize = 2048;

    /// Construye el subárbol de `indices` (se reordena in situ). El resultado no depende
    /// del número de hilos: mismo eje, mismo orden estable y mismo corte por la mitad.
    fn build_recursive(triangles: &[Triangle], tri_bounds: &[Rect], indices: &mut [usize]) -> BvhNode {
        if indices.len() == 1 {
            let idx = indices[0];
            return BvhNode::Leaf {
                bounds: tri_bounds[idx],
                triangle_idx: idx,
            };
        }

        // Calcular bounds de todos los triángulos
        let bounds = indices.iter()
            .map(|&i| tri_bounds[i])
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

        // Ordenar (de forma estable) por centroide en el eje elegido
        let compare = |&a: &usize, &b: &usize| {
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
        };
        let parallel = indices.len() >= Self::PAR_BUILD_MIN;
        if parallel {
            indices.par_sort_by(compare);
        } else {
            indices.sort_by(compare);
        }

        // Dividir en dos mitades
        let mid = indices.len() / 2;
        let (left_indices, right_indices) = indices.split_at_mut(mid);

        let (left, right) = if parallel {
            rayon::join(
                || Self::build_recursive(triangles, tri_bounds, left_indices),
                || Self::build_recursive(triangles, tri_bounds, right_indices),
            )
        } else {
            (
                Self::build_recursive(triangles, tri_bounds, left_indices),
                Self::build_recursive(triangles, tri_bounds, right_indices),
            )
        };

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

    /// Punto más cercano de la malla a `point`
    pub fn query_closest(&self, point: &Vector3) -> Option<ClosestHit> {
        let root = self.root.as_ref()?;
        let mut best: Option<ClosestHit> = None;
        self.closest_recursive(root, point, &mut best);
        best
    }

    /// Punto más cercano entre los triángulos que miran hacia el mismo lado
    /// que `normal` (producto escalar positivo): en una lámina delgada, la
    /// piel propia y no la de enfrente.
    pub fn query_closest_facing(&self, point: &Vector3, normal: &Vector3) -> Option<ClosestHit> {
        let root = self.root.as_ref()?;
        let mut best: Option<ClosestHit> = None;
        self.closest_filtered(root, point, &mut best, &|t: &Triangle| t.normal().dot(normal) > 0.0);
        best
    }

    fn closest_recursive(&self, node: &BvhNode, point: &Vector3, best: &mut Option<ClosestHit>) {
        self.closest_filtered(node, point, best, &|_: &Triangle| true)
    }

    fn closest_filtered(&self, node: &BvhNode, point: &Vector3, best: &mut Option<ClosestHit>, keep: &dyn Fn(&Triangle) -> bool) {
        let best_dist = best.map_or(Real::INFINITY, |h| h.distance);
        if Self::get_bounds(node).distance_to_point(point) >= best_dist {
            return;
        }
        match node {
            BvhNode::Leaf { triangle_idx, .. } => {
                let tri = &self.triangles[*triangle_idx];
                if !keep(tri) {
                    return;
                }
                let closest = closest_point_on_triangle(point, &tri.v0, &tri.v1, &tri.v2);
                let distance = point.distance(&closest);
                if distance < best_dist {
                    *best = Some(ClosestHit { distance, point: closest, triangle: *triangle_idx });
                }
            }
            BvhNode::Internal { left, right, .. } => {
                let left_dist = Self::get_bounds(left).distance_to_point(point);
                let right_dist = Self::get_bounds(right).distance_to_point(point);
                if left_dist < right_dist {
                    self.closest_filtered(left, point, best, keep);
                    self.closest_filtered(right, point, best, keep);
                } else {
                    self.closest_filtered(right, point, best, keep);
                    self.closest_filtered(left, point, best, keep);
                }
            }
        }
    }

    /// Indica si el segmento `a → b` corta algún triángulo.
    ///
    /// Se ignoran los cortes a menos de `margin` (fracción de la longitud del
    /// segmento, en `[0, 0.5)`) de cada extremo, para que un vértice de la
    /// malla no quede bloqueado por sus propios triángulos adyacentes.
    pub fn segment_intersects(&self, a: &Vector3, b: &Vector3, margin: Real) -> bool {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        let dir = *b - *a;
        if dir.length_squared() == 0.0 {
            return false;
        }
        let inv_dir = Vector3::new(1.0 / dir.x(), 1.0 / dir.y(), 1.0 / dir.z());
        self.any_hit_recursive(root, a, &dir, &inv_dir, margin, 1.0 - margin)
    }

    fn any_hit_recursive(
        &self,
        node: &BvhNode,
        origin: &Vector3,
        dir: &Vector3,
        inv_dir: &Vector3,
        t_min: Real,
        t_max: Real,
    ) -> bool {
        if !ray_hits_bounds(Self::get_bounds(node), origin, inv_dir, t_min, t_max) {
            return false;
        }
        match node {
            BvhNode::Leaf { triangle_idx, .. } => {
                ray_triangle_t(origin, dir, &self.triangles[*triangle_idx])
                    .is_some_and(|t| t > t_min && t < t_max)
            }
            BvhNode::Internal { left, right, .. } => {
                self.any_hit_recursive(left, origin, dir, inv_dir, t_min, t_max)
                    || self.any_hit_recursive(right, origin, dir, inv_dir, t_min, t_max)
            }
        }
    }

    /// Distancia al primer triángulo que corta el rayo `origin + t·dir`
    /// (`dir` unitario) con `t` en `(t_min, t_max)`.
    pub fn ray_distance(&self, origin: &Vector3, dir: &Vector3, t_min: Real, t_max: Real) -> Option<Real> {
        self.ray_hit(origin, dir, t_min, t_max).map(|(t, _)| t)
    }

    /// Primer corte del rayo `origin + t·dir` con `t` en `(t_min, t_max)`:
    /// su `t` y el índice del triángulo. Con `dir` unitario, `t` es la
    /// distancia.
    pub fn ray_hit(&self, origin: &Vector3, dir: &Vector3, t_min: Real, t_max: Real) -> Option<(Real, usize)> {
        let root = self.root.as_ref()?;
        let inv_dir = Vector3::new(1.0 / dir.x(), 1.0 / dir.y(), 1.0 / dir.z());
        let mut best = (t_max, usize::MAX);
        self.first_hit_recursive(root, origin, dir, &inv_dir, t_min, &mut best);
        (best.0 < t_max).then_some(best)
    }

    fn first_hit_recursive(
        &self,
        node: &BvhNode,
        origin: &Vector3,
        dir: &Vector3,
        inv_dir: &Vector3,
        t_min: Real,
        best: &mut (Real, usize),
    ) {
        if !ray_hits_bounds(Self::get_bounds(node), origin, inv_dir, t_min, best.0) {
            return;
        }
        match node {
            BvhNode::Leaf { triangle_idx, .. } => {
                if let Some(t) = ray_triangle_t(origin, dir, &self.triangles[*triangle_idx])
                    && t > t_min
                    && t < best.0
                {
                    *best = (t, *triangle_idx);
                }
            }
            BvhNode::Internal { left, right, .. } => {
                self.first_hit_recursive(left, origin, dir, inv_dir, t_min, best);
                self.first_hit_recursive(right, origin, dir, inv_dir, t_min, best);
            }
        }
    }

    /// Triángulo por índice
    pub fn triangle(&self, idx: usize) -> &Triangle {
        &self.triangles[idx]
    }

    /// Número de triángulos en el BVH
    pub fn num_triangles(&self) -> usize {
        self.triangles.len()
    }
}

/// Distancia de un punto a un triángulo
pub(crate) fn point_triangle_distance(p: &Vector3, v0: &Vector3, v1: &Vector3, v2: &Vector3) -> Real {
    p.distance(&closest_point_on_triangle(p, v0, v1, v2))
}

/// Punto más cercano a `p` sobre el triángulo `abc`
///
/// Algoritmo de Ericson, *Real-Time Collision Detection* (5.1.5): clasifica
/// `p` en las regiones de Voronoi de vértices, aristas y cara.
pub fn closest_point_on_triangle(p: &Vector3, a: &Vector3, b: &Vector3, c: &Vector3) -> Vector3 {
    let ab = *b - *a;
    let ac = *c - *a;
    let ap = *p - *a;

    let d1 = ab.dot(&ap);
    let d2 = ac.dot(&ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return *a;
    }

    let bp = *p - *b;
    let d3 = ab.dot(&bp);
    let d4 = ac.dot(&bp);
    if d3 >= 0.0 && d4 <= d3 {
        return *b;
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return *a + ab * v;
    }

    let cp = *p - *c;
    let d5 = ab.dot(&cp);
    let d6 = ac.dot(&cp);
    if d6 >= 0.0 && d5 <= d6 {
        return *c;
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return *a + ac * w;
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return *b + (*c - *b) * w;
    }

    let denom = va + vb + vc;
    if denom.abs() < Real::MIN_POSITIVE {
        // Triángulo degenerado: devolver el vértice más cercano
        let mut best = *a;
        for q in [*b, *c] {
            if p.distance_squared(&q) < p.distance_squared(&best) {
                best = q;
            }
        }
        return best;
    }
    let v = vb / denom;
    let w = vc / denom;
    *a + ab * v + ac * w
}

/// Intersección rayo-triángulo (Möller-Trumbore). Devuelve `t` tal que el punto
/// de corte es `origin + dir * t` (sin normalizar `dir`).
fn ray_triangle_t(origin: &Vector3, dir: &Vector3, tri: &Triangle) -> Option<Real> {
    let e1 = tri.v1 - tri.v0;
    let e2 = tri.v2 - tri.v0;
    let h = dir.cross(&e2);
    let det = e1.dot(&h);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = *origin - tri.v0;
    // Tolerancia en baricéntricas (no depende de la escala): un rayo que pasa
    // justo por una arista o un vértice compartido le pega a algún triángulo
    // aunque el redondeo lo deje apenas afuera de todos
    const EDGE: Real = 1e-9;
    let u = inv * s.dot(&h);
    if !(-EDGE..=1.0 + EDGE).contains(&u) {
        return None;
    }
    let q = s.cross(&e1);
    let v = inv * dir.dot(&q);
    if v < -EDGE || u + v > 1.0 + EDGE {
        return None;
    }
    Some(inv * e2.dot(&q))
}

/// Test rayo-AABB por slabs restringido a `[t_min, t_max]`
fn ray_hits_bounds(bounds: &Rect, origin: &Vector3, inv_dir: &Vector3, t_min: Real, t_max: Real) -> bool {
    let mut lo = t_min;
    let mut hi = t_max;
    for axis in 0..3 {
        let (o, inv, min, max) = match axis {
            0 => (origin.x(), inv_dir.x(), bounds.min.x(), bounds.max.x()),
            1 => (origin.y(), inv_dir.y(), bounds.min.y(), bounds.max.y()),
            _ => (origin.z(), inv_dir.z(), bounds.min.z(), bounds.max.z()),
        };
        // Rayo paralelo a este eje (componente ±0, inversa ±inf): toca la caja
        // solo si el origen está dentro de su franja. Sin este caso, un origen
        // justo en el borde daba 0 · inf = NaN, y con −0 la caja se descartaba
        if inv.is_infinite() {
            if o < min || o > max {
                return false;
            }
            continue;
        }
        let mut t0 = (min - o) * inv;
        let mut t1 = (max - o) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        lo = lo.max(t0);
        hi = hi.min(t1);
        if lo > hi {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El algoritmo de construcción original (ordenar por completo en cada nivel, en un hilo)
    fn build_reference(triangles: &[Triangle], mut indices: Vec<usize>) -> BvhNode {
        if indices.len() == 1 {
            let idx = indices[0];
            return BvhNode::Leaf { bounds: triangles[idx].bounds(), triangle_idx: idx };
        }
        let bounds = indices.iter().map(|&i| triangles[i].bounds()).fold(Rect::empty(), |acc, b| acc.union(&b));
        let size = bounds.size();
        let axis = if size.x() >= size.y() && size.x() >= size.z() { 0 } else if size.y() >= size.z() { 1 } else { 2 };
        indices.sort_by(|&a, &b| {
            let (ca, cb) = (&triangles[a].centroid, &triangles[b].centroid);
            let pick = |c: &Vector3| match axis { 0 => c.x(), 1 => c.y(), _ => c.z() };
            pick(ca).partial_cmp(&pick(cb)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mid = indices.len() / 2;
        let (l, r) = indices.split_at(mid);
        BvhNode::Internal {
            bounds,
            left: Box::new(build_reference(triangles, l.to_vec())),
            right: Box::new(build_reference(triangles, r.to_vec())),
        }
    }

    fn same_tree(a: &BvhNode, b: &BvhNode) -> bool {
        match (a, b) {
            (BvhNode::Leaf { bounds: ba, triangle_idx: ia }, BvhNode::Leaf { bounds: bb, triangle_idx: ib }) => ia == ib && ba == bb,
            (BvhNode::Internal { bounds: ba, left: la, right: ra }, BvhNode::Internal { bounds: bb, left: lb, right: rb }) => {
                ba == bb && same_tree(la, lb) && same_tree(ra, rb)
            }
            _ => false,
        }
    }

    /// Rejilla regular de triángulos: muchos centroides empatan en cada eje
    fn grid_triangles(n: usize) -> Vec<Triangle> {
        let mut out = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let (x, z) = (i as Real, j as Real);
                let (a, b, c, d) = (
                    Vector3::new(x, 0.0, z),
                    Vector3::new(x + 1.0, 0.0, z),
                    Vector3::new(x, 0.0, z + 1.0),
                    Vector3::new(x + 1.0, 0.0, z + 1.0),
                );
                out.push(Triangle::new(a, b, c));
                out.push(Triangle::new(b, d, c));
            }
        }
        out
    }

    #[test]
    fn parallel_build_gives_the_same_tree_as_the_original_algorithm() {
        // 2 · 70² = 9800 triángulos: supera PAR_BUILD_MIN y hay empates en todos los ejes
        let triangles = grid_triangles(70);
        let reference = build_reference(&triangles, (0..triangles.len()).collect());
        let bvh = Bvh::build(triangles);
        assert!(same_tree(bvh.root.as_ref().unwrap(), &reference), "el árbol cambió");
    }


    #[test]
    fn ray_with_negative_zero_sees_boxes_touching_its_origin() {
        // Triángulo con una arista en x = 0: su caja empieza justo en el
        // origen del rayo, que viaja paralelo a esa cara
        let bvh = Bvh::build(vec![Triangle::new(
            Vector3::new(0.0, -5.0, 1.0),
            Vector3::new(5.0, 0.0, 1.0),
            Vector3::new(0.0, 5.0, 1.0),
        )]);
        let origin = Vector3::new(0.0, 0.0, 0.0);
        for dir in [Vector3::new(0.0, 0.0, 1.0), Vector3::new(-0.0, -0.0, 1.0)] {
            let t = bvh.ray_distance(&origin, &dir, 0.0, 10.0);
            assert!(t.is_some_and(|t| (t - 1.0).abs() < 1e-12), "{dir:?}: {t:?}");
        }
        // Paralelo pero fuera de la franja de la caja: no la toca
        let outside = Vector3::new(-1.0, 0.0, 0.0);
        assert!(bvh.ray_distance(&outside, &Vector3::new(-0.0, 0.0, 1.0), 0.0, 10.0).is_none());
    }

    #[test]
    fn ray_distance_finds_the_first_hit() {
        // Dos planos paralelos z = 1 y z = 3
        let plane = |z: Real| {
            [
                Triangle::new(Vector3::new(-5.0, -5.0, z), Vector3::new(5.0, -5.0, z), Vector3::new(5.0, 5.0, z)),
                Triangle::new(Vector3::new(-5.0, -5.0, z), Vector3::new(5.0, 5.0, z), Vector3::new(-5.0, 5.0, z)),
            ]
        };
        let bvh = Bvh::build(plane(3.0).into_iter().chain(plane(1.0)).collect());
        let up = Vector3::new(0.0, 0.0, 1.0);
        let t = bvh.ray_distance(&Vector3::new(0.3, 0.2, 0.0), &up, 0.0, 10.0).unwrap();
        assert!((t - 1.0).abs() < 1e-12);
        // Desde un plano, ignorando el propio con t_min
        let t = bvh.ray_distance(&Vector3::new(0.3, 0.2, 1.0), &up, 1e-6, 10.0).unwrap();
        assert!((t - 2.0).abs() < 1e-12);
        assert!(bvh.ray_distance(&Vector3::new(0.3, 0.2, 1.0), &up, 1e-6, 1.5).is_none());
        // Justo por la arista compartida de los dos triángulos del plano
        let t = bvh.ray_distance(&Vector3::new(0.0, 0.0, 0.0), &up, 0.0, 10.0).unwrap();
        assert!((t - 1.0).abs() < 1e-12, "{t}");
        // El triángulo del corte es uno del plano z = 1 (índices 2 y 3)
        let (t, tri) = bvh.ray_hit(&Vector3::new(0.3, 0.2, 0.0), &up, 0.0, 10.0).unwrap();
        assert!((t - 1.0).abs() < 1e-12 && (2..4).contains(&tri), "{t} {tri}");
        let down = Vector3::new(0.0, 0.0, -1.0);
        assert!(bvh.ray_distance(&Vector3::new(0.3, 0.2, 0.5), &down, 0.0, 10.0).is_none());
    }

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

    /// Punto más cercano por fuerza bruta (muestreo baricéntrico denso)
    fn brute_force_distance(p: &Vector3, a: &Vector3, b: &Vector3, c: &Vector3) -> Real {
        let n = 400;
        let mut best = Real::INFINITY;
        for i in 0..=n {
            for j in 0..=(n - i) {
                let u = i as Real / n as Real;
                let v = j as Real / n as Real;
                let q = *a + (*b - *a) * u + (*c - *a) * v;
                best = best.min(p.distance(&q));
            }
        }
        best
    }

    #[test]
    fn test_closest_point_all_regions() {
        let a = Vector3::new(0.0, 0.0, 0.0);
        let b = Vector3::new(2.0, 0.0, 0.0);
        let c = Vector3::new(0.5, 1.5, 0.0);
        // Puntos en regiones de vértices, aristas y cara, dentro y fuera del plano
        let points = [
            Vector3::new(-1.0, -1.0, 0.3),
            Vector3::new(3.0, -0.5, -0.2),
            Vector3::new(0.4, 3.0, 0.5),
            Vector3::new(1.0, -1.0, 0.0),
            Vector3::new(1.8, 1.2, 0.4),
            Vector3::new(-0.8, 0.9, -0.3),
            Vector3::new(0.8, 0.5, 1.0),
            Vector3::new(2.5, 0.2, 0.0),
        ];
        for p in &points {
            let fast = point_triangle_distance(p, &a, &b, &c);
            let brute = brute_force_distance(p, &a, &b, &c);
            assert!((fast - brute).abs() < 5e-3, "p={p:?}: {fast} vs {brute}");
        }
    }

    #[test]
    fn test_query_closest_returns_triangle() {
        let triangles = vec![
            Triangle::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)),
            Triangle::new(Vector3::new(5.0, 0.0, 0.0), Vector3::new(6.0, 0.0, 0.0), Vector3::new(5.0, 1.0, 0.0)),
        ];
        let bvh = Bvh::build(triangles);
        let hit = bvh.query_closest(&Vector3::new(5.2, 0.2, 2.0)).unwrap();
        assert_eq!(hit.triangle, 1);
        assert!((hit.distance - 2.0).abs() < 1e-9);
        assert!(hit.point.distance(&Vector3::new(5.2, 0.2, 0.0)) < 1e-9);
    }

    #[test]
    fn test_segment_intersects() {
        let triangles = vec![Triangle::new(
            Vector3::new(-1.0, -1.0, 0.0),
            Vector3::new(1.0, -1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        )];
        let bvh = Bvh::build(triangles);
        // Cruza el triángulo
        assert!(bvh.segment_intersects(&Vector3::new(0.0, 0.0, -1.0), &Vector3::new(0.0, 0.0, 1.0), 1e-4));
        // No llega al triángulo
        assert!(!bvh.segment_intersects(&Vector3::new(0.0, 0.0, 0.5), &Vector3::new(0.0, 0.0, 1.0), 1e-4));
        // Pasa por fuera
        assert!(!bvh.segment_intersects(&Vector3::new(3.0, 0.0, -1.0), &Vector3::new(3.0, 0.0, 1.0), 1e-4));
        // Empieza sobre el triángulo: el margen ignora ese corte
        assert!(!bvh.segment_intersects(&Vector3::new(0.0, 0.0, 0.0), &Vector3::new(0.0, 0.0, 1.0), 1e-4));
    }
}
