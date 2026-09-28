//! Superficie de referencia con UV: triángulos, cartas y consultas de cercanía.

use pinocchio_math::Vector3;
use pinocchio_spatial::{closest_point_on_triangle, Bvh, ClosestHit, Triangle};
use std::collections::{HashMap, HashSet, VecDeque};

/// Máximo de triángulos que recorre [`UvSurface::nearest_connected`].
const MAX_SEARCH: usize = 4096;

/// Parte de una malla con UV: una primitiva, normalmente con un material propio.
#[derive(Debug, Clone, Copy)]
pub struct UvPart<'a> {
    /// Identificador que se devuelve con cada cara que toma su textura de esta
    /// parte (p. ej. el índice de la primitiva).
    pub group: usize,
    pub positions: &'a [[f32; 3]],
    /// Una UV por posición.
    pub uvs: &'a [[f32; 2]],
    pub triangles: &'a [[u32; 3]],
}

/// Superficie con UV de referencia.
///
/// Dos triángulos son vecinos si comparten una arista con las mismas posiciones
/// y las mismas UV a ambos lados; en una costura UV la arista se repite con UV
/// distintas y los triángulos no son vecinos. Las cartas son las islas conexas
/// del mapa: una costura puede separar dos cartas o cortar una misma carta (un
/// cilindro desenrollado).
pub struct UvSurface {
    corners: Vec<[Vector3; 3]>,
    uvs: Vec<[[f64; 2]; 3]>,
    groups: Vec<usize>,
    charts: Vec<usize>,
    num_charts: usize,
    /// Triángulos vecinos por aristas sin costura.
    neighbors: Vec<Vec<usize>>,
    bvh: Bvh,
}

impl UvSurface {
    /// Construye la superficie. `None` si no queda ningún triángulo válido
    /// (índices en rango, UV presentes y área no nula).
    pub fn new<'a>(parts: impl IntoIterator<Item = UvPart<'a>>) -> Option<Self> {
        let parts: Vec<UvPart<'a>> = parts
            .into_iter()
            .filter(|p| p.uvs.len() == p.positions.len())
            .collect();

        // Escala para soldar posiciones: una fracción de la diagonal total
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in parts.iter().flat_map(|p| p.positions) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] as f64);
                hi[k] = hi[k].max(p[k] as f64);
            }
        }
        let diagonal = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt();
        let pos_step = (diagonal * 1e-7).max(f64::MIN_POSITIVE);
        let uv_step = 1e-6;

        let mut corners = Vec::new();
        let mut uvs = Vec::new();
        let mut groups = Vec::new();
        // Vértices soldados por (grupo, posición, UV)
        let mut keys: HashMap<(usize, [i64; 3], [i64; 2]), u32> = HashMap::new();
        let mut welded: Vec<[u32; 3]> = Vec::new();

        for part in &parts {
            let count = part.positions.len();
            for tri in part.triangles {
                if tri.iter().any(|&i| i as usize >= count) {
                    continue;
                }
                let p = tri.map(|i| {
                    let p = part.positions[i as usize];
                    Vector3::new(p[0] as f64, p[1] as f64, p[2] as f64)
                });
                let area = (p[1] - p[0]).cross(&(p[2] - p[0])).length();
                if area.is_nan() || area <= diagonal * diagonal * 1e-14 {
                    continue;
                }
                let uv = tri.map(|i| part.uvs[i as usize].map(|c| c as f64));
                let ids = tri.map(|i| {
                    let p = part.positions[i as usize];
                    let t = part.uvs[i as usize];
                    let key = (
                        part.group,
                        p.map(|c| (c as f64 / pos_step).round() as i64),
                        t.map(|c| (c as f64 / uv_step).round() as i64),
                    );
                    let next = keys.len() as u32;
                    *keys.entry(key).or_insert(next)
                });
                corners.push(p);
                uvs.push(uv);
                groups.push(part.group);
                welded.push(ids);
            }
        }
        if corners.is_empty() {
            return None;
        }

        // Triángulos por arista soldada
        let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (t, ids) in welded.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (ids[k], ids[(k + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push(t);
            }
        }
        let mut neighbors = vec![Vec::new(); corners.len()];
        let mut parent: Vec<usize> = (0..corners.len()).collect();
        for tris in edges.values() {
            for (i, &a) in tris.iter().enumerate() {
                for &b in &tris[i + 1..] {
                    neighbors[a].push(b);
                    neighbors[b].push(a);
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    parent[ra] = rb;
                }
            }
        }
        let mut chart_of_root = HashMap::new();
        let charts: Vec<usize> = (0..corners.len())
            .map(|t| {
                let root = find(&mut parent, t);
                let next = chart_of_root.len();
                *chart_of_root.entry(root).or_insert(next)
            })
            .collect();

        let bvh = Bvh::build(corners.iter().map(|c| Triangle::new(c[0], c[1], c[2])).collect());
        Some(Self { corners, uvs, groups, charts, num_charts: chart_of_root.len(), neighbors, bvh })
    }

    /// Número de triángulos válidos.
    pub fn num_triangles(&self) -> usize {
        self.corners.len()
    }

    /// Número de cartas (islas UV).
    pub fn num_charts(&self) -> usize {
        self.num_charts
    }

    /// Carta del triángulo `t`.
    pub fn chart(&self, t: usize) -> usize {
        self.charts[t]
    }

    /// Grupo del triángulo `t`.
    pub fn group(&self, t: usize) -> usize {
        self.groups[t]
    }

    /// Punto más cercano de la superficie.
    pub fn closest(&self, p: &Vector3) -> ClosestHit {
        self.bvh.query_closest(p).expect("la superficie tiene triángulos")
    }

    /// UV en el punto `p` según el mapa afín del triángulo `t`. `p` se proyecta
    /// al plano del triángulo y fuera de él la UV se extrapola.
    pub fn uv_at(&self, t: usize, p: &Vector3) -> [f64; 2] {
        let [a, b, c] = self.corners[t];
        let (e0, e1, e2) = (b - a, c - a, *p - a);
        let (d00, d01, d11) = (e0.dot(&e0), e0.dot(&e1), e1.dot(&e1));
        let (d20, d21) = (e2.dot(&e0), e2.dot(&e1));
        let denom = d00 * d11 - d01 * d01;
        let (v, w) = if denom > 0.0 {
            ((d11 * d20 - d01 * d21) / denom, (d00 * d21 - d01 * d20) / denom)
        } else {
            (1.0 / 3.0, 1.0 / 3.0)
        };
        let u = 1.0 - v - w;
        let [ua, ub, uc] = self.uvs[t];
        [0, 1].map(|k| u * ua[k] + v * ub[k] + w * uc[k])
    }

    /// Triángulo más cercano a `p` alcanzable desde `start` sin cruzar costuras
    /// (solo por aristas donde las UV son continuas) y sin alejarse más de
    /// `radius` de `p`. Devuelve el triángulo y su distancia a `p`.
    pub fn nearest_connected(&self, start: usize, p: &Vector3, radius: f64) -> (usize, f64) {
        let dist = |t: usize| {
            let [a, b, c] = &self.corners[t];
            p.distance(&closest_point_on_triangle(p, a, b, c))
        };
        let mut best = (start, dist(start));
        let mut visited = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(t) = queue.pop_front() {
            if visited.len() >= MAX_SEARCH {
                break;
            }
            for &n in &self.neighbors[t] {
                if !visited.insert(n) {
                    continue;
                }
                let d = dist(n);
                if d < best.1 {
                    best = (n, d);
                }
                if d <= radius {
                    queue.push_back(n);
                }
            }
        }
        best
    }
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}
