//! Vértices coincidentes: rejilla espacial y costura de bordes.

use crate::topology::{self, EdgeTopology, UnionFind};
use pinocchio_math::{Real, Vector3};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hash multiplicativo para celdas de la rejilla (SipHash es innecesariamente
/// lento para claves enteras que no vienen de un atacante)
#[derive(Default)]
struct CellHasher(u64);

impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_i64(&mut self, x: i64) {
        self.write_u64(x as u64);
    }
    fn write_u64(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// Para cada vértice, el índice del representante de su grupo de vértices
/// coincidentes (a distancia ≤ `tolerance`, cerrado transitivamente). El
/// representante es el menor índice del grupo.
///
/// Con `tolerance == 0` solo agrupa posiciones idénticas.
pub fn weld_map(positions: &[Vector3], tolerance: Real) -> Vec<usize> {
    let n = positions.len();
    let mut uf = UnionFind::new(n);

    if tolerance <= 0.0 {
        let mut order: Vec<usize> = (0..n).collect();
        let key = |p: &Vector3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        order.sort_unstable_by_key(|&i| key(&positions[i]));
        for w in order.windows(2) {
            if positions[w[0]] == positions[w[1]] {
                uf.union(w[0], w[1]);
            }
        }
    } else {
        // Celdas de 4·tolerancia: un punto solo puede tener vecinos en otra
        // celda si está a menos de `tolerance` de la cara compartida, así que
        // casi nunca hace falta mirar las vecinas
        let size = 4.0 * tolerance;
        let cell_of = |p: &Vector3| [0, 1, 2].map(|k| (p[k] / size).floor() as i64);
        let mut head: HashMap<[i64; 3], usize, BuildHasherDefault<CellHasher>> =
            HashMap::with_capacity_and_hasher(n, Default::default());
        let mut next = vec![usize::MAX; n];
        for (i, p) in positions.iter().enumerate() {
            if crate::trimesh::is_finite(p)
                && let Some(first) = head.insert(cell_of(p), i)
            {
                next[i] = first;
            }
        }
        let tol_sq = tolerance * tolerance;
        for (a, p) in positions.iter().enumerate() {
            if !crate::trimesh::is_finite(p) {
                continue;
            }
            let cell = cell_of(p);
            // Por eje: -1, 0 o +1 si la celda vecina en ese sentido puede
            // tener puntos a distancia ≤ tolerance
            let reach = [0, 1, 2].map(|k| {
                let lo = p[k] - cell[k] as Real * size <= tolerance;
                let hi = (cell[k] + 1) as Real * size - p[k] <= tolerance;
                [lo.then_some(-1), Some(0), hi.then_some(1)]
            });
            for ox in reach[0].into_iter().flatten() {
                for oy in reach[1].into_iter().flatten() {
                    for oz in reach[2].into_iter().flatten() {
                        let key = [cell[0] + ox, cell[1] + oy, cell[2] + oz];
                        let Some(&first) = head.get(&key) else { continue };
                        let mut b = first;
                        // Cada par se compara desde su menor índice
                        while b != usize::MAX {
                            if b > a && p.distance_squared(&positions[b]) <= tol_sq {
                                uf.union(a, b);
                            }
                            b = next[b];
                        }
                    }
                }
            }
        }
    }

    representatives(&mut uf, n)
}

/// Costura: fusiona vértices coincidentes solo a lo largo de aristas de
/// borde coincidentes.
///
/// Cierra las costuras de un STL sin indexar o de un glTF con vértices
/// duplicados por UV, pero respeta los vértices separados a propósito: dos
/// cuerpos cerrados que se tocan en una arista o un vértice no tienen aristas
/// de borde ahí, así que siguen separados. Devuelve, para cada vértice, el
/// representante (menor índice) de su grupo.
pub fn stitch_map(positions: &[Vector3], triangles: &[[usize; 3]], tolerance: Real) -> Vec<usize> {
    let n = positions.len();
    let mut uf = UnionFind::new(n);

    // Dos pasadas: coser puede alinear aristas que antes no coincidían
    for _ in 0..2 {
        let current = representatives(&mut uf, n);
        let tris: Vec<[usize; 3]> = triangles
            .iter()
            .map(|t| t.map(|v| current[v]))
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        let topo = EdgeTopology::build(&tris);
        let boundary: Vec<(usize, usize)> = (0..topo.num_edges())
            .filter(|&e| topo.valence(e) == 1)
            .map(|e| topology::directed(&tris, topo.faces(e)[0]))
            .collect();
        if boundary.is_empty() {
            break;
        }

        // Grupos de posiciones coincidentes, solo entre vértices de borde
        let mut local = vec![usize::MAX; n];
        let mut subset = Vec::new();
        for &(a, b) in &boundary {
            for v in [a, b] {
                if local[v] == usize::MAX {
                    local[v] = subset.len();
                    subset.push(v);
                }
            }
        }
        let points: Vec<Vector3> = subset.iter().map(|&v| positions[v]).collect();
        let local_group = weld_map(&points, tolerance);
        let group = |v: usize| subset[local_group[local[v]]];

        let mut keyed: Vec<([usize; 2], (usize, usize))> = boundary
            .iter()
            .filter_map(|&(a, b)| {
                let (ga, gb) = (group(a), group(b));
                (ga != gb).then_some(([ga.min(gb), ga.max(gb)], (a, b)))
            })
            .collect();
        keyed.sort_unstable();

        let mut changed = false;
        for run in keyed.chunk_by(|x, y| x.0 == y.0) {
            let (a0, b0) = run[0].1;
            for &(_, (a, b)) in &run[1..] {
                let (a, b) = if group(a) == group(a0) { (a, b) } else { (b, a) };
                changed |= uf.union(a, a0) | uf.union(b, b0);
            }
        }
        if !changed {
            break;
        }
    }
    representatives(&mut uf, n)
}

fn representatives(uf: &mut UnionFind, n: usize) -> Vec<usize> {
    let mut smallest = vec![usize::MAX; n];
    for v in 0..n {
        let r = uf.find(v);
        smallest[r] = smallest[r].min(v);
    }
    (0..n).map(|v| smallest[uf.find(v)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stitch_closes_seams_but_not_touching_fans() {
        // Dos triángulos que comparten la arista (0,0)-(1,0) con vértices propios
        let p = vec![
            Vector3::zero(), Vector3::unit_x(), Vector3::unit_y(),
            Vector3::unit_x(), Vector3::zero(), Vector3::new(0.0, -1.0, 0.0),
            // Tercer triángulo que solo toca el vértice (1,0) desde otro lado
            Vector3::unit_x(), Vector3::new(2.0, 0.0, 1.0), Vector3::new(2.0, 1.0, 1.0),
        ];
        let t = [[0, 1, 2], [3, 4, 5], [6, 7, 8]];
        let map = stitch_map(&p, &t, 1e-9);
        assert_eq!(map[3], 1);
        assert_eq!(map[4], 0);
        assert_eq!(map[6], 6, "un vértice que solo se toca no se cose");
    }

    #[test]
    fn diagonal_neighbours_are_found() {
        // Separados en celdas (1,-1) respecto al otro: el caso que la versión
        // anterior no comparaba
        let t = 0.1;
        let p = vec![Vector3::new(0.099, 0.101, 0.0), Vector3::new(0.101, 0.099, 0.0)];
        assert_eq!(weld_map(&p, t), vec![0, 0]);
    }

    #[test]
    fn exact_mode() {
        let p = vec![Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_x()];
        assert_eq!(weld_map(&p, 0.0), vec![0, 1, 0]);
    }

    #[test]
    fn far_points_stay_apart() {
        let p = vec![Vector3::zero(), Vector3::new(1.0, 0.0, 0.0)];
        assert_eq!(weld_map(&p, 1e-3), vec![0, 1]);
    }
}
