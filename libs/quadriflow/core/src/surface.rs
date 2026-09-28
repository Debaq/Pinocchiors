//! Preparación de la superficie de entrada: soldado, subdivisión, normales,
//! áreas, adyacencia y aristas características (borde y aristas vivas).

use crate::V3;
use std::collections::HashMap;

/// Superficie triangular indexada.
#[derive(Debug, Clone)]
pub(crate) struct Surface {
    pub positions: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
    /// Aristas interiores que los quads deben seguir aunque la superficie
    /// sea lisa ahí: costuras de la piel original (ver [`Surface::mark_seams`]).
    pub seams: Vec<(u32, u32)>,
}

/// Restricción que una arista característica impone a un vértice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Constraint {
    Free,
    /// El retículo debe seguir la recta `point + t·dir` (borde o arista viva).
    Line { point: V3, dir: V3 },
    /// Esquina: un vértice del retículo cae exactamente en `point`.
    Corner { point: V3 },
}

impl Constraint {
    pub fn rank(&self) -> u8 {
        match self {
            Constraint::Free => 0,
            Constraint::Line { .. } => 1,
            Constraint::Corner { .. } => 2,
        }
    }

    /// La restricción más fuerte de las dos.
    pub fn strongest(self, other: Self) -> Self {
        if other.rank() > self.rank() { other } else { self }
    }

    /// Proyecta un punto sobre la restricción.
    pub fn project_position(&self, p: V3) -> V3 {
        match *self {
            Constraint::Free => p,
            Constraint::Line { point, dir } => point + dir * dir.dot(&(p - point)),
            Constraint::Corner { point } => point,
        }
    }
}

/// Grafo de vértices del nivel más fino.
#[derive(Debug, Clone)]
pub(crate) struct VertexGraph {
    pub pos: Vec<V3>,
    pub nrm: Vec<V3>,
    pub area: Vec<f64>,
    /// Vecinos en formato CSR: los de `i` son `adj[adj_start[i]..adj_start[i + 1]]`.
    pub adj_start: Vec<usize>,
    pub adj: Vec<u32>,
    pub constraint: Vec<Constraint>,
    /// Vértices sobre el borde de la malla (aristas con una sola cara).
    pub on_boundary: Vec<bool>,
    /// Aristas de borde y aristas vivas.
    pub feature_edges: Vec<(u32, u32)>,
}

impl VertexGraph {
    pub fn len(&self) -> usize {
        self.pos.len()
    }
}

/// Celda de soldado de una posición.
fn position_key(p: &V3, tolerance: f64) -> (i64, i64, i64) {
    (
        (p.x / tolerance).round() as i64,
        (p.y / tolerance).round() as i64,
        (p.z / tolerance).round() as i64,
    )
}

impl Surface {
    pub fn from_mesh(mesh: &pinocchio_mesh::Mesh) -> Self {
        Self {
            positions: mesh.vertices.iter().map(|v| v.position.0).collect(),
            triangles: (0..mesh.num_faces())
                .map(|f| mesh.get_face_vertices(f).map(|i| i as u32))
                .collect(),
            seams: Vec::new(),
        }
    }

    pub fn new(positions: Vec<V3>, triangles: Vec<[u32; 3]>) -> Self {
        Self { positions, triangles, seams: Vec::new() }
    }

    pub fn bbox_diagonal(&self) -> f64 {
        let mut min = V3::repeat(f64::INFINITY);
        let mut max = V3::repeat(f64::NEG_INFINITY);
        for p in &self.positions {
            min = min.inf(p);
            max = max.sup(p);
        }
        if self.positions.is_empty() { 0.0 } else { (max - min).norm() }
    }

    fn corners(&self, t: [u32; 3]) -> [V3; 3] {
        t.map(|i| self.positions[i as usize])
    }

    /// Número de componentes conexas (por aristas compartidas).
    pub fn component_count(&self) -> usize {
        let mut parent: Vec<u32> = (0..self.positions.len() as u32).collect();
        fn find(p: &mut [u32], mut i: u32) -> u32 {
            while p[i as usize] != i {
                p[i as usize] = p[p[i as usize] as usize];
                i = p[i as usize];
            }
            i
        }
        for t in &self.triangles {
            for k in 1..3 {
                let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
                parent[a as usize] = b;
            }
        }
        (0..self.positions.len() as u32).filter(|&i| find(&mut parent, i) == i).count()
    }

    pub fn area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|&t| {
                let [a, b, c] = self.corners(t);
                0.5 * (b - a).cross(&(c - a)).norm()
            })
            .sum()
    }

    pub fn average_edge_length(&self) -> f64 {
        if self.triangles.is_empty() {
            return 0.0;
        }
        let total: f64 = self
            .triangles
            .iter()
            .map(|&t| {
                let [a, b, c] = self.corners(t);
                (b - a).norm() + (c - b).norm() + (a - c).norm()
            })
            .sum();
        total / (3 * self.triangles.len()) as f64
    }

    /// Suelda posiciones a menos de `tolerance` (costuras UV de glTF), descarta
    /// triángulos degenerados o repetidos y vértices sin caras.
    pub fn weld(&mut self, tolerance: f64) {
        let tolerance = tolerance.max(f64::MIN_POSITIVE);
        let key = |p: &V3| position_key(p, tolerance);
        let mut index_of: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let map: Vec<u32> = self
            .positions
            .iter()
            .map(|p| {
                let next = index_of.len() as u32;
                *index_of.entry(key(p)).or_insert(next)
            })
            .collect();

        let mut seen = std::collections::HashSet::new();
        let area_eps = (tolerance * tolerance).max(f64::MIN_POSITIVE);
        let triangles: Vec<[u32; 3]> = self
            .triangles
            .iter()
            .map(|t| t.map(|i| map[i as usize]))
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .filter(|t| {
                let mut sorted = *t;
                sorted.sort_unstable();
                seen.insert(sorted)
            })
            .collect();

        // Compactar: solo vértices usados, en orden de primera aparición
        let mut new_index = vec![u32::MAX; index_of.len()];
        let mut first_pos = vec![V3::zeros(); index_of.len()];
        for (old, &m) in map.iter().enumerate().rev() {
            first_pos[m as usize] = self.positions[old];
        }
        let mut positions = Vec::new();
        let mut out = Vec::with_capacity(triangles.len());
        for t in triangles {
            let t = t.map(|m| {
                let slot = &mut new_index[m as usize];
                if *slot == u32::MAX {
                    *slot = positions.len() as u32;
                    positions.push(first_pos[m as usize]);
                }
                *slot
            });
            let [a, b, c] = t.map(|i| positions[i as usize]);
            if (b - a).cross(&(c - a)).norm_squared() > area_eps * area_eps {
                out.push(t);
            }
        }
        // Las costuras siguen a sus vértices; las que se degeneran se van
        self.seams = self
            .seams
            .iter()
            .map(|&(a, b)| (new_index[map[a as usize] as usize], new_index[map[b as usize] as usize]))
            .filter(|&(a, b)| a != b && a != u32::MAX && b != u32::MAX)
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        self.seams.sort_unstable();
        self.seams.dedup();
        self.positions = positions;
        self.triangles = out;
    }

    /// Marca como costuras las aristas donde la entrada viene partida sin
    /// soldar (dos vértices en la misma posición: costuras de UV, de normales
    /// o entre materiales), para que los quads las sigan y cada uno caiga
    /// dentro de una sola isla de la piel. Hay que llamarla antes de
    /// [`Surface::weld`], que las borra de la topología.
    ///
    /// Solo cuentan las costuras entre piezas de grosor (`2 · área /
    /// perímetro`) no menor que `min_thickness`: forzar el borde de una isla
    /// más angosta que un quad solo deja quads deformes. Así un modelo
    /// sombreado plano (todos los triángulos partidos) no marca ninguna.
    pub fn mark_seams(&mut self, tolerance: f64, min_thickness: f64) {
        let tolerance = tolerance.max(f64::MIN_POSITIVE);
        let n = self.positions.len();

        // Piezas: componentes por vértices compartidos (sin soldar)
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut i: u32) -> u32 {
            while p[i as usize] != i {
                p[i as usize] = p[p[i as usize] as usize];
                i = p[i as usize];
            }
            i
        }
        for t in &self.triangles {
            for k in 1..3 {
                let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
                parent[a as usize] = b;
            }
        }

        let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut area = vec![0.0; n];
        let mut perimeter = vec![0.0; n];
        for &t in &self.triangles {
            let [a, b, c] = self.corners(t);
            area[find(&mut parent, t[0]) as usize] += 0.5 * (b - a).cross(&(c - a)).norm();
        }
        let mut borders = Vec::new();
        for (&(a, b), &count) in &edge_count {
            if count == 1 {
                let piece = find(&mut parent, a) as usize;
                perimeter[piece] += (self.positions[a as usize] - self.positions[b as usize]).norm();
                borders.push((a, b, piece));
            }
        }
        let thick = |piece: usize| perimeter[piece] > 0.0 && 2.0 * area[piece] / perimeter[piece] >= min_thickness;

        // Bordes en la misma posición: (cuántos, todos entre piezas gruesas, uno de ellos)
        let key = |v: u32| position_key(&self.positions[v as usize], tolerance);
        let mut twins: HashMap<_, (u32, bool, (u32, u32))> = HashMap::new();
        for &(a, b, piece) in &borders {
            let (ka, kb) = (key(a), key(b));
            let entry = twins.entry((ka.min(kb), ka.max(kb))).or_insert((0, true, (a, b)));
            entry.0 += 1;
            entry.1 &= thick(piece);
        }
        let mut seams: Vec<(u32, u32)> = twins
            .into_values()
            .filter(|&(count, thick, _)| count >= 2 && thick)
            .map(|(_, _, (a, b))| (a.min(b), a.max(b)))
            .collect();
        seams.sort_unstable();
        self.seams = seams;
    }

    /// Bisecta aristas (la más larga primero) hasta que ninguna supere `max_len`.
    pub fn subdivide(&mut self, max_len: f64) {
        type Edge = (u32, u32);
        let key = |a: u32, b: u32| (a.min(b), a.max(b));
        let max_sq = max_len * max_len;
        let len_sq = |positions: &[V3], (a, b): Edge| {
            (positions[a as usize] - positions[b as usize]).norm_squared()
        };

        let mut edge_faces: HashMap<Edge, Vec<u32>> = HashMap::new();
        for (f, t) in self.triangles.iter().enumerate() {
            for k in 0..3 {
                edge_faces.entry(key(t[k], t[(k + 1) % 3])).or_default().push(f as u32);
            }
        }
        // Largos no negativos: el orden de sus bits coincide con el numérico
        let mut heap: std::collections::BinaryHeap<(u64, std::cmp::Reverse<Edge>)> = edge_faces
            .keys()
            .map(|&e| (len_sq(&self.positions, e), e))
            .filter(|&(l, _)| l > max_sq)
            .map(|(l, e)| (l.to_bits(), std::cmp::Reverse(e)))
            .collect();

        let mut seams: std::collections::HashSet<Edge> = self.seams.iter().copied().collect();
        while let Some((_, std::cmp::Reverse((a, b)))) = heap.pop() {
            let Some(faces) = edge_faces.remove(&(a, b)) else { continue };
            let m = self.positions.len() as u32;
            self.positions
                .push((self.positions[a as usize] + self.positions[b as usize]) * 0.5);
            if seams.remove(&(a, b)) {
                seams.extend([key(a, m), key(m, b)]);
            }
            let mut new_edges = vec![key(a, m), key(m, b)];
            for f in faces {
                let t = self.triangles[f as usize];
                let k = (0..3)
                    .find(|&k| key(t[k], t[(k + 1) % 3]) == (a, b))
                    .expect("la cara contiene la arista");
                let (x, y, z) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                let g = self.triangles.len() as u32;
                self.triangles[f as usize] = [x, m, z];
                self.triangles.push([m, y, z]);

                if let Some(list) = edge_faces.get_mut(&key(y, z)) {
                    for face in list.iter_mut().filter(|face| **face == f) {
                        *face = g;
                    }
                }
                edge_faces.entry(key(x, m)).or_default().push(f);
                edge_faces.entry(key(m, y)).or_default().push(g);
                edge_faces.entry(key(m, z)).or_default().extend([f, g]);
                new_edges.push(key(m, z));
            }
            for e in new_edges {
                let l = len_sq(&self.positions, e);
                if l > max_sq {
                    heap.push((l.to_bits(), std::cmp::Reverse(e)));
                }
            }
        }
        self.seams = seams.into_iter().collect();
        self.seams.sort_unstable();
    }

    /// Construye el grafo de vértices con normales ponderadas por ángulo, áreas
    /// (un tercio de cada cara) y restricciones de borde, de costuras y, si
    /// `sharp_angle` es `Some`, de aristas vivas.
    pub fn vertex_graph(&self, sharp_angle: Option<f64>, corner_angle: f64) -> VertexGraph {
        let n = self.positions.len();
        let mut nrm = vec![V3::zeros(); n];
        let mut area = vec![0.0; n];
        let mut face_normals = Vec::with_capacity(self.triangles.len());
        let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();

        for (f, &t) in self.triangles.iter().enumerate() {
            let p = self.corners(t);
            let cross = (p[1] - p[0]).cross(&(p[2] - p[0]));
            let a = 0.5 * cross.norm();
            let fnrm = if a > 0.0 { cross / (2.0 * a) } else { V3::zeros() };
            face_normals.push(fnrm);
            for k in 0..3 {
                let e1 = p[(k + 1) % 3] - p[k];
                let e2 = p[(k + 2) % 3] - p[k];
                let angle = e1.angle(&e2);
                if angle.is_finite() {
                    nrm[t[k] as usize] += fnrm * angle;
                }
                area[t[k] as usize] += a / 3.0;
                let (u, v) = (t[k], t[(k + 1) % 3]);
                edge_faces.entry((u.min(v), u.max(v))).or_default().push(f as u32);
            }
        }
        for n in &mut nrm {
            // Normal indefinida (caras opuestas): cualquier dirección sirve
            *n = n.try_normalize(1e-30).unwrap_or_else(V3::z);
        }

        let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut features: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut on_boundary = vec![false; n];
        let mut feature_edges = Vec::new();
        let cos_sharp = sharp_angle.map(f64::cos);
        let seams: std::collections::HashSet<(u32, u32)> = self.seams.iter().copied().collect();
        let mut seam_count = vec![0u32; n];
        for (&(a, b), faces) in &edge_faces {
            neighbors[a as usize].push(b);
            neighbors[b as usize].push(a);
            let feature = match faces.len() {
                1 => {
                    on_boundary[a as usize] = true;
                    on_boundary[b as usize] = true;
                    true
                }
                2 if seams.contains(&(a.min(b), a.max(b))) => {
                    seam_count[a as usize] += 1;
                    seam_count[b as usize] += 1;
                    true
                }
                2 => cos_sharp.is_some_and(|c| {
                    face_normals[faces[0] as usize].dot(&face_normals[faces[1] as usize]) < c
                }),
                _ => false,
            };
            if feature {
                feature_edges.push((a.min(b), a.max(b)));
                features[a as usize].push(b);
                features[b as usize].push(a);
            }
        }

        // Orden fijo: el de iteración del HashMap cambia entre ejecuciones
        for list in &mut features {
            list.sort_unstable();
        }
        feature_edges.sort_unstable();

        let mut adj_start = Vec::with_capacity(n + 1);
        let mut adj = Vec::new();
        adj_start.push(0);
        for list in &mut neighbors {
            list.sort_unstable();
            adj.extend_from_slice(list);
            adj_start.push(adj.len());
        }

        let cos_corner = corner_angle.cos();
        let constraint = (0..n)
            .map(|i| {
                let p = self.positions[i];
                let dirs: Vec<V3> = features[i]
                    .iter()
                    .filter_map(|&j| (self.positions[j as usize] - p).try_normalize(1e-30))
                    .collect();
                // Una costura quiebra en zigzag sin ser esquina (ver isotropic)
                let seam = seam_count[i] == 2;
                match dirs.as_slice() {
                    [] => Constraint::Free,
                    // Dos aristas casi alineadas: la recta continúa
                    [d0, d1] if seam || (-d0).dot(d1) > cos_corner => {
                        let dir = (d1 - d0).normalize();
                        let dir = (dir - nrm[i] * nrm[i].dot(&dir))
                            .try_normalize(1e-12)
                            .unwrap_or(dir);
                        Constraint::Line { point: p, dir }
                    }
                    _ => Constraint::Corner { point: p },
                }
            })
            .collect();

        VertexGraph {
            pos: self.positions.clone(),
            nrm,
            area,
            adj_start,
            adj,
            constraint,
            on_boundary,
            feature_edges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Surface {
        Surface {
            positions: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(4.0, 0.0, 0.0),
                V3::new(4.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            seams: Vec::new(),
        }
    }

    #[test]
    fn subdivide_bounds_edge_length_and_keeps_area() {
        let mut s = quad();
        s.subdivide(0.3);
        for t in &s.triangles {
            let [a, b, c] = s.corners(*t);
            for len in [(b - a).norm(), (c - b).norm(), (a - c).norm()] {
                assert!(len <= 0.3 + 1e-12, "arista de {len}");
            }
        }
        assert!((s.area() - 4.0).abs() < 1e-9);
        // Sin vértices colgando: el grafo es conexo y cada arista interior tiene 2 caras
        let g = s.vertex_graph(None, 0.5);
        let boundary = g.constraint.iter().filter(|c| **c != Constraint::Free).count();
        let perimeter_vertices = (0..g.len())
            .filter(|&i| {
                let p = g.pos[i];
                p.x.abs() < 1e-9 || (p.x - 4.0).abs() < 1e-9 || p.y.abs() < 1e-9 || (p.y - 1.0).abs() < 1e-9
            })
            .count();
        assert_eq!(boundary, perimeter_vertices);
    }

    #[test]
    fn weld_merges_seams_and_drops_degenerates() {
        let mut s = Surface {
            positions: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(1.0, 0.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
                V3::new(1.0, 0.0, 1e-12), // duplicado de 1
                V3::new(1.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0), // duplicado de 2
                V3::new(9.0, 9.0, 9.0), // sin caras
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5], [1, 3, 4], [0, 1, 2]],
            seams: Vec::new(),
        };
        s.weld(1e-9);
        assert_eq!(s.positions.len(), 4);
        assert_eq!(s.triangles.len(), 2);
    }

    /// Grilla de `n × n` celdas en [0, w] × [0, 1], partida sin soldar en
    /// x = `cut` (una costura de UV).
    fn split_grid(n: usize, w: f64, cut: usize) -> Surface {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for (x0, x1) in [(0, cut), (cut, n)] {
            let base = positions.len() as u32;
            let cols = (x1 - x0 + 1) as u32;
            for j in 0..=n {
                for i in x0..=x1 {
                    positions.push(V3::new(w * i as f64 / n as f64, j as f64 / n as f64, 0.0));
                }
            }
            for j in 0..n as u32 {
                for i in 0..(x1 - x0) as u32 {
                    let v = base + j * cols + i;
                    triangles.push([v, v + 1, v + cols + 1]);
                    triangles.push([v, v + cols + 1, v + cols]);
                }
            }
        }
        Surface::new(positions, triangles)
    }

    #[test]
    fn seams_between_thick_pieces_survive_the_weld() {
        let mut s = split_grid(8, 1.0, 4);
        s.mark_seams(1e-9, 0.2);
        s.weld(1e-9);
        // La costura: 8 aristas sobre x = 0.5, ya interiores
        assert_eq!(s.seams.len(), 8);
        for &(a, b) in &s.seams {
            assert!((s.positions[a as usize].x - 0.5).abs() < 1e-12);
            assert!((s.positions[b as usize].x - 0.5).abs() < 1e-12);
        }
        let g = s.vertex_graph(None, std::f64::consts::FRAC_PI_4);
        let inner_seam = (0..g.len()).find(|&i| (g.pos[i] - V3::new(0.5, 0.5, 0.0)).norm() < 1e-12).unwrap();
        assert!(matches!(g.constraint[inner_seam], Constraint::Line { .. }));
    }

    #[test]
    fn narrow_islands_and_flat_shading_mark_no_seams() {
        // Una isla de 1/8 de ancho: más angosta que el mínimo
        let mut s = split_grid(8, 1.0, 1);
        s.mark_seams(1e-9, 0.2);
        assert!(s.seams.is_empty());
        // Sombreado plano: cada triángulo con sus propios vértices, todos
        // mucho más chicos que un quad
        let grid = split_grid(8, 1.0, 4);
        let positions = grid.triangles.iter().flat_map(|t| t.map(|i| grid.positions[i as usize])).collect();
        let triangles = (0..grid.triangles.len() as u32).map(|f| [3 * f, 3 * f + 1, 3 * f + 2]).collect();
        let mut flat = Surface::new(positions, triangles);
        flat.mark_seams(1e-9, 0.2);
        assert!(flat.seams.is_empty());
    }

    #[test]
    fn constraints_on_open_square() {
        let mut s = quad();
        s.subdivide(0.5);
        let g = s.vertex_graph(None, std::f64::consts::FRAC_PI_4);
        for i in 0..g.len() {
            let p = g.pos[i];
            let on_x = p.x.abs() < 1e-9 || (p.x - 4.0).abs() < 1e-9;
            let on_y = p.y.abs() < 1e-9 || (p.y - 1.0).abs() < 1e-9;
            match g.constraint[i] {
                Constraint::Corner { .. } => assert!(on_x && on_y, "esquina en {p:?}"),
                Constraint::Line { dir, .. } => {
                    assert!(on_x ^ on_y);
                    let expected = if on_x { V3::y() } else { V3::x() };
                    assert!(dir.dot(&expected).abs() > 1.0 - 1e-9);
                }
                Constraint::Free => assert!(!on_x && !on_y),
            }
            assert!((g.nrm[i] - V3::z()).norm() < 1e-9);
        }
        let total: f64 = g.area.iter().sum();
        assert!((total - 4.0).abs() < 1e-9);
    }
}
