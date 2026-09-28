//! Planos de apoyo: sobre qué cara puede descansar un modelo.
//!
//! Un sólido apoyado en un piso solo lo toca con su envolvente convexa, así
//! que los candidatos son las caras de la envolvente, agrupadas en planos
//! (un piso "plano" de un escaneo trae muchos triángulos casi coplanares).
//! Un plano es estable si el centro de masa cae sobre él: si no, el modelo
//! se vuelca.

use pinocchio_math::{Real, Vector3};
use std::collections::{HashMap, HashSet};

/// Plano sobre el que puede apoyarse el modelo
#[derive(Debug, Clone)]
pub struct FloorCandidate {
    /// Normal hacia afuera del modelo: al apoyarlo, apunta hacia abajo
    pub normal: Vector3,
    /// Contorno del apoyo sobre el plano, en orden antihorario visto desde afuera
    pub polygon: Vec<Vector3>,
    pub area: Real,
    /// El centro de masa cae dentro del contorno
    pub stable: bool,
}

/// Desviación máxima entre la normal de una cara y la de su plano para agruparlas
const MERGE_ANGLE_DEG: Real = 3.0;

/// Planos con menos que esta fracción del área de la envolvente se descartan
const MIN_AREA_FRACTION: Real = 0.002;

/// Centro de masa de un sólido cerrado (volumen); si la malla está abierta o
/// es plana, el centroide de su superficie.
pub fn center_of_mass(positions: &[Vector3], triangles: &[[usize; 3]]) -> Vector3 {
    let mut volume = 0.0;
    let mut weighted = Vector3::zero();
    let mut area = 0.0;
    let mut surface = Vector3::zero();
    for t in triangles {
        let [a, b, c] = t.map(|i| positions[i]);
        // Tetraedro con el origen
        let v = a.dot(&b.cross(&c)) / 6.0;
        volume += v;
        weighted += (a + b + c) * (v / 4.0);
        let s = (b - a).cross(&(c - a)).length() / 2.0;
        area += s;
        surface += (a + b + c) * (s / 3.0);
    }
    let closed = is_closed(triangles);
    if closed && volume.abs() > 1e-12 * area.powf(1.5).max(Real::MIN_POSITIVE) {
        weighted / volume
    } else if area > 0.0 {
        surface / area
    } else if positions.is_empty() {
        Vector3::zero()
    } else {
        positions.iter().fold(Vector3::zero(), |s, &p| s + p) / positions.len() as Real
    }
}

/// Cada arista está en exactamente dos triángulos
fn is_closed(triangles: &[[usize; 3]]) -> bool {
    let mut uses: HashMap<(usize, usize), u32> = HashMap::new();
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *uses.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !uses.is_empty() && uses.values().all(|&n| n == 2)
}

/// Planos de apoyo de `points` (a lo más `max`): los estables primero, cada
/// grupo de mayor a menor área
pub fn floor_candidates(points: &[Vector3], center_of_mass: Vector3, max: usize) -> Vec<FloorCandidate> {
    let hull = convex_hull(points);
    if hull.is_empty() {
        return Vec::new();
    }
    let normal_of = |t: &[usize; 3]| {
        let [a, b, c] = t.map(|i| points[i]);
        (b - a).cross(&(c - a))
    };
    let total_area: Real = hull.iter().map(|t| normal_of(t).length() / 2.0).sum();

    // Vecinos por arista (la envolvente es cerrada y orientada)
    let mut edge_face: HashMap<(usize, usize), usize> = HashMap::new();
    for (f, t) in hull.iter().enumerate() {
        for k in 0..3 {
            edge_face.insert((t[k], t[(k + 1) % 3]), f);
        }
    }

    // Crecimiento de regiones desde las caras más grandes
    let cos_merge = MERGE_ANGLE_DEG.to_radians().cos();
    let mut order: Vec<usize> = (0..hull.len()).collect();
    let areas: Vec<Real> = hull.iter().map(|t| normal_of(t).length() / 2.0).collect();
    order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]));
    let mut group = vec![usize::MAX; hull.len()];
    let mut candidates = Vec::new();
    for seed in order {
        if group[seed] != usize::MAX || areas[seed] <= 0.0 {
            continue;
        }
        let id = candidates.len();
        let mut members = vec![seed];
        let mut sum = normal_of(&hull[seed]);
        group[seed] = id;
        let mut i = 0;
        while i < members.len() {
            let t = hull[members[i]];
            i += 1;
            let n = sum.normalize();
            for k in 0..3 {
                let Some(&g) = edge_face.get(&(t[(k + 1) % 3], t[k])) else { continue };
                if group[g] != usize::MAX || areas[g] <= 0.0 {
                    continue;
                }
                if normal_of(&hull[g]).normalize().dot(&n) >= cos_merge {
                    group[g] = id;
                    sum += normal_of(&hull[g]);
                    members.push(g);
                }
            }
        }
        let area = sum.length() / 2.0;
        let normal = sum.normalize();
        let vertices: Vec<Vector3> = members.iter().flat_map(|&f| hull[f]).map(|v| points[v]).collect();
        candidates.push((normal, area, vertices));
    }

    let mut out: Vec<FloorCandidate> = candidates
        .into_iter()
        .filter(|(_, area, _)| *area >= MIN_AREA_FRACTION * total_area)
        .map(|(normal, area, vertices)| support_polygon(normal, area, &vertices, center_of_mass))
        .collect();
    // Primero los estables: los grandes inestables (costados) casi nunca se buscan
    out.sort_by(|a, b| b.stable.cmp(&a.stable).then(b.area.total_cmp(&a.area)));
    out.truncate(max);
    out
}

/// Contorno del apoyo sobre el plano más externo de normal `normal`
fn support_polygon(normal: Vector3, area: Real, vertices: &[Vector3], com: Vector3) -> FloorCandidate {
    let offset = vertices.iter().map(|v| v.dot(&normal)).fold(Real::NEG_INFINITY, Real::max);
    let (u, w) = basis(normal);
    let project = |p: &Vector3| (p.dot(&u), p.dot(&w));
    let flat: Vec<(Real, Real)> = vertices.iter().map(project).collect();
    let ring = convex_hull_2d(&flat);
    let polygon = ring.iter().map(|&(x, y)| u * x + w * y + normal * offset).collect();
    let stable = inside_convex(&ring, project(&com));
    FloorCandidate { normal, polygon, area, stable }
}

/// Base ortonormal (u, w) con u × w = n: el contorno queda antihorario visto desde afuera
fn basis(n: Vector3) -> (Vector3, Vector3) {
    let helper = if n.x().abs() < 0.9 { Vector3::unit_x() } else { Vector3::unit_y() };
    let u = helper.cross(&n).normalize();
    let w = n.cross(&u);
    (u, w)
}

/// Envolvente 2D en orden antihorario (cadena monótona de Andrew)
fn convex_hull_2d(points: &[(Real, Real)]) -> Vec<(Real, Real)> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: (Real, Real), a: (Real, Real), b: (Real, Real)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(Real, Real)> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &(Real, Real)>> =
            if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
                hull.pop();
            }
            hull.push(q);
        }
        hull.pop();
    }
    hull
}

/// `q` dentro del polígono convexo antihorario (sin incluir el borde)
fn inside_convex(ring: &[(Real, Real)], q: (Real, Real)) -> bool {
    ring.len() >= 3
        && (0..ring.len()).all(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            (b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0) > 0.0
        })
}

struct HullFace {
    v: [usize; 3],
    normal: Vector3,
    offset: Real,
    outside: Vec<usize>,
    alive: bool,
}

impl HullFace {
    fn new(points: &[Vector3], v: [usize; 3]) -> Self {
        let [a, b, c] = v.map(|i| points[i]);
        let normal = (b - a).cross(&(c - a)).try_normalize().unwrap_or_else(Vector3::zero);
        Self { v, normal, offset: normal.dot(&a), outside: Vec::new(), alive: true }
    }

    fn distance(&self, p: &Vector3) -> Real {
        self.normal.dot(p) - self.offset
    }
}

/// Envolvente convexa 3D (quickhull): triángulos orientados hacia afuera,
/// con índices en `points`. Vacía si los puntos no encierran volumen.
pub fn convex_hull(points: &[Vector3]) -> Vec<[usize; 3]> {
    if points.len() < 4 {
        return Vec::new();
    }
    let (min, max) = points.iter().fold((points[0], points[0]), |(lo, hi), p| {
        (
            Vector3::new(lo.x().min(p.x()), lo.y().min(p.y()), lo.z().min(p.z())),
            Vector3::new(hi.x().max(p.x()), hi.y().max(p.y()), hi.z().max(p.z())),
        )
    });
    let eps = 1e-9 * (max - min).length().max(Real::MIN_POSITIVE);

    // Tetraedro inicial: extremos en x, el más lejano a su recta y el más lejano a su plano
    let extreme = |key: &dyn Fn(&Vector3) -> Real| {
        (0..points.len()).max_by(|&a, &b| key(&points[a]).total_cmp(&key(&points[b]))).expect("puntos")
    };
    let i0 = extreme(&|p| -p.x());
    let i1 = extreme(&|p| p.x());
    let dir = points[i1] - points[i0];
    let i2 = extreme(&|p| (*p - points[i0]).cross(&dir).length());
    let plane = dir.cross(&(points[i2] - points[i0]));
    let i3 = extreme(&|p| (*p - points[i0]).dot(&plane).abs());
    if dir.length() <= eps
        || plane.length() <= eps * dir.length()
        || (points[i3] - points[i0]).dot(&plane).abs() <= eps * plane.length()
    {
        return Vec::new();
    }
    let (i1, i2) = if (points[i3] - points[i0]).dot(&plane) > 0.0 { (i2, i1) } else { (i1, i2) };
    let mut faces: Vec<HullFace> = [[i0, i1, i2], [i0, i3, i1], [i1, i3, i2], [i2, i3, i0]]
        .into_iter()
        .map(|v| HullFace::new(points, v))
        .collect();
    let mut edge_face: HashMap<(usize, usize), usize> = HashMap::new();
    for (f, face) in faces.iter().enumerate() {
        for k in 0..3 {
            edge_face.insert((face.v[k], face.v[(k + 1) % 3]), f);
        }
    }
    for (p, point) in points.iter().enumerate() {
        if let Some(face) = faces.iter_mut().find(|f| f.distance(point) > eps) {
            face.outside.push(p);
        }
    }

    let mut pending: Vec<usize> = (0..faces.len()).collect();
    while let Some(start) = pending.pop() {
        if !faces[start].alive || faces[start].outside.is_empty() {
            continue;
        }
        let apex = *faces[start]
            .outside
            .iter()
            .max_by(|&&a, &&b| faces[start].distance(&points[a]).total_cmp(&faces[start].distance(&points[b])))
            .expect("punto exterior");
        let eye = points[apex];

        // Caras visibles desde el ápice, conexas a partir de `start`
        let mut visible = vec![start];
        let mut seen: HashSet<usize> = HashSet::from([start]);
        let mut i = 0;
        while i < visible.len() {
            let v = faces[visible[i]].v;
            i += 1;
            for k in 0..3 {
                let Some(&g) = edge_face.get(&(v[(k + 1) % 3], v[k])) else { continue };
                if faces[g].alive && !seen.contains(&g) && faces[g].distance(&eye) > eps {
                    seen.insert(g);
                    visible.push(g);
                }
            }
        }

        // Horizonte: aristas de caras visibles cuyo vecino no lo es
        let mut horizon = Vec::new();
        for &f in &visible {
            let v = faces[f].v;
            for k in 0..3 {
                let (a, b) = (v[k], v[(k + 1) % 3]);
                if edge_face.get(&(b, a)).is_none_or(|g| !seen.contains(g)) {
                    horizon.push((a, b));
                }
            }
        }
        let mut orphans = Vec::new();
        for &f in &visible {
            faces[f].alive = false;
            orphans.append(&mut faces[f].outside);
            let v = faces[f].v;
            for k in 0..3 {
                edge_face.remove(&(v[k], v[(k + 1) % 3]));
            }
        }
        let first = faces.len();
        for (a, b) in horizon {
            let f = faces.len();
            faces.push(HullFace::new(points, [a, b, apex]));
            for (x, y) in [(a, b), (b, apex), (apex, a)] {
                edge_face.insert((x, y), f);
            }
        }
        for p in orphans {
            if p == apex {
                continue;
            }
            if let Some(face) = faces[first..].iter_mut().find(|f| f.distance(&points[p]) > eps) {
                face.outside.push(p);
            }
        }
        pending.extend(first..faces.len());
    }

    faces.into_iter().filter(|f| f.alive).map(|f| f.v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> (Vec<Vector3>, Vec<[usize; 3]>) {
        let p: Vec<Vector3> = (0..8)
            .map(|i| Vector3::new((i & 1) as Real, ((i >> 1) & 1) as Real, ((i >> 2) & 1) as Real))
            .collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let t = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        (p, t)
    }

    #[test]
    fn hull_of_cube_with_interior_points() {
        let (mut p, _) = cube();
        p.extend([Vector3::new(0.5, 0.5, 0.5), Vector3::new(0.2, 0.7, 0.4)]);
        let hull = convex_hull(&p);
        assert_eq!(hull.len(), 12);
        assert!(hull.iter().all(|t| t.iter().all(|&v| v < 8)));
        // Orientada hacia afuera: el centro queda detrás de todas las caras
        let c = Vector3::new(0.5, 0.5, 0.5);
        for t in &hull {
            let [a, b, d] = t.map(|i| p[i]);
            assert!((b - a).cross(&(d - a)).dot(&(c - a)) < 0.0);
        }
    }

    #[test]
    fn hull_of_sphere_points_contains_all() {
        let mut p = Vec::new();
        for i in 0..40 {
            for j in 0..20 {
                let (th, ph) = (i as Real * 0.157, j as Real * 0.157);
                p.push(Vector3::new(ph.sin() * th.cos(), ph.sin() * th.sin(), ph.cos()));
            }
        }
        let hull = convex_hull(&p);
        assert!(!hull.is_empty());
        for t in &hull {
            let [a, b, c] = t.map(|i| p[i]);
            let n = (b - a).cross(&(c - a));
            assert!(p.iter().all(|q| (*q - a).dot(&n) <= 1e-9 * n.length()));
        }
    }

    #[test]
    fn flat_points_have_no_hull() {
        let p: Vec<Vector3> = (0..10).map(|i| Vector3::new(i as Real, (i * i) as Real, 0.0)).collect();
        assert!(convex_hull(&p).is_empty());
    }

    #[test]
    fn cube_has_six_stable_faces() {
        let (p, t) = cube();
        let com = center_of_mass(&p, &t);
        assert!((com - Vector3::new(0.5, 0.5, 0.5)).length() < 1e-9);
        let c = floor_candidates(&p, com, 30);
        assert_eq!(c.len(), 6);
        for f in &c {
            assert!(f.stable);
            assert!((f.area - 1.0).abs() < 1e-9);
            assert_eq!(f.polygon.len(), 4);
            // El contorno está en el plano más externo
            assert!(f.polygon.iter().all(|q| (q.dot(&f.normal) - com.dot(&f.normal) - 0.5).abs() < 1e-9));
        }
    }

    #[test]
    fn leaning_block_is_unstable_on_its_short_sides() {
        // Prisma de sección de paralelogramo largo e inclinado: el centro de
        // masa (x = 2) cae fuera de las caras horizontales cortas (x en 0..1 y 3..4)
        let base = [(0.0, 0.0), (1.0, 0.0), (4.0, 1.0), (3.0, 1.0)];
        let p: Vec<Vector3> =
            [0.0, 1.0].iter().flat_map(|&z| base.iter().map(move |&(x, y)| Vector3::new(x, y, z))).collect();
        let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
        let t: Vec<[usize; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        let com = center_of_mass(&p, &t);
        let c = floor_candidates(&p, com, 30);
        assert_eq!(c.len(), 6);
        let short: Vec<_> = c.iter().filter(|f| f.normal.y().abs() > 0.99).collect();
        assert!(short.len() == 2 && short.iter().all(|f| !f.stable));
        let long: Vec<_> = c.iter().filter(|f| f.normal.y().abs() <= 0.99).collect();
        assert!(long.len() == 4 && long.iter().all(|f| f.stable));
    }

    #[test]
    fn nearly_flat_base_is_one_plane() {
        // Base con relieve de 0.1°: los triángulos se agrupan en un solo plano
        let mut p = Vec::new();
        for i in 0..6 {
            for j in 0..6 {
                let (x, z) = (i as Real, j as Real);
                p.push(Vector3::new(x, 0.001 * ((i + j) % 2) as Real, z));
                p.push(Vector3::new(x, 3.0, z));
            }
        }
        let com = Vector3::new(2.5, 1.5, 2.5);
        let c = floor_candidates(&p, com, 30);
        let bottom: Vec<_> = c.iter().filter(|f| f.normal.y() < -0.99).collect();
        assert_eq!(bottom.len(), 1);
        assert!((bottom[0].area - 25.0).abs() < 0.1);
    }

    #[test]
    fn open_surface_uses_area_centroid() {
        let p = vec![Vector3::new(0.0, 0.0, 0.0), Vector3::new(2.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 2.0)];
        let com = center_of_mass(&p, &[[0, 1, 2]]);
        assert!((com - Vector3::new(2.0 / 3.0, 0.0, 2.0 / 3.0)).length() < 1e-12);
    }
}
