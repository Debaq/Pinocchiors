//! Extracción de la malla a partir de los campos.
//!
//! 1. Cada arista del grafo fino compara los retículos de sus extremos: si
//!    ambos redondean al mismo punto, los vértices se funden; si difieren en
//!    una celda (horizontal o vertical), hay una arista de la malla final.
//! 2. Las caras se trazan girando alrededor de cada vértice en orden angular.
//! 3. Cada polígono se divide en quads (centroide + puntos medios), así la
//!    salida tiene solo quads aunque haya triángulos o pentágonos en las
//!    singularidades.

use crate::field::{compat_orientation, compat_position_index};
use crate::hierarchy::Level;
use crate::quad::{QuadFace, QuadMesh};
use crate::V3;
use std::collections::HashMap;

/// Largo máximo de una cara normal; los loops más largos son agujeros o bordes.
const MAX_FACE_SIZE: usize = 8;
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

/// Extrae la malla poligonal del nivel más fino.
pub(crate) fn extract_polygons(level: &Level, q: &[V3], o: &[V3], scale: f64) -> Polygons {
    let n = level.len();
    let inv_scale = 1.0 / scale;
    let mut uf = UnionFind((0..n as u32).collect());
    let mut links: Vec<(u32, u32)> = Vec::new();

    for i in 0..n {
        for &(j, _) in level.neighbors(i) {
            let j = j as usize;
            if j <= i {
                continue;
            }
            let (qi, qj) = compat_orientation(&q[i], &level.nrm[i], &q[j], &level.nrm[j]);
            let (si, sj) = compat_position_index(
                &level.pos[i], &level.nrm[i], &qi, &o[i],
                &level.pos[j], &level.nrm[j], &qj, &o[j],
                scale, inv_scale,
            );
            let d = [(si[0] - sj[0]).abs(), (si[1] - sj[1]).abs()];
            match d {
                [0, 0] => uf.union(i as u32, j as u32),
                [1, 0] | [0, 1] => links.push((i as u32, j as u32)),
                _ => {}
            }
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
    let normals: Vec<V3> = sums
        .iter()
        .map(|(_, nrm, _)| nrm.try_normalize(1e-30).unwrap_or_else(V3::z))
        .collect();

    let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); vertices.len()];
    for (i, j) in links {
        let (a, b) = (cluster[i as usize], cluster[j as usize]);
        if a != b {
            neighbors[a as usize].push(b);
            neighbors[b as usize].push(a);
        }
    }
    for list in &mut neighbors {
        list.sort_unstable();
        list.dedup();
    }
    prune_dangling(&mut neighbors);
    for (a, list) in neighbors.iter_mut().enumerate() {
        sort_by_angle(list, a, &vertices, &normals);
    }

    let faces = remove_pillows(trace_faces(&neighbors, &vertices, &normals, &on_boundary));
    Polygons { vertices, fixed, faces }
}

/// Un ciclo aislado del grafo produce dos caras con los mismos vértices (una a
/// cada lado) que forman una "almohada" cerrada sin volumen: se eliminan ambas.
fn remove_pillows(faces: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    let key = |f: &[u32]| {
        let mut k = f.to_vec();
        k.sort_unstable();
        k
    };
    let mut count: HashMap<Vec<u32>, u32> = HashMap::new();
    for f in &faces {
        *count.entry(key(f)).or_default() += 1;
    }
    faces.into_iter().filter(|f| count[&key(f)] == 1).collect()
}

/// Ordena los vecinos de `a` en sentido antihorario alrededor de su normal.
fn sort_by_angle(list: &mut [u32], a: usize, vertices: &[V3], normals: &[V3]) {
    let n = normals[a];
    let e1 = {
        let helper = if n.x.abs() > 0.9 { V3::y() } else { V3::x() };
        n.cross(&helper).normalize()
    };
    let e2 = n.cross(&e1);
    let angle = |b: u32| {
        let d = vertices[b as usize] - vertices[a];
        d.dot(&e2).atan2(d.dot(&e1))
    };
    list.sort_by(|&x, &y| angle(x).total_cmp(&angle(y)));
}

/// Elimina iterativamente los vértices de valencia 1 (aristas colgantes):
/// dejarían un "pico" con vértices repetidos en la cara que los rodea.
fn prune_dangling(neighbors: &mut [Vec<u32>]) {
    let mut stack: Vec<u32> = (0..neighbors.len() as u32)
        .filter(|&v| neighbors[v as usize].len() == 1)
        .collect();
    while let Some(v) = stack.pop() {
        let Some(&u) = neighbors[v as usize].first() else { continue };
        neighbors[v as usize].clear();
        let list = &mut neighbors[u as usize];
        list.retain(|&x| x != v);
        if list.len() == 1 {
            stack.push(u);
        }
    }
}

/// Recorre las caras de un grafo plano local: desde la semiarista a→b, la
/// siguiente es b→c con c el vecino anterior a `a` en el orden antihorario de
/// `b`. Esa regla es una permutación de las semiaristas y cada órbita es una
/// cara. Las órbitas largas son agujeros: se rellenan salvo que recorran el
/// borde de la malla de entrada.
fn trace_faces(
    neighbors: &[Vec<u32>],
    vertices: &[V3],
    normals: &[V3],
    on_boundary: &[bool],
) -> Vec<Vec<u32>> {
    let mut visited: Vec<Vec<bool>> = neighbors.iter().map(|l| vec![false; l.len()]).collect();
    let mut faces = Vec::new();
    let mut face: Vec<u32> = Vec::new();

    for start in 0..neighbors.len() {
        for k in 0..neighbors[start].len() {
            if visited[start][k] {
                continue;
            }
            face.clear();
            let (mut cur, mut kk) = (start, k);
            loop {
                visited[cur][kk] = true;
                face.push(cur as u32);
                let next = neighbors[cur][kk] as usize;
                let list = &neighbors[next];
                let back = list.iter().position(|&x| x as usize == cur).expect("adyacencia simétrica");
                (cur, kk) = (next, (back + list.len() - 1) % list.len());
                if (cur, kk) == (start, k) || visited[cur][kk] {
                    break;
                }
            }
            for cycle in simple_cycles(&face) {
                if accept_face(&cycle, vertices, normals, on_boundary) {
                    faces.push(cycle);
                }
            }
        }
    }
    faces
}

/// Parte un loop cerrado que repite vértices (puentes, cruces) en ciclos
/// simples; los tramos de ida y vuelta quedan como ciclos de 2 y se descartan.
fn simple_cycles(walk: &[u32]) -> Vec<Vec<u32>> {
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

fn accept_face(face: &[u32], vertices: &[V3], normals: &[V3], on_boundary: &[bool]) -> bool {
    if face.len() < 3 || face.len() > MAX_HOLE_SIZE {
        return false;
    }
    let along_boundary = {
        let count = face.iter().filter(|&&v| on_boundary[v as usize]).count();
        count * 2 >= face.len()
    };
    let mut newell = V3::zeros();
    let mut nsum = V3::zeros();
    for (idx, &v) in face.iter().enumerate() {
        let (p, q) = (vertices[v as usize], vertices[face[(idx + 1) % face.len()] as usize]);
        newell += p.cross(&q);
        nsum += normals[v as usize];
    }
    let forward = newell.dot(&nsum) > 0.0;
    match (face.len() <= MAX_FACE_SIZE, forward) {
        (true, true) => true,
        // Loop invertido: el contorno exterior de un borde, o un pliegue local
        // del grafo que se cierra igual (la relajación posterior lo despliega)
        (true, false) => !along_boundary,
        // Agujero: se rellena salvo que siga el borde real de la entrada
        (false, true) => !along_boundary,
        (false, false) => false,
    }
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

    fn grid(n: usize) -> (Vec<Vec<u32>>, Vec<V3>, Vec<V3>) {
        let idx = |i: usize, j: usize| (i * (n + 1) + j) as u32;
        let mut vertices = Vec::new();
        let mut neighbors = vec![Vec::new(); (n + 1) * (n + 1)];
        for i in 0..=n {
            for j in 0..=n {
                vertices.push(V3::new(j as f64, i as f64, 0.0));
                if j < n {
                    neighbors[idx(i, j) as usize].push(idx(i, j + 1));
                    neighbors[idx(i, j + 1) as usize].push(idx(i, j));
                }
                if i < n {
                    neighbors[idx(i, j) as usize].push(idx(i + 1, j));
                    neighbors[idx(i + 1, j) as usize].push(idx(i, j));
                }
            }
        }
        let normals = vec![V3::z(); vertices.len()];
        for (a, list) in neighbors.iter_mut().enumerate() {
            sort_by_angle(list, a, &vertices, &normals);
        }
        (neighbors, vertices, normals)
    }

    #[test]
    fn traces_every_cell_of_a_grid_counterclockwise() {
        let (neighbors, vertices, normals) = grid(3);
        let faces = trace_faces(&neighbors, &vertices, &normals, &vec![false; vertices.len()]);
        assert_eq!(faces.len(), 9);
        for f in &faces {
            assert_eq!(f.len(), 4);
            let [a, b, c] = [0, 1, 2].map(|k| vertices[f[k] as usize]);
            assert!((b - a).cross(&(c - b)).z > 0.0);
        }
    }

    #[test]
    fn isolated_cycle_leaves_no_pillow() {
        let faces = vec![vec![0, 1, 2, 3], vec![3, 2, 1, 0], vec![4, 5, 6]];
        assert_eq!(remove_pillows(faces), vec![vec![4, 5, 6]]);
    }

    #[test]
    fn walks_split_into_simple_cycles() {
        // Dos ciclos unidos por el puente 2-5: 0 1 2 5 6 7 5 2 3
        let walk = [0, 1, 2, 5, 6, 7, 5, 2, 3];
        let cycles = simple_cycles(&walk);
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

