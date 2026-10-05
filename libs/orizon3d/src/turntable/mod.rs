//! Escáner de plato giratorio con webcam UVC: el objeto gira sobre un plato
//! movido por un motor paso a paso, la cámara sube por un arco y uno o más
//! láseres de línea vertical lo cortan. Cada línea vista por la cámara se
//! triangula contra el plano calibrado de su láser y se lleva al marco del
//! objeto con el ángulo del plato.
//!
//! Marco del mundo, en mm: origen en el centro del plato sobre su cara de
//! apoyo, Z hacia arriba (el eje del plato). Marco del objeto: el del mundo con
//! el plato en 0°. Cámara al estilo OpenCV: X a la derecha, Y hacia abajo, Z
//! hacia adelante; los píxeles enteros son centros de píxel.
//!
//! Todo lo que toca el hardware pasa por [`Rig`] y [`FrameSource`]: el banco
//! sintético ([`synth`]) los implementa con un render del rig, así la misma
//! cadena se prueba sin hardware y con error medible contra la forma real.

pub mod laser;
pub mod lens;
pub mod rig;
pub mod scan;
pub mod scanear;
pub mod synth;

pub use laser::{LineSettings, TriangulateSettings};
pub use lens::CameraModel;
pub use rig::{Axis, Calibration, LaserMount, Plane, RigGeometry, View};
pub use scan::{run_laser_scan, FrameSource, Rig, ScanPlan};

/// Vector en mm (doble precisión: la calibración y la triangulación lo piden)
pub type Vec3 = nalgebra::Vector3<f64>;
