//! Mide el desplegado (sin horneado) a varias densidades de retopología.
//!
//! ```text
//! cargo run --release -p uv-core --example unwrap_bench -- modelo.glb 2000 5000 12000 [--size 2048] [--tri]
//! ```
//!
//! `--tri`: despliega además la malla original de triángulos.

use quadriflow_core::{remesh, RemeshConfig};
use std::time::Instant;
use uv_core::{unwrap, UnwrapOptions};

fn report<const N: usize>(label: &str, positions: &[[f64; 3]], faces: &[[usize; N]], options: &UnwrapOptions) {
    let start = Instant::now();
    let result = unwrap(positions, faces, options);
    println!(
        "{label:>10} caras {:>7}: {:>4} cartas, estiramiento {:.3}, cobertura {:.1} %, {:.2} s",
        faces.len(),
        result.num_charts,
        result.stretch,
        100.0 * result.coverage,
        start.elapsed().as_secs_f64()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).expect("uso: unwrap_bench <modelo.glb> <quads>... [--size px] [--tri]");
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(2048);
    let targets: Vec<usize> = args[2..].iter().filter_map(|a| a.parse().ok()).filter(|&t| t != size as usize).collect();
    let options = UnwrapOptions { texture_size: size, ..Default::default() };

    let scene = converter_gltf_io::import_gltf(input).expect("importar");
    let mesh = pinocchio_mesh::scene_to_mesh(&scene).expect("malla");
    if args.iter().any(|a| a == "--tri") {
        let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| [v.position.x(), v.position.y(), v.position.z()]).collect();
        let faces: Vec<[usize; 3]> = (0..mesh.num_faces()).map(|f| mesh.get_face_vertices(f)).collect();
        report("original", &positions, &faces, &options);
    }
    for target in targets {
        let config = RemeshConfig { target_faces: target, ..Default::default() };
        let quads = remesh(&mesh, &config).expect("retopología");
        let positions: Vec<[f64; 3]> = quads.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();
        let faces: Vec<[usize; 4]> = quads.faces.iter().map(|f| f.v).collect();
        report(&format!("{target} q"), &positions, &faces, &options);
    }
}
