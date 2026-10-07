//! Configuración para análisis y reparación de mallas

use pinocchio_math::Real;
use serde::{Deserialize, Serialize};

/// Configuración del análisis.
///
/// Las tolerancias son relativas a la diagonal de la caja envolvente, así el
/// resultado no depende de las unidades del modelo (mm, m, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Distancia (relativa) bajo la cual dos vértices se consideran el mismo
    pub duplicate_tolerance: Real,
    /// Altura (relativa) bajo la cual un triángulo se considera degenerado
    pub degenerate_tolerance: Real,
    /// Ángulo mínimo en radianes: por debajo, el triángulo es una "aguja".
    /// Es un indicador de calidad, no un defecto.
    pub needle_angle_threshold: Real,
    /// Ángulo máximo en radianes: por encima, el triángulo es una "gorra".
    /// Es un indicador de calidad, no un defecto.
    pub cap_angle_threshold: Real,
    /// Si true, busca auto-intersecciones (puede ser lento en mallas grandes)
    pub check_self_intersections: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            duplicate_tolerance: 1e-6,
            degenerate_tolerance: 1e-7,
            needle_angle_threshold: 1.0_f64.to_radians(),
            cap_angle_threshold: 179.0_f64.to_radians(),
            check_self_intersections: false,
        }
    }
}

/// Configuración de la reparación completa ([`crate::repair_all`]).
///
/// Ningún paso borra geometría válida: las caras degeneradas se colapsan o se
/// absorben en sus vecinas, y la geometría non-manifold se separa en vez de
/// eliminarse.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepairConfig {
    /// Suelda vértices coincidentes
    pub merge_duplicates: bool,
    /// Distancia de soldadura, relativa a la diagonal de la caja envolvente
    pub merge_tolerance: Real,
    /// Corrige caras de área nula (colapso de aristas cortas, absorción de
    /// triángulos planos en sus vecinos)
    pub remove_degenerates: bool,
    /// Altura mínima de un triángulo, relativa a la diagonal
    pub degenerate_tolerance: Real,
    /// Elimina caras duplicadas (mismos vértices)
    pub remove_duplicate_faces: bool,
    /// Orienta las caras de forma consistente
    pub fix_normals: bool,
    /// Separa aristas y vértices non-manifold duplicando vértices
    pub fix_non_manifold: bool,
    /// Orienta las normales hacia afuera en cada cáscara cerrada (las cáscaras
    /// internas de un objeto hueco quedan hacia adentro)
    pub orient_outward: bool,
    /// Elimina piezas sueltas pequeñas
    pub remove_small_components: bool,
    /// Área mínima de una pieza, relativa a la pieza más grande
    pub small_component_ratio: Real,
    /// Rellena agujeros
    pub fill_holes: bool,
    /// Opciones del relleno de agujeros
    pub hole_fill_config: HoleFillConfig,
    /// Corta la malla por sus auto-intersecciones y elimina la superficie que
    /// queda dentro de otro cuerpo (une los cuerpos solapados). Desactivado
    /// por defecto: también borra las piezas internas puestas a propósito.
    pub resolve_intersections: bool,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            merge_duplicates: true,
            merge_tolerance: 1e-6,
            remove_degenerates: true,
            degenerate_tolerance: 1e-7,
            remove_duplicate_faces: true,
            fix_normals: true,
            fix_non_manifold: true,
            orient_outward: true,
            remove_small_components: false,
            small_component_ratio: 0.001,
            fill_holes: false,
            hole_fill_config: HoleFillConfig::default(),
            resolve_intersections: false,
        }
    }
}

/// Configuración del relleno de agujeros
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HoleFillConfig {
    /// Máximo de aristas de borde de un agujero a rellenar (0 = sin límite)
    pub max_hole_edges: usize,
    /// Refina el parche para igualar la densidad de la malla que lo rodea
    pub refine: bool,
    /// Ajusta la forma del parche para que continúe la curvatura del borde
    /// (requiere `refine` para tener vértices interiores que mover)
    pub fair: bool,
}

impl Default for HoleFillConfig {
    fn default() -> Self {
        Self { max_hole_edges: 0, refine: true, fair: true }
    }
}

/// Resultado del análisis de una malla
#[derive(Debug, Clone, Default, Serialize)]
pub struct MeshDiagnostics {
    /// Vértices
    pub num_vertices: usize,
    /// Triángulos
    pub num_faces: usize,
    /// Número de agujeros (ciclos de aristas de borde)
    pub boundary_loops: usize,
    /// Aristas de borde (con una sola cara)
    pub boundary_edges: usize,
    /// Vértices duplicados en costuras (se fusionarían al soldar)
    pub duplicate_vertices: usize,
    /// Vértices que ninguna cara usa
    pub unreferenced_vertices: usize,
    /// Caras con índices repetidos, fuera de rango o coordenadas no finitas
    pub invalid_faces: usize,
    /// Caras repetidas (mismos tres vértices)
    pub duplicate_faces: usize,
    /// Caras de área nula (altura bajo la tolerancia)
    pub degenerate_faces: usize,
    /// Triángulos con un ángulo muy agudo (calidad, no defecto)
    pub needle_faces: usize,
    /// Triángulos con un ángulo casi llano (calidad, no defecto)
    pub cap_faces: usize,
    /// Aristas con más de dos caras
    pub non_manifold_edges: usize,
    /// Vértices donde se tocan abanicos de caras separados
    pub non_manifold_vertices: usize,
    /// Aristas cuyas dos caras tienen orientaciones opuestas
    pub inconsistent_edges: usize,
    /// Si todas las caras vecinas tienen la misma orientación
    pub normals_consistent: bool,
    /// Si las normales apuntan hacia afuera (solo si la malla es cerrada y
    /// consistente)
    pub normals_outward: Option<bool>,
    /// Piezas conectadas
    pub connected_components: usize,
    /// Sin bordes ni aristas non-manifold
    pub is_closed: bool,
    /// Sin aristas ni vértices non-manifold
    pub is_manifold: bool,
    /// Pares de triángulos que se cruzan (si se analizó)
    pub self_intersections: usize,
    /// Área de la superficie
    pub area: Real,
    /// Volumen encerrado (solo significativo si es cerrada)
    pub volume: Real,
}

impl MeshDiagnostics {
    /// Si hay defectos que [`crate::repair_all`] puede corregir
    pub fn needs_repair(&self) -> bool {
        self.duplicate_vertices > 0
            || self.unreferenced_vertices > 0
            || self.invalid_faces > 0
            || self.duplicate_faces > 0
            || self.degenerate_faces > 0
            || self.non_manifold_edges > 0
            || self.non_manifold_vertices > 0
            || !self.normals_consistent
            || self.normals_outward == Some(false)
            || self.boundary_loops > 0
            || self.self_intersections > 0
    }

    /// Cerrada, manifold, bien orientada y sin defectos
    pub fn is_healthy(&self) -> bool {
        self.is_closed
            && self.is_manifold
            && self.normals_consistent
            && self.normals_outward != Some(false)
            && self.duplicate_vertices == 0
            && self.invalid_faces == 0
            && self.duplicate_faces == 0
            && self.degenerate_faces == 0
            && self.self_intersections == 0
    }
}

/// Resumen de las reparaciones realizadas
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepairSummary {
    /// Caras inválidas eliminadas
    pub invalid_faces_removed: usize,
    /// Vértices fusionados
    pub vertices_merged: usize,
    /// Caras degeneradas corregidas
    pub degenerate_fixed: usize,
    /// Caras duplicadas eliminadas
    pub duplicate_faces_removed: usize,
    /// Caras eliminadas en total
    pub faces_removed: usize,
    /// Caras reorientadas
    pub faces_flipped: usize,
    /// Vértices duplicados para separar geometría non-manifold
    pub non_manifold_fixed: usize,
    /// Piezas sueltas eliminadas
    pub components_removed: usize,
    /// Agujeros rellenados
    pub holes_filled: usize,
    /// Agujeros que no se rellenaron (tamaño máximo o borde inválido)
    pub holes_skipped: usize,
    /// Caras añadidas por el relleno
    pub faces_added: usize,
    /// Vértices añadidos por el relleno
    pub vertices_added: usize,
    /// Pares de triángulos cortados por su curva de cruce
    pub intersections_cut: usize,
    /// Pares que se cruzan de forma degenerada y quedaron sin cortar
    pub intersections_skipped: usize,
    /// Parches de superficie eliminados por quedar dentro de otro cuerpo
    pub inner_patches_removed: usize,
    /// La unión se descartó porque dejaba la malla con más bordes o aristas
    /// non-manifold (superficies abiertas)
    pub intersections_reverted: bool,
}

impl RepairSummary {
    /// Si se realizó alguna reparación
    pub fn any_repairs(&self) -> bool {
        self.invalid_faces_removed > 0
            || self.vertices_merged > 0
            || self.degenerate_fixed > 0
            || self.duplicate_faces_removed > 0
            || self.faces_removed > 0
            || self.faces_flipped > 0
            || self.non_manifold_fixed > 0
            || self.components_removed > 0
            || self.holes_filled > 0
            || self.intersections_cut > 0
            || self.inner_patches_removed > 0
    }
}
