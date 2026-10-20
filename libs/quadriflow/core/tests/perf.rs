//! Benchmarks de rendimiento de quadriflow-core (Remallar y retopología).
//!
//! No son pruebas de corrección: miden tiempos de las operaciones que usa la
//! app y los imprimen con el formato fijo
//! `PERF crate=<crate> bench=<nombre> threads=<n> tris=<n> ms=<mediana>`.
//! Cada uno verifica además que el resultado sea sensato (assert barato).
//!
//! Ejecutar (sin LTO: el perfil release del repo tarda muchísimo en enlazar):
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p quadriflow-core --features simplify --test perf \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! (`--features simplify` activa el benchmark de simplificar; sin él se omite.)
//!
//! - `--test-threads=1`: los benchmarks no deben competir entre sí.
//! - `PERF_SCALE=small`: tamaño diminuto, solo para humo. Por defecto: 40k–300k
//!   triángulos según la operación.
//! - Escalado con hilos: correr una vez con `RAYON_NUM_THREADS=1` y otra con el
//!   valor por defecto, y comparar el campo `threads=`/`ms=`.
//! - Medición: 1 calentamiento + mediana de 3. Si el calentamiento tarda más de
//!   20 s se informa ese único tiempo (1 repetición).

use pinocchio_math::Vector3;
use pinocchio_mesh::Mesh;
use quadriflow_core::remesh::{
    all_quads, isotropic, smooth, tris_to_quads, voxel, IsotropicOptions, QuadsInput, QuadsOptions, SmoothOptions,
    VoxelOptions,
};
use quadriflow_core::{remesh, RemeshConfig};
use std::collections::HashMap;
use std::time::Instant;

const CRATE: &str = "quadriflow-core";

fn small() -> bool {
    std::env::var("PERF_SCALE").is_ok_and(|s| s == "small")
}

/// Elige entre el tamaño de humo y el real
fn pick<T>(small_v: T, real_v: T) -> T {
    if small() { small_v } else { real_v }
}

/// Calentamiento + mediana de 3 (1 sola si el calentamiento pasa de 20 s).
/// Imprime la línea PERF y devuelve el resultado de la última corrida.
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
        // La primera corrida (fría) puede diferir de las siguientes por la memoria que ya tiene el proceso
        eprintln!("  ({name}: primera corrida {warm:.0} ms, siguientes {times:.0?} ms)");
        times[1]
    };
    println!("PERF crate={CRATE} bench={name} threads={} tris={tris} ms={ms:.2}", rayon::current_num_threads());
    last
}

/// Esfera UV soldada y cerrada, con relieve determinista (no es trivial para
/// simplificar ni para el isótropo). Tiene `2 * rings * segments - 2 * segments` triángulos.
fn sphere(rings: usize, segments: usize) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut p = vec![[0.0, 1.0, 0.0]];
    for r in 1..rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let k = 1.0 + 0.05 * (5.0 * theta).sin() * phi.sin() + 0.02 * (13.0 * phi).cos() * (7.0 * theta).sin();
            p.push([k * phi.sin() * theta.cos(), k * phi.cos(), k * phi.sin() * theta.sin()]);
        }
    }
    p.push([0.0, -1.0, 0.0]);
    let bottom = (p.len() - 1) as u32;
    let ring = |r: usize, s: usize| (1 + (r - 1) * segments + s % segments) as u32;
    let mut t = Vec::new();
    for s in 0..segments {
        t.extend([0, ring(1, s + 1), ring(1, s)]);
        t.extend([bottom, ring(rings - 1, s), ring(rings - 1, s + 1)]);
        for r in 1..rings - 1 {
            let (a, b, c, d) = (ring(r, s), ring(r, s + 1), ring(r + 1, s), ring(r + 1, s + 1));
            t.extend([a, b, d, a, d, c]);
        }
    }
    (p, t)
}

/// Cada arista en exactamente dos triángulos
fn is_closed(indices: &[u32]) -> bool {
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for t in indices.chunks(3) {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges.values().all(|&n| n == 2)
}

fn area(p: &[[f32; 3]], t: &[u32]) -> f64 {
    t.chunks(3)
        .map(|c| {
            let [a, b, d] = [c[0], c[1], c[2]].map(|i| p[i as usize].map(f64::from));
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [d[0] - a[0], d[1] - a[1], d[2] - a[2]]);
            let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
        })
        .sum()
}

/// Lado de triángulo equilátero para obtener ~`tris` triángulos en `area`
fn edge_for(area: f64, tris: usize) -> f64 {
    (area / (3f64.sqrt() / 4.0 * tris as f64)).sqrt()
}

#[cfg(feature = "simplify")]
#[test]
#[ignore = "perf"]
fn perf_simplify() {
    use quadriflow_core::remesh::{simplify, SimplifyInput, SimplifyOptions};
    let (p, t) = pick(sphere(24, 48), sphere(316, 474));
    let before = t.len() / 3;
    let target = before / 10;
    let input = SimplifyInput { positions: &p, indices: &t, attributes: &[], attribute_weights: &[], groups: &[] };
    let r = bench("simplify_10pct", before, || {
        simplify(&input, &SimplifyOptions { target_triangles: target, ..Default::default() })
    });
    let after = r.indices.len() / 3;
    assert!(after < before / 4, "{before} → {after}");
    assert!(is_closed(&r.indices));
}

#[test]
#[ignore = "perf"]
fn perf_smooth() {
    let (p, t) = pick(sphere(24, 48), sphere(316, 474));
    let opts = SmoothOptions { iterations: 10, sharp_angle: Some(45.0), ..Default::default() };
    let out = bench("smooth_10it", t.len() / 3, || smooth(&p, &t, &opts));
    assert_eq!(out.len(), p.len());
    assert!(out.iter().all(|v| v.iter().all(|c| c.is_finite())));
    // Suavizar no encoge la malla de forma notable
    let (a0, a1) = (area(&p, &t), area(&out, &t));
    assert!(a1 < a0 * 1.01 && a1 > a0 * 0.8, "área {a0} → {a1}");
}

#[test]
#[ignore = "perf"]
fn perf_isotropic() {
    let (p, t) = pick(sphere(24, 48), sphere(200, 300));
    let target = pick(600, 60_000);
    let edge = edge_for(area(&p, &t), target);
    let opts = IsotropicOptions { edge_length: edge, ..Default::default() };
    let out = bench("isotropic", t.len() / 3, || isotropic(&p, &t, &opts).unwrap());
    let n = out.indices.len() / 3;
    assert!(n > target / 3 && n < target * 3, "pedidos ~{target}, salieron {n}");
    assert!(is_closed(&out.indices));
    println!("CHECK isotropic hash={:016x} vertices={} triangulos={}", checksum(&out.positions, &out.indices), out.positions.len(), n);
}

/// Suma de comprobación (FNV-1a) de una malla de salida: sirve para comprobar que un cambio de
/// rendimiento no altera el resultado
fn checksum(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |v: u64| hash = (hash ^ v).wrapping_mul(0x100000001b3);
    for p in positions.iter().flatten() {
        mix(p.to_bits() as u64);
    }
    for &i in indices {
        mix(i as u64);
    }
    hash
}

#[test]
#[ignore = "perf"]
fn perf_voxel() {
    let (p, t) = pick(sphere(24, 48), sphere(200, 300));
    let cells = pick(24.0, 256.0);
    let opts = VoxelOptions { voxel_size: 2.2 / cells, smooth_iterations: 2, isotropic_edge: None, offset: 0.0, thickness: 0.0 };
    let out = bench("voxel_smooth2", t.len() / 3, || voxel(&p, &t, &opts).unwrap());
    assert!(!out.indices.is_empty());
    assert!(is_closed(&out.indices), "los vóxeles deben dar una malla cerrada");
    println!("CHECK voxel_smooth2 hash={:016x} vertices={} triangulos={}", checksum(&out.positions, &out.indices), out.positions.len(), out.indices.len() / 3);
}

#[test]
#[ignore = "perf"]
fn perf_voxel_isotropic() {
    // El flujo completo de la app: vóxeles + isótropo final
    let (p, t) = pick(sphere(24, 48), sphere(100, 200));
    let cells = pick(24.0, 160.0);
    let edge = pick(0.1, 0.02);
    let opts = VoxelOptions {
        voxel_size: 2.2 / cells,
        smooth_iterations: 1,
        isotropic_edge: Some(edge),
        offset: 0.0,
        thickness: 0.0,
    };
    let out = bench("voxel_plus_isotropic", t.len() / 3, || voxel(&p, &t, &opts).unwrap());
    assert!(!out.indices.is_empty());
    assert!(is_closed(&out.indices));
    println!("CHECK voxel_plus_isotropic hash={:016x} vertices={} triangulos={}", checksum(&out.positions, &out.indices), out.positions.len(), out.indices.len() / 3);
}

#[test]
#[ignore = "perf"]
fn perf_tris_to_quads() {
    let (p, t) = pick(sphere(24, 48), sphere(316, 474));
    let input = QuadsInput { positions: &p, indices: &t, attributes: &[], stride: 0, groups: &[] };
    let faces = bench("tris_to_quads", t.len() / 3, || tris_to_quads(&input, &QuadsOptions::default()));
    let tris = t.len() / 3;
    assert!(faces.len() < tris * 3 / 4, "{} caras de {tris} triángulos: debería juntar casi todos", faces.len());
    // Subdividir a quads puros
    let sub = bench("all_quads", tris, || all_quads(&faces, p.len()));
    assert!(!sub.quads.is_empty());
}

#[test]
#[ignore = "perf"]
fn perf_retopology_quads() {
    // Retopología de campo orientado (Instant Meshes): la más pesada
    let (p, t) = pick(sphere(24, 48), sphere(100, 200));
    let positions: Vec<Vector3> = p.iter().map(|v| Vector3::new(v[0] as f64, v[1] as f64, v[2] as f64)).collect();
    let triangles: Vec<[usize; 3]> = t.chunks(3).map(|c| [c[0] as usize, c[1] as usize, c[2] as usize]).collect();
    let mesh = Mesh::from_triangles(&positions, &triangles);
    let target = pick(200, 5_000);
    let cfg = RemeshConfig { target_faces: target, ..Default::default() };
    let out = bench("retopology_quads", triangles.len(), || remesh(&mesh, &cfg).unwrap());
    assert!(!out.faces.is_empty(), "sin caras");
}
