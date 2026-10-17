//! Benchmarks de rendimiento de pinocchio-attachment (pesos de piel por heat
//! diffusion y visibilidad por ray casting).
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-attachment perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa entradas diminutas (humo); `PERF_SCALE=large` una
//!   icosfera de ~330k triangulos; sin variable, ~82k triangulos (41k vertices).
//! - Hilos de rayon: `RAYON_NUM_THREADS=<n>` para medir escalado. compute_weights
//!   resuelve un sistema disperso por hueso con `into_par_iter`, asi que el
//!   escalado depende del numero de huesos.
//! - Cada benchmark imprime `PERF crate=.. bench=.. threads=.. tris=.. ms=<mediana>`.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::{HeatDiffusion, VisibilityTester};
use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::{BasicSkeleton, Bone, Skeleton};
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

const CRATE: &str = "attachment";

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

fn bone_counts() -> (usize, usize) {
    match scale() {
        "small" => (6, 12),
        _ => (20, 120),
    }
}

fn check_weights(w: &[Vec<Real>], verts: usize, bones: usize) {
    assert_eq!(w.len(), verts);
    for row in w {
        assert_eq!(row.len(), bones);
        assert!(row.iter().all(|x| x.is_finite() && *x >= -1e-9));
        let sum: Real = row.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "los pesos suman {sum}");
    }
}

fn heat_bench(name: &str, bones: usize) {
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let skel = synthetic_skeleton(bones);
    let hd = HeatDiffusion::new(&mesh);
    let (ms, w) = bench(|| hd.compute_weights(&skel).expect("compute_weights"));
    check_weights(&w, pos.len(), bones);
    report(&format!("{name}_{bones}bones"), rayon::current_num_threads(), idx.len(), ms);
}

#[test]
#[ignore = "perf"]
fn perf_heat_weights_few_bones() {
    heat_bench("heat_compute_weights", bone_counts().0);
}

#[test]
#[ignore = "perf"]
fn perf_heat_weights_many_bones() {
    heat_bench("heat_compute_weights", bone_counts().1);
}

#[test]
#[ignore = "perf"]
fn perf_visibility_is_visible_par() {
    // is_visible recorre todos los triangulos por consulta (fuerza bruta)
    let (pos, idx) = icosphere(subdivisions());
    let mesh = Mesh::from_triangles(&pos, &idx);
    let tester = VisibilityTester::new(&mesh);
    let n = if scale() == "small" { 200 } else { 2_000 };
    let mut r = Rng(3);
    let pairs: Vec<(Vector3, Vector3)> = (0..n)
        .map(|_| {
            let a = Vector3::new((r.next() - 0.5) * 0.4, (r.next() - 0.5) * 1.2, (r.next() - 0.5) * 0.4);
            let b = Vector3::new((r.next() - 0.5) * 2.4, (r.next() - 0.5) * 3.0, (r.next() - 0.5) * 2.0);
            (a, b)
        })
        .collect();
    let (ms, visible) = bench(|| pairs.par_iter().filter(|(a, b)| tester.is_visible(a, b)).count());
    // Desde dentro, los que acaban fuera cruzan la superficie
    assert!(visible < n);
    report("visibility_is_visible_par", rayon::current_num_threads(), idx.len(), ms);
}
