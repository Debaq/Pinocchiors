//! Optimización geométrica de la malla de quads final.
//!
//! Cada vértice se mueve, de a uno, a la mejor de dos posiciones candidatas:
//! el promedio de sus vecinos y el promedio "de paralelogramo" (la posición
//! que haría de cada quad vecino un paralelogramo), llevadas a la superficie.
//! Los vértices sobre aristas vivas o bordes se deslizan a lo largo de la
//! línea y las esquinas no se mueven. Gana la posición con menor energía de
//! sus quads (Σ (1 − calidad)², con una barrera cerca de plegarse), y solo si
//! mejora la actual; si algún quad del vértice es malo se prueban además
//! posiciones cercanas en su plano tangente.

use crate::features::FeatureLines;
use crate::quad::QuadMesh;
use crate::V3;
use pinocchio_spatial::Bvh;
use std::collections::HashMap;

/// Calidad (ver [`quality`]) a partir de la cual un quad se considera bueno.
const GOOD: f64 = 0.5;

/// Búsqueda local para vértices con quads malos: direcciones y radios (en
/// largos medios de arista) de las posiciones de prueba.
const SEARCH_DIRECTIONS: usize = 8;
const SEARCH_RADII: [f64; 3] = [0.05, 0.1, 0.2];

/// Una posición candidata que al proyectarse sobre la superficie se mueve más
/// que esto (en largos medios de arista) cayó en otra hoja y se descarta.
const MAX_PROJECTION: f64 = 0.25;

fn tangent_basis(normal: &V3) -> (V3, V3) {
    let n = normal.try_normalize(1e-300).unwrap_or_else(V3::z);
    let helper = if n.x.abs() > 0.9 { V3::y() } else { V3::x() };
    let t1 = n.cross(&helper).normalize();
    (t1, n.cross(&t1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Free,
    Line,
    Corner,
}

struct Topology {
    faces_of: Vec<Vec<usize>>,
    neighbors: Vec<Vec<usize>>,
    /// Vecinos a lo largo de aristas vivas o de borde
    line_neighbors: Vec<Vec<usize>>,
    kind: Vec<Kind>,
}

fn topology(q: &QuadMesh, fixed: &[bool], lines: &FeatureLines, cos_corner: f64) -> Topology {
    let n = q.vertices.len();
    let mut faces_of = vec![Vec::new(); n];
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for (f, face) in q.faces.iter().enumerate() {
        for k in 0..4 {
            faces_of[face.v[k]].push(f);
            let (a, b) = (face.v[k], face.v[(k + 1) % 4]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut edges: Vec<((usize, usize), u32)> = edges.into_iter().collect();
    edges.sort_unstable();
    let mut neighbors = vec![Vec::new(); n];
    let mut line_neighbors = vec![Vec::new(); n];
    for ((a, b), count) in edges {
        neighbors[a].push(b);
        neighbors[b].push(a);
        // Arista sobre un rasgo: de borde, o entre dos vértices fijos con el
        // punto medio sobre una línea viva
        let feature = count == 1
            || (fixed[a] && fixed[b] && {
                let m = (q.vertices[a] + q.vertices[b]) * 0.5;
                let tolerance = 0.05 * (q.vertices[a] - q.vertices[b]).norm();
                lines.closest(&m).is_some_and(|p| (p - m).norm() <= tolerance)
            });
        if feature {
            line_neighbors[a].push(b);
            line_neighbors[b].push(a);
        }
    }
    let kind = (0..n)
        .map(|v| {
            match line_neighbors[v][..] {
                // Fijo sin aristas vivas reales (punto medio entre dos líneas,
                // proyección descartada): se mueve libre
                [] => Kind::Free,
                [x, y] => {
                    let (dx, dy) = (q.vertices[x] - q.vertices[v], q.vertices[y] - q.vertices[v]);
                    let straight = dx.try_normalize(1e-300).zip(dy.try_normalize(1e-300));
                    if straight.is_some_and(|(dx, dy)| (-dx).dot(&dy) > cos_corner) { Kind::Line } else { Kind::Corner }
                }
                _ => Kind::Corner,
            }
        })
        .collect();
    Topology { faces_of, neighbors, line_neighbors, kind }
}

/// Calidad del quad: la peor de sus esquinas, cada una medida como
/// 2 (a × b)·n / (|a|² + |b|²). Vale 1 en un cuadrado y baja tanto con el
/// ángulo como con el estiramiento (a diferencia del jacobiano escalado, que
/// da 1 a cualquier rectángulo); es negativa si la esquina se pliega.
fn quality(p: [V3; 4], reference: &V3) -> f64 {
    (0..4)
        .map(|k| {
            let (a, b) = (p[(k + 1) % 4] - p[k], p[(k + 3) % 4] - p[k]);
            let d = a.norm_squared() + b.norm_squared();
            if d == 0.0 { -1.0 } else { 2.0 * a.cross(&b).dot(reference) / d }
        })
        .fold(f64::INFINITY, f64::min)
}

/// Energía de un quad según su calidad: cuadrática alrededor del cuadrado y
/// con una barrera fuerte al acercarse a plegarse.
fn energy(quality: f64) -> f64 {
    let barrier = (BARRIER - quality).max(0.0);
    (1.0 - quality).powi(2) + BARRIER_WEIGHT * barrier * barrier
}

const BARRIER: f64 = 0.1;
const BARRIER_WEIGHT: f64 = 10.0;

pub(crate) fn optimize(
    q: &mut QuadMesh,
    fixed: &[bool],
    bvh: &Bvh,
    lines: &FeatureLines,
    sharp_angle: f64,
    iterations: usize,
) {
    let topo = topology(q, fixed, lines, sharp_angle.cos());
    let face_quality = |q: &QuadMesh, f: usize, reference: &[V3], moved: Option<(usize, V3)>| {
        let p = q.faces[f].v.map(|i| match moved {
            Some((v, pos)) if v == i => pos,
            _ => q.vertices[i],
        });
        quality(p, &reference[f])
    };

    for _ in 0..iterations {
        // Normal de referencia de cada quad: la de la superficie bajo su centro
        let reference: Vec<V3> = q
            .faces
            .iter()
            .map(|f| {
                let c = f.v.iter().map(|&i| q.vertices[i]).sum::<V3>() / 4.0;
                bvh.query_closest(&pinocchio_math::Vector3(c))
                    .and_then(|hit| bvh.triangle(hit.triangle).normal().0.try_normalize(1e-300))
                    .unwrap_or_else(V3::zeros)
            })
            .collect();

        for v in 0..q.vertices.len() {
            if topo.kind[v] == Kind::Corner || topo.faces_of[v].is_empty() {
                continue;
            }
            let energy_at = |moved: Option<(usize, V3)>| -> (f64, f64) {
                topo.faces_of[v].iter().fold((0.0, f64::INFINITY), |(e, worst), &f| {
                    let quality = face_quality(q, f, &reference, moved);
                    (e + energy(quality), worst.min(quality))
                })
            };
            let (current_energy, current) = energy_at(None);

            let laplacian = match topo.kind[v] {
                Kind::Line => {
                    let [x, y] = topo.line_neighbors[v][..] else { continue };
                    (q.vertices[x] + q.vertices[y]) * 0.5
                }
                _ => topo.neighbors[v].iter().map(|&u| q.vertices[u]).sum::<V3>() / topo.neighbors[v].len() as f64,
            };
            // En cada quad (v, a, b, c) el paralelogramo pide v = a + c − b
            let parallelogram = topo.faces_of[v]
                .iter()
                .map(|&f| {
                    let face = q.faces[f].v;
                    let k = face.iter().position(|&i| i == v).expect("v en su cara");
                    let (a, b, c) = (face[(k + 1) % 4], face[(k + 2) % 4], face[(k + 3) % 4]);
                    q.vertices[a] + q.vertices[c] - q.vertices[b]
                })
                .sum::<V3>()
                / topo.faces_of[v].len() as f64;

            let here = q.vertices[v];
            let spacing = topo.neighbors[v].iter().map(|&u| (q.vertices[u] - here).norm()).sum::<f64>()
                / topo.neighbors[v].len().max(1) as f64;
            // Llevar a la superficie sin saltar a otra hoja (paredes delgadas)
            let place = |p: V3| {
                let on = match topo.kind[v] {
                    Kind::Line => lines.closest(&p),
                    _ => bvh.query_closest(&pinocchio_math::Vector3(p)).map(|hit| hit.point.0),
                }?;
                ((on - p).norm() <= MAX_PROJECTION * spacing).then_some(on)
            };
            let mut candidates = vec![laplacian, parallelogram];
            if current < GOOD {
                // Búsqueda local alrededor del vértice, en su plano tangente
                let normal = topo.faces_of[v].iter().map(|&f| reference[f]).sum::<V3>();
                let (t1, t2) = tangent_basis(&normal);
                for radius in SEARCH_RADII {
                    for k in 0..SEARCH_DIRECTIONS {
                        let angle = std::f64::consts::TAU * k as f64 / SEARCH_DIRECTIONS as f64;
                        candidates.push(here + (t1 * angle.cos() + t2 * angle.sin()) * (radius * spacing));
                    }
                }
            }
            let best = candidates
                .into_iter()
                .filter_map(place)
                .map(|p| (energy_at(Some((v, p))).0, p))
                .filter(|&(e, _)| e < current_energy - 1e-12)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, p)) = best {
                q.vertices[v] = p;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quad::QuadFace;
    use pinocchio_spatial::Triangle;

    /// Grilla de 3×3 vértices en el plano z = 0 (cuatro quads); el borde es fijo.
    fn grid() -> (QuadMesh, Vec<bool>, Bvh, FeatureLines) {
        let vertices: Vec<V3> = (0..9).map(|i| V3::new((i % 3) as f64, (i / 3) as f64, 0.0)).collect();
        let faces = [[0, 1, 4, 3], [1, 2, 5, 4], [3, 4, 7, 6], [4, 5, 8, 7]].map(|v| QuadFace { v }).to_vec();
        let fixed = (0..9).map(|i| i != 4).collect();
        let v3 = |x: f64, y: f64| pinocchio_math::Vector3(V3::new(x, y, 0.0));
        let bvh = Bvh::build(vec![
            Triangle::new(v3(-1.0, -1.0), v3(3.0, -1.0), v3(3.0, 3.0)),
            Triangle::new(v3(-1.0, -1.0), v3(3.0, 3.0), v3(-1.0, 3.0)),
        ]);
        let corners = [V3::new(0.0, 0.0, 0.0), V3::new(2.0, 0.0, 0.0), V3::new(2.0, 2.0, 0.0), V3::new(0.0, 2.0, 0.0)];
        let segments = (0..4).map(|k| (corners[k], corners[(k + 1) % 4])).collect();
        (QuadMesh { vertices, faces }, fixed, bvh, FeatureLines::from_segments(segments, 1.0))
    }

    #[test]
    fn folded_center_is_untangled() {
        let (mut q, fixed, bvh, lines) = grid();
        q.vertices[4] = V3::new(1.9, 1.8, 0.0); // pliega el quad de abajo a la izquierda
        optimize(&mut q, &fixed, &bvh, &lines, std::f64::consts::FRAC_PI_4, 30);
        // Vuelve a una grilla sana (la energía mide forma, no tamaño parejo)
        assert!((q.vertices[4] - V3::new(1.0, 1.0, 0.0)).norm() < 0.2, "{:?}", q.vertices[4]);
        for f in &q.faces {
            let p = f.v.map(|i| q.vertices[i]);
            assert!(quality(p, &V3::z()) > 0.9, "{p:?}");
        }
    }

    #[test]
    fn boundary_vertices_slide_along_the_boundary() {
        let (mut q, mut fixed, bvh, lines) = grid();
        fixed[4] = false;
        q.vertices[1] = V3::new(0.3, 0.0, 0.0); // punto medio del borde inferior, corrido
        optimize(&mut q, &fixed, &bvh, &lines, std::f64::consts::FRAC_PI_4, 20);
        let p = q.vertices[1];
        assert!(p.y.abs() < 1e-12, "salió del borde: {p:?}");
        assert!((p.x - 1.0).abs() < 1e-3, "{p:?}");
        // Las esquinas no se mueven
        assert_eq!(q.vertices[0], V3::zeros());
    }
}
