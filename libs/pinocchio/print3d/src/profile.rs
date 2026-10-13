//! Perfiles de impresora y material, y comprobación de volumen de impresión

use pinocchio_math::Real;
use serde::{Deserialize, Serialize};

/// Impresora FDM: cama y boquilla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterProfile {
    pub name: String,
    /// Volumen de impresión [x, y, z] en mm
    pub build_volume: [Real; 3],
    /// Diámetro de la boquilla (mm)
    pub nozzle: Real,
    /// Altura de capa habitual (mm)
    pub layer_height: Real,
    /// Holgura de base entre una pieza y su alojamiento, en la medida total (mm); parte de la boquilla
    #[serde(default = "default_fit_clearance")]
    pub fit_clearance: Real,
    /// Cuánto más chico sale un agujero de lo diseñado, por lado (mm)
    #[serde(default = "default_hole_comp")]
    pub hole_comp: Real,
}

fn default_fit_clearance() -> Real {
    0.4
}

fn default_hole_comp() -> Real {
    0.1
}

impl PrinterProfile {
    pub fn new(name: &str, build_volume: [Real; 3], nozzle: Real, layer_height: Real) -> Self {
        // La holgura de base sigue a la boquilla (una pasada); los agujeros salen ~0,25 de boquilla más chicos
        Self { name: name.to_string(), build_volume, nozzle, layer_height, fit_clearance: nozzle, hole_comp: nozzle * 0.25 }
    }

    /// Perfiles de impresoras comunes
    pub fn presets() -> Vec<PrinterProfile> {
        vec![
            Self::new("Creality Ender 3", [220.0, 220.0, 250.0], 0.4, 0.2),
            Self::new("Creality K1", [220.0, 220.0, 250.0], 0.4, 0.2),
            Self::new("Prusa MK4", [250.0, 210.0, 220.0], 0.4, 0.2),
            Self::new("Prusa MINI", [180.0, 180.0, 180.0], 0.4, 0.2),
            Self::new("Bambu Lab X1 / P1", [256.0, 256.0, 256.0], 0.4, 0.2),
            Self::new("Voron 2.4 (350)", [350.0, 350.0, 330.0], 0.4, 0.2),
        ]
    }

    /// Ancho mínimo de pared imprimible: dos pasadas de boquilla
    pub fn min_wall(&self) -> Real {
        self.nozzle * 2.0
    }

    /// Detalle mínimo: una pasada de boquilla
    pub fn min_feature(&self) -> Real {
        self.nozzle
    }
}

impl Default for PrinterProfile {
    fn default() -> Self {
        Self::presets().remove(0)
    }
}

/// Material de impresión
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialProfile {
    pub name: String,
    /// Densidad (g/cm³)
    pub density: Real,
    /// Voladizo máximo sin soporte, medido desde la vertical (grados)
    pub max_overhang_deg: Real,
    /// Contracción lineal al enfriar por eje [x, y, z] (fracción: 0,004 = 0,4 %)
    #[serde(default)]
    pub shrink: [Real; 3],
    /// Cuánto se multiplica la holgura de base (los materiales que ceden o hacen hilos necesitan más)
    #[serde(default = "default_clearance_factor")]
    pub clearance_factor: Real,
}

fn default_clearance_factor() -> Real {
    1.0
}

impl MaterialProfile {
    pub fn new(name: &str, density: Real, max_overhang_deg: Real) -> Self {
        Self { name: name.to_string(), density, max_overhang_deg, shrink: [0.0; 3], clearance_factor: 1.0 }
    }

    /// El mismo material con contracción `shrink` (igual en los tres ejes) y factor de holgura
    pub fn with_fit(mut self, shrink: Real, clearance_factor: Real) -> Self {
        self.shrink = [shrink; 3];
        self.clearance_factor = clearance_factor;
        self
    }

    pub fn presets() -> Vec<MaterialProfile> {
        vec![
            Self::new("PLA", 1.24, 50.0).with_fit(0.003, 1.0),
            Self::new("PETG", 1.27, 45.0).with_fit(0.004, 1.1),
            Self::new("ABS", 1.04, 45.0).with_fit(0.008, 1.1),
            Self::new("ASA", 1.07, 45.0).with_fit(0.006, 1.1),
            Self::new("TPU", 1.21, 35.0).with_fit(0.010, 1.5),
        ]
    }
}

impl Default for MaterialProfile {
    fn default() -> Self {
        Self::presets().remove(0)
    }
}

/// Resultado de comprobar si una pieza cabe en la cama
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitReport {
    /// Cabe tal cual, con el margen pedido
    pub fits: bool,
    /// Cabe girándola sobre el eje Z (intercambiando X e Y)
    pub fits_rotated: bool,
    /// Cuánto se pasa por eje (mm, 0 si cabe)
    pub excess: [Real; 3],
    /// Piezas necesarias por eje para cortar a la medida de la cama
    pub pieces_needed: [usize; 3],
}

/// ¿Caben `dimensions` (x, y, z) en `build_volume` dejando `margin` por lado?
pub fn check_fit(dimensions: [Real; 3], build_volume: [Real; 3], margin: Real) -> FitReport {
    let avail = build_volume.map(|v| (v - 2.0 * margin).max(1e-6));
    let excess = [0, 1, 2].map(|i| (dimensions[i] - avail[i]).max(0.0));
    let fits = excess.iter().all(|&e| e <= 1e-9);
    let fits_rotated = dimensions[1] <= avail[0] + 1e-9 && dimensions[0] <= avail[1] + 1e-9 && dimensions[2] <= avail[2] + 1e-9;
    let pieces_needed = [0, 1, 2].map(|i| (dimensions[i] / avail[i] - 1e-9).ceil().max(1.0) as usize);
    FitReport { fits, fits_rotated, excess, pieces_needed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caben_y_no_caben() {
        let ok = check_fit([100.0, 100.0, 100.0], [220.0, 220.0, 250.0], 5.0);
        assert!(ok.fits);
        assert_eq!(ok.pieces_needed, [1, 1, 1]);

        let grande = check_fit([450.0, 100.0, 100.0], [220.0, 220.0, 250.0], 5.0);
        assert!(!grande.fits);
        assert!((grande.excess[0] - 240.0).abs() < 1e-9);
        assert_eq!(grande.pieces_needed, [3, 1, 1]);
    }

    #[test]
    fn cabe_girada() {
        let r = check_fit([240.0, 100.0, 50.0], [250.0, 210.0, 220.0], 0.0);
        assert!(r.fits);
        let r = check_fit([240.0, 200.0, 50.0], [250.0, 210.0, 220.0], 0.0);
        assert!(r.fits && !r.fits_rotated);
        let r = check_fit([205.0, 245.0, 50.0], [250.0, 210.0, 220.0], 0.0);
        assert!(!r.fits && r.fits_rotated);
    }

    #[test]
    fn presets_no_vacios() {
        assert!(PrinterProfile::presets().len() >= 4);
        assert!(MaterialProfile::presets().iter().any(|m| m.name == "PLA"));
        assert!((PrinterProfile::default().min_wall() - 0.8).abs() < 1e-12);
    }
}
