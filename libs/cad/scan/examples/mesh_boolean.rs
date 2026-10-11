//! Banco: ¿aguanta OpenCASCADE booleanas contra una malla cosida como sólido?
//! Simplifica la malla a N triángulos, la cose (`Shape::from_mesh`) y le resta,
//! une e interseca una caja que la corta por la mitad. Mide tiempos y revisa
//! que corte + intersección = caja en volumen.
//!
//! `cargo run --release -p cad-scan --example mesh_boolean -- malla.(stl|obj|glb) N [--voxeles]`
//!
//! Con `--voxeles` la malla se rehace por vóxeles + isótropo (cerrada, manifold
//! y sin cruces) en vez de simplificarla con meshopt.

use std::time::Instant;

use cad_occt::{BooleanOp, Frame, Shape, ShapeKind};
use quadriflow_core::remesh::{SimplifyInput, SimplifyOptions, VoxelOptions, simplify, surface_stats, voxel};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("falta la malla");
    let target: usize = args.next().map(|s| s.parse().unwrap()).unwrap_or(20_000);
    let use_voxels = args.next().as_deref() == Some("--voxeles");

    let scene = converter_core::import(&path).unwrap();
    let mm = (scene.meters_per_unit * 1000.0) as f32;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for prim in scene.world_primitives_z_up() {
        let base = positions.len() as u32;
        positions.extend(prim.positions.iter().map(|p| p.map(|c| c * mm)));
        indices.extend(prim.triangles.iter().flat_map(|t| t.map(|i| i + base)));
    }

    if use_voxels {
        let t0 = Instant::now();
        let area = surface_stats(&positions, &indices).area;
        let edge = (area / (3f64.sqrt() / 4.0 * target as f64)).sqrt();
        let m = voxel(&positions, &indices, &VoxelOptions { voxel_size: edge / 2.0, smooth_iterations: 0, isotropic_edge: Some(edge), offset: 0.0, thickness: 0.0 }).unwrap();
        println!("vóxeles + isótropo (lado {edge:.3} mm) en {:.2} s", t0.elapsed().as_secs_f64());
        positions = m.positions;
        indices = m.indices;
    }

    let t0 = Instant::now();
    let input = SimplifyInput { positions: &positions, indices: &indices, attributes: &[], attribute_weights: &[], groups: &[] };
    let s = simplify(&input, &SimplifyOptions { target_triangles: target, keep_seams: false, ..Default::default() });
    let vertices: Vec<[f64; 3]> = positions.iter().map(|p| p.map(f64::from)).collect();
    let mut triangles: Vec<[u32; 3]> = s.indices.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    // Simplificar deja pares de caras espalda con espalda (mismos 3 vértices):
    // se quitan los dos, son una lámina sin volumen
    let mut count = std::collections::HashMap::<[u32; 3], u32>::new();
    for t in &triangles {
        let mut k = *t;
        k.sort();
        *count.entry(k).or_default() += 1;
    }
    let before = triangles.len();
    triangles.retain(|t| {
        let mut k = *t;
        k.sort();
        count[&k] == 1
    });
    println!("caras repetidas quitadas: {}", before - triangles.len());
    println!("{} → {} triángulos (error {:.3} mm) en {:.2} s", indices.len() / 3, triangles.len(), s.error, t0.elapsed().as_secs_f64());

    // ¿Cerrada y manifold? (aristas con una sola cara o con más de dos)
    let mut edges = std::collections::HashMap::<(u32, u32), u32>::new();
    for t in &triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let open = edges.values().filter(|&&n| n == 1).count();
    let multi = edges.values().filter(|&&n| n > 2).count();
    println!("aristas: {} abiertas, {} de más de dos caras", open, multi);

    let t0 = Instant::now();
    let solid = match Shape::from_mesh(&vertices, &triangles, 1e-4) {
        Ok(s) => s,
        Err(e) => return println!("a sólido: falló ({e})"),
    };
    let kind = solid.kind();
    println!("a sólido: {:.2} s, {:?}, {} caras, válido {}", t0.elapsed().as_secs_f64(), kind, solid.face_count(), solid.is_valid());
    if kind != ShapeKind::Solid {
        println!("no quedó sólido: la malla no está cerrada");
    }
    let mass = solid.mass().unwrap();
    println!("volumen {:.1} mm³", mass.volume);

    // Caja que toma la mitad de abajo y sobresale por los costados
    let (lo, hi) = (mass.bbox_min, mass.bbox_max);
    let pad = [0, 1, 2].map(|k| (hi[k] - lo[k]) * 0.1);
    let mid = (lo[2] + hi[2]) / 2.0;
    let frame = Frame { origin: [lo[0] - pad[0], lo[1] - pad[1], lo[2] - pad[2]], ..Frame::WORLD };
    let block = Shape::make_box(frame, hi[0] - lo[0] + 2.0 * pad[0], hi[1] - lo[1] + 2.0 * pad[1], mid - lo[2] + pad[2]).unwrap();
    let box_volume = block.mass().unwrap().volume;

    let mut volumes = Vec::new();
    for (name, op) in [("restar (caja − malla)", BooleanOp::Cut), ("intersecar", BooleanOp::Intersect), ("unir", BooleanOp::Union)] {
        let t0 = Instant::now();
        match block.boolean(&solid, op) {
            Ok(r) => {
                let v = r.mass().map(|m| m.volume).unwrap_or(f64::NAN);
                println!("{name}: {:.2} s, {:?}, {} caras, válido {}, volumen {v:.1}", t0.elapsed().as_secs_f64(), r.kind(), r.face_count(), r.is_valid());
                volumes.push(v);
            }
            Err(e) => {
                println!("{name}: falló en {:.2} s ({e})", t0.elapsed().as_secs_f64());
                volumes.push(f64::NAN);
            }
        }
    }
    let err = (volumes[0] + volumes[1] - box_volume) / box_volume;
    println!("corte + intersección vs caja: {:+.4} %", err * 100.0);
    let union_err = (volumes[2] - (box_volume + mass.volume - volumes[1])) / volumes[2];
    println!("unión vs caja + malla − común: {:+.4} %", union_err * 100.0);
}
