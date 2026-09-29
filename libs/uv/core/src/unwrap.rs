//! Desplegado completo: cartas → parametrización → empaquetado.

use crate::charts::{chart_faces, is_disk, segment, split, ChartOptions};
use crate::geometry::PolyMesh;
use crate::pack::{pack, ChartShape};
use crate::param::{parametrize, ChartUv};
use rayon::prelude::*;
use std::collections::HashMap;

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
}

impl Default for UnwrapOptions {
    fn default() -> Self {
        Self {
            charts: ChartOptions::default(),
            max_stretch: 1.25,
            arap_iterations: 10,
            texture_size: 2048,
            padding: 4,
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

/// Puntos UV de una carta y sus triángulos (índices a los puntos).
type ChartGeometry = (Vec<[f64; 2]>, Vec<[usize; 3]>);

/// Rondas máximas de partir cartas malas.
const MAX_SPLIT_ROUNDS: usize = 10;

/// Parte `faces` hasta que cada parte sea un disco.
fn into_disks(mesh: &PolyMesh, faces: Vec<usize>) -> Vec<Vec<usize>> {
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
        match split(mesh, &faces) {
            Some((a, b)) => pending.extend([a, b]),
            None => out.push(faces),
        }
    }
    out
}

/// Despliega una malla de caras de `N` vértices en un atlas UV.
pub fn unwrap<const N: usize>(positions: &[[f64; 3]], faces: &[[usize; N]], options: &UnwrapOptions) -> Unwrap<N> {
    let mesh = PolyMesh::new(positions, faces);
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
            let parts = if bad && round < MAX_SPLIT_ROUNDS { split(&mesh, &faces) } else { None };
            match parts {
                Some((a, b)) => {
                    pending.extend(into_disks(&mesh, a));
                    pending.extend(into_disks(&mesh, b));
                }
                None => accepted.push((faces, uv)),
            }
        }
    }
    // Orden estable: por la primera cara de cada carta
    accepted.sort_by_key(|(faces, _)| faces.iter().copied().min().unwrap_or(usize::MAX));

    // Puntos y triángulos de cada carta para el empaquetado
    let geometry: Vec<ChartGeometry> = accepted
        .iter()
        .map(|(chart_faces, uv)| {
            let mut index = HashMap::new();
            let mut points = Vec::new();
            let mut local = |v: usize| {
                *index.entry(v).or_insert_with(|| {
                    points.push(uv.vertex_uv[&v]);
                    points.len() - 1
                })
            };
            let triangles: Vec<[usize; 3]> =
                chart_faces.iter().flat_map(|&f| mesh.fan(f).collect::<Vec<_>>()).map(|t| t.map(&mut local)).collect();
            (points, triangles)
        })
        .collect();
    let shapes: Vec<ChartShape> = accepted
        .iter()
        .zip(&geometry)
        .map(|((_, uv), (points, triangles))| ChartShape { points, triangles, area_3d: uv.area_3d, area_uv: uv.area_uv })
        .collect();
    let (placements, coverage) = pack(&shapes, options.texture_size, options.padding);

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
