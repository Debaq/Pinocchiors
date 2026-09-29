//! Compara las tangentes propias con MikkTSpace sobre un desplegado.
//!
//! ```text
//! cargo run --release -p uv-core --example tangent_check -- modelo.glb 5000
//! ```

use quadriflow_core::{remesh, RemeshConfig};
use uv_core::{corner_frames, mikk_tangents, unwrap, UnwrapOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scene = converter_gltf_io::import_gltf(&args[1]).expect("importar");
    let mesh = pinocchio_mesh::scene_to_mesh(&scene).expect("malla");
    let quads = remesh(&mesh, &RemeshConfig { target_faces: args[2].parse().expect("quads"), ..Default::default() }).expect("retopo");
    let positions: Vec<[f64; 3]> = quads.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();
    let faces: Vec<[usize; 4]> = quads.faces.iter().map(|f| f.v).collect();
    let layout = unwrap(&positions, &faces, &UnwrapOptions::default());
    let frames = corner_frames(&positions, &faces, &layout.corners);
    let mikk = mikk_tangents(&positions, &faces, &layout.corners, &frames.normals);

    let mut angles: Vec<f64> = Vec::new();
    let mut sign_mismatch = 0;
    for (ours, theirs) in frames.tangents.iter().zip(&mikk) {
        for (a, b) in ours.iter().zip(theirs) {
            let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) as f64;
            let la = ((a[0] * a[0] + a[1] * a[1] + a[2] * a[2]) as f64).sqrt();
            let lb = ((b[0] * b[0] + b[1] * b[1] + b[2] * b[2]) as f64).sqrt();
            angles.push((dot / (la * lb).max(1e-12)).clamp(-1.0, 1.0).acos().to_degrees());
            if a[3] != b[3] {
                sign_mismatch += 1;
            }
        }
    }
    angles.sort_by(f64::total_cmp);
    let q = |p: f64| angles[((angles.len() - 1) as f64 * p) as usize];
    let over = |d: f64| 100.0 * angles.iter().filter(|&&a| a > d).count() as f64 / angles.len() as f64;
    println!(
        "{} esquinas: ángulo mediana {:.2}°, p90 {:.2}°, p99 {:.2}°, máx {:.1}°; >5° {:.1} %, >15° {:.1} %; signo distinto {}",
        angles.len(), q(0.5), q(0.9), q(0.99), q(1.0), over(5.0), over(15.0), sign_mismatch
    );
}
