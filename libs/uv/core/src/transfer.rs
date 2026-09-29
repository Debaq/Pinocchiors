//! Traspaso de UV de una superficie de referencia a otra malla (p. ej. la
//! retopologizada).

use crate::UvSurface;
use pinocchio_math::Vector3;
use rayon::prelude::*;

/// UV de cada esquina de cada cara de la malla destino.
#[derive(Debug, Clone, PartialEq)]
pub struct UvTransfer<const N: usize> {
    /// UV por cara y esquina, en el orden de los vértices de la cara.
    pub corners: Vec<[[f32; 2]; N]>,
    /// Grupo de la superficie del que cada cara toma su textura.
    pub groups: Vec<usize>,
    /// Caras que cruzan una costura del mapa original: alguna esquina toma su
    /// UV extrapolada desde el lado del centro, más allá de la costura por
    /// más de una décima del tamaño de la cara, así que la textura puede
    /// verse estirada o con astillas de otra isla ahí.
    pub seam_faces: usize,
}

/// Cuánto (en radios de la cara) puede pasar una esquina la costura sin
/// contar como cruce.
const SEAM_TOLERANCE: f64 = 0.1;

/// Lleva las UV de `surface` a las caras de `N` vértices de otra malla que
/// recubre la misma superficie.
///
/// Cada esquina toma la UV del triángulo más cercano alcanzable desde el
/// triángulo bajo el centro de la cara sin cruzar costuras. Si el punto más
/// cercano de verdad queda al otro lado de una costura, se extrapola el mapa
/// del triángulo de este lado: la textura sigue continua dentro de la cara en
/// vez de saltar a otra parte del atlas.
pub fn transfer_uvs<const N: usize>(
    surface: &UvSurface,
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
) -> UvTransfer<N> {
    let points: Vec<Vector3> = positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
    let hits: Vec<_> = points.par_iter().map(|p| surface.closest(p)).collect();

    let per_face: Vec<([[f32; 2]; N], usize, bool)> = faces
        .par_iter()
        .map(|face| {
            let center = face.iter().fold(Vector3::zero(), |acc, &v| acc + points[v]) * (1.0 / N as f64);
            let hit = surface.closest(&center);
            let mut overshoot: f64 = 0.0;
            let uvs = face.map(|v| {
                let p = &points[v];
                let own = hits[v];
                let radius = 2.0 * (p.distance(&center) + hit.distance) + own.distance;
                let (t, d) = surface.nearest_connected(hit.triangle, p, radius);
                let uv = if d <= own.distance + 1e-9 * (1.0 + radius) {
                    // Mismo punto que el más cercano (o empate en una arista)
                    surface.uv_at(t, &own.point)
                } else {
                    overshoot = overshoot.max(d - own.distance);
                    surface.uv_at(t, p)
                };
                uv.map(|c| c as f32)
            });
            // Una esquina que pasa la costura por una fracción de la cara
            // (vértice apenas corrido de ella) no se nota
            let size = face.iter().map(|&v| points[v].distance(&center)).fold(0.0, f64::max);
            (uvs, surface.group(hit.triangle), overshoot > SEAM_TOLERANCE * size)
        })
        .collect();

    let seam_faces = per_face.iter().filter(|f| f.2).count();
    let (corners, groups) = per_face.into_iter().map(|(uv, g, _)| (uv, g)).unzip();
    UvTransfer { corners, groups, seam_faces }
}

/// Isla del mapa de `surface` bajo el centro de cada cara: una arista entre
/// caras de islas distintas cae sobre una costura del original (ver
/// [`crate::unwrap_with_regions`]).
pub fn original_regions<const N: usize>(surface: &UvSurface, positions: &[[f64; 3]], faces: &[[usize; N]]) -> Vec<usize> {
    faces
        .par_iter()
        .map(|face| {
            let center = face.iter().fold(Vector3::zero(), |acc, &v| acc + Vector3::new(positions[v][0], positions[v][1], positions[v][2]))
                * (1.0 / N as f64);
            surface.chart(surface.closest(&center).triangle)
        })
        .collect()
}
