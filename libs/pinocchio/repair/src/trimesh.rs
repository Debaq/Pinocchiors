//! Malla indexada de triángulos: el formato de trabajo de la reparación.
//!
//! La estructura half-edge de [`Mesh`] asume una malla manifold y orientada:
//! con aristas compartidas por 3+ caras o caras volteadas, los `twin` quedan
//! mal emparejados. Por eso el análisis y la reparación trabajan sobre una
//! sopa indexada y solo vuelven a half-edge al final, cuando la malla ya es
//! válida.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

/// Malla de triángulos indexada (posiciones + índices).
#[derive(Debug, Clone, Default)]
pub struct TriMesh {
    /// Posiciones de los vértices
    pub positions: Vec<Vector3>,
    /// Triángulos como índices en `positions`
    pub triangles: Vec<[usize; 3]>,
}

impl TriMesh {
    /// Crea una malla a partir de posiciones y triángulos.
    pub fn new(positions: Vec<Vector3>, triangles: Vec<[usize; 3]>) -> Self {
        Self { positions, triangles }
    }

    /// Extrae posiciones y triángulos de una malla half-edge.
    pub fn from_mesh(mesh: &Mesh) -> Self {
        Self {
            positions: mesh.vertices.iter().map(|v| v.position).collect(),
            triangles: (0..mesh.num_faces()).map(|f| mesh.get_face_vertices(f)).collect(),
        }
    }

    /// Construye la malla half-edge equivalente.
    pub fn to_mesh(&self) -> Mesh {
        if self.triangles.is_empty() {
            return Mesh::new();
        }
        Mesh::from_triangles(&self.positions, &self.triangles)
    }

    /// Número de vértices
    pub fn num_vertices(&self) -> usize {
        self.positions.len()
    }

    /// Número de triángulos
    pub fn num_faces(&self) -> usize {
        self.triangles.len()
    }

    /// Posiciones de las tres esquinas de una cara
    pub fn corners(&self, face: usize) -> [Vector3; 3] {
        self.triangles[face].map(|v| self.positions[v])
    }

    /// Producto cruz de las aristas de una cara: normal × 2·área
    pub fn face_cross(&self, face: usize) -> Vector3 {
        let [a, b, c] = self.corners(face);
        (b - a).cross(&(c - a))
    }

    /// Área de una cara
    pub fn face_area(&self, face: usize) -> Real {
        self.face_cross(face).length() * 0.5
    }

    /// Área total
    pub fn area(&self) -> Real {
        (0..self.num_faces()).map(|f| self.face_area(f)).sum()
    }

    /// Volumen con signo (positivo si las normales apuntan hacia afuera en una
    /// malla cerrada)
    pub fn signed_volume(&self) -> Real {
        signed_volume_of(&self.positions, self.triangles.iter())
    }

    /// Diagonal de la caja envolvente de los vértices finitos (1 si no hay)
    pub fn diagonal(&self) -> Real {
        let mut min = Vector3::new(Real::INFINITY, Real::INFINITY, Real::INFINITY);
        let mut max = -min;
        for p in self.positions.iter().filter(|p| is_finite(p)) {
            min = min.min(p);
            max = max.max(p);
        }
        let d = (max - min).length();
        if d.is_finite() && d > 0.0 { d } else { 1.0 }
    }

    /// Conserva solo las caras para las que `keep` devuelve true.
    ///
    /// Devuelve cuántas se eliminaron.
    pub fn retain_faces(&mut self, mut keep: impl FnMut(usize, &[usize; 3]) -> bool) -> usize {
        let before = self.triangles.len();
        let mut index = 0;
        self.triangles.retain(|t| {
            let k = keep(index, t);
            index += 1;
            k
        });
        before - self.triangles.len()
    }

    /// Elimina los vértices que ninguna cara usa y reindexa.
    ///
    /// Devuelve cuántos se eliminaron.
    pub fn remove_unreferenced_vertices(&mut self) -> usize {
        let mut new_index = vec![usize::MAX; self.positions.len()];
        let mut positions = Vec::with_capacity(self.positions.len());
        for t in &mut self.triangles {
            for v in t.iter_mut() {
                if new_index[*v] == usize::MAX {
                    new_index[*v] = positions.len();
                    positions.push(self.positions[*v]);
                }
                *v = new_index[*v];
            }
        }
        let removed = self.positions.len() - positions.len();
        self.positions = positions;
        removed
    }
}

impl From<&Mesh> for TriMesh {
    fn from(mesh: &Mesh) -> Self {
        Self::from_mesh(mesh)
    }
}

/// Volumen con signo de un conjunto de caras
pub(crate) fn signed_volume_of<'a>(
    positions: &[Vector3],
    triangles: impl Iterator<Item = &'a [usize; 3]>,
) -> Real {
    triangles
        .map(|t| {
            let [a, b, c] = t.map(|v| positions[v]);
            a.dot(&b.cross(&c))
        })
        .sum::<Real>()
        / 6.0
}

pub(crate) fn is_finite(p: &Vector3) -> bool {
    p.x().is_finite() && p.y().is_finite() && p.z().is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mesh() {
        let positions = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let tm = TriMesh::new(positions, vec![[0, 1, 2]]);
        let back = TriMesh::from_mesh(&tm.to_mesh());
        assert_eq!(back.triangles, vec![[0, 1, 2]]);
        assert!((back.area() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn remove_unreferenced() {
        let positions = vec![Vector3::zero(), Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z()];
        let mut tm = TriMesh::new(positions, vec![[0, 2, 3]]);
        assert_eq!(tm.remove_unreferenced_vertices(), 1);
        assert_eq!(tm.triangles, vec![[0, 1, 2]]);
        assert_eq!(tm.positions[1], Vector3::unit_y());
    }
}
