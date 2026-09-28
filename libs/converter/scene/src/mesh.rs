/// Datos de índices de una primitiva.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IndexData {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

/// Atributo de vértice con semántica conocida.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VertexAttribute {
    /// Posiciones [x, y, z] — siempre presente.
    Positions(Vec<[f32; 3]>),
    /// Normales [x, y, z].
    Normals(Vec<[f32; 3]>),
    /// Tangentes [x, y, z, w] donde w indica handedness.
    Tangents(Vec<[f32; 4]>),
    /// Coordenadas UV (set index, datos).
    TexCoords(u32, Vec<[f32; 2]>),
    /// Colores de vértice RGBA.
    Colors(Vec<[f32; 4]>),
    /// Índices de joints para skinning (4 por vértice).
    JointIndices(Vec<[u16; 4]>),
    /// Pesos de joints para skinning (4 por vértice).
    JointWeights(Vec<[f32; 4]>),
}

/// Una primitiva geométrica (submesh con un solo material).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Primitive {
    pub attributes: Vec<VertexAttribute>,
    pub indices: Option<IndexData>,
    pub material: Option<usize>,
}

/// Malla compuesta de una o más primitivas.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mesh {
    pub name: String,
    pub primitives: Vec<Primitive>,
}
