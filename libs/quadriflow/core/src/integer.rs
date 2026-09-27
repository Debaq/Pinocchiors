//! Desplazamientos enteros entre retículos vecinos y eliminación de
//! singularidades de posición (QuadriFlow, Huang et al. 2018, sección 5).
//!
//! Cada vértice tiene un marco (`q`, `n × q`) y un origen de retículo `o`. Para
//! cada arista `i–j` se guarda cuántos cuartos de vuelta separan sus marcos y
//! el desplazamiento entero de `o_i` a `o_j` en celdas, en el marco de `i`.
//!
//! Alrededor de un triángulo sin singularidad de orientación, esos
//! desplazamientos deberían sumar cero. Cuando no, hay una *singularidad de
//! posición*: la extracción ve ahí un triángulo o un pentágono en lugar de un
//! quad. Las sumas no nulas se tratan como cargas unitarias que se trasladan de
//! cara en cara (cambiando en ±1 el desplazamiento de la arista cruzada) hasta
//! anularse con una carga opuesta, salir por un borde o absorberse en una
//! singularidad de orientación: un flujo de costo mínimo resuelto de a una
//! unidad por el camino más corto.

use crate::field::{compat_orientation_index, compat_position_index};
use crate::hierarchy::Level;
use crate::V3;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Coordenadas `v` dadas en el marco `R^k` → coordenadas en el marco base
/// (`R` = cuarto de vuelta antihorario).
#[inline]
pub(crate) fn rot2(v: [i32; 2], k: u8) -> [i32; 2] {
    match k & 3 {
        0 => v,
        1 => [-v[1], v[0]],
        2 => [-v[0], -v[1]],
        _ => [v[1], -v[0]],
    }
}

#[inline]
fn add(a: [i32; 2], b: [i32; 2]) -> [i32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

/// Desplazamientos por arista del grafo fino.
#[derive(Debug, Clone)]
pub(crate) struct EdgeOffsets {
    index: HashMap<(u32, u32), u32>,
    /// Aristas `(i, j)` con `i < j`.
    pub edges: Vec<(u32, u32)>,
    /// El marco de `j` es el de `i` girado `rot` cuartos de vuelta.
    pub rot: Vec<u8>,
    /// Desplazamiento de `o_i` a `o_j`, en celdas y en el marco de `i`.
    pub offset: Vec<[i32; 2]>,
}

impl EdgeOffsets {
    pub fn compute(level: &Level, q: &[V3], o: &[V3], scales: &[f64]) -> Self {
        let mut result = Self {
            index: HashMap::new(),
            edges: Vec::new(),
            rot: Vec::new(),
            offset: Vec::new(),
        };
        for i in 0..level.len() {
            for &(j, _) in level.neighbors(i) {
                let j = j as usize;
                if j <= i {
                    continue;
                }
                let (qi, qj, ki, kj) =
                    compat_orientation_index(&q[i], &level.nrm[i], &q[j], &level.nrm[j]);
                let scale = 0.5 * (scales[i] + scales[j]);
                let inv_scale = 1.0 / scale;
                let (si, sj) = compat_position_index(
                    &level.pos[i], &level.nrm[i], &qi, &o[i],
                    &level.pos[j], &level.nrm[j], &qj, &o[j],
                    scale, inv_scale,
                );
                // o_i + si ≈ o_j + sj en el marco alineado R^ki(marco de i)
                let d = [(si[0] - sj[0]) as i32, (si[1] - sj[1]) as i32];
                result.index.insert((i as u32, j as u32), result.edges.len() as u32);
                result.edges.push((i as u32, j as u32));
                result.rot.push((4 + ki - kj) % 4);
                result.offset.push(rot2(d, ki));
            }
        }
        result
    }

    pub fn edge(&self, a: u32, b: u32) -> Option<u32> {
        self.index.get(&(a.min(b), a.max(b))).copied()
    }

    /// Rotación y desplazamiento de la arista dirigida `a → b`, en el marco de `a`.
    pub fn directed(&self, a: u32, b: u32) -> Option<(u8, [i32; 2])> {
        let e = self.edge(a, b)? as usize;
        let (r, d) = (self.rot[e], self.offset[e]);
        if a < b {
            Some((r, d))
        } else {
            // El marco de a es el de b girado 4 - r; d está en el marco de b
            let back = (4 - r) % 4;
            let d = rot2(d, back);
            Some((back, [-d[0], -d[1]]))
        }
    }
}

/// Estado de un triángulo respecto de los campos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaceState {
    /// Singularidad de orientación: su suma no tiene por qué anularse.
    Singular,
    /// Suma de desplazamientos alrededor del triángulo, en el marco de su
    /// primer vértice; `[0, 0]` si es regular.
    Charge([i32; 2]),
}

/// Rotación de cada esquina del triángulo respecto de la primera.
fn corner_rotations(offsets: &EdgeOffsets, t: [u32; 3]) -> Option<([u8; 3], u8)> {
    let (r_ab, _) = offsets.directed(t[0], t[1])?;
    let (r_bc, _) = offsets.directed(t[1], t[2])?;
    let (r_ca, _) = offsets.directed(t[2], t[0])?;
    Some(([0, r_ab, (r_ab + r_bc) % 4], (r_ab + r_bc + r_ca) % 4))
}

pub(crate) fn face_state(offsets: &EdgeOffsets, t: [u32; 3]) -> Option<FaceState> {
    let (frames, total) = corner_rotations(offsets, t)?;
    if total != 0 {
        return Some(FaceState::Singular);
    }
    let mut sum = [0, 0];
    for k in 0..3 {
        let (_, d) = offsets.directed(t[k], t[(k + 1) % 3])?;
        sum = add(sum, rot2(d, frames[k]));
    }
    Some(FaceState::Charge(sum))
}

/// Posiciones enteras de las esquinas del triángulo en el marco de la primera
/// (`None` si es una singularidad de orientación).
pub(crate) fn lattice_corners(offsets: &EdgeOffsets, t: [u32; 3]) -> Option<[[i32; 2]; 3]> {
    let (frames, total) = corner_rotations(offsets, t)?;
    if total != 0 {
        return None;
    }
    let (_, d_ab) = offsets.directed(t[0], t[1])?;
    let (_, d_bc) = offsets.directed(t[1], t[2])?;
    Some([[0, 0], d_ab, add(d_ab, rot2(d_bc, frames[1]))])
}

/// Área con signo (×2) del triángulo en el retículo, en el marco de su primer
/// vértice: > 0 orientado como la superficie, < 0 invertido, 0 colapsado.
pub(crate) fn lattice_area(offsets: &EdgeOffsets, t: [u32; 3]) -> Option<i32> {
    let (frames, total) = corner_rotations(offsets, t)?;
    if total != 0 {
        return None;
    }
    let (_, d_ab) = offsets.directed(t[0], t[1])?;
    let (_, d_bc) = offsets.directed(t[1], t[2])?;
    let p2 = add(d_ab, rot2(d_bc, frames[1]));
    Some(d_ab[0] * p2[1] - d_ab[1] * p2[0])
}

#[cfg(test)]
pub(crate) fn flipped_count(offsets: &EdgeOffsets, triangles: &[[u32; 3]]) -> usize {
    triangles.iter().filter(|&&t| lattice_area(offsets, t).is_some_and(|a| a < 0)).count()
}

/// Cantidad de triángulos con singularidad de orientación y de posición.
#[cfg(test)]
pub(crate) fn singularity_counts(offsets: &EdgeOffsets, triangles: &[[u32; 3]]) -> (usize, usize) {
    let mut orientation = 0;
    let mut position = 0;
    for &t in triangles {
        match face_state(offsets, t) {
            Some(FaceState::Singular) => orientation += 1,
            Some(FaceState::Charge(c)) if c != [0, 0] => position += 1,
            _ => {}
        }
    }
    (orientation, position)
}

/// Direcciones unitarias del retículo.
const UNITS: [[i32; 2]; 4] = [[1, 0], [0, 1], [-1, 0], [0, -1]];

fn unit_index(u: [i32; 2]) -> usize {
    UNITS.iter().position(|&x| x == u).expect("vector unitario")
}

/// Costo de cruzar una arista cuyo desplazamiento dejaría de ser una celda
/// vecina: se permite, pero solo si no hay otro camino.
const LONG_EDGE_PENALTY: u32 = 20;
/// Límite de estados visitados por búsqueda (las cargas suelen estar a pocas caras).
const MAX_VISITED: usize = 20_000;

/// Topología del grafo dual (caras vecinas por arista).
struct Dual {
    /// Caras de cada arista (a lo sumo dos; `u32::MAX` si falta).
    edge_faces: Vec<[u32; 2]>,
    /// Arista de cada lado del triángulo `k → k+1`.
    face_edges: Vec<[u32; 3]>,
    /// Rotación del marco de cada esquina respecto de la primera.
    frames: Vec<[u8; 3]>,
    state: Vec<FaceState>,
}

impl Dual {
    fn build(offsets: &EdgeOffsets, triangles: &[[u32; 3]]) -> Self {
        let mut edge_faces = vec![[u32::MAX; 2]; offsets.edges.len()];
        let mut blocked = vec![false; offsets.edges.len()];
        let mut face_edges = Vec::with_capacity(triangles.len());
        let mut frames = Vec::with_capacity(triangles.len());
        let mut state = Vec::with_capacity(triangles.len());
        for (f, &t) in triangles.iter().enumerate() {
            let edges = [0, 1, 2].map(|k| offsets.edge(t[k], t[(k + 1) % 3]).unwrap_or(u32::MAX));
            for &e in edges.iter().filter(|&&e| e != u32::MAX) {
                let slot = &mut edge_faces[e as usize];
                if slot[0] == u32::MAX {
                    slot[0] = f as u32;
                } else if slot[1] == u32::MAX {
                    slot[1] = f as u32;
                } else {
                    blocked[e as usize] = true; // arista no-manifold
                }
            }
            face_edges.push(edges);
            let (fr, _) = corner_rotations(offsets, t).unwrap_or(([0; 3], 0));
            frames.push(fr);
            state.push(face_state(offsets, t).unwrap_or(FaceState::Singular));
        }
        for (e, b) in blocked.into_iter().enumerate() {
            if b {
                edge_faces[e] = [u32::MAX; 2];
            }
        }
        Self { edge_faces, face_edges, frames, state }
    }

    /// Esquina del triángulo `f` que es el vértice `v`.
    fn corner(triangles: &[[u32; 3]], f: usize, v: u32) -> usize {
        triangles[f].iter().position(|&x| x == v).expect("vértice de la cara")
    }
}

/// Cambio del desplazamiento de la arista `e` (en el marco de su vértice menor)
/// que quita la carga `u` (en el marco de la cara `f`) de la cara `f`.
fn edge_delta(triangles: &[[u32; 3]], dual: &Dual, f: usize, k: usize, u: [i32; 2]) -> [i32; 2] {
    let t = triangles[f];
    let (a, b) = (t[k], t[(k + 1) % 3]);
    let low = a.min(b);
    let rho = dual.frames[f][Dual::corner(triangles, f, low)];
    // La cara recorre la arista en el sentido guardado (menor → mayor) o al revés
    let sigma = if a < b { 1 } else { -1 };
    let local = rot2(u, (4 - rho) % 4);
    [-sigma * local[0], -sigma * local[1]]
}

/// Estado virtual de Dijkstra: la carga salió por una arista de borde.
const EXIT: u32 = u32::MAX;

/// Mueve cada carga unitaria por el camino más barato hasta anularla. Devuelve
/// cuántas singularidades de posición quedan.
pub(crate) fn remove_position_singularities(offsets: &mut EdgeOffsets, triangles: &[[u32; 3]]) -> usize {
    let mut dual = Dual::build(offsets, triangles);

    // Dijkstra sobre estados (cara, dirección de la carga en el marco de la
    // cara); `prev` guarda el estado anterior y la arista (lado 0..3) cruzada
    let mut dist: HashMap<(u32, u8), u32> = HashMap::new();
    let mut prev: HashMap<(u32, u8), ((u32, u8), u8)> = HashMap::new();

    for start in 0..triangles.len() {
        while let FaceState::Charge(c) = dual.state[start] {
            if c == [0, 0] {
                break;
            }
            let u0 = if c[0] != 0 { [c[0].signum(), 0] } else { [0, c[1].signum()] };

            dist.clear();
            prev.clear();
            let origin = (start as u32, unit_index(u0) as u8);
            let mut heap = BinaryHeap::new();
            dist.insert(origin, 0);
            heap.push(Reverse((0u32, origin)));
            let mut target = None;

            while let Some(Reverse((cost, node))) = heap.pop() {
                if dist.get(&node).is_some_and(|&d| d < cost) {
                    continue;
                }
                if dist.len() > MAX_VISITED {
                    break;
                }
                if node != origin {
                    let reached = node.0 == EXIT
                        || match dual.state[node.0 as usize] {
                            FaceState::Singular => true,
                            // Carga opuesta: se anulan
                            FaceState::Charge(cf) => {
                                let u = UNITS[node.1 as usize];
                                cf[0] * u[0] + cf[1] * u[1] < 0
                            }
                        };
                    if reached {
                        target = Some(node);
                        break;
                    }
                }

                let (f, u) = (node.0 as usize, UNITS[node.1 as usize]);
                for k in 0..3 {
                    let e = dual.face_edges[f][k];
                    if e == u32::MAX {
                        continue;
                    }
                    let faces = dual.edge_faces[e as usize];
                    if faces == [u32::MAX; 2] {
                        continue; // arista no-manifold: barrera
                    }
                    let before = offsets.offset[e as usize];
                    let after = add(before, edge_delta(triangles, &dual, f, k, u));
                    // No invertir ninguna de las caras de la arista
                    let flips = |offsets: &EdgeOffsets| {
                        faces.iter().filter(|&&x| x != u32::MAX).any(|&x| {
                            lattice_area(offsets, triangles[x as usize]).is_some_and(|a| a < 0)
                        })
                    };
                    let flipped_before = flips(offsets);
                    offsets.offset[e as usize] = after;
                    let flipped_after = flips(offsets);
                    offsets.offset[e as usize] = before;
                    if flipped_after && !flipped_before {
                        continue;
                    }

                    let other = if faces[0] as usize == f { faces[1] } else { faces[0] };
                    let next = if other == u32::MAX {
                        // Borde: la carga sale de la superficie
                        (EXIT, 0)
                    } else if matches!(dual.state[other as usize], FaceState::Singular) {
                        // Se absorbe; la dirección en esa cara no importa
                        (other, 0)
                    } else {
                        // Traslado al marco de la otra cara, por el vértice menor de la arista
                        let t = triangles[f];
                        let low = t[k].min(t[(k + 1) % 3]);
                        let rho_f = dual.frames[f][Dual::corner(triangles, f, low)];
                        let rho_g = dual.frames[other as usize][Dual::corner(triangles, other as usize, low)];
                        (other, unit_index(rot2(u, (4 + rho_g - rho_f) % 4)) as u8)
                    };
                    let long = after[0].abs().max(after[1].abs()) > 1;
                    let nd = cost + 1 + if long { LONG_EDGE_PENALTY } else { 0 };
                    if dist.get(&next).is_none_or(|&d| nd < d) {
                        dist.insert(next, nd);
                        prev.insert(next, (node, k as u8));
                        heap.push(Reverse((nd, next)));
                    }
                }
            }

            let Some(end) = target else { break };
            // Aplicar los cambios de arista del camino
            let mut node = end;
            while node != origin {
                let (from, k) = prev[&node];
                let f = from.0 as usize;
                let e = dual.face_edges[f][k as usize] as usize;
                let delta = edge_delta(triangles, &dual, f, k as usize, UNITS[from.1 as usize]);
                offsets.offset[e] = add(offsets.offset[e], delta);
                node = from;
            }
            // La carga sale del origen y se anula en el destino (o se absorbe)
            if let FaceState::Charge(cs) = &mut dual.state[start] {
                *cs = [cs[0] - u0[0], cs[1] - u0[1]];
            }
            if end.0 != EXIT
                && let FaceState::Charge(cg) = &mut dual.state[end.0 as usize]
            {
                *cg = add(*cg, UNITS[end.1 as usize]);
            }
        }
    }

    dual.state
        .iter()
        .filter(|s| matches!(s, FaceState::Charge(c) if *c != [0, 0]))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla n×n de vértices, cada uno en su propio punto del retículo
    /// (marcos alineados), triangulada con diagonales (i,j)–(i+1,j+1).
    fn grid(n: u32) -> (EdgeOffsets, Vec<[u32; 3]>) {
        let idx = |i: u32, j: u32| i * n + j;
        let mut triangles = Vec::new();
        for i in 0..n - 1 {
            for j in 0..n - 1 {
                let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
            }
        }
        let coords = |v: u32| [(v / n) as i32, (v % n) as i32];
        let mut offsets = EdgeOffsets { index: HashMap::new(), edges: Vec::new(), rot: Vec::new(), offset: Vec::new() };
        for t in &triangles {
            for k in 0..3 {
                let (a, b) = (t[k].min(t[(k + 1) % 3]), t[k].max(t[(k + 1) % 3]));
                if offsets.edge(a, b).is_none() {
                    offsets.index.insert((a, b), offsets.edges.len() as u32);
                    offsets.edges.push((a, b));
                    offsets.rot.push(0);
                    let (ca, cb) = (coords(a), coords(b));
                    offsets.offset.push([cb[0] - ca[0], cb[1] - ca[1]]);
                }
            }
        }
        (offsets, triangles)
    }

    #[test]
    fn consistent_grid_has_no_singularities() {
        let (offsets, triangles) = grid(6);
        assert_eq!(singularity_counts(&offsets, &triangles), (0, 0));
        assert_eq!(flipped_count(&offsets, &triangles), 0);
        assert!(triangles.iter().all(|&t| lattice_area(&offsets, t) == Some(1)));
    }

    #[test]
    fn corrupted_edge_dipole_is_removed_without_flips() {
        let (mut offsets, triangles) = grid(8);
        // Una diagonal interior con un desplazamiento equivocado: carga ±1
        // en las dos caras vecinas
        let e = offsets.edge(3 * 8 + 3, 4 * 8 + 4).unwrap() as usize;
        offsets.offset[e] = [1, 0];
        assert_eq!(singularity_counts(&offsets, &triangles), (0, 2));

        let remaining = remove_position_singularities(&mut offsets, &triangles);
        assert_eq!(remaining, 0);
        assert_eq!(singularity_counts(&offsets, &triangles), (0, 0));
        assert_eq!(flipped_count(&offsets, &triangles), 0);
    }

    #[test]
    fn lone_charge_leaves_through_the_boundary() {
        let (mut offsets, triangles) = grid(5);
        // Arista de borde corrompida: una sola cara cargada, sin pareja posible
        let e = offsets.edge(0, 1).unwrap() as usize;
        offsets.offset[e] = [1, 1];
        assert_eq!(singularity_counts(&offsets, &triangles).1, 1);
        assert_eq!(remove_position_singularities(&mut offsets, &triangles), 0);
    }

    #[test]
    fn rot2_is_a_quarter_turn() {
        assert_eq!(rot2([1, 0], 1), [0, 1]);
        assert_eq!(rot2([0, 1], 1), [-1, 0]);
        for k in 0..4 {
            assert_eq!(rot2(rot2([2, -3], k), (4 - k) % 4), [2, -3]);
        }
    }
}
