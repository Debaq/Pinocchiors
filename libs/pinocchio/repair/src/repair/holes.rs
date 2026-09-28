//! Relleno de agujeros (Liepa 2003, "Filling Holes in Meshes").
//!
//! 1. **Triangulación**: programación dinámica sobre el polígono del borde en
//!    3D (Barequet–Sharir), minimizando primero el mayor ángulo diedro (entre
//!    triángulos nuevos y con las caras del borde) y luego el área. No
//!    proyecta a un plano, así que sirve para agujeros alabeados, y nunca crea
//!    una arista que ya exista en la malla.
//! 2. **Refinamiento**: parte los triángulos grandes por su centroide hasta
//!    igualar la densidad de la malla que rodea al agujero, relajando con
//!    flips de Delaunay.
//! 3. **Fairing**: mueve los vértices nuevos para minimizar la energía
//!    bi-laplaciana, de modo que el parche continúe la curvatura del borde en
//!    vez de quedar plano.
//!
//! Requiere una malla orientada y manifold (ver
//! [`crate::repair::manifold::orient_and_split`]).

use crate::analysis::degenerate::angle;
use crate::sparse::solve_least_squares;
use crate::config::HoleFillConfig;
use crate::topology::{self, BoundaryLoop, EdgeTopology};
use crate::trimesh::TriMesh;
use pinocchio_math::{Real, Vector3};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

/// Agujeros con más aristas se triangulan con un abanico desde el centroide
/// (la programación dinámica es O(n³)); el refinamiento y el fairing
/// arreglan después la forma.
const MAX_DP_EDGES: usize = 400;

/// Límite de triángulos por parche durante el refinamiento
const MAX_PATCH_FACES: usize = 500_000;

/// Un "agujero" cuyo parche tendría más que esta fracción del área de su
/// pieza no es un agujero: es el contorno de una lámina abierta (un plano, una
/// aleta). Rellenarlo solo la duplicaría con la cara opuesta.
const MAX_PATCH_AREA_RATIO: Real = 0.6;

/// Área mínima de un parche relativa al cuadrado de su perímetro (un círculo
/// tiene 1/4π ≈ 0.08); por debajo, el agujero es una rendija sin área.
const MIN_PATCH_AREA: Real = 1e-6;

/// Resultado del relleno
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HoleFillReport {
    /// Agujeros rellenados
    pub filled: usize,
    /// Agujeros sin rellenar (demasiado grandes o con borde inválido)
    pub skipped: usize,
    /// Caras añadidas
    pub faces_added: usize,
    /// Vértices añadidos
    pub vertices_added: usize,
}

/// Rellena los agujeros de la malla según `config`.
pub fn fill_holes(mesh: &mut TriMesh, config: &HoleFillConfig) -> HoleFillReport {
    let topo = EdgeTopology::build(&mesh.triangles);
    let loops = topology::boundary_loops(&mesh.triangles, &topo);
    let mut report = HoleFillReport::default();
    if loops.is_empty() {
        return report;
    }

    let mut sigma = vertex_scales(mesh, &topo);
    let (component, count) = topology::face_components(&topo, mesh.num_faces());
    let mut component_area = vec![0.0; count];
    for (f, &c) in component.iter().enumerate() {
        component_area[c] += mesh.face_area(f);
    }
    let mut new_vertices = Vec::new();
    let mut new_faces = Vec::new();

    for hole in &loops {
        let piece = topo
            .find(hole.vertices[0], hole.vertices[1 % hole.vertices.len()])
            .map(|e| component_area[component[topo.faces(e)[0].face]])
            .unwrap_or(0.0);
        match fill_one(mesh, &topo, hole, piece, config, &mut sigma, &mut new_vertices) {
            Some(patch) => {
                report.filled += 1;
                new_faces.extend(patch);
            }
            None => report.skipped += 1,
        }
    }

    report.faces_added = new_faces.len();
    report.vertices_added = new_vertices.len();
    mesh.triangles.extend(new_faces);

    if config.fair && !new_vertices.is_empty() {
        fair(mesh, &new_vertices);
    }
    report
}

/// Triangula y refina un agujero; devuelve las caras nuevas. `piece_area`
/// es el área de la pieza a la que pertenece el borde.
#[allow(clippy::too_many_arguments)]
fn fill_one(
    mesh: &mut TriMesh,
    topo: &EdgeTopology,
    hole: &BoundaryLoop,
    piece_area: Real,
    config: &HoleFillConfig,
    sigma: &mut Vec<Real>,
    new_vertices: &mut Vec<usize>,
) -> Option<Vec<[usize; 3]>> {
    let n = hole.vertices.len();
    if !hole.closed || n < 3 || (config.max_hole_edges > 0 && n > config.max_hole_edges) {
        return None;
    }
    let mut unique = hole.vertices.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != n {
        return None;
    }

    // Las caras nuevas recorren el borde al revés que las existentes
    let polygon: Vec<usize> = hole.vertices.iter().rev().copied().collect();

    let dp = if n <= MAX_DP_EDGES {
        // Normal de la cara existente al otro lado de cada arista (i, i+1)
        let side: Vec<Option<Vector3>> = (0..n)
            .map(|i| {
                let e = topo.find(polygon[i], polygon[(i + 1) % n])?;
                mesh.face_cross(topo.faces(e)[0].face).try_normalize()
            })
            .collect();
        let points: Vec<Vector3> = polygon.iter().map(|&v| mesh.positions[v]).collect();
        triangulate(&points, &side, |i, j| topo.find(polygon[i], polygon[j]).is_some())
            .map(|tris| tris.into_iter().map(|t| t.map(|i| polygon[i])).collect::<Vec<_>>())
    } else {
        None
    };

    let triangle_area = |a: Vector3, b: Vector3, c: Vector3| (b - a).cross(&(c - a)).length() * 0.5;
    let perimeter: Real = (0..n).map(|i| (mesh.positions[polygon[(i + 1) % n]] - mesh.positions[polygon[i]]).length()).sum();
    // Rendija de ancho nulo: rellenarla solo agregaría caras degeneradas
    let min_area = MIN_PATCH_AREA * perimeter * perimeter;
    let mut patch = match dp {
        Some(patch) => {
            let area: Real = patch.iter().map(|t| {
                let [a, b, c] = t.map(|v| mesh.positions[v]);
                triangle_area(a, b, c)
            }).sum();
            if area > MAX_PATCH_AREA_RATIO * piece_area || area < min_area {
                return None;
            }
            patch
        }
        None => {
            // Abanico desde el centroide: el vértice nuevo no puede chocar con
            // aristas existentes
            let center = polygon.iter().fold(Vector3::zero(), |acc, &v| acc + mesh.positions[v]) / n as Real;
            let area: Real = (0..n)
                .map(|i| triangle_area(mesh.positions[polygon[i]], mesh.positions[polygon[(i + 1) % n]], center))
                .sum();
            if area > MAX_PATCH_AREA_RATIO * piece_area || area < min_area {
                return None;
            }
            let c = mesh.positions.len();
            mesh.positions.push(center);
            sigma.push(polygon.iter().map(|&v| sigma[v]).sum::<Real>() / n as Real);
            new_vertices.push(c);
            (0..n).map(|i| [polygon[i], polygon[(i + 1) % n], c]).collect()
        }
    };

    if config.refine {
        let exists = |a: usize, b: usize| topo.find(a, b).is_some();
        refine(mesh, sigma, &mut patch, &exists, new_vertices);
    }
    Some(patch)
}

/// Triangulación de peso mínimo de un polígono cerrado.
///
/// `side[i]` es la normal de la cara vecina por la arista `(i, i+1)`; `exists`
/// dice si una diagonal ya es arista de la malla (prohibida). Devuelve
/// triángulos en índices locales, orientados según el orden del polígono.
pub(crate) fn triangulate(
    points: &[Vector3],
    side: &[Option<Vector3>],
    exists: impl Fn(usize, usize) -> bool,
) -> Option<Vec<[usize; 3]>> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    // El peor diedro se lleva como el menor coseno entre normales (mismo
    // orden que el ángulo, sin trigonometría). Normal ausente = diedro π.
    let at = |i: usize, j: usize| i * n + j;
    let mut worst_cos = vec![Real::NEG_INFINITY; n * n];
    let mut area = vec![Real::INFINITY; n * n];
    let mut split = vec![usize::MAX; n * n];
    // Normal del triángulo elegido para cerrar (i, j)
    let mut chosen = vec![None; n * n];
    for i in 0..n - 1 {
        worst_cos[at(i, i + 1)] = 1.0;
        area[at(i, i + 1)] = 0.0;
        chosen[at(i, i + 1)] = side[i];
    }
    let cos = |a: Option<Vector3>, b: Option<Vector3>| match (a, b) {
        (Some(a), Some(b)) => a.dot(&b),
        _ => -1.0,
    };

    for len in 2..n {
        for i in 0..n - len {
            let j = i + len;
            let closing = i == 0 && j == n - 1;
            if !closing && exists(i, j) {
                continue;
            }
            let (mut best_cos, mut best_area, mut best_k, mut best_normal) =
                (Real::NEG_INFINITY, Real::INFINITY, usize::MAX, None);
            let (pi, pj) = (points[i], points[j]);
            for (k, &pk) in points.iter().enumerate().take(j).skip(i + 1) {
                let (ik, kj) = (at(i, k), at(k, j));
                if !area[ik].is_finite() || !area[kj].is_finite() {
                    continue;
                }
                let cross = (pk - pi).cross(&(pj - pi));
                let norm = cross.length();
                let normal = (norm > 0.0).then(|| cross / norm);
                let mut worst = worst_cos[ik].min(worst_cos[kj]);
                worst = worst.min(cos(normal, chosen[ik])).min(cos(normal, chosen[kj]));
                if closing {
                    worst = worst.min(cos(normal, side[n - 1]));
                }
                let total = area[ik] + area[kj] + norm * 0.5;
                if worst > best_cos + 1e-12 || (worst >= best_cos - 1e-12 && total < best_area) {
                    (best_cos, best_area, best_k, best_normal) = (worst, total, k, normal);
                }
            }
            if best_k != usize::MAX {
                worst_cos[at(i, j)] = best_cos;
                area[at(i, j)] = best_area;
                split[at(i, j)] = best_k;
                chosen[at(i, j)] = best_normal;
            }
        }
    }

    if split[at(0, n - 1)] == usize::MAX {
        return None;
    }
    let mut triangles = Vec::with_capacity(n - 2);
    let mut stack = vec![(0, n - 1)];
    while let Some((i, j)) = stack.pop() {
        let k = split[at(i, j)];
        triangles.push([i, k, j]);
        if k > i + 1 {
            stack.push((i, k));
        }
        if j > k + 1 {
            stack.push((k, j));
        }
    }
    Some(triangles)
}

/// Escala local de cada vértice: largo medio de sus aristas
fn vertex_scales(mesh: &TriMesh, topo: &EdgeTopology) -> Vec<Real> {
    let mut sum = vec![0.0; mesh.num_vertices()];
    let mut count = vec![0u32; mesh.num_vertices()];
    for &[a, b] in &topo.edges {
        let l = (mesh.positions[a] - mesh.positions[b]).length();
        sum[a] += l;
        sum[b] += l;
        count[a] += 1;
        count[b] += 1;
    }
    sum.iter().zip(&count).map(|(&s, &c)| if c > 0 { s / c as Real } else { 0.0 }).collect()
}

/// Refinamiento de Liepa: parte por el centroide cada triángulo cuyo tamaño
/// supera la escala local, y relaja con flips de Delaunay.
fn refine(
    mesh: &mut TriMesh,
    sigma: &mut Vec<Real>,
    patch: &mut Vec<[usize; 3]>,
    exists: &impl Fn(usize, usize) -> bool,
    new_vertices: &mut Vec<usize>,
) {
    let alpha = Real::sqrt(2.0);
    relax(mesh, patch, exists);
    for _ in 0..64 {
        let mut split_any = false;
        for t in 0..patch.len() {
            let [a, b, c] = patch[t];
            let center = (mesh.positions[a] + mesh.positions[b] + mesh.positions[c]) / 3.0;
            let s_center = (sigma[a] + sigma[b] + sigma[c]) / 3.0;
            let too_big = [a, b, c].iter().all(|&m| {
                let d = alpha * (center - mesh.positions[m]).length();
                d > s_center && d > sigma[m]
            });
            if too_big {
                let x = mesh.positions.len();
                mesh.positions.push(center);
                sigma.push(s_center);
                new_vertices.push(x);
                patch[t] = [a, b, x];
                patch.push([b, c, x]);
                patch.push([c, a, x]);
                split_any = true;
            }
        }
        if !split_any {
            break;
        }
        relax(mesh, patch, exists);
        if patch.len() > MAX_PATCH_FACES {
            break;
        }
    }
}

/// Flips de Delaunay dentro del parche: cambia la diagonal de dos triángulos
/// vecinos si la suma de los ángulos opuestos supera π.
fn relax(mesh: &TriMesh, patch: &mut [[usize; 3]], exists: &impl Fn(usize, usize) -> bool) {
    for _ in 0..100 {
        let mut owner: HashMap<(usize, usize), usize> = HashMap::with_capacity(patch.len() * 3);
        for (t, tri) in patch.iter().enumerate() {
            for i in 0..3 {
                owner.insert((tri[i], tri[(i + 1) % 3]), t);
            }
        }
        let mut edges: HashSet<(usize, usize)> = owner.keys().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        let mut changed = vec![false; patch.len()];
        let mut flips = 0;

        for t1 in 0..patch.len() {
            for i in 0..3 {
                if changed[t1] {
                    break;
                }
                let [u, v, c] = [patch[t1][i], patch[t1][(i + 1) % 3], patch[t1][(i + 2) % 3]];
                let Some(&t2) = owner.get(&(v, u)) else { continue };
                if t2 == t1 || changed[t2] {
                    continue;
                }
                let d = topology::opposite(&patch[t2], patch[t2].iter().position(|&x| x == v).unwrap() as u8);
                if d == c || edges.contains(&(c.min(d), c.max(d))) || exists(c, d) {
                    continue;
                }
                let p = |x: usize| mesh.positions[x];
                let opposite_angles = angle(p(u) - p(c), p(v) - p(c)) + angle(p(u) - p(d), p(v) - p(d));
                if opposite_angles <= PI + 1e-9 {
                    continue;
                }
                // No plegar: los triángulos nuevos deben mirar hacia el mismo lado
                let old = (p(v) - p(u)).cross(&(p(c) - p(u))) + (p(u) - p(v)).cross(&(p(d) - p(v)));
                let n1 = (p(u) - p(c)).cross(&(p(d) - p(c)));
                let n2 = (p(v) - p(d)).cross(&(p(c) - p(d)));
                if n1.dot(&old) <= 0.0 || n2.dot(&old) <= 0.0 {
                    continue;
                }
                patch[t1] = [c, u, d];
                patch[t2] = [d, v, c];
                changed[t1] = true;
                changed[t2] = true;
                edges.insert((c.min(d), c.max(d)));
                flips += 1;
            }
        }
        if flips == 0 {
            break;
        }
    }
}

/// Fairing: ubica los vértices `free` minimizando Σ‖Δx‖² (laplaciano
/// uniforme) sobre ellos y sus vecinos, con el resto de la malla fijo.
/// Equivale a una superficie de placa delgada que empalma con el borde.
fn fair(mesh: &mut TriMesh, free: &[usize]) {
    let topo = EdgeTopology::build(&mesh.triangles);
    let nv = mesh.num_vertices();

    // Adyacencia (CSR)
    let mut degree = vec![0usize; nv + 1];
    for &[a, b] in &topo.edges {
        degree[a] += 1;
        degree[b] += 1;
    }
    let mut start = vec![0usize; nv + 1];
    for v in 0..nv {
        start[v + 1] = start[v] + degree[v];
    }
    let mut fill = start.clone();
    let mut neighbours = vec![0usize; start[nv]];
    for &[a, b] in &topo.edges {
        neighbours[fill[a]] = b;
        fill[a] += 1;
        neighbours[fill[b]] = a;
        fill[b] += 1;
    }
    let ring = |v: usize| &neighbours[start[v]..start[v + 1]];

    let mut column = vec![usize::MAX; nv];
    for (k, &v) in free.iter().enumerate() {
        column[v] = k;
    }
    // Filas: vértices libres y sus vecinos (su laplaciano depende de los libres)
    let mut rows: Vec<usize> = free.to_vec();
    let mut in_rows = vec![false; nv];
    for &v in free {
        in_rows[v] = true;
    }
    for &v in free {
        for &w in ring(v) {
            if !in_rows[w] {
                in_rows[w] = true;
                rows.push(w);
            }
        }
    }

    // A x = b por coordenada: fila r = x_r − media(x_vecinos)
    let mut a_rows: Vec<Vec<(usize, Real)>> = Vec::with_capacity(rows.len());
    let mut b_rows: Vec<Vector3> = Vec::with_capacity(rows.len());
    for &r in &rows {
        let nb = ring(r);
        if nb.is_empty() {
            continue;
        }
        let w = 1.0 / nb.len() as Real;
        let mut coeffs = Vec::with_capacity(nb.len() + 1);
        let mut rhs = Vector3::zero();
        for (v, c) in std::iter::once((r, 1.0)).chain(nb.iter().map(|&v| (v, -w))) {
            if column[v] != usize::MAX {
                coeffs.push((column[v], c));
            } else {
                rhs -= mesh.positions[v] * c;
            }
        }
        a_rows.push(coeffs);
        b_rows.push(rhs);
    }

    let solution = solve_least_squares(&a_rows, &b_rows, free.len());
    for (k, &v) in free.iter().enumerate() {
        if let Some(p) = solution.as_ref().map(|x| x[k]).filter(crate::trimesh::is_finite) {
            mesh.positions[v] = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze_trimesh;
    use crate::config::AnalysisConfig;

    fn open_box() -> TriMesh {
        let p = [
            [0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.],
        ];
        TriMesh::new(
            p.iter().map(|&[x, y, z]| Vector3::new(x, y, z)).collect(),
            vec![
                [0, 2, 1], [0, 3, 2], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
                [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
            ],
        )
    }

    /// Esfera UV con un casquete polar quitado: agujero circular y curvo
    fn sphere_with_hole(rings: usize, segments: usize, removed_rings: usize) -> TriMesh {
        let mut positions = vec![Vector3::new(0.0, 0.0, -1.0)];
        for r in 1..rings {
            let theta = PI * r as Real / rings as Real;
            for s in 0..segments {
                let phi = 2.0 * PI * s as Real / segments as Real;
                positions.push(Vector3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), -theta.cos()));
            }
        }
        let ring = |r: usize, s: usize| 1 + (r - 1) * segments + s % segments;
        let mut triangles = Vec::new();
        for s in 0..segments {
            triangles.push([0, ring(1, s + 1), ring(1, s)]);
        }
        for r in 1..rings - 1 - removed_rings {
            for s in 0..segments {
                triangles.push([ring(r, s), ring(r, s + 1), ring(r + 1, s + 1)]);
                triangles.push([ring(r, s), ring(r + 1, s + 1), ring(r + 1, s)]);
            }
        }
        TriMesh::new(positions, triangles)
    }

    #[test]
    fn fills_open_box() {
        let mut m = open_box();
        let r = fill_holes(&mut m, &HoleFillConfig { refine: false, fair: false, ..Default::default() });
        assert_eq!((r.filled, r.faces_added, r.vertices_added), (1, 2, 0));
        let d = analyze_trimesh(&m, &AnalysisConfig::default());
        assert!(d.is_closed && d.normals_consistent, "{d:#?}");
        assert_eq!(d.normals_outward, Some(true));
        assert!((d.volume - 1.0).abs() < 1e-12);
    }

    #[test]
    fn open_sheet_is_not_capped() {
        // Un cuadrado suelto: su contorno no es un agujero
        let mut m = TriMesh::new(
            vec![Vector3::zero(), Vector3::unit_x(), Vector3::new(1., 1., 0.), Vector3::unit_y()],
            vec![[0, 1, 2], [0, 2, 3]],
        );
        let r = fill_holes(&mut m, &HoleFillConfig::default());
        assert_eq!((r.filled, r.skipped), (0, 1));
        assert_eq!(m.num_faces(), 2);
    }

    #[test]
    fn respects_max_hole_edges() {
        let mut m = open_box();
        let r = fill_holes(&mut m, &HoleFillConfig { max_hole_edges: 3, ..Default::default() });
        assert_eq!((r.filled, r.skipped), (0, 1));
    }

    #[test]
    fn triangulation_avoids_existing_edges() {
        // Cuadrado donde la diagonal 0-2 ya existe: debe usar 1-3
        let pts = [
            Vector3::new(0., 0., 0.), Vector3::new(1., 0., 0.), Vector3::new(1., 1., 0.), Vector3::new(0., 1., 0.),
        ];
        let tris = triangulate(&pts, &[None; 4], |i, j| (i, j) == (0, 2)).unwrap();
        assert!(tris.iter().all(|t| !(t.contains(&0) && t.contains(&2))));
    }

    #[test]
    fn curved_hole_is_refined_and_faired() {
        let mut m = sphere_with_hole(16, 24, 4);
        let before = analyze_trimesh(&m, &AnalysisConfig::default());
        assert_eq!(before.boundary_loops, 1);

        let r = fill_holes(&mut m, &HoleFillConfig::default());
        assert_eq!(r.filled, 1);
        assert!(r.vertices_added > 10, "debe refinar: {r:?}");

        let d = analyze_trimesh(&m, &AnalysisConfig { check_self_intersections: true, ..Default::default() });
        assert!(d.is_closed && d.is_manifold && d.normals_consistent, "{d:#?}");
        assert_eq!(d.normals_outward, Some(true));
        assert_eq!(d.self_intersections, 0);

        // El parche sigue la esfera: un parche plano quedaría con desvío
        // radial ≈ 0.44 en el centro (borde a z ≈ 0.56)
        let n0 = m.num_vertices() - r.vertices_added;
        let worst = m.positions[n0..].iter().map(|p| (p.length() - 1.0).abs()).fold(0.0, Real::max);
        assert!(worst < 0.2, "desvío radial {worst}");
    }
}
