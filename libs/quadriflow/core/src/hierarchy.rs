//! Jerarquía multiresolución del grafo de vértices.
//!
//! Cada nivel empareja vértices vecinos del anterior (con normales parecidas),
//! así que tiene la mitad de vértices. Los campos se resuelven primero en el
//! nivel más grueso y se refinan hacia el más fino, lo que propaga la
//! información a toda la malla en pocas iteraciones.

use crate::surface::{Constraint, VertexGraph};
use crate::V3;

/// Un nivel de la jerarquía.
#[derive(Debug, Clone)]
pub(crate) struct Level {
    pub pos: Vec<V3>,
    pub nrm: Vec<V3>,
    pub area: Vec<f64>,
    /// Vecinos con peso, en formato CSR.
    pub adj_start: Vec<usize>,
    pub adj: Vec<(u32, f64)>,
    pub constraint: Vec<Constraint>,
    pub on_boundary: Vec<bool>,
    /// Para cada vértice, su vértice en el nivel siguiente (más grueso).
    pub parent: Vec<u32>,
    /// Hijos en el nivel anterior (más fino); el segundo es `NONE` si hay uno solo.
    pub children: Vec<[u32; 2]>,
    /// Coloreo del grafo: los vértices de un mismo grupo no son vecinos, así
    /// que Gauss-Seidel puede actualizarlos en paralelo.
    pub phases: Vec<Vec<u32>>,
}

pub(crate) const NONE: u32 = u32::MAX;

impl Level {
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn neighbors(&self, i: usize) -> &[(u32, f64)] {
        &self.adj[self.adj_start[i]..self.adj_start[i + 1]]
    }

    /// Coloreo voraz en orden de índice (determinista).
    fn color(&mut self) {
        let mut color = vec![u32::MAX; self.len()];
        let mut taken: Vec<bool> = Vec::new();
        self.phases.clear();
        for i in 0..self.len() {
            taken.clear();
            for &(j, _) in self.neighbors(i) {
                let c = color[j as usize];
                if c != u32::MAX {
                    if taken.len() <= c as usize {
                        taken.resize(c as usize + 1, false);
                    }
                    taken[c as usize] = true;
                }
            }
            let c = taken.iter().position(|&t| !t).unwrap_or(taken.len());
            color[i] = c as u32;
            if self.phases.len() <= c {
                self.phases.resize(c + 1, Vec::new());
            }
            self.phases[c].push(i as u32);
        }
    }

    fn from_graph(g: &VertexGraph) -> Self {
        Self {
            pos: g.pos.clone(),
            nrm: g.nrm.clone(),
            area: g.area.clone(),
            adj_start: g.adj_start.clone(),
            adj: g.adj.iter().map(|&j| (j, 1.0)).collect(),
            constraint: g.constraint.clone(),
            on_boundary: g.on_boundary.clone(),
            parent: Vec::new(),
            children: (0..g.len() as u32).map(|i| [i, NONE]).collect(),
            phases: Vec::new(),
        }
    }

    /// Empareja vértices vecinos y devuelve el nivel más grueso; llena `self.parent`.
    fn downsample(&mut self) -> Level {
        let n = self.len();
        let mut pairs: Vec<(f64, u32, u32)> = Vec::new();
        for i in 0..n {
            for &(j, _) in self.neighbors(i) {
                if (i as u32) < j {
                    let j = j as usize;
                    let (ai, aj) = (self.area[i].max(1e-300), self.area[j].max(1e-300));
                    let score = self.nrm[i].dot(&self.nrm[j]) * (ai / aj).max(aj / ai);
                    pairs.push((score, i as u32, j as u32));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then((a.1, a.2).cmp(&(b.1, b.2))));

        let mut parent = vec![NONE; n];
        let mut children: Vec<[u32; 2]> = Vec::with_capacity(n / 2 + 1);
        for (_, i, j) in pairs {
            if parent[i as usize] == NONE && parent[j as usize] == NONE {
                parent[i as usize] = children.len() as u32;
                parent[j as usize] = children.len() as u32;
                children.push([i, j]);
            }
        }
        for (i, p) in parent.iter_mut().enumerate() {
            if *p == NONE {
                *p = children.len() as u32;
                children.push([i as u32, NONE]);
            }
        }

        let m = children.len();
        let mut pos = Vec::with_capacity(m);
        let mut nrm = Vec::with_capacity(m);
        let mut area = Vec::with_capacity(m);
        let mut constraint = Vec::with_capacity(m);
        let mut on_boundary = Vec::with_capacity(m);
        for &[a, b] in &children {
            let a = a as usize;
            on_boundary.push(self.on_boundary[a] || (b != NONE && self.on_boundary[b as usize]));
            if b == NONE {
                pos.push(self.pos[a]);
                nrm.push(self.nrm[a]);
                area.push(self.area[a]);
                constraint.push(self.constraint[a]);
                continue;
            }
            let b = b as usize;
            let (wa, wb) = (self.area[a], self.area[b]);
            let total = wa + wb;
            let (wa, wb) = if total > 0.0 { (wa / total, wb / total) } else { (0.5, 0.5) };
            pos.push(self.pos[a] * wa + self.pos[b] * wb);
            nrm.push(
                (self.nrm[a] * wa + self.nrm[b] * wb)
                    .try_normalize(1e-12)
                    .unwrap_or(self.nrm[a]),
            );
            area.push(total);
            constraint.push(self.constraint[a].strongest(self.constraint[b]));
        }

        let mut adj_start = Vec::with_capacity(m + 1);
        let mut adj: Vec<(u32, f64)> = Vec::new();
        adj_start.push(0);
        let mut scratch: Vec<(u32, f64)> = Vec::new();
        for (c, kids) in children.iter().enumerate() {
            scratch.clear();
            for &k in kids.iter().filter(|&&k| k != NONE) {
                for &(j, w) in self.neighbors(k as usize) {
                    let pj = parent[j as usize];
                    if pj != c as u32 {
                        scratch.push((pj, w));
                    }
                }
            }
            scratch.sort_unstable_by_key(|e| e.0);
            for &(j, w) in &scratch {
                if adj.len() > adj_start[c] && adj[adj.len() - 1].0 == j {
                    let last = adj.len() - 1;
                    adj[last].1 += w;
                } else {
                    adj.push((j, w));
                }
            }
            adj_start.push(adj.len());
        }

        self.parent = parent;
        Level {
            pos,
            nrm,
            area,
            adj_start,
            adj,
            constraint,
            on_boundary,
            parent: Vec::new(),
            children,
            phases: Vec::new(),
        }
    }
}

/// Jerarquía completa; `levels[0]` es el nivel más fino.
#[derive(Debug, Clone)]
pub(crate) struct Hierarchy {
    pub levels: Vec<Level>,
}

impl Hierarchy {
    pub fn build(graph: &VertexGraph) -> Self {
        let mut levels = vec![Level::from_graph(graph)];
        loop {
            let fine = levels.last_mut().expect("al menos un nivel");
            if fine.len() <= 1 {
                break;
            }
            let coarse = fine.downsample();
            if coarse.len() == fine.len() {
                // Sin aristas que colapsar (vértices aislados)
                fine.parent.clear();
                break;
            }
            levels.push(coarse);
        }
        for level in &mut levels {
            level.color();
        }
        Self { levels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::Surface;

    #[test]
    fn levels_halve_and_preserve_area() {
        let mut s = Surface {
            positions: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(1.0, 0.0, 0.0),
                V3::new(1.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        s.subdivide(0.05);
        let g = s.vertex_graph(None, 0.7);
        let h = Hierarchy::build(&g);

        assert!(h.levels.len() > 5);
        assert_eq!(h.levels.last().unwrap().len(), 1);
        for pair in h.levels.windows(2) {
            let (fine, coarse) = (&pair[0], &pair[1]);
            assert!(coarse.len() * 2 >= fine.len() && coarse.len() < fine.len());
            let (af, ac): (f64, f64) = (fine.area.iter().sum(), coarse.area.iter().sum());
            assert!((af - ac).abs() < 1e-9);
            // parent y children son inversos
            for (i, &p) in fine.parent.iter().enumerate() {
                assert!(coarse.children[p as usize].contains(&(i as u32)));
            }
            // cada fase es un conjunto independiente que cubre el nivel
            let covered: usize = coarse.phases.iter().map(Vec::len).sum();
            assert_eq!(covered, coarse.len());
            for phase in &coarse.phases {
                for &i in phase {
                    assert!(coarse.neighbors(i as usize).iter().all(|&(j, _)| !phase.contains(&j)));
                }
            }
            // adyacencia simétrica y sin lazos
            for c in 0..coarse.len() {
                for &(j, _) in coarse.neighbors(c) {
                    assert_ne!(j as usize, c);
                    assert!(coarse.neighbors(j as usize).iter().any(|&(k, _)| k as usize == c));
                }
            }
        }
    }
}
