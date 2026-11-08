//! Influencia estimada de los huesos antes de calcular los pesos: cada vértice
//! se reparte entre los huesos más cercanos (distancia al segmento padre →
//! hueso), como vista previa de qué mueve cada uno. No reemplaza a los pesos
//! calculados (difusión de calor), que miran el grosor de la malla.
//!
//! Es la misma cuenta que `estimateInfluence` de apps/web/src/lib/boneInfluence.ts
//! (que queda como respaldo sin backend), repartida entre núcleos.

use rayon::prelude::*;

/// Se descartan los huesos a más de 2,5 × la distancia al más cercano
/// (pesarían menos de 0,4 %); en distancias al cuadrado
const CUTOFF2: f64 = 2.5 * 2.5;

/// Vértices por tarea de rayon
const CHUNK: usize = 2048;

/// Segmento padre → hueso: origen, dirección y 1 / largo²
struct Segment {
    bone: u32,
    origin: [f64; 3],
    dir: [f64; 3],
    inv_len2: f64,
}

/// Por vértice, `k` pares (hueso, peso) en `f32`, como los pesos calculados
/// (`numVertices × k × 2`). Los pares sin hueso quedan en (0, 0).
///
/// `positions` son x, y, z seguidos; `bones` la posición de cada articulación y
/// su padre (el peso del hueso `b` es el de su segmento padre → `b`).
pub fn estimate_influence(positions: &[f32], bones: &[([f32; 3], Option<usize>)], k: usize) -> Vec<f32> {
    let num_vertices = positions.len() / 3;
    let mut weights = vec![0.0f32; num_vertices * k * 2];
    if k == 0 {
        return weights;
    }

    let mut extent = 0.0f64;
    let segments: Vec<Segment> = bones
        .iter()
        .enumerate()
        .filter_map(|(b, (pos, parent))| {
            let a = bones.get((*parent)?)?.0;
            let origin = a.map(f64::from);
            let dir = [pos[0] as f64 - origin[0], pos[1] as f64 - origin[1], pos[2] as f64 - origin[2]];
            let len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
            extent = extent.max(len2.sqrt());
            Some(Segment { bone: b as u32, origin, dir, inv_len2: if len2 > 0.0 { 1.0 / len2 } else { 0.0 } })
        })
        .collect();
    if segments.is_empty() {
        return weights;
    }

    // Tamaño del esqueleto: evita dividir por cero sobre el hueso mismo
    let eps2 = (extent * 1e-3).max(1e-9).powi(2);

    weights
        .par_chunks_mut(CHUNK * k * 2)
        .enumerate()
        .for_each(|(chunk, out)| {
            let mut dist = vec![0.0f64; segments.len()];
            let mut best = vec![usize::MAX; k];
            let mut best_w = vec![0.0f64; k];
            for (local, row) in out.chunks_mut(k * 2).enumerate() {
                let v = chunk * CHUNK + local;
                let p = [positions[3 * v] as f64, positions[3 * v + 1] as f64, positions[3 * v + 2] as f64];
                let mut dmin = f64::INFINITY;
                for (d, s) in dist.iter_mut().zip(&segments) {
                    let w = [p[0] - s.origin[0], p[1] - s.origin[1], p[2] - s.origin[2]];
                    let t = ((w[0] * s.dir[0] + w[1] * s.dir[1] + w[2] * s.dir[2]) * s.inv_len2).clamp(0.0, 1.0);
                    let e = [w[0] - t * s.dir[0], w[1] - t * s.dir[1], w[2] - t * s.dir[2]];
                    *d = e[0] * e[0] + e[1] * e[1] + e[2] * e[2] + eps2;
                    dmin = dmin.min(*d);
                }

                // Los k de más peso: (dmin / d)^6 = (dmin² / d²)³
                best.fill(usize::MAX);
                best_w.fill(0.0);
                let far = dmin * CUTOFF2;
                for (i, &d) in dist.iter().enumerate() {
                    if d > far {
                        continue;
                    }
                    let r = dmin / d;
                    let w = r * r * r;
                    if w <= best_w[k - 1] {
                        continue;
                    }
                    let mut slot = k - 1;
                    while slot > 0 && best_w[slot - 1] < w {
                        best_w[slot] = best_w[slot - 1];
                        best[slot] = best[slot - 1];
                        slot -= 1;
                    }
                    best_w[slot] = w;
                    best[slot] = i;
                }
                let sum: f64 = best_w.iter().sum();
                for slot in 0..k {
                    if best[slot] == usize::MAX {
                        continue;
                    }
                    row[slot * 2] = segments[best[slot]].bone as f32;
                    row[slot * 2 + 1] = (best_w[slot] / sum) as f32;
                }
            }
        });
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cadena recta raíz (0) → 1 → 2 sobre el eje Y
    fn chain() -> Vec<([f32; 3], Option<usize>)> {
        vec![([0.0, 0.0, 0.0], None), ([0.0, 1.0, 0.0], Some(0)), ([0.0, 2.0, 0.0], Some(1))]
    }

    #[test]
    fn vertex_near_a_bone_belongs_to_it() {
        // Un vértice a media altura del primer segmento y otro del segundo
        let positions = [0.1, 0.5, 0.0, 0.1, 1.5, 0.0];
        let w = estimate_influence(&positions, &chain(), 4);
        assert_eq!(w.len(), 2 * 4 * 2);
        assert_eq!(w[0], 1.0, "hueso 1 manda en el primer vértice");
        assert!(w[1] > 0.99);
        assert_eq!(w[8], 2.0, "hueso 2 manda en el segundo vértice");
        assert!(w[9] > 0.99);
    }

    #[test]
    fn weights_add_up_to_one_and_skip_root() {
        let positions: Vec<f32> = (0..5000).flat_map(|i| {
            let t = i as f32 / 5000.0;
            [(t * 37.0).sin() * 0.3, t * 2.2 - 0.1, (t * 53.0).cos() * 0.3]
        }).collect();
        let w = estimate_influence(&positions, &chain(), 4);
        for row in w.chunks(8) {
            let sum: f32 = row.iter().skip(1).step_by(2).sum();
            assert!((sum - 1.0).abs() < 1e-4, "suma {sum}");
            for pair in row.chunks(2) {
                assert!(pair[1] == 0.0 || pair[0] != 0.0, "la raíz no tiene segmento propio");
            }
        }
    }

    #[test]
    fn no_segments_gives_zeros() {
        let w = estimate_influence(&[1.0, 2.0, 3.0], &[([0.0; 3], None)], 4);
        assert!(w.iter().all(|&x| x == 0.0));
    }
}
