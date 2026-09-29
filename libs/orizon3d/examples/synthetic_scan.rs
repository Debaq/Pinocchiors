//! Escaneo sintético de extremo a extremo: renderiza la profundidad de un objeto
//! asimétrico girando en una tornamesa, la pasa por la misma cadena que el
//! escáner (nube → limpieza → ICP → fusión → malla) y mide el error contra la
//! forma y las poses reales.
//!
//! cargo run --release -p orizon3d-core --example synthetic_scan -- [grados_por_cuadro=2] [ruido_mm=0.3] [hfov_real=30.7] [malla.ply]

#[path = "../tests/common/synthetic.rs"]
mod synthetic;

use orizon3d_core::camera::DepthFrame;
use orizon3d_core::scan::ScanSession;
use orizon3d_core::{mesh, scan_frame_cloud, ScanSettings};
use synthetic::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, default: f32| args.get(i).map_or(default, |a| a.parse().unwrap());
    let (step, noise, hfov) = (num(0, 2.0), num(1, 0.3), num(2, 30.7));
    let (w, h) = (640u32, 400u32);
    let (params, settings, f) = (cloud_params(w, h), ScanSettings::default(), focal(w, hfov));

    let mut session = ScanSession::new();
    let mut seed = 12345u32;
    let frames = (360.0 / step).round() as usize;
    let mut worst = 0.0f32;
    for i in 0..frames {
        let deg = i as f32 * step;
        let depth = render(deg.to_radians(), w, h, f, noise, &mut seed);
        let cloud = scan_frame_cloud(&DepthFrame { width: w, height: h, depth, timestamp_ms: 0.0 }, None, &params, &settings);
        let ok = session.integrate_frame(&cloud);
        // Error de pose: dónde deja la pose estimada el centro de la tornamesa
        let e = session.pose().apply([0.0, 0.0, CENTER_Z]);
        let t = truth_pose(deg).apply([0.0, 0.0, CENTER_Z]);
        let err = ((e[0] - t[0]).powi(2) + (e[1] - t[1]).powi(2) + (e[2] - t[2]).powi(2)).sqrt();
        worst = worst.max(err);
        if i % (frames / 12).max(1) == 0 || !ok {
            println!(
                "{deg:5.0}°: {:6} pts, {}, rmse {:.2} mm, error de pose {err:.2} mm",
                cloud.points.len(),
                if ok { "sumado    " } else { "DESCARTADO" },
                session.stats.last_rmse
            );
        }
    }
    let report = |name: &str, pts: &[[f32; 3]]| {
        let errs: Vec<f32> = pts.iter().map(|&p| surface_error(p)).collect();
        let mean = errs.iter().sum::<f32>() / errs.len().max(1) as f32;
        let far = errs.iter().filter(|&&e| e > 2.0).count() as f32 / errs.len().max(1) as f32;
        println!("{name}: error medio {mean:.2} mm, a más de 2 mm: {:.1}%", far * 100.0);
    };
    println!("sumados {} / descartados {} · peor error de pose {worst:.2} mm", session.stats.registered, session.stats.dropped);
    let cloud = session.fused_cloud();
    report("nube fusionada", &cloud.points.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>());
    let m = mesh::reconstruct(&cloud, 2.0, 1, 2);
    report("malla", &m.vertices);
    if let Some(path) = args.get(3) {
        m.export_ply(std::path::Path::new(path)).unwrap();
    }
}
