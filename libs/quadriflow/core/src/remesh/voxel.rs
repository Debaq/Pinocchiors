//! Vóxeles: rehace la superficie desde el volumen que encierra (la
//! reconstrucción que la retopología usa con las mallas rotas, ver
//! `crate::rebuild`).
//!
//! Cierra agujeros, une las piezas que se cruzan y descarta las caras
//! internas: la salida es siempre cerrada y manifold. Los rasgos más chicos
//! que un vóxel se pierden y las aristas vivas quedan redondeadas a esa
//! escala.

use super::{smooth, SmoothOptions, TriMesh};
use crate::{RemeshError, V3};

/// Cómo reconstruir.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoxelOptions {
    /// Lado del vóxel, en las unidades de la malla (se ajusta a
    /// [`VoxelGrid::voxel_size`]).
    pub voxel_size: f64,
    /// Pasadas de suavizado después (Taubin; 0: ninguna).
    pub smooth_iterations: usize,
    /// Remallado isótropo después, con este lado (`None`: los triángulos de
    /// la grilla, de tamaños dispares).
    pub isotropic_edge: Option<f64>,
}

/// Vóxeles en el eje más largo, a lo sumo.
pub const MAX_CELLS: f64 = 640.0;

/// La grilla que usaría la reconstrucción, para avisar antes de correrla.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoxelGrid {
    /// Lado del vóxel que se usa de verdad (el pedido, dentro de los límites).
    pub voxel_size: f64,
    /// Puntos de la grilla por eje.
    pub dims: [usize; 3],
    /// Triángulos aproximados del resultado (antes del isótropo).
    pub triangles: usize,
    /// Memoria aproximada que hace falta, en bytes.
    pub memory: u64,
}

/// La grilla para vóxeles de lado `voxel_size`.
pub fn voxel_grid(positions: &[[f32; 3]], indices: &[u32], voxel_size: f64) -> VoxelGrid {
    let (lo, hi) = bounds(positions);
    let size = [0, 1, 2].map(|k| hi[k] - lo[k]);
    let extent = size.iter().copied().fold(0.0, f64::max);
    let h = if extent > 0.0 { crate::rebuild::voxel_size(extent, voxel_size, MAX_CELLS) } else { voxel_size };
    let dims = crate::rebuild::grid_dims(size, h);
    let area: f64 = indices
        .chunks_exact(3)
        .filter(|t| t.iter().all(|&i| (i as usize) < positions.len()))
        .map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| V3::from(positions[i as usize].map(f64::from)));
            (b - a).cross(&(c - a)).norm() / 2.0
        })
        .sum();
    let triangles = (TRIANGLES_PER_AREA * area / (h * h)) as usize;
    let points: u64 = dims.iter().map(|&d| d as u64).product();
    VoxelGrid { voxel_size: h, dims, triangles, memory: points * BYTES_PER_POINT + triangles as u64 * BYTES_PER_TRIANGLE }
}

/// Triángulos de la salida por vóxel² de superficie (medido)
const TRIANGLES_PER_AREA: f64 = 5.5;
/// Memoria por punto de la grilla y por triángulo de la salida (medido)
const BYTES_PER_POINT: u64 = 1;
const BYTES_PER_TRIANGLE: u64 = 180;

/// Reconstruye la superficie cerrada que encierra la malla.
pub fn voxel(positions: &[[f32; 3]], indices: &[u32], options: &VoxelOptions) -> Result<TriMesh, RemeshError> {
    if !(options.voxel_size.is_finite() && options.voxel_size > 0.0) {
        return Err(RemeshError::InvalidConfig("el tamaño de vóxel debe ser positivo".into()));
    }
    let surface = super::surface_of(positions, indices)?;
    let diagonal = surface.bbox_diagonal();
    let mut rebuilt = crate::rebuild::rebuild_with_limit(&surface, options.voxel_size, MAX_CELLS);
    rebuilt.weld(diagonal * 1e-7);
    if rebuilt.triangles.is_empty() {
        return Err(RemeshError::EmptyMesh);
    }
    let mut mesh = TriMesh::from_surface(&rebuilt);
    if options.smooth_iterations > 0 {
        let smoothing = SmoothOptions { iterations: options.smooth_iterations, normal_only: false, ..Default::default() };
        mesh.positions = smooth(&mesh.positions, &mesh.indices, &smoothing);
    }
    if let Some(edge) = options.isotropic_edge {
        let iso = super::IsotropicOptions { edge_length: edge, sharp_angle: None, ..Default::default() };
        mesh = super::isotropic(&mesh.positions, &mesh.indices, &iso)?;
    }
    Ok(mesh)
}

fn bounds(positions: &[[f32; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(p[k]));
            hi[k] = hi[k].max(f64::from(p[k]));
        }
    }
    if positions.is_empty() { ([0.0; 3], [0.0; 3]) } else { (lo, hi) }
}
