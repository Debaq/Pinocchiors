//! Vértice de malla half-edge

use pinocchio_math::{Real, Vector3};

/// Vértice en una malla half-edge
#[derive(Debug, Clone)]
pub struct MeshVertex {
    /// Posición del vértice
    pub position: Vector3,
    /// Normal del vértice (calculada)
    pub normal: Vector3,
    /// Índice de una arista saliente (half-edge)
    pub edge: Option<usize>,
}

impl MeshVertex {
    /// Crea un nuevo vértice
    pub fn new(position: Vector3) -> Self {
        Self {
            position,
            normal: Vector3::zero(),
            edge: None,
        }
    }

    /// Crea un vértice desde coordenadas
    pub fn from_coords(x: Real, y: Real, z: Real) -> Self {
        Self::new(Vector3::new(x, y, z))
    }
}

impl Default for MeshVertex {
    fn default() -> Self {
        Self::new(Vector3::zero())
    }
}
