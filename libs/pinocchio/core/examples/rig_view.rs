//! Evalúa el autorig sobre un modelo real y deja GLB para mirar con f3d:
//! `<salida>_rest.glb` (malla coloreada por pesos + huesos) y
//! `<salida>_pose.glb` (la malla deformada al doblar un hueso).
//!
//! ```text
//! cargo run --release -p pinocchio-core --example rig_view -- modelo.glb quad salida [--bend hueso grados] [--fit auto|none]
//! (el preset puede ser una variante de `BodyPlan`: elephant, octopus, dragon…)
//! ```

use converter_scene::{AlphaMode, IndexData, Material, Mesh as SceneMesh, Node, Primitive, Scene, Transform, VertexAttribute};
use pinocchio_core::math::{Real, Vector3};
use pinocchio_core::skeleton::*;
use pinocchio_core::{autorig, PinocchioConfig, PinocchioOutput, SkeletonFit};
use std::time::Instant;

fn preset(name: &str) -> BasicSkeleton {
    let bones = match name {
        "human" => HumanSkeleton::new().bones().to_vec(),
        "quad" => QuadSkeleton::new().bones().to_vec(),
        "horse" => HorseSkeleton::new().bones().to_vec(),
        "bird" => BirdSkeleton::new().bones().to_vec(),
        "spider" => SpiderSkeleton::new().bones().to_vec(),
        "serpent" => SerpentSkeleton::default().bones().to_vec(),
        "mech" => MechSkeleton::new().bones().to_vec(),
        other => return BodyPlan::variant(other).unwrap_or_else(|| panic!("preset desconocido: {other}")).build(),
    };
    BasicSkeleton::from_bones(bones)
}

/// Color de cada hueso (tonos repartidos)
fn bone_color(b: usize) -> [f32; 3] {
    let h = (b as f32 * 0.618_034).fract() * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.1],
        1 => [x, 1.0, 0.1],
        2 => [0.1, 1.0, x],
        3 => [0.1, x, 1.0],
        4 => [x, 0.1, 1.0],
        _ => [1.0, 0.1, x],
    }
}

/// Octaedros en las articulaciones y prismas finos en los huesos
fn skeleton_geometry(skeleton: &BasicSkeleton, positions: &[Vector3], size: Real) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    let f = |v: Vector3| [v.x() as f32, v.y() as f32, v.z() as f32];
    for (b, &p) in positions.iter().enumerate() {
        let r = size * if skeleton.is_leaf(b) { 0.6 } else { 1.0 };
        let base = pos.len() as u32;
        for d in [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]] {
            pos.push(f(p + Vector3::new(d[0], d[1], d[2]) * r));
            col.push(bone_color(b));
        }
        for [a, c, d] in [[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]] {
            idx.extend([base + a, base + c, base + d]);
        }
        if let Some(parent) = skeleton.get_parent(b) {
            let (a, c) = (positions[parent], p);
            let axis = (c - a).try_normalize().unwrap_or(Vector3::unit_y());
            let helper = if axis.x().abs() < 0.9 { Vector3::unit_x() } else { Vector3::unit_y() };
            let u = axis.cross(&helper).normalize() * (size * 0.35);
            let v = axis.cross(&u).normalize() * (size * 0.35);
            let base = pos.len() as u32;
            for end in [a, c] {
                for off in [u, v, u * -1.0, v * -1.0] {
                    pos.push(f(end + off));
                    col.push([0.95, 0.95, 0.95]);
                }
            }
            for k in 0..4u32 {
                let (a0, a1, b0, b1) = (k, (k + 1) % 4, k + 4, (k + 1) % 4 + 4);
                idx.extend([base + a0, base + a1, base + b1, base + a0, base + b1, base + b0]);
            }
        }
    }
    (pos, col, idx)
}

fn primitive(pos: Vec<[f32; 3]>, col: Vec<[f32; 3]>, idx: Vec<u32>, material: usize) -> Primitive {
    Primitive {
        attributes: vec![
            VertexAttribute::Positions(pos),
            VertexAttribute::Colors(col.into_iter().map(|c| [c[0], c[1], c[2], 1.0]).collect()),
        ],
        indices: Some(IndexData::U32(idx)),
        material: Some(material),
    }
}

fn write(path: &str, prims: Vec<Primitive>) {
    let scene = Scene {
        meshes: vec![SceneMesh { name: "rig".into(), primitives: prims }],
        nodes: vec![Node { name: "rig".into(), transform: Transform::identity(), mesh: Some(0), skin: None, children: vec![] }],
        root_nodes: vec![0],
        materials: vec![
            Material { name: "skin".into(), metallic_factor: 0.0, roughness_factor: 0.8, double_sided: true, ..Default::default() },
            Material {
                name: "skin_ghost".into(),
                base_color_factor: [1.0, 1.0, 1.0, 0.35],
                alpha_mode: AlphaMode::Blend,
                metallic_factor: 0.0,
                roughness_factor: 0.8,
                double_sided: true,
                ..Default::default()
            },
            Material { name: "bones".into(), metallic_factor: 0.0, roughness_factor: 0.5, unlit: true, ..Default::default() },
        ],
        ..Scene::default()
    };
    let options = converter_gltf_io::GlbExportOptions { generate_normals: true, ..Default::default() };
    converter_gltf_io::export_glb(&scene, path, &options).expect("exportar");
    println!("escrito {path}");
}

/// Rotación de `p` alrededor de `center` por el eje `axis` (unitario)
fn rotate(p: Vector3, center: Vector3, axis: Vector3, angle: Real) -> Vector3 {
    let v = p - center;
    let (s, c) = angle.sin_cos();
    center + v * c + axis.cross(&v) * s + axis * (axis.dot(&v) * (1.0 - c))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(kind), Some(out)) = (args.get(1), args.get(2), args.get(3)) else {
        panic!("uso: rig_view <modelo> <preset> <salida> [--bend hueso grados] [--fit auto|none]");
    };
    let bend = args.iter().position(|a| a == "--bend").map(|i| (args[i + 1].clone(), args[i + 2].parse::<Real>().unwrap()));

    let scene = converter_gltf_io::import_gltf(input).expect("importar");
    let mesh = pinocchio_mesh::scene_to_mesh(&scene).expect("malla");
    let skeleton = preset(kind);
    if args.iter().any(|a| a == "--extremities") {
        let bbox = mesh.bounding_box();
        let (center, scale) = (bbox.center(), 1.0 / bbox.longest_axis_length());
        println!(
            "caja: centro ({:+.3}, {:+.3}, {:+.3}), eje mayor {:.3} (extremidades en coordenadas normalizadas)",
            center.x(), center.y(), center.z(), bbox.longest_axis_length()
        );
        let mut normalized = mesh.clone();
        for v in &mut normalized.vertices {
            v.position = (v.position - center) * scale;
        }
        let options = pinocchio_embedding::FitOptions { resolution: 96, ..Default::default() };
        let fit = pinocchio_embedding::fit_skeleton(&normalized, &skeleton, &options).expect("ajuste");
        for (i, e) in fit.extremities.iter().enumerate() {
            let used: Vec<&str> = fit
                .bone_extremity
                .iter()
                .enumerate()
                .filter(|(_, m)| **m == Some(i))
                .map(|(b, _)| skeleton.bones()[b].name.as_str())
                .collect();
            println!(
                "ext {i:>2}: punta ({:+.2}, {:+.2}, {:+.2}) sobresale {:.3} grosor {:.3} {:?}",
                e.tip.x(), e.tip.y(), e.tip.z(), e.length, e.girth, used
            );
        }
    }
    let fit = match args.iter().position(|a| a == "--fit").map(|i| args[i + 1].as_str()) {
        Some("none") => SkeletonFit::None,
        _ => SkeletonFit::Auto,
    };
    let start = Instant::now();
    let output: PinocchioOutput =
        autorig(&mesh, &skeleton, Some(PinocchioConfig::default().with_skeleton_fit(fit))).expect("autorig");
    println!(
        "autorig {:.1} s: {} vértices, {} huesos, calidad {:.2}, {:.2} influencias/vértice",
        start.elapsed().as_secs_f64(),
        output.stats.num_vertices,
        output.stats.num_bones,
        output.stats.embedding_quality,
        output.stats.avg_influences_per_vertex
    );
    for (b, p) in output.bone_positions.iter().enumerate() {
        println!("  {:>12} ({:+.3}, {:+.3}, {:+.3})", skeleton.bones()[b].name, p.x(), p.y(), p.z());
    }

    // Vértices que domina cada hueso y su centro
    let mut dominated = vec![(0usize, Vector3::zero()); skeleton.num_bones()];
    for (v, vert) in mesh.vertices.iter().enumerate() {
        let b = output.get_dominant_bones(v, 1)[0].0;
        dominated[b].0 += 1;
        dominated[b].1 += vert.position;
    }
    for (b, (count, sum)) in dominated.iter().enumerate() {
        let c = *sum * (1.0 / (*count).max(1) as Real);
        println!("  domina {:>12}: {:>6} vértices, centro ({:+.3}, {:+.3}, {:+.3})", skeleton.bones()[b].name, count, c.x(), c.y(), c.z());
    }

    let size = mesh.bounding_box().diagonal() * 0.008;
    let faces: Vec<u32> = (0..mesh.num_faces()).flat_map(|f| mesh.get_face_vertices(f).map(|v| v as u32)).collect();
    let colors: Vec<[f32; 3]> = (0..mesh.num_vertices())
        .map(|v| {
            let mut c = [0.0f32; 3];
            for (b, &w) in output.get_weights(v).iter().enumerate() {
                let bc = bone_color(b);
                for k in 0..3 {
                    c[k] += w as f32 * bc[k];
                }
            }
            c
        })
        .collect();
    let rest: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| [v.position.x() as f32, v.position.y() as f32, v.position.z() as f32]).collect();

    let (sp, sc, si) = skeleton_geometry(&skeleton, &output.bone_positions, size);
    write(&format!("{out}_rest.glb"), vec![primitive(rest.clone(), colors.clone(), faces.clone(), 1), primitive(sp, sc, si, 2)]);

    if let Some((name, degrees)) = bend {
        let bone = skeleton.bones().iter().position(|b| b.name == name).expect("hueso");
        let pivot_bone = skeleton.get_parent(bone).expect("el hueso tiene padre");
        let pivot = output.bone_positions[pivot_bone];
        // Eje: perpendicular al hueso y a la vertical (dobla "hacia abajo/adelante")
        let dir = (output.bone_positions[bone] - pivot).normalize();
        let axis = dir.cross(&Vector3::unit_y()).try_normalize().unwrap_or(Vector3::unit_x());
        let angle = degrees.to_radians();
        // Huesos que se mueven: `bone` y sus descendientes
        let moving: Vec<bool> = (0..skeleton.num_bones())
            .map(|b| {
                let mut cur = Some(b);
                while let Some(c) = cur {
                    if c == bone {
                        return true;
                    }
                    cur = skeleton.get_parent(c);
                }
                false
            })
            .collect();
        let posed: Vec<[f32; 3]> = mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(v, vert)| {
                let w: Real = output.get_weights(v).iter().enumerate().filter(|(b, _)| moving[*b]).map(|(_, w)| w).sum();
                let p = vert.position;
                let q = p * (1.0 - w) + rotate(p, pivot, axis, angle) * w;
                [q.x() as f32, q.y() as f32, q.z() as f32]
            })
            .collect();
        let posed_bones: Vec<Vector3> = output
            .bone_positions
            .iter()
            .enumerate()
            .map(|(b, &p)| if moving[b] { rotate(p, pivot, axis, angle) } else { p })
            .collect();
        let (sp, sc, si) = skeleton_geometry(&skeleton, &posed_bones, size);
        write(&format!("{out}_pose.glb"), vec![primitive(posed, colors, faces, 0), primitive(sp, sc, si, 2)]);
    }
}
