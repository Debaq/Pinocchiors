//! Benchmarks de rendimiento de pinocchio-spatial (BVH, campo de distancias, octree).
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-spatial perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa entradas diminutas (humo); `PERF_SCALE=large` una
//!   icosfera de ~330k triangulos; sin variable, ~82k triangulos.
//! - Hilos de rayon: `RAYON_NUM_THREADS=<n>` para medir escalado.
//! - Cada benchmark imprime `PERF crate=.. bench=.. threads=.. tris=.. ms=<mediana>`.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::{Bvh, DistanceField, Octree, Triangle};
use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

const CRATE: &str = "spatial";

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

fn report(bench: &str, threads: usize, tris: usize, ms: f64) {
    println!("PERF crate={CRATE} bench={bench} threads={threads} tris={tris} ms={ms:.3}");
}

fn triangles(pos: &[Vector3], idx: &[[usize; 3]]) -> Vec<Triangle> {
    idx.iter().map(|t| Triangle::new(pos[t[0]], pos[t[1]], pos[t[2]])).collect()
}

/// Puntos de consulta deterministas alrededor de la malla (dentro y fuera).
fn query_points(n: usize) -> Vec<Vector3> {
    let mut r = Rng(42);
    (0..n)
        .map(|_| Vector3::new((r.next() - 0.5) * 2.0, (r.next() - 0.5) * 2.4, (r.next() - 0.5) * 2.0))
        .collect()
}

fn queries() -> usize {
    if scale() == "small" { 2_000 } else { 200_000 }
}

#[test]
#[ignore = "perf"]
fn perf_bvh_build() {
    let (pos, idx) = icosphere(subdivisions());
    let (ms, bvh) = bench(|| Bvh::build(triangles(&pos, &idx)));
    assert_eq!(bvh.num_triangles(), idx.len());
    report("bvh_build", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_bvh_query_closest_seq() {
    let (pos, idx) = icosphere(subdivisions());
    let bvh = Bvh::build(triangles(&pos, &idx));
    let pts = query_points(queries());
    let (ms, sum) = bench(|| pts.iter().map(|p| bvh.query_closest(p).unwrap().distance).sum::<Real>());
    assert!(sum.is_finite() && sum >= 0.0);
    report("bvh_query_closest_seq", 1, idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_bvh_query_closest_par() {
    let (pos, idx) = icosphere(subdivisions());
    let bvh = Bvh::build(triangles(&pos, &idx));
    let pts = query_points(queries());
    let (ms, sum) = bench(|| pts.par_iter().map(|p| bvh.query_closest(p).unwrap().distance).sum::<Real>());
    assert!(sum.is_finite() && sum >= 0.0);
    report("bvh_query_closest_par", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_bvh_query_distance_par() {
    let (pos, idx) = icosphere(subdivisions());
    let bvh = Bvh::build(triangles(&pos, &idx));
    let pts = query_points(queries());
    let (ms, sum) = bench(|| pts.par_iter().map(|p| bvh.query_distance(p)).sum::<Real>());
    assert!(sum.is_finite() && sum >= 0.0);
    report("bvh_query_distance_par", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_bvh_ray_hit_par() {
    let (pos, idx) = icosphere(subdivisions());
    let bvh = Bvh::build(triangles(&pos, &idx));
    let pts = query_points(queries());
    let dir = Vector3::new(0.0, 0.0, 1.0);
    // Rayos desde z = -3 hacia +z: los que pasan por el elipsoide golpean
    let (ms, hits) = bench(|| {
        pts.par_iter()
            .filter(|p| bvh.ray_hit(&Vector3::new(p.x(), p.y(), -3.0), &dir, 0.0, 10.0).is_some())
            .count()
    });
    assert!(hits > 0 && hits < pts.len());
    report("bvh_ray_hit_par", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_bvh_segment_intersects_par() {
    let (pos, idx) = icosphere(subdivisions());
    let bvh = Bvh::build(triangles(&pos, &idx));
    let pts = query_points(queries());
    let origin = Vector3::new(0.0, 0.0, 0.0);
    // Visibilidad desde el centro: el tipo de consulta que hace heat diffusion
    let (ms, blocked) = bench(|| pts.par_iter().filter(|p| bvh.segment_intersects(&origin, p, 1e-3)).count());
    assert!(blocked <= pts.len());
    report("bvh_segment_intersects_par", rayon::current_num_threads(), idx.len(), ms);
}

fn field_resolution() -> usize {
    match scale() {
        "small" => 16,
        "large" => 96,
        _ => 64,
    }
}

#[test]
#[ignore = "perf"]
fn perf_distance_field_bvh() {
    // `from_mesh` usa este camino con más de 1000 caras; `from_mesh_parallel` es fuerza bruta
    // (solo para mallas diminutas) y con cientos de miles de caras no termina en minutos.
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let r = field_resolution();
    let (ms, field) = bench(|| DistanceField::from_mesh_bvh(&mesh, [r, r, r], 0.1));
    assert!(field.values().iter().all(|v| !v.is_nan()));
    report("distance_field_bvh", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_distance_field_signed() {
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let r = field_resolution();
    let (ms, field) = bench(|| DistanceField::from_mesh_signed(&mesh, [r, r, r], 0.1));
    assert!(field.values().iter().all(|v| !v.is_nan()));
    assert!(field.is_signed());
    report("distance_field_signed", rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_octree_insert_nearest() {
    // El octree no lo usa la app (solo se exporta) y su vecino más cercano es casi lineal: 20k bastan
    let n = if scale() == "small" { 2_000 } else { 20_000 };
    let pts = query_points(n);
    let (ms_ins, tree) = bench(|| {
        let mut t = Octree::new(10, 16);
        for (i, p) in pts.iter().enumerate() {
            t.insert(*p, i);
        }
        t
    });
    assert_eq!(tree.len(), n);
    report("octree_insert", 1, n, ms_ins);

    let probes = query_points(n.min(20_000));
    let (ms_nn, found) = bench(|| probes.iter().filter(|q| tree.nearest_neighbor(q).is_some()).count());
    assert_eq!(found, probes.len());
    report("octree_nearest_neighbor", 1, n, ms_nn);

    let (ms_r, total) = bench(|| probes.iter().take(2_000).map(|q| tree.query_radius(q, 0.05).len()).sum::<usize>());
    assert!(total > 0);
    report("octree_query_radius", 1, n, ms_r);
}
