//! Vaciado (cáscara) y estimación de material
//!
//! El exterior se conserva tal cual; el hueco sale de la superficie desplazada
//! hacia adentro el espesor de pared, reconstruida por vóxeles, e invertida.
//! Las partes más finas que el doble de la pared quedan macizas.

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use quadriflow_core::remesh::{voxel, VoxelOptions};
use serde::{Deserialize, Serialize};

use crate::analysis::compute_volume;
use crate::error::{Print3dError, Result};
use crate::geom::triangles;
use crate::profile::MaterialProfile;

/// Parámetros del vaciado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HollowConfig {
    /// Espesor de pared (mm)
    pub wall: Real,
    /// Lado del vóxel (mm); `None`: la pared entre 2,5 (con un mínimo para limitar la grilla)
    pub voxel_size: Option<Real>,
}

impl Default for HollowConfig {
    fn default() -> Self {
        Self { wall: 2.0, voxel_size: None }
    }
}

/// Resultado del vaciado
#[derive(Debug, Clone)]
pub struct HollowResult {
    pub mesh: Mesh,
    /// Volumen del hueco (mm³)
    pub cavity_volume: Real,
    /// Volumen del sólido original (mm³)
    pub original_volume: Real,
    /// Fracción de material ahorrada (0..1)
    pub saved_fraction: Real,
}

/// Vacía una malla cerrada dejando paredes de `config.wall`
pub fn hollow(mesh: &Mesh, config: &HollowConfig) -> Result<HollowResult> {
    if !(config.wall.is_finite() && config.wall > 0.0) {
        return Err(Print3dError::InvalidConfig("el espesor de pared debe ser positivo".into()));
    }
    if mesh.vertices.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    if !mesh.is_closed() {
        return Err(Print3dError::MeshNotClosed);
    }
    let (pos, tris) = triangles(mesh);
    let original_volume = compute_volume(mesh);
    let positions: Vec<[f32; 3]> = pos.iter().map(|p| [p.x() as f32, p.y() as f32, p.z() as f32]).collect();
    let indices: Vec<u32> = tris.iter().flat_map(|t| t.map(|i| i as u32)).collect();

    let voxel_size = config.voxel_size.unwrap_or(config.wall / 2.5).max(1e-6);
    let options = VoxelOptions { voxel_size, smooth_iterations: 2, isotropic_edge: Some(voxel_size * 1.5), offset: -config.wall, thickness: 0.0 };
    let cavity = voxel(&positions, &indices, &options).map_err(|e| match e {
        quadriflow_core::RemeshError::EmptyMesh => Print3dError::InvalidConfig("la pared es demasiado gruesa: no queda hueco".into()),
        other => Print3dError::GeometryError(other.to_string()),
    })?;

    // Exterior + hueco con las caras al revés (miran al vacío)
    let mut all: Vec<Vector3> = pos.clone();
    let mut all_tris: Vec<[usize; 3]> = tris.clone();
    let base = all.len();
    all.extend(cavity.positions.iter().map(|p| Vector3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))));
    for t in cavity.indices.as_chunks::<3>().0 {
        all_tris.push([base + t[0] as usize, base + t[2] as usize, base + t[1] as usize]);
    }
    let result = Mesh::from_triangles(&all, &all_tris);

    let new_volume = compute_signed_volume(&result);
    let cavity_volume = (original_volume - new_volume).max(0.0);
    Ok(HollowResult { mesh: result, cavity_volume, original_volume, saved_fraction: if original_volume > 0.0 { cavity_volume / original_volume } else { 0.0 } })
}

/// Volumen con signo: el exterior suma y las cavidades restan
fn compute_signed_volume(mesh: &Mesh) -> Real {
    let (pos, tris) = triangles(mesh);
    tris.iter().map(|t| pos[t[0]].dot(&pos[t[1]].cross(&pos[t[2]]))).sum::<Real>() / 6.0
}

/// Material que gasta una impresión
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaterialEstimate {
    /// Volumen de material (mm³)
    pub volume_mm3: Real,
    pub mass_g: Real,
    /// Largo de filamento (m)
    pub filament_m: Real,
}

/// Estima el material con `infill` (1 = macizo) y filamento de `filament_diameter` mm
///
/// Con un hueco interior, `infill` solo se aplica al volumen que queda; no cuenta
/// perímetros ni capas superiores, así que es una cota aproximada.
pub fn estimate_material(mesh: &Mesh, material: &MaterialProfile, infill: Real, filament_diameter: Real) -> MaterialEstimate {
    let volume = compute_signed_volume(mesh).abs() * infill.clamp(0.0, 1.0);
    let area = std::f64::consts::PI * (filament_diameter * 0.5).powi(2);
    MaterialEstimate { volume_mm3: volume, mass_g: volume * material.density / 1000.0, filament_m: volume / area / 1000.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::{check_printability, tests::make_box};
    use crate::profile::PrinterProfile;

    #[test]
    fn caja_hueca() {
        let b = make_box(40.0, 40.0, 40.0);
        let r = hollow(&b, &HollowConfig { wall: 3.0, voxel_size: Some(1.0) }).unwrap();
        assert!(r.mesh.is_closed());
        let expected = 34f64.powi(3);
        assert!((r.cavity_volume - expected).abs() / expected < 0.06, "hueco {} esperado {}", r.cavity_volume, expected);
        assert!(r.saved_fraction > 0.5);
        // Paredes de al menos ~la mitad del pedido y sin errores de topología
        let rep = check_printability(&r.mesh, &PrinterProfile::default(), &MaterialProfile::default()).unwrap();
        assert!(rep.is_watertight, "{:?}", rep.issues);
        assert!(rep.min_thickness.unwrap() > 2.4, "grosor {:?}", rep.min_thickness);
        assert_eq!(rep.components, 2);
    }

    #[test]
    fn pared_gruesa_no_deja_hueco() {
        let b = make_box(10.0, 10.0, 10.0);
        assert!(hollow(&b, &HollowConfig { wall: 8.0, voxel_size: Some(1.0) }).is_err());
    }

    #[test]
    fn malla_abierta_se_rechaza() {
        let b = make_box(20.0, 20.0, 20.0);
        let (pos, mut tris) = triangles(&b);
        tris.pop();
        let m = Mesh::from_triangles(&pos, &tris);
        assert!(matches!(hollow(&m, &HollowConfig::default()), Err(Print3dError::MeshNotClosed)));
    }

    #[test]
    fn material_de_una_caja() {
        let b = make_box(10.0, 10.0, 10.0);
        let e = estimate_material(&b, &MaterialProfile::default(), 1.0, 1.75);
        assert!((e.volume_mm3 - 1000.0).abs() < 1e-6);
        assert!((e.mass_g - 1.24).abs() < 1e-9);
        let half = estimate_material(&b, &MaterialProfile::default(), 0.5, 1.75);
        assert!((half.volume_mm3 - 500.0).abs() < 1e-6);
        assert!(e.filament_m > 0.4 && e.filament_m < 0.5);
    }

    #[test]
    fn esfera_hueca() {
        // Esfera UV de radio 20
        let (rings, segs, r) = (40usize, 64usize, 20.0);
        let mut pos = vec![Vector3::new(0.0, 0.0, r)];
        for i in 1..rings {
            let th = std::f64::consts::PI * i as Real / rings as Real;
            for j in 0..segs {
                let ph = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()));
            }
        }
        pos.push(Vector3::new(0.0, 0.0, -r));
        let bottom = pos.len() - 1;
        let at = |i: usize, j: usize| 1 + (i - 1) * segs + j % segs;
        let mut tris = Vec::new();
        for j in 0..segs {
            tris.push([0, at(1, j), at(1, j + 1)]);
            tris.push([bottom, at(rings - 1, j + 1), at(rings - 1, j)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        let sphere = Mesh::from_triangles(&pos, &tris);
        let res = hollow(&sphere, &HollowConfig { wall: 2.0, voxel_size: Some(0.8) }).unwrap();
        let expected = 4.0 / 3.0 * std::f64::consts::PI * 18f64.powi(3);
        assert!((res.cavity_volume - expected).abs() / expected < 0.06, "{} vs {}", res.cavity_volume, expected);
        assert!(res.mesh.is_closed());
    }
}
