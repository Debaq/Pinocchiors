//! Bucle de escaneo: mueve el rig por el plan, toma los cuadros de cada láser
//! y junta los puntos triangulados en una sola nube.

use std::io;

use crate::camera::RgbFrame;
use crate::pointcloud::PointCloud;

use super::laser::{extract_vertical_line, triangulate, LineSettings, TriangulateSettings};
use super::rig::Calibration;

/// Mecánica del rig: plato, arco, láseres y luz del domo
pub trait Rig {
    /// Lleva plato y arco a su origen (finales de carrera)
    fn home(&mut self) -> io::Result<()>;
    /// Mueve plato y arco (grados) y vuelve cuando quedaron quietos
    fn move_to(&mut self, plate_deg: f64, elevation_deg: f64) -> io::Result<()>;
    fn set_laser(&mut self, index: usize, on: bool) -> io::Result<()>;
    /// Luz del domo, de 0 a 1
    fn set_light(&mut self, level: f64) -> io::Result<()>;
}

/// Cámara: entrega un cuadro tomado después de la llamada (sin cuadros viejos
/// del búfer)
pub trait FrameSource {
    fn grab(&mut self) -> io::Result<RgbFrame>;
}

/// Qué tomar en un escaneo
#[derive(Debug, Clone)]
pub struct ScanPlan {
    /// Alturas del arco (grados), cada una con su vista en la calibración
    pub elevations: Vec<f64>,
    /// Paradas del plato por vuelta
    pub steps: u32,
    /// Láseres a usar, de a uno por cuadro
    pub lasers: Vec<usize>,
    /// Luz del domo durante los cuadros del láser (0 = a oscuras, más
    /// contraste)
    pub laser_light: f64,
    /// Luz para el cuadro de color de cada parada; `None` = sin color
    pub color_light: Option<f64>,
}

impl Default for ScanPlan {
    fn default() -> Self {
        ScanPlan { elevations: vec![0.0, 30.0, 60.0], steps: 120, lasers: vec![0, 1], laser_light: 0.0, color_light: Some(1.0) }
    }
}

impl ScanPlan {
    pub fn stops(&self) -> usize {
        self.elevations.len() * self.steps as usize
    }
}

/// Escanea con láser según `plan`. `progress(hechas, total)` se llama tras
/// cada parada; si devuelve `false` el escaneo se corta y se devuelve lo que
/// haya. La nube queda en el marco del objeto
pub fn run_laser_scan(
    rig: &mut dyn Rig,
    camera: &mut dyn FrameSource,
    calib: &Calibration,
    plan: &ScanPlan,
    line: &LineSettings,
    tri: &TriangulateSettings,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> io::Result<PointCloud> {
    let mut cloud = PointCloud { points: Vec::new(), has_color: plan.color_light.is_some() };
    let total = plan.stops();
    rig.home()?;
    for &laser in &plan.lasers {
        rig.set_laser(laser, false)?;
    }
    let mut done = 0;
    for &elevation in &plan.elevations {
        let view = calib
            .view(elevation)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("sin calibración para el arco a {elevation}°")))?;
        for step in 0..plan.steps {
            let plate = step as f64 * 360.0 / plan.steps as f64;
            rig.move_to(plate, elevation)?;
            let color = match plan.color_light {
                Some(level) => {
                    rig.set_light(level)?;
                    Some(camera.grab()?)
                }
                None => None,
            };
            rig.set_light(plan.laser_light)?;
            let dark = camera.grab()?;
            for &laser in &plan.lasers {
                rig.set_laser(laser, true)?;
                let lit = camera.grab()?;
                rig.set_laser(laser, false)?;
                let points = extract_vertical_line(&lit, Some(&dark), line);
                let part = triangulate(&points, calib, view, laser, plate, color.as_ref(), tri);
                cloud.points.extend(part.points);
            }
            done += 1;
            if !progress(done, total) {
                return Ok(cloud);
            }
        }
    }
    Ok(cloud)
}
