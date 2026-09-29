//! Escaneo de 360° de un objeto sintético por la misma cadena que el escáner:
//! la alineación no debe derivar aunque el objeto deje alguna dirección libre
//! (sus paredes verticales no fijan la altura), y la nube fusionada debe quedar
//! sobre la superficie real.

mod common;

use common::synthetic::*;
use orizon3d_core::camera::DepthFrame;
use orizon3d_core::scan::ScanSession;
use orizon3d_core::{scan_frame_cloud, ScanSettings};

#[test]
fn full_turn_does_not_drift() {
    let (w, h, step) = (640u32, 400u32, 4.0f32);
    let (params, settings, f) = (cloud_params(w, h), ScanSettings::default(), focal(w, 30.7));
    let mut session = ScanSession::new();
    let mut seed = 12345u32;
    let mut worst = 0.0f32;
    for i in 0..(360.0 / step) as usize {
        let deg = i as f32 * step;
        let depth = render(deg.to_radians(), w, h, f, 0.3, &mut seed);
        let cloud = scan_frame_cloud(&DepthFrame { width: w, height: h, depth, timestamp_ms: 0.0 }, None, &params, &settings);
        assert!(session.integrate_frame(&cloud), "cuadro {i} descartado");
        let e = session.pose().apply([0.0, 0.0, CENTER_Z]);
        let t = truth_pose(deg).apply([0.0, 0.0, CENTER_Z]);
        worst = worst.max(((e[0] - t[0]).powi(2) + (e[1] - t[1]).powi(2) + (e[2] - t[2]).powi(2)).sqrt());
    }
    let cloud = session.fused_cloud();
    let mean = cloud.points.iter().map(|p| surface_error([p.x, p.y, p.z])).sum::<f32>() / cloud.points.len() as f32;
    println!("peor error de pose {worst:.2} mm, error medio de la nube {mean:.2} mm");
    assert!(worst < 8.0, "la pose derivó {worst:.2} mm");
    assert!(mean < 1.5, "error medio de la nube {mean:.2} mm");
}
