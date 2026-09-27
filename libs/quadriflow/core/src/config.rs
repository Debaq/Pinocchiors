//! Configuración de la retopología.

/// Parámetros de [`remesh`](crate::remesh).
#[derive(Debug, Clone)]
pub struct RemeshConfig {
    /// Número aproximado de quads de la malla resultante.
    pub target_faces: usize,

    /// Alinear los quads a las aristas vivas (además de los bordes, que se
    /// respetan siempre). Sin efecto si la superficie se reconstruye.
    pub preserve_sharp: bool,

    /// Ángulo diedro (radianes) a partir del cual una arista es viva. También
    /// separa las esquinas de los bordes curvos.
    pub sharp_angle: f64,

    /// Iteraciones de suavizado de los campos en cada nivel de la jerarquía.
    pub smooth_iterations: usize,

    /// Cuándo reconstruir la superficie antes de retopologizar.
    pub rebuild: Rebuild,

    /// Cuánto se alinean los quads a las direcciones principales de
    /// curvatura (0 = nada). Donde la superficie es plana o esférica no hay
    /// dirección preferida y no influye.
    pub curvature_alignment: f64,
}

/// Reconstrucción volumétrica de la entrada: reemplaza la malla por la
/// superficie exterior de la unión de sus volúmenes. Arregla aristas
/// no-manifold, cáscaras superpuestas y caras interiores; a cambio redondea
/// las aristas vivas a la escala de un tercio de quad y cierra los agujeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rebuild {
    /// Solo si la malla tiene aristas no-manifold, orientación incoherente o
    /// cáscaras metidas unas en otras.
    #[default]
    Auto,
    /// Siempre.
    Always,
    /// Nunca: la entrada se usa tal cual.
    Never,
}

impl Default for RemeshConfig {
    fn default() -> Self {
        Self {
            target_faces: 1000,
            preserve_sharp: false,
            sharp_angle: std::f64::consts::FRAC_PI_4, // 45°
            smooth_iterations: 10,
            rebuild: Rebuild::Auto,
            curvature_alignment: 1.0,
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
