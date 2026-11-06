//! Parámetros por capa (SLA11): el área expuesta de cada capa, la **subida dinámica**
//! (más lenta y más alta en capas con mucha área: al despegar del film la fuerza crece con
//! el área) y la **exposición por tramos de altura**.
//!
//! El `.ctb` v2 de la LD-002H guarda z, exposición y luz apagada por capa, pero la impresora
//! usa los valores generales (los archivos v2 no permiten parámetros por capa); la subida por
//! capa recién está en v3. Por eso [`layer_params`] devuelve valores por capa solo si el
//! formato los admite ([`ResinSettings::per_layer`]); si no, todas las capas llevan los
//! generales y el tiempo estimado es el real.

use serde::{Deserialize, Serialize};

use crate::sla::{Layer, ResinPrinter, ResinSettings};

/// Lo que la impresora hace en una capa.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayerParams {
    /// Exposición y espera con la luz apagada, s.
    pub exposure: f32,
    pub light_off: f32,
    /// Subida (mm) y su velocidad (mm/min); bajada, mm/min.
    pub lift: f32,
    pub lift_speed: f32,
    pub retract_speed: f32,
    /// Potencia de la luz (0–255).
    pub pwm: u8,
}

/// Exposición distinta entre dos alturas (z de la cara de arriba de la capa, mm; `from`
/// incluido, `to` no). No toca las capas de base.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExposureRange {
    pub from: f32,
    pub to: f32,
    pub exposure: f32,
}

/// Subida dinámica: hasta `area_min` mm² se usan la subida y la velocidad normales; desde
/// `area_max` mm², `lift_max` mm a `speed_min` mm/min; en el medio, una recta.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicLift {
    pub enabled: bool,
    pub area_min: f32,
    pub area_max: f32,
    pub speed_min: f32,
    pub lift_max: f32,
}

impl Default for DynamicLift {
    fn default() -> Self {
        Self { enabled: false, area_min: 200.0, area_max: 2500.0, speed_min: 30.0, lift_max: 10.0 }
    }
}

/// Área expuesta de cada capa, mm² (los grises cuentan su fracción).
pub fn layer_areas(layers: &[Layer], printer: &ResinPrinter) -> Vec<f32> {
    let (pw, ph) = printer.pixel_size();
    layers.iter().map(|l| (l.light() * pw as f64 * ph as f64) as f32).collect()
}

/// Los parámetros de cada capa (ver el comentario del módulo).
pub fn layer_params(areas: &[f32], z: &[f32], s: &ResinSettings) -> Vec<LayerParams> {
    let per_layer = s.per_layer();
    areas
        .iter()
        .zip(z)
        .enumerate()
        .map(|(i, (&area, &z))| {
            if (i as u32) < s.bottom_layers {
                return LayerParams {
                    exposure: s.bottom_exposure,
                    light_off: s.bottom_light_off,
                    lift: s.bottom_lift,
                    lift_speed: s.bottom_lift_speed,
                    retract_speed: s.retract_speed,
                    pwm: s.bottom_pwm,
                };
            }
            let mut p = LayerParams {
                exposure: s.exposure,
                light_off: s.light_off,
                lift: s.lift,
                lift_speed: s.lift_speed,
                retract_speed: s.retract_speed,
                pwm: s.pwm,
            };
            if !per_layer {
                return p;
            }
            if let Some(r) = s.exposure_ranges.iter().rev().find(|r| z >= r.from - 1e-4 && z < r.to - 1e-4) {
                p.exposure = r.exposure;
            }
            let d = &s.dynamic_lift;
            if d.enabled {
                let t = if d.area_max > d.area_min { ((area - d.area_min) / (d.area_max - d.area_min)).clamp(0.0, 1.0) } else { (area >= d.area_max) as u8 as f32 };
                if d.speed_min < p.lift_speed {
                    p.lift_speed += (d.speed_min - p.lift_speed) * t;
                }
                if d.lift_max > p.lift {
                    p.lift += (d.lift_max - p.lift) * t;
                }
            }
            p
        })
        .collect()
}

/// Tiempo de una capa (s): espera, exposición, subida y bajada.
pub fn layer_time(p: &LayerParams) -> f32 {
    let up = if p.lift_speed > 0.0 { p.lift / p.lift_speed * 60.0 } else { 0.0 };
    let down = if p.retract_speed > 0.0 { p.lift / p.retract_speed * 60.0 } else { 0.0 };
    p.exposure + p.light_off + up + down
}

/// Tiempo total (s).
pub fn total_time(params: &[LayerParams]) -> f32 {
    params.iter().map(layer_time).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::print_time;

    fn ramp(n: usize) -> (Vec<f32>, Vec<f32>) {
        ((0..n).map(|i| i as f32 * 10.0).collect(), (0..n).map(|i| (i + 1) as f32 * 0.05).collect())
    }

    #[test]
    fn sin_cambios_da_el_tiempo_de_siempre() {
        let s = ResinSettings::default();
        let (a, z) = ramp(500);
        let p = layer_params(&a, &z, &s);
        assert!((total_time(&p) - print_time(500, &s)).abs() < 0.5);
        assert_eq!(p[0].exposure, s.bottom_exposure);
        assert_eq!(p[8].exposure, s.exposure);
    }

    #[test]
    fn subida_dinamica_y_tramos() {
        let mut s = ResinSettings { ctb_version: 3, ..Default::default() };
        s.dynamic_lift = DynamicLift { enabled: true, area_min: 100.0, area_max: 1100.0, speed_min: 30.0, lift_max: 9.0 };
        s.exposure_ranges = vec![ExposureRange { from: 1.0, to: 2.0, exposure: 5.0 }];
        let (a, z) = ramp(200);
        let p = layer_params(&a, &z, &s);
        // Base: sin tocar.
        assert_eq!(p[3].lift_speed, s.bottom_lift_speed);
        // Área 100 → normal; 600 → mitad; 1100 o más → lo más lento.
        assert_eq!(p[10].lift_speed, 70.0);
        assert!((p[60].lift_speed - 50.0).abs() < 1e-3 && (p[60].lift - 8.0).abs() < 1e-3);
        assert_eq!(p[150].lift_speed, 30.0);
        // z 1,0 (capa 19) a 2,0 (capa 39, excluida).
        assert_eq!(p[18].exposure, 3.5);
        assert_eq!(p[19].exposure, 5.0);
        assert_eq!(p[38].exposure, 5.0);
        assert_eq!(p[39].exposure, 3.5);
        assert!(total_time(&p) > print_time(200, &s));
    }

    #[test]
    fn el_ctb_v2_no_lleva_valores_por_capa() {
        let mut s = ResinSettings::default();
        s.dynamic_lift.enabled = true;
        s.exposure_ranges = vec![ExposureRange { from: 0.0, to: 100.0, exposure: 9.0 }];
        let (a, z) = ramp(100);
        assert!(layer_params(&a, &z, &s).iter().skip(8).all(|p| p.exposure == 3.5 && p.lift_speed == 70.0));
    }
}
