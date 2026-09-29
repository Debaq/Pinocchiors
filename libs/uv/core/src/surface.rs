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
    /// Normales de sombreado, una por posición. Sin ellas se calculan normales
    /// suaves soldando posiciones.
    pub normals: Option<&'a [[f32; 3]]>,
    /// Colores de vértice RGBA lineales, uno por posición.
    pub colors: Option<&'a [[f32; 4]]>,
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
    /// Normal de sombreado por esquina.
    normals: Vec<[Vector3; 3]>,
    /// Color por esquina, si alguna parte trae colores (las demás, blanco).
    colors: Option<Vec<[[f32; 4]; 3]>>,
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
        let mut normals: Vec<Option<[Vector3; 3]>> = Vec::new();
        let any_colors = parts.iter().any(|p| p.colors.is_some_and(|c| c.len() == p.positions.len()));
        let mut colors: Vec<[[f32; 4]; 3]> = Vec::new();
        let mut position_keys: Vec<[[i64; 3]; 3]> = Vec::new();
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
                let normal = part.normals.filter(|n| n.len() == count).map(|n| {
                    tri.map(|i| {
                        let q = n[i as usize];
                        Vector3::new(q[0] as f64, q[1] as f64, q[2] as f64)
                    })
                });
                position_keys.push(tri.map(|i| part.positions[i as usize].map(|c| (c as f64 / pos_step).round() as i64)));
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
                if any_colors {
                    let c = part.colors.filter(|c| c.len() == count);
                    colors.push(tri.map(|i| c.map_or([1.0; 4], |c| c[i as usize])));
                }
                corners.push(p);
                uvs.push(uv);
                normals.push(normal);
                groups.push(part.group);
                welded.push(ids);
            }
        }
        if corners.is_empty() {
            return None;
        }
        let normals = smooth_normals(&corners, normals, &position_keys);

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
        let colors = any_colors.then_some(colors);
        Some(Self { corners, uvs, normals, colors, groups, charts, num_charts: chart_of_root.len(), neighbors, bvh })
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

    /// Punto de la superficie que ve `p` a lo largo de la normal `n`
    /// (unitaria): el corte más cercano del rayo hacia `±n`, a menos de
    /// `reach`, de un triángulo que mire hacia el mismo lado que `n`. `None`
    /// si no hay ninguno.
    ///
    /// Es la proyección de un horneado con jaula: el punto más cercano puede
    /// caer en otra pieza que pasa cerca (un colmillo junto a una pata); el
    /// rayo por la normal se queda en la pieza que la malla recubre.
    pub fn project(&self, p: &Vector3, n: &Vector3, reach: f64) -> Option<(usize, Vector3)> {
        let facing = |t: usize| {
            let [a, b, c] = self.corners[t];
            (b - a).cross(&(c - a)).dot(n) > 0.0
        };
        // + 0.0 pasa las componentes −0 a +0: el BVH descarta cajas con −0
        let canonical = |d: Vector3| Vector3::new(d.x() + 0.0, d.y() + 0.0, d.z() + 0.0);
        [canonical(*n), canonical(*n * -1.0)]
            .into_iter()
            .filter_map(|dir| {
                // Primer corte que mire hacia el mismo lado; los que miran al
                // revés se saltan (la otra cara de una pieza fina)
                let mut from = 0.0;
                loop {
                    let (t, tri) = self.bvh.ray_hit(p, &dir, from, reach)?;
                    if facing(tri) {
                        return Some((t, tri, *p + dir * t));
                    }
                    from = t;
                }
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, tri, point)| (tri, point))
    }

    /// Coordenadas baricéntricas de `p` proyectado al plano del triángulo `t`
    /// (fuera del triángulo alguna es negativa).
    pub fn barycentric(&self, t: usize, p: &Vector3) -> [f64; 3] {
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
        [1.0 - v - w, v, w]
    }

    /// UV en el punto `p` según el mapa afín del triángulo `t`. `p` se proyecta
    /// al plano del triángulo y fuera de él la UV se extrapola.
    pub fn uv_at(&self, t: usize, p: &Vector3) -> [f64; 2] {
        let l = self.barycentric(t, p);
        let [ua, ub, uc] = self.uvs[t];
        [0, 1].map(|k| l[0] * ua[k] + l[1] * ub[k] + l[2] * uc[k])
    }

    /// ¿Trae colores de vértice?
    pub fn has_colors(&self) -> bool {
        self.colors.is_some()
    }

    /// Color de vértice interpolado en el punto `p` del triángulo `t`.
    pub fn color_at(&self, t: usize, p: &Vector3) -> Option<[f32; 4]> {
        let [a, b, c] = self.colors.as_ref()?[t];
        let l = self.barycentric(t, p).map(|x| x.clamp(0.0, 1.0) as f32);
        let sum = (l[0] + l[1] + l[2]).max(1e-6);
        Some(std::array::from_fn(|k| (l[0] * a[k] + l[1] * b[k] + l[2] * c[k]) / sum))
    }

    /// Marco tangente `[T, B, N]` en el punto `p` del triángulo `t`, con la
    /// convención de los normal maps glTF: T hacia +u, B hacia −v (arriba en
    /// la imagen), N la normal de sombreado interpolada.
    pub fn frame_at(&self, t: usize, p: &Vector3) -> [Vector3; 3] {
        let l = self.barycentric(t, p);
        let [na, nb, nc] = self.normals[t];
        let [a, b, c] = self.corners[t];
        let face_normal = (b - a).cross(&(c - a)).try_normalize().unwrap_or(Vector3::unit_z());
        let n = (na * l[0] + nb * l[1] + nc * l[2]).try_normalize().unwrap_or(face_normal);
        let [ua, ub, uc] = self.uvs[t];
        let (dpdu, dpdv) = uv_derivatives([a, b, c], [ua, ub, uc]);
        tangent_frame(n, dpdu, dpdv)
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

/// Derivadas ∂p/∂u y ∂p/∂v de un triángulo (cero si sus UV son degeneradas).
pub(crate) fn uv_derivatives(p: [Vector3; 3], uv: [[f64; 2]; 3]) -> (Vector3, Vector3) {
    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
    let (du1, dv1) = (uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]);
    let (du2, dv2) = (uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-20 {
        return (Vector3::zero(), Vector3::zero());
    }
    let r = 1.0 / det;
    ((e1 * dv2 - e2 * dv1) * r, (e2 * du1 - e1 * du2) * r)
}

/// `[T, B, N]` ortonormal a partir de la normal y las derivadas UV. B apunta
/// hacia −v (convención glTF). Devuelve también el signo en
/// [`tangent_handedness`].
pub(crate) fn tangent_frame(n: Vector3, dpdu: Vector3, dpdv: Vector3) -> [Vector3; 3] {
    let (t, w) = tangent_handedness(n, dpdu, dpdv);
    [t, n.cross(&t) * w, n]
}

/// Tangente ortogonalizada contra `n` y signo `w` tal que `B = w (N × T)`
/// apunte hacia −v.
pub(crate) fn tangent_handedness(n: Vector3, dpdu: Vector3, dpdv: Vector3) -> (Vector3, f64) {
    let t = (dpdu - n * n.dot(&dpdu)).try_normalize().unwrap_or_else(|| {
        let helper = if n.x().abs() < 0.9 { Vector3::unit_x() } else { Vector3::unit_y() };
        helper.cross(&n).normalize()
    });
    let w = if n.cross(&t).dot(&(dpdv * -1.0)) >= 0.0 { 1.0 } else { -1.0 };
    (t, w)
}

/// Completa las normales que faltan con normales suaves: suma de normales de
/// cara (ponderadas por área) de los triángulos que tocan cada posición.
fn smooth_normals(
    corners: &[[Vector3; 3]],
    given: Vec<Option<[Vector3; 3]>>,
    keys: &[[[i64; 3]; 3]],
) -> Vec<[Vector3; 3]> {
    let mut sums: HashMap<[i64; 3], Vector3> = HashMap::new();
    for (t, normal) in given.iter().enumerate() {
        if normal.is_none() {
            let [a, b, c] = corners[t];
            let n = (b - a).cross(&(c - a));
            for key in keys[t] {
                let entry = sums.entry(key).or_insert(Vector3::zero());
                *entry += n;
            }
        }
    }
    given
        .into_iter()
        .enumerate()
        .map(|(t, normal)| {
            normal.unwrap_or_else(|| keys[t].map(|k| sums[&k].try_normalize().unwrap_or(Vector3::unit_z())))
        })
        .collect()
}
