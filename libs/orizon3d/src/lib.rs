//! Backend de escaneo 3D tomado de [Orizon3D](https://github.com/Vanne11/Orizon3D)
//! (licencia MIT, ver `LICENSE-ORIZON3D`): captura de los escáneres Revopoint
//! POP 2 / POP 3 por V4L2, nube de puntos, escaneo multi-frame con ICP y
//! reconstrucción de malla. La GUI de Orizon3D (egui) no se incluye: la
//! interfaz es la de Pinocchio y [`Scanner`] reúne lo que hacía su ventana.
//!
//! Coordenadas de cámara en mm: X a la derecha, Y hacia abajo, Z hacia
//! adelante (profundidad).

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

pub mod camera;
pub mod capture;
pub mod mesh;
pub mod pointcloud;
pub mod recording;
pub mod scan;
mod scanner;

pub use camera::{CameraDescription, StreamInfo};
pub use capture::DepthControls;
pub use mesh::Mesh;
pub use pointcloud::PointCloud;
pub use scan::ScanStats;
pub use scanner::{scan_frame_cloud, Measurement, MeshSettings, Preview, ScanSettings, Scanner, ScannerState, ScannerStatus};
