//! Benchmarks de rendimiento de pinocchio-sparse (matrices SPD y solvers).
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-sparse perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa entradas diminutas (humo); `PERF_SCALE=large` una
//!   rejilla de 64^3; sin variable, 50^3 (125k incognitas, como una malla de
//!   ~250k triangulos).
//! - Este crate no usa rayon: todo es secuencial (threads=1). La paralelizacion
//!   (un sistema por hueso) vive en pinocchio-attachment.
//! - Cada benchmark imprime `PERF crate=.. bench=.. threads=.. tris=.. ms=<mediana>`;
//!   aqui `tris` es el numero de incognitas del sistema.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::SPDMatrix;
use pinocchio_math::Real;
use std::time::Instant;

const CRATE: &str = "sparse";

fn side() -> usize {
    match std::env::var("PERF_SCALE").as_deref() {
        Ok("small") => 12,
        Ok("large") => 64,
        _ => 50,
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

fn report(bench: &str, n: usize, ms: f64) {
    println!("PERF crate={CRATE} bench={bench} threads=1 tris={n} ms={ms:.3}");
}

/// Triplets de un Laplaciano de rejilla 3D (grafo con grado <= 6) mas una
/// diagonal "de calor" variable y positiva, como `L + A·H` en heat diffusion.
fn grid_triplets(s: usize) -> (usize, Vec<usize>, Vec<usize>, Vec<Real>) {
    let n = s * s * s;
    let id = |x: usize, y: usize, z: usize| (z * s + y) * s + x;
    let (mut r, mut c, mut v) = (Vec::new(), Vec::new(), Vec::new());
    for z in 0..s {
        for y in 0..s {
            for x in 0..s {
                let i = id(x, y, z);
                let mut deg = 0.0;
                let mut link = |j: usize| {
                    r.push(i);
                    c.push(j);
                    v.push(-1.0);
                    deg += 1.0;
                };
                if x > 0 { link(id(x - 1, y, z)); }
                if x + 1 < s { link(id(x + 1, y, z)); }
                if y > 0 { link(id(x, y - 1, z)); }
                if y + 1 < s { link(id(x, y + 1, z)); }
                if z > 0 { link(id(x, y, z - 1)); }
                if z + 1 < s { link(id(x, y, z + 1)); }
                let heat = 1e-3 + 0.5 * (((i * 2654435761) % 1000) as Real / 1000.0);
                r.push(i);
                c.push(i);
                v.push(deg + heat);
            }
        }
    }
    (n, r, c, v)
}

fn rhs(n: usize) -> Vec<Real> {
    (0..n).map(|i| ((i * 40503) % 997) as Real / 997.0).collect()
}

fn residual(a: &SPDMatrix, x: &[Real], b: &[Real]) -> Real {
    let ax = a.mul_vec(x);
    let num: Real = ax.iter().zip(b).map(|(p, q)| (p - q).powi(2)).sum::<Real>().sqrt();
    let den: Real = b.iter().map(|q| q * q).sum::<Real>().sqrt();
    num / den
}

#[test]
#[ignore = "perf"]
fn perf_spd_from_triplets() {
    let (n, r, c, v) = grid_triplets(side());
    let (ms, m) = bench(|| SPDMatrix::from_triplets(n, &r, &c, &v).unwrap());
    assert_eq!(m.size(), n);
    assert!(m.nnz() >= n);
    report("spd_from_triplets", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_spd_mul_vec() {
    let (n, r, c, v) = grid_triplets(side());
    let m = SPDMatrix::from_triplets(n, &r, &c, &v).unwrap();
    let x = rhs(n);
    // 20 productos por muestra: una sola pasada dura microsegundos
    let (ms, y) = bench(|| {
        let mut y = Vec::new();
        for _ in 0..20 {
            y = m.mul_vec(&x);
        }
        y
    });
    assert_eq!(y.len(), n);
    assert!(y.iter().all(|t| t.is_finite()));
    report("spd_mul_vec_x20", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_spd_add_diagonal() {
    let (n, r, c, v) = grid_triplets(side());
    let m = SPDMatrix::from_triplets(n, &r, &c, &v).unwrap();
    let d: Vec<Real> = (0..n).map(|i| 0.1 + (i % 7) as Real).collect();
    let (ms, m2) = bench(|| m.add_diagonal(&d).unwrap());
    assert_eq!(m2.size(), n);
    report("spd_add_diagonal", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_spd_solve_cg() {
    let (n, r, c, v) = grid_triplets(side());
    let m = SPDMatrix::from_triplets(n, &r, &c, &v).unwrap();
    let b = rhs(n);
    // Mismos parametros que heat diffusion (5000 iteraciones, 1e-9)
    let (ms, x) = bench(|| m.solve_cg(&b, 5000, 1e-9).unwrap());
    let res = residual(&m, &x, &b);
    assert!(res < 1e-6, "residuo relativo {res}");
    report("spd_solve_cg", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_spd_solve_gauss_seidel() {
    let (n, r, c, v) = grid_triplets(side());
    let m = SPDMatrix::from_triplets(n, &r, &c, &v).unwrap();
    let b = rhs(n);
    // Iteraciones acotadas: GS converge lento; se mide el costo por pasada
    let (ms, x) = bench(|| m.solve_gauss_seidel(&b, 100, 0.0).unwrap());
    assert!(x.iter().all(|t| t.is_finite()));
    report("spd_solve_gauss_seidel_100it", n, ms);
}
