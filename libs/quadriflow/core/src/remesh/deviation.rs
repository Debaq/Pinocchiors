//! Desviación entre dos mallas de triángulos.
//!
//! Se mide en los dos sentidos: los vértices de cada una contra la superficie
//! de la otra, y además los centros de los triángulos del resultado. Al
//! simplificar, los vértices que quedan están sobre el original (distancia
//! cero); lo que se pierde se ve desde los vértices del original que ya no
//! están, y lo que se agrega, desde el interior de los triángulos nuevos.

use crate::V3;
use pinocchio_math::Vector3;
use pinocchio_spatial::{Bvh, Triangle};
use rayon::prelude::*;

/// Cuánto se aleja una malla de otra.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Deviation {
    /// Distancia máxima, en las unidades de las mallas.
    pub max: f64,
    /// Distancia media de los puntos medidos.
    pub mean: f64,
    /// Diagonal de la caja envolvente del original (para expresarla en %).
    pub diagonal: f64,
}

impl Deviation {
    /// Máxima en porcentaje de la diagonal del original.
    pub fn max_percent(&self) -> f64 {
        percent(self.max, self.diagonal)
    }

    /// Media en porcentaje de la diagonal del original.
    pub fn mean_percent(&self) -> f64 {
        percent(self.mean, self.diagonal)
    }
}

fn percent(value: f64, diagonal: f64) -> f64 {
    if diagonal > 0.0 { 100.0 * value / diagonal } else { 0.0 }
}

/// Desviación de `result` respecto de `original` (vértices y triángulos de cada una).
pub fn deviation(
    original: (&[[f64; 3]], &[[usize; 3]]),
    result: (&[[f64; 3]], &[[usize; 3]]),
) -> Deviation {
    Reference::new(original).measure(result)
}

/// Una malla original lista para medir varios resultados contra ella (su
/// BVH se arma una sola vez).
pub struct Reference<'a> {
    positions: &'a [[f64; 3]],
    bvh: Option<Bvh>,
    /// Vértices usados (una malla puede traer vértices sueltos)
    used: Vec<usize>,
    diagonal: f64,
}

impl<'a> Reference<'a> {
    pub fn new(original: (&'a [[f64; 3]], &[[usize; 3]])) -> Self {
        let diagonal = bounds_diagonal(original.0);
        let bvh = (!original.1.is_empty()).then(|| bvh(original));
        Self { positions: original.0, bvh, used: used_vertices(original), diagonal }
    }

    pub fn measure(&self, result: (&[[f64; 3]], &[[usize; 3]])) -> Deviation {
        let Some(original_bvh) = &self.bvh else {
            return Deviation { diagonal: self.diagonal, ..Default::default() };
        };
        if result.1.is_empty() {
            return Deviation { diagonal: self.diagonal, ..Default::default() };
        }
        let result_bvh = bvh(result);
        let centers: Vec<[f64; 3]> = result
            .1
            .iter()
            .map(|t| {
                let c = t.iter().map(|&i| V3::from(result.0[i])).sum::<V3>() / 3.0;
                [c.x, c.y, c.z]
            })
            .collect();
        let added = used_vertices(result);
        let (a, b) = rayon::join(
            || distances(self.used.par_iter().map(|&i| self.positions[i]), &result_bvh),
            || distances(added.par_iter().map(|&i| result.0[i]).chain(centers.into_par_iter()), original_bvh),
        );
        let count = (a.2 + b.2).max(1) as f64;
        Deviation { max: a.0.max(b.0), mean: (a.1 + b.1) / count, diagonal: self.diagonal }
    }
}

fn bounds_diagonal(positions: &[[f64; 3]]) -> f64 {
    if positions.is_empty() {
        return 0.0;
    }
    let (lo, hi) = positions.iter().fold(
        (V3::repeat(f64::INFINITY), V3::repeat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.inf(&V3::from(*p)), hi.sup(&V3::from(*p))),
    );
    (hi - lo).norm()
}

fn bvh((positions, triangles): (&[[f64; 3]], &[[usize; 3]])) -> Bvh {
    Bvh::build(
        triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| Vector3(V3::from(positions[i])));
                Triangle::new(a, b, c)
            })
            .collect(),
    )
}

fn used_vertices((positions, triangles): (&[[f64; 3]], &[[usize; 3]])) -> Vec<usize> {
    let mut used = vec![false; positions.len()];
    triangles.iter().flatten().for_each(|&i| used[i] = true);
    (0..positions.len()).filter(|&i| used[i]).collect()
}

/// (máxima, suma, cantidad) de las distancias de los puntos a la superficie
fn distances(points: impl ParallelIterator<Item = [f64; 3]>, bvh: &Bvh) -> (f64, f64, usize) {
    points
        .map(|p| {
            let d = bvh.query_distance(&Vector3(V3::from(p)));
            (d, d, 1)
        })
        .reduce(|| (0.0, 0.0, 0), |a, b| (a.0.max(b.0), a.1 + b.1, a.2 + b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f64) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        (
            vec![[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }

    #[test]
    fn same_mesh_has_no_deviation() {
        let (p, t) = square(0.0);
        let d = deviation((&p, &t), (&p, &t));
        assert!(d.max < 1e-12);
        assert!((d.diagonal - 2f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn shifted_mesh_deviates_by_the_shift() {
        let (p, t) = square(0.0);
        let (q, s) = square(0.1);
        let d = deviation((&p, &t), (&q, &s));
        assert!((d.max - 0.1).abs() < 1e-9, "{d:?}");
        assert!((d.mean - 0.1).abs() < 1e-9, "{d:?}");
        assert!((d.max_percent() - 10.0 / 2f64.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn lost_vertex_is_seen_from_the_original() {
        // Pirámide baja: el vértice de arriba se pierde al aplanarla
        let mut p = square(0.0).0;
        p.push([0.5, 0.5, 0.3]);
        let t = vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
        let (q, s) = square(0.0);
        let d = deviation((&p, &t), (&q, &s));
        assert!((d.max - 0.3).abs() < 1e-9, "{d:?}");
    }
}
