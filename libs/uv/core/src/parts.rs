//! Cartas por partes del cuerpo (cabeza, torso, cada pata, cola) para el mapa
//! para pintar.
//!
//! Cada parte se vuelve un disco con cortes por su lado escondido: la cara
//! interna de las patas (hacia el cuerpo) y la panza (hacia abajo; la escena
//! es Y arriba). Una parte con varios bordes (el torso, abierto donde salen
//! el cuello, las patas y la cola) une sus bordes con caminos cortos por ese
//! lado; una parte cerrada o muy honda (una pata es una media, la cabeza un
//! casquete) se abre con un cierre desde el borde hasta su punta. Los cortes
//! se hacen duplicando vértices: las caras son las mismas y en el mismo orden.

use crate::geometry::PolyMesh;
use pinocchio_math::Vector3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Cuánto más cuesta cortar por el lado visible que por el escondido.
const VISIBLE_COST: f64 = 6.0;

/// Una parte con más curvatura que esto en su interior (suma de defectos
/// angulares: 0 un disco plano, 2π un casquete de media esfera) no se aplana
/// sin estirarse: se abre con un cierre hasta su punta. No depende del borde,
/// que en las partes sacadas de los pesos es dentado.
const ZIP_CURVATURE: f64 = 0.75 * std::f64::consts::PI;

/// Cierres como mucho por parte.
const MAX_ZIPS: usize = 2;

/// Trozos sueltos de una parte más chicos que esta fracción de su trozo
/// principal pasan a la parte vecina con la que más borde comparten (manchas
/// de pesos: la mano que toca el muslo).
const STRAY_FRACTION: f64 = 0.3;

/// Malla cortada: posiciones (con los vértices del corte duplicados), caras
/// con los mismos índices de cara y las caras de cada parte conexa.
pub(crate) struct CutMesh<const N: usize> {
    pub positions: Vec<[f64; 3]>,
    pub faces: Vec<[usize; N]>,
    pub charts: Vec<Vec<usize>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Item {
    cost: f64,
    vertex: usize,
}
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.total_cmp(&self.cost).then(other.vertex.cmp(&self.vertex))
    }
}

/// Grafo de aristas interiores de una parte: por vértice, (vecino, largo,
/// costo de cortar por ahí).
type Graph = HashMap<usize, Vec<(usize, f64, f64)>>;

/// Dijkstra desde `sources`; `weighted` usa el costo de corte, si no el largo.
/// Devuelve distancias y padres.
fn dijkstra(graph: &Graph, sources: &HashSet<usize>, weighted: bool) -> (HashMap<usize, f64>, HashMap<usize, usize>) {
    let mut dist: HashMap<usize, f64> = HashMap::new();
    let mut parent = HashMap::new();
    let mut heap = BinaryHeap::new();
    for &s in sources {
        dist.insert(s, 0.0);
        heap.push(Item { cost: 0.0, vertex: s });
    }
    while let Some(Item { cost, vertex }) = heap.pop() {
        if dist.get(&vertex).is_some_and(|&d| d < cost) {
            continue;
        }
        for &(w, len, cut) in graph.get(&vertex).map_or(&[][..], |v| &v[..]) {
            let d = cost + if weighted { cut } else { len };
            if dist.get(&w).is_none_or(|&old| d < old) {
                dist.insert(w, d);
                parent.insert(w, vertex);
                heap.push(Item { cost: d, vertex: w });
            }
        }
    }
    (dist, parent)
}

/// Aristas del camino de `parent` desde `end` hasta una fuente.
fn path_edges(parent: &HashMap<usize, usize>, mut end: usize) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    while let Some(&p) = parent.get(&end) {
        edges.push((p.min(end), p.max(end)));
        end = p;
    }
    edges
}

fn find(parent: &mut HashMap<usize, usize>, x: usize) -> usize {
    let mut r = x;
    while let Some(&p) = parent.get(&r).filter(|&&p| p != r) {
        r = p;
    }
    parent.insert(x, r);
    r
}

/// Caras conexas de igual parte.
fn components(mesh: &PolyMesh, parts: &[usize]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; mesh.num_faces()];
    let mut out = Vec::new();
    for start in 0..mesh.num_faces() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut faces = vec![start];
        let mut i = 0;
        while i < faces.len() {
            let f = faces[i];
            i += 1;
            for g in mesh.adjacent[f].iter().flatten() {
                if !seen[*g] && parts[*g] == parts[f] {
                    seen[*g] = true;
                    faces.push(*g);
                }
            }
        }
        out.push(faces);
    }
    out
}

/// Partes limpias: los trozos sueltos chicos de cada parte (ver
/// `STRAY_FRACTION`) pasan a la vecina con más borde compartido. Es lo que usa
/// [`crate::unwrap_by_parts`] para cortar.
pub fn clean_parts<const N: usize>(positions: &[[f64; 3]], faces: &[[usize; N]], parts: &[usize]) -> Vec<usize> {
    if parts.len() != faces.len() {
        return vec![0; faces.len()];
    }
    absorb_strays(&PolyMesh::new(positions, faces), parts)
}

/// Pasa los trozos sueltos chicos de cada parte a la vecina con más borde
/// compartido.
fn absorb_strays(mesh: &PolyMesh, parts: &[usize]) -> Vec<usize> {
    let mut parts = parts.to_vec();
    let comps = components(mesh, &parts);
    let area = |c: &Vec<usize>| c.iter().map(|&f| mesh.areas[f]).sum::<f64>();
    // Área del trozo principal de cada parte
    let mut main_area: HashMap<usize, f64> = HashMap::new();
    for comp in &comps {
        let entry = main_area.entry(parts[comp[0]]).or_default();
        *entry = entry.max(area(comp));
    }
    // De menor a mayor: un trozo absorbido puede agrandar a otro
    let mut order: Vec<&Vec<usize>> = comps.iter().collect();
    order.sort_by(|a, b| area(a).total_cmp(&area(b)));
    for comp in order {
        let label = parts[comp[0]];
        if area(comp) >= STRAY_FRACTION * main_area[&label] {
            continue;
        }
        let inside: HashSet<usize> = comp.iter().copied().collect();
        let mut shared: HashMap<usize, f64> = HashMap::new();
        for &f in comp {
            for (k, g) in mesh.adjacent[f].iter().enumerate() {
                if let Some(g) = g.filter(|g| !inside.contains(g)) {
                    *shared.entry(parts[g]).or_default() += mesh.edge_lengths[f][k];
                }
            }
        }
        if let Some((&to, _)) = shared.iter().filter(|(p, _)| **p != label).max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0))) {
            for &f in comp {
                parts[f] = to;
            }
        }
    }
    parts
}

/// Corta la malla por partes (`parts[f]`: parte de la cara `f`).
pub(crate) fn cut_by_parts<const N: usize>(positions: &[[f64; 3]], faces: &[[usize; N]], parts: &[usize]) -> CutMesh<N> {
    let mesh = PolyMesh::new(positions, faces);
    let parts = if parts.len() == faces.len() { absorb_strays(&mesh, parts) } else { vec![0; faces.len()] };
    let comps = components(&mesh, &parts);

    let centroid = |faces: &[usize]| {
        let (mut sum, mut area) = (Vector3::zero(), 0.0);
        for &f in faces {
            sum += mesh.centroids[f] * mesh.areas[f];
            area += mesh.areas[f];
        }
        (if area > 0.0 { sum * (1.0 / area) } else { sum }, area)
    };
    let all: Vec<usize> = (0..mesh.num_faces()).collect();
    let (model_center, _) = centroid(&all);
    let size = mesh.points.iter().fold(0.0f64, |m, p| m.max(p.distance(&model_center)));

    let mut new_positions = positions.to_vec();
    let mut new_faces = faces.to_vec();
    for comp in &comps {
        let cuts = part_cuts(&mesh, comp, model_center, size);
        if cuts.is_empty() {
            continue;
        }
        split_vertices(&mesh, comp, &cuts, &mut new_positions, &mut new_faces);
    }
    CutMesh { positions: new_positions, faces: new_faces, charts: comps }
}

/// Aristas (pares de vértices) a cortar para abrir la parte `comp`.
fn part_cuts(mesh: &PolyMesh, comp: &[usize], model_center: Vector3, size: f64) -> HashSet<(usize, usize)> {
    // Lado escondido: hacia el cuerpo en horizontal (y algo abajo) para las
    // partes que salen de él, abajo para el torso
    let center = {
        let (mut sum, mut area) = (Vector3::zero(), 0.0);
        for &f in comp {
            sum += mesh.centroids[f] * mesh.areas[f];
            area += mesh.areas[f];
        }
        if area > 0.0 { sum * (1.0 / area) } else { sum }
    };
    let down = Vector3::new(0.0, -1.0, 0.0);
    let inward = model_center - center;
    let horizontal = Vector3::new(inward.x(), 0.0, inward.z());
    let hidden = if horizontal.length() > 0.1 * size {
        (horizontal.normalize() + down * 0.5).normalize()
    } else {
        down
    };

    // Aristas de la parte: interiores (dos caras) y de borde (una)
    let mut edges: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for &f in comp {
        let face = &mesh.faces[f];
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            edges.entry((a.min(b), a.max(b))).or_default().push((f, k));
        }
    }
    let mut graph: Graph = HashMap::new();
    let mut boundary: Vec<(usize, usize)> = Vec::new();
    for (&(a, b), users) in &edges {
        let len = mesh.points[a].distance(&mesh.points[b]);
        match users[..] {
            [(f, _), (g, _)] => {
                let normal = (mesh.normals[f] + mesh.normals[g]).try_normalize().unwrap_or(mesh.normals[f]);
                let h = normal.dot(&hidden).clamp(-1.0, 1.0);
                let cut = len * (1.0 + VISIBLE_COST * (1.0 - h) / 2.0);
                graph.entry(a).or_default().push((b, len, cut));
                graph.entry(b).or_default().push((a, len, cut));
            }
            _ => boundary.push((a, b)),
        }
    }

    // Lazos de borde
    let mut uf: HashMap<usize, usize> = HashMap::new();
    for &(a, b) in &boundary {
        uf.entry(a).or_insert(a);
        uf.entry(b).or_insert(b);
        let (ra, rb) = (find(&mut uf, a), find(&mut uf, b));
        uf.insert(ra, rb);
    }
    let boundary_vertices: Vec<usize> = uf.keys().copied().collect();
    let mut loops: HashMap<usize, Vec<usize>> = HashMap::new();
    for v in boundary_vertices {
        let root = find(&mut uf, v);
        loops.entry(root).or_default().push(v);
    }
    let mut loops: Vec<Vec<usize>> = loops.into_values().collect();
    loops.sort_by_key(|l| std::cmp::Reverse(l.len()));

    let mut cuts: HashSet<(usize, usize)> = HashSet::new();
    let mut opened: HashSet<usize> = HashSet::new();
    let add_path = |cuts: &mut HashSet<(usize, usize)>, opened: &mut HashSet<usize>, path: Vec<(usize, usize)>| {
        for (a, b) in path {
            cuts.insert((a, b));
            opened.insert(a);
            opened.insert(b);
        }
    };

    if loops.is_empty() {
        // Cerrada: un corte entre los dos puntos más alejados
        let Some(&start) = graph.keys().min() else { return cuts };
        let far = |from: usize| {
            let (dist, _) = dijkstra(&graph, &HashSet::from([from]), false);
            dist.into_iter().max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0))).map(|(v, _)| v).unwrap_or(from)
        };
        let a = far(start);
        let b = far(a);
        if a == b {
            return cuts;
        }
        let (_, parent) = dijkstra(&graph, &HashSet::from([a]), true);
        add_path(&mut cuts, &mut opened, path_edges(&parent, b));
    } else {
        // Unir los lazos: desde lo abierto hasta el lazo más cercano
        opened.extend(loops[0].iter().copied());
        let mut pending: Vec<HashSet<usize>> = loops[1..].iter().map(|l| l.iter().copied().collect()).collect();
        while !pending.is_empty() {
            let (dist, parent) = dijkstra(&graph, &opened, true);
            let best = pending
                .iter()
                .enumerate()
                .flat_map(|(i, l)| l.iter().filter_map(|v| dist.get(v).map(|&d| (d, *v, i))).collect::<Vec<_>>())
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let Some((_, end, i)) = best else { break };
            add_path(&mut cuts, &mut opened, path_edges(&parent, end));
            opened.extend(pending.swap_remove(i));
        }
    }

    // Defecto angular de cada vértice de la parte
    let mut angles: HashMap<usize, f64> = HashMap::new();
    for &f in comp {
        let face = &mesh.faces[f];
        let m = face.len();
        for k in 0..m {
            let p = mesh.points[face[k]];
            let (a, b) = (mesh.points[face[(k + m - 1) % m]] - p, mesh.points[face[(k + 1) % m]] - p);
            let angle = match (a.try_normalize(), b.try_normalize()) {
                (Some(a), Some(b)) => a.dot(&b).clamp(-1.0, 1.0).acos(),
                _ => 0.0,
            };
            *angles.entry(face[k]).or_default() += angle;
        }
    }
    let on_boundary: HashSet<usize> = boundary.iter().flat_map(|&(a, b)| [a, b]).collect();
    let curvature = |opened: &HashSet<usize>| -> f64 {
        angles
            .iter()
            .filter(|(v, _)| !on_boundary.contains(v) && !opened.contains(v))
            .map(|(_, sum)| 2.0 * std::f64::consts::PI - sum)
            .sum()
    };

    // Casquetes y medias: cierre desde lo abierto hasta el punto más lejano
    for _ in 0..MAX_ZIPS {
        if curvature(&opened) <= ZIP_CURVATURE {
            break;
        }
        let (dist, _) = dijkstra(&graph, &opened, false);
        let Some((&tip, _)) = dist.iter().max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0))) else { break };
        if opened.contains(&tip) {
            break;
        }
        let (_, parent) = dijkstra(&graph, &opened, true);
        add_path(&mut cuts, &mut opened, path_edges(&parent, tip));
    }
    cuts
}

/// Duplica los vértices de los cortes de `comp`: alrededor de cada vértice,
/// las caras de la parte que no se tocan por una arista sin cortar van con
/// su propia copia.
fn split_vertices<const N: usize>(
    mesh: &PolyMesh,
    comp: &[usize],
    cuts: &HashSet<(usize, usize)>,
    positions: &mut Vec<[f64; 3]>,
    faces: &mut [[usize; N]],
) {
    let on_cut: HashSet<usize> = cuts.iter().flat_map(|&(a, b)| [a, b]).collect();
    // Caras de la parte alrededor de cada vértice cortado
    let mut around: HashMap<usize, Vec<usize>> = HashMap::new();
    for &f in comp {
        for &v in &mesh.faces[f] {
            if on_cut.contains(&v) {
                around.entry(v).or_default().push(f);
            }
        }
    }
    let mut vertices: Vec<usize> = around.keys().copied().collect();
    vertices.sort_unstable();
    for v in vertices {
        let fan = &around[&v];
        let mut group: HashMap<usize, usize> = fan.iter().map(|&f| (f, f)).collect();
        // Unir caras que comparten una arista (v, w) sin cortar
        let mut by_edge: HashMap<usize, Vec<usize>> = HashMap::new();
        for &f in fan {
            let face = &mesh.faces[f];
            let k = face.iter().position(|&x| x == v).expect("la cara toca el vértice");
            for w in [face[(k + 1) % face.len()], face[(k + face.len() - 1) % face.len()]] {
                if !cuts.contains(&(v.min(w), v.max(w))) {
                    by_edge.entry(w).or_default().push(f);
                }
            }
        }
        for users in by_edge.values() {
            for pair in users.windows(2) {
                let (a, b) = (find(&mut group, pair[0]), find(&mut group, pair[1]));
                group.insert(a, b);
            }
        }
        let mut copy_of: HashMap<usize, usize> = HashMap::new();
        let mut roots: Vec<usize> = fan.iter().map(|&f| find(&mut group, f)).collect();
        roots.sort_unstable();
        roots.dedup();
        // La primera rueda conserva el vértice; las demás, una copia
        for (i, root) in roots.into_iter().enumerate() {
            let id = if i == 0 {
                v
            } else {
                positions.push(positions[v]);
                positions.len() - 1
            };
            copy_of.insert(root, id);
        }
        for &f in fan {
            let id = copy_of[&find(&mut group, f)];
            for slot in faces[f].iter_mut().filter(|x| **x == v) {
                *slot = id;
            }
        }
    }
}
