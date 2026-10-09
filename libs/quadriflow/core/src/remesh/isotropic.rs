//! Isótropo: triángulos parejos, todos del lado pedido (el remallado de
//! Botsch y Kobbelt que la retopología usa por dentro, ver `crate::isotropic`).
//!
//! Las aristas vivas y los bordes se conservan como aristas; los vértices
//! nuevos se reproyectan sobre la superficie original.

use super::TriMesh;
use crate::surface::Surface;
use crate::{RemeshError, V3};

/// Cómo remallar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IsotropicOptions {
    /// Lado de los triángulos, en las unidades de la malla.
    pub edge_length: f64,
    /// Ángulo entre caras (grados) desde el que una arista es viva y se
    /// conserva (`None`: todo se redondea a la escala de los triángulos).
    pub sharp_angle: Option<f64>,
    /// Pasadas de partir, colapsar, voltear y relajar.
    pub iterations: usize,
    /// Triángulos más chicos en las partes delgadas (paredes, puntas, ranuras),
    /// con la misma cantidad total.
    pub thin_features: bool,
}

impl Default for IsotropicOptions {
    fn default() -> Self {
        Self { edge_length: 1.0, sharp_angle: Some(45.0), iterations: 5, thin_features: false }
    }
}

/// Triángulos de más: el resultado no entraría en memoria ni en el visor.
pub const MAX_TRIANGLES: usize = 6_000_000;

/// Triángulos equiláteros de lado `edge` que caben en `area`.
pub fn estimated_triangles(area: f64, edge: f64) -> usize {
    if edge > 0.0 { (area / (3f64.sqrt() / 4.0 * edge * edge)).round() as usize } else { usize::MAX }
}

/// Remalla con triángulos parejos. Necesita una malla sin aristas de más de
/// dos caras ni caras dadas vuelta (si no, Vóxeles).
pub fn isotropic(positions: &[[f32; 3]], indices: &[u32], options: &IsotropicOptions) -> Result<TriMesh, RemeshError> {
    let surface = super::surface_of(positions, indices)?;
    if !crate::rebuild::is_manifold(&surface) {
        return Err(RemeshError::NonManifold);
    }
    let edge = options.edge_length;
    if !(edge.is_finite() && edge > 0.0) {
        return Err(RemeshError::InvalidConfig("el lado de los triángulos debe ser positivo".into()));
    }
    let estimate = estimated_triangles(surface.area(), edge);
    if estimate > MAX_TRIANGLES {
        return Err(RemeshError::TooDense(estimate));
    }
    Ok(TriMesh::from_surface(&remesh_surface(&surface, options)))
}

pub(crate) fn remesh_surface(surface: &Surface, options: &IsotropicOptions) -> Surface {
    let edge = options.edge_length;
    let sharp = options.sharp_angle.map(f64::to_radians);
    let iterations = options.iterations.max(1);
    if options.thin_features {
        let bvh = crate::surface_bvh(surface);
        let sizing = crate::sizing::Sizing::measure(surface, &bvh, edge);
        if sizing.is_adaptive() {
            let target = |p: &V3| sizing.at(p);
            return crate::isotropic::remesh(surface, &target, sizing.base(), sharp, iterations);
        }
    }
    crate::isotropic::remesh(surface, &|_: &V3| edge, edge, sharp, iterations)
}
