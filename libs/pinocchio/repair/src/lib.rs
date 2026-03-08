//! Pinocchio Repair - Análisis y reparación de mallas 3D
//!
//! Este crate proporciona herramientas para:
//! - Detectar y cerrar agujeros en mallas
//! - Orientar normales consistentemente
//! - Fusionar vértices duplicados
//! - Eliminar caras degeneradas
//! - Detectar y reparar geometría non-manifold
//! - Detectar auto-intersecciones
//!
//! # Ejemplo
//!
//! ```ignore
//! use pinocchio_mesh::Mesh;
//! use pinocchio_repair::{analyze, repair_all, RepairConfig, AnalysisConfig};
//!
//! // Analizar la malla
//! let diagnostics = analyze(&mesh, &AnalysisConfig::default());
//! println!("Agujeros: {}", diagnostics.boundary_loops);
//!
//! // Reparar la malla
//! let summary = repair_all(&mut mesh, &RepairConfig::default())?;
//! println!("Vértices fusionados: {}", summary.vertices_merged);
//! ```

mod error;
mod config;
pub mod analysis;
pub mod repair;
pub mod triangulation;

pub use error::{RepairError, RepairResult};
pub use config::{
    AnalysisConfig,
    RepairConfig,
    DegenerateConfig,
    HoleFillConfig,
    HoleFillMethod,
    MeshDiagnostics,
    RepairSummary,
};

use pinocchio_mesh::Mesh;

// Re-exportar tipos de análisis
pub use analysis::boundary::BoundaryLoop;
pub use analysis::degenerate::DegenerateFace;
pub use analysis::duplicates::DuplicateGroup;
pub use analysis::normals::NormalAnalysis;
pub use analysis::manifold::{NonManifoldEdge, NonManifoldVertex};
pub use analysis::intersections::{Intersection, IntersectionAnalysis};

/// Analiza una malla y devuelve diagnósticos
///
/// Esta función examina la malla para detectar problemas como:
/// - Agujeros (boundary loops)
/// - Vértices duplicados
/// - Caras degeneradas
/// - Geometría non-manifold
/// - Normales inconsistentes
///
/// # Argumentos
///
/// * `mesh` - La malla a analizar
/// * `config` - Configuración del análisis
///
/// # Retorna
///
/// Diagnósticos detallados de la malla
pub fn analyze(mesh: &Mesh, config: &AnalysisConfig) -> MeshDiagnostics {
    let mut diagnostics = MeshDiagnostics::default();

    if mesh.num_faces() == 0 {
        return diagnostics;
    }

    // Boundary loops
    let loops = analysis::boundary::find_boundary_loops(mesh);
    diagnostics.boundary_loops = loops.len();
    diagnostics.boundary_edges = loops.iter().map(|l| l.edges.len()).sum();
    diagnostics.is_closed = loops.is_empty();

    // Vértices duplicados
    let duplicates = analysis::duplicates::find_duplicate_vertices(mesh, config.duplicate_tolerance);
    diagnostics.duplicate_vertices = duplicates.iter().map(|g| g.indices.len() - 1).sum();

    // Caras degeneradas
    let degenerates = analysis::degenerate::find_degenerate_faces(
        mesh,
        config.degenerate_area_threshold,
        config.needle_angle_threshold,
        config.cap_angle_threshold,
    );
    diagnostics.degenerate_faces = degenerates.len();
    for d in &degenerates {
        match d {
            DegenerateFace::ZeroArea(_) => diagnostics.zero_area_faces += 1,
            DegenerateFace::Needle(_) => diagnostics.needle_faces += 1,
            DegenerateFace::Cap(_) => diagnostics.cap_faces += 1,
        }
    }

    // Non-manifold
    if config.check_non_manifold {
        let nm_edges = analysis::manifold::find_non_manifold_edges(mesh);
        let nm_vertices = analysis::manifold::find_non_manifold_vertices(mesh);
        diagnostics.non_manifold_edges = nm_edges.len();
        diagnostics.non_manifold_vertices = nm_vertices.len();
    }

    // Normales
    let normal_analysis = analysis::normals::analyze_normal_orientation(mesh);
    diagnostics.normals_consistent = normal_analysis.is_consistent;
    diagnostics.connected_components = normal_analysis.components;

    // Auto-intersecciones
    if config.check_self_intersections {
        let intersections = analysis::intersections::find_self_intersections(
            mesh,
            config.intersection_tolerance,
        );
        diagnostics.self_intersections = intersections.count();
    }

    diagnostics
}

/// Repara una malla aplicando todas las correcciones configuradas
///
/// El orden de las operaciones es:
/// 1. Fusionar vértices duplicados
/// 2. Eliminar caras degeneradas
/// 3. Hacer normales consistentes
/// 4. Orientar normales hacia afuera
/// 5. Rellenar agujeros (si está habilitado)
/// 6. Reparar non-manifold (si está habilitado)
///
/// # Argumentos
///
/// * `mesh` - La malla a reparar (se modifica in-place)
/// * `config` - Configuración de la reparación
///
/// # Retorna
///
/// Resumen de las reparaciones realizadas o error
pub fn repair_all(mesh: &mut Mesh, config: &RepairConfig) -> RepairResult<RepairSummary> {
    let mut summary = RepairSummary::default();

    if mesh.num_faces() == 0 {
        return Err(RepairError::EmptyMesh);
    }

    // 1. Fusionar duplicados
    if config.merge_duplicates {
        summary.vertices_merged = repair::merge::merge_duplicate_vertices(mesh, config.merge_tolerance);
    }

    // 2. Eliminar degeneradas
    if config.remove_degenerates {
        summary.faces_removed = repair::cleanup::remove_degenerate_faces(mesh, &config.degenerate_config);
    }

    // 3. Normales consistentes
    if config.fix_normals {
        summary.faces_flipped = repair::normals::make_normals_consistent(mesh);
    }

    // 4. Orientar hacia afuera
    if config.orient_outward {
        if let Ok(flipped) = repair::normals::orient_normals_outward(mesh) {
            summary.faces_flipped += flipped;
        }
    }

    // 5. Rellenar agujeros
    if config.fill_holes {
        let (filled, added) = repair::holes::fill_all_holes(mesh, &config.hole_fill_config)?;
        summary.holes_filled = filled;
        summary.faces_added = added;
    }

    // 6. Non-manifold
    if config.fix_non_manifold {
        summary.non_manifold_fixed = repair::manifold::repair_non_manifold_edges(mesh);
    }

    Ok(summary)
}

// Re-exportar funciones individuales para uso directo
pub use analysis::boundary::find_boundary_loops;
pub use analysis::degenerate::find_degenerate_faces;
pub use analysis::duplicates::find_duplicate_vertices;
pub use analysis::normals::analyze_normal_orientation;
pub use analysis::manifold::{find_non_manifold_edges, find_non_manifold_vertices};
pub use analysis::intersections::find_self_intersections;

pub use repair::merge::merge_duplicate_vertices;
pub use repair::cleanup::remove_degenerate_faces;
pub use repair::normals::{make_normals_consistent, orient_normals_outward};
pub use repair::holes::{fill_hole, fill_all_holes};

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    fn make_open_box() -> Mesh {
        // Cubo sin tapa (5 caras)
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),  // 0
            Vector3::new(1.0, 0.0, 0.0),  // 1
            Vector3::new(1.0, 1.0, 0.0),  // 2
            Vector3::new(0.0, 1.0, 0.0),  // 3
            Vector3::new(0.0, 0.0, 1.0),  // 4
            Vector3::new(1.0, 0.0, 1.0),  // 5
            Vector3::new(1.0, 1.0, 1.0),  // 6
            Vector3::new(0.0, 1.0, 1.0),  // 7
        ];
        let faces = vec![
            // Base
            [0, 2, 1], [0, 3, 2],
            // Frente
            [0, 1, 5], [0, 5, 4],
            // Derecha
            [1, 2, 6], [1, 6, 5],
            // Atrás
            [2, 3, 7], [2, 7, 6],
            // Izquierda
            [3, 0, 4], [3, 4, 7],
            // Tapa omitida para crear agujero
        ];
        Mesh::from_triangles(&vertices, &faces)
    }

    #[test]
    fn test_analyze_open_mesh() {
        let mesh = make_open_box();
        let diagnostics = analyze(&mesh, &AnalysisConfig::default());

        assert!(!diagnostics.is_closed);
        assert_eq!(diagnostics.boundary_loops, 1);
        assert_eq!(diagnostics.boundary_edges, 4);
    }

    #[test]
    fn test_analyze_closed_mesh() {
        let vertices = vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, 1.0, 0.0),
            Vector3::new(0.5, 0.5, 1.0),
        ];
        let faces = vec![
            [0, 1, 2],
            [0, 3, 1],
            [1, 3, 2],
            [2, 3, 0],
        ];
        let mesh = Mesh::from_triangles(&vertices, &faces);
        let diagnostics = analyze(&mesh, &AnalysisConfig::default());

        assert!(diagnostics.is_closed);
        assert_eq!(diagnostics.boundary_loops, 0);
    }
}
