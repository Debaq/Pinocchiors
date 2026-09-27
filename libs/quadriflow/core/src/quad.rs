//! Malla de quads resultante.

use crate::V3;
use std::collections::HashMap;

/// Cara de cuatro vértices, en sentido antihorario visto desde afuera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuadFace {
    pub v: [usize; 4],
}

/// Malla formada solo por quads.
#[derive(Debug, Clone, Default)]
pub struct QuadMesh {
    /// Posiciones de los vértices.
    pub vertices: Vec<V3>,
    /// Caras (orientadas como la malla de entrada).
    pub faces: Vec<QuadFace>,
}

/// Resumen topológico de una [`QuadMesh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuadTopology {
    /// Aristas usadas por una sola cara.
    pub boundary_edges: usize,
    /// Aristas usadas por más de dos caras.
    pub non_manifold_edges: usize,
    /// Aristas que dos caras recorren en el mismo sentido (orientación incoherente).
    pub flipped_edges: usize,
    /// Caras con vértices repetidos.
    pub degenerate_faces: usize,
    /// Vértices donde se tocan dos o más abanicos de caras (pellizcos).
    pub non_manifold_vertices: usize,
    /// Característica de Euler V − E + F (2 para una esfera, 0 para un toro).
    pub euler_characteristic: i64,
}

impl QuadMesh {
    /// Crea una malla vacía.
    pub fn new() -> Self {
        Self::default()
    }

    /// Número de vértices.
    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    /// Número de caras.
    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }

    /// `true` si no tiene caras.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Analiza aristas, orientación y característica de Euler.
    pub fn topology(&self) -> QuadTopology {
        // Por arista no dirigida: (caras en sentido a→b, caras en sentido b→a)
        let mut edges: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
        let mut degenerate_faces = 0;
        for face in &self.faces {
            let v = face.v;
            if (0..4).any(|i| (i + 1..4).any(|j| v[i] == v[j])) {
                degenerate_faces += 1;
            }
            for k in 0..4 {
                let (a, b) = (v[k], v[(k + 1) % 4]);
                let entry = edges.entry((a.min(b), a.max(b))).or_default();
                if a < b {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
            }
        }

        let mut topo = QuadTopology {
            degenerate_faces,
            ..Default::default()
        };
        for &(forward, backward) in edges.values() {
            match forward + backward {
                1 => topo.boundary_edges += 1,
                2 if forward != 1 => topo.flipped_edges += 1,
                2 => {}
                _ => topo.non_manifold_edges += 1,
            }
        }

        topo.non_manifold_vertices = self.vertex_fans().iter().filter(|f| f.len() > 1).count();

        let used = {
            let mut used = vec![false; self.vertices.len()];
            for face in &self.faces {
                for &i in &face.v {
                    used[i] = true;
                }
            }
            used.iter().filter(|&&u| u).count()
        };
        topo.euler_characteristic = used as i64 - edges.len() as i64 + self.faces.len() as i64;
        topo
    }

    /// Agrupa las caras de cada vértice en abanicos conectados por aristas.
    /// Devuelve, por vértice, una lista de abanicos (índices de cara).
    fn vertex_fans(&self) -> Vec<Vec<Vec<usize>>> {
        let mut incident: Vec<Vec<usize>> = vec![Vec::new(); self.vertices.len()];
        for (fi, face) in self.faces.iter().enumerate() {
            for &v in &face.v {
                if incident[v].last() != Some(&fi) {
                    incident[v].push(fi);
                }
            }
        }
        incident
            .iter()
            .enumerate()
            .map(|(v, faces)| {
                // Unión de caras que comparten una arista que sale de v
                let mut parent: Vec<usize> = (0..faces.len()).collect();
                fn find(p: &mut [usize], mut i: usize) -> usize {
                    while p[i] != i {
                        p[i] = p[p[i]];
                        i = p[i];
                    }
                    i
                }
                let mut by_edge: HashMap<usize, usize> = HashMap::new();
                for (local, &fi) in faces.iter().enumerate() {
                    let f = self.faces[fi].v;
                    let k = f.iter().position(|&x| x == v).expect("cara incidente");
                    for other in [f[(k + 1) % 4], f[(k + 3) % 4]] {
                        match by_edge.get(&other) {
                            Some(&prev) => {
                                let (a, b) = (find(&mut parent, prev), find(&mut parent, local));
                                parent[a] = b;
                            }
                            None => {
                                by_edge.insert(other, local);
                            }
                        }
                    }
                }
                let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
                for (local, &fi) in faces.iter().enumerate() {
                    groups.entry(find(&mut parent, local)).or_default().push(fi);
                }
                let mut fans: Vec<Vec<usize>> = groups.into_values().collect();
                fans.sort();
                fans
            })
            .collect()
    }

    /// Conserva las `keep` componentes conexas con más caras y descarta los
    /// vértices que quedan sin uso.
    pub(crate) fn keep_largest_components(&mut self, keep: usize) {
        let mut parent: Vec<usize> = (0..self.vertices.len()).collect();
        fn find(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for face in &self.faces {
            for k in 1..4 {
                let (a, b) = (find(&mut parent, face.v[0]), find(&mut parent, face.v[k]));
                parent[a] = b;
            }
        }
        let mut sizes: HashMap<usize, usize> = HashMap::new();
        for face in &self.faces {
            *sizes.entry(find(&mut parent, face.v[0])).or_default() += 1;
        }
        let mut ranked: Vec<(usize, usize)> = sizes.into_iter().map(|(root, n)| (n, root)).collect();
        ranked.sort_unstable_by(|a, b| b.cmp(a));
        let kept: std::collections::HashSet<usize> =
            ranked.iter().take(keep).map(|&(_, root)| root).collect();
        self.faces.retain(|f| kept.contains(&find(&mut parent, f.v[0])));
        self.remove_unused_vertices();
    }

    fn remove_unused_vertices(&mut self) {
        let mut remap = vec![usize::MAX; self.vertices.len()];
        let mut vertices = Vec::new();
        for face in &mut self.faces {
            for v in &mut face.v {
                if remap[*v] == usize::MAX {
                    remap[*v] = vertices.len();
                    vertices.push(self.vertices[*v]);
                }
                *v = remap[*v];
            }
        }
        self.vertices = vertices;
    }

    /// Separa los pellizcos: cada abanico adicional de un vértice pasa a usar
    /// una copia propia de ese vértice.
    pub(crate) fn split_nonmanifold_vertices(&mut self) {
        let fans = self.vertex_fans();
        for (v, fans) in fans.iter().enumerate() {
            for fan in fans.iter().skip(1) {
                let copy = self.vertices.len();
                self.vertices.push(self.vertices[v]);
                for &fi in fan {
                    for slot in self.faces[fi].v.iter_mut().filter(|x| **x == v) {
                        *slot = copy;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinched_vertex_is_split() {
        // Dos quads que solo comparten el vértice 0
        let mut mesh = QuadMesh {
            vertices: vec![V3::zeros(); 7],
            faces: vec![QuadFace { v: [0, 1, 2, 3] }, QuadFace { v: [0, 4, 5, 6] }],
        };
        assert_eq!(mesh.topology().non_manifold_vertices, 1);
        mesh.split_nonmanifold_vertices();
        let topo = mesh.topology();
        assert_eq!(topo.non_manifold_vertices, 0);
        assert_eq!(topo.euler_characteristic, 2);
    }

    #[test]
    fn keeps_largest_components() {
        let mut mesh = QuadMesh {
            vertices: vec![V3::zeros(); 12],
            faces: vec![
                QuadFace { v: [0, 1, 2, 3] },
                QuadFace { v: [3, 2, 4, 5] },
                QuadFace { v: [8, 9, 10, 11] },
            ],
        };
        mesh.keep_largest_components(1);
        assert_eq!(mesh.faces.len(), 2);
        assert_eq!(mesh.vertices.len(), 6);
    }
}
