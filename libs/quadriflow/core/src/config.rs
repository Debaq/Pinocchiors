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

    /// Achicar los quads donde la pieza es más delgada que un quad (paredes,
    /// tubos, puntas, ranuras, agujeros chicos), que de otro modo se tapan o
    /// se pierden. La cantidad total se conserva (el resto queda algo más
    /// grueso) y cada cambio de tamaño agrega vértices irregulares: por eso
    /// está desactivado por defecto.
    pub adaptive_density: bool,

    /// Simetría espejo: se retopologiza la mitad del lado positivo del plano y
    /// se refleja, así el resultado es exactamente simétrico (útil para
    /// personajes). Si la entrada no lo es, se simetriza esa mitad.
    pub symmetry: Symmetry,
}

/// Plano de simetría: perpendicular al eje, por el centro de la caja
/// envolvente de la malla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Symmetry {
    #[default]
    None,
    X,
    Y,
    Z,
}

impl Symmetry {
    /// Índice del eje (0, 1, 2), si hay simetría.
    pub fn axis(&self) -> Option<usize> {
        match self {
            Symmetry::None => None,
            Symmetry::X => Some(0),
            Symmetry::Y => Some(1),
            Symmetry::Z => Some(2),
        }
    }
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
            adaptive_density: false,
            symmetry: Symmetry::None,
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
