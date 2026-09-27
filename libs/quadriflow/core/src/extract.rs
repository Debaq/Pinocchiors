//! Extracción de la malla a partir de los desplazamientos enteros.
//!
//! 1. Las aristas del grafo fino con desplazamiento (0, 0) se colapsan: sus
//!    extremos caen en el mismo punto del retículo y forman un vértice final.
//! 2. Cada triángulo de entrada que no se degenera queda como medio quad: dos
//!    lados de una celda y su diagonal. La topología (y la orientación) sale
//!    de la malla de entrada, no de ángulos medidos sobre la geometría.
//! 3. Los medios quads que comparten la diagonal forman un quad; los que
//!    quedan solos (junto a singularidades) son triángulos, y los agujeros que
//!    no son borde real se rellenan.
//! 4. Tras la limpieza (`cleanup`), cada polígono se divide en quads (centroide + puntos medios), así la
//!    salida tiene solo quads.

use crate::hierarchy::Level;
use crate::integer::{self, EdgeOffsets};
use crate::quad::{QuadFace, QuadMesh};
use crate::V3;
use std::collections::HashMap;

/// Largo máximo de un agujero que se rellena.
const MAX_HOLE_SIZE: usize = 64;

/// Malla poligonal intermedia.
#[derive(Debug, Clone, Default)]
pub(crate) struct Polygons {
    pub vertices: Vec<V3>,
    /// Vértices sobre bordes o aristas vivas, que la relajación no debe mover.
    pub fixed: Vec<bool>,
    pub faces: Vec<Vec<u32>>,
}

struct UnionFind(Vec<u32>);

impl UnionFind {
    fn find(&mut self, mut i: u32) -> u32 {
        while self.0[i as usize] != i {
            let parent = self.0[self.0[i as usize] as usize];
            self.0[i as usize] = parent;
            i = parent;
        }
        i
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi as usize] = lo;
        }
    }
}

/// Extrae la malla poligonal del nivel más fino. `triangles` son los
/// triángulos de la superficie cuyos vértices son los del nivel.
pub(crate) fn extract_polygons(
    level: &Level,
    offsets: &EdgeOffsets,
    o: &[V3],
    triangles: &[[u32; 3]],
) -> Polygons {
    let n = level.len();
    let mut uf = UnionFind((0..n as u32).collect());
    for (&(i, j), d) in offsets.edges.iter().zip(&offsets.offset) {
        if *d == [0, 0] {
            uf.union(i, j);
        }
    }
    // Vértices de salida: promedio (por área) de los puntos del retículo fundidos
    let mut cluster = vec![u32::MAX; n];
    let mut sums: Vec<(V3, V3, f64)> = Vec::new();
    for i in 0..n {
        let root = uf.find(i as u32) as usize;
        if cluster[root] == u32::MAX {
            cluster[root] = sums.len() as u32;
            sums.push((V3::zeros(), V3::zeros(), 0.0));
        }
        let c = cluster[root];
        cluster[i] = c;
        let w = level.area[i].max(1e-300);
        let entry = &mut sums[c as usize];
        entry.0 += o[i] * w;
        entry.1 += level.nrm[i] * w;
        entry.2 += w;
    }
    // Sobre bordes y aristas vivas, la posición la dan solo los miembros con la
    // restricción más fuerte (esquina > línea): promediar con los puntos de las
    // caras vecinas sacaría al vértice de la arista.
    let mut on_boundary = vec![false; sums.len()];
    let mut rank = vec![0u8; sums.len()];
    for ((&c, &boundary), constraint) in cluster.iter().zip(&level.on_boundary).zip(&level.constraint) {
        on_boundary[c as usize] |= boundary;
        rank[c as usize] = rank[c as usize].max(constraint.rank());
    }
    let fixed: Vec<bool> = rank.iter().map(|&r| r > 0).collect();
    let mut constrained: Vec<(V3, f64)> = vec![(V3::zeros(), 0.0); sums.len()];
    for i in 0..n {
        let c = cluster[i] as usize;
        if rank[c] > 0 && level.constraint[i].rank() == rank[c] {
            let w = level.area[i].max(1e-300);
            constrained[c].0 += o[i] * w;
            constrained[c].1 += w;
        }
    }
    let vertices: Vec<V3> = sums
        .iter()
        .zip(&constrained)
        .map(|((p, _, w), (cp, cw))| if *cw > 0.0 { cp / *cw } else { p / *w })
        .collect();

    // Tipo de cada par de vértices finales vecinos: ¿diagonal de una celda?
    let mut diagonal: HashMap<(u32, u32), bool> = HashMap::new();
    for (&(i, j), d) in offsets.edges.iter().zip(&offsets.offset) {
        let (a, b) = (cluster[i as usize], cluster[j as usize]);
        if a != b {
            diagonal
                .entry((a.min(b), a.max(b)))
                .or_insert(d[0].abs() == 1 && d[1].abs() == 1);
        }
    }
    let is_diagonal = |a: u32, b: u32| diagonal.get(&(a.min(b), a.max(b))).copied().unwrap_or(false);

    let halves = collapsed_triangles(triangles, &cluster, |t| lattice_diagonal(offsets, t));
    let mut faces = pair_halves(&halves, &is_diagonal);
    fill_holes(&mut faces, &on_boundary);
    Polygons { vertices, fixed, faces }
}

/// Lado (`k` → `k+1`) del triángulo que es la diagonal de su celda, según sus
/// propias esquinas en el retículo: solo si es medio quad (área ½ celda).
fn lattice_diagonal(offsets: &EdgeOffsets, t: [u32; 3]) -> Option<usize> {
    let c = integer::lattice_corners(offsets, t)?;
    let area = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[1][1] - c[0][1]) * (c[2][0] - c[0][0]);
    if area.abs() != 1 {
        return None;
    }
    (0..3).find(|&k| {
        let (p, q) = (c[k], c[(k + 1) % 3]);
        (q[0] - p[0]).abs() == 1 && (q[1] - p[1]).abs() == 1
    })
}

/// Medio quad: triángulo sobre los vértices finales y, si se conoce, el lado
/// que es la diagonal de la celda.
type Half = ([u32; 3], Option<usize>);

/// Triángulos de entrada vistos sobre los vértices finales: se descartan los
/// degenerados y, si dos se superponen (comparten una semiarista, por un
/// pliegue de la parametrización), gana el que cubre más triángulos de entrada.
/// `diagonal_of` da el lado diagonal de un triángulo de entrada.
fn collapsed_triangles(
    triangles: &[[u32; 3]],
    cluster: &[u32],
    diagonal_of: impl Fn([u32; 3]) -> Option<usize>,
) -> Vec<Half> {
    let mut support: HashMap<[u32; 3], (u32, Option<usize>)> = HashMap::new();
    let mut order: Vec<[u32; 3]> = Vec::new();
    for &t in triangles {
        let c = t.map(|v| cluster[v as usize]);
        if c[0] == c[1] || c[1] == c[2] || c[0] == c[2] {
            continue;
        }
        // Rotación canónica: el menor primero, conservando la orientación
        let k = (0..3).min_by_key(|&k| c[k]).expect("tres vértices");
        let key = [c[k], c[(k + 1) % 3], c[(k + 2) % 3]];
        let entry = support.entry(key).or_insert((0, None));
        if entry.0 == 0 {
            order.push(key);
        }
        entry.0 += 1;
        if entry.1.is_none() {
            entry.1 = diagonal_of(t).map(|m| (m + 3 - k) % 3);
        }
    }
    order.sort_by_key(|key| std::cmp::Reverse(support[key].0));

    let mut used: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut accepted = Vec::new();
    for t in order {
        let half_edges = [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
        if half_edges.iter().any(|h| used.contains(h)) {
            continue;
        }
        used.extend(half_edges);
        accepted.push((t, support[&t].1));
    }
    accepted
}

/// Une los medios quads que comparten su diagonal. Un triángulo `(a, b, x)` y
/// otro `(b, a, y)` sobre la diagonal `a–b` forman el quad `(a, y, b, x)`. La
/// diagonal de cada uno sale de su retículo o, si no se conoce (singularidad
/// de orientación), de `is_diagonal` cuando señala un único lado.
fn pair_halves(halves: &[Half], is_diagonal: &impl Fn(u32, u32) -> bool) -> Vec<Vec<u32>> {
    let mut by_half_edge: HashMap<(u32, u32), usize> = HashMap::new();
    for (idx, (t, _)) in halves.iter().enumerate() {
        for k in 0..3 {
            by_half_edge.insert((t[k], t[(k + 1) % 3]), idx);
        }
    }
    let diagonal_of = |(t, known): &Half| -> Option<usize> {
        if known.is_some() {
            return *known;
        }
        let mut found = (0..3).filter(|&k| is_diagonal(t[k], t[(k + 1) % 3]));
        let k = found.next()?;
        found.next().is_none().then_some(k)
    };

    let mut paired = vec![false; halves.len()];
    let mut faces = Vec::with_capacity(halves.len() / 2 + 1);
    for (idx, half) in halves.iter().enumerate() {
        if paired[idx] {
            continue;
        }
        let t = half.0;
        if let Some(k) = diagonal_of(half) {
            let (a, b, x) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            if let Some(&other) = by_half_edge.get(&(b, a))
                && !paired[other]
                && other != idx
            {
                let u = halves[other].0;
                let ko = (0..3).find(|&j| u[j] == b).expect("comparte la diagonal");
                if diagonal_of(&halves[other]) == Some(ko) {
                    let y = u[(ko + 2) % 3];
                    paired[idx] = true;
                    paired[other] = true;
                    faces.push(vec![a, y, b, x]);
                    continue;
                }
            }
        }
    }
    for (idx, (t, _)) in halves.iter().enumerate() {
        if !paired[idx] {
            faces.push(t.to_vec());
        }
    }
    faces
}

/// Rellena los agujeros: cada loop de semiaristas sin gemela se cierra con un
/// polígono, salvo que siga el borde real de la entrada o sea muy largo.
fn fill_holes(faces: &mut Vec<Vec<u32>>, on_boundary: &[bool]) {
    let mut half_edges: std::collections::HashSet<(u32, u32)> = Default::default();
    for f in faces.iter() {
        for k in 0..f.len() {
            half_edges.insert((f[k], f[(k + 1) % f.len()]));
        }
    }
    // Semiaristas del agujero: las gemelas que faltan, indexadas por su origen
    let mut open: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut missing: Vec<(u32, u32)> = half_edges
        .iter()
        .filter(|&&(a, b)| !half_edges.contains(&(b, a)))
        .map(|&(a, b)| (b, a))
        .collect();
    missing.sort_unstable();
    for &(a, b) in &missing {
        open.entry(a).or_default().push(b);
    }

    for &(start, first) in &missing {
        if !open.get(&start).is_some_and(|l| l.contains(&first)) {
            continue; // ya usada
        }
        let mut loop_ = vec![start];
        let mut cur = first;
        let take = |open: &mut HashMap<u32, Vec<u32>>, a: u32, b: u32| {
            if let Some(list) = open.get_mut(&a) {
                list.retain(|&x| x != b);
            }
        };
        take(&mut open, start, first);
        let mut closed = false;
        while loop_.len() <= MAX_HOLE_SIZE {
            if cur == start {
                closed = true;
                break;
            }
            loop_.push(cur);
            let Some(&next) = open.get(&cur).and_then(|l| l.first()) else { break };
            take(&mut open, cur, next);
            cur = next;
        }
        if !closed {
            continue;
        }
        // Un agujero que pasa dos veces por un vértice se parte en ciclos simples
        for cycle in simple_cycles(&loop_) {
            let along_boundary =
                cycle.iter().filter(|&&v| on_boundary[v as usize]).count() * 2 >= cycle.len();
            if cycle.len() >= 3 && !along_boundary {
                faces.push(cycle);
            }
        }
    }
}

/// Parte un loop cerrado que repite vértices en ciclos simples; los tramos de
/// ida y vuelta quedan como ciclos de 2 y se descartan.
pub(crate) fn simple_cycles(walk: &[u32]) -> Vec<Vec<u32>> {
    let mut cycles = Vec::new();
    let mut stack: Vec<u32> = Vec::with_capacity(walk.len());
    let mut position: HashMap<u32, usize> = HashMap::new();
    for &v in walk {
        if let Some(&p) = position.get(&v) {
            cycles.push(stack[p..].to_vec());
            for &w in &stack[p + 1..] {
                position.remove(&w);
            }
            stack.truncate(p + 1);
        } else {
            position.insert(v, stack.len());
            stack.push(v);
        }
    }
    cycles.push(stack);
    cycles
}

/// Divide cada polígono de n lados en n quads (centroide + puntos medios) y
/// descarta vértices sin caras. Devuelve también qué vértices quedan fijos:
/// los fijos originales y los puntos medios entre dos fijos.
pub(crate) fn polygons_to_quads(poly: &Polygons) -> (QuadMesh, Vec<bool>) {
    let mut vertices: Vec<V3> = Vec::new();
    let mut fixed: Vec<bool> = Vec::new();
    let mut remap = vec![usize::MAX; poly.vertices.len()];
    let mut midpoints: HashMap<(u32, u32), usize> = HashMap::new();
    let mut faces = Vec::new();

    for face in &poly.faces {
        let n = face.len();
        let corners: Vec<usize> = face
            .iter()
            .map(|&v| {
                let slot = &mut remap[v as usize];
                if *slot == usize::MAX {
                    *slot = vertices.len();
                    vertices.push(poly.vertices[v as usize]);
                    fixed.push(poly.fixed[v as usize]);
                }
                *slot
            })
            .collect();
        let c = vertices.len();
        vertices.push(face.iter().map(|&v| poly.vertices[v as usize]).sum::<V3>() / n as f64);
        fixed.push(false);
        let mids: Vec<usize> = (0..n)
            .map(|k| {
                let (a, b) = (face[k], face[(k + 1) % n]);
                *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    vertices.push((poly.vertices[a as usize] + poly.vertices[b as usize]) * 0.5);
                    fixed.push(poly.fixed[a as usize] && poly.fixed[b as usize]);
                    vertices.len() - 1
                })
            })
            .collect();
        for k in 0..n {
            faces.push(QuadFace {
                v: [corners[k], mids[k], c, mids[(k + n - 1) % n]],
            });
        }
    }
    (QuadMesh { vertices, faces }, fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_sharing_a_diagonal_become_a_quad() {
        // Celda 0-1-2-3 partida por la diagonal 0-2, más un triángulo suelto
        let halves = [([0, 1, 2], None), ([0, 2, 3], None), ([1, 4, 2], None)];
        let faces = pair_halves(&halves, &|a, b| (a.min(b), a.max(b)) == (0, 2));
        assert_eq!(faces.len(), 2);
        // El mismo ciclo 0-1-2-3, empezando donde sea
        let quad = &faces[0];
        let start = quad.iter().position(|&v| v == 0).unwrap();
        let rotated: Vec<u32> = (0..4).map(|k| quad[(start + k) % 4]).collect();
        assert_eq!(rotated, vec![0, 1, 2, 3]);
        assert_eq!(faces[1], vec![1, 4, 2]);
    }

    #[test]
    fn overlapping_collapsed_triangles_keep_the_best_supported() {
        // (0,1,2) aparece dos veces (desde vértices distintos); (0,1,3) usa la
        // misma semiarista 0→1 una sola vez: es un pliegue y se descarta.
        // (4,5,6) está degenerado tras el colapso.
        let cluster = [0, 1, 2, 3, 0, 0, 1, 1, 2];
        let tris = [[0, 1, 2], [4, 7, 8], [0, 1, 3], [4, 5, 6]];
        let out = collapsed_triangles(&tris, &cluster, |_| None);
        assert_eq!(out, vec![([0, 1, 2], None)]);
    }

    #[test]
    fn small_holes_are_filled_but_real_boundaries_are_not() {
        // Anillo de 4 quads alrededor de un agujero cuadrado 4-5-6-7
        let mut faces = vec![
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        // Sin bordes en la entrada, el anillo tiene dos agujeros: se cierran ambos
        let mut on_boundary = vec![false; 8];
        fill_holes(&mut faces, &on_boundary);
        assert_eq!(faces.len(), 6);
        assert!(faces[4..].iter().all(|f| f.len() == 4));

        // Si el contorno exterior es borde de la entrada, solo se cierra el interior
        faces.truncate(4);
        on_boundary[..4].fill(true);
        fill_holes(&mut faces, &on_boundary);
        assert_eq!(faces.len(), 5);
        assert!(faces[4].iter().all(|&v| v >= 4));
    }

    #[test]
    fn walks_split_into_simple_cycles() {
        // Dos ciclos unidos por el puente 2-5: 0 1 2 5 6 7 5 2 3
        let cycles = simple_cycles(&[0, 1, 2, 5, 6, 7, 5, 2, 3]);
        assert!(cycles.contains(&vec![5, 6, 7]));
        assert!(cycles.contains(&vec![2, 5]));
        assert!(cycles.contains(&vec![0, 1, 2, 3]));
    }

    #[test]
    fn polygons_become_quads() {
        let poly = Polygons {
            vertices: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(1.0, 0.0, 0.0),
                V3::new(1.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
                V3::new(2.0, 0.5, 0.0),
            ],
            fixed: vec![false; 5],
            faces: vec![vec![0, 1, 2, 3], vec![1, 4, 2]],
        };
        let (quads, _) = polygons_to_quads(&poly);
        assert_eq!(quads.faces.len(), 7);
        let topo = quads.topology();
        assert_eq!(topo.non_manifold_edges, 0);
        assert_eq!(topo.flipped_edges, 0);
        assert_eq!(topo.euler_characteristic, 1);
    }
}
