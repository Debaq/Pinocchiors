//! Reconstrucción de superficies rotas: cáscaras superpuestas, aristas
//! no-manifold u orientación incoherente.
//!
//! El interior se decide con el número de vueltas generalizado (Jacobson et
//! al. 2013), aproximado por dipolos en un árbol de triángulos (Barill et al.
//! 2018), que tolera sopas de triángulos, cáscaras que se cruzan y agujeros
//! pequeños. La superficie de la unión se extrae con *marching tetrahedra*
//! sobre una grilla con distancias exactas en los puntos de cruce: sin casos
//! ambiguos, la salida es siempre manifold y cerrada.

use crate::surface::Surface;
use crate::V3;
use pinocchio_spatial::{Bvh, Triangle};
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::f64::consts::PI;

/// Lados de la grilla (en vóxeles) permitidos para el eje más largo.
pub(crate) const MIN_CELLS: f64 = 48.0;
const MAX_CELLS: f64 = 320.0;

/// Radio (en vóxeles) de la banda cercana a la superficie: mayor que la
/// diagonal de un vóxel, así ningún punto fuera de la banda tiene un vecino
/// del otro lado de la superficie.
const BAND: f64 = 1.75;

const NEAR: u8 = 1;
const INSIDE: u8 = 2;
const KNOWN: u8 = 4;

/// Vértices muestreados para buscar cáscaras superpuestas.
const OVERLAP_SAMPLES: usize = 10_000;

/// `true` si la superficie no delimita bien un volumen: aristas no-manifold,
/// orientación incoherente o cáscaras metidas unas en otras.
/// Las cáscaras superpuestas solo cuentan si son cerradas: la unión de
/// superficies abiertas taparía sus agujeros.
pub(crate) fn needs_rebuild(surface: &Surface) -> bool {
    match edge_state(surface) {
        EdgeState::Broken => true,
        EdgeState::Open => false,
        EdgeState::Closed => surface.component_count() > 1 && has_overlapping_shells(surface),
    }
}

/// Sobre la superficie de una cáscara aislada el número de vueltas es a lo sumo
/// ~1 (0.5 en zonas suaves); más que eso indica un vértice dentro de otra.
fn has_overlapping_shells(surface: &Surface) -> bool {
    let tree = WindingTree::build(surface);
    let step = surface.positions.len().div_ceil(OVERLAP_SAMPLES).max(1);
    surface.positions.par_iter().step_by(step).any(|&p| tree.winding(p) > 1.1)
}

/// Toda arista con dos caras recorridas en sentidos opuestos.
#[cfg(test)]
pub(crate) fn is_closed_manifold(surface: &Surface) -> bool {
    matches!(edge_state(surface), EdgeState::Closed)
}

/// Sin aristas no-manifold ni orientación incoherente (puede tener bordes).
pub(crate) fn is_manifold(surface: &Surface) -> bool {
    !matches!(edge_state(surface), EdgeState::Broken)
}

enum EdgeState {
    /// Toda arista con dos caras en sentidos opuestos.
    Closed,
    /// Como `Closed`, pero con aristas de borde.
    Open,
    /// Aristas no-manifold u orientación incoherente.
    Broken,
}

fn edge_state(surface: &Surface) -> EdgeState {
    let mut directed = std::collections::HashSet::new();
    for t in &surface.triangles {
        for k in 0..3 {
            if !directed.insert((t[k], t[(k + 1) % 3])) {
                return EdgeState::Broken;
            }
        }
    }
    if directed.iter().all(|&(a, b)| directed.contains(&(b, a))) { EdgeState::Closed } else { EdgeState::Open }
}

/// Reconstruye la superficie cerrada que encierra la unión de los volúmenes
/// de `surface`, con vóxeles de lado cercano a `voxel`.
pub(crate) fn rebuild(surface: &Surface, voxel: f64) -> Surface {
    rebuild_with_limit(surface, voxel, MAX_CELLS)
}

/// Lado del vóxel que se usa de verdad: entre `extent / max_cells` y
/// `extent / MIN_CELLS` (`extent`, el lado más largo de la caja).
pub(crate) fn voxel_size(extent: f64, voxel: f64, max_cells: f64) -> f64 {
    voxel.clamp(extent / max_cells, extent / MIN_CELLS.min(max_cells))
}

/// Puntos de la grilla de cada eje con vóxeles de lado `h` (el margen incluido).
pub(crate) fn grid_dims(size: [f64; 3], h: f64) -> [usize; 3] {
    size.map(|e| (e / h).ceil() as usize + 6)
}

/// Como [`rebuild`], con a lo sumo `max_cells` vóxeles en el eje más largo.
pub(crate) fn rebuild_with_limit(surface: &Surface, voxel: f64, max_cells: f64) -> Surface {
    let (lo, hi) = bounds(&surface.positions);
    let extent = (hi - lo).max();
    let h = voxel_size(extent, voxel, max_cells);
    // Desfase irracional: las caras alineadas con la caja (piezas CAD) no caen
    // sobre planos de la grilla, donde el signo sería ambiguo
    let origin = lo - V3::new(2.0 + 0.5f64.sqrt() * 0.5, 2.0 + 0.3f64.sqrt() * 0.5, 2.0 + 0.7f64.sqrt() * 0.5) * h;
    let size = hi - lo;
    let grid = Grid { origin, h, dims: grid_dims([size.x, size.y, size.z], h) };

    let tree = WindingTree::build(surface);
    let flags = classify(surface, &grid, &tree);

    // Distancia exacta en los puntos con un vecino del otro lado
    let bvh = triangle_bvh(surface);
    let near: Vec<u32> = (0..flags.len() as u32).filter(|&i| flags[i as usize] & NEAR != 0).collect();
    // Sobre una superficie abierta el signo también cambia al cruzar un agujero,
    // así que puede cambiar entre un punto de la banda y uno de afuera
    let mut crossing: Vec<u32> = near
        .par_iter()
        .flat_map_iter(|&i| {
            let differs = |j: &u32| (flags[*j as usize] ^ flags[i as usize]) & INSIDE != 0;
            let outside_band: Vec<u32> =
                grid.neighbors(i).filter(differs).filter(|&j| flags[j as usize] & NEAR == 0).collect();
            let own = grid.neighbors(i).any(|j| differs(&j)).then_some(i);
            own.into_iter().chain(outside_band)
        })
        .collect();
    // Lejos de la banda también puede haber cambios de signo: donde se tocan
    // regiones inundadas con distinto número de vueltas (pasa con superficies
    // abiertas y rotas, como las de un escáner). Un cambio en diagonal dentro
    // de un cubo siempre deja un cambio sobre alguna arista del cubo, así que
    // basta buscar vecinos por las caras y sumar los vecinos del otro lado
    let far: Vec<u32> = (0..flags.len() as u32)
        .into_par_iter()
        .filter(|&i| flags[i as usize] & NEAR == 0)
        .filter(|&i| grid.face_neighbors(i).any(|j| (flags[j as usize] ^ flags[i as usize]) & INSIDE != 0))
        .flat_map_iter(|i| {
            let other: Vec<u32> =
                grid.neighbors(i).filter(|&j| (flags[j as usize] ^ flags[i as usize]) & INSIDE != 0).collect();
            std::iter::once(i).chain(other)
        })
        .collect();
    crossing.extend(far);
    crossing.sort_unstable();
    crossing.dedup();
    let signed = |i: u32| {
        let d = bvh.query_distance(&pinocchio_math::Vector3(grid.point(i))).max(1e-6 * h);
        if flags[i as usize] & INSIDE != 0 { -d } else { d }
    };
    let crossing: HashMap<u32, f64> = crossing.par_iter().map(|&i| (i, signed(i))).collect();

    let mut out = marching_tetrahedra(&grid, &flags, &crossing, &signed);
    collapse_short_edges(&mut out, SHORT_EDGE * h);
    out
}

/// Grilla regular de puntos.
struct Grid {
    origin: V3,
    h: f64,
    dims: [usize; 3],
}

impl Grid {
    fn len(&self) -> usize {
        self.dims.iter().product()
    }

    fn index(&self, x: usize, y: usize, z: usize) -> u32 {
        (x + self.dims[0] * (y + self.dims[1] * z)) as u32
    }

    fn coords(&self, i: u32) -> [usize; 3] {
        let i = i as usize;
        let (nx, ny) = (self.dims[0], self.dims[1]);
        [i % nx, (i / nx) % ny, i / (nx * ny)]
    }

    fn point(&self, i: u32) -> V3 {
        let [x, y, z] = self.coords(i);
        self.origin + V3::new(x as f64, y as f64, z as f64) * self.h
    }

    /// Los 26 vecinos dentro de la grilla.
    fn neighbors(&self, i: u32) -> impl Iterator<Item = u32> + '_ {
        let c = self.coords(i);
        (0..27).filter(|&k| k != 13).filter_map(move |k| {
            let d = [k % 3, (k / 3) % 3, k / 9];
            let p: Vec<usize> = (0..3)
                .map(|a| (c[a] + d[a]).checked_sub(1).filter(|&v| v < self.dims[a]))
                .collect::<Option<_>>()?;
            Some(self.index(p[0], p[1], p[2]))
        })
    }

    /// Los 6 vecinos por las caras.
    fn face_neighbors(&self, i: u32) -> impl Iterator<Item = u32> + '_ {
        let c = self.coords(i);
        (0..6).filter_map(move |k| {
            let (axis, up) = (k / 2, k % 2 == 1);
            let mut p = c;
            p[axis] = if up { c[axis] + 1 } else { c[axis].checked_sub(1)? };
            (p[axis] < self.dims[axis]).then(|| self.index(p[0], p[1], p[2]))
        })
    }
}

/// Marca la banda cercana a la superficie y decide qué puntos están adentro:
/// uno por uno en la banda y por inundación fuera de ella, donde el número de
/// vueltas es constante en cada región conexa.
fn classify(surface: &Surface, grid: &Grid, tree: &WindingTree) -> Vec<u8> {
    let mut flags = vec![0u8; grid.len()];
    let band = BAND * grid.h;
    let near: Vec<u32> = surface
        .triangles
        .par_iter()
        .flat_map_iter(|t| {
            let [a, b, c] = t.map(|i| surface.positions[i as usize]);
            let n = (b - a).cross(&(c - a));
            let n = if n.norm() > 0.0 { n.normalize() } else { n };
            let lo = ((a.inf(&b).inf(&c) - grid.origin) / grid.h).map(|v| (v - BAND).ceil().max(0.0) as usize);
            let hi = ((a.sup(&b).sup(&c) - grid.origin) / grid.h).map(|v| (v + BAND).floor() as usize);
            let hi = [hi.x.min(grid.dims[0] - 1), hi.y.min(grid.dims[1] - 1), hi.z.min(grid.dims[2] - 1)];
            let mut found = Vec::new();
            for z in lo.z..=hi[2] {
                for y in lo.y..=hi[1] {
                    for x in lo.x..=hi[0] {
                        let i = grid.index(x, y, z);
                        let p = grid.point(i);
                        if n.dot(&(p - a)).abs() > band {
                            continue;
                        }
                        let q = pinocchio_spatial::closest_point_on_triangle(
                            &pinocchio_math::Vector3(p),
                            &pinocchio_math::Vector3(a),
                            &pinocchio_math::Vector3(b),
                            &pinocchio_math::Vector3(c),
                        );
                        if (q.0 - p).norm() <= band {
                            found.push(i);
                        }
                    }
                }
            }
            found
        })
        .collect();
    for &i in &near {
        flags[i as usize] |= NEAR | KNOWN;
    }

    let mut near: Vec<u32> = (0..flags.len() as u32).filter(|&i| flags[i as usize] & NEAR != 0).collect();
    near.sort_unstable();
    let inside: Vec<bool> = near.par_iter().map(|&i| tree.winding(grid.point(i)).abs() > 0.5).collect();
    for (&i, inside) in near.iter().zip(inside) {
        if inside {
            flags[i as usize] |= INSIDE;
        }
    }

    let mut queue = VecDeque::new();
    for seed in 0..flags.len() as u32 {
        if flags[seed as usize] & KNOWN != 0 {
            continue;
        }
        let bit = if tree.winding(grid.point(seed)).abs() > 0.5 { INSIDE } else { 0 };
        flags[seed as usize] |= KNOWN | bit;
        queue.push_back(seed);
        while let Some(i) = queue.pop_front() {
            for j in grid.face_neighbors(i) {
                if flags[j as usize] & KNOWN == 0 {
                    flags[j as usize] |= KNOWN | bit;
                    queue.push_back(j);
                }
            }
        }
    }
    flags
}

/// Tetraedros de Kuhn: todos comparten la diagonal 0 → 7 del cubo, así las
/// caras de cubos vecinos se dividen igual. Esquina `k` = (k & 1, k >> 1 & 1, k >> 2 & 1).
const KUHN: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 1, 5, 7],
    [0, 2, 3, 7],
    [0, 2, 6, 7],
    [0, 4, 5, 7],
    [0, 4, 6, 7],
];

/// `signed` da la distancia con signo de un punto que no esté en `crossing`
/// (no debería pasar, pero una esquina sin distancia no puede tumbar la
/// reconstrucción)
fn marching_tetrahedra(grid: &Grid, flags: &[u8], crossing: &HashMap<u32, f64>, signed: &(dyn Fn(u32) -> f64 + Sync)) -> Surface {
    // Cubos con alguna esquina de cruce
    let [nx, ny, nz] = grid.dims;
    let mut cells: Vec<u32> = crossing
        .keys()
        .flat_map(|&i| {
            let [x, y, z] = grid.coords(i);
            (0..8).filter_map(move |k| {
                let c = [x.checked_sub(k & 1)?, y.checked_sub(k >> 1 & 1)?, z.checked_sub(k >> 2 & 1)?];
                (c[0] + 1 < nx && c[1] + 1 < ny && c[2] + 1 < nz).then(|| grid.index(c[0], c[1], c[2]))
            })
        })
        .collect();
    cells.sort_unstable();
    cells.dedup();

    let mut surface = Surface::new(Vec::new(), Vec::new());
    let mut edge_vertex: HashMap<(u32, u32), u32> = HashMap::new();
    let inside = |i: u32| flags[i as usize] & INSIDE != 0;
    for cell in cells {
        let [x, y, z] = grid.coords(cell);
        let corner = |k: usize| grid.index(x + (k & 1), y + (k >> 1 & 1), z + (k >> 2 & 1));
        for tet in KUHN {
            let v = tet.map(corner);
            let (ins, outs): (Vec<u32>, Vec<u32>) = v.iter().partition(|&&i| inside(i));
            if ins.is_empty() || outs.is_empty() {
                continue;
            }
            let mut vertex = |a: u32, b: u32| -> u32 {
                *edge_vertex.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    let dist = |i: u32| crossing.get(&i).copied().unwrap_or_else(|| signed(i));
                    let (da, db) = (dist(a), dist(b));
                    let t = da / (da - db);
                    surface.positions.push(grid.point(a) + (grid.point(b) - grid.point(a)) * t);
                    (surface.positions.len() - 1) as u32
                })
            };
            let polygon: Vec<u32> = match (ins.len(), outs.len()) {
                (1, 3) => outs.iter().map(|&o| vertex(ins[0], o)).collect(),
                (3, 1) => ins.iter().map(|&i| vertex(i, outs[0])).collect(),
                _ => vec![
                    vertex(ins[0], outs[0]),
                    vertex(ins[0], outs[1]),
                    vertex(ins[1], outs[1]),
                    vertex(ins[1], outs[0]),
                ],
            };
            // Normal hacia afuera: del lado interior al exterior
            let outward = grid.point(outs[0]) - grid.point(ins[0]);
            for k in 1..polygon.len() - 1 {
                let mut t = [polygon[0], polygon[k], polygon[k + 1]];
                let [a, b, c] = t.map(|i| surface.positions[i as usize]);
                if (b - a).cross(&(c - a)).dot(&outward) < 0.0 {
                    t.swap(1, 2);
                }
                surface.triangles.push(t);
            }
        }
    }
    surface
}

/// Aristas más cortas que esta fracción de vóxel se colapsan: los cortes de
/// *marching tetrahedra* cerca de un punto de la grilla dejan astillas.
const SHORT_EDGE: f64 = 0.3;

/// Colapsa las aristas más cortas que `min_len` (la más corta primero) en su
/// punto medio, solo si se conserva la variedad (condición de enlace) y
/// ninguna cara vecina se da vuelta.
fn collapse_short_edges(surface: &mut Surface, min_len: f64) {
    let n = surface.positions.len();
    let mut faces_of: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (f, t) in surface.triangles.iter().enumerate() {
        for &v in t {
            faces_of[v as usize].push(f as u32);
        }
    }
    let mut alive = vec![true; surface.triangles.len()];
    let mut candidates: Vec<(f64, u32, u32)> = surface
        .triangles
        .iter()
        .flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3])))
        .filter(|&(a, b)| a < b)
        .map(|(a, b)| ((surface.positions[a as usize] - surface.positions[b as usize]).norm(), a, b))
        .filter(|&(l, _, _)| l < min_len)
        .collect();
    candidates.sort_unstable_by(|x, y| x.0.total_cmp(&y.0).then((x.1, x.2).cmp(&(y.1, y.2))));

    let neighbors = |v: u32, faces_of: &[Vec<u32>], triangles: &[[u32; 3]]| {
        let mut out: Vec<u32> =
            faces_of[v as usize].iter().flat_map(|&f| triangles[f as usize]).filter(|&u| u != v).collect();
        out.sort_unstable();
        out.dedup();
        out
    };

    for (_, a, b) in candidates {
        let positions = &surface.positions;
        if (positions[a as usize] - positions[b as usize]).norm() >= min_len {
            continue;
        }
        let shared: Vec<u32> =
            faces_of[a as usize].iter().copied().filter(|&f| surface.triangles[f as usize].contains(&b)).collect();
        if shared.len() != 2 {
            continue;
        }
        let (na, nb) = (neighbors(a, &faces_of, &surface.triangles), neighbors(b, &faces_of, &surface.triangles));
        if na.len() <= 3 || nb.len() <= 3 {
            continue;
        }
        let common = na.iter().filter(|v| nb.binary_search(v).is_ok()).count();
        if common != 2 {
            continue;
        }
        let m = (positions[a as usize] + positions[b as usize]) * 0.5;
        let keeps_orientation = faces_of[a as usize]
            .iter()
            .chain(&faces_of[b as usize])
            .filter(|f| !shared.contains(f))
            .all(|&f| {
                let t = surface.triangles[f as usize];
                let [p, q, r] = t.map(|i| positions[i as usize]);
                let [p2, q2, r2] = t.map(|i| if i == a || i == b { m } else { positions[i as usize] });
                let (before, after) = ((q - p).cross(&(r - p)), (q2 - p2).cross(&(r2 - p2)));
                after.dot(&before) > 0.2 * before.norm() * after.norm()
            });
        if !keeps_orientation {
            continue;
        }

        surface.positions[a as usize] = m;
        for &f in &shared {
            alive[f as usize] = false;
            for v in surface.triangles[f as usize] {
                faces_of[v as usize].retain(|&g| g != f);
            }
        }
        for f in std::mem::take(&mut faces_of[b as usize]) {
            for v in surface.triangles[f as usize].iter_mut().filter(|v| **v == b) {
                *v = a;
            }
            faces_of[a as usize].push(f);
        }
    }

    // Compactar
    let mut index = vec![u32::MAX; n];
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    for (t, _) in surface.triangles.iter().zip(&alive).filter(|(_, a)| **a) {
        triangles.push(t.map(|v| {
            if index[v as usize] == u32::MAX {
                index[v as usize] = positions.len() as u32;
                positions.push(surface.positions[v as usize]);
            }
            index[v as usize]
        }));
    }
    surface.positions = positions;
    surface.triangles = triangles;
}

fn bounds(points: &[V3]) -> (V3, V3) {
    points.iter().fold(
        (V3::repeat(f64::INFINITY), V3::repeat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.inf(p), hi.sup(p)),
    )
}

fn triangle_bvh(surface: &Surface) -> Bvh {
    Bvh::build(
        surface
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| pinocchio_math::Vector3(surface.positions[i as usize]));
                Triangle::new(a, b, c)
            })
            .collect(),
    )
}

/// Árbol de triángulos para el número de vueltas rápido: cada nodo guarda su
/// dipolo (suma de normales por área) para evaluarlo de lejos.
struct WindingTree {
    triangles: Vec<[V3; 3]>,
    nodes: Vec<Node>,
}

struct Node {
    center: V3,
    radius: f64,
    dipole: V3,
    /// Triángulos `start..end` de `WindingTree::triangles`
    start: usize,
    end: usize,
    children: Option<[usize; 2]>,
}

/// Distancia (en radios del nodo) a partir de la cual se usa el dipolo.
const BETA: f64 = 2.0;
const LEAF_SIZE: usize = 8;

impl WindingTree {
    fn build(surface: &Surface) -> Self {
        let mut triangles: Vec<[V3; 3]> =
            surface.triangles.iter().map(|t| t.map(|i| surface.positions[i as usize])).collect();
        let mut nodes = Vec::new();
        Self::build_node(&mut triangles, 0, &mut nodes);
        Self { triangles, nodes }
    }

    /// Construye el nodo de `tris` (que empieza en `offset`) y devuelve su índice.
    fn build_node(tris: &mut [[V3; 3]], offset: usize, nodes: &mut Vec<Node>) -> usize {
        let mut dipole = V3::zeros();
        let mut weighted = V3::zeros();
        let mut area = 0.0;
        for [a, b, c] in tris.iter() {
            let n = 0.5 * (b - a).cross(&(c - a));
            let w = n.norm();
            dipole += n;
            weighted += (a + b + c) / 3.0 * w;
            area += w;
        }
        let centroid = |t: &[V3; 3]| (t[0] + t[1] + t[2]) / 3.0;
        let center = if area > 0.0 {
            weighted / area
        } else {
            tris.iter().map(centroid).sum::<V3>() / tris.len().max(1) as f64
        };
        let radius = tris.iter().flatten().map(|p| (p - center).norm()).fold(0.0, f64::max);

        let index = nodes.len();
        nodes.push(Node { center, radius, dipole, start: offset, end: offset + tris.len(), children: None });
        if tris.len() > LEAF_SIZE {
            let (lo, hi) = tris
                .iter()
                .map(centroid)
                .fold((V3::repeat(f64::INFINITY), V3::repeat(f64::NEG_INFINITY)), |(lo, hi), p| {
                    (lo.inf(&p), hi.sup(&p))
                });
            let axis = (hi - lo).imax();
            let mid = tris.len() / 2;
            tris.select_nth_unstable_by(mid, |a, b| centroid(a)[axis].total_cmp(&centroid(b)[axis]));
            let (left, right) = tris.split_at_mut(mid);
            let l = Self::build_node(left, offset, nodes);
            let r = Self::build_node(right, offset + mid, nodes);
            nodes[index].children = Some([l, r]);
        }
        index
    }

    /// Número de vueltas en `p`: ≈1 adentro, ≈0 afuera.
    fn winding(&self, p: V3) -> f64 {
        let mut total = 0.0;
        let mut stack = vec![0];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            let r = node.center - p;
            let dist = r.norm();
            if dist > BETA * node.radius {
                total += node.dipole.dot(&r) / (dist * dist * dist);
            } else if let Some(children) = node.children {
                stack.extend(children);
            } else {
                total += self.triangles[node.start..node.end]
                    .iter()
                    .map(|t| solid_angle(t, p))
                    .sum::<f64>();
            }
        }
        total / (4.0 * PI)
    }
}

/// Ángulo sólido del triángulo visto desde `p` (Van Oosterom y Strackee).
fn solid_angle(t: &[V3; 3], p: V3) -> f64 {
    let [a, b, c] = t.map(|v| v - p);
    let (la, lb, lc) = (a.norm(), b.norm(), c.norm());
    let num = a.dot(&b.cross(&c));
    let den = la * lb * lc + a.dot(&b) * lc + b.dot(&c) * la + c.dot(&a) * lb;
    2.0 * num.atan2(den)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cubo [0,1]³ con normales hacia afuera.
    fn cube(offset: V3) -> Surface {
        let positions = (0..8)
            .map(|k| offset + V3::new((k & 1) as f64, (k >> 1 & 1) as f64, (k >> 2 & 1) as f64))
            .collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let triangles = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        Surface::new(positions, triangles)
    }

    #[test]
    fn winding_number_of_a_cube() {
        let tree = WindingTree::build(&cube(V3::zeros()));
        assert!((tree.winding(V3::repeat(0.5)) - 1.0).abs() < 1e-9);
        assert!((tree.winding(V3::new(0.9, 0.1, 0.2)) - 1.0).abs() < 1e-9);
        assert!(tree.winding(V3::repeat(3.0)).abs() < 1e-3);
    }

    #[test]
    fn overlapping_cubes_become_their_union() {
        let mut soup = cube(V3::zeros());
        let other = cube(V3::new(0.5, 0.25, 0.0));
        let n = soup.positions.len() as u32;
        soup.positions.extend(other.positions);
        soup.triangles.extend(other.triangles.iter().map(|t| t.map(|i| i + n)));
        assert!(!needs_rebuild(&cube(V3::zeros())));
        assert!(matches!(edge_state(&soup), EdgeState::Closed));
        assert!(needs_rebuild(&soup));

        let out = rebuild(&soup, 0.05);
        assert!(!needs_rebuild(&out));
        assert_eq!(out.component_count(), 1);
        // Volumen de la unión: 1 + 1 − 0.5·0.75·1
        let volume: f64 = out
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| out.positions[i as usize]);
                a.dot(&b.cross(&c)) / 6.0
            })
            .sum();
        assert!((volume - 1.625).abs() < 0.03, "volumen {volume}");
        // Cerrada: toda arista con exactamente dos caras
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &out.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&c| c == 2));
    }
}
