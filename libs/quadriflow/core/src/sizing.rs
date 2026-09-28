//! Densidad adaptativa: escala local del retículo según el grosor.
//!
//! Un rasgo más angosto que un quad (pared, tubo, punta, ranura, agujero
//! chico) no se puede representar: los quads lo cruzan, lo tapan o se pliegan.
//! Desde muestras repartidas sobre la superficie se lanzan dos rayos por la
//! normal (hacia adentro: paredes, tubos, puntas; hacia afuera: ranuras,
//! huecos, agujeros) y la escala local queda en el grosor medido. Luego se
//! limita su crecimiento con la distancia sobre la superficie (transiciones
//! graduales) y se calibra la escala base para conservar la cantidad de quads.

use crate::surface::Surface;
use crate::V3;
use pinocchio_spatial::Bvh;
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// La escala nunca baja de esta fracción de la base (limita la cantidad de
/// quads que se gasta en un rasgo).
pub(crate) const MIN_FACTOR: f64 = 1.0 / 6.0;
/// Crecimiento máximo de la escala por unidad de distancia sobre la superficie.
const GRADATION: f64 = 0.3;
/// Lado de las celdas de la grilla de consulta, en unidades de la escala.
const CELL: f64 = 0.25;

/// Escala local del retículo en cualquier punto: grilla de celdas con el
/// grosor mínimo de las muestras que caen en cada una.
pub(crate) struct Sizing {
    /// Grosor graduado por celda (sin los límites de la base)
    grid: HashMap<[i64; 3], f64>,
    adaptive: bool,
    base: f64,
    cell: f64,
}

impl Sizing {
    /// Escala uniforme (sin rasgos delgados).
    pub fn uniform(scale: f64) -> Self {
        Self { grid: HashMap::new(), adaptive: false, base: scale, cell: scale }
    }

    /// Mide el grosor de `surface` (la entrada, antes de remallar) y calibra la
    /// base para que la cantidad de quads sea la de una escala uniforme `scale`.
    pub fn measure(surface: &Surface, bvh: &Bvh, scale: f64) -> Self {
        let mut samples = surface.clone();
        samples.subdivide(scale * 0.5);
        let n = samples.positions.len();

        // Normales y áreas por muestra
        let mut normals = vec![V3::zeros(); n];
        let mut areas = vec![0.0; n];
        for t in &samples.triangles {
            let [a, b, c] = t.map(|i| samples.positions[i as usize]);
            let cross = (b - a).cross(&(c - a));
            for &v in t {
                normals[v as usize] += cross;
                areas[v as usize] += cross.norm() / 6.0;
            }
        }

        // Grosor: el primer choque por la normal, hacia adentro y hacia afuera
        // La base termina algo por encima de `scale`: se mide hasta el doble
        let reach = scale * 2.0;
        let epsilon = scale * 1e-3;
        let mut thickness: Vec<f64> = (0..n)
            .into_par_iter()
            .map(|i| {
                let Some(nrm) = normals[i].try_normalize(1e-300) else { return f64::INFINITY };
                let p = samples.positions[i];
                [nrm, -nrm]
                    .iter()
                    .filter_map(|d| {
                        bvh.ray_distance(&pinocchio_math::Vector3(p), &pinocchio_math::Vector3(*d), epsilon, reach)
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();

        // Gradación: ningún punto con más escala que un vecino más la distancia
        let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); n];
        for t in &samples.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                neighbors[a as usize].push(b);
                neighbors[b as usize].push(a);
            }
        }
        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = (0..n as u32)
            .filter(|&i| thickness[i as usize].is_finite())
            .map(|i| Reverse((thickness[i as usize].to_bits(), i)))
            .collect();
        while let Some(Reverse((bits, i))) = heap.pop() {
            let z = f64::from_bits(bits);
            if z > thickness[i as usize] {
                continue;
            }
            for &j in &neighbors[i as usize] {
                let candidate = z + GRADATION * (samples.positions[j as usize] - samples.positions[i as usize]).norm();
                if candidate < thickness[j as usize] {
                    thickness[j as usize] = candidate;
                    heap.push(Reverse((candidate.to_bits(), j)));
                }
            }
        }

        // Base: la que gasta en total los mismos quads que la escala uniforme
        let total_area: f64 = areas.iter().sum();
        let budget = total_area / (scale * scale);
        let count = |base: f64| -> f64 {
            areas
                .iter()
                .zip(&thickness)
                .map(|(a, &z)| {
                    let s = z.clamp(base * MIN_FACTOR, base);
                    a / (s * s)
                })
                .sum()
        };
        let (mut lo, mut hi) = (scale, scale);
        while count(hi) > budget && hi < scale * 16.0 {
            hi *= 2.0;
        }
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if count(mid) > budget { lo = mid } else { hi = mid }
        }
        let base = hi;

        let cell = scale * CELL;
        let mut grid: HashMap<[i64; 3], f64> = HashMap::new();
        for (p, &z) in samples.positions.iter().zip(&thickness) {
            let slot = grid.entry(cell_of(p, cell)).or_insert(f64::INFINITY);
            *slot = slot.min(z);
        }
        let adaptive = thickness.iter().any(|&z| z < base);
        Self { grid, adaptive, base, cell }
    }

    /// `true` si algún rasgo es más angosto que la base.
    pub fn is_adaptive(&self) -> bool {
        self.adaptive
    }

    pub fn base(&self) -> f64 {
        self.base
    }

    /// Cambia la base (corrección de la cantidad de quads); el grosor es
    /// geometría y no cambia.
    pub fn set_base(&mut self, base: f64) {
        self.base = base;
    }

    /// Escala del retículo en `p`: la de su celda o, si está vacía, la menor
    /// de las celdas vecinas.
    pub fn at(&self, p: &V3) -> f64 {
        if self.grid.is_empty() {
            return self.base;
        }
        let c = cell_of(p, self.cell);
        let z = self.grid.get(&c).copied().unwrap_or_else(|| {
            let mut best = f64::INFINITY;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(&z) = self.grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                            best = best.min(z);
                        }
                    }
                }
            }
            best
        });
        z.clamp(self.base * MIN_FACTOR, self.base)
    }
}

fn cell_of(p: &V3, cell: f64) -> [i64; 3] {
    [(p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio_spatial::Triangle;

    /// Caja de lados `size` con la esquina en `origin`.
    fn cuboid(origin: V3, size: V3) -> Surface {
        let positions = (0..8)
            .map(|k| origin + size.component_mul(&V3::new((k & 1) as f64, (k >> 1 & 1) as f64, (k >> 2 & 1) as f64)))
            .collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let triangles = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        Surface::new(positions, triangles)
    }

    /// Cubo de 10 y, aparte, una aleta de 0.5 de espesor.
    fn cube_and_fin() -> Surface {
        let mut s = cuboid(V3::zeros(), V3::repeat(10.0));
        let fin = cuboid(V3::new(20.0, 0.0, 0.0), V3::new(0.5, 3.0, 3.0));
        let n = s.positions.len() as u32;
        s.positions.extend(fin.positions);
        s.triangles.extend(fin.triangles.iter().map(|t| t.map(|i| i + n)));
        s
    }

    fn bvh(s: &Surface) -> Bvh {
        Bvh::build(
            s.triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|i| pinocchio_math::Vector3(s.positions[i as usize]));
                    Triangle::new(a, b, c)
                })
                .collect(),
        )
    }

    #[test]
    fn thin_fin_gets_a_smaller_scale() {
        let s = cube_and_fin();
        let sizing = Sizing::measure(&s, &bvh(&s), 2.0);
        assert!(sizing.is_adaptive());
        // En la cara grande de la aleta, la escala es su espesor
        let fin = sizing.at(&V3::new(20.0, 1.5, 1.5));
        assert!((fin - 0.5).abs() < 0.1, "{fin}");
        // El cubo queda con la base, que crece para gastar lo mismo en total
        let cube = sizing.at(&V3::new(5.0, 5.0, 10.0));
        assert!(cube > 2.0 && cube < 3.5, "{cube}");
        assert_eq!(cube, sizing.base());
    }

    #[test]
    fn thick_box_is_uniform() {
        let s = cuboid(V3::zeros(), V3::repeat(10.0));
        let sizing = Sizing::measure(&s, &bvh(&s), 2.0);
        assert!(!sizing.is_adaptive());
        assert!((sizing.at(&V3::new(5.0, 5.0, 0.0)) - 2.0).abs() < 1e-9);
    }
}
