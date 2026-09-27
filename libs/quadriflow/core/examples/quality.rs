//! Mide la calidad de la retopología sobre un modelo real.
//!
//! ```text
//! cargo run --release -p quadriflow-core --example quality -- modelo.glb 5000 [--sharp] [--rebuild always|never] [--curvature 1.0] [--obj salida.obj]
//! ```

use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use quadriflow_core::{remesh_with_callback, QuadMesh, Rebuild, RemeshConfig};
use std::collections::HashMap;
use std::io::Write;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("uso: quality <modelo> [quads] [--sharp] [--obj salida.obj]");
    let target = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5000);
    let sharp = args.iter().any(|a| a == "--sharp");
    let obj = args.iter().position(|a| a == "--obj").and_then(|i| args.get(i + 1));
    let rebuild = match args.iter().position(|a| a == "--rebuild").and_then(|i| args.get(i + 1)).map(String::as_str) {
        Some("always") => Rebuild::Always,
        Some("never") => Rebuild::Never,
        _ => Rebuild::Auto,
    };

    let mesh = load(path);
    println!("entrada: {} vértices, {} triángulos; {}", mesh.num_vertices(), mesh.num_faces(), input_topology(&mesh));

    let curvature = args
        .iter()
        .position(|a| a == "--curvature")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(RemeshConfig::default().curvature_alignment);
    let config = RemeshConfig {
        target_faces: target,
        preserve_sharp: sharp,
        rebuild,
        curvature_alignment: curvature,
        ..Default::default()
    };
    let start = Instant::now();
    let mut last = (start, String::from("inicio"));
    let mut stages = Vec::new();
    let q = remesh_with_callback(&mesh, &config, |_, message| {
        let now = Instant::now();
        stages.push(format!("{} {:.2}s", last.1, (now - last.0).as_secs_f64()));
        last = (now, message.trim_end_matches("...").to_string());
    })
    .expect("retopología");
    let elapsed = start.elapsed();
    println!("etapas: {}", stages[1..].join(" | "));

    let topo = q.topology();
    println!("tiempo: {:.2} s", elapsed.as_secs_f64());
    println!("quads: {} (objetivo {target}, {:+.1} %)", q.num_faces(), 100.0 * (q.num_faces() as f64 / target as f64 - 1.0));
    println!("topología: {topo:?}");

    let (interior, boundary) = valences(&q);
    let irregular: usize = interior.iter().filter(|(v, _)| **v != 4).map(|(_, n)| n).sum();
    let total: usize = interior.values().sum();
    println!(
        "irregulares interiores: {irregular} de {total} ({:.2} %) — valencias {:?}",
        100.0 * irregular as f64 / total.max(1) as f64,
        sorted(&interior)
    );
    println!("valencias de borde: {:?}", sorted(&boundary));

    let (mean_dev, bad_angles, min_angle, max_angle) = angles(&q);
    println!("ángulos: desviación media {mean_dev:.1}°, {bad_angles:.2} % fuera de [60°,120°], rango [{min_angle:.1}°, {max_angle:.1}°]");
    let (aspect_mean, aspect_max, edge_cv) = shape(&q);
    println!("aspecto (lado mayor/menor): medio {aspect_mean:.2}, máx {aspect_max:.1}; variación de aristas {:.1} %", 100.0 * edge_cv);

    let (bvh, diagonal) = bvh(&mesh);
    let (mean_d, max_d) = distance(&q, &bvh);
    println!("distancia a la original (% diagonal): media {:.3}, máx {:.3}", 100.0 * mean_d / diagonal, 100.0 * max_d / diagonal);
    println!("quads desalineados con la superficie (>20°): {:.2} %", 100.0 * misaligned(&q, &bvh, 20.0));

    if let Some(out) = obj {
        let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
        for v in &q.vertices {
            writeln!(f, "v {} {} {}", v.x, v.y, v.z).unwrap();
        }
        for face in &q.faces {
            let [a, b, c, d] = face.v.map(|i| i + 1);
            writeln!(f, "f {a} {b} {c} {d}").unwrap();
        }
        println!("guardado en {out}");
    }
}

fn load(path: &str) -> Mesh {
    let lower = path.to_lowercase();
    let mesh = if lower.ends_with(".stl") {
        pinocchio_mesh::load_stl(path)
    } else {
        pinocchio_mesh::load_mesh(path)
    };
    mesh.expect("no se pudo cargar el modelo")
}

fn sorted(map: &HashMap<usize, usize>) -> Vec<(usize, usize)> {
    let mut v: Vec<_> = map.iter().map(|(&k, &n)| (k, n)).collect();
    v.sort_unstable();
    v
}

fn valences(q: &QuadMesh) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for f in &q.faces {
        for k in 0..4 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 4]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut valence = vec![0; q.vertices.len()];
    let mut on_boundary = vec![false; q.vertices.len()];
    for (&(a, b), &count) in &edges {
        valence[a] += 1;
        valence[b] += 1;
        if count == 1 {
            on_boundary[a] = true;
            on_boundary[b] = true;
        }
    }
    let (mut interior, mut boundary) = (HashMap::new(), HashMap::new());
    for v in 0..q.vertices.len() {
        if valence[v] > 0 {
            *if on_boundary[v] { &mut boundary } else { &mut interior }.entry(valence[v]).or_default() += 1;
        }
    }
    (interior, boundary)
}

fn angles(q: &QuadMesh) -> (f64, f64, f64, f64) {
    let (mut total, mut bad, mut min, mut max) = (0.0, 0usize, 180.0f64, 0.0f64);
    for f in &q.faces {
        for k in 0..4 {
            let p = q.vertices[f.v[k]];
            let (a, b) = (q.vertices[f.v[(k + 1) % 4]] - p, q.vertices[f.v[(k + 3) % 4]] - p);
            let angle = a.angle(&b).to_degrees();
            total += (angle - 90.0).abs();
            bad += usize::from(!(60.0..=120.0).contains(&angle));
            min = min.min(angle);
            max = max.max(angle);
        }
    }
    let n = (4 * q.num_faces()) as f64;
    (total / n, 100.0 * bad as f64 / n, min, max)
}

fn shape(q: &QuadMesh) -> (f64, f64, f64) {
    let (mut sum, mut max) = (0.0, 0.0f64);
    let mut lengths = Vec::new();
    for f in &q.faces {
        let l: Vec<f64> = (0..4).map(|k| (q.vertices[f.v[(k + 1) % 4]] - q.vertices[f.v[k]]).norm()).collect();
        let ratio = l.iter().cloned().fold(0.0, f64::max) / l.iter().cloned().fold(f64::INFINITY, f64::min).max(1e-12);
        sum += ratio;
        max = max.max(ratio);
        lengths.extend(l);
    }
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    let var = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
    (sum / q.num_faces() as f64, max, var.sqrt() / mean)
}

fn bvh(mesh: &Mesh) -> (Bvh, f64) {
    let pos: Vec<Vector3> = mesh.vertices.iter().map(|v| v.position).collect();
    let tris = (0..mesh.num_faces())
        .map(|f| {
            let [a, b, c] = mesh.get_face_vertices(f).map(|i| pos[i]);
            Triangle::new(a, b, c)
        })
        .collect();
    let (mut lo, mut hi) = (pos[0].0, pos[0].0);
    for p in &pos {
        lo = lo.inf(&p.0);
        hi = hi.sup(&p.0);
    }
    (Bvh::build(tris), (hi - lo).norm())
}

/// Distancia desde centros y vértices de los quads a la superficie original.
fn distance(q: &QuadMesh, bvh: &Bvh) -> (f64, f64) {
    let samples = q.faces.iter().map(|f| f.v.iter().map(|&i| q.vertices[i]).sum::<pinocchio_math::nalgebra::Vector3<f64>>() / 4.0);
    let (mut sum, mut max, mut n) = (0.0, 0.0f64, 0usize);
    for p in samples.chain(q.vertices.iter().copied()) {
        let d = bvh.query_distance(&Vector3(p));
        sum += d;
        max = max.max(d);
        n += 1;
    }
    (sum / n as f64, max)
}

/// Componentes, característica de Euler y aristas de borde de la entrada,
/// soldando vértices en la misma posición.
fn input_topology(mesh: &Mesh) -> String {
    let mut ids: HashMap<[u64; 3], usize> = HashMap::new();
    let welded: Vec<usize> = mesh
        .vertices
        .iter()
        .map(|v| {
            let key = [v.position.0.x, v.position.0.y, v.position.0.z].map(f64::to_bits);
            let next = ids.len();
            *ids.entry(key).or_insert(next)
        })
        .collect();
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for f in 0..mesh.num_faces() {
        let t = mesh.get_face_vertices(f).map(|i| welded[i]);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            parent[ra] = rb;
        }
    }
    let components = (0..ids.len()).filter(|&i| find(&mut parent, i) == i).count();
    let boundary = edges.values().filter(|&&c| c == 1).count();
    let non_manifold = edges.values().filter(|&&c| c > 2).count();
    let euler = ids.len() as i64 - edges.len() as i64 + mesh.num_faces() as i64;
    format!("{components} componentes, Euler {euler}, {boundary} aristas de borde, {non_manifold} no-manifold")
}

/// Fracción de quads con alguna mitad cuya normal se aparta más de
/// `max_angle` grados de la del triángulo original más cercano a su centro:
/// quads que cruzan una arista viva o doblados.
fn misaligned(q: &QuadMesh, bvh: &Bvh, max_angle: f64) -> f64 {
    let cos = max_angle.to_radians().cos();
    let bad = q
        .faces
        .iter()
        .filter(|f| {
            let [a, b, c, d] = f.v.map(|i| q.vertices[i]);
            [[a, b, c], [a, c, d]].iter().any(|t| {
                let n = (t[1] - t[0]).cross(&(t[2] - t[0]));
                if n.norm() == 0.0 {
                    return true;
                }
                let center = (t[0] + t[1] + t[2]) / 3.0;
                let Some(hit) = bvh.query_closest(&Vector3(center)) else { return false };
                let tri = bvh.triangle(hit.triangle);
                let m = tri.normal().0;
                n.normalize().dot(&m.normalize()) < cos
            })
        })
        .count();
    bad as f64 / q.num_faces() as f64
}
