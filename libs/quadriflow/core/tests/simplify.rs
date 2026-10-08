//! Simplificar sobre una malla cerrada (las pruebas de PLAN_REMALLAR.md).
#![cfg(feature = "simplify")]

use quadriflow_core::remesh::{deviation, simplify, SimplifyInput, SimplifyOptions};
use std::collections::HashMap;

/// Esfera de radio 1 con `rings` paralelos y `segments` meridianos, soldada y cerrada
fn sphere(rings: usize, segments: usize) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut p = vec![[0.0, 1.0, 0.0]];
    for r in 1..rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            // Un poco de relieve para que no sea trivial
            let k = 1.0 + 0.05 * (5.0 * theta).sin() * phi.sin();
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

/// Cada arista está en exactamente dos triángulos
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

fn as_f64(p: &[[f32; 3]], t: &[u32]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    (
        p.iter().map(|v| v.map(f64::from)).collect(),
        t.chunks(3).map(|c| [c[0] as usize, c[1] as usize, c[2] as usize]).collect(),
    )
}

#[test]
fn closed_sphere_reaches_target_and_stays_closed() {
    let (p, t) = sphere(80, 160);
    assert!(is_closed(&t));
    let before = t.len() / 3;
    let target = before / 10;
    let input = SimplifyInput { positions: &p, indices: &t, attributes: &[], attribute_weights: &[], groups: &[] };
    let r = simplify(&input, &SimplifyOptions { target_triangles: target, ..Default::default() });
    let after = r.indices.len() / 3;
    assert!(after.abs_diff(target) * 20 <= target, "{before} → {after} (pedido {target})");
    assert!(is_closed(&r.indices));
}

#[test]
fn deviation_stays_under_the_limit() {
    let (p, t) = sphere(80, 160);
    let limit = 0.01;
    let input = SimplifyInput { positions: &p, indices: &t, attributes: &[], attribute_weights: &[], groups: &[] };
    let r = simplify(&input, &SimplifyOptions { target_triangles: 10, max_error: Some(limit), ..Default::default() });
    assert!(r.indices.len() / 3 < t.len() / 3 / 4, "debería reducir bastante");
    let original = as_f64(&p, &t);
    let result = as_f64(&p, &r.indices);
    let d = deviation((&original.0, &original.1), (&result.0, &result.1));
    assert!(d.max <= limit as f64, "{d:?}");
}
