//! Topología de aristas de una sopa de triángulos.
//!
//! No asume nada de la malla: una arista puede tener cualquier número de
//! caras y en cualquier dirección. Se construye ordenando las medias aristas,
//! sin tablas hash, así que es lineal-logarítmica y cabe bien en caché.

/// Referencia a la arista local `edge` (de `t[edge]` a `t[(edge + 1) % 3]`)
/// de la cara `face`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceEdge {
    /// Índice de la cara
    pub face: usize,
    /// Arista local (0, 1 o 2)
    pub edge: u8,
}

/// Aristas no dirigidas y sus caras incidentes.
#[derive(Debug, Clone)]
pub struct EdgeTopology {
    /// Extremos de cada arista, `[menor, mayor]`, en orden lexicográfico
    pub edges: Vec<[usize; 2]>,
    /// Para cada cara, el índice de arista de cada una de sus 3 aristas locales
    pub face_edges: Vec<[usize; 3]>,
    offsets: Vec<usize>,
    incidences: Vec<FaceEdge>,
}

impl EdgeTopology {
    /// Construye la topología. Las caras con índices repetidos también se
    /// registran (una arista `[v, v]`), así que conviene sanearlas antes.
    ///
    /// Ordena las medias aristas por cubetas según su vértice menor y dentro
    /// de cada cubeta por el mayor: lineal salvo cubetas grandes.
    pub fn build(triangles: &[[usize; 3]]) -> Self {
        let nv = triangles.iter().flatten().copied().max().map_or(0, |m| m + 1);
        let mut start = vec![0u32; nv + 1];
        for t in triangles {
            for i in 0..3 {
                start[t[i].min(t[(i + 1) % 3]) + 1] += 1;
            }
        }
        for v in 0..nv {
            start[v + 1] += start[v];
        }
        // Cada cubeta guarda (vértice mayor << 32) | (3·cara + arista local)
        let mut bucket = vec![0u64; triangles.len() * 3];
        let mut fill = start.clone();
        for (face, t) in triangles.iter().enumerate() {
            for i in 0..3 {
                let (a, b) = (t[i], t[(i + 1) % 3]);
                let slot = &mut fill[a.min(b)];
                bucket[*slot as usize] = ((a.max(b) as u64) << 32) | (3 * face + i) as u64;
                *slot += 1;
            }
        }

        let mut edges = Vec::with_capacity(triangles.len() * 3 / 2 + 1);
        let mut offsets = Vec::with_capacity(triangles.len() * 3 / 2 + 2);
        let mut incidences = Vec::with_capacity(triangles.len() * 3);
        let mut face_edges = vec![[usize::MAX; 3]; triangles.len()];
        for v in 0..nv {
            let range = &mut bucket[start[v] as usize..start[v + 1] as usize];
            range.sort_unstable();
            let mut previous = u64::MAX;
            for &entry in range.iter() {
                let other = entry >> 32;
                if other != previous {
                    previous = other;
                    edges.push([v, other as usize]);
                    offsets.push(incidences.len());
                }
                let corner = (entry & 0xFFFF_FFFF) as usize;
                let fe = FaceEdge { face: corner / 3, edge: (corner % 3) as u8 };
                face_edges[fe.face][fe.edge as usize] = edges.len() - 1;
                incidences.push(fe);
            }
        }
        offsets.push(incidences.len());

        Self { edges, face_edges, offsets, incidences }
    }

    /// Número de aristas no dirigidas
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Caras incidentes a una arista
    pub fn faces(&self, edge: usize) -> &[FaceEdge] {
        &self.incidences[self.offsets[edge]..self.offsets[edge + 1]]
    }

    /// Número de caras incidentes a una arista
    pub fn valence(&self, edge: usize) -> usize {
        self.offsets[edge + 1] - self.offsets[edge]
    }

    /// Busca la arista entre dos vértices
    pub fn find(&self, a: usize, b: usize) -> Option<usize> {
        self.edges.binary_search(&[a.min(b), a.max(b)]).ok()
    }
}

/// Vértices de la arista local `fe` en el sentido de la cara.
pub fn directed(triangles: &[[usize; 3]], fe: FaceEdge) -> (usize, usize) {
    let t = &triangles[fe.face];
    (t[fe.edge as usize], t[(fe.edge as usize + 1) % 3])
}

/// Vértice de la cara opuesto a su arista local `edge`.
pub fn opposite(t: &[usize; 3], edge: u8) -> usize {
    t[(edge as usize + 2) % 3]
}

/// Union-find con compresión de caminos y unión por tamaño.
#[derive(Debug, Clone)]
pub struct UnionFind {
    parent: Vec<usize>,
    size: Vec<u32>,
}

impl UnionFind {
    /// `n` conjuntos unitarios
    pub fn new(n: usize) -> Self {
        Self { parent: (0..n).collect(), size: vec![1; n] }
    }

    /// Representante del conjunto de `x`
    pub fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Une los conjuntos de `a` y `b`; devuelve true si eran distintos
    pub fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut ra, mut rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        if self.size[ra] < self.size[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        self.size[ra] += self.size[rb];
        true
    }
}

/// Componentes conexas de caras unidas por aristas compartidas (de cualquier
/// valencia ≥ 2). Devuelve la componente de cada cara y el número de
/// componentes; los índices siguen el orden de la primera cara de cada una.
pub fn face_components(topology: &EdgeTopology, num_faces: usize) -> (Vec<usize>, usize) {
    let mut uf = UnionFind::new(num_faces);
    for e in 0..topology.num_edges() {
        let faces = topology.faces(e);
        for w in faces.windows(2) {
            uf.union(w[0].face, w[1].face);
        }
    }
    let mut label = vec![usize::MAX; num_faces];
    let mut component = Vec::with_capacity(num_faces);
    let mut count = 0;
    for f in 0..num_faces {
        let root = uf.find(f);
        if label[root] == usize::MAX {
            label[root] = count;
            count += 1;
        }
        component.push(label[root]);
    }
    (component, count)
}

/// Agrupa las esquinas de las caras (esquina `3·cara + i` = vértice
/// `triangles[cara][i]`) en abanicos alrededor de cada vértice.
///
/// `links` son pares de caras `(f, g, [u, v])` que se consideran pegadas por
/// la arista `u-v`: sus esquinas en `u` y en `v` quedan en el mismo abanico.
/// En una malla manifold cada vértice tiene un único abanico.
pub fn corner_fans(
    triangles: &[[usize; 3]],
    links: impl IntoIterator<Item = (usize, usize, [usize; 2])>,
) -> UnionFind {
    let mut fans = UnionFind::new(triangles.len() * 3);
    let corner = |face: usize, v: usize| {
        3 * face + triangles[face].iter().position(|&x| x == v).expect("vértice de la arista")
    };
    for (f, g, edge) in links {
        for v in edge {
            fans.union(corner(f, v), corner(g, v));
        }
    }
    fans
}

/// Pares de caras de las aristas con exactamente dos caras
pub fn manifold_links(topology: &EdgeTopology) -> impl Iterator<Item = (usize, usize, [usize; 2])> + '_ {
    (0..topology.num_edges()).filter(|&e| topology.valence(e) == 2).map(move |e| {
        let f = topology.faces(e);
        (f[0].face, f[1].face, topology.edges[e])
    })
}

/// Un ciclo de aristas de borde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryLoop {
    /// Vértices en el sentido de las caras que tocan el borde
    pub vertices: Vec<usize>,
    /// False si el recorrido no pudo cerrarse (orientación inconsistente)
    pub closed: bool,
}

/// Recorre las aristas de borde (valencia 1) en el sentido de sus caras.
///
/// En una malla orientada y manifold cada vértice de borde tiene exactamente
/// una arista de borde saliente y una entrante, así que los ciclos son únicos.
/// En vértices con varias salidas (non-manifold) se toma la primera libre;
/// igual se obtienen ciclos cerrados si la orientación es consistente.
pub fn boundary_loops(triangles: &[[usize; 3]], topology: &EdgeTopology) -> Vec<BoundaryLoop> {
    // Aristas de borde dirigidas, ordenadas por vértice de salida
    let mut out: Vec<(usize, usize)> = (0..topology.num_edges())
        .filter(|&e| topology.valence(e) == 1)
        .map(|e| directed(triangles, topology.faces(e)[0]))
        .collect();
    out.sort_unstable();
    let mut used = vec![false; out.len()];

    let first_from = |v: usize, used: &[bool]| -> Option<usize> {
        let start = out.partition_point(|&(a, _)| a < v);
        (start..out.len()).take_while(|&i| out[i].0 == v).find(|&i| !used[i])
    };

    let mut loops = Vec::new();
    for start in 0..out.len() {
        if used[start] {
            continue;
        }
        let origin = out[start].0;
        let mut vertices = vec![origin];
        let mut current = start;
        let closed = loop {
            used[current] = true;
            let next_vertex = out[current].1;
            if next_vertex == origin {
                break true;
            }
            vertices.push(next_vertex);
            match first_from(next_vertex, &used) {
                Some(next) => current = next,
                None => break false,
            }
        };
        loops.push(BoundaryLoop { vertices, closed });
    }
    loops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_valence() {
        // Dos triángulos que comparten la arista 1-2
        let tris = [[0, 1, 2], [2, 1, 3]];
        let topo = EdgeTopology::build(&tris);
        assert_eq!(topo.num_edges(), 5);
        let shared = topo.find(1, 2).unwrap();
        assert_eq!(topo.valence(shared), 2);
        assert_eq!(topo.valence(topo.find(0, 1).unwrap()), 1);
        assert_eq!(topo.face_edges[0][1], shared);
        assert_eq!(topo.face_edges[1][0], shared);
    }

    #[test]
    fn loops_of_open_quad() {
        let tris = [[0, 1, 2], [0, 2, 3]];
        let topo = EdgeTopology::build(&tris);
        let loops = boundary_loops(&tris, &topo);
        assert_eq!(loops.len(), 1);
        assert!(loops[0].closed);
        assert_eq!(loops[0].vertices.len(), 4);
    }

    #[test]
    fn components() {
        let tris = [[0, 1, 2], [2, 1, 3], [4, 5, 6]];
        let topo = EdgeTopology::build(&tris);
        let (comp, n) = face_components(&topo, tris.len());
        assert_eq!(n, 2);
        assert_eq!(comp, vec![0, 0, 1]);
    }
}
