//! Suavizar sobre mallas cerradas (las pruebas de PLAN_REMALLAR.md).

use quadriflow_core::remesh::{smooth, SmoothOptions};

/// Esfera de radio 1 soldada, con ruido en el radio de amplitud `noise`
fn sphere(rings: usize, segments: usize, noise: f32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut seed = 11u32;
    let mut rand = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    let mut p = vec![[0.0, 1.0, 0.0]];
    for r in 1..rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let k = 1.0 + noise * rand();
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

fn volume(p: &[[f32; 3]], t: &[u32]) -> f64 {
    t.chunks(3)
        .map(|c| {
            let [a, b, d] = [c[0], c[1], c[2]].map(|i| p[i as usize].map(f64::from));
            (a[0] * (b[1] * d[2] - b[2] * d[1]) - a[1] * (b[0] * d[2] - b[2] * d[0]) + a[2] * (b[0] * d[1] - b[1] * d[0])) / 6.0
        })
        .sum::<f64>()
        .abs()
}

/// Desvío medio del radio respecto de 1
fn roughness(p: &[[f32; 3]]) -> f32 {
    p.iter().map(|v| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs()).sum::<f32>() / p.len() as f32
}

#[test]
fn taubin_keeps_the_volume_and_removes_noise() {
    let (p, t) = sphere(60, 120, 0.02);
    for normal_only in [true, false] {
        let out = smooth(&p, &t, &SmoothOptions { iterations: 20, normal_only, ..Default::default() });
        let (before, after) = (volume(&p, &t), volume(&out, &t));
        assert!((after - before).abs() / before < 0.01, "volumen {before} → {after} (normal_only {normal_only})");
        assert!(roughness(&out) < roughness(&p) * 0.5, "ruido {} → {}", roughness(&p), roughness(&out));
    }
}

#[test]
fn long_smoothing_of_a_clean_sphere_barely_changes_it() {
    let (p, t) = sphere(30, 60, 0.0);
    for normal_only in [true, false] {
        let out = smooth(&p, &t, &SmoothOptions { iterations: 50, normal_only, ..Default::default() });
        let (before, after) = (volume(&p, &t), volume(&out, &t));
        assert!((after - before).abs() / before < 0.01, "volumen {before} → {after} (normal_only {normal_only})");
    }
}

/// Cubo de lado 2 con cada cara en n×n cuadrados, sin soldar (un juego de
/// vértices por cara) y con ruido en el interior de las caras
fn noisy_cube(n: usize) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut seed = 3u32;
    let mut rand = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    let mut p = Vec::new();
    let mut t = Vec::new();
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let base = p.len() as u32;
            for j in 0..=n {
                for i in 0..=n {
                    let (u, v) = (2.0 * i as f32 / n as f32 - 1.0, 2.0 * j as f32 / n as f32 - 1.0);
                    let inside = i > 0 && j > 0 && i < n && j < n;
                    let w = side * (1.0 + if inside { 0.01 * rand() } else { 0.0 });
                    let mut q = [0.0; 3];
                    q[axis] = w;
                    q[(axis + 1) % 3] = u;
                    q[(axis + 2) % 3] = v;
                    p.push(q);
                }
            }
            for j in 0..n as u32 {
                for i in 0..n as u32 {
                    let a = base + j * (n as u32 + 1) + i;
                    let (b, c, d) = (a + 1, a + n as u32 + 1, a + n as u32 + 2);
                    if side > 0.0 { t.extend([a, b, d, a, d, c]) } else { t.extend([a, d, b, a, c, d]) }
                }
            }
        }
    }
    (p, t)
}

#[test]
fn sharp_edges_and_corners_are_kept() {
    let (p, t) = noisy_cube(10);
    let options = SmoothOptions { iterations: 20, sharp_angle: Some(45.0), normal_only: false, ..Default::default() };
    let out = smooth(&p, &t, &options);
    for (a, b) in p.iter().zip(&out) {
        let on_edge = a.iter().filter(|c| (c.abs() - 1.0).abs() < 1e-6).count();
        // Esquinas: quietas; aristas: siguen sobre la arista
        if on_edge == 3 {
            assert_eq!(a, b);
        } else if on_edge == 2 {
            for k in 0..3 {
                if (a[k].abs() - 1.0).abs() < 1e-6 {
                    assert!((a[k] - b[k]).abs() < 1e-5, "{a:?} → {b:?}");
                }
            }
        }
    }
    // Sin aristas vivas, las esquinas se redondean
    let rounded = smooth(&p, &t, &SmoothOptions { sharp_angle: None, ..options });
    let corner = p.iter().position(|v| v.iter().all(|c| (c.abs() - 1.0).abs() < 1e-6)).unwrap();
    assert!(rounded[corner].iter().any(|c| c.abs() < 0.99), "{:?}", rounded[corner]);
}
