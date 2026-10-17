//! Benchmarks de rendimiento de pinocchio-mesh (half-edge y decimación).
//!
//! No son pruebas de corrección: miden tiempos y los imprimen con el formato
//! fijo `PERF crate=<crate> bench=<nombre> threads=<n> tris=<n> ms=<mediana>`.
//! Cada uno verifica además que el resultado sea sensato (assert barato).
//!
//! Ejecutar (sin LTO: el perfil release del repo tarda muchísimo en enlazar):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-mesh --lib perf_tests \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `--test-threads=1`: los benchmarks no deben competir entre sí.
//! - `PERF_SCALE=small`: tamaño diminuto, solo para humo. Por defecto: ~300k
//!   triángulos.
//! - Escalado con hilos: correr con `RAYON_NUM_THREADS=1` y con el valor por
//!   defecto (este crate usa rayon en `Mesh::from_triangles`).
//! - Medición: 1 calentamiento + mediana de 3. Si el calentamiento tarda más de
//!   20 s se informa ese único tiempo (1 repetición).

use crate::{decimate, decimate_with_resolution, Mesh};
use pinocchio_math::Vector3;
use std::time::Instant;

const CRATE: &str = "pinocchio-mesh";

fn small() -> bool {
    std::env::var("PERF_SCALE").is_ok_and(|s| s == "small")
}

/// Calentamiento + mediana de 3 (1 sola si el calentamiento pasa de 20 s).
fn bench<T>(name: &str, tris: usize, mut f: impl FnMut() -> T) -> T {
    let t0 = Instant::now();
    let mut last = f();
    let warm = t0.elapsed().as_secs_f64() * 1e3;
    let ms = if warm > 20_000.0 {
        warm
    } else {
        let mut times = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            last = f();
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times[1]
    };
    println!("PERF crate={CRATE} bench={name} threads={} tris={tris} ms={ms:.2}", rayon::current_num_threads());
    last
}

/// Esfera UV con relieve y la costura de UV duplicada (la última columna de
/// vértices repite la primera, como en un glTF). Devuelve posiciones,
/// triángulos y la cantidad de vértices que tendría soldada.
fn seamed_sphere(rings: usize, segments: usize) -> (Vec<Vector3>, Vec<[usize; 3]>, usize) {
    let mut p = vec![Vector3::new(0.0, 1.0, 0.0)];
    for r in 1..rings {
        let phi = std::f64::consts::PI * r as f64 / rings as f64;
        for s in 0..=segments {
            let theta = std::f64::consts::TAU * s as f64 / segments as f64;
            let k = 1.0 + 0.05 * (5.0 * theta).sin() * phi.sin() + 0.02 * (13.0 * phi).cos() * (7.0 * theta).sin();
            p.push(Vector3::new(k * phi.sin() * theta.cos(), k * phi.cos(), k * phi.sin() * theta.sin()));
        }
    }
    p.push(Vector3::new(0.0, -1.0, 0.0));
    let bottom = p.len() - 1;
    let ring = |r: usize, s: usize| 1 + (r - 1) * (segments + 1) + s;
    let mut t = Vec::new();
    for s in 0..segments {
        t.push([0, ring(1, s + 1), ring(1, s)]);
        t.push([bottom, ring(rings - 1, s), ring(rings - 1, s + 1)]);
        for r in 1..rings - 1 {
            let (a, b, c, d) = (ring(r, s), ring(r, s + 1), ring(r + 1, s), ring(r + 1, s + 1));
            t.push([a, b, d]);
            t.push([a, d, c]);
        }
    }
    (p, t, 2 + (rings - 1) * segments)
}

fn size() -> (usize, usize) {
    if small() { (24, 48) } else { (316, 474) }
}

#[test]
#[ignore = "perf"]
fn perf_from_triangles() {
    let (rings, segments) = size();
    let (p, t, _) = seamed_sphere(rings, segments);
    let mesh = bench("from_triangles", t.len(), || Mesh::from_triangles(&p, &t));
    assert_eq!(mesh.num_faces(), t.len());
    assert_eq!(mesh.num_vertices(), p.len());
}

#[test]
#[ignore = "perf"]
fn perf_welded() {
    let (rings, segments) = size();
    let (p, t, welded_vertices) = seamed_sphere(rings, segments);
    let mesh = Mesh::from_triangles(&p, &t);
    let tol = 1e-7;
    let (w, map) = bench("welded_seams", t.len(), || mesh.welded(tol));
    assert_eq!(w.num_vertices(), welded_vertices, "la costura debe fundirse");
    assert_eq!(map.len(), mesh.num_vertices());
    assert!(w.is_closed());
}

#[test]
#[ignore = "perf"]
fn perf_vertex_normals() {
    let (rings, segments) = size();
    let (p, t, _) = seamed_sphere(rings, segments);
    let base = Mesh::from_triangles(&p, &t);
    let m = bench("compute_vertex_normals", t.len(), || {
        let mut m = base.clone();
        m.compute_vertex_normals();
        m
    });
    // Las normales de una esfera apuntan hacia afuera: n·p > 0 en casi todos
    let outward = m.vertices.iter().filter(|v| v.normal.dot(&v.position) > 0.0).count();
    assert!(outward * 10 > m.num_vertices() * 9, "{outward} de {} hacia afuera", m.num_vertices());
}

#[test]
#[ignore = "perf"]
fn perf_decimate_ratio() {
    // Decimación por grilla (vertex clustering) al 10 % de los vértices
    let (rings, segments) = size();
    let (p, t, _) = seamed_sphere(rings, segments);
    let mesh = Mesh::from_triangles(&p, &t);
    let out = bench("decimate_10pct", t.len(), || decimate(&mesh, 0.1));
    assert!(out.num_faces() > 0 && out.num_faces() < mesh.num_faces() / 3, "{} → {}", mesh.num_faces(), out.num_faces());
}

#[test]
#[ignore = "perf"]
fn perf_decimate_resolution() {
    let (rings, segments) = size();
    let (p, t, _) = seamed_sphere(rings, segments);
    let mesh = Mesh::from_triangles(&p, &t);
    let res = if small() { 12 } else { 64 };
    let out = bench("decimate_resolution", t.len(), || decimate_with_resolution(&mesh, res));
    assert!(out.num_faces() > 0 && out.num_faces() < mesh.num_faces() / 2, "{} → {}", mesh.num_faces(), out.num_faces());
}
