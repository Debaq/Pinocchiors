//! Configuraciones para preparación de impresión 3D

use pinocchio_math::Real;
use serde::{Deserialize, Serialize};

/// Configuración para subdivisión de modelos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubdivideConfig {
    /// Volumen de construcción de la impresora (mm) [x, y, z]
    pub build_volume: [Real; 3],

    /// Dimensión máxima permitida por pieza (mm)
    /// Si es None, usa build_volume
    pub max_dimension: Option<Real>,

    /// Solapamiento entre piezas para joints (mm)
    pub overlap: Real,

    /// Estrategia de subdivisión
    pub strategy: SubdivideStrategy,

    /// Margen de seguridad respecto al volumen de construcción (mm)
    pub margin: Real,
}

impl Default for SubdivideConfig {
    fn default() -> Self {
        Self {
            build_volume: [220.0, 220.0, 250.0], // Ender 3 típico
            max_dimension: None,
            overlap: 0.0,
            strategy: SubdivideStrategy::Grid,
            margin: 5.0,
        }
    }
}

/// Estrategia para subdividir el modelo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubdivideStrategy {
    /// Cortes regulares en grid X, Y, Z
    #[default]
    Grid,

    /// Minimiza número de cortes respetando volumen
    Optimal,

    /// Solo corta en eje Z (capas)
    ZLayers,
}

/// Configuración para análisis de malla
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Verificar si la malla está cerrada
    pub check_closed: bool,

    /// Calcular centro de masa
    pub compute_center_of_mass: bool,

    /// Densidad del material para estimación de peso (g/cm³)
    /// PLA típico: 1.24, ABS: 1.04, PETG: 1.27
    pub material_density: Option<Real>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            check_closed: true,
            compute_center_of_mass: true,
            material_density: Some(1.24), // PLA
        }
    }
}

/// Tolerancias para impresión FDM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FdmTolerances {
    /// Tolerancia general (mm)
    pub general: Real,

    /// Tolerancia para joints - diferencia macho/hembra (mm)
    /// Valor negativo = macho más pequeño que hembra
    pub joint_clearance: Real,

    /// Altura de capa (mm)
    pub layer_height: Real,

    /// Ancho de línea (mm)
    pub line_width: Real,
}

impl Default for FdmTolerances {
    fn default() -> Self {
        Self {
            general: 0.4,
            joint_clearance: -0.4,
            layer_height: 0.2,
            line_width: 0.4,
        }
    }
}

impl FdmTolerances {
    /// Tolerancias para impresora de escritorio estándar
    pub fn desktop() -> Self {
        Self::default()
    }

    /// Tolerancias para impresora industrial (más precisas)
    pub fn industrial() -> Self {
        Self {
            general: 0.2,
            joint_clearance: -0.2,
            layer_height: 0.1,
            line_width: 0.35,
        }
    }

    /// Tolerancias para impresión de alta calidad (lenta)
    pub fn high_quality() -> Self {
        Self {
            general: 0.3,
            joint_clearance: -0.3,
            layer_height: 0.1,
            line_width: 0.4,
        }
    }
}

/// Configuración completa para preparación de impresión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintPrepConfig {
    /// Volumen de construcción de la impresora [x, y, z]
    pub build_volume: [Real; 3],

    /// Factor de escala objetivo (None = no escalar)
    pub target_scale: Option<Real>,

    /// Subdividir si excede volumen
    pub subdivide: bool,

    /// Configuración de subdivisión
    pub subdivide_config: SubdivideConfig,

    /// Añadir joints entre piezas
    pub add_joints: bool,

    /// Esquema de etiquetado
    pub labeling: LabelingScheme,

    /// Tolerancias FDM
    pub tolerances: FdmTolerances,
}

impl Default for PrintPrepConfig {
    fn default() -> Self {
        Self {
            build_volume: [220.0, 220.0, 250.0],
            target_scale: None,
            subdivide: true,
            subdivide_config: SubdivideConfig::default(),
            add_joints: false,
            labeling: LabelingScheme::Numeric,
            tolerances: FdmTolerances::desktop(),
        }
    }
}

/// Esquema de etiquetado para piezas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LabelingScheme {
    /// Numérico simple: 1, 2, 3...
    #[default]
    Numeric,

    /// Alfanumérico: 1A, 1B, 2A, 2B...
    Alphanumeric,

    /// Por coordenadas: X0Y0Z0, X1Y0Z0...
    Coordinate,
}
