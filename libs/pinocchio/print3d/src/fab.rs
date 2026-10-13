//! Tolerancias de fabricación derivadas de la impresora y el material
//!
//! Un solo lugar para las holguras y compensaciones: la preparación para imprimir las usa para
//! las uniones y la contracción, y el diseño (CAD) para engranes, agujeros y ajustes.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

use crate::profile::{MaterialProfile, PrinterProfile};

/// Qué tan justo debe ser un ajuste entre dos piezas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// A presión: casi sin juego (cuesta armarlo)
    Tight,
    /// Entra con la mano y no baila
    #[default]
    Normal,
    /// Gira o desliza libre
    Loose,
}

impl Fit {
    /// Multiplicador de la holgura de base
    pub fn factor(self) -> Real {
        match self {
            Fit::Tight => 0.5,
            Fit::Normal => 1.0,
            Fit::Loose => 1.6,
        }
    }
}

/// Holguras y compensaciones para una impresora y un material
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FabTolerances {
    /// Holgura de base en la medida total entre pieza y alojamiento (mm, positiva)
    pub clearance: Real,
    /// Cuánto más chico sale un agujero, por lado (mm)
    pub hole_comp: Real,
    /// Contracción lineal por eje (fracción)
    pub shrink: [Real; 3],
    /// Pared mínima imprimible (mm)
    pub min_wall: Real,
}

impl FabTolerances {
    pub fn derive(printer: &PrinterProfile, material: &MaterialProfile) -> Self {
        Self {
            clearance: printer.fit_clearance * material.clearance_factor,
            hole_comp: printer.hole_comp,
            shrink: material.shrink,
            min_wall: printer.min_wall(),
        }
    }

    /// Holgura total del ajuste (mm, positiva: cuánto más chico el eje que el agujero)
    pub fn fit_clearance(&self, fit: Fit) -> Real {
        self.clearance * fit.factor()
    }

    /// La misma holgura con el signo de las uniones (negativa: el macho más chico)
    pub fn joint_tolerance(&self, fit: Fit) -> Real {
        -self.fit_clearance(fit)
    }

    /// Cuánto hay que agrandar un agujero de diámetro `d` en el diseño para que salga de `d`:
    /// los agujeros salen más chicos de lo diseñado (`hole_comp` por lado)
    pub fn compensated_hole(&self, d: Real) -> Real {
        d + 2.0 * self.hole_comp
    }

    /// Factor por eje para que la pieza, ya encogida, quede en la medida diseñada
    pub fn shrink_scale(&self) -> [Real; 3] {
        self.shrink.map(|s| 1.0 / (1.0 - s.clamp(0.0, 0.5)))
    }
}

/// Agranda la malla por eje para compensar la contracción: la base queda donde estaba (z mínima) y
/// cada eje crece respecto del centro de su caja
pub fn compensate_shrink(mesh: &mut Mesh, tolerances: &FabTolerances) -> [Real; 3] {
    let scale = tolerances.shrink_scale();
    if mesh.vertices.is_empty() || scale.iter().all(|s| (*s - 1.0).abs() < 1e-12) {
        return scale;
    }
    let (mut lo, mut hi) = (mesh.vertices[0].position, mesh.vertices[0].position);
    for v in &mesh.vertices {
        lo = lo.min(&v.position);
        hi = hi.max(&v.position);
    }
    let center = Vector3::new((lo.x() + hi.x()) * 0.5, (lo.y() + hi.y()) * 0.5, lo.z());
    for v in &mut mesh.vertices {
        let d = v.position - center;
        v.position = center + Vector3::new(d.x() * scale[0], d.y() * scale[1], d.z() * scale[2]);
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::compute_bounding_box;
    use crate::printability::tests::make_box;

    #[test]
    fn la_holgura_sigue_al_material_y_al_ajuste() {
        let printer = PrinterProfile::default();
        let pla = FabTolerances::derive(&printer, &MaterialProfile::presets()[0]);
        let tpu = FabTolerances::derive(&printer, &MaterialProfile::presets().into_iter().find(|m| m.name == "TPU").unwrap());
        assert!((pla.fit_clearance(Fit::Normal) - 0.4).abs() < 1e-12);
        assert!(tpu.fit_clearance(Fit::Normal) > pla.fit_clearance(Fit::Normal));
        assert!(pla.fit_clearance(Fit::Tight) < pla.fit_clearance(Fit::Normal));
        assert!(pla.fit_clearance(Fit::Loose) > pla.fit_clearance(Fit::Normal));
        assert!((pla.joint_tolerance(Fit::Normal) + 0.4).abs() < 1e-12);
    }

    #[test]
    fn la_holgura_sigue_a_la_boquilla() {
        let fine = PrinterProfile::new("fina", [200.0; 3], 0.25, 0.1);
        let t = FabTolerances::derive(&fine, &MaterialProfile::default());
        assert!((t.clearance - 0.25).abs() < 1e-12 && (t.min_wall - 0.5).abs() < 1e-12);
    }

    #[test]
    fn el_agujero_se_agranda_para_salir_de_su_medida() {
        let t = FabTolerances::derive(&PrinterProfile::default(), &MaterialProfile::default());
        assert!((t.compensated_hole(5.0) - 5.2).abs() < 1e-12);
    }

    #[test]
    fn la_contraccion_agranda_para_quedar_nominal() {
        let abs = MaterialProfile::presets().into_iter().find(|m| m.name == "ABS").unwrap();
        let t = FabTolerances::derive(&PrinterProfile::default(), &abs);
        let mut m = make_box(100.0, 50.0, 20.0);
        let scale = compensate_shrink(&mut m, &t);
        assert!((scale[0] - 1.0 / 0.992).abs() < 1e-12);
        let bb = compute_bounding_box(&m);
        let d = bb.dimensions();
        // Al encogerse 0,8 % vuelve a 100 × 50 × 20
        assert!((d[0] * 0.992 - 100.0).abs() < 1e-9 && (d[1] * 0.992 - 50.0).abs() < 1e-9 && (d[2] * 0.992 - 20.0).abs() < 1e-9);
        // La base no se levanta
        assert!(bb.min[2].abs() < 1e-9);
    }

    #[test]
    fn sin_contraccion_no_cambia_nada() {
        let t = FabTolerances::derive(&PrinterProfile::default(), &MaterialProfile::new("x", 1.0, 45.0));
        let mut m = make_box(10.0, 10.0, 10.0);
        let before = compute_bounding_box(&m);
        compensate_shrink(&mut m, &t);
        let after = compute_bounding_box(&m);
        assert_eq!((before.min, before.max), (after.min, after.max));
    }
}
