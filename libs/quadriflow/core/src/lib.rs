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
mod isotropic;
mod integer;
mod quad;
mod rebuild;
mod sizing;
mod smooth;
mod symmetry;
mod surface;

pub use config::{Rebuild, RemeshConfig, Symmetry};
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
    let (surface, rebuild) = prepare(mesh, config, &mut on_progress)?;
    let quads = match config.symmetry.axis() {
        None => remesh_surface(surface, rebuild, config, config.target_faces, &mut on_progress)?,
        Some(axis) => {
            let center = symmetry::center(&surface, axis);
            let half = symmetry::clip(&surface, axis, center);
            if half.triangles.is_empty() {
                return Err(RemeshError::EmptyMesh);
            }
            let tolerance = surface.bbox_diagonal() * 1e-6;
            let target = config.target_faces.div_ceil(2);
            let half = remesh_surface(half, rebuild, config, target, &mut on_progress)?;
            symmetry::mirror(&half, axis, center, tolerance)
        }
    };

    on_progress(
        RemeshStage::Done,
        &format!("{} vértices, {} quads", quads.num_vertices(), quads.num_faces()),
    );
    Ok(quads)
}

/// Soldado de costuras y, si hace falta, reconstrucción. Devuelve la
/// superficie y si se reconstruyó.
fn prepare<F>(mesh: &Mesh, config: &RemeshConfig, on_progress: &mut F) -> Result<(Surface, bool), RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
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
    Ok((surface, rebuild))
}

/// Retopologiza una superficie ya preparada con `target_faces` quads.
fn remesh_surface<F>(
    mut surface: Surface,
    rebuild: bool,
    config: &RemeshConfig,
    target_faces: usize,
    on_progress: &mut F,
) -> Result<QuadMesh, RemeshError>
where
    F: FnMut(RemeshStage, &str),
{
    let area = surface.area();
    let original = surface.clone();
    let bvh = surface_bvh(&original);

    // Se extrae con el doble de lado y cada polígono se divide en ~4 quads
    let scale = 2.0 * (area / target_faces as f64).sqrt();
    // Densidad adaptativa: escala local no mayor que el grosor de la pieza
    let mut sizing = sizing::Sizing::uniform(scale);
    if config.adaptive_density {
        let measured = sizing::Sizing::measure(&original, &bvh, scale);
        if measured.is_adaptive() {
            sizing = measured;
        }
    }
    if rebuild::is_manifold(&surface) {
        // Triángulos uniformes: el campo no hereda el muestreo de la entrada.
        // Las aristas vivas se conservan siempre (la geometría no se redondea);
        // `preserve_sharp` decide solo si los quads se alinean a ellas
        let features = (!rebuild).then_some(config.sharp_angle);
        let target = |p: &V3| sizing.at(p) * ISOTROPIC_EDGE;
        let cell = sizing.base() * ISOTROPIC_EDGE;
        surface = isotropic::remesh(&surface, &target, cell, features, ISOTROPIC_ITERATIONS);
    } else {
        let max_edge = (scale * 0.5).min(surface.average_edge_length() * 2.0);
        surface.subdivide(max_edge);
    }
    // Las esquinas de una superficie reconstruida están redondeadas a escala de
    // vóxel: su ángulo diedro no distingue aristas vivas del escalonado
    let sharp = (config.preserve_sharp && !rebuild).then_some(config.sharp_angle);
    let graph = surface.vertex_graph(sharp, config.sharp_angle);

    on_progress(RemeshStage::Hierarchy, "Construyendo la jerarquía...");
    let hierarchy = hierarchy::Hierarchy::build(&graph);

    let iterations = config.smooth_iterations.max(1);
    on_progress(RemeshStage::OrientationField, "Calculando el campo de orientación...");
    let strength = config.curvature_alignment;
    let guide = (strength > 0.0).then(|| field::curvature_guide(&hierarchy.levels[0], sizing.base(), strength));
    let orientation = field::solve_orientation(&hierarchy, iterations, guide);

    let mut polygons = extract_polygons(&surface, &hierarchy, &orientation, &sizing, iterations, on_progress);
    // Cada polígono de n lados termina en n quads: si la cantidad se desvía
    // del objetivo, se ajusta la escala base y se extrae de nuevo
    let ratio_of = |p: &extract::Polygons| p.faces.iter().map(Vec::len).sum::<usize>() as f64 / target_faces as f64;
    let (mut base, mut ratio) = (sizing.base(), ratio_of(&polygons));
    let mut trial = (base, ratio);
    for _ in 0..COUNT_CORRECTIONS {
        if trial.1 == 0.0 || (trial.1 - 1.0).abs() <= COUNT_TOLERANCE {
            break;
        }
        sizing.set_base(trial.0 * trial.1.sqrt());
        let next = extract_polygons(&surface, &hierarchy, &orientation, &sizing, iterations, &mut |_, _| {});
        trial = (sizing.base(), ratio_of(&next));
        if (trial.1 - 1.0).abs() < (ratio - 1.0).abs() {
            (base, ratio, polygons) = (trial.0, trial.1, next);
        }
    }
    sizing.set_base(base);
    let (mut quads, mut fixed) = extract::polygons_to_quads(&polygons);
    if quads.is_empty() {
        return Err(RemeshError::ExtractionFailed);
    }
    quads.split_nonmanifold_vertices();
    fixed.resize(quads.vertices.len(), true);
    project_to_surface(&mut quads.vertices, &bvh);
    let lines = features::FeatureLines::new(&graph, sizing.base());
    snap_to_features(&mut quads, &fixed, &lines, |p| MIN_SNAP_DISTANCE * sizing.at(p) * 0.5);
    smooth::optimize(&mut quads, &fixed, &bvh, &lines, config.sharp_angle, SMOOTH_ITERATIONS);
    // Restos de la extracción (burbujas en pellizcos): nunca más piezas que la entrada
    quads.keep_largest_components(original.component_count());
    Ok(quads)
}

/// Lado de los triángulos del remallado isótropo, en unidades de `scale` (el
/// doble del lado de un quad final).
const ISOTROPIC_EDGE: f64 = 1.0 / 3.0;
/// Iteraciones del remallado isótropo.
const ISOTROPIC_ITERATIONS: usize = 5;

/// Vóxeles por lado de quad en la reconstrucción.
const REBUILD_VOXELS_PER_QUAD: f64 = 3.0;

/// Pasadas de optimización geométrica de la malla final.
const SMOOTH_ITERATIONS: usize = 10;

/// Desvío relativo de la cantidad de quads a partir del cual se corrige la
/// escala con otra extracción (a lo sumo `COUNT_CORRECTIONS` veces; queda la
/// más cercana al objetivo).
const COUNT_TOLERANCE: f64 = 0.08;
const COUNT_CORRECTIONS: usize = 2;

/// Campo de posición, desplazamientos enteros, extracción y limpieza con la
/// escala local de `sizing`.
fn extract_polygons<F>(
    surface: &Surface,
    hierarchy: &hierarchy::Hierarchy,
    orientation: &[Vec<V3>],
    sizing: &sizing::Sizing,
    iterations: usize,
    on_progress: &mut F,
) -> extract::Polygons
where
    F: FnMut(RemeshStage, &str),
{
    on_progress(RemeshStage::PositionField, "Calculando el campo de posición...");
    let scales = field::level_scales(hierarchy, |p| sizing.at(p));
    let position = field::solve_position(hierarchy, orientation, &scales, iterations);

    on_progress(RemeshStage::Singularities, "Eliminando singularidades de posición...");
    let mut offsets = integer::EdgeOffsets::compute(&hierarchy.levels[0], &orientation[0], &position, &scales[0]);
    integer::remove_position_singularities(&mut offsets, &surface.triangles);

    on_progress(RemeshStage::Extraction, "Extrayendo quads...");
    let mut polygons = extract::extract_polygons(&hierarchy.levels[0], &offsets, &position, &surface.triangles);
    cleanup::simplify(&mut polygons);
    polygons
}

/// Distancia mínima (en lados de quad) entre un vértice llevado a una arista
/// viva y los demás vértices de sus quads.
const MIN_SNAP_DISTANCE: f64 = 0.2;

/// Lleva los vértices fijos a la arista viva o borde más cercano, salvo que
/// quedaran encima de otro vértice fijo de sus quads: pasa con los puntos
/// medios entre dos líneas vivas distintas cuando una cara es más angosta que
/// un quad. (Los vértices libres cercanos los separa la relajación.)
fn snap_to_features(
    quads: &mut QuadMesh,
    fixed: &[bool],
    lines: &features::FeatureLines,
    min_distance: impl Fn(&V3) -> f64,
) {
    let mut faces_of: Vec<Vec<usize>> = vec![Vec::new(); quads.vertices.len()];
    for (f, face) in quads.faces.iter().enumerate() {
        for &v in &face.v {
            faces_of[v].push(f);
        }
    }
    for v in (0..quads.vertices.len()).filter(|&v| fixed[v]) {
        let Some(p) = lines.closest(&quads.vertices[v]) else { continue };
        let crowded = faces_of[v]
            .iter()
            .flat_map(|&f| quads.faces[f].v)
            .any(|u| u != v && fixed[u] && (quads.vertices[u] - p).norm() < min_distance(&p));
        if !crowded {
            quads.vertices[v] = p;
        }
    }
}

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
