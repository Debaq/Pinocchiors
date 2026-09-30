//! Desplegado completo: cartas → parametrización → empaquetado.

use crate::charts::{chart_faces, is_disk, segment, split, ChartOptions};
use crate::geometry::PolyMesh;
use crate::pack::{pack, pack_paintable, ChartShape, Paint};
use crate::param::{parametrize, ChartUv};
use crate::surface::uv_derivatives;
use pinocchio_math::Vector3;
use rayon::prelude::*;

/// Cómo se acomodan las cartas en el atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Lo más apretado posible: máxima resolución por texel (para exportar).
    #[default]
    Compact,
    /// Legible para pintar a mano: cartas más grandes, derechas (lo de
    /// arriba del modelo arriba en la imagen) y ubicadas donde están en el
    /// modelo, en dos paneles como una hoja de vistas (un lado y el otro).
    Paintable,
}

/// Opciones del desplegado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnwrapOptions {
    pub charts: ChartOptions,
    /// Estiramiento L2 máximo por carta; si lo supera (o tiene triángulos
    /// invertidos) la carta se parte en dos.
    pub max_stretch: f64,
    /// Iteraciones de ARAP tras LSCM.
    pub arap_iterations: usize,
    /// Tamaño de la textura destino (px), para medir el margen.
    pub texture_size: u32,
    /// Margen entre cartas (px).
    pub padding: u32,
    pub layout: Layout,
}

impl Default for UnwrapOptions {
    fn default() -> Self {
        Self {
            charts: ChartOptions::default(),
            max_stretch: 1.25,
            arap_iterations: 10,
            texture_size: 2048,
            padding: 4,
            layout: Layout::Compact,
        }
    }
}

/// Resultado del desplegado.
#[derive(Debug, Clone, PartialEq)]
pub struct Unwrap<const N: usize> {
    /// UV por cara y esquina, en [0, 1]².
    pub corners: Vec<[[f32; 2]; N]>,
    /// Carta de cada cara.
    pub chart_of_face: Vec<usize>,
    pub num_charts: usize,
    /// Estiramiento L2 medio (ponderado por área): 1 es isométrico.
    pub stretch: f64,
    /// Triángulos invertidos que quedaron (cartas de una cara que no se
    /// pueden partir más).
    pub flipped: usize,
    /// Fracción del cuadrado UV cubierta por cartas.
    pub coverage: f64,
}

/// Rondas máximas de partir cartas malas.
const MAX_SPLIT_ROUNDS: usize = 10;

/// Parte `faces` hasta que cada parte sea un disco.
fn into_disks(mesh: &PolyMesh, faces: Vec<usize>, old_seam_weight: f64) -> Vec<Vec<usize>> {
    let mut pending = vec![faces];
    let mut out = Vec::new();
    while let Some(faces) = pending.pop() {
        if faces.is_empty() {
            continue;
        }
        if is_disk(mesh, &faces) {
            out.push(faces);
            continue;
        }
        match split(mesh, &faces, old_seam_weight) {
            Some((a, b)) => pending.extend([a, b]),
            None => out.push(faces),
        }
    }
    out
}

/// Caja del modelo.
fn bounds(positions: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Dónde está la carta en el modelo y hacia dónde crecen en ella sus
/// coordenadas 3D: para acomodarla derecha y en su lugar.
fn paint_info(mesh: &PolyMesh, chart_faces: &[usize], uv: &ChartUv) -> Paint {
    let (mut center, mut normal, mut area) = (Vector3::zero(), Vector3::zero(), 0.0);
    // Gradiente de cada coordenada 3D en el plano UV de la carta
    let mut gradient = [[0.0f64; 2]; 3];
    for &f in chart_faces {
        center += mesh.centroids[f] * mesh.areas[f];
        normal += mesh.normals[f] * mesh.areas[f];
        area += mesh.areas[f];
        for tri in mesh.fan(f) {
            let p = tri.map(|v| mesh.points[v]);
            let t = tri.map(|v| uv.vertex_uv[&v]);
            let (dpdu, dpdv) = uv_derivatives(p, t);
            let weight = (p[1] - p[0]).cross(&(p[2] - p[0])).length();
            for (c, g) in gradient.iter_mut().enumerate() {
                let (du, dv) = match c {
                    0 => (dpdu.x(), dpdv.x()),
                    1 => (dpdu.y(), dpdv.y()),
                    _ => (dpdu.z(), dpdv.z()),
                };
                g[0] += weight * du;
                g[1] += weight * dv;
            }
        }
    }
    let center = center * (1.0 / area.max(f64::MIN_POSITIVE));
    Paint {
        center: [center.x(), center.y(), center.z()],
        normal: [normal.x(), normal.y(), normal.z()],
        gradient,
    }
}

/// Triángulos que cubren el polígono aunque no sea convexo: los abanicos
/// desde cada vértice (un quad queda cubierto por sus dos triangulaciones).
pub(crate) fn covering_triangles<const N: usize>(corners: [[f64; 2]; N]) -> Vec<[[f64; 2]; 3]> {
    let fans = if N == 3 { 1 } else { N };
    (0..fans)
        .flat_map(|a| (1..N - 1).map(move |k| [corners[a], corners[(a + k) % N], corners[(a + k + 1) % N]]))
        .collect()
}

/// Despliega una malla de caras de `N` vértices en un atlas UV.
pub fn unwrap<const N: usize>(positions: &[[f64; 3]], faces: &[[usize; N]], options: &UnwrapOptions) -> Unwrap<N> {
    unwrap_with_regions(positions, faces, None, options)
}

/// Como [`unwrap`], sabiendo en qué isla del mapa original cae cada cara
/// (`regions[f]`, ver [`crate::original_regions`]): las cartas nuevas
/// prefieren cortar donde cortaba el original, que suele esconder sus
/// costuras (ver [`ChartOptions::old_seam_weight`]).
pub fn unwrap_with_regions<const N: usize>(
    positions: &[[f64; 3]],
    faces: &[[usize; N]],
    regions: Option<&[usize]>,
    options: &UnwrapOptions,
) -> Unwrap<N> {
    let mesh = PolyMesh::new(positions, faces);
    let mesh = match regions {
        Some(regions) => mesh.with_regions(regions),
        None => mesh,
    };
    let mut options = *options;
    if options.layout == Layout::Paintable {
        // Cartas más grandes a cambio de algo de estiramiento: menos piezas
        // que reconocer (gonfoterio: 74 → 42)
        options.charts.max_angle = options.charts.max_angle.max(85.0);
        options.max_stretch = options.max_stretch.max(2.0);
    }
    let options = &options;
    let initial = chart_faces(&segment(&mesh, &options.charts));

    let mut accepted: Vec<(Vec<usize>, ChartUv)> = Vec::new();
    let mut pending = initial;
    for round in 0..=MAX_SPLIT_ROUNDS {
        if pending.is_empty() {
            break;
        }
        let results: Vec<(Vec<usize>, ChartUv)> = pending
            .into_par_iter()
            .map(|faces| {
                let uv = parametrize(&mesh, &faces, options.arap_iterations);
                (faces, uv)
            })
            .collect();
        pending = Vec::new();
        for (faces, uv) in results {
            let bad = uv.flipped > 0 || uv.stretch > options.max_stretch;
            let weight = options.charts.old_seam_weight;
            let parts = if bad && round < MAX_SPLIT_ROUNDS { split(&mesh, &faces, weight) } else { None };
            match parts {
                Some((a, b)) => {
                    pending.extend(into_disks(&mesh, a, weight));
                    pending.extend(into_disks(&mesh, b, weight));
                }
                None => accepted.push((faces, uv)),
            }
        }
    }
    // Orden estable: por la primera cara de cada carta
    accepted.sort_by_key(|(faces, _)| faces.iter().copied().min().unwrap_or(usize::MAX));

    let shapes: Vec<ChartShape> = accepted
        .iter()
        .map(|(chart_faces, uv)| ChartShape {
            triangles: chart_faces.iter().flat_map(|&f| covering_triangles(faces[f].map(|v| uv.vertex_uv[&v]))).collect(),
            area_3d: uv.area_3d,
            area_uv: uv.area_uv,
        })
        .collect();
    let (placements, coverage) = match options.layout {
        Layout::Compact => pack(&shapes, options.texture_size, options.padding),
        Layout::Paintable => {
            let paint: Vec<Paint> = accepted.iter().map(|(chart_faces, uv)| paint_info(&mesh, chart_faces, uv)).collect();
            pack_paintable(&shapes, &paint, bounds(positions), options.texture_size, options.padding)
        }
    };

    let mut chart_of_face = vec![0; faces.len()];
    let mut corners = vec![[[0.0f32; 2]; N]; faces.len()];
    for (chart, ((chart_faces, uv), placement)) in accepted.iter().zip(&placements).enumerate() {
        for &f in chart_faces {
            chart_of_face[f] = chart;
            corners[f] = faces[f].map(|v| placement.apply(uv.vertex_uv[&v]).map(|c| c as f32));
        }
    }

    let total_area: f64 = accepted.iter().map(|(_, uv)| uv.area_3d).sum();
    let stretch_sq: f64 = accepted.iter().map(|(_, uv)| uv.area_3d * uv.stretch * uv.stretch).sum();
    Unwrap {
        corners,
        chart_of_face,
        num_charts: accepted.len(),
        stretch: if total_area > 0.0 { (stretch_sq / total_area).sqrt() } else { 1.0 },
        flipped: accepted.iter().map(|(_, uv)| uv.flipped).sum(),
        coverage,
    }
}
