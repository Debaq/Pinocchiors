//! Arista half-edge

/// Arista en una malla half-edge
///
/// Cada arista direccionada tiene:
/// - Un vértice de destino
/// - La arista gemela (opuesta)
/// - La siguiente arista en la cara
/// - La cara a la que pertenece (None si es borde)
#[derive(Debug, Clone, Copy)]
pub struct MeshEdge {
    /// Índice del vértice de destino
    pub vertex: usize,
    /// Índice de la arista gemela (twin/opposite)
    pub twin: Option<usize>,
    /// Índice de la siguiente arista en la cara
    pub next: usize,
    /// Índice de la cara (None si es arista de borde)
    pub face: Option<usize>,
}

impl MeshEdge {
    /// Crea una nueva arista
    pub fn new(vertex: usize, next: usize) -> Self {
        Self {
            vertex,
            twin: None,
            next,
            face: None,
        }
    }

    /// Verifica si es una arista de borde
    pub fn is_boundary(&self) -> bool {
        self.face.is_none()
    }
}

impl Default for MeshEdge {
    fn default() -> Self {
        Self {
            vertex: 0,
            twin: None,
            next: 0,
            face: None,
        }
    }
}
