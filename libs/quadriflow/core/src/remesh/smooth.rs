//! Suavizar: empareja los vértices sin cambiar la conectividad (Taubin).
//!
//! Cada pasada mueve los vértices hacia el promedio de sus vecinos (λ > 0) y
//! enseguida en sentido contrario un poco más (μ < −λ): el primer paso quita
//! el ruido, el segundo devuelve el volumen que el primero se llevaba, así la
//! malla no encoge como con el laplaciano simple (Taubin 1995, filtro de paso
//! bajo con frecuencia de corte [`PASS_BAND`]).
//!
//! Los vértices que comparten posición se mueven juntos (costuras de UV,
//! normales partidas, STL sin índices). En las aristas vivas y los bordes
//! abiertos el vértice se suaviza solo a lo largo de la arista o del borde, y
//! las esquinas (donde se juntan más de dos) quedan fijas.

use rayon::prelude::*;
use std::collections::HashMap;

/// Cómo suavizar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothOptions {
    /// Pasadas (cada una, un paso λ y uno μ).
    pub iterations: usize,
    /// Cuánto se acerca cada vértice al promedio de sus vecinos por paso (λ, 0–1).
    pub strength: f32,
    /// Ángulo entre caras (grados) desde el que una arista es viva y se
    /// respeta (`None`: no se buscan aristas vivas). También vale para las
    /// esquinas de los bordes y de las aristas vivas, que quedan fijas.
    pub sharp_angle: Option<f32>,
    /// No mover los vértices de los bordes abiertos (si no, se suavizan solo
    /// a lo largo del borde).
    pub fix_borders: bool,
    /// Mover los vértices solo en la dirección de la normal: quita el ruido
    /// sin deslizar los vértices por la superficie (la textura no se corre).
    pub normal_only: bool,
}

impl Default for SmoothOptions {
    fn default() -> Self {
        Self { iterations: 10, strength: 0.5, sharp_angle: None, fix_borders: false, normal_only: true }
    }
}

/// Frecuencia de corte del filtro de Taubin (k_PB): μ = 1 / (k_PB − 1/λ). Con
/// 0,1 (el valor del artículo) las frecuencias bajas crecen un poco en cada
/// pasada: una esfera ganaba 2 % de volumen en 50; con 0,02, 0,25 %.
const PASS_BAND: f64 = 0.02;

/// Posiciones suavizadas, una por vértice de la entrada (los que no están en
/// ningún triángulo quedan donde estaban).
pub fn smooth(positions: &[[f32; 3]], indices: &[u32], options: &SmoothOptions) -> Vec<[f32; 3]> {
    let lambda = f64::from(options.strength.clamp(0.0, 1.0));
    if options.iterations == 0 || lambda <= 0.0 || indices.len() < 3 {
        return positions.to_vec();
    }
    let mu = 1.0 / (PASS_BAND - 1.0 / lambda);

    let (welded, mut p) = weld(positions);
    let triangles: Vec<[u32; 3]> = indices
        .chunks_exact(3)
        .filter(|t| t.iter().all(|&i| (i as usize) < positions.len()))
        .map(|t| [welded[t[0] as usize], welded[t[1] as usize], welded[t[2] as usize]])
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
    let rules = Rules::of(&p, &triangles, options);
    // Las normales de la superficie de entrada, fijas: recalcularlas en cada
    // paso hace el filtro inestable (en una esquina las caras se dan vuelta,
    // la normal se invierte y el paso μ empuja hacia afuera sin parar)
    let normals = if options.normal_only { vertex_normals(&p, &triangles) } else { Vec::new() };

    for _ in 0..options.iterations {
        step(&mut p, &rules, &normals, lambda);
        step(&mut p, &rules, &normals, mu);
    }
    welded.iter().map(|&w| p[w as usize].map(|c| c as f32)).collect()
}

/// Funde los vértices de igual posición. Devuelve el soldado de cada uno y
/// las posiciones soldadas.
fn weld(positions: &[[f32; 3]]) -> (Vec<u32>, Vec<[f64; 3]>) {
    let mut map: HashMap<[u32; 3], u32> = HashMap::with_capacity(positions.len());
    let mut welded_positions = Vec::new();
    let welded = positions
        .iter()
        .map(|v| {
            let key = v.map(|f| if f == 0.0 { 0 } else { f.to_bits() });
            *map.entry(key).or_insert_with(|| {
                welded_positions.push(v.map(f64::from));
                welded_positions.len() as u32 - 1
            })
        })
        .collect();
    (welded, welded_positions)
}

/// Hacia qué vecinos se mueve cada vértice
struct Rules {
    /// Vecinos de cada vértice (CSR): `neighbors[start[v]..start[v + 1]]`
    start: Vec<usize>,
    neighbors: Vec<u32>,
    /// Se suaviza en la superficie (si no, a lo largo de una arista o borde)
    free: Vec<bool>,
}

impl Rules {
    fn of(p: &[[f64; 3]], triangles: &[[u32; 3]], options: &SmoothOptions) -> Self {
        // Caras de cada arista
        let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::with_capacity(triangles.len() * 3 / 2);
        for (f, t) in triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push(f as u32);
            }
        }
        let cos_sharp = options.sharp_angle.map(|a| f64::from(a).to_radians().cos());
        let normals: Vec<[f64; 3]> = triangles.iter().map(|t| normalize(face_normal(p, t))).collect();

        let n = p.len();
        let mut all: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut border: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut crease: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (&(a, b), faces) in &edges {
            all[a as usize].push(b);
            all[b as usize].push(a);
            let target = match faces.len() {
                1 => Some(&mut border),
                2 => cos_sharp.filter(|&c| dot(normals[faces[0] as usize], normals[faces[1] as usize]) < c).map(|_| &mut crease),
                // Arista de más de dos caras: se respeta como una viva
                _ => Some(&mut crease),
            };
            if let Some(list) = target {
                list[a as usize].push(b);
                list[b as usize].push(a);
            }
        }

        let mut start = Vec::with_capacity(n + 1);
        let mut neighbors = Vec::new();
        let mut free = vec![false; n];
        for v in 0..n {
            start.push(neighbors.len());
            let (b, c) = (&border[v], &crease[v]);
            let chosen: &[u32] = if all[v].is_empty() {
                &[]
            } else if b.is_empty() && c.is_empty() {
                free[v] = true;
                &all[v]
            } else if b.is_empty() && c.len() == 2 && !bends(p, v, c, cos_sharp) {
                c
            } else if c.is_empty() && b.len() == 2 && !options.fix_borders && !bends(p, v, b, cos_sharp) {
                b
            } else {
                // Esquina, punta de una arista viva, borde fijo o raro: quieto
                &[]
            };
            neighbors.extend_from_slice(chosen);
        }
        start.push(neighbors.len());
        Self { start, neighbors, free }
    }
}

/// La línea (arista viva o borde) dobla en `v` más que el ángulo vivo: es
/// una esquina y no se mueve
fn bends(p: &[[f64; 3]], v: usize, line: &[u32], cos_sharp: Option<f64>) -> bool {
    let Some(cos_sharp) = cos_sharp else { return false };
    let [a, b] = [line[0], line[1]].map(|u| normalize([0, 1, 2].map(|k| p[v][k] - p[u as usize][k])));
    // Recta: a y b opuestos (−1); el desvío de la recta es el ángulo entre a y −b
    -dot(a, b) < cos_sharp
}

/// Un paso del filtro: cada vértice se acerca (factor > 0) o se aleja
/// (factor < 0) del promedio de sus vecinos
/// (solo según `normals`, si hay)
fn step(p: &mut Vec<[f64; 3]>, rules: &Rules, normals: &[[f64; 3]], factor: f64) {
    let current = &*p;
    let next: Vec<[f64; 3]> = (0..current.len())
        .into_par_iter()
        .map(|v| {
            let around = &rules.neighbors[rules.start[v]..rules.start[v + 1]];
            if around.is_empty() {
                return current[v];
            }
            let mut mean = [0.0; 3];
            for &u in around {
                for k in 0..3 {
                    mean[k] += current[u as usize][k];
                }
            }
            let mut delta = [0.0; 3];
            for k in 0..3 {
                delta[k] = mean[k] / around.len() as f64 - current[v][k];
            }
            // Las aristas y bordes se suavizan a lo largo de ellos, sin proyectar
            if !normals.is_empty() && rules.free[v] {
                let n = normals[v];
                let d = dot(delta, n);
                delta = n.map(|c| c * d);
            }
            [0, 1, 2].map(|k| current[v][k] + factor * delta[k])
        })
        .collect();
    *p = next;
}

/// Normal de cada vértice, promedio de sus caras pesado por área
fn vertex_normals(p: &[[f64; 3]], triangles: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let mut normals = vec![[0.0; 3]; p.len()];
    for t in triangles {
        let n = face_normal(p, t);
        for &v in t {
            for k in 0..3 {
                normals[v as usize][k] += n[k];
            }
        }
    }
    normals.into_iter().map(normalize).collect()
}

/// Normal de la cara con largo = doble del área
fn face_normal(p: &[[f64; 3]], t: &[u32; 3]) -> [f64; 3] {
    let [a, b, c] = t.map(|i| p[i as usize]);
    let (u, w) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
    [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = dot(v, v).sqrt();
    if len > 0.0 { v.map(|c| c / len) } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla plana de n×n con relieve de ruido en z
    fn noisy_grid(n: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
        let mut seed = 7u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        let mut p = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                p.push([x as f32 / n as f32, y as f32 / n as f32, 0.01 * noise()]);
            }
        }
        let mut t = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                t.extend([a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
            }
        }
        (p, t)
    }

    fn roughness(p: &[[f32; 3]]) -> f32 {
        (p.iter().map(|v| v[2] * v[2]).sum::<f32>() / p.len() as f32).sqrt()
    }

    #[test]
    fn removes_noise_from_a_plane() {
        let (p, t) = noisy_grid(30);
        let out = smooth(&p, &t, &SmoothOptions::default());
        assert!(roughness(&out) < roughness(&p) * 0.5, "{} → {}", roughness(&p), roughness(&out));
    }

    #[test]
    fn fixed_borders_do_not_move() {
        let (p, t) = noisy_grid(20);
        let out = smooth(&p, &t, &SmoothOptions { fix_borders: true, normal_only: false, ..Default::default() });
        for (i, (a, b)) in p.iter().zip(&out).enumerate() {
            let (x, y) = (i % 21, i / 21);
            if x == 0 || y == 0 || x == 20 || y == 20 {
                assert_eq!(a, b, "vértice de borde {i}");
            }
        }
    }

    #[test]
    fn free_borders_stay_on_the_border_line() {
        // Sin ruido, los bordes rectos siguen en su recta aunque se suavicen
        let (mut p, t) = noisy_grid(10);
        p.iter_mut().for_each(|v| v[2] = 0.0);
        let options = SmoothOptions { normal_only: false, sharp_angle: Some(30.0), ..Default::default() };
        let out = smooth(&p, &t, &options);
        assert_eq!(out[0], p[0], "la esquina queda fija");
        for (i, v) in out.iter().enumerate() {
            let (x, y) = (i % 11, i / 11);
            if x == 0 {
                assert!(v[0].abs() < 1e-6);
            }
            if y == 0 {
                assert!(v[1].abs() < 1e-6);
            }
        }
    }

    #[test]
    fn duplicated_vertices_move_together() {
        let (p, t) = noisy_grid(12);
        let split: Vec<[f32; 3]> = t.iter().map(|&i| p[i as usize]).collect();
        let indices: Vec<u32> = (0..split.len() as u32).collect();
        let welded = smooth(&p, &t, &SmoothOptions::default());
        let unwelded = smooth(&split, &indices, &SmoothOptions::default());
        for (k, &i) in t.iter().enumerate() {
            assert_eq!(unwelded[k], welded[i as usize]);
        }
    }

    #[test]
    fn nothing_to_do_returns_input() {
        let (p, t) = noisy_grid(4);
        assert_eq!(smooth(&p, &t, &SmoothOptions { iterations: 0, ..Default::default() }), p);
        assert_eq!(smooth(&p, &[], &SmoothOptions::default()), p);
    }
}
