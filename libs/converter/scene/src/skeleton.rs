use glam::Mat4;

/// Un joint individual dentro del esqueleto.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Joint {
    pub name: String,
    pub children: Vec<usize>,
    pub inverse_bind_matrix: Mat4,
    pub local_transform: Mat4,
    /// Índice del nodo de Scene correspondiente a este joint.
    /// Se usa para mapear Channel.node → joint en animaciones.
    pub node_index: Option<usize>,
}

/// Esqueleto completo para skinning.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Skeleton {
    pub name: String,
    pub joints: Vec<Joint>,
    pub roots: Vec<usize>,
}
