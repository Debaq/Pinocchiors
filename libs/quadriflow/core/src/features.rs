//! Consulta del punto más cercano sobre las aristas características (bordes y
//! aristas vivas), para dejar sobre ellas los vértices que las siguen.

use crate::surface::VertexGraph;
use crate::V3;
use std::collections::HashMap;

pub(crate) struct FeatureLines {
    cell: f64,
    segments: Vec<(V3, V3)>,
    grid: HashMap<[i64; 3], Vec<u32>>,
}

fn closest_on_segment(p: &V3, a: &V3, b: &V3) -> V3 {
    let ab = b - a;
    let len_sq = ab.norm_squared();
    if len_sq <= 0.0 {
        return *a;
    }
    a + ab * ((p - a).dot(&ab) / len_sq).clamp(0.0, 1.0)
}

impl FeatureLines {
    /// Indexa las aristas características en una grilla de lado `cell`.
    pub fn new(graph: &VertexGraph, cell: f64) -> Self {
        let segments = graph
            .feature_edges
            .iter()
            .map(|&(a, b)| (graph.pos[a as usize], graph.pos[b as usize]))
            .collect();
        Self::from_segments(segments, cell)
    }

    /// Indexa segmentos sueltos en una grilla de lado `cell`.
    pub fn from_segments(segments: Vec<(V3, V3)>, cell: f64) -> Self {
        let mut lines = Self { cell, segments: Vec::new(), grid: HashMap::new() };
        for (id, &(pa, pb)) in segments.iter().enumerate() {
            let (lo, hi) = (lines.cell_of(&pa.inf(&pb)), lines.cell_of(&pa.sup(&pb)));
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        lines.grid.entry([x, y, z]).or_default().push(id as u32);
                    }
                }
            }
        }
        lines.segments = segments;
        lines
    }

    fn cell_of(&self, p: &V3) -> [i64; 3] {
        [
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
            (p.z / self.cell).floor() as i64,
        ]
    }

    /// Punto más cercano sobre una arista característica a menos de `cell`.
    pub fn closest(&self, p: &V3) -> Option<V3> {
        let c = self.cell_of(p);
        let mut best: Option<(f64, V3)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(ids) = self.grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else {
                        continue;
                    };
                    for &id in ids {
                        let (a, b) = &self.segments[id as usize];
                        let q = closest_on_segment(p, a, b);
                        let d = (q - p).norm_squared();
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, q));
                        }
                    }
                }
            }
        }
        best.filter(|&(d, _)| d <= self.cell * self.cell).map(|(_, q)| q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::Surface;

    #[test]
    fn snaps_to_nearest_boundary_segment() {
        let s = Surface {
            positions: vec![
                V3::new(0.0, 0.0, 0.0),
                V3::new(1.0, 0.0, 0.0),
                V3::new(1.0, 1.0, 0.0),
                V3::new(0.0, 1.0, 0.0),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let lines = FeatureLines::new(&s.vertex_graph(None, 0.7), 0.25);
        let q = lines.closest(&V3::new(0.4, 0.1, 0.0)).unwrap();
        assert!((q - V3::new(0.4, 0.0, 0.0)).norm() < 1e-12);
        // La diagonal no es característica y el centro está lejos del borde
        assert!(lines.closest(&V3::new(0.5, 0.5, 0.0)).is_none());
    }
}
