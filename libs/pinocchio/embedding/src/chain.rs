//! Embedding por cadenas sobre el eje medial.
//!
//! El esqueleto se descompone en cadenas: desde la raíz o una bifurcación
//! hasta la siguiente bifurcación u hoja, pasando por articulaciones con un
//! solo hijo (p. ej. pecho → hombro → codo → muñeca → mano). Para cada cadena:
//!
//! 1. Se buscan caminos por el interior de la malla con Dijkstra sobre la
//!    grilla del campo de distancias, con costo `longitud / d²`: los caminos
//!    siguen el eje medial en vez de pegarse a la superficie.
//! 2. El extremo de una cadena que termina en hoja es el punto del eje medial
//!    más lejano (a lo largo del camino) dentro de la región de esa
//!    extremidad; la región son las celdas cuya plantilla más cercana es un
//!    hueso de la cadena. Si la cadena termina en una bifurcación, el extremo
//!    es el punto medial más cercano a la plantilla.
//! 3. Las articulaciones se reparten sobre el camino según las proporciones de
//!    longitud de la plantilla.
//!
//! Así el esqueleto se adapta a la longitud y la pose real de cada extremidad
//! en lugar de quedarse en la posición de la plantilla.

use crate::embedding::{EmbeddingError, EmbeddingResult};
use crate::medial_surface::MedialSphere;
use pinocchio_math::{Real, Vector3};
use pinocchio_skeleton::Skeleton;
use pinocchio_spatial::DistanceField;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Grilla del campo de distancias vista como grafo de celdas interiores
pub(crate) struct CellGraph<'a> {
    field: &'a DistanceField,
    res: [usize; 3],
}

/// Resultado de Dijkstra desde una celda
pub(crate) struct ShortestPaths {
    /// Celda anterior en el camino (usize::MAX si no alcanzable / origen)
    pub prev: Vec<usize>,
    /// Longitud geométrica del camino
    pub length: Vec<Real>,
    pub reached: Vec<bool>,
    /// Celdas en el orden en que se cerraron (costo creciente)
    pub order: Vec<usize>,
}

#[derive(PartialEq)]
struct HeapItem(Real, usize);

impl Eq for HeapItem {}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap por costo
        other.0.total_cmp(&self.0)
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> CellGraph<'a> {
    pub fn new(field: &'a DistanceField) -> Self {
        Self { field, res: field.resolution() }
    }

    pub fn len(&self) -> usize {
        self.res[0] * self.res[1] * self.res[2]
    }

    fn coords(&self, idx: usize) -> (usize, usize, usize) {
        let [rx, ry, _] = self.res;
        (idx % rx, (idx / rx) % ry, idx / (rx * ry))
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.res[1] + y) * self.res[0] + x
    }

    pub fn value(&self, idx: usize) -> Real {
        let (x, y, z) = self.coords(idx);
        self.field.get(x, y, z)
    }

    pub fn is_interior(&self, idx: usize) -> bool {
        let v = self.value(idx);
        v > 0.0 && v.is_finite()
    }

    pub fn center(&self, idx: usize) -> Vector3 {
        let (x, y, z) = self.coords(idx);
        self.field.cell_center(x, y, z)
    }

    /// Celda interior más cercana a `pos` (búsqueda por anillos crecientes)
    pub fn nearest_interior(&self, pos: &Vector3) -> Option<usize> {
        let [rx, ry, rz] = self.res;
        let bounds = self.field.bounds();
        let size = bounds.size();
        let rel = *pos - bounds.min;
        let clamp = |v: Real, n: usize, s: Real| ((v / s * n as Real).floor().max(0.0) as usize).min(n - 1);
        let (cx, cy, cz) = (clamp(rel.x(), rx, size.x()), clamp(rel.y(), ry, size.y()), clamp(rel.z(), rz, size.z()));

        let max_r = rx.max(ry).max(rz);
        for r in 0..max_r {
            let mut best: Option<(Real, usize)> = None;
            let (x0, x1) = (cx.saturating_sub(r), (cx + r).min(rx - 1));
            let (y0, y1) = (cy.saturating_sub(r), (cy + r).min(ry - 1));
            let (z0, z1) = (cz.saturating_sub(r), (cz + r).min(rz - 1));
            for z in z0..=z1 {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let on_shell = x == x0 || x == x1 || y == y0 || y == y1 || z == z0 || z == z1;
                        if !on_shell {
                            continue;
                        }
                        let idx = self.index(x, y, z);
                        if self.is_interior(idx) {
                            let d = self.center(idx).distance_squared(pos);
                            if best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, idx));
                            }
                        }
                    }
                }
            }
            if let Some((_, idx)) = best {
                return Some(idx);
            }
        }
        None
    }

    /// Lado de una celda (el menor de los tres ejes)
    pub fn cell_size(&self) -> Real {
        let size = self.field.bounds().size();
        let [rx, ry, rz] = self.res;
        (size.x() / rx as Real).min(size.y() / ry as Real).min(size.z() / rz as Real)
    }

    /// Celdas vecinas (26-vecindad) dentro de la grilla
    pub fn neighbors(&self, idx: usize) -> impl Iterator<Item = usize> + '_ {
        let (x, y, z) = self.coords(idx);
        let [rx, ry, rz] = self.res;
        (-1i64..=1)
            .flat_map(|dz| (-1i64..=1).flat_map(move |dy| (-1i64..=1).map(move |dx| (dx, dy, dz))))
            .filter(|&d| d != (0, 0, 0))
            .filter_map(move |(dx, dy, dz)| {
                let (nx, ny, nz) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                let inside = nx >= 0 && ny >= 0 && nz >= 0 && nx < rx as i64 && ny < ry as i64 && nz < rz as i64;
                inside.then(|| self.index(nx as usize, ny as usize, nz as usize))
            })
    }

    /// Dijkstra por celdas interiores (26-vecindad), costo `longitud / d²`
    pub fn shortest_paths(&self, start: usize) -> ShortestPaths {
        let n = self.len();
        let mut cost = vec![Real::INFINITY; n];
        let mut paths = ShortestPaths {
            prev: vec![usize::MAX; n],
            length: vec![Real::INFINITY; n],
            reached: vec![false; n],
            order: Vec::new(),
        };
        let [rx, ry, rz] = self.res;
        let mut heap = BinaryHeap::new();
        cost[start] = 0.0;
        paths.length[start] = 0.0;
        heap.push(HeapItem(0.0, start));

        while let Some(HeapItem(c, idx)) = heap.pop() {
            if paths.reached[idx] {
                continue;
            }
            paths.reached[idx] = true;
            paths.order.push(idx);
            let (x, y, z) = self.coords(idx);
            let d_here = self.value(idx);
            let p_here = self.center(idx);

            for dz in -1i64..=1 {
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if dx == 0 && dy == 0 && dz == 0 {
                            continue;
                        }
                        let (nx, ny, nz) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                        if nx < 0 || ny < 0 || nz < 0 || nx >= rx as i64 || ny >= ry as i64 || nz >= rz as i64 {
                            continue;
                        }
                        let nidx = self.index(nx as usize, ny as usize, nz as usize);
                        if paths.reached[nidx] || !self.is_interior(nidx) {
                            continue;
                        }
                        let step = p_here.distance(&self.center(nidx));
                        let d = 0.5 * (d_here + self.value(nidx));
                        let new_cost = c + step / (d * d);
                        if new_cost < cost[nidx] {
                            cost[nidx] = new_cost;
                            paths.prev[nidx] = idx;
                            paths.length[nidx] = paths.length[idx] + step;
                            heap.push(HeapItem(new_cost, nidx));
                        }
                    }
                }
            }
        }
        paths
    }
}

/// Punto a distancia `s` a lo largo de la polilínea
pub(crate) fn point_along(polyline: &[Vector3], s: Real) -> Vector3 {
    let mut remaining = s.max(0.0);
    for w in polyline.windows(2) {
        let seg = w[0].distance(&w[1]);
        if remaining <= seg && seg > 0.0 {
            return w[0].lerp(&w[1], remaining / seg);
        }
        remaining -= seg;
    }
    *polyline.last().expect("polilínea vacía")
}

/// Longitud de arco del punto de la polilínea más cercano a `p`
pub(crate) fn closest_arc_length(polyline: &[Vector3], p: &Vector3) -> Real {
    let mut best = (Real::INFINITY, 0.0);
    let mut acc = 0.0;
    for w in polyline.windows(2) {
        let seg = w[1] - w[0];
        let len = seg.length();
        let t = if len > 0.0 { ((*p - w[0]).dot(&seg) / (len * len)).clamp(0.0, 1.0) } else { 0.0 };
        let d = p.distance_squared(&(w[0] + seg * t));
        if d < best.0 {
            best = (d, acc + t * len);
        }
        acc += len;
    }
    best.1
}

pub(crate) fn polyline_length(polyline: &[Vector3]) -> Real {
    polyline.windows(2).map(|w| w[0].distance(&w[1])).sum()
}

/// Suaviza la escalera de voxels con promedios móviles, fijando los extremos
pub(crate) fn smooth(polyline: &mut [Vector3], iterations: usize) {
    for _ in 0..iterations {
        let copy = polyline.to_vec();
        for i in 1..polyline.len().saturating_sub(1) {
            polyline[i] = (copy[i - 1] + copy[i] * 2.0 + copy[i + 1]) * 0.25;
        }
    }
}

fn point_segment_distance(p: &Vector3, a: &Vector3, b: &Vector3) -> Real {
    let v = *b - *a;
    let len2 = v.dot(&v);
    let t = if len2 > 0.0 { ((*p - *a).dot(&v) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(&(*a + v * t))
}

/// Embedding por cadenas. `skeleton` debe estar en el espacio de la malla y
/// aproximadamente alineado con ella; `field` debe tener signo (positivo dentro).
pub fn chain_embed<S: Skeleton>(
    skeleton: &S,
    field: &DistanceField,
    medial: &[MedialSphere],
) -> Result<EmbeddingResult, EmbeddingError> {
    let num_bones = skeleton.num_bones();
    if num_bones == 0 {
        return Err(EmbeddingError::EmptySkeleton);
    }
    let graph = CellGraph::new(field);
    let template: Vec<Vector3> = skeleton.bones().iter().map(|b| b.position).collect();

    // Celdas del eje medial y el hueso de plantilla más cercano a cada una
    let medial_cells: Vec<usize> = medial
        .iter()
        .filter_map(|s| graph.nearest_interior(&s.center))
        .collect();
    if medial_cells.is_empty() {
        return Err(EmbeddingError::NoValidEmbedding);
    }
    let segments: Vec<(usize, usize)> = (0..num_bones)
        .filter_map(|b| skeleton.get_parent(b).map(|p| (p, b)))
        .collect();
    let medial_owner: Vec<Option<usize>> = medial_cells
        .iter()
        .map(|&cell| {
            let c = graph.center(cell);
            segments
                .iter()
                .min_by(|&&(pa, a), &&(pb, b)| {
                    point_segment_distance(&c, &template[pa], &template[a])
                        .total_cmp(&point_segment_distance(&c, &template[pb], &template[b]))
                })
                .map(|&(_, b)| b)
        })
        .collect();

    let nearest_medial = |pos: &Vector3| -> usize {
        *medial_cells
            .iter()
            .min_by(|&&a, &&b| graph.center(a).distance_squared(pos).total_cmp(&graph.center(b).distance_squared(pos)))
            .expect("hay celdas mediales")
    };

    let mut positions: Vec<Option<Vector3>> = vec![None; num_bones];
    let mut cells: Vec<Option<usize>> = vec![None; num_bones];

    // Raíces: punto medial más cercano a la plantilla
    let roots: Vec<usize> = (0..num_bones).filter(|&b| skeleton.get_parent(b).is_none()).collect();
    let mut pending: Vec<usize> = Vec::new();
    for &root in &roots {
        let cell = nearest_medial(&template[root]);
        positions[root] = Some(graph.center(cell));
        cells[root] = Some(cell);
        pending.push(root);
    }

    while let Some(base) = pending.pop() {
        let base_cell = cells[base].expect("base colocada");
        let base_pos = positions[base].expect("base colocada");
        let paths = graph.shortest_paths(base_cell);

        for first in skeleton.get_children(base) {
            // Recorrer la cadena hasta una hoja o bifurcación
            let mut chain = vec![first];
            loop {
                let children = skeleton.get_children(*chain.last().unwrap());
                if children.len() == 1 {
                    chain.push(children[0]);
                } else {
                    break;
                }
            }
            let end = *chain.last().unwrap();
            let is_leaf = skeleton.get_children(end).is_empty();

            // Extremo de la cadena
            let end_cell = if is_leaf {
                medial_cells
                    .iter()
                    .zip(&medial_owner)
                    .filter(|&(&cell, owner)| owner.is_some_and(|o| chain.contains(&o)) && paths.reached[cell])
                    .max_by(|a, b| paths.length[*a.0].total_cmp(&paths.length[*b.0]))
                    .map(|(&cell, _)| cell)
            } else {
                None
            };
            let end_cell = end_cell.unwrap_or_else(|| {
                // Bifurcación (o región vacía): medial más cercano a la plantilla y alcanzable
                medial_cells
                    .iter()
                    .copied()
                    .filter(|&c| paths.reached[c])
                    .min_by(|&a, &b| {
                        graph.center(a).distance_squared(&template[end])
                            .total_cmp(&graph.center(b).distance_squared(&template[end]))
                    })
                    .unwrap_or(base_cell)
            });

            // Camino base → extremo
            let mut path_cells = vec![end_cell];
            let mut cur = end_cell;
            while cur != base_cell && paths.prev[cur] != usize::MAX {
                cur = paths.prev[cur];
                path_cells.push(cur);
            }
            path_cells.reverse();
            let mut polyline: Vec<Vector3> = path_cells.iter().map(|&c| graph.center(c)).collect();
            polyline[0] = base_pos;
            smooth(&mut polyline, 2);
            let total = polyline_length(&polyline);

            // Repartir según las proporciones de la plantilla. El primer joint
            // de una cadena con más de un hueso se ancla a la proyección de su
            // posición de plantilla sobre el camino: el tramo desde la base
            // (p. ej. pelvis→cadera) atraviesa el torso y su longitud en la
            // malla no guarda proporción con la plantilla.
            let lengths: Vec<Real> = chain
                .iter()
                .map(|&b| template[b].distance(&template[skeleton.get_parent(b).unwrap()]))
                .collect();
            let (start, first_bone) = if chain.len() >= 2 {
                let anchor = closest_arc_length(&polyline, &template[chain[0]]).min(0.5 * total);
                positions[chain[0]] = Some(point_along(&polyline, anchor));
                cells[chain[0]] = graph.nearest_interior(&positions[chain[0]].unwrap()).or(Some(end_cell));
                (anchor, 1)
            } else {
                (0.0, 0)
            };
            let rest_total: Real = lengths[first_bone..].iter().sum();
            let mut acc = 0.0;
            for (&bone, &len) in chain.iter().zip(&lengths).skip(first_bone) {
                acc += len;
                let fraction = if rest_total > 0.0 { acc / rest_total } else { 1.0 };
                let pos = point_along(&polyline, start + fraction * (total - start));
                positions[bone] = Some(pos);
                cells[bone] = graph.nearest_interior(&pos).or(Some(end_cell));
            }
            positions[end] = Some(*polyline.last().unwrap());
            cells[end] = Some(end_cell);

            if !is_leaf {
                pending.push(end);
            }
        }
    }

    // Huesos no alcanzados (grafo desconectado): posición de la plantilla
    let bone_positions: Vec<Vector3> = positions
        .iter()
        .zip(&template)
        .map(|(p, t)| p.unwrap_or(*t))
        .collect();

    let mut sphere_bone_map = vec![None; medial.len()];
    for (bone, pos) in bone_positions.iter().enumerate() {
        if let Some((i, _)) = medial
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.center.distance_squared(pos).total_cmp(&b.1.center.distance_squared(pos)))
        {
            sphere_bone_map[i] = Some(bone);
        }
    }

    let quality_score = proportion_quality(skeleton, &bone_positions);
    Ok(EmbeddingResult { bone_positions, sphere_bone_map, quality_score })
}

/// Calidad invariante a la escala: compara la proporción de cada hueso
/// respecto del total, en la plantilla y en el embedding (1 = idénticas)
pub(crate) fn proportion_quality<S: Skeleton>(skeleton: &S, positions: &[Vector3]) -> Real {
    let bones: Vec<(Real, Real)> = (0..skeleton.num_bones())
        .filter_map(|b| {
            let p = skeleton.get_parent(b)?;
            let t = skeleton.get_bone(b)?.position.distance(&skeleton.get_bone(p)?.position);
            Some((t, positions[b].distance(&positions[p])))
        })
        .collect();
    let t_total: Real = bones.iter().map(|b| b.0).sum();
    let e_total: Real = bones.iter().map(|b| b.1).sum();
    if bones.is_empty() || t_total <= 0.0 || e_total <= 0.0 {
        return 1.0;
    }
    let error: Real = bones.iter().map(|(t, e)| (t / t_total - e / e_total).abs()).sum();
    (1.0 - error).clamp(0.0, 1.0)
}
