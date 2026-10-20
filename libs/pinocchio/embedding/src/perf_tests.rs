//! Benchmarks de rendimiento de pinocchio-embedding (campo medial, grafo de
//! esferas, embebido del esqueleto y ajuste de plantilla a la malla).
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-embedding perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa entradas diminutas (humo); `PERF_SCALE=large` una
//!   icosfera de ~330k triangulos y campo de 96^3; sin variable, ~82k
//!   triangulos y campo de 64^3.
//! - Hilos de rayon: `RAYON_NUM_THREADS=<n>` para medir escalado (el campo de
//!   distancias con signo y el embebido usan rayon).
//! - Cada benchmark imprime `PERF crate=.. bench=.. threads=.. tris=.. ms=<mediana>`;
//!   en los benchmarks de esferas `tris` es el numero de esferas.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::{
    fit_skeleton, full_embedding_pipeline, medial_spheres_from_field, pack_spheres, FitOptions, MedialSphere,
    SphereGraph,
};
use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::{BasicSkeleton, Bone, Skeleton};
use pinocchio_spatial::DistanceField;
use std::collections::HashMap;
use std::time::Instant;

const CRATE: &str = "embedding";

fn scale() -> &'static str {
    match std::env::var("PERF_SCALE").as_deref() {
        Ok("small") => "small",
        Ok("large") => "large",
        _ => "default",
    }
}

/// Subdivisiones de la icosfera segun la escala (20 * 4^s triangulos).
fn subdivisions() -> usize {
    match scale() {
        "small" => 3,
        "large" => 7,
        _ => 6,
    }
}

/// Icosfera subdividida, estirada como un cuerpo (elipsoide). Determinista.
fn icosphere(sub: usize) -> (Vec<Vector3>, Vec<[usize; 3]>) {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let mut v: Vec<[f64; 3]> = vec![
        [-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0],
        [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t],
        [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0],
    ];
    let norm = |p: [f64; 3]| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        [p[0] / l, p[1] / l, p[2] / l]
    };
    v.iter_mut().for_each(|p| *p = norm(*p));
    let mut f: Vec<[usize; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..sub {
        let mut cache: HashMap<(usize, usize), usize> = HashMap::new();
        let mut mid = |a: usize, b: usize, v: &mut Vec<[f64; 3]>| {
            let key = (a.min(b), a.max(b));
            *cache.entry(key).or_insert_with(|| {
                let m = norm([
                    (v[a][0] + v[b][0]) / 2.0,
                    (v[a][1] + v[b][1]) / 2.0,
                    (v[a][2] + v[b][2]) / 2.0,
                ]);
                v.push(m);
                v.len() - 1
            })
        };
        let mut nf = Vec::with_capacity(f.len() * 4);
        for &[a, b, c] in &f {
            let ab = mid(a, b, &mut v);
            let bc = mid(b, c, &mut v);
            let ca = mid(c, a, &mut v);
            nf.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        f = nf;
    }
    let pos = v.iter().map(|p| Vector3::new(p[0] * 0.6, p[1], p[2] * 0.5)).collect();
    (pos, f)
}

/// LCG determinista en [0, 1).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// Calentamiento + mediana de 3 repeticiones (1 si una pasada pasa de ~20 s).
fn bench<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let t0 = Instant::now();
    let mut last = f(); // calentamiento
    let warm = t0.elapsed().as_secs_f64() * 1000.0;
    if warm > 20_000.0 {
        return (warm, last);
    }
    let mut times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        last = f();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times[1], last)
}

/// Esqueleto sintetico tipo arbol de `n` huesos dentro del elipsoide de la
/// icosfera (x en +-0.6, y en +-1, z en +-0.5): columna central y pares de
/// extremidades que salen hacia los lados.
fn synthetic_skeleton(n: usize) -> BasicSkeleton {
    let mut s = BasicSkeleton::new();
    let spine = (n / 4).max(2);
    let mut prev = s.add_bone(Bone::new("root", Vector3::new(0.0, -0.7, 0.0)));
    let mut ids = vec![prev];
    for k in 1..spine {
        let y = -0.7 + 1.4 * k as f64 / (spine - 1) as f64;
        prev = s.add_bone(Bone::with_parent(format!("spine{k}"), Vector3::new(0.0, y, 0.0), prev));
        ids.push(prev);
    }
    let mut r = Rng(7);
    let mut k = 0;
    'outer: loop {
        let anchor = ids[1 + k % (ids.len() - 1)];
        let base = s.bones()[anchor].position;
        for sign in [1.0, -1.0] {
            let mut parent = anchor;
            let dz = (r.next() - 0.5) * 0.3;
            for j in 1..=3 {
                if s.num_bones() >= n {
                    break 'outer;
                }
                let t = j as f64 / 3.0;
                let p = Vector3::new(sign * 0.35 * t, base.y() - 0.12 * t, dz * t);
                parent = s.add_bone(Bone::with_parent(format!("limb{k}_{j}"), p, parent));
            }
        }
        k += 1;
    }
    s
}

fn report(bench: &str, threads: usize, tris: usize, ms: f64) {
    println!("PERF crate={CRATE} bench={bench} threads={threads} tris={tris} ms={ms:.3}");
}

fn field_resolution() -> usize {
    match scale() {
        "small" => 20,
        "large" => 96,
        _ => 64,
    }
}

fn bones() -> usize {
    match scale() {
        "small" => 12,
        "large" => 120,
        _ => 40,
    }
}

/// Esferas mediales sinteticas (dentro del elipsoide) para grafo y empaquetado.
fn random_spheres(n: usize) -> Vec<MedialSphere> {
    let mut r = Rng(11);
    (0..n)
        .map(|_| {
            let c = Vector3::new((r.next() - 0.5) * 1.2, (r.next() - 0.5) * 2.0, (r.next() - 0.5) * 1.0);
            MedialSphere::new(c, 0.02 + 0.2 * r.next())
        })
        .collect()
}

fn spheres_count() -> usize {
    match scale() {
        "small" => 300,
        "large" => 8_000,
        _ => 3_000,
    }
}

#[test]
#[ignore = "perf"]
fn perf_sphere_graph_knn() {
    let spheres = random_spheres(spheres_count());
    let (ms, g) = bench(|| SphereGraph::from_spheres_knn(&spheres, 8));
    assert_eq!(g.num_spheres(), spheres.len());
    assert!(g.num_edges() > 0);
    assert!(g.edges().iter().all(|e| e.2.is_finite() && e.2 >= 0.0));
    report("sphere_graph_knn_k8", 1, spheres.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_pack_spheres() {
    let spheres = random_spheres(spheres_count());
    let keep = spheres.len() / 10;
    let (ms, packed) = bench(|| pack_spheres(&spheres, keep));
    assert!(!packed.is_empty() && packed.len() <= keep);
    assert!(packed.iter().all(|s| s.radius.is_finite() && s.radius > 0.0));
    report("pack_spheres", 1, spheres.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_medial_spheres_from_field() {
    // Solo mide la extraccion de esferas: el campo se calcula fuera del cronometro
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let r = field_resolution();
    let field = DistanceField::from_mesh_signed(&mesh, [r, r, r], 0.1);
    let (ms, spheres) = bench(|| medial_spheres_from_field(&field));
    assert!(!spheres.is_empty());
    assert!(spheres.iter().all(|s| s.radius.is_finite() && s.center.x().is_finite()));
    report("medial_spheres_from_field", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_full_embedding_pipeline() {
    // Campo con signo + esferas mediales + embebido por cadenas
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let skel = synthetic_skeleton(bones());
    let r = field_resolution();
    let (ms, res) = bench(|| full_embedding_pipeline(&mesh, &skel, [r, r, r]).expect("embedding"));
    assert_eq!(res.bone_positions.len(), skel.num_bones());
    assert!(res.bone_positions.iter().all(|p| p.x().is_finite() && p.y().is_finite() && p.z().is_finite()));
    report(&format!("full_embedding_pipeline_{}bones", skel.num_bones()), rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_fit_skeleton() {
    // Ajuste de plantilla (extremidades + hungaro + colocacion)
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let template = synthetic_skeleton(bones());
    let resolution = match scale() {
        "small" => 24,
        "large" => 128,
        _ => 80,
    };
    let opts = FitOptions { resolution, ..FitOptions::default() };
    let (ms, fit) = bench(|| fit_skeleton(&mesh, &template, &opts).expect("fit_skeleton"));
    assert_eq!(fit.skeleton.num_bones(), template.num_bones());
    assert!(fit.skeleton.bones().iter().all(|b| b.position.x().is_finite() && b.position.y().is_finite()));
    report(&format!("fit_skeleton_{}bones", template.num_bones()), rayon::current_num_threads(), idx.len(), ms);
}
