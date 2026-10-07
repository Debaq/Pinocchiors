//! Banco de reparación: analiza, repara y vuelve a analizar un modelo.
//!
//! ```text
//! cargo run --release -p pinocchio-repair --example repair_report -- modelo.stl [--no-fill] [--union] [--brief] [--punch N] [--obj salida.obj]
//! ```
//!
//! `--punch N` abre N agujeros en el modelo antes de repararlo (para probar el
//! relleno); con `--obj` también guarda `<salida>.agujereado.obj`.
//!
//! Con `--brief` imprime una sola línea: defectos antes → después. `--union`
//! une los cuerpos solapados (corta por las auto-intersecciones); `--cruces`
//! cuenta las auto-intersecciones sin unir.

use pinocchio_repair::{analyze, repair_all, AnalysisConfig, MeshDiagnostics, RepairConfig, TriMesh};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("uso: repair_report <modelo> [--no-fill] [--obj salida.obj]");
        std::process::exit(2);
    };
    let obj_out = args.iter().position(|a| a == "--obj").and_then(|i| args.get(i + 1));

    let mut mesh = pinocchio_mesh::load_mesh(path).expect("no se pudo cargar el modelo");
    if let Some(n) = args.iter().position(|a| a == "--punch").and_then(|i| args.get(i + 1)) {
        mesh = punch_holes(&mesh, n.parse().expect("número de agujeros"));
        if let Some(out) = obj_out {
            pinocchio_mesh::io::save_obj(&mesh, format!("{out}.agujereado.obj")).expect("guardar OBJ");
        }
    }
    let brief = args.iter().any(|a| a == "--brief");
    let union = args.iter().any(|a| a == "--union");
    let crossings = !brief || union || args.iter().any(|a| a == "--cruces");
    let analysis = AnalysisConfig { check_self_intersections: crossings, ..Default::default() };
    if brief {
        let before = analyze(&mesh, &analysis);
        let config = RepairConfig {
            fill_holes: !args.iter().any(|a| a == "--no-fill"),
            resolve_intersections: union,
            ..Default::default()
        };
        let t = Instant::now();
        let summary = repair_all(&mut mesh, &config).expect("reparación");
        let secs = t.elapsed().as_secs_f64();
        let after = analyze(&mesh, &analysis);
        println!(
            "{:>7} caras {:6.2}s | {} → {} | rellenos {} omitidos {} | cortes {} (omitidos {}) parches {} | vol {:.4e} → {:.4e}",
            before.num_faces,
            secs,
            line(&before),
            line(&after),
            summary.holes_filled,
            summary.holes_skipped,
            summary.intersections_cut,
            summary.intersections_skipped,
            summary.inner_patches_removed,
            before.volume,
            after.volume
        );
        if let Some(out) = obj_out {
            pinocchio_mesh::io::save_obj(&mesh, out).expect("no se pudo guardar el OBJ");
        }
        return;
    }

    let t = Instant::now();
    let before = analyze(&mesh, &analysis);
    println!("ANTES   ({:.2}s) v={} f={}", t.elapsed().as_secs_f64(), mesh.num_vertices(), mesh.num_faces());
    println!("{before:#?}");

    let config = RepairConfig {
            fill_holes: !args.iter().any(|a| a == "--no-fill"),
            resolve_intersections: union,
            ..Default::default()
        };
    let t = Instant::now();
    match repair_all(&mut mesh, &config) {
        Ok(summary) => println!("REPARACIÓN ({:.2}s): {summary:#?}", t.elapsed().as_secs_f64()),
        Err(e) => println!("REPARACIÓN falló: {e}"),
    }

    let after = analyze(&mesh, &analysis);
    println!("DESPUÉS v={} f={}", mesh.num_vertices(), mesh.num_faces());
    println!("{after:#?}");

    if let Some(out) = obj_out {
        pinocchio_mesh::io::save_obj(&mesh, out).expect("no se pudo guardar el OBJ");
    }
}

fn line(d: &MeshDiagnostics) -> String {
    format!(
        "{}{} huecos {} dup {} deg {} nmE {} nmV {} inc {} piezas {} cruces {}",
        if d.is_healthy() { "SANA " } else { "" },
        if d.normals_outward == Some(false) { "INV " } else { "" },
        d.boundary_loops,
        d.duplicate_vertices,
        d.degenerate_faces,
        d.non_manifold_edges,
        d.non_manifold_vertices,
        d.inconsistent_edges,
        d.connected_components,
        d.self_intersections
    )
}

/// Quita las caras cuyo centroide cae cerca de `count` vértices repartidos
fn punch_holes(mesh: &pinocchio_mesh::Mesh, count: usize) -> pinocchio_mesh::Mesh {
    let mut m = TriMesh::from_mesh(mesh);
    let radius = 0.05 * m.diagonal();
    let centers: Vec<_> = (0..count).map(|k| m.positions[(k * 7919 + 13) * m.num_vertices() / (count * 7919 + 17)]).collect();
    let positions = m.positions.clone();
    m.retain_faces(|_, t| {
        let centroid = (positions[t[0]] + positions[t[1]] + positions[t[2]]) / 3.0;
        centers.iter().all(|p| (centroid - *p).length() > radius)
    });
    m.remove_unreferenced_vertices();
    m.to_mesh()
}
