//! Hueso del esqueleto

use pinocchio_math::Vector3;

/// Hueso de un esqueleto
#[derive(Debug, Clone)]
pub struct Bone {
    /// Nombre del hueso
    pub name: String,
    /// Posición del hueso (extremo)
    pub position: Vector3,
    /// Índice del hueso padre (None para la raíz)
    pub parent: Option<usize>,
    /// Indica si este hueso es un extremo (leaf)
    pub is_leaf: bool,
}

impl Bone {
    /// Crea un nuevo hueso
    pub fn new(name: impl Into<String>, position: Vector3) -> Self {
        Self {
            name: name.into(),
            position,
            parent: None,
            is_leaf: false,
        }
    }

    /// Crea un hueso con padre
    pub fn with_parent(name: impl Into<String>, position: Vector3, parent: usize) -> Self {
        Self {
            name: name.into(),
            position,
            parent: Some(parent),
            is_leaf: false,
        }
    }

    /// Marca el hueso como extremo (leaf)
    pub fn as_leaf(mut self) -> Self {
        self.is_leaf = true;
        self
    }
}
