//! Malla half-edge

use crate::{MeshEdge, MeshVertex};
use pinocchio_math::{Real, Rect, Vector3};
use std::collections::HashMap;

/// Malla 3D con estructura half-edge
#[derive(Debug, Clone)]
pub struct Mesh {
    /// Vértices de la malla
    pub vertices: Vec<MeshVertex>,
    /// Aristas half-edge
    pub edges: Vec<MeshEdge>,
    /// Índices de caras (cada cara apunta a una arista)
    pub faces: Vec<usize>,
}

impl Mesh {
    /// Crea una malla vacía
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// Como [`from_triangles`](Self::from_triangles), pero devuelve un error en
    /// vez de hacer panic si algún índice no existe.
    pub fn try_from_triangles(positions: &[Vector3], indices: &[[usize; 3]]) -> Result<Self, String> {
        if let Some((face, tri)) = indices
            .iter()
            .enumerate()
            .find(|(_, t)| t.iter().any(|&i| i >= positions.len()))
        {
            return Err(format!(
                "la cara {face} referencia {tri:?} pero solo hay {} vértices",
                positions.len()
            ));
        }
        Ok(Self::from_triangles(positions, indices))
    }

    /// Crea una malla a partir de posiciones y triángulos.
    ///
    /// # Panics
    ///
    /// Si algún índice es `>= positions.len()`; usar
    /// [`try_from_triangles`](Self::try_from_triangles) con datos no confiables.
    pub fn from_triangles(positions: &[Vector3], indices: &[[usize; 3]]) -> Self {
        let mut mesh = Self::new();

        // Añadir vértices
        for &pos in positions {
            mesh.vertices.push(MeshVertex::new(pos));
        }

        // Mapa para encontrar aristas gemelas
        let mut edge_map: HashMap<(usize, usize), usize> = HashMap::new();

        // Añadir caras y aristas
        for triangle in indices {
            let face_idx = mesh.faces.len();
            let base_edge = mesh.edges.len();

            // Crear las 3 aristas de la cara
            for i in 0..3 {
                let v0 = triangle[i];
                let v1 = triangle[(i + 1) % 3];
                let next = base_edge + (i + 1) % 3;

                let mut edge = MeshEdge::new(v1, next);
                edge.face = Some(face_idx);

                // Buscar arista gemela
                if let Some(&twin_idx) = edge_map.get(&(v1, v0)) {
                    edge.twin = Some(twin_idx);
                    mesh.edges[twin_idx].twin = Some(mesh.edges.len());
                }

                // Registrar esta arista
                edge_map.insert((v0, v1), mesh.edges.len());

                // Actualizar vértice
                mesh.vertices[v0].edge = Some(mesh.edges.len());

                mesh.edges.push(edge);
            }

            // Registrar la cara
            mesh.faces.push(base_edge);
        }

        mesh.compute_vertex_normals();
        mesh
    }

    /// Número de vértices
    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    /// Número de aristas (half-edges)
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Número de caras
    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }

    /// Calcula el bounding box de la malla
    pub fn bounding_box(&self) -> Rect {
        self.vertices
            .iter()
            .map(|v| v.position)
            .collect()
    }

    /// Calcula las normales de los vértices
    pub fn compute_vertex_normals(&mut self) {
        // Resetear normales
        for v in &mut self.vertices {
            v.normal = Vector3::zero();
        }

        // Acumular normales de caras
        for &face_edge in &self.faces {
            let e0 = &self.edges[face_edge];
            let e1 = &self.edges[e0.next];

            let v0 = self.get_edge_origin(face_edge);
            let v1 = e0.vertex;
            let v2 = e1.vertex;

            let p0 = self.vertices[v0].position;
            let p1 = self.vertices[v1].position;
            let p2 = self.vertices[v2].position;

            let edge1 = p1 - p0;
            let edge2 = p2 - p0;
            let normal = edge1.cross(&edge2);

            self.vertices[v0].normal += normal;
            self.vertices[v1].normal += normal;
            self.vertices[v2].normal += normal;
        }

        // Normalizar
        for v in &mut self.vertices {
            if let Some(n) = v.normal.try_normalize() {
                v.normal = n;
            }
        }
    }

    /// Obtiene el vértice de origen de una arista
    pub fn get_edge_origin(&self, edge_idx: usize) -> usize {
        // El origen de una arista es el destino de la arista previa
        let edge = &self.edges[edge_idx];
        if let Some(twin) = edge.twin {
            self.edges[twin].vertex
        } else {
            // Buscar la arista previa en la cara
            let mut prev = edge.next;
            while self.edges[prev].next != edge_idx {
                prev = self.edges[prev].next;
            }
            self.edges[prev].vertex
        }
    }

    /// Normaliza la malla al cubo unitario centrado en el origen
    pub fn normalize_bounding_box(&mut self) {
        let bbox = self.bounding_box();
        let center = bbox.center();
        let scale = 1.0 / bbox.longest_axis_length();

        for v in &mut self.vertices {
            v.position = (v.position - center) * scale;
        }
    }

    /// Verifica la integridad de la estructura half-edge
    pub fn integrity_check(&self) -> Result<(), String> {
        // Verificar que cada arista tiene un gemelo válido o es borde
        for (i, edge) in self.edges.iter().enumerate() {
            if let Some(twin) = edge.twin {
                if twin >= self.edges.len() {
                    return Err(format!("Arista {} tiene twin inválido {}", i, twin));
                }
                if self.edges[twin].twin != Some(i) {
                    return Err(format!("Arista {} y su twin {} no son recíprocos", i, twin));
                }
            }

            if edge.next >= self.edges.len() {
                return Err(format!("Arista {} tiene next inválido {}", i, edge.next));
            }

            if edge.vertex >= self.vertices.len() {
                return Err(format!("Arista {} tiene vértice inválido {}", i, edge.vertex));
            }
        }

        // Verificar que cada cara forma un ciclo
        for (face_idx, &start_edge) in self.faces.iter().enumerate() {
            let mut edge = start_edge;
            let mut count = 0;
            loop {
                count += 1;
                edge = self.edges[edge].next;
                if edge == start_edge {
                    break;
                }
                if count > self.edges.len() {
                    return Err(format!("Cara {} tiene ciclo infinito", face_idx));
                }
            }
            if count != 3 {
                return Err(format!("Cara {} tiene {} aristas (esperado 3)", face_idx, count));
            }
        }

        Ok(())
    }

    /// Obtiene los índices de los vértices de una cara
    pub fn get_face_vertices(&self, face_idx: usize) -> [usize; 3] {
        let e0 = self.faces[face_idx];
        let e1 = self.edges[e0].next;

        [
            self.get_edge_origin(e0),
            self.edges[e0].vertex,
            self.edges[e1].vertex,
        ]
    }

    /// Obtiene las posiciones de los vértices de una cara
    pub fn get_face_positions(&self, face_idx: usize) -> [Vector3; 3] {
        let verts = self.get_face_vertices(face_idx);
        [
            self.vertices[verts[0]].position,
            self.vertices[verts[1]].position,
            self.vertices[verts[2]].position,
        ]
    }

    /// Calcula la normal de una cara
    pub fn get_face_normal(&self, face_idx: usize) -> Vector3 {
        let [p0, p1, p2] = self.get_face_positions(face_idx);
        let edge1 = p1 - p0;
        let edge2 = p2 - p0;
        edge1.cross(&edge2).normalize()
    }

    /// Calcula el área de una cara
    pub fn get_face_area(&self, face_idx: usize) -> Real {
        let [p0, p1, p2] = self.get_face_positions(face_idx);
        let edge1 = p1 - p0;
        let edge2 = p2 - p0;
        edge1.cross(&edge2).length() * 0.5
    }

    /// Calcula el área total de la malla
    pub fn total_area(&self) -> Real {
        (0..self.num_faces())
            .map(|i| self.get_face_area(i))
            .sum()
    }

    /// Calcula el volumen de la malla (asumiendo que es cerrada)
    pub fn volume(&self) -> Real {
        let mut vol = 0.0;
        for i in 0..self.num_faces() {
            let [p0, p1, p2] = self.get_face_positions(i);
            vol += p0.dot(&p1.cross(&p2));
        }
        vol.abs() / 6.0
    }

    /// Obtiene los índices de las caras vecinas (adyacentes por arista).
    ///
    /// Para cada una de las 3 aristas de la cara, si tiene un twin (arista gemela),
    /// devuelve el índice de la cara a la que pertenece ese twin.
    /// Las aristas de borde (sin twin) no generan vecinos.
    ///
    /// # Returns
    /// Vector con 0 a 3 índices de caras vecinas.
    pub fn get_face_neighbors(&self, face_idx: usize) -> Vec<usize> {
        let mut neighbors = Vec::with_capacity(3);
        let start_edge = self.faces[face_idx];

        // Recorrer las 3 aristas de la cara
        let mut edge_idx = start_edge;
        for _ in 0..3 {
            let edge = &self.edges[edge_idx];

            // Si la arista tiene twin, su cara es vecina
            if let Some(twin_idx) = edge.twin {
                if let Some(neighbor_face) = self.edges[twin_idx].face {
                    neighbors.push(neighbor_face);
                }
            }

            edge_idx = edge.next;
        }

        neighbors
    }

    /// Obtiene las aristas de borde (sin twin) de la malla.
    ///
    /// Útil para detectar límites de mallas abiertas.
    pub fn get_boundary_edges(&self) -> Vec<usize> {
        self.edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.twin.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Verifica si la malla es cerrada (sin bordes).
    pub fn is_closed(&self) -> bool {
        self.edges.iter().all(|e| e.twin.is_some())
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tetrahedron() -> Mesh {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
            [1, 3, 2],
            [2, 3, 0],
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_mesh_creation() {
        let mesh = make_tetrahedron();
        assert_eq!(mesh.num_vertices(), 4);
        assert_eq!(mesh.num_faces(), 4);
        assert_eq!(mesh.num_edges(), 12); // 4 caras * 3 aristas
    }

    #[test]
    fn test_integrity() {
        let mesh = make_tetrahedron();
        assert!(mesh.integrity_check().is_ok());
    }

    #[test]
    fn test_bounding_box() {
        let mesh = make_tetrahedron();
        let bbox = mesh.bounding_box();
        assert!(bbox.min.x() >= 0.0);
        assert!(bbox.max.x() <= 1.0);
    }

    #[test]
    fn test_face_area() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        ];
        let faces = vec![[0, 1, 2]];
        let mesh = Mesh::from_triangles(&vertices, &faces);

        let area = mesh.get_face_area(0);
        assert!((area - 0.5).abs() < 1e-10);
    }
}
