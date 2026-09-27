//! # quadriflow-core
//!
//! Retopología: convierte una malla de triángulos en una malla de quads
//! alineada a la forma.
//!
//! ## Uso
//!
//! ```no_run
//! use quadriflow_core::{remesh, RemeshConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input_mesh = pinocchio_mesh::load_obj("model.obj")?;
//! let config = RemeshConfig {
//!     target_faces: 5000,
//!     ..Default::default()
//! };
//! let quad_mesh = remesh(&input_mesh, &config)?;
//! println!("{} quads", quad_mesh.faces.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Algoritmo
//!
//! Sigue *Instant Field-Aligned Meshes* (Jakob et al. 2015), la base de
//! QuadriFlow (Huang et al. 2018):
//!
//! 1. **Preparación**: soldado de costuras; si la malla está rota (aristas
//!    no-manifold, cáscaras superpuestas) se reconstruye por vóxeles; luego
//!    subdivisión hasta que las aristas
//!    sean más cortas que medio quad; se marcan bordes y aristas vivas.
//! 2. **Jerarquía**: niveles cada vez más gruesos emparejando vértices.
//! 3. **Campo de orientación** 4-RoSy, suavizado de grueso a fino.
//! 4. **Campo de posición** 4-PoSy: un retículo local por vértice.
//! 5. **Singularidades de posición** (QuadriFlow): los desplazamientos enteros
//!    entre retículos vecinos se corrigen para que sumen cero alrededor de
//!    cada triángulo regular, sin invertir ninguno.
//! 6. **Extracción**: se funden los vértices del mismo punto del retículo, los
//!    triángulos que sobreviven (medios quads) se emparejan por su diagonal.
//! 7. **Limpieza**: operaciones locales sobre la malla poligonal (fundir caras,
//!    disolver *doublets*, colapsar diagonales y aristas degeneradas) mientras
//!    bajen la irregularidad; cada polígono se divide en quads, que se
//!    proyectan sobre la superficie.

pub mod config;
mod cleanup;
mod extract;
mod features;
mod field;
mod hierarchy;
mod integer;
mod quad;
mod rebuild;
mod surface;

pub use config::{Rebuild, RemeshConfig};
pub use quad::{QuadFace, QuadMesh, QuadTopology};

use pinocchio_mesh::Mesh;
use pinocchio_spatial::{Bvh, Triangle};
use surface::Surface;
use thiserror::Error;

pub(crate) type V3 = pinocchio_math::nalgebra::Vector3<f64>;

/// Errores de la retopología.
#[derive(Error, Debug)]
pub enum RemeshError {
    #[error("la malla de entrada no tiene triángulos")]
    EmptyMesh,
    #[error("configuración inválida: {0}")]
    InvalidConfig(String),
    #[error("no se pudo extraer ningún quad; probar con más quads objetivo")]
    ExtractionFailed,
}

/// Etapa del proceso, para reportar progreso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemeshStage {
    /// Soldado, subdivisión y detección de bordes
    Preprocess,
    /// Reconstrucción por vóxeles de una entrada rota (solo si hace falta)
    Rebuild,
    /// Construcción de la jerarquía multiresolución
    Hierarchy,
    /// Campo de orientación
    OrientationField,
    /// Campo de posición
    PositionField,
    /// Eliminación de singularidades de posición
    Singularities,
    /// Extracción de quads
    Extraction,
    /// Terminado
    Done,
}

impl RemeshStage {
    /// Identificador de la etapa.
    pub fn name(&self) -> &'static str {
        match self {
            RemeshStage::Preprocess => "preprocess",
            RemeshStage::Rebuild => "rebuild",
            RemeshStage::Hierarchy => "hierarchy",
            RemeshStage::OrientationField => "orientation_field",
            RemeshStage::PositionField => "position_field",
            RemeshStage::Singularities => "singularities",
            RemeshStage::Extraction => "extraction",
            RemeshStage::Done => "done",
        }
    }

    /// Porcentaje aproximado al comenzar la etapa.
    pub fn progress(&self) -> u32 {
        match self {
            RemeshStage::Preprocess => 2,
            RemeshStage::Rebuild => 5,
            RemeshStage::Hierarchy => 15,
            RemeshStage::OrientationField => 25,
            RemeshStage::PositionField => 50,
            RemeshStage::Singularities => 75,
            RemeshStage::Extraction => 85,
            RemeshStage::Done => 100,
        }
    }
}

/// Retopologiza una malla de triángulos a una malla de quads con
/// aproximadamente `config.target_faces` caras.
pub fn remesh(mesh: &Mesh, config: &RemeshConfig) -> Result<QuadMesh, RemeshError> {
    remesh_with_callback(mesh, config, |_, _| {})
}

/// Igual que [`remesh`], llamando a `on_progress(etapa, mensaje)` al comenzar
/// cada etapa.
pub fn remesh_with_callback<F>(
    mesh: &Mesh,
    config: &RemeshConfig,
    mut on_progress: F,
) -> Result<QuadMesh, RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
    if config.target_faces == 0 {
        return Err(RemeshError::InvalidConfig("target_faces debe ser mayor que 0".into()));
    }
    if !(config.sharp_angle.is_finite() && config.sharp_angle > 0.0) {
        return Err(RemeshError::InvalidConfig("sharp_angle debe ser positivo".into()));
    }

    on_progress(RemeshStage::Preprocess, "Preparando la superficie...");
    let mut surface = Surface::from_mesh(mesh);
    let diagonal = surface.bbox_diagonal();
    surface.weld(diagonal * 1e-7);
    let mut area = surface.area();
    if surface.triangles.is_empty() || area.is_nan() || area <= 0.0 {
        return Err(RemeshError::EmptyMesh);
    }
    let rebuild = match config.rebuild {
        Rebuild::Auto => rebuild::needs_rebuild(&surface),
        Rebuild::Always => true,
        Rebuild::Never => false,
    };
    if rebuild {
        on_progress(RemeshStage::Rebuild, "Reconstruyendo la superficie...");
        let quad_edge = (area / config.target_faces as f64).sqrt();
        surface = rebuild::rebuild(&surface, quad_edge / REBUILD_VOXELS_PER_QUAD);
        surface.weld(diagonal * 1e-7);
        area = surface.area();
        if surface.triangles.is_empty() || area.is_nan() || area <= 0.0 {
            return Err(RemeshError::EmptyMesh);
        }
    }
    let original = surface.clone();

    // Se extrae con el doble de lado y cada polígono se divide en ~4 quads
    let scale = 2.0 * (area / config.target_faces as f64).sqrt();
    let max_edge = (scale * 0.5).min(surface.average_edge_length() * 2.0);
    surface.subdivide(max_edge);
    // Las esquinas de una superficie reconstruida están redondeadas a escala de
    // vóxel: su ángulo diedro no distingue aristas vivas del escalonado
    let sharp = (config.preserve_sharp && !rebuild).then_some(config.sharp_angle);
    let graph = surface.vertex_graph(sharp, config.sharp_angle);

    on_progress(RemeshStage::Hierarchy, "Construyendo la jerarquía...");
    let hierarchy = hierarchy::Hierarchy::build(&graph);

    let iterations = config.smooth_iterations.max(1);
    on_progress(RemeshStage::OrientationField, "Calculando el campo de orientación...");
    let orientation = field::solve_orientation(&hierarchy, iterations);

    on_progress(RemeshStage::PositionField, "Calculando el campo de posición...");
    let position = field::solve_position(&hierarchy, &orientation, scale, iterations);

    on_progress(RemeshStage::Singularities, "Eliminando singularidades de posición...");
    let mut offsets = integer::EdgeOffsets::compute(&hierarchy.levels[0], &orientation[0], &position, scale);
    integer::remove_position_singularities(&mut offsets, &surface.triangles);

    on_progress(RemeshStage::Extraction, "Extrayendo quads...");
    let mut polygons = extract::extract_polygons(&hierarchy.levels[0], &offsets, &position, &surface.triangles);
    cleanup::simplify(&mut polygons);
    let (mut quads, mut fixed) = extract::polygons_to_quads(&polygons);
    if quads.is_empty() {
        return Err(RemeshError::ExtractionFailed);
    }
    quads.split_nonmanifold_vertices();
    fixed.resize(quads.vertices.len(), true);
    let bvh = surface_bvh(&original);
    project_to_surface(&mut quads.vertices, &bvh);
    let lines = features::FeatureLines::new(&graph, scale);
    for (v, _) in quads.vertices.iter_mut().zip(&fixed).filter(|(_, f)| **f) {
        if let Some(p) = lines.closest(v) {
            *v = p;
        }
    }
    relax(&mut quads, &fixed, &bvh, RELAX_ITERATIONS);
    // Restos de la extracción (burbujas en pellizcos): nunca más piezas que la entrada
    quads.keep_largest_components(original.component_count());

    on_progress(
        RemeshStage::Done,
        &format!("{} vértices, {} quads", quads.num_vertices(), quads.num_faces()),
    );
    Ok(quads)
}

/// Vóxeles por lado de quad en la reconstrucción.
const REBUILD_VOXELS_PER_QUAD: f64 = 3.0;

/// Iteraciones de relajación tangencial de la malla final.
const RELAX_ITERATIONS: usize = 3;

fn surface_bvh(surface: &Surface) -> Bvh {
    let triangles = surface
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| pinocchio_math::Vector3(surface.positions[i as usize]));
            Triangle::new(a, b, c)
        })
        .collect();
    Bvh::build(triangles)
}

/// Lleva cada vértice al punto más cercano de la superficie original.
fn project_to_surface(vertices: &mut [V3], bvh: &Bvh) {
    for v in vertices {
        if let Some(hit) = bvh.query_closest(&pinocchio_math::Vector3(*v)) {
            *v = hit.point.0;
        }
    }
}

/// Suaviza la distribución de vértices sobre la superficie: cada vértice libre
/// va al promedio de sus vecinos y se reproyecta. Los vértices sobre bordes,
/// aristas vivas o el contorno de la malla de quads no se mueven.
fn relax(quads: &mut QuadMesh, fixed: &[bool], bvh: &Bvh, iterations: usize) {
    let n = quads.vertices.len();
    let mut edge_count: std::collections::HashMap<(usize, usize), u32> = Default::default();
    for f in &quads.faces {
        for k in 0..4 {
            let (a, b) = (f.v[k], f.v[(k + 1) % 4]);
            *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pinned = fixed.to_vec();
    let mut edges: Vec<_> = edge_count.into_iter().collect();
    edges.sort_unstable();
    for ((a, b), count) in edges {
        neighbors[a].push(b);
        neighbors[b].push(a);
        if count != 2 {
            pinned[a] = true;
            pinned[b] = true;
        }
    }

    let free: Vec<usize> = (0..n).filter(|&i| !pinned[i] && !neighbors[i].is_empty()).collect();
    for _ in 0..iterations {
        let mut moved: Vec<V3> = free
            .iter()
            .map(|&i| {
                neighbors[i].iter().map(|&j| quads.vertices[j]).sum::<V3>() / neighbors[i].len() as f64
            })
            .collect();
        project_to_surface(&mut moved, bvh);
        for (&i, p) in free.iter().zip(moved) {
            quads.vertices[i] = p;
        }
    }
}
