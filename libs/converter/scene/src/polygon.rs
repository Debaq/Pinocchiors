//! Caras de más de tres lados sobre primitivas trianguladas.
//!
//! Las primitivas guardan siempre triángulos. Una malla con quads (la
//! retopología, "triángulos a quads") los guarda como pares de triángulos
//! seguidos al comienzo de la primitiva: el par `(a, b, c), (a, c, d)` es el
//! quad `(a, b, c, d)`, y [`Scene::quads`] dice cuántos pares hay. Los
//! formatos que admiten polígonos (OBJ, USD) los escriben como quads; los
//! demás ven los triángulos de siempre.
//!
//! Cada par se comprueba al leerlo: si una operación reordenó o cambió los
//! triángulos, los pares que ya no calzan vuelven a ser dos triángulos.

use crate::{IndexData, Scene};

/// Una cara: triángulo o quad, con índices a los vértices de la primitiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polygon {
    Tri([u32; 3]),
    Quad([u32; 4]),
}

impl Polygon {
    pub fn vertices(&self) -> &[u32] {
        match self {
            Polygon::Tri(v) => v,
            Polygon::Quad(v) => v,
        }
    }

    /// La misma cara recorrida al revés (`flip` de una transformación que refleja).
    pub fn reversed(&self) -> Self {
        match *self {
            Polygon::Tri([a, b, c]) => Polygon::Tri([a, c, b]),
            Polygon::Quad([a, b, c, d]) => Polygon::Quad([a, d, c, b]),
        }
    }
}

/// Cuántos pares de triángulos del comienzo de una primitiva son quads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QuadPairs {
    pub mesh: usize,
    pub primitive: usize,
    pub pairs: usize,
}

/// Las caras de una lista de triángulos cuyos primeros `pairs` pares son
/// quads (los que no calzan quedan como triángulos). Acepta también los
/// pares invertidos `(a, c, b), (a, d, c)` que deja una transformación que
/// refleja.
pub fn polygons(triangles: &[[u32; 3]], pairs: usize) -> Vec<Polygon> {
    let pairs = pairs.min(triangles.len() / 2);
    let mut out = Vec::with_capacity(triangles.len() - pairs);
    for k in 0..pairs {
        let (s, t) = (triangles[2 * k], triangles[2 * k + 1]);
        match quad_of(s, t) {
            Some(q) => out.push(Polygon::Quad(q)),
            None => out.extend([Polygon::Tri(s), Polygon::Tri(t)]),
        }
    }
    out.extend(triangles[2 * pairs..].iter().map(|&t| Polygon::Tri(t)));
    out
}

fn quad_of(s: [u32; 3], t: [u32; 3]) -> Option<[u32; 4]> {
    if s[0] != t[0] {
        return None;
    }
    let distinct = |q: [u32; 4]| (0..4).all(|i| (i + 1..4).all(|j| q[i] != q[j]));
    // (a, b, c) + (a, c, d)
    if s[2] == t[1] {
        return Some([s[0], s[1], s[2], t[2]]).filter(|q| distinct(*q));
    }
    // Invertidos: (a, c, b) + (a, d, c) → (a, d, c, b)
    if s[1] == t[2] {
        return Some([t[0], t[1], t[2], s[2]]).filter(|q| distinct(*q));
    }
    None
}

/// Triángulos de las caras en el orden de [`polygons`]: cada quad, su par
/// `(a, b, c), (a, c, d)`, y después los triángulos sueltos. Devuelve los
/// índices y la cantidad de pares.
pub fn triangulate(faces: &[Polygon]) -> (Vec<u32>, usize) {
    let mut indices = Vec::with_capacity(faces.len() * 6);
    let mut pairs = 0;
    for f in faces {
        if let Polygon::Quad([a, b, c, d]) = *f {
            indices.extend([a, b, c, a, c, d]);
            pairs += 1;
        }
    }
    for f in faces {
        if let Polygon::Tri(t) = *f {
            indices.extend(t);
        }
    }
    (indices, pairs)
}

impl Scene {
    /// Pares de triángulos que son quads al comienzo de una primitiva.
    pub fn quad_pairs(&self, mesh: usize, primitive: usize) -> usize {
        self.quads.iter().find(|q| q.mesh == mesh && q.primitive == primitive).map_or(0, |q| q.pairs)
    }

    /// Cambia la cantidad de pares de una primitiva (0: solo triángulos).
    pub fn set_quad_pairs(&mut self, mesh: usize, primitive: usize, pairs: usize) {
        self.quads.retain(|q| !(q.mesh == mesh && q.primitive == primitive));
        if pairs > 0 {
            self.quads.push(QuadPairs { mesh, primitive, pairs });
        }
    }

    /// Alguna primitiva tiene quads.
    pub fn has_quads(&self) -> bool {
        self.quads.iter().any(|q| q.pairs > 0)
    }

    /// Las caras de una primitiva (triángulos y quads).
    pub fn polygons(&self, mesh: usize, primitive: usize) -> Vec<Polygon> {
        let Some(prim) = self.meshes.get(mesh).and_then(|m| m.primitives.get(primitive)) else { return Vec::new() };
        let count = prim.attributes.iter().find_map(|a| match a {
            crate::VertexAttribute::Positions(p) => Some(p.len()),
            _ => None,
        });
        let Some(count) = count else { return Vec::new() };
        let indices: Vec<u32> = match &prim.indices {
            Some(IndexData::U16(i)) => i.iter().map(|&i| i as u32).collect(),
            Some(IndexData::U32(i)) => i.clone(),
            None => (0..count as u32).collect(),
        };
        // Los mismos triángulos que `world_primitives` (los inválidos fuera)
        let triangles: Vec<[u32; 3]> = indices
            .chunks_exact(3)
            .filter(|t| t.iter().all(|&i| (i as usize) < count))
            .map(|t| [t[0], t[1], t[2]])
            .collect();
        polygons(&triangles, self.quad_pairs(mesh, primitive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_become_quads_and_the_rest_triangles() {
        let tris = [[0, 1, 2], [0, 2, 3], [4, 5, 6]];
        assert_eq!(polygons(&tris, 1), vec![Polygon::Quad([0, 1, 2, 3]), Polygon::Tri([4, 5, 6])]);
        assert_eq!(polygons(&tris, 0).len(), 3);
        // Más pares de los que hay: se limita
        assert_eq!(polygons(&tris, 9).len(), 2);
    }

    #[test]
    fn pairs_that_do_not_match_stay_triangles() {
        let tris = [[0, 1, 2], [3, 4, 5], [0, 1, 2], [0, 2, 3]];
        assert_eq!(polygons(&tris, 2), vec![Polygon::Tri([0, 1, 2]), Polygon::Tri([3, 4, 5]), Polygon::Quad([0, 1, 2, 3])]);
    }

    #[test]
    fn reflected_pairs_are_read_reversed() {
        let flipped = [[0, 2, 1], [0, 3, 2]];
        assert_eq!(polygons(&flipped, 1), vec![Polygon::Quad([0, 3, 2, 1])]);
        assert_eq!(Polygon::Quad([0, 1, 2, 3]).reversed(), Polygon::Quad([0, 3, 2, 1]));
    }

    #[test]
    fn triangulate_round_trips() {
        let faces = [Polygon::Tri([7, 8, 9]), Polygon::Quad([0, 1, 2, 3]), Polygon::Quad([4, 5, 6, 7])];
        let (indices, pairs) = triangulate(&faces);
        assert_eq!(pairs, 2);
        let tris: Vec<[u32; 3]> = indices.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
        assert_eq!(polygons(&tris, pairs), vec![faces[1], faces[2], faces[0]]);
    }
}
