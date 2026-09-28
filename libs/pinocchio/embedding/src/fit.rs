//! Ajuste automático de una plantilla de esqueleto a una malla.
//!
//! 1. **Extremidades.** Desde la celda más profunda del volumen (el centro del
//!    tronco) se calculan caminos por el interior que siguen el eje medial
//!    (Dijkstra con costo `longitud / d²`). Las ramas se extraen de mayor a
//!    menor: la celda que más sobresale de lo ya cubierto (distancia a su
//!    unión menos el radio medial máximo en el camino) es la punta de la
//!    siguiente extremidad (pata, cabeza, cola, trompa…), hasta que ninguna
//!    sobresale. Un bulto del tronco no pasa más allá de lo más grueso que
//!    atraviesa y no cuenta.
//! 2. **Orientación y asignación.** La plantilla se prueba en 8 orientaciones
//!    (4 giros con Y arriba y 4 con Z arriba) encajada en la caja de la malla.
//!    Sus extremos (hojas, y la raíz si es punta de una cadena) se asignan a
//!    extremidades con el algoritmo húngaro (costo: distancia, altura para los
//!    extremos que tocan el suelo, diferencia de dirección vista desde el
//!    centro del cuerpo, preferencia por las
//!    extremidades prominentes, y cabeza gruesa / cola fina según el nombre
//!    del hueso); gana la orientación de menor costo, con un sesgo a favor de
//!    la estándar (Y arriba, mirando a +Z).
//! 3. **Colocación.** Una articulación de bifurcación (pelvis, pecho) va al
//!    promedio de los puntos donde sus extremidades entran al tronco (donde el
//!    radio llega al 75 % del grosor del tronco): el pecho entre los hombros y
//!    la base del cuello. Si una rama es mucho más grande que las demás (la
//!    columna que sale de la pelvis) se excluye, así la pelvis queda entre las
//!    caderas. Los extremos van a la punta de su extremidad (la más avanzada
//!    en la dirección del hueso: los dedos del pie, no el talón) y las cadenas
//!    intermedias se reparten por el camino con las proporciones de la
//!    plantilla.

use crate::chain::{closest_arc_length, point_along, polyline_length, proportion_quality, smooth, CellGraph, ShortestPaths};
use crate::embedding::EmbeddingError;
use pinocchio_math::{Real, Rect, Vector3};
use pinocchio_mesh::Mesh;
use pinocchio_skeleton::{map_positions, skeleton_bounds, BasicSkeleton, Bone, Skeleton};
use pinocchio_spatial::DistanceField;

/// Costo de dejar un extremo de la plantilla sin extremidad (en fracciones
/// del eje mayor de la malla).
const UNMATCHED_COST: Real = 0.6;

/// Peso de la diferencia de dirección (vista desde el centro del cuerpo)
/// entre un extremo de la plantilla y una extremidad: la cabeza apunta hacia
/// arriba y adelante, la trompa hacia abajo.
const DIRECTION_WEIGHT: Real = 0.25;

/// Sesgo contra girar la plantilla: los modelos suelen venir con Y arriba
/// mirando a +Z, y un cuerpo casi simétrico de frente y de espaldas no debe
/// invertir izquierda y derecha por un margen mínimo.
const TURN_PENALTY: Real = 0.15;
const Z_UP_PENALTY: Real = 0.15;

/// Peso del grosor: la cabeza prefiere extremidades gruesas y la cola finas
/// (por el nombre del hueso: `head*`, `tail*`).
const GIRTH_WEIGHT: Real = 0.3;

/// Preferencia por las extremidades prominentes: a igual distancia, una cola
/// gana sobre un bulto del tronco.
const PROMINENCE_WEIGHT: Real = 0.05;

/// Peso extra de la diferencia de altura (en fracción del alto de la malla)
/// para los extremos que en la plantilla tocan el suelo (patas, pies): los
/// colmillos no llegan al suelo aunque queden donde la plantilla espera las
/// patas delanteras.
const HEIGHT_WEIGHT: Real = 0.5;

/// Fracción de la caja de la malla que ocupa la plantilla encajada.
const FILL: Real = 0.9;

/// Opciones del ajuste.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitOptions {
    /// Celdas del campo de distancias en el eje mayor de la malla.
    pub resolution: usize,
    /// Probar las 8 orientaciones de la plantilla. Si es `false`, la plantilla
    /// ya está colocada sobre la malla y se usa tal cual (sin encajarla).
    pub search_orientation: bool,
    /// Largo mínimo de una extremidad, en fracción del eje mayor.
    pub min_branch: Real,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self { resolution: 80, search_orientation: true, min_branch: 0.06 }
    }
}

/// Extremidad de la malla: una rama del eje medial.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extremity {
    /// Punta (el punto medial más lejano de la rama).
    pub tip: Vector3,
    /// Donde la rama se une al resto.
    pub junction: Vector3,
    /// Cuánto sobresale la rama del cuerpo (distancia de la punta a la unión
    /// menos el radio medial máximo en el camino).
    pub length: Real,
    /// Radio medial mediano del tercio de la rama más cercano a la punta.
    pub girth: Real,
}

/// Resultado del ajuste.
#[derive(Debug, Clone)]
pub struct FitResult {
    /// La plantilla con sus articulaciones sobre la malla.
    pub skeleton: BasicSkeleton,
    /// Giro aplicado a la plantilla (filas de la matriz).
    pub rotation: [[Real; 3]; 3],
    /// Extremidades detectadas, de la más larga a la más corta.
    pub extremities: Vec<Extremity>,
    /// Extremidad asignada a cada hueso extremo (`None` en los demás o si
    /// quedó sin asignar).
    pub bone_extremity: Vec<Option<usize>>,
    /// Similitud de proporciones con la plantilla (1 = idénticas).
    pub quality: Real,
}

impl FitResult {
    /// Extremidades que ningún hueso usa (p. ej. la trompa si la plantilla no
    /// la tiene): candidatas a agregar una cadena.
    pub fn unused_extremities(&self) -> Vec<usize> {
        (0..self.extremities.len()).filter(|e| !self.bone_extremity.contains(&Some(*e))).collect()
    }
}

struct Branch {
    tip: usize,
    junction: usize,
    length: Real,
    girth: Real,
}

/// Ramas del eje medial, de la más larga a la más corta.
fn extract_branches(graph: &CellGraph, tree: &ShortestPaths, core: usize, min_length: Real) -> Vec<Branch> {
    let n = graph.len();
    let mut covered = vec![false; n];
    covered[core] = true;
    let mut branches = Vec::new();
    let mut anchor = vec![usize::MAX; n];
    let mut thickest: Vec<Real> = vec![0.0; n];
    while branches.len() < 32 {
        // Ancestro cubierto más cercano de cada celda (el orden de Dijkstra
        // cierra al padre antes que al hijo) y el radio máximo en el camino.
        // Lo que sobresale: distancia a la unión menos ese radio (un bulto
        // del tronco no pasa más allá de lo más grueso que atraviesa)
        let mut best = (0.0, usize::MAX);
        for &c in &tree.order {
            (anchor[c], thickest[c]) = if covered[c] {
                (c, graph.value(c))
            } else {
                let p = tree.prev[c];
                (anchor[p], thickest[p].max(graph.value(c)))
            };
            let length = graph.center(c).distance(&graph.center(anchor[c])) - thickest[c];
            if length > best.0 {
                best = (length, c);
            }
        }
        if best.0 < min_length || best.1 == usize::MAX {
            break;
        }
        let tip = best.1;
        let junction = anchor[tip];
        // Grosor: solo en el tercio de la rama más cercano a la punta, no en
        // el tramo que cruza el tronco
        let tip_center = graph.center(tip);
        let mut c = tip;
        let mut radii = Vec::new();
        let mut path = Vec::new();
        while !covered[c] {
            path.push(c);
            if graph.center(c).distance(&tip_center) <= best.0 / 3.0 || radii.is_empty() {
                radii.push(graph.value(c));
            }
            c = tree.prev[c];
        }
        // Cubrir el tubo alrededor del camino (el radio medial en cada punto):
        // la celda vecina de la misma pata no es otra extremidad
        let cell = graph.cell_size();
        for &p in &path {
            cover_ball(graph, &mut covered, p, graph.value(p).max(1.5 * cell));
        }
        radii.sort_by(Real::total_cmp);
        let girth = radii.get(radii.len() / 2).copied().unwrap_or(0.0);
        branches.push(Branch { tip, junction, length: best.0, girth });
    }
    branches
}

/// Marca cubiertas las celdas interiores a menos de `radius` de `center`
/// (conectadas a ella).
fn cover_ball(graph: &CellGraph, covered: &mut [bool], center: usize, radius: Real) {
    let origin = graph.center(center);
    covered[center] = true;
    let mut stack = vec![center];
    let mut seen = std::collections::HashSet::from([center]);
    while let Some(c) = stack.pop() {
        for n in graph.neighbors(c) {
            if graph.is_interior(n) && graph.center(n).distance(&origin) <= radius && seen.insert(n) {
                covered[n] = true;
                stack.push(n);
            }
        }
    }
}

/// Asignación de costo mínimo filas → columnas (algoritmo húngaro, filas ≤
/// columnas). Devuelve la columna de cada fila.
fn hungarian(cost: &[Vec<Real>]) -> Vec<usize> {
    let n = cost.len();
    let m = cost.first().map_or(0, Vec::len);
    if n == 0 {
        return Vec::new();
    }
    // Potenciales (índices desde 1, como en la formulación clásica)
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; m + 1];
    let mut p = vec![0usize; m + 1];
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![Real::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = Real::INFINITY;
            let mut j1 = 0;
            for j in 1..=m {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut result = vec![0; n];
    for j in 1..=m {
        if p[j] > 0 {
            result[p[j] - 1] = j - 1;
        }
    }
    result
}

fn mat_mul(a: [[Real; 3]; 3], b: [[Real; 3]; 3]) -> [[Real; 3]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

fn apply(m: &[[Real; 3]; 3], p: Vector3) -> Vector3 {
    let v = [p.x(), p.y(), p.z()];
    let r: [Real; 3] = std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]);
    Vector3::new(r[0], r[1], r[2])
}

/// Las 8 orientaciones: 4 giros alrededor de Y, y las mismas con Y → Z.
fn orientations() -> Vec<[[Real; 3]; 3]> {
    let yaw = |k: usize| {
        let a = k as Real * std::f64::consts::FRAC_PI_2 as Real;
        let (s, c) = (a.sin().round(), a.cos().round());
        [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
    };
    let y_to_z = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
    let mut out: Vec<[[Real; 3]; 3]> = (0..4).map(yaw).collect();
    out.extend((0..4).map(|k| mat_mul(y_to_z, yaw(k))));
    out
}

/// Encaja la plantilla girada en la caja de la malla: escala por la altura
/// (eje `up`) si la plantilla es alta, si no por su eje mayor.
fn fit_box(skeleton: &BasicSkeleton, target: &Rect, up: usize) -> BasicSkeleton {
    let Some(bounds) = skeleton_bounds(skeleton) else { return skeleton.clone() };
    let size = bounds.size();
    let target_size = target.size();
    let axis = |v: Vector3, i: usize| [v.x(), v.y(), v.z()][i];
    let max = size.max_component();
    let scale = if max <= 1e-12 {
        1.0
    } else if axis(size, up) >= 0.5 * max {
        FILL * axis(target_size, up) / axis(size, up)
    } else {
        FILL * target_size.max_component() / max
    };
    let (from, to) = (bounds.center(), target.center());
    map_positions(skeleton, |p| (p - from) * scale + to)
}

/// Huesos extremo: hojas, y la raíz si tiene un solo hijo (punta de cadena).
fn endpoints<S: Skeleton>(skeleton: &S) -> Vec<usize> {
    (0..skeleton.num_bones())
        .filter(|&b| {
            let children = skeleton.get_children(b).len();
            children == 0 || (skeleton.get_parent(b).is_none() && children == 1)
        })
        .collect()
}

/// Ancestro común más profundo en el árbol de caminos.
fn lowest_common_ancestor(tree: &ShortestPaths, cells: &[usize]) -> Option<usize> {
    let ancestors = |mut c: usize| {
        let mut chain = vec![c];
        while tree.prev[c] != usize::MAX {
            c = tree.prev[c];
            chain.push(c);
        }
        chain
    };
    let (&first, rest) = cells.split_first()?;
    let mut common = ancestors(first);
    for &c in rest {
        let set: std::collections::HashSet<usize> = ancestors(c).into_iter().collect();
        let keep = common.iter().position(|a| set.contains(a))?;
        common.drain(..keep);
    }
    common.first().copied()
}

/// Ajusta `template` a `mesh`.
pub fn fit_skeleton<S: Skeleton>(mesh: &Mesh, template: &S, options: &FitOptions) -> Result<FitResult, EmbeddingError> {
    if mesh.num_vertices() == 0 {
        return Err(EmbeddingError::EmptyMesh);
    }
    if template.num_bones() == 0 {
        return Err(EmbeddingError::EmptySkeleton);
    }
    let bbox = mesh.bounding_box();
    let longest = bbox.longest_axis_length().max(1e-12);
    let padding = longest * 0.1;
    let size = bbox.size();
    let res = [size.x(), size.y(), size.z()]
        .map(|s| (((s + 2.0 * padding) / (longest + 2.0 * padding)) * options.resolution as Real).round().max(8.0) as usize);
    let field = DistanceField::from_mesh_signed(mesh, res, padding);
    let graph = CellGraph::new(&field);

    let core = (0..graph.len())
        .filter(|&c| graph.is_interior(c))
        .max_by(|&a, &b| graph.value(a).total_cmp(&graph.value(b)))
        .ok_or(EmbeddingError::NoValidEmbedding)?;
    let tree = graph.shortest_paths(core);
    let branches = extract_branches(&graph, &tree, core, options.min_branch * longest);
    let extremities: Vec<Extremity> = branches
        .iter()
        .map(|b| Extremity {
            tip: graph.center(b.tip),
            junction: graph.center(b.junction),
            length: b.length,
            girth: b.girth,
        })
        .collect();

    // Orientación y asignación de extremos a extremidades
    let base = BasicSkeleton::from_bones(template.bones().to_vec());
    let ends = endpoints(template);
    let candidates: Vec<[[Real; 3]; 3]> = if options.search_orientation {
        orientations()
    } else {
        vec![[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]]
    };
    let most_prominent = extremities.iter().map(|e| e.length).fold(1e-12, Real::max);
    let thickest = extremities.iter().map(|e| e.girth).fold(1e-12, Real::max);
    let girth_cost = |bone: usize, e: &Extremity| {
        let name = template.bones()[bone].name.to_lowercase();
        let g = e.girth / thickest;
        if name.starts_with("head") {
            GIRTH_WEIGHT * (1.0 - g)
        } else if name.starts_with("tail") {
            GIRTH_WEIGHT * g
        } else {
            0.0
        }
    };
    let center = graph.center(core);
    let direction_cost = |a: Vector3, b: Vector3| match ((a - center).try_normalize(), (b - center).try_normalize()) {
        (Some(u), Some(v)) => DIRECTION_WEIGHT * (1.0 - u.dot(&v)),
        _ => 0.0,
    };
    // (costo, giro, plantilla encajada, extremidad de cada extremo)
    type Candidate = (Real, [[Real; 3]; 3], BasicSkeleton, Vec<Option<usize>>);
    let mut best: Option<Candidate> = None;
    for (k, rotation) in candidates.into_iter().enumerate() {
        // Orden de `orientations`: 0 = identidad, 1-3 giros, 4-7 con Z arriba
        let bias = match k {
            0 => 0.0,
            1..=3 => TURN_PENALTY,
            _ => Z_UP_PENALTY,
        };
        let up = if rotation[2][1].abs() > 0.5 { 2 } else { 1 };
        let height = [size.x(), size.y(), size.z()][up].max(1e-12);
        let along_up = |v: Vector3| [v.x(), v.y(), v.z()][up];
        let placed = if options.search_orientation {
            fit_box(&map_positions(&base, |p| apply(&rotation, p)), &bbox, up)
        } else {
            base.clone()
        };
        // Extremos que tocan el suelo en la plantilla
        let ground = placed.bones().iter().map(|b| along_up(b.position)).fold(Real::INFINITY, Real::min);
        let top = placed.bones().iter().map(|b| along_up(b.position)).fold(Real::NEG_INFINITY, Real::max);
        let grounded = |b: usize| along_up(placed.bones()[b].position) - ground <= 0.1 * (top - ground);
        // Columnas: extremidades + una columna "sin asignar" por extremo
        let cost: Vec<Vec<Real>> = ends
            .iter()
            .map(|&b| {
                let p = placed.bones()[b].position;
                let height_weight = if grounded(b) { HEIGHT_WEIGHT } else { 0.0 };
                extremities
                    .iter()
                    .map(|e| {
                        p.distance(&e.tip) / longest
                            + height_weight * (along_up(p) - along_up(e.tip)).abs() / height
                            + direction_cost(p, e.tip)
                            + girth_cost(b, e)
                            + PROMINENCE_WEIGHT * (1.0 - e.length / most_prominent)
                    })
                    .chain(std::iter::repeat_n(UNMATCHED_COST, ends.len()))
                    .collect()
            })
            .collect();
        let assignment = hungarian(&cost);
        let total: Real = bias + assignment.iter().enumerate().map(|(i, &j)| cost[i][j]).sum::<Real>();
        let matched: Vec<Option<usize>> = assignment.iter().map(|&j| (j < extremities.len()).then_some(j)).collect();
        if best.as_ref().is_none_or(|b| total < b.0 - 1e-9) {
            best = Some((total, rotation, placed, matched));
        }
    }
    let (_, rotation, placed, matched) = best.expect("al menos una orientación");
    let mut bone_extremity = vec![None; template.num_bones()];
    for (&b, &m) in ends.iter().zip(&matched) {
        bone_extremity[b] = m;
    }

    let positions = place_joints(&graph, &tree, &placed, &branches, &bone_extremity);
    let skeleton = BasicSkeleton::from_bones(
        placed.bones().iter().zip(&positions).map(|(bone, &position)| Bone { position, ..bone.clone() }).collect(),
    );
    let quality = proportion_quality(template, &positions);
    Ok(FitResult { skeleton, rotation, extremities, bone_extremity, quality })
}

/// Coloca cada articulación de `placed` (la plantilla encajada) sobre el eje
/// medial siguiendo las extremidades asignadas.
fn place_joints(
    graph: &CellGraph,
    tree: &ShortestPaths,
    placed: &BasicSkeleton,
    branches: &[Branch],
    bone_extremity: &[Option<usize>],
) -> Vec<Vector3> {
    let n = placed.num_bones();
    let template: Vec<Vector3> = placed.bones().iter().map(|b| b.position).collect();

    // Extremidades bajo cada hueso
    let mut tips_below: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, extremity) in bone_extremity.iter().enumerate() {
        if let Some(e) = *extremity {
            let mut cur = Some(b);
            while let Some(c) = cur {
                tips_below[c].push(e);
                cur = placed.get_parent(c);
            }
        }
    }
    let fallback = |b: usize| graph.nearest_interior(&template[b]);
    let body_radius = tree.order.first().map_or(0.0, |&core| graph.value(core));

    // Celda de una articulación de bifurcación (o de la raíz)
    let junction_cell = |b: usize| -> Option<usize> {
        if let Some(e) = bone_extremity[b] {
            return Some(branches[e].tip);
        }
        let mut groups: Vec<Vec<usize>> = placed
            .get_children(b)
            .iter()
            .map(|&c| tips_below[c].clone())
            .filter(|g| !g.is_empty())
            .collect();
        groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
        // Una rama que domina (la columna desde la pelvis) no cuenta
        if groups.len() >= 3 && groups[0].len() > groups[1].len() {
            groups.remove(0);
        }
        let limbs: Vec<usize> = groups.into_iter().flatten().collect();
        if limbs.len() < 2 {
            return fallback(b);
        }
        // Promedio de los puntos donde cada extremidad sale del cuerpo
        let entries: Vec<Vector3> =
            limbs.iter().map(|&e| graph.center(entry_cell(graph, tree, &branches[e], body_radius))).collect();
        let mean = entries.iter().fold(Vector3::zero(), |acc, &p| acc + p) * (1.0 / entries.len() as Real);
        let tips: Vec<usize> = limbs.iter().map(|&e| branches[e].tip).collect();
        graph.nearest_interior(&mean).or_else(|| lowest_common_ancestor(tree, &tips))
    };

    let mut positions: Vec<Option<Vector3>> = vec![None; n];
    let mut cells: Vec<Option<usize>> = vec![None; n];
    let mut pending = Vec::new();
    for root in (0..n).filter(|&b| placed.get_parent(b).is_none()) {
        if let Some(cell) = junction_cell(root) {
            positions[root] = Some(graph.center(cell));
            cells[root] = Some(cell);
            pending.push(root);
        }
    }

    while let Some(base) = pending.pop() {
        let base_cell = cells[base].expect("base colocada");
        let base_pos = positions[base].expect("base colocada");
        let paths = graph.shortest_paths(base_cell);

        for first in placed.get_children(base) {
            let mut chain = vec![first];
            loop {
                let children = placed.get_children(*chain.last().unwrap());
                if children.len() == 1 {
                    chain.push(children[0]);
                } else {
                    break;
                }
            }
            let end = *chain.last().unwrap();
            let is_leaf = placed.get_children(end).is_empty();
            let end_cell = if is_leaf {
                bone_extremity[end].map(|e| branches[e].tip).or_else(|| fallback(end))
            } else {
                junction_cell(end)
            }
            .filter(|&c| paths.reached[c])
            .unwrap_or(base_cell);

            let mut path_cells = vec![end_cell];
            let mut cur = end_cell;
            while cur != base_cell && paths.prev[cur] != usize::MAX {
                cur = paths.prev[cur];
                path_cells.push(cur);
            }
            path_cells.reverse();
            let mut polyline: Vec<Vector3> = path_cells.iter().map(|&c| graph.center(c)).collect();
            polyline[0] = base_pos;
            if polyline.len() == 1 {
                polyline.push(base_pos);
            }
            smooth(&mut polyline, 2);
            let total = polyline_length(&polyline);

            // Proporciones de la plantilla; el primer tramo (p. ej. pelvis →
            // cadera) cruza el tronco y no guarda proporción con el resto: se
            // ancla en la proyección de la plantilla, pero no antes de donde
            // la extremidad entra al cuerpo ni de su parte proporcional (si
            // no, en un tronco gordo la cadera cae sobre la pelvis y el muslo
            // cruza toda la grupa)
            let lengths: Vec<Real> = chain.iter().map(|&b| template[b].distance(&template[placed.get_parent(b).unwrap()])).collect();
            let (start, skip) = if chain.len() >= 2 {
                let entry = bone_extremity[end]
                    .filter(|_| is_leaf)
                    .map(|e| closest_arc_length(&polyline, &graph.center(entry_cell(graph, tree, &branches[e], body_radius))))
                    .unwrap_or(0.0);
                // Y al menos la mitad de su parte según la plantilla: un
                // conector de largo cero (cadera sobre la pelvis) no articula
                let share = 0.5 * lengths[0] / lengths.iter().sum::<Real>().max(1e-12) * total;
                let anchor = closest_arc_length(&polyline, &template[chain[0]]).max(entry).max(share).min(0.5 * total);
                positions[chain[0]] = Some(point_along(&polyline, anchor));
                (anchor, 1)
            } else {
                (0.0, 0)
            };
            let rest: Real = lengths[skip..].iter().sum();
            let mut acc = 0.0;
            for (&bone, &len) in chain.iter().zip(&lengths).skip(skip) {
                acc += len;
                let fraction = if rest > 0.0 { acc / rest } else { 1.0 };
                positions[bone] = Some(point_along(&polyline, start + fraction * (total - start)));
            }
            positions[end] = Some(*polyline.last().unwrap());
            cells[end] = Some(end_cell);
            if is_leaf && let Some(e) = bone_extremity[end] {
                let parent = placed.get_parent(end).expect("hoja con padre");
                let direction = template[end] - template[parent];
                let radius = 3.0 * branches[e].girth.max(graph.value(end_cell));
                positions[end] = Some(graph.center(farthest_along(graph, end_cell, direction, radius)));
            }
            if !is_leaf {
                pending.push(end);
            }
        }
    }
    positions.iter().zip(&template).map(|(p, t)| p.unwrap_or(*t)).collect()
}

/// Donde una extremidad entra al tronco: caminando desde su punta hacia el
/// centro, la primera celda con radio ≥ 75 % del grosor del tronco
/// (`body_radius`). Sirve para ubicar las bifurcaciones (pelvis, pecho).
fn entry_cell(graph: &CellGraph, tree: &ShortestPaths, branch: &Branch, body_radius: Real) -> usize {
    let mut c = branch.tip;
    while graph.value(c) < 0.75 * body_radius && tree.prev[c] != usize::MAX {
        c = tree.prev[c];
    }
    c
}

/// Celda interior cerca de `start` (a menos de `radius`) que más avanza en
/// `direction`: la punta del pie hacia adelante y no el talón, aunque estén a
/// la misma distancia de la cadera.
fn farthest_along(graph: &CellGraph, start: usize, direction: Vector3, radius: Real) -> usize {
    let Some(dir) = direction.try_normalize() else { return start };
    let origin = graph.center(start);
    let mut best = (0.0, start);
    let mut seen = std::collections::HashSet::from([start]);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(c) = queue.pop_front() {
        let offset = graph.center(c) - origin;
        let advance = offset.dot(&dir);
        if advance > best.0 {
            best = (advance, c);
        }
        for n in graph.neighbors(c) {
            if graph.is_interior(n) && graph.center(n).distance(&origin) <= radius && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hungarian_finds_the_optimal_assignment() {
        // Codicioso tomaría (0,0)=1 y dejaría (1,1)=10; lo óptimo cruza
        let cost = vec![vec![1.0, 2.0, 9.0], vec![2.0, 10.0, 9.0]];
        assert_eq!(hungarian(&cost), vec![1, 0]);
    }

    #[test]
    fn orientations_are_rotations() {
        for m in orientations() {
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            assert!((det - 1.0).abs() < 1e-9);
        }
    }
}
