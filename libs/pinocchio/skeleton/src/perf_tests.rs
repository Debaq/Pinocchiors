//! Benchmarks de rendimiento de pinocchio-skeleton (consultas del arbol de
//! huesos, simetria, encaje a caja, JSON).
//!
//! Nota: el ajuste pesado del esqueleto a la malla (`fit_skeleton`, embebido)
//! vive en pinocchio-embedding; aqui solo se mide lo que hace este crate, que
//! es barato salvo con esqueletos enormes.
//!
//! Todos son `#[ignore = "perf"]`; no corren con `cargo test` normal. Comando
//! (el perfil release con LTO tarda muchisimo, por eso se desactiva LTO):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-skeleton perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `PERF_SCALE=small` usa 20 huesos (humo); `PERF_SCALE=large` 4000 huesos;
//!   sin variable, 120 huesos (el maximo que usa la app) cada muestra repite la operacion 200 veces
//!   (20 en el humo, 1 en large) para que el tiempo sea medible.
//! - Este crate no usa rayon: threads=1. En la salida `tris` = numero de huesos.
//! - Calentamiento + mediana de 3 repeticiones (1 si la operacion pasa de ~20 s).

use crate::{
    fit_to_bounds, load_skeleton_json, mirror_pairs, save_skeleton_json, skeleton_bounds, symmetry_plane,
    BasicSkeleton, Bone, Skeleton,
};
use pinocchio_math::{Rect, Vector3};
use std::time::Instant;

const CRATE: &str = "skeleton";

fn bones() -> (usize, usize) {
    // (huesos, repeticiones por muestra)
    match std::env::var("PERF_SCALE").as_deref() {
        Ok("small") => (20, 2),
        Ok("large") => (4000, 1),
        _ => (120, 200),
    }
}

/// Esqueleto sintetico: columna central y pares de extremidades de 3 huesos
/// (`limb{k}_{j}_l` / `_r`) colgando de las articulaciones de la columna.
fn synthetic_skeleton(n: usize) -> BasicSkeleton {
    let mut s = BasicSkeleton::new();
    let spine = (n / 7).max(2);
    let mut prev = s.add_bone(Bone::new("root", Vector3::new(0.0, 0.0, 0.0)));
    let mut spine_ids = vec![prev];
    for k in 1..spine {
        prev = s.add_bone(Bone::with_parent(format!("spine{k}"), Vector3::new(0.0, k as f64 * 0.1, 0.0), prev));
        spine_ids.push(prev);
    }
    let mut k = 0;
    'outer: loop {
        let anchor = spine_ids[k % spine_ids.len()];
        let base = s.bones()[anchor].position;
        for (suffix, sign) in [("_l", 1.0), ("_r", -1.0)] {
            let mut parent = anchor;
            for j in 0..3 {
                if s.num_bones() >= n {
                    break 'outer;
                }
                let p = base + Vector3::new(sign * (0.1 + 0.1 * j as f64), -0.05 * j as f64, 0.01 * (k / spine_ids.len()) as f64);
                parent = s.add_bone(Bone::with_parent(format!("limb{k}_{j}{suffix}"), p, parent));
            }
        }
        k += 1;
    }
    s
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

#[test]
#[ignore = "perf"]
fn perf_tree_queries() {
    let (n, reps) = bones();
    let s = synthetic_skeleton(n);
    assert_eq!(s.num_bones(), n);
    // get_children / get_depth recorren todos los huesos por llamada (O(n))
    let (ms, total) = bench(|| {
        let mut total = 0usize;
        for _ in 0..reps {
            for i in 0..s.num_bones() {
                total += s.get_children(i).len() + s.get_depth(i);
            }
        }
        total
    });
    assert!(total >= (n - 1) * reps);
    report("tree_children_depth", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_mirror_pairs() {
    let (n, reps) = bones();
    let s = synthetic_skeleton(n);
    let (ms, pairs) = bench(|| {
        let mut p = Vec::new();
        for _ in 0..reps {
            p = mirror_pairs(&s);
        }
        p
    });
    assert_eq!(pairs.len(), n);
    assert!(pairs.iter().flatten().count() > 0);
    // Cada par es mutuo
    for (i, p) in pairs.iter().enumerate() {
        if let Some(j) = p {
            assert_eq!(pairs[*j], Some(i));
        }
    }
    assert!(symmetry_plane(&s).is_some());
    report("mirror_pairs", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_fit_to_bounds() {
    let (n, reps) = bones();
    let s = synthetic_skeleton(n);
    let target = Rect::new(Vector3::new(-1.0, 0.0, -1.0), Vector3::new(1.0, 2.0, 1.0));
    let (ms, fitted) = bench(|| {
        let mut f = BasicSkeleton::new();
        for _ in 0..reps {
            f = fit_to_bounds(&s, &target, 0.9);
        }
        f
    });
    let b = skeleton_bounds(&fitted).unwrap();
    assert!(b.size().y().is_finite() && b.size().y() > 0.0);
    assert!(fitted.bones().iter().all(|b| b.position.x().is_finite()));
    report("fit_to_bounds", n, ms);
}

#[test]
#[ignore = "perf"]
fn perf_json_roundtrip() {
    let (n, reps) = bones();
    let s = synthetic_skeleton(n);
    let (ms, back) = bench(|| {
        let mut b = BasicSkeleton::new();
        for _ in 0..reps {
            let json = save_skeleton_json(&s, "perf");
            b = load_skeleton_json(&json).unwrap();
        }
        b
    });
    assert_eq!(back.num_bones(), n);
    report("json_roundtrip", n, ms);
}
