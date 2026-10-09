//! Remallar: otras formas de ordenar una malla además de la retopología
//! (ver `PLAN_REMALLAR.md`).
//!
//! - [`simplify`]: quita triángulos conservando la forma, las costuras de UV y
//!   los atributos de los vértices que quedan (feature `simplify`).
//! - [`smooth`]: empareja los vértices sin cambiar la conectividad ni
//!   encoger (Taubin).
//! - [`isotropic`]: triángulos parejos del lado pedido.
//! - [`voxel`]: rehace la superficie desde el volumen (cerrada y manifold).
//! - [`deviation`]: cuánto se aleja una malla de otra, para el "antes →
//!   después" de cualquier modo.

pub mod deviation;
pub mod isotropic;
pub mod voxel;
#[cfg(feature = "simplify")]
pub mod simplify;
pub mod smooth;

pub use deviation::{deviation, Deviation};
#[cfg(feature = "simplify")]
pub use simplify::{simplify, SimplifyInput, SimplifyOptions, Simplified};
pub use smooth::{smooth, SmoothOptions};
pub use isotropic::{isotropic, IsotropicOptions};
pub use voxel::{voxel, voxel_grid, VoxelGrid, VoxelOptions};

use crate::surface::Surface;
use crate::{RemeshError, V3};

/// Malla de triángulos sin atributos (lo que devuelven los modos que hacen
/// una malla nueva).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    /// Triángulos (3 índices cada uno).
    pub indices: Vec<u32>,
}

impl TriMesh {
    pub(crate) fn from_surface(surface: &Surface) -> Self {
        Self {
            positions: surface.positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect(),
            indices: surface.triangles.iter().flatten().copied().collect(),
        }
    }
}

/// La superficie soldada (los vértices a menos de una diez millonésima del
/// tamaño se funden), sin triángulos degenerados.
pub(crate) fn surface_of(positions: &[[f32; 3]], indices: &[u32]) -> Result<Surface, RemeshError> {
    let triangles: Vec<[u32; 3]> = indices
        .chunks_exact(3)
        .filter(|t| t.iter().all(|&i| (i as usize) < positions.len()))
        .map(|t| [t[0], t[1], t[2]])
        .collect();
    let mut surface = Surface::new(positions.iter().map(|p| V3::from(p.map(f64::from))).collect(), triangles);
    let diagonal = surface.bbox_diagonal();
    surface.weld(diagonal * 1e-7);
    let area = surface.area();
    if surface.triangles.is_empty() || !(area > 0.0) {
        return Err(RemeshError::EmptyMesh);
    }
    Ok(surface)
}

/// Lo que conviene saber de una malla antes de elegir parámetros.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SurfaceStats {
    pub area: f64,
    /// Largo medio de las aristas.
    pub mean_edge: f64,
    /// Lado más largo de la caja.
    pub extent: f64,
    /// Sin aristas de más de dos caras ni caras dadas vuelta (Isótropo la necesita así).
    pub manifold: bool,
}

/// Área, arista media, tamaño y si es manifold (tras soldar como los modos).
pub fn surface_stats(positions: &[[f32; 3]], indices: &[u32]) -> SurfaceStats {
    let Ok(surface) = surface_of(positions, indices) else { return SurfaceStats::default() };
    let (mut lo, mut hi) = (V3::repeat(f64::INFINITY), V3::repeat(f64::NEG_INFINITY));
    for p in &surface.positions {
        lo = lo.inf(p);
        hi = hi.sup(p);
    }
    SurfaceStats {
        area: surface.area(),
        mean_edge: surface.average_edge_length(),
        extent: (hi - lo).max(),
        manifold: crate::rebuild::is_manifold(&surface),
    }
}
