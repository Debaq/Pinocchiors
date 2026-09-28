//! Retopologiza un modelo con textura y le traspasa las UV originales.
//!
//! ```text
//! cargo run --release -p uv-core --example retopo_uv -- modelo.glb 5000 salida.glb
//! ```

use converter_scene::{IndexData, Mesh, Node, Primitive, Scene, Transform, VertexAttribute};
use quadriflow_core::{remesh, RemeshConfig};
use std::time::Instant;
use uv_core::{transfer_uvs, UvPart, UvSurface};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(3)) else {
        panic!("uso: retopo_uv <modelo.glb> <quads> <salida.glb>");
    };
    let target = args[2].parse().expect("quads");

    let scene = converter_gltf_io::import_gltf(input).expect("importar");
    let mesh = pinocchio_mesh::scene_to_mesh(&scene).expect("malla");
    let quads = remesh(&mesh, &RemeshConfig { target_faces: target, ..Default::default() }).expect("retopología");
    println!("retopología: {} vértices, {} quads", quads.num_vertices(), quads.num_faces());

    let start = Instant::now();
    let prims = scene.world_primitives();
    let surface = UvSurface::new(prims.iter().filter_map(|p| {
        Some(UvPart {
            group: p.material.map_or(0, |m| m + 1),
            positions: &p.positions,
            uvs: p.uvs.as_deref()?,
            triangles: &p.triangles,
        })
    }))
    .expect("el modelo no tiene UV");
    let positions: Vec<[f64; 3]> = quads.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();
    let faces: Vec<[usize; 4]> = quads.faces.iter().map(|f| f.v).collect();
    let transfer = transfer_uvs(&surface, &positions, &faces);
    println!(
        "UV: {} triángulos, {} cartas; {} caras cruzan costuras ({:.1} %); {:.2} s",
        surface.num_triangles(),
        surface.num_charts(),
        transfer.seam_faces,
        100.0 * transfer.seam_faces as f64 / faces.len() as f64,
        start.elapsed().as_secs_f64()
    );

    // Un vértice por esquina: basta para mirar el resultado
    let mut groups = transfer.groups.clone();
    groups.sort_unstable();
    groups.dedup();
    let primitives = groups
        .into_iter()
        .map(|group| {
            let (mut pos, mut uv) = (Vec::new(), Vec::new());
            for (face, corners) in faces.iter().zip(&transfer.corners).zip(&transfer.groups).filter(|(_, g)| **g == group).map(|(f, _)| f) {
                for k in [0, 1, 2, 0, 2, 3] {
                    pos.push(positions[face[k]].map(|c| c as f32));
                    uv.push(corners[k]);
                }
            }
            let n = pos.len() as u32;
            Primitive {
                attributes: vec![VertexAttribute::Positions(pos), VertexAttribute::TexCoords(0, uv)],
                indices: Some(IndexData::U32((0..n).collect())),
                material: group.checked_sub(1),
            }
        })
        .collect();
    let out = Scene {
        meshes: vec![Mesh { name: "retopo".into(), primitives }],
        nodes: vec![Node { name: "retopo".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] }],
        root_nodes: vec![0],
        materials: scene.materials.clone(),
        textures: scene.textures.clone(),
        meters_per_unit: scene.meters_per_unit,
        y_up: scene.y_up,
        ..Scene::default()
    };
    let options = converter_gltf_io::GlbExportOptions { generate_normals: true, ..Default::default() };
    converter_gltf_io::export_glb(&out, output, &options).expect("exportar");
    println!("escrito {output}");
}
