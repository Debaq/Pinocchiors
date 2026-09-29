//! Segmentación en cartas: parches de caras que se despliegan cada uno como
//! un disco.
//!
//! Las cartas crecen a la vez desde semillas (una cola de prioridad global,
//! como D-Charts / xatlas): cada cara va a la carta que la acepta con menor
//! costo. El costo mide cuánto se desvía su normal de la normal media de la
//! carta y cuánto alarga el borde. Una carta nunca cruza una arista viva ni
//! acepta caras que se desvíen más de `max_angle`, así cada carta es casi un
//! campo de alturas sobre su plano y se despliega con poca distorsión.
//! Después se relaja: cada semilla pasa a la cara más interior de su carta y
//! se vuelve a crecer.

use crate::geometry::PolyMesh;
use pinocchio_math::Vector3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

const UNASSIGNED: usize = usize::MAX;

/// Peso del término de redondez frente al de normales.
const ROUNDNESS_WEIGHT: f64 = 0.1;

/// [`ChartOptions::old_seam_weight`] por omisión.
const OLD_SEAM_WEIGHT: f64 = 0.5;

/// Opciones de la segmentación.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartOptions {
    /// Máxima desviación (grados) entre la normal de una cara y la normal
    /// media de su carta.
    pub max_angle: f64,
    /// Aristas con ángulo diedro mayor (grados) son siempre costura.
    pub sharp_angle: f64,
    /// Rondas de reubicar semillas y volver a crecer.
    pub relax_iterations: usize,
    /// Alisar los bordes de las cartas al final (costuras más cortas, ver
    /// [`smooth_seams`]).
    pub smooth_seams: bool,
    /// Preferencia por cortar sobre las costuras del mapa original, cuando se
    /// conocen (ver [`crate::unwrap_with_regions`]): costo de unir una cara
    /// a una carta a través de una costura vieja, y descuento del largo de
    /// borde que cae sobre ellas al alisar (`1 / (1 + peso)`). 0 las ignora.
    pub old_seam_weight: f64,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self { max_angle: 55.0, sharp_angle: 70.0, relax_iterations: 4, smooth_seams: true, old_seam_weight: OLD_SEAM_WEIGHT }
    }
}


#[derive(Clone, Copy)]
struct Entry {
    cost: f64,
    face: usize,
    chart: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    // Montículo de mínimos por costo
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.total_cmp(&self.cost).then(other.face.cmp(&self.face))
    }
}

struct Grower<'a> {
    mesh: &'a PolyMesh,
    cos_max: f64,
    old_seam_weight: f64,
    /// Arista local `k` de la cara `f` que la carta no puede cruzar.
    crossable: Vec<Vec<bool>>,
    chart_of: Vec<usize>,
    normal_sums: Vec<Vector3>,
    heap: BinaryHeap<Entry>,
}

impl<'a> Grower<'a> {
    fn new(mesh: &'a PolyMesh, options: &ChartOptions) -> Self {
        let cos_sharp = options.sharp_angle.to_radians().cos();
        let crossable = (0..mesh.num_faces())
            .map(|f| {
                mesh.adjacent[f]
                    .iter()
                    .map(|g| g.is_some_and(|g| mesh.normals[f].dot(&mesh.normals[g]) >= cos_sharp))
                    .collect()
            })
            .collect();
        Self {
            mesh,
            cos_max: options.max_angle.to_radians().cos(),
            old_seam_weight: options.old_seam_weight,
            crossable,
            chart_of: vec![UNASSIGNED; mesh.num_faces()],
            normal_sums: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }

    fn cost(&self, f: usize, chart: usize) -> f64 {
        let mesh = self.mesh;
        let dot = match self.normal_sums[chart].try_normalize() {
            // Caras degeneradas: sin normal, van con cualquier carta
            Some(n) if mesh.areas[f] > 0.0 && mesh.normals[f].length_squared() > 0.0 => mesh.normals[f].dot(&n),
            _ => 1.0,
        };
        if dot < self.cos_max {
            return f64::INFINITY;
        }
        let (mut shared, mut total, mut over_seams) = (0.0, 0.0, 0.0);
        for (k, len) in mesh.edge_lengths[f].iter().enumerate() {
            total += len;
            if mesh.adjacent[f][k].is_some_and(|g| self.chart_of[g] == chart) {
                shared += len;
                if mesh.old_seams[f][k] {
                    over_seams += len;
                }
            }
        }
        let roundness = if total > 0.0 { 1.0 - 2.0 * shared / total } else { 0.0 };
        // Unirse a través de una costura vieja: mejor que la carta pare ahí
        let seam = if shared > 0.0 { over_seams / shared } else { 0.0 };
        (1.0 - dot) + ROUNDNESS_WEIGHT * roundness + self.old_seam_weight * seam
    }

    fn add_chart(&mut self, seed: usize) -> usize {
        let chart = self.normal_sums.len();
        self.normal_sums.push(Vector3::zero());
        self.assign(seed, chart);
        chart
    }

    fn assign(&mut self, f: usize, chart: usize) {
        self.chart_of[f] = chart;
        self.normal_sums[chart] = self.normal_sums[chart] + self.mesh.normals[f] * self.mesh.areas[f];
        for k in 0..self.mesh.faces[f].len() {
            if !self.crossable[f][k] {
                continue;
            }
            let g = self.mesh.adjacent[f][k].expect("arista cruzable tiene vecina");
            if self.chart_of[g] == UNASSIGNED {
                let cost = self.cost(g, chart);
                if cost.is_finite() {
                    self.heap.push(Entry { cost, face: g, chart });
                }
            }
        }
    }

    /// Crece todas las cartas hasta agotar la cola. Si `fill`, las caras que
    /// ninguna carta acepta siembran cartas nuevas.
    fn grow(&mut self, fill: bool) {
        loop {
            while let Some(entry) = self.heap.pop() {
                if self.chart_of[entry.face] != UNASSIGNED {
                    continue;
                }
                // La carta cambió desde que se encoló: reevaluar
                let cost = self.cost(entry.face, entry.chart);
                if !cost.is_finite() {
                    continue;
                }
                if cost > entry.cost + 1e-12 {
                    self.heap.push(Entry { cost, ..entry });
                    continue;
                }
                self.assign(entry.face, entry.chart);
            }
            if !fill {
                return;
            }
            // Nueva semilla: la cara libre más grande
            let free = (0..self.mesh.num_faces())
                .filter(|&f| self.chart_of[f] == UNASSIGNED)
                .max_by(|&a, &b| self.mesh.areas[a].total_cmp(&self.mesh.areas[b]));
            match free {
                Some(seed) => {
                    self.add_chart(seed);
                }
                None => return,
            }
        }
    }
}

/// Carta de cada cara. Todas las cartas resultantes son conexas y discos
/// topológicos.
pub(crate) fn segment(mesh: &PolyMesh, options: &ChartOptions) -> Vec<usize> {
    let mut seeds: Vec<usize> = Vec::new();
    let mut chart_of = Vec::new();
    for round in 0..=options.relax_iterations {
        let mut grower = Grower::new(mesh, options);
        for &seed in &seeds {
            if grower.chart_of[seed] == UNASSIGNED {
                grower.add_chart(seed);
            }
        }
        grower.grow(true);
        if round == options.relax_iterations && options.smooth_seams {
            smooth_seams(&mut grower);
        }
        chart_of = grower.chart_of;

        if round == options.relax_iterations {
            break;
        }
        let new_seeds = interior_faces(mesh, &chart_of);
        if new_seeds == seeds {
            break;
        }
        seeds = new_seeds;
    }
    ensure_disks(mesh, compact(chart_of), options.old_seam_weight)
}

/// Alisa los bordes de las cartas: una cara de borde pasa a la carta vecina
/// si eso acorta las costuras, la carta nueva la acepta (ángulo y aristas
/// vivas) y la vieja no queda vacía. Quita los dientes que deja el
/// crecimiento: costuras ~7 % más cortas y cartas más compactas, que se
/// empaquetan mejor.
fn smooth_seams(grower: &mut Grower) {
    let mesh = grower.mesh;
    let mut sizes = vec![0usize; grower.normal_sums.len()];
    for &c in &grower.chart_of {
        sizes[c] += 1;
    }
    for _ in 0..REFINE_PASSES {
        let mut moved = 0;
        for f in 0..mesh.num_faces() {
            let a = grower.chart_of[f];
            if sizes[a] <= 1 {
                continue;
            }
            let mut best: Option<(f64, usize)> = None;
            for b in mesh.adjacent[f].iter().flatten().map(|&g| grower.chart_of[g]).filter(|&b| b != a) {
                if best.is_some_and(|(_, c)| c == b) {
                    continue;
                }
                // Solo por aristas cruzables y dentro del cono de normales
                let joins = mesh.adjacent[f].iter().enumerate().all(|(k, g)| g.is_none_or(|g| grower.chart_of[g] != b || grower.crossable[f][k]));
                let accepts = grower.normal_sums[b].try_normalize().is_none_or(|n| mesh.normals[f].dot(&n) >= grower.cos_max);
                if !joins || !accepts {
                    continue;
                }
                let delta: f64 = mesh.adjacent[f]
                    .iter()
                    .enumerate()
                    .filter_map(|(k, g)| g.map(|g| (k, g)))
                    .map(|(k, g)| {
                        let c = grower.chart_of[g];
                        mesh.seam_length(f, k, grower.old_seam_weight) * ((c != b) as u8 as f64 - (c != a) as u8 as f64)
                    })
                    .sum();
                if delta < -1e-12 && best.is_none_or(|(d, _)| delta < d) {
                    best = Some((delta, b));
                }
            }
            if let Some((_, b)) = best {
                let contribution = mesh.normals[f] * mesh.areas[f];
                grower.normal_sums[a] -= contribution;
                grower.normal_sums[b] += contribution;
                sizes[a] -= 1;
                sizes[b] += 1;
                grower.chart_of[f] = b;
                moved += 1;
            }
        }
        if moved == 0 {
            break;
        }
    }
}

/// Pasadas máximas de [`smooth_seams`].
const REFINE_PASSES: usize = 20;

/// Renumera las cartas en orden de aparición, sin huecos.
fn compact(chart_of: Vec<usize>) -> Vec<usize> {
    let mut ids = HashMap::new();
    chart_of
        .into_iter()
        .map(|c| {
            let next = ids.len();
            *ids.entry(c).or_insert(next)
        })
        .collect()
}

/// Caras de cada carta.
pub(crate) fn chart_faces(chart_of: &[usize]) -> Vec<Vec<usize>> {
    let num = chart_of.iter().map(|&c| c + 1).max().unwrap_or(0);
    let mut faces = vec![Vec::new(); num];
    for (f, &c) in chart_of.iter().enumerate() {
        faces[c].push(f);
    }
    faces
}

/// La cara más alejada (en saltos) del borde de cada carta.
fn interior_faces(mesh: &PolyMesh, chart_of: &[usize]) -> Vec<usize> {
    let mut depth = vec![usize::MAX; mesh.num_faces()];
    let mut queue = VecDeque::new();
    for f in 0..mesh.num_faces() {
        let on_border = mesh.adjacent[f].iter().any(|g| g.is_none_or(|g| chart_of[g] != chart_of[f]));
        if on_border {
            depth[f] = 0;
            queue.push_back(f);
        }
    }
    while let Some(f) = queue.pop_front() {
        for (g, _) in mesh.neighbors(f) {
            if chart_of[g] == chart_of[f] && depth[g] == usize::MAX {
                depth[g] = depth[f] + 1;
                queue.push_back(g);
            }
        }
    }
    chart_faces(chart_of)
        .into_iter()
        .filter(|faces| !faces.is_empty())
        .map(|faces| {
            *faces
                .iter()
                .max_by(|&&a, &&b| {
                    depth[a].min(usize::MAX - 1).cmp(&depth[b].min(usize::MAX - 1)).then(b.cmp(&a))
                })
                .expect("carta no vacía")
        })
        .collect()
}

/// `true` si las caras forman un disco: conexas, característica de Euler 1 y
/// un solo lazo de borde.
pub(crate) fn is_disk(mesh: &PolyMesh, faces: &[usize]) -> bool {
    let mut vertices = HashSet::new();
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for &f in faces {
        let face = &mesh.faces[f];
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            vertices.insert(a);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let euler = vertices.len() as i64 - edges.len() as i64 + faces.len() as i64;
    if euler != 1 {
        return false;
    }
    // Lazos de borde: componentes conexas del grafo de aristas de borde
    let boundary: Vec<(usize, usize)> = edges.iter().filter(|(_, n)| **n == 1).map(|(e, _)| *e).collect();
    if boundary.is_empty() {
        return false;
    }
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in &boundary {
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    let start = boundary[0].0;
    let mut seen = HashSet::from([start]);
    let mut stack = vec![start];
    while let Some(v) = stack.pop() {
        for &w in &adjacency[&v] {
            if seen.insert(w) {
                stack.push(w);
            }
        }
    }
    seen.len() == adjacency.len() && is_connected(mesh, faces)
}

fn is_connected(mesh: &PolyMesh, faces: &[usize]) -> bool {
    component(mesh, faces).len() == faces.len()
}

/// Caras conectadas a `faces[0]` dentro de `faces`.
fn component(mesh: &PolyMesh, faces: &[usize]) -> HashSet<usize> {
    let inside: HashSet<usize> = faces.iter().copied().collect();
    let mut seen = HashSet::from([faces[0]]);
    let mut stack = vec![faces[0]];
    while let Some(f) = stack.pop() {
        for (g, _) in mesh.neighbors(f) {
            if inside.contains(&g) && seen.insert(g) {
                stack.push(g);
            }
        }
    }
    seen
}

/// Parte una carta en dos: semillas en dos caras alejadas y cada cara va a
/// la más cercana (Dijkstra por centroides, solo dentro de la carta). Pasar
/// por una costura vieja cuesta `1 + old_seam_weight` veces más, así el
/// corte tiende a caer sobre ellas. Devuelve `None` si la carta tiene una
/// sola cara.
pub(crate) fn split(mesh: &PolyMesh, faces: &[usize], old_seam_weight: f64) -> Option<(Vec<usize>, Vec<usize>)> {
    if faces.len() < 2 {
        return None;
    }
    let first = component(mesh, faces);
    if first.len() < faces.len() {
        return Some(faces.iter().partition(|f| first.contains(f)));
    }
    let inside: HashSet<usize> = faces.iter().copied().collect();
    let distances = |sources: &[usize]| -> HashMap<usize, (f64, usize)> {
        let mut best: HashMap<usize, (f64, usize)> = HashMap::new();
        let mut heap = BinaryHeap::new();
        for (label, &s) in sources.iter().enumerate() {
            best.insert(s, (0.0, label));
            heap.push(Entry { cost: 0.0, face: s, chart: label });
        }
        while let Some(Entry { cost, face, chart }) = heap.pop() {
            if best.get(&face).is_some_and(|&(d, _)| d < cost) {
                continue;
            }
            for (k, g) in mesh.adjacent[face].iter().enumerate() {
                let Some(g) = *g else { continue };
                if !inside.contains(&g) {
                    continue;
                }
                let step = mesh.centroids[face].distance(&mesh.centroids[g]);
                let d = cost + if mesh.old_seams[face][k] { step * (1.0 + old_seam_weight) } else { step };
                if best.get(&g).is_none_or(|&(old, _)| d < old) {
                    best.insert(g, (d, chart));
                    heap.push(Entry { cost: d, face: g, chart });
                }
            }
        }
        best
    };
    let farthest = |d: &HashMap<usize, (f64, usize)>| {
        *d.iter().max_by(|a, b| a.1.0.total_cmp(&b.1.0).then(b.0.cmp(a.0))).expect("carta no vacía").0
    };
    let a = farthest(&distances(&[faces[0]]));
    let b = farthest(&distances(&[a]));
    if a == b {
        return None;
    }
    let labels = distances(&[a, b]);
    let (mut first, mut second) = (Vec::new(), Vec::new());
    for &f in faces {
        match labels.get(&f) {
            Some(&(_, 1)) => second.push(f),
            _ => first.push(f),
        }
    }
    Some((first, second))
}

/// Parte las cartas que no son discos hasta que todas lo sean.
fn ensure_disks(mesh: &PolyMesh, chart_of: Vec<usize>, old_seam_weight: f64) -> Vec<usize> {
    let mut pending = chart_faces(&chart_of);
    let mut done = Vec::new();
    while let Some(faces) = pending.pop() {
        if faces.is_empty() {
            continue;
        }
        if is_disk(mesh, &faces) {
            done.push(faces);
            continue;
        }
        match split(mesh, &faces, old_seam_weight) {
            Some((a, b)) => {
                pending.push(a);
                pending.push(b);
            }
            // Una sola cara que no es disco (polígono degenerado): se deja
            None => done.push(faces),
        }
    }
    let mut result = vec![0; mesh.num_faces()];
    for (chart, faces) in done.iter().enumerate() {
        for &f in faces {
            result[f] = chart;
        }
    }
    result
}
