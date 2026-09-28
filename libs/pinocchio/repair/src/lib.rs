//! Pinocchio Repair - Análisis y reparación de mallas 3D
//!
//! Detecta y corrige los defectos típicos de mallas exportadas o escaneadas:
//! vértices duplicados, caras inválidas, degeneradas o repetidas, geometría
//! non-manifold, normales inconsistentes o invertidas, piezas sueltas y
//! agujeros.
//!
//! La reparación es conservadora: ningún paso abre agujeros ni borra
//! geometría válida. Las caras degeneradas se colapsan o se absorben en sus
//! vecinas, y la geometría non-manifold se separa duplicando vértices.
//!
//! Todo trabaja sobre [`TriMesh`] (posiciones + índices), que admite mallas
//! arbitrariamente rotas; [`analyze`] y [`repair_all`] convierten desde y
//! hacia la [`Mesh`] half-edge.
//!
//! # Ejemplo
//!
//! ```no_run
//! use pinocchio_repair::{analyze, repair_all, AnalysisConfig, RepairConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut mesh = pinocchio_mesh::load_obj("modelo.obj")?;
//!
//! let diagnostics = analyze(&mesh, &AnalysisConfig::default());
//! println!("Agujeros: {}", diagnostics.boundary_loops);
//!
//! let config = RepairConfig { fill_holes: true, ..Default::default() };
//! let summary = repair_all(&mut mesh, &config)?;
//! println!("Agujeros rellenados: {}", summary.holes_filled);
//! # Ok(())
//! # }
//! ```

mod config;
mod error;
mod sparse;
mod trimesh;
pub mod analysis;
pub mod repair;
pub mod topology;

pub use analysis::analyze_trimesh;
pub use analysis::degenerate::FaceQuality;
pub use analysis::intersections::{find_self_intersections, Intersection, IntersectionAnalysis};
pub use config::{AnalysisConfig, HoleFillConfig, MeshDiagnostics, RepairConfig, RepairSummary};
pub use error::{RepairError, RepairResult};
pub use trimesh::TriMesh;

use pinocchio_mesh::Mesh;
use repair::{cleanup, holes, manifold, orient};

/// Analiza una malla. Ver [`analyze_trimesh`].
pub fn analyze(mesh: &Mesh, config: &AnalysisConfig) -> MeshDiagnostics {
    analyze_trimesh(&TriMesh::from_mesh(mesh), config)
}

/// Repara una malla en su lugar. Ver [`repair_trimesh`].
pub fn repair_all(mesh: &mut Mesh, config: &RepairConfig) -> RepairResult<RepairSummary> {
    repair_all_with_progress(mesh, config, |_, _| {})
}

/// Como [`repair_all`], informando el avance: `progress(fracción 0..=1, etapa)`.
pub fn repair_all_with_progress(
    mesh: &mut Mesh,
    config: &RepairConfig,
    mut progress: impl FnMut(f32, &str),
) -> RepairResult<RepairSummary> {
    progress(0.0, "Preparando malla");
    let mut trimesh = TriMesh::from_mesh(mesh);
    let summary = repair_trimesh_with_progress(&mut trimesh, config, |f, stage| progress(0.02 + 0.9 * f, stage))?;
    progress(0.93, "Reconstruyendo malla");
    *mesh = trimesh.to_mesh();
    progress(1.0, "Listo");
    Ok(summary)
}

/// Repara una malla indexada aplicando los pasos habilitados en `config`.
///
/// Orden:
/// 1. Eliminar caras inválidas (índices fuera de rango o repetidos, NaN)
/// 2. Soldar vértices coincidentes
/// 3. Corregir caras degeneradas (colapso de aristas / absorción de gorras)
/// 4. Eliminar caras repetidas
/// 5. Orientar de forma consistente y separar geometría non-manifold
/// 6. Eliminar piezas sueltas pequeñas
/// 7. Rellenar agujeros
/// 8. Orientar cada cáscara cerrada hacia afuera
/// 9. Eliminar vértices sin usar
///
/// # Errores
///
/// [`RepairError::EmptyMesh`] si la malla no tiene caras.
pub fn repair_trimesh(mesh: &mut TriMesh, config: &RepairConfig) -> RepairResult<RepairSummary> {
    repair_trimesh_with_progress(mesh, config, |_, _| {})
}

/// Como [`repair_trimesh`], informando el avance: `progress(fracción 0..=1, etapa)`.
pub fn repair_trimesh_with_progress(
    mesh: &mut TriMesh,
    config: &RepairConfig,
    mut progress: impl FnMut(f32, &str),
) -> RepairResult<RepairSummary> {
    if mesh.num_faces() == 0 {
        return Err(RepairError::EmptyMesh);
    }
    let mut summary = RepairSummary::default();
    let diagonal = mesh.diagonal();

    progress(0.0, "Eliminando caras inválidas");
    summary.invalid_faces_removed = cleanup::remove_invalid_faces(mesh);
    summary.faces_removed += summary.invalid_faces_removed;

    if config.merge_duplicates {
        progress(0.03, "Soldando costuras");
        let (merged, collapsed) = cleanup::weld_vertices(mesh, config.merge_tolerance * diagonal);
        summary.vertices_merged = merged;
        summary.degenerate_fixed += collapsed;
        summary.faces_removed += collapsed;
    }

    if config.remove_degenerates {
        progress(0.15, "Corrigiendo caras degeneradas");
        let before = mesh.num_faces();
        summary.degenerate_fixed += cleanup::fix_degenerate_faces(mesh, config.degenerate_tolerance * diagonal);
        summary.faces_removed += before.saturating_sub(mesh.num_faces());
    }

    if config.remove_duplicate_faces {
        progress(0.25, "Eliminando caras repetidas");
        summary.duplicate_faces_removed = cleanup::remove_duplicate_faces(mesh);
        summary.faces_removed += summary.duplicate_faces_removed;
    }

    if config.fix_normals || config.fix_non_manifold {
        progress(0.30, "Orientando y separando geometría non-manifold");
        let report = manifold::orient_and_split(mesh, config.fix_normals, config.fix_non_manifold);
        summary.faces_flipped += report.faces_flipped;
        summary.non_manifold_fixed = report.vertices_split;
        if config.fix_non_manifold && report.vertices_split > 0 {
            for _ in 0..4 {
                if manifold::zip_slits(mesh) == 0 {
                    break;
                }
                let report = manifold::orient_and_split(mesh, config.fix_normals, true);
                summary.faces_flipped += report.faces_flipped;
            }
        }
    }

    if config.remove_small_components {
        progress(0.50, "Eliminando piezas sueltas");
        let (components, faces) = cleanup::remove_small_components(mesh, config.small_component_ratio);
        summary.components_removed = components;
        summary.faces_removed += faces;
    }

    if config.fill_holes {
        progress(0.55, "Rellenando agujeros");
        let report = holes::fill_holes_with_progress(mesh, &config.hole_fill_config, |done, total| {
            progress(0.55 + 0.35 * done as f32 / total.max(1) as f32, &format!("Rellenando agujeros ({done}/{total})"));
        });
        summary.holes_filled = report.filled;
        summary.holes_skipped = report.skipped;
        summary.faces_added = report.faces_added;
        summary.vertices_added = report.vertices_added;
    }

    if config.orient_outward {
        progress(0.90, "Orientando normales hacia afuera");
        summary.faces_flipped += orient::orient_outward(mesh);
    }

    mesh.remove_unreferenced_vertices();
    progress(1.0, "Listo");
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_math::Vector3;

    /// Cubo en "sopa" (cada triángulo con sus vértices, como un STL), con una
    /// cara volteada, una duplicada, una degenerada y la tapa abierta.
    fn broken_cube() -> TriMesh {
        let p = [
            [0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.],
        ];
        let mut indexed = vec![
            [0, 2, 1], [0, 3, 2], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
            [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
        ];
        indexed[4].swap(1, 2);
        indexed.push([0, 2, 1]);
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for t in &indexed {
            let base = positions.len();
            positions.extend(t.iter().map(|&v| Vector3::new(p[v][0], p[v][1], p[v][2])));
            triangles.push([base, base + 1, base + 2]);
        }
        // Degenerada: tres puntos colineales sobre una arista del fondo
        let base = positions.len();
        positions.extend([Vector3::zero(), Vector3::new(0.5, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0)]);
        triangles.push([base, base + 1, base + 2]);
        TriMesh::new(positions, triangles)
    }

    #[test]
    fn repairs_broken_cube() {
        let mut m = broken_cube();
        let before = analyze_trimesh(&m, &AnalysisConfig::default());
        assert!(before.needs_repair());

        let config = RepairConfig { fill_holes: true, ..Default::default() };
        let summary = repair_trimesh(&mut m, &config).unwrap();
        assert!(summary.any_repairs());
        assert_eq!(summary.holes_filled, 1);

        let after = analyze_trimesh(&m, &AnalysisConfig { check_self_intersections: true, ..Default::default() });
        assert!(after.is_healthy(), "{after:#?}\n{summary:#?}");
        assert!((after.volume - 1.0).abs() < 1e-9);
        assert_eq!(m.num_vertices(), 8);
    }

    #[test]
    fn healthy_mesh_is_untouched() {
        let mut m = broken_cube();
        repair_trimesh(&mut m, &RepairConfig { fill_holes: true, ..Default::default() }).unwrap();
        let reference = m.clone();
        let summary = repair_trimesh(&mut m, &RepairConfig { fill_holes: true, ..Default::default() }).unwrap();
        assert!(!summary.any_repairs(), "{summary:#?}");
        assert_eq!(m.triangles, reference.triangles);
    }

    #[test]
    fn mesh_roundtrip() {
        let tm = broken_cube();
        let mut mesh = tm.to_mesh();
        let summary = repair_all(&mut mesh, &RepairConfig { fill_holes: true, ..Default::default() }).unwrap();
        assert_eq!(summary.holes_filled, 1);
        assert!(mesh.integrity_check().is_ok());
        assert!(mesh.is_closed());
        assert!(analyze(&mesh, &AnalysisConfig::default()).is_healthy());
    }

    #[test]
    fn empty_mesh_is_an_error() {
        assert!(matches!(repair_trimesh(&mut TriMesh::default(), &RepairConfig::default()), Err(RepairError::EmptyMesh)));
    }
}
