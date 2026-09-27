//! Configuración de la retopología.

/// Parámetros de [`remesh`](crate::remesh).
#[derive(Debug, Clone)]
pub struct RemeshConfig {
    /// Número aproximado de quads de la malla resultante.
    pub target_faces: usize,

    /// Alinear los quads a las aristas vivas (además de los bordes, que se
    /// respetan siempre).
    pub preserve_sharp: bool,

    /// Ángulo diedro (radianes) a partir del cual una arista es viva. También
    /// separa las esquinas de los bordes curvos.
    pub sharp_angle: f64,

    /// Iteraciones de suavizado de los campos en cada nivel de la jerarquía.
    pub smooth_iterations: usize,
}

impl Default for RemeshConfig {
    fn default() -> Self {
        Self {
            target_faces: 1000,
            preserve_sharp: false,
            sharp_angle: std::f64::consts::FRAC_PI_4, // 45°
            smooth_iterations: 10,
        }
    }
}

impl RemeshConfig {
    /// Menos iteraciones: más rápido, campos algo menos suaves.
    pub fn fast(target_faces: usize) -> Self {
        Self {
            target_faces,
            smooth_iterations: 5,
            ..Default::default()
        }
    }

    /// Más iteraciones y aristas vivas preservadas.
    pub fn quality(target_faces: usize) -> Self {
        Self {
            target_faces,
            preserve_sharp: true,
            smooth_iterations: 20,
            ..Default::default()
        }
    }
}
