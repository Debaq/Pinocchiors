//! Configuración para análisis y reparación de mallas

use pinocchio_math::Real;
use serde::{Deserialize, Serialize};

/// Configuración para el análisis de mallas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Tolerancia para detectar vértices duplicados
    pub duplicate_tolerance: Real,
    /// Umbral de área para considerar una cara degenerada
    pub degenerate_area_threshold: Real,
    /// Umbral de ángulo mínimo (en radianes) para detectar "needles"
    pub needle_angle_threshold: Real,
    /// Umbral de ángulo máximo (en radianes) para detectar "caps"
    pub cap_angle_threshold: Real,
    /// Si true, analiza non-manifold geometry
    pub check_non_manifold: bool,
    /// Si true, detecta auto-intersecciones (puede ser lento en mallas grandes)
    pub check_self_intersections: bool,
    /// Tolerancia para detección de intersecciones
    pub intersection_tolerance: Real,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            duplicate_tolerance: 1e-6,
            degenerate_area_threshold: 1e-10,
            needle_angle_threshold: 0.01,                              // ~0.57 grados
            cap_angle_threshold: std::f64::consts::PI - 0.01,          // ~179.43 grados
            check_non_manifold: true,
            check_self_intersections: false,  // Desactivado por defecto (puede ser lento)
            intersection_tolerance: 1e-8,
        }
    }
}

/// Configuración para la reparación completa
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepairConfig {
    /// Si true, fusiona vértices duplicados
    pub merge_duplicates: bool,
    /// Tolerancia para fusión de duplicados
    pub merge_tolerance: Real,
    /// Si true, elimina caras degeneradas
    pub remove_degenerates: bool,
    /// Configuración para detección de degeneradas
    pub degenerate_config: DegenerateConfig,
    /// Si true, hace las normales consistentes
    pub fix_normals: bool,
    /// Si true, orienta las normales hacia afuera
    pub orient_outward: bool,
    /// Si true, rellena agujeros
    pub fill_holes: bool,
    /// Configuración para relleno de agujeros
    pub hole_fill_config: HoleFillConfig,
    /// Si true, repara geometría non-manifold
    pub fix_non_manifold: bool,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            merge_duplicates: true,
            merge_tolerance: 1e-6,
            remove_degenerates: true,
            degenerate_config: DegenerateConfig::default(),
            fix_normals: true,
            orient_outward: true,
            fill_holes: false,  // Desactivado por defecto (puede ser destructivo)
            hole_fill_config: HoleFillConfig::default(),
            fix_non_manifold: false,  // Desactivado por defecto (complejo)
        }
    }
}

/// Configuración para detección/eliminación de caras degeneradas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DegenerateConfig {
    /// Umbral de área mínima
    pub min_area: Real,
    /// Eliminar triángulos con ángulos muy pequeños ("needles")
    pub remove_needles: bool,
    /// Ángulo mínimo en radianes para needles
    pub needle_angle: Real,
    /// Eliminar triángulos con ángulos muy grandes ("caps")
    pub remove_caps: bool,
    /// Ángulo máximo en radianes para caps
    pub cap_angle: Real,
}

impl Default for DegenerateConfig {
    fn default() -> Self {
        Self {
            min_area: 1e-10,
            remove_needles: true,
            needle_angle: 0.01,
            remove_caps: true,
            cap_angle: std::f64::consts::PI - 0.01,
        }
    }
}

/// Configuración para relleno de agujeros
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HoleFillConfig {
    /// Método de triangulación
    pub method: HoleFillMethod,
    /// Tamaño máximo de agujero a rellenar (en número de aristas)
    pub max_hole_size: usize,
    /// Si true, refina el relleno para mejor calidad
    pub refine: bool,
    /// Número de iteraciones de suavizado
    pub smooth_iterations: usize,
}

impl Default for HoleFillConfig {
    fn default() -> Self {
        Self {
            method: HoleFillMethod::EarClipping,
            max_hole_size: 100,
            refine: false,
            smooth_iterations: 0,
        }
    }
}

/// Método de triangulación para relleno de agujeros
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoleFillMethod {
    /// Ear clipping simple - O(n²), bueno para agujeros pequeños
    EarClipping,
    /// Método de Liepa - alta calidad, con refinamiento
    Liepa,
}

/// Resultado del análisis de una malla
#[derive(Debug, Clone, Default, Serialize)]
pub struct MeshDiagnostics {
    /// Número de boundary loops (agujeros)
    pub boundary_loops: usize,
    /// Total de aristas de borde
    pub boundary_edges: usize,
    /// Número de vértices duplicados encontrados
    pub duplicate_vertices: usize,
    /// Número de caras degeneradas
    pub degenerate_faces: usize,
    /// Número de caras con área cero
    pub zero_area_faces: usize,
    /// Número de "needles" (triángulos muy alargados)
    pub needle_faces: usize,
    /// Número de "caps" (triángulos con ángulo muy grande)
    pub cap_faces: usize,
    /// Número de aristas non-manifold
    pub non_manifold_edges: usize,
    /// Número de vértices non-manifold
    pub non_manifold_vertices: usize,
    /// Si las normales son consistentes
    pub normals_consistent: bool,
    /// Número de componentes conectados
    pub connected_components: usize,
    /// Si la malla es cerrada (watertight)
    pub is_closed: bool,
    /// Número de pares de triángulos que se auto-intersectan
    pub self_intersections: usize,
}

impl MeshDiagnostics {
    /// Verifica si la malla necesita reparación
    pub fn needs_repair(&self) -> bool {
        self.duplicate_vertices > 0
            || self.degenerate_faces > 0
            || self.non_manifold_edges > 0
            || self.non_manifold_vertices > 0
            || !self.normals_consistent
            || self.self_intersections > 0
    }

    /// Verifica si la malla está en buen estado
    pub fn is_healthy(&self) -> bool {
        self.is_closed
            && self.normals_consistent
            && self.degenerate_faces == 0
            && self.duplicate_vertices == 0
            && self.non_manifold_edges == 0
            && self.non_manifold_vertices == 0
            && self.self_intersections == 0
    }
}

/// Resumen de las reparaciones realizadas
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepairSummary {
    /// Vértices fusionados
    pub vertices_merged: usize,
    /// Caras eliminadas
    pub faces_removed: usize,
    /// Caras reorientadas
    pub faces_flipped: usize,
    /// Agujeros rellenados
    pub holes_filled: usize,
    /// Caras añadidas (por relleno de agujeros)
    pub faces_added: usize,
    /// Aristas non-manifold reparadas
    pub non_manifold_fixed: usize,
}

impl RepairSummary {
    /// Verifica si se realizó alguna reparación
    pub fn any_repairs(&self) -> bool {
        self.vertices_merged > 0
            || self.faces_removed > 0
            || self.faces_flipped > 0
            || self.holes_filled > 0
            || self.non_manifold_fixed > 0
    }
}
