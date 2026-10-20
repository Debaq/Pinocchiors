//! Benchmarks de rendimiento de orizon3d-core con datos sinteticos (sin camara
//! ni hardware): alineacion de nubes (ICP), fusion, comparacion, limpieza y
//! reconstruccion de malla (MLS/marching y Poisson) desde una nube de puntos.
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p orizon3d-core perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa nubes diminutas (humo); `PERF_SCALE=large` ~320k
//!   puntos (alineacion: 80k); sin variable, 100k puntos (alineacion: 20k).
//! - Nube: elipsoide con bultos (~80x110x60 mm) con ruido de 0,1 mm,
//!   determinista. En la salida `tris` = numero de puntos de la nube principal.
//! - Estas funciones no usan rayon (el rayon de orizon3d es del render del
//!   plato giratorio): threads=1.
//! - Cada benchmark imprime `PERF crate=.. bench=.. threads=.. tris=.. ms=<mediana>`.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::edit;
use crate::mesh::reconstruct;
use crate::poisson::reconstruct_poisson;
use crate::pointcloud::{Point, PointCloud};
use crate::register::{align, distances, merge, summarize, transform_cloud, AlignMode};
use crate::scan::{clean_cloud, Transform};
use std::time::Instant;

const CRATE: &str = "orizon3d-core";

fn scale() -> &'static str {
    match std::env::var("PERF_SCALE").as_deref() {
        Ok("small") => "small",
        Ok("large") => "large",
        _ => "default",
    }
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

fn report(bench: &str, tris: usize, ms: f64) {
    println!("PERF crate={CRATE} bench={bench} threads=1 tris={tris} ms={ms:.3}");
}

/// Elipsoide con bultos muestreado en rejilla (nu x nv) con ruido de `noise` mm.
fn lumpy_cloud(nu: usize, nv: usize, noise: f32, seed: u64) -> PointCloud {
    let mut rng = Rng(seed);
    let mut points = Vec::with_capacity(nu * nv);
    for i in 0..nu {
        for j in 1..nv {
            let th = i as f32 / nu as f32 * std::f32::consts::TAU;
            let ph = j as f32 / nv as f32 * std::f32::consts::PI;
            let bump = 1.0 + 0.12 * (3.0 * th).sin() * (2.0 * ph).cos() + 0.08 * (5.0 * ph + th).sin();
            let (x, y, z) = (40.0 * ph.sin() * th.cos(), 55.0 * ph.cos(), 30.0 * ph.sin() * th.sin());
            let mut n = || (rng.next() as f32 - 0.5) * 2.0 * noise;
            points.push(Point {
                x: x * bump + n(),
                y: y * bump + n(),
                z: z * bump + n(),
                rgb: [200; 3],
                view: [0.0; 3],
            });
        }
    }
    PointCloud { points, has_color: true }
}

/// Nube principal (reconstruccion, limpieza, comparacion).
fn main_cloud() -> PointCloud {
    let (nu, nv) = match scale() {
        "small" => (40, 25),
        "large" => (800, 400),
        _ => (400, 250),
    };
    lumpy_cloud(nu, nv, 0.1, 1)
}

/// Nube para alinear (el ICP es mas caro que el resto).
fn align_cloud(seed: u64) -> PointCloud {
    let (nu, nv) = match scale() {
        "small" => (30, 20),
        "large" => (400, 200),
        _ => (200, 100),
    };
    lumpy_cloud(nu, nv, 0.1, seed)
}

fn voxel() -> f32 {
    if scale() == "small" { 3.0 } else { 1.0 }
}

/// Rotacion de `deg` grados sobre Y y traslacion `t`.
fn rot_y(deg: f32, t: [f32; 3]) -> Transform {
    let (s, c) = deg.to_radians().sin_cos();
    Transform { r: [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c], t }
}

fn assert_mesh_sane(m: &crate::mesh::Mesh) {
    assert!(!m.is_empty(), "malla vacia");
    assert!(m.vertices.iter().all(|v| v.iter().all(|c| c.is_finite())));
    let n = m.vertices.len() as u32;
    assert!(m.tris.iter().all(|t| t.iter().all(|&i| i < n)));
}

#[test]
#[ignore = "perf"]
fn perf_align_fine() {
    let target = align_cloud(1);
    // Misma superficie con otro ruido, girada 6 grados y corrida
    let moved = transform_cloud(&align_cloud(2), &rot_y(6.0, [3.0, -2.0, 4.0]));
    let (ms, a) = bench(|| align(&moved, &target, AlignMode::Fine).expect("alineacion"));
    assert!(a.overlap > 0.8, "solape {}", a.overlap);
    assert!(a.rmse.is_finite() && a.rmse < 2.0, "rmse {}", a.rmse);
    report("align_fine", target.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_align_auto() {
    let target = align_cloud(1);
    // Girada 70 grados y corrida lejos: exige la busqueda global
    let moved = transform_cloud(&align_cloud(2), &rot_y(70.0, [120.0, -40.0, 300.0]));
    let (ms, a) = bench(|| align(&moved, &target, AlignMode::Auto).expect("alineacion"));
    assert!(a.overlap > 0.5, "solape {}", a.overlap);
    assert!(a.rmse.is_finite());
    report("align_auto", target.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_merge_fuse() {
    let a = main_cloud();
    let b = transform_cloud(&main_cloud(), &rot_y(1.0, [0.5, 0.0, 0.0]));
    let (ms, m) = bench(|| merge(&a, &b, voxel() * 0.5));
    assert!(!m.points.is_empty() && m.points.len() < a.points.len() + b.points.len());
    assert!(m.points.iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()));
    report("merge_fuse", a.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_downsample() {
    let c = main_cloud();
    let (ms, d) = bench(|| edit::downsample(&c, voxel()));
    assert!(!d.points.is_empty() && d.points.len() < c.points.len());
    report("downsample", c.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_clean_cloud() {
    let c = main_cloud();
    let (ms, d) = bench(|| clean_cloud(&c, voxel(), 3, true));
    assert!(!d.points.is_empty() && d.points.len() <= c.points.len());
    report("clean_cloud", c.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_distances() {
    let reference = main_cloud();
    let probe = transform_cloud(&align_cloud(5), &rot_y(0.5, [0.2, 0.0, 0.0]));
    let (ms, d) = bench(|| distances(&probe, &reference, 5.0));
    assert_eq!(d.len(), probe.points.len());
    let dev = summarize(&d, 1.0);
    assert!(dev.compared > 0 && dev.rms.is_finite());
    report("distances", reference.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_reconstruct_mls() {
    let c = main_cloud();
    let (ms, m) = bench(|| reconstruct(&c, voxel(), 1, 1));
    assert_mesh_sane(&m);
    println!("  (malla resultante: {} triangulos)", m.tris.len());
    report("reconstruct_mls", c.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_reconstruct_poisson() {
    let c = main_cloud();
    let (ms, m) = bench(|| reconstruct_poisson(&c, voxel(), 1, 1));
    assert_mesh_sane(&m);
    println!("  (malla resultante: {} triangulos)", m.tris.len());
    report("reconstruct_poisson", c.points.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_mesh_decimate() {
    let c = main_cloud();
    let m = reconstruct(&c, voxel(), 1, 1);
    assert_mesh_sane(&m);
    let (ms, d) = bench(|| m.decimate(voxel() * 3.0));
    assert_mesh_sane(&d);
    assert!(d.tris.len() < m.tris.len());
    println!("  (decimada de {} a {} triangulos)", m.tris.len(), d.tris.len());
    report("mesh_decimate", c.points.len(), ms);
}
