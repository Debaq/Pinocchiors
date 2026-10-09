//! Triángulos a quads: junta pares de triángulos vecinos sin mover vértices.
//!
//! Cada par de triángulos que comparte una arista es un quad candidato; se
//! puntúa por lo plano que queda (ángulo entre las dos caras) y por lo
//! cuadrado (cuánto se alejan sus esquinas de 90°), y se juntan los mejores
//! primero mientras los dos triángulos estén libres. Los triángulos que
//! quedan sin pareja siguen siendo triángulos.
//!
//! Dos triángulos son vecinos solo si comparten los vértices de la arista con
//! todos sus atributos: por una costura de UV, una arista viva con normales
//! partidas o un límite entre grupos (materiales) no se juntan.
//!
//! [`all_quads`] subdivide un paso para que todo sean quads: cada triángulo
//! da tres y cada quad cuatro. Los vértices nuevos se describen como mezcla
//! de los de la entrada, así quien llama interpola cualquier atributo.

use std::collections::HashMap;

/// Un triángulo o un quad, con índices a los vértices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Tri([u32; 3]),
    Quad([u32; 4]),
}

impl Face {
    pub fn vertices(&self) -> &[u32] {
        match self {
            Face::Tri(v) => v,
            Face::Quad(v) => v,
        }
    }

    /// Triángulos de la cara (el quad, por la diagonal 0–2, que es la arista
    /// que compartían los dos triángulos de [`tris_to_quads`]).
    pub fn triangles(&self) -> impl Iterator<Item = [u32; 3]> {
        let (a, b) = match *self {
            Face::Tri(t) => (Some(t), None),
            Face::Quad([a, b, c, d]) => (Some([a, b, c]), Some([a, c, d])),
        };
        a.into_iter().chain(b)
    }
}

/// La malla a convertir.
#[derive(Debug, Clone, Copy)]
pub struct QuadsInput<'a> {
    pub positions: &'a [[f32; 3]],
    /// Triángulos (3 índices cada uno).
    pub indices: &'a [u32],
    /// Atributos por vértice (`stride` valores cada uno; vacío: ninguno).
    /// Vértices de igual posición y atributos cuentan como el mismo.
    pub attributes: &'a [f32],
    pub stride: usize,
    /// Grupo de cada vértice (material, primitiva; vacío: todos el mismo).
    pub groups: &'a [u32],
}

/// Qué pares se aceptan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadsOptions {
    /// Ángulo máximo entre las dos caras, en grados (más: arista viva).
    pub max_face_angle: f64,
    /// Desvío máximo de las esquinas del quad respecto de 90°, en grados.
    pub max_shape_angle: f64,
}

impl Default for QuadsOptions {
    fn default() -> Self {
        Self { max_face_angle: 40.0, max_shape_angle: 40.0 }
    }
}

/// Junta los pares de triángulos que cumplen las opciones. Las caras salen
/// con la orientación de la entrada y los índices de la entrada.
pub fn tris_to_quads(input: &QuadsInput, options: &QuadsOptions) -> Vec<Face> {
    let triangles: Vec<[u32; 3]> = input
        .indices
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]])
        .filter(|t| t.iter().all(|&i| (i as usize) < input.positions.len()))
        .collect();
    let welded = weld(input);
    let w = |i: u32| welded[i as usize];

    // Triángulos de cada arista (soldada y orientada como en el triángulo)
    let mut by_edge: HashMap<(u32, u32), Vec<(u32, usize)>> = HashMap::with_capacity(triangles.len() * 2);
    for (f, t) in triangles.iter().enumerate() {
        if w(t[0]) == w(t[1]) || w(t[1]) == w(t[2]) || w(t[0]) == w(t[2]) {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (w(t[k]), w(t[(k + 1) % 3]));
            by_edge.entry((a.min(b), a.max(b))).or_default().push((f as u32, k));
        }
    }

    let cos_face = options.max_face_angle.to_radians().cos();
    let max_shape = options.max_shape_angle;
    let p = |i: u32| input.positions[i as usize].map(f64::from);
    let mut candidates: Vec<(f64, u32, u32, [u32; 4])> = by_edge
        .values()
        .filter(|faces| faces.len() == 2)
        .filter_map(|faces| {
            let ((f, k), (g, m)) = (faces[0], faces[1]);
            let (s, t) = (triangles[f as usize], triangles[g as usize]);
            // Orientación coherente: la arista va en sentidos opuestos
            if w(s[k]) != w(t[(m + 1) % 3]) {
                return None;
            }
            // s = (u, v, x) con la arista u → v; t tiene v → u y su opuesto y
            let (u, v, x) = (s[k], s[(k + 1) % 3], s[(k + 2) % 3]);
            let y = t[(m + 2) % 3];
            // Empieza en u: la diagonal 0–2 es la arista compartida, así
            // triangularlo da los mismos dos triángulos (la forma no cambia)
            let quad = [u, y, v, x];
            let (ns, nt) = (normal(p(u), p(v), p(x)), normal(p(t[m]), p(t[(m + 1) % 3]), p(y)));
            let bend = dot(unit(ns), unit(nt));
            if bend < cos_face {
                return None;
            }
            let shape = shape_error(quad.map(p))?;
            if shape > max_shape {
                return None;
            }
            // Primero los más cuadrados; a igualdad, los más planos
            Some((shape + (1.0 - bend) * 10.0, f, g, quad))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));

    let mut used = vec![false; triangles.len()];
    let mut faces = Vec::with_capacity(triangles.len());
    let mut quad_of: Vec<Option<usize>> = vec![None; triangles.len()];
    for (_, f, g, quad) in candidates {
        if used[f as usize] || used[g as usize] {
            continue;
        }
        used[f as usize] = true;
        used[g as usize] = true;
        quad_of[f.min(g) as usize] = Some(faces.len());
        faces.push(Face::Quad(quad));
    }
    // En el orden de la entrada: cada quad donde estaba su primer triángulo
    let mut ordered = Vec::with_capacity(faces.len() + triangles.len());
    for (f, t) in triangles.iter().enumerate() {
        if let Some(q) = quad_of[f] {
            ordered.push(faces[q]);
        } else if !used[f] {
            ordered.push(Face::Tri(*t));
        }
    }
    ordered
}

/// Mayor desvío de las esquinas respecto de 90°, en grados; `None` si el
/// quad no es convexo
fn shape_error(q: [[f64; 3]; 4]) -> Option<f64> {
    let n = unit(add(normal(q[0], q[1], q[2]), normal(q[0], q[2], q[3])));
    let mut worst: f64 = 0.0;
    for k in 0..4 {
        let (prev, here, next) = (q[(k + 3) % 4], q[k], q[(k + 1) % 4]);
        let (a, b) = (sub(prev, here), sub(next, here));
        // Convexo: todas las esquinas giran hacia el mismo lado
        if dot(cross(b, a), n) <= 0.0 {
            return None;
        }
        let angle = (dot(a, b) / (len(a) * len(b))).clamp(-1.0, 1.0).acos().to_degrees();
        worst = worst.max((angle - 90.0).abs());
    }
    Some(worst)
}

/// Vértice soldado de cada uno: iguales en grupo, posición y atributos
fn weld(input: &QuadsInput) -> Vec<u32> {
    let stride = input.stride;
    let mut map: HashMap<Vec<u32>, u32> = HashMap::with_capacity(input.positions.len());
    (0..input.positions.len())
        .map(|i| {
            let group = input.groups.get(i).copied().unwrap_or(0);
            let attributes = input.attributes.get(i * stride..(i + 1) * stride).unwrap_or(&[]);
            let key: Vec<u32> = std::iter::once(group)
                .chain(input.positions[i].iter().map(|f| canonical(*f)))
                .chain(attributes.iter().map(|f| canonical(*f)))
                .collect();
            let next = map.len() as u32;
            *map.entry(key).or_insert(next)
        })
        .collect()
}

fn canonical(f: f32) -> u32 {
    if f == 0.0 { 0 } else { f.to_bits() }
}

/// Resultado de [`all_quads`]: los quads y cómo armar cada vértice nuevo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subdivided {
    /// Quads con índices a los vértices: primero los de la entrada (mismo
    /// índice) y después los nuevos (`input_vertices + k` es `new_vertices[k]`).
    pub quads: Vec<[u32; 4]>,
    /// Cada vértice nuevo como mezcla de vértices de la entrada (pesos que
    /// suman 1): puntos medios de las aristas y centros de las caras.
    pub new_vertices: Vec<Vec<(u32, f32)>>,
}

/// Subdivide un paso para que todo sean quads (lineal: la forma no cambia).
/// `input_vertices` es la cantidad de vértices de la entrada. Las caras con
/// un vértice repetido (sin área) se descartan.
pub fn all_quads(faces: &[Face], input_vertices: usize) -> Subdivided {
    let mut out = Subdivided::default();
    let mut midpoint: HashMap<(u32, u32), u32> = HashMap::new();
    let new_vertex = |out: &mut Subdivided, mix: Vec<(u32, f32)>| {
        out.new_vertices.push(mix);
        (input_vertices + out.new_vertices.len() - 1) as u32
    };
    for face in faces {
        let v = face.vertices();
        let n = v.len();
        // Una cara con un vértice repetido no tiene área: daría quads degenerados
        if (0..n).any(|k| v[k] == v[(k + 1) % n]) || (n == 4 && (v[0] == v[2] || v[1] == v[3])) {
            continue;
        }
        let mids: Vec<u32> = (0..n)
            .map(|k| {
                let (a, b) = (v[k], v[(k + 1) % n]);
                if let Some(&m) = midpoint.get(&(a.min(b), a.max(b))) {
                    return m;
                }
                let m = new_vertex(&mut out, vec![(a, 0.5), (b, 0.5)]);
                midpoint.insert((a.min(b), a.max(b)), m);
                m
            })
            .collect();
        let center = new_vertex(&mut out, v.iter().map(|&i| (i, 1.0 / n as f32)).collect());
        for k in 0..n {
            out.quads.push([v[k], mids[k], center, mids[(k + n - 1) % n]]);
        }
    }
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = len(a);
    if l > 0.0 { a.map(|c| c / l) } else { a }
}

fn normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    cross(sub(b, a), sub(c, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla de n×n cuadrados, cada uno en dos triángulos (diagonal alternada si `alternate`)
    fn grid(n: u32, alternate: bool) -> (Vec<[f32; 3]>, Vec<u32>) {
        let mut p = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                p.push([x as f32, y as f32, 0.0]);
            }
        }
        let mut t = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                let (b, c, d) = (a + 1, a + n + 2, a + n + 1);
                if alternate && (x + y) % 2 == 1 {
                    t.extend([a, b, d, b, c, d]);
                } else {
                    t.extend([a, b, c, a, c, d]);
                }
            }
        }
        (p, t)
    }

    fn input<'a>(p: &'a [[f32; 3]], t: &'a [u32]) -> QuadsInput<'a> {
        QuadsInput { positions: p, indices: t, attributes: &[], stride: 0, groups: &[] }
    }

    fn count(faces: &[Face]) -> (usize, usize) {
        let quads = faces.iter().filter(|f| matches!(f, Face::Quad(_))).count();
        (quads, faces.len() - quads)
    }

    #[test]
    fn triangulated_grid_becomes_all_quads() {
        for alternate in [false, true] {
            let (p, t) = grid(8, alternate);
            let faces = tris_to_quads(&input(&p, &t), &QuadsOptions::default());
            assert_eq!(count(&faces), (64, 0), "diagonal alternada: {alternate}");
            // Cada quad es una celda de la grilla: lados de 1
            for f in &faces {
                let v = f.vertices();
                for k in 0..4 {
                    let (a, b) = (p[v[k] as usize], p[v[(k + 1) % 4] as usize]);
                    assert!(((a[0] - b[0]).abs() + (a[1] - b[1]).abs() - 1.0).abs() < 1e-6, "{f:?}");
                }
            }
        }
    }

    #[test]
    fn quads_triangulate_back_into_the_input_triangles() {
        // Con la diagonal alternada: cada quad vuelve a dar sus dos triángulos
        let (p, t) = grid(6, true);
        let rotations = |x: [u32; 3]| [x, [x[1], x[2], x[0]], [x[2], x[0], x[1]]];
        let original: std::collections::HashSet<[u32; 3]> = t.chunks(3).flat_map(|c| rotations([c[0], c[1], c[2]])).collect();
        for f in tris_to_quads(&input(&p, &t), &QuadsOptions::default()) {
            assert!(f.triangles().all(|tri| original.contains(&tri)), "{f:?}");
        }
    }

    #[test]
    fn quads_keep_orientation() {
        let (p, t) = grid(3, false);
        let faces = tris_to_quads(&input(&p, &t), &QuadsOptions::default());
        // Normales hacia +z como la entrada
        for f in &faces {
            let v: Vec<[f64; 3]> = f.vertices().iter().map(|&i| p[i as usize].map(f64::from)).collect();
            assert!(normal(v[0], v[1], v[2])[2] > 0.0);
        }
    }

    #[test]
    fn sharp_folds_and_seams_are_not_crossed() {
        // Dos triángulos plegados 90°: no se juntan
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let t = [0, 1, 2, 1, 0, 3];
        assert_eq!(count(&tris_to_quads(&input(&p, &t), &QuadsOptions::default())), (0, 2));
        // Un cuadrado con una costura de UV en la diagonal: vértices duplicados con UV distintas
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let uv = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.5, 0.5, 0.6, 0.6];
        let t = [0, 1, 2, 4, 5, 3];
        let seam = QuadsInput { positions: &p, indices: &t, attributes: &uv, stride: 2, groups: &[] };
        assert_eq!(count(&tris_to_quads(&seam, &QuadsOptions::default())), (0, 2));
        // Sin UV distintas (STL sin índices) sí se juntan
        let same = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0];
        let welded = QuadsInput { attributes: &same, ..seam };
        assert_eq!(count(&tris_to_quads(&welded, &QuadsOptions::default())), (1, 0));
    }

    #[test]
    fn skewed_pairs_stay_triangles() {
        // Un rombo muy aplastado: esquinas de 20° y 160°
        let a = 10f32.to_radians();
        let p = [[0.0, 0.0, 0.0], [a.cos(), a.sin(), 0.0], [2.0 * a.cos(), 0.0, 0.0], [a.cos(), -a.sin(), 0.0]];
        let t = [0, 3, 2, 0, 2, 1];
        assert_eq!(count(&tris_to_quads(&input(&p, &t), &QuadsOptions::default())), (0, 2));
        let loose = QuadsOptions { max_shape_angle: 80.0, ..Default::default() };
        assert_eq!(count(&tris_to_quads(&input(&p, &t), &loose)), (1, 0));
    }

    #[test]
    fn all_quads_drops_degenerate_faces() {
        let s = all_quads(&[Face::Tri([0, 0, 1]), Face::Tri([0, 1, 2])], 3);
        assert_eq!(s.quads.len(), 3);
        assert!(s.quads.iter().all(|q| (0..4).all(|i| (i + 1..4).all(|j| q[i] != q[j]))));
    }

    #[test]
    fn all_quads_subdivides_tris_and_quads() {
        let faces = [Face::Quad([0, 1, 2, 3]), Face::Tri([1, 4, 2])];
        let s = all_quads(&faces, 5);
        assert_eq!(s.quads.len(), 4 + 3);
        // Aristas: 4 + 3 − 1 compartida = 6 puntos medios, más 2 centros
        assert_eq!(s.new_vertices.len(), 8);
        for mix in &s.new_vertices {
            assert!((mix.iter().map(|(_, w)| w).sum::<f32>() - 1.0).abs() < 1e-6);
        }
        assert!(s.quads.iter().flatten().all(|&i| (i as usize) < 5 + 8));
    }
}
