//! Retopologiza un modelo y le da piel: UV trasladadas, o desplegado nuevo
//! con las texturas horneadas.
//!
//! ```text
//! cargo run --release -p uv-core --example retopo_uv -- modelo.glb 5000 salida.glb [--unwrap] [--checker] [--size 2048] [--seams]
//! ```
//!
//! `--seams`: los quads siguen las costuras de UV del modelo.
//! `--paint`: distribución para pintar (con `--unwrap`).

use quadriflow_core::{remesh, RemeshConfig};
use std::time::Instant;
use uv_core::{scene_surface, skin_scene, transferred_skin, unwrapped_skin, BakeOptions, SkinInfo};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(3)) else {
        panic!("uso: retopo_uv <modelo.glb> <quads> <salida.glb> [--unwrap] [--checker] [--size px] [--seams]");
    };
    let target = args[2].parse().expect("quads");
    let flag = |name: &str| args.iter().any(|a| a == name);
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(2048);

    let scene = converter_gltf_io::import_gltf(input).expect("importar");
    let mesh = pinocchio_mesh::scene_to_mesh(&scene).expect("malla");
    let start = Instant::now();
    let config = RemeshConfig { target_faces: target, preserve_seams: flag("--seams"), ..Default::default() };
    let quads = remesh(&mesh, &config).expect("retopología");
    println!("retopología: {} vértices, {} quads; {:.2} s", quads.num_vertices(), quads.num_faces(), start.elapsed().as_secs_f64());
    let quality = quadriflow_core::quality::analyze(&quads, None);
    println!(
        "calidad: {:.1} % irregulares, {} plegados, {} malos, {} estirados",
        quality.irregular_percent(),
        quality.folded_quads,
        quality.poor_quads,
        quality.stretched_quads
    );

    let positions: Vec<[f64; 3]> = quads.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();
    let faces: Vec<[usize; 4]> = quads.faces.iter().map(|f| f.v).collect();

    let start = Instant::now();
    let surface = scene_surface(&scene);
    let skin = match &surface {
        Some(surface) if !flag("--unwrap") => transferred_skin(&scene, surface, &positions, &faces),
        _ => {
            let mut options = BakeOptions { texture_size: size, ..Default::default() };
            if flag("--paint") {
                options.unwrap.layout = uv_core::Layout::Paintable;
            }
            unwrapped_skin(&scene, surface.as_ref(), &positions, &faces, &options)
        }
    };
    match skin.info {
        SkinInfo::Transferred { seam_faces } => println!(
            "UV trasladadas: {seam_faces} caras cruzan costuras ({:.1} %)",
            100.0 * seam_faces as f64 / faces.len() as f64
        ),
        SkinInfo::Unwrapped { num_charts, stretch, coverage, texture_size } => println!(
            "desplegado: {num_charts} cartas, estiramiento {stretch:.3}, cobertura {:.1} %, horneado {texture_size} px",
            100.0 * coverage
        ),
    }
    println!("piel: {:.2} s", start.elapsed().as_secs_f64());

    let skin = if flag("--checker") { skin.with_checker(32) } else { skin };
    let (out, _) = skin_scene(&positions, &faces, Some(&skin), &scene);
    converter_gltf_io::export_glb(&out, output, &Default::default()).expect("exportar");
    println!("escrito {output}");
}
