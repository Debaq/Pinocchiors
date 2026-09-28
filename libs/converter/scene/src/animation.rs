/// Tipo de interpolación entre keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

/// Tiempos de los keyframes (en segundos).
pub type KeyframeTimes = Vec<f32>;

/// Valores de los keyframes según el tipo de canal.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum KeyframeValues {
    Translation(Vec<[f32; 3]>),
    Rotation(Vec<[f32; 4]>),
    Scale(Vec<[f32; 3]>),
    Weights(Vec<f32>),
}

/// Canal de animación: un target + propiedad animada.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Channel {
    /// Índice del nodo target.
    pub node: usize,
    pub interpolation: Interpolation,
    pub times: KeyframeTimes,
    pub values: KeyframeValues,
}

/// Animación completa con múltiples canales.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Animation {
    pub name: String,
    pub channels: Vec<Channel>,
}
