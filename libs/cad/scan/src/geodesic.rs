//! Camino más corto sobre la piel de un escaneo (geodésica aproximada): el
//! menor recorrido por las aristas, enderezado después sobre los triángulos
//! de alrededor hasta que deja de acortarse.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use cad_model::geom::{P3, add, norm, scale, sub};

use crate::ScanMesh;
use crate::refs::closest_on_triangle;

#[derive(PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Item {
    // Montículo de mínimos
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}

impl ScanMesh {
    /// Camino sobre la piel por los puntos dados (triángulo y punto en mm de
    /// cada uno), cerrado si `closed`: polilínea de a lo sumo ~100 puntos por
    /// tramo, que empieza y termina en los puntos pedidos.
    pub fn geodesic(&self, stops: &[(u32, P3)], closed: bool) -> Option<Vec<P3>> {
        if stops.len() < 2 || stops.iter().any(|(t, _)| *t as usize >= self.triangles.len()) {
            return None;
        }
        let mut incident: Vec<Vec<u32>> = vec![Vec::new(); self.vertices.len()];
        for (f, t) in self.triangles.iter().enumerate() {
            for &v in t {
                incident[v as usize].push(f as u32);
            }
        }
        let mut legs: Vec<(usize, usize)> = (0..stops.len() - 1).map(|k| (k, k + 1)).collect();
        if closed && stops.len() > 2 {
            legs.push((stops.len() - 1, 0));
        }
        let mut out: Vec<P3> = Vec::new();
        for (a, b) in legs {
            let leg = self.leg(stops[a], stops[b], &incident)?;
            out.extend(leg.into_iter().skip(usize::from(!out.is_empty())));
        }
        Some(out)
    }

    fn leg(&self, from: (u32, P3), to: (u32, P3), incident: &[Vec<u32>]) -> Option<Vec<P3>> {
        let (ta, pa) = from;
        let (tb, pb) = to;
        let straight = ta == tb;
        let mut route = vec![pa];
        let mut near: HashSet<u32> = HashSet::from([ta, tb]);
        if !straight {
            let verts = |t: u32| self.triangles[t as usize];
            let mut dist: HashMap<u32, f64> = HashMap::new();
            let mut prev: HashMap<u32, u32> = HashMap::new();
            let mut heap = BinaryHeap::new();
            for v in verts(ta) {
                let d = norm(sub(self.vertices[v as usize], pa));
                dist.insert(v, d);
                heap.push(Item(d, v as usize));
            }
            let goal: HashSet<u32> = verts(tb).into_iter().collect();
            let mut reached: Option<u32> = None;
            while let Some(Item(d, v)) = heap.pop() {
                let v = v as u32;
                if d > dist[&v] {
                    continue;
                }
                if goal.contains(&v) {
                    // Aún puede ganar otro vértice de llegada si está más cerca de `pb`: se resuelve abajo
                    reached = Some(v);
                    break;
                }
                for &f in &incident[v as usize] {
                    for w in self.triangles[f as usize] {
                        if w == v {
                            continue;
                        }
                        let nd = d + norm(sub(self.vertices[w as usize], self.vertices[v as usize]));
                        if dist.get(&w).is_none_or(|&old| nd < old) {
                            dist.insert(w, nd);
                            prev.insert(w, v);
                            heap.push(Item(nd, w as usize));
                        }
                    }
                }
            }
            let mut v = reached?;
            let mut chain = vec![v];
            while let Some(&p) = prev.get(&v) {
                chain.push(p);
                v = p;
            }
            chain.reverse();
            route.extend(chain.iter().map(|&v| self.vertices[v as usize]));
            for &v in &chain {
                near.extend(incident[v as usize].iter().copied());
            }
        }
        route.push(pb);
        // Un anillo más de triángulos, para que el camino pueda correrse
        let ring: Vec<u32> = near.iter().copied().collect();
        for t in ring {
            for v in self.triangles[t as usize] {
                near.extend(incident[v as usize].iter().copied());
            }
        }
        let set: Vec<[P3; 3]> = near.iter().map(|&t| self.triangles[t as usize].map(|i| self.vertices[i as usize])).collect();
        let onto = |p: P3| -> P3 {
            let mut best = (f64::MAX, p);
            for [a, b, c] in &set {
                let w = closest_on_triangle(p, *a, *b, *c);
                let q = add(add(scale(*a, w[0]), scale(*b, w[1])), scale(*c, w[2]));
                let d = norm(sub(q, p));
                if d < best.0 {
                    best = (d, q);
                }
            }
            best.1
        };
        // Cantidad de puntos según el largo, repartidos parejo, y a enderezar
        let length = |r: &[P3]| r.windows(2).map(|w| norm(sub(w[1], w[0]))).sum::<f64>();
        let count = ((length(&route) / (self.diagonal() * 0.01).max(1e-9)).ceil() as usize).clamp(4, 100);
        let mut pts = resample(&route, count);
        for _ in 0..40 {
            let mut moved = 0.0f64;
            for i in 1..pts.len() - 1 {
                let target = scale(add(pts[i - 1], pts[i + 1]), 0.5);
                let q = onto(target);
                moved = moved.max(norm(sub(q, pts[i])));
                pts[i] = q;
            }
            if moved < length(&pts) * 1e-5 {
                break;
            }
        }
        Some(pts)
    }
}

/// `n` puntos a distancias iguales a lo largo de la polilínea
fn resample(route: &[P3], n: usize) -> Vec<P3> {
    let mut acc = vec![0.0];
    for w in route.windows(2) {
        acc.push(acc.last().unwrap() + norm(sub(w[1], w[0])));
    }
    let total = *acc.last().unwrap();
    if total <= 0.0 {
        return vec![route[0]; n];
    }
    let mut out = Vec::with_capacity(n);
    let mut seg = 0;
    for k in 0..n {
        let s = total * k as f64 / (n - 1) as f64;
        while seg + 2 < route.len() && acc[seg + 1] < s {
            seg += 1;
        }
        let span = (acc[seg + 1] - acc[seg]).max(1e-12);
        let t = ((s - acc[seg]) / span).clamp(0.0, 1.0);
        out.push(add(route[seg], scale(sub(route[seg + 1], route[seg]), t)));
    }
    out
}
