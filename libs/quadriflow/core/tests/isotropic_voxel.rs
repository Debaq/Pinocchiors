//! Isótropo y Vóxeles (las pruebas de PLAN_REMALLAR.md).

use quadriflow_core::remesh::{isotropic, voxel, voxel_grid, IsotropicOptions, VoxelOptions};
use quadriflow_core::RemeshError;
use std::collections::HashMap;

/// Esfera UV de radio `r` centrada en `c` (triángulos muy dispares: alargados
/// cerca del ecuador, en abanico en los polos)
fn sphere(c: [f32; 3], r: f32, rings: usize, segments: usize) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut p = vec![[c[0], c[1] + r, c[2]]];
    for i in 1..rings {
        let phi = std::f32::consts::PI * i as f32 / rings as f32;
        for s in 0..segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            p.push([c[0] + r * phi.sin() * theta.cos(), c[1] + r * phi.cos(), c[2] + r * phi.sin() * theta.sin()]);
        }
    }
    p.push([c[0], c[1] - r, c[2]]);
    let bottom = (p.len() - 1) as u32;
    let ring = |i: usize, s: usize| (1 + (i - 1) * segments + s % segments) as u32;
    let mut t = Vec::new();
    for s in 0..segments {
        t.extend([0, ring(1, s + 1), ring(1, s)]);
        t.extend([bottom, ring(rings - 1, s), ring(rings - 1, s + 1)]);
        for i in 1..rings - 1 {
            let (a, b, c, d) = (ring(i, s), ring(i, s + 1), ring(i + 1, s), ring(i + 1, s + 1));
            t.extend([a, b, d, a, d, c]);
        }
    }
    (p, t)
}

/// Toda arista con dos caras recorridas en sentidos opuestos
fn closed_manifold(t: &[u32]) -> bool {
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for c in t.chunks(3) {
        for k in 0..3 {
            *directed.entry((c[k], c[(k + 1) % 3])).or_default() += 1;
        }
    }
    directed.iter().all(|(&(a, b), &n)| n == 1 && directed.get(&(b, a)) == Some(&1))
}

fn edge_lengths(p: &[[f32; 3]], t: &[u32]) -> Vec<f32> {
    t.chunks(3)
        .flat_map(|c| (0..3).map(move |k| (c[k], c[(k + 1) % 3])))
        .filter(|(a, b)| a < b)
        .map(|(a, b)| {
            let (u, v) = (p[a as usize], p[b as usize]);
            ((u[0] - v[0]).powi(2) + (u[1] - v[1]).powi(2) + (u[2] - v[2]).powi(2)).sqrt()
        })
        .collect()
}

fn volume(p: &[[f32; 3]], t: &[u32]) -> f64 {
    t.chunks(3)
        .map(|c| {
            let [a, b, d] = [c[0], c[1], c[2]].map(|i| p[i as usize].map(f64::from));
            (a[0] * (b[1] * d[2] - b[2] * d[1]) - a[1] * (b[0] * d[2] - b[2] * d[0]) + a[2] * (b[0] * d[1] - b[1] * d[0])) / 6.0
        })
        .sum()
}

#[test]
fn isotropic_gives_even_edges_of_the_asked_length() {
    let (p, t) = sphere([0.0; 3], 1.0, 40, 80);
    let out = isotropic(&p, &t, &IsotropicOptions { edge_length: 0.1, ..Default::default() }).unwrap();
    assert!(closed_manifold(&out.indices));
    let lengths = edge_lengths(&out.positions, &out.indices);
    let mean = lengths.iter().sum::<f32>() / lengths.len() as f32;
    let sd = (lengths.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / lengths.len() as f32).sqrt();
    assert!((mean - 0.1).abs() < 0.015, "lado medio {mean}");
    assert!(sd / mean < 0.2, "desvío relativo {}", sd / mean);
    // Los vértices quedan sobre la esfera (la original es un poliedro inscrito)
    assert!(out.positions.iter().all(|v| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() < 0.01));
}

#[test]
fn isotropic_refuses_non_manifold_and_too_dense() {
    // Dos esferas que comparten un triángulo duplicado: arista de cuatro caras
    let (mut p, mut t) = sphere([0.0; 3], 1.0, 10, 20);
    let n = p.len() as u32;
    p.extend([p[t[0] as usize], p[t[1] as usize], [5.0, 5.0, 5.0]]);
    t.extend([n, n + 1, n + 2, n + 1, n, n + 2]);
    let r = isotropic(&p, &t, &IsotropicOptions { edge_length: 0.1, ..Default::default() });
    assert!(matches!(r, Err(RemeshError::NonManifold)), "{r:?}");

    let (p, t) = sphere([0.0; 3], 1.0, 10, 20);
    let r = isotropic(&p, &t, &IsotropicOptions { edge_length: 1e-4, ..Default::default() });
    assert!(matches!(r, Err(RemeshError::TooDense(n)) if n > 1_000_000), "{r:?}");
}

#[test]
fn voxel_joins_crossing_shells_into_one_closed_surface() {
    // Dos esferas que se cruzan: la salida es una sola superficie cerrada
    let (mut p, mut t) = sphere([0.0; 3], 1.0, 30, 60);
    let (q, u) = sphere([1.0, 0.0, 0.0], 1.0, 30, 60);
    let n = p.len() as u32;
    p.extend(q);
    t.extend(u.iter().map(|i| i + n));
    let out = voxel(&p, &t, &VoxelOptions { voxel_size: 0.04, smooth_iterations: 0, isotropic_edge: None }).unwrap();
    assert!(closed_manifold(&out.indices));
    // Volumen de la unión: 2·(4/3)π − la lente de r = 1, d = 1 (5π/12)
    let union = 2.0 * 4.0 / 3.0 * std::f64::consts::PI - 5.0 * std::f64::consts::PI / 12.0;
    let v = volume(&out.positions, &out.indices);
    assert!((v - union).abs() / union < 0.02, "volumen {v} (unión {union})");
    // Y sin caras adentro: ningún vértice dentro de las dos esferas a la vez lejos del borde
    let inside_both = |v: &[f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) < 0.8 && ((v[0] - 1.0).powi(2) + v[1] * v[1] + v[2] * v[2]) < 0.8;
    assert!(!out.positions.iter().any(inside_both));
}

#[test]
fn voxel_smoothing_and_isotropic_after() {
    let (p, t) = sphere([0.0; 3], 1.0, 30, 60);
    let raw = voxel(&p, &t, &VoxelOptions { voxel_size: 0.05, smooth_iterations: 0, isotropic_edge: None }).unwrap();
    let even = voxel(&p, &t, &VoxelOptions { voxel_size: 0.05, smooth_iterations: 3, isotropic_edge: Some(0.1) }).unwrap();
    assert!(closed_manifold(&even.indices));
    assert!(even.indices.len() < raw.indices.len() / 2, "{} → {}", raw.indices.len() / 3, even.indices.len() / 3);
    let lengths = edge_lengths(&even.positions, &even.indices);
    let mean = lengths.iter().sum::<f32>() / lengths.len() as f32;
    assert!((mean - 0.1).abs() < 0.02, "lado medio {mean}");
}

#[test]
fn voxel_grid_is_limited_and_estimated() {
    let (p, t) = sphere([0.0; 3], 1.0, 20, 40);
    let g = voxel_grid(&p, &t, 0.03);
    assert!((g.voxel_size - 0.03).abs() < 1e-9);
    // 2 / 0.03 = 66,7 vóxeles más el margen
    assert!(g.dims.iter().all(|&d| (72..=74).contains(&d)), "{:?}", g.dims);
    assert!(g.triangles > 0 && g.memory > 72 * 72 * 72);
    // Muy fino: se limita a 640 vóxeles en el eje largo; muy grueso, a 48
    assert!((voxel_grid(&p, &t, 1e-6).voxel_size - 2.0 / 640.0).abs() < 1e-6);
    assert!((voxel_grid(&p, &t, 10.0).voxel_size - 2.0 / 48.0).abs() < 1e-6);
}
