//! BVH de triángulos con SAH por cubetas.

use crate::scene::Triangle;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub t: f32,
    pub tri: u32,
    /// Coordenadas baricéntricas (peso de v1 y de v2)
    pub u: f32,
    pub v: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) };

    pub fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(&mut self, o: &Aabb) {
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }

    pub fn area(&self) -> f32 {
        let d = (self.max - self.min).max(Vec3::ZERO);
        2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
    }

    /// Distancia de entrada del rayo a la caja, o `None` si no la toca antes de `t_max`
    #[inline]
    fn hit(&self, origin: Vec3, inv_dir: Vec3, t_max: f32) -> Option<f32> {
        let t0 = (self.min - origin) * inv_dir;
        let t1 = (self.max - origin) * inv_dir;
        let tmin = t0.min(t1).max_element().max(0.0);
        let tmax = t0.max(t1).min_element().min(t_max);
        (tmin <= tmax).then_some(tmin)
    }
}

#[derive(Debug, Clone, Copy)]
struct Node {
    bounds: Aabb,
    /// Hoja: primer triángulo; interno: hijo izquierdo (el derecho es `left + 1`)
    first: u32,
    /// 0 = nodo interno; >0 = triángulos de la hoja
    count: u32,
}

#[derive(Debug, Clone)]
pub struct Bvh {
    nodes: Vec<Node>,
    /// Orden de los triángulos en las hojas (índices a la lista original)
    order: Vec<u32>,
}

const BINS: usize = 12;
const MAX_LEAF: usize = 4;

impl Bvh {
    pub fn build(tris: &[Triangle]) -> Bvh {
        let n = tris.len();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let bounds: Vec<Aabb> = tris.iter().map(Triangle::bounds).collect();
        let centers: Vec<Vec3> = bounds.iter().map(|b| (b.min + b.max) * 0.5).collect();
        let mut nodes = Vec::with_capacity(2 * n.max(1));
        nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0 });
        if n > 0 {
            build_node(&mut nodes, 0, &mut order, 0, n, &bounds, &centers);
        }
        Bvh { nodes, order }
    }

    pub fn bounds(&self) -> Aabb {
        self.nodes[0].bounds
    }

    /// Triángulo más cercano que cruza el rayo en (0, t_max)
    pub fn closest(&self, tris: &[Triangle], ray: &Ray, t_max: f32) -> Option<Hit> {
        self.traverse(tris, ray, t_max, false, &mut |_, _, _| true)
    }

    /// ¿Hay algo entre el origen y `t_max`? (rayos de sombra)
    pub fn occluded(&self, tris: &[Triangle], ray: &Ray, t_max: f32) -> bool {
        self.traverse(tris, ray, t_max, true, &mut |_, _, _| true).is_some()
    }

    /// Como [`Bvh::closest`], pero `accept(triángulo, u, v)` puede descartar un cruce
    /// (opacidad: máscara y mezcla)
    pub fn closest_with(
        &self,
        tris: &[Triangle],
        ray: &Ray,
        t_max: f32,
        accept: &mut impl FnMut(u32, f32, f32) -> bool,
    ) -> Option<Hit> {
        self.traverse(tris, ray, t_max, false, accept)
    }

    pub fn occluded_with(
        &self,
        tris: &[Triangle],
        ray: &Ray,
        t_max: f32,
        accept: &mut impl FnMut(u32, f32, f32) -> bool,
    ) -> bool {
        self.traverse(tris, ray, t_max, true, accept).is_some()
    }

    fn traverse(
        &self,
        tris: &[Triangle],
        ray: &Ray,
        mut t_max: f32,
        any: bool,
        accept: &mut impl FnMut(u32, f32, f32) -> bool,
    ) -> Option<Hit> {
        if tris.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / ray.dir.x, 1.0 / ray.dir.y, 1.0 / ray.dir.z);
        let mut best = None;
        let mut stack = [0u32; 64];
        let mut sp = 0;
        let mut cur = 0u32;
        if self.nodes[0].bounds.hit(ray.origin, inv, t_max).is_none() {
            return None;
        }
        loop {
            let node = &self.nodes[cur as usize];
            if node.count > 0 {
                for i in node.first..node.first + node.count {
                    let ti = self.order[i as usize];
                    if let Some((t, u, v)) = tris[ti as usize].intersect(ray, t_max) {
                        if !accept(ti, u, v) {
                            continue;
                        }
                        t_max = t;
                        best = Some(Hit { t, tri: ti, u, v });
                        if any {
                            return best;
                        }
                    }
                }
            } else {
                let l = node.first;
                let dl = self.nodes[l as usize].bounds.hit(ray.origin, inv, t_max);
                let dr = self.nodes[l as usize + 1].bounds.hit(ray.origin, inv, t_max);
                match (dl, dr) {
                    (Some(a), Some(b)) => {
                        let (near, far) = if a <= b { (l, l + 1) } else { (l + 1, l) };
                        stack[sp] = far;
                        sp += 1;
                        cur = near;
                        continue;
                    }
                    (Some(_), None) => {
                        cur = l;
                        continue;
                    }
                    (None, Some(_)) => {
                        cur = l + 1;
                        continue;
                    }
                    (None, None) => {}
                }
            }
            if sp == 0 {
                break;
            }
            sp -= 1;
            cur = stack[sp];
        }
        best
    }
}

fn build_node(
    nodes: &mut Vec<Node>,
    idx: usize,
    order: &mut [u32],
    start: usize,
    end: usize,
    bounds: &[Aabb],
    centers: &[Vec3],
) {
    let mut b = Aabb::EMPTY;
    let mut cb = Aabb::EMPTY;
    for &i in &order[start..end] {
        b.union(&bounds[i as usize]);
        cb.grow(centers[i as usize]);
    }
    let count = end - start;
    nodes[idx].bounds = b;
    let leaf = |nodes: &mut Vec<Node>| {
        nodes[idx].first = start as u32;
        nodes[idx].count = count as u32;
    };
    if count <= MAX_LEAF {
        return leaf(nodes);
    }
    let ext = cb.max - cb.min;
    let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
    if ext[axis] <= 1e-12 {
        return leaf(nodes);
    }

    // SAH por cubetas
    let lo = cb.min[axis];
    let scale = BINS as f32 / ext[axis];
    let bin_of = |c: f32| (((c - lo) * scale) as usize).min(BINS - 1);
    let mut bin_b = [Aabb::EMPTY; BINS];
    let mut bin_n = [0usize; BINS];
    for &i in &order[start..end] {
        let k = bin_of(centers[i as usize][axis]);
        bin_n[k] += 1;
        bin_b[k].union(&bounds[i as usize]);
    }
    let mut left_area = [0f32; BINS - 1];
    let mut left_n = [0usize; BINS - 1];
    let mut acc = Aabb::EMPTY;
    let mut n_acc = 0;
    for k in 0..BINS - 1 {
        acc.union(&bin_b[k]);
        n_acc += bin_n[k];
        left_area[k] = acc.area();
        left_n[k] = n_acc;
    }
    let mut acc = Aabb::EMPTY;
    let mut n_acc = 0;
    let mut best = (f32::INFINITY, 0usize);
    for k in (0..BINS - 1).rev() {
        acc.union(&bin_b[k + 1]);
        n_acc += bin_n[k + 1];
        let cost = left_area[k] * left_n[k] as f32 + acc.area() * n_acc as f32;
        if cost < best.0 {
            best = (cost, k);
        }
    }
    // Si dividir no mejora a una hoja, queda hoja
    let leaf_cost = b.area() * count as f32;
    let split_cost = 0.125 * b.area() * count as f32 + best.0; // 0.125 ≈ costo de recorrer un nodo
    if count <= 16 && split_cost >= leaf_cost {
        return leaf(nodes);
    }

    // Partición in situ
    let mut i = start;
    let mut j = end;
    while i < j {
        if bin_of(centers[order[i] as usize][axis]) <= best.1 {
            i += 1;
        } else {
            j -= 1;
            order.swap(i, j);
        }
    }
    let mid = if i == start || i == end { (start + end) / 2 } else { i };

    let left = nodes.len();
    nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0 });
    nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0 });
    nodes[idx].first = left as u32;
    nodes[idx].count = 0;
    build_node(nodes, left, order, start, mid, bounds, centers);
    build_node(nodes, left + 1, order, mid, end, bounds, centers);
}
